"""Semantic positives, planted faults and comparison-stub regressions; no CLI."""
from copy import deepcopy
import unittest
from unittest.mock import patch

import loop_composable_reference as C


def scalar(nodes, root, lo=0, hi=3, boolean=False):
    return {"inputs": [{"kind": "Int", "min": str(lo), "max": str(hi)}],
            "outputs": [{"kind": "Bool"}] if boolean else
                       [{"kind": "Int", "min": str(-(1 << 127)), "max": str((1 << 127) - 1)}],
            "nodes": nodes, "roots": [root]}


X = scalar([["Input", 0]], 0)
LONG = scalar([["Input", 0], ["Add", 0, 0], ["Sub", 1, 0]], 2)
WRONG = scalar([["Input", 0], ["Int", "1"], ["Add", 0, 1]], 2)


def bank():
    return {"terms": [X, LONG], "classes": [[0, 1]], "rejected": [],
            "rules": [{"lhs": LONG, "rhs": X}]}


class Reference(unittest.TestCase):
    def test_fixed_grammar_counts_without_adjacent_repetition(self):
        for depth, count in [(1, 5), (2, 17), (3, 53)]:
            self.assertEqual(len(C.grammar(depth)), count)
            self.assertTrue(all(a != b for word in C.grammar(depth) for a, b in zip(word, word[1:])))
        for invalid in (True, 0, 4):
            with self.assertRaises(ValueError):
                C.grammar(invalid)

    def test_every_reference_self_test_detects_its_planted_fault_when_stubbed(self):
        self.assertEqual(C.self_test(), [])
        with patch.object(C, "agree", return_value=True):
            failures = C.self_test()
            self.assertGreaterEqual(set(failures), {"partition-negative", "rule-negative", "program-negative",
                                                   "replay-negative", "strategy-negative", "development-stop-negative", "target-count-negative"})

    def test_control_classifier_self_test_detects_a_stub_without_running_cli(self):
        import loop_composable_controls as controls
        self.assertEqual(controls.self_test(False), [])
        self.assertEqual(controls.self_test(True), ["2", "3", "4"])

    def test_exact_partitions_and_wrong_class_control(self):
        good = bank()
        self.assertEqual(C.check_bank(good), [])
        bad = deepcopy(good)
        bad["terms"][1] = WRONG
        self.assertEqual(set(C.check_bank(bad)), {"term-partition"})
        with patch.object(C, "agree", return_value=True):
            self.assertEqual(C.check_bank(bad), [])

    def test_forged_rule_control_is_load_bearing(self):
        bad = bank()
        bad["rules"] = [{"lhs": WRONG, "rhs": X}]
        self.assertIn("rule-not-equivalent", C.check_bank(bad))
        with patch.object(C, "agree", return_value=True):
            self.assertEqual(C.check_bank(bad), [])

    def test_rule_transfer_is_rechecked_on_target_domain(self):
        lhs = scalar([["Input", 0], ["Int", "0"], ["Eq", 0, 1]], 2, hi=0, boolean=True)
        rhs = scalar([["Bool", True]], 0, hi=0, boolean=True)
        self.assertEqual(C.comparison(C.program(lhs), C.program(rhs))["status"], "equivalent")
        lhs["inputs"][0]["max"] = rhs["inputs"][0]["max"] = "1"
        result = C.comparison(C.program(lhs), C.program(rhs))
        self.assertEqual((result["status"], result["ordinal"]), ("counterexample", 1))

    def test_whole_program_unused_and_unselected_eager_errors(self):
        dead = scalar([["Input", 0], ["Int", str((1 << 127) - 1)], ["Int", "1"], ["Add", 1, 2]], 0)
        branch = deepcopy(dead)
        branch["nodes"] += [["Bool", False], ["Select", 4, 3, 0]]
        branch["roots"] = [5]
        for value in (dead, branch):
            self.assertEqual(C.R.table(C.program(value))[0], ("error", "arithmetic"))
            self.assertEqual(C.comparison(C.program(value), C.program(X))["status"], "counterexample")
        groups, rejected = C.partitions([X, dead, branch])
        self.assertEqual((groups, rejected), ([[0]], [[1, 0], [2, 0]]))

    def test_qualification_and_replay_checks_have_planted_negatives(self):
        good = {"status": "qualified", "best": C.metadata(X),
                "best_check": {"verdict": "equivalent", "replay": "matched"}}
        self.assertEqual(C.check_program_report(LONG, good), [])
        bad = deepcopy(good)
        bad["best"] = C.metadata(WRONG)
        self.assertIn("qualified-wrong-program", C.check_program_report(LONG, bad))
        bad["best_check"]["replay"] = "differs"
        self.assertIn("qualified-without-replay", C.check_program_report(LONG, bad))
        with patch.object(C, "agree", return_value=True):
            self.assertEqual(C.check_program_report(LONG, bad), [])

    def test_strategy_count_selection_and_digest_are_load_bearing(self):
        policies = C.grammar(1)
        trials = [{"stages": list(word), "score": {"qualified": 1, "nodes_with_failure_penalty": 1,
                   "gate_pairs_reserved": 8, "stage_count": len(word), "grammar_index": i},
                   "runs": [{"status": "qualified", "original": C.metadata(LONG), "best": C.metadata(X),
                             "best_check": {"verdict": "equivalent", "replay": "matched"},
                             "candidates": [{"candidate": C.metadata(X), "check": {"verdict": "equivalent",
                                             "replay": "matched", "gate_pairs_reserved": 8}}]}]}
                  for i, word in enumerate(policies)]
        selection = {"depth": 1, "stages": []}
        report = {"status": "evaluated", "policies_generated": 5, "trials": trials, "selection": selection,
                  "selection_sha256": C.selection_digest(selection), "selection_frozen_before_evaluation": True,
                  "evaluation_influenced_selection": False}
        self.assertEqual(C.check_strategy(report), [])
        report["selection_sha256"] = C.digest(selection)
        self.assertEqual(C.check_strategy(report), ["selection-digest"])
        report["policies_generated"] = 4
        report["trials"][0]["score"]["qualified"] = 9
        report["selection_sha256"] = "0" * 64
        self.assertGreaterEqual(len(C.check_strategy(report)), 3)
        with patch.object(C, "agree", return_value=True):
            self.assertEqual(C.check_strategy(report), [])

    def test_reference_rejects_a_winner_after_any_development_check_stop(self):
        # Structurally plausible ranking, but a stopped first gate was recorded
        # as a normal failure. The reference must inspect gate evidence itself.
        trials = []
        for index, word in enumerate(C.grammar(1)):
            stopped = index == 4
            run = {"status": "inconclusive" if stopped else "qualified", "reason": "bounded-search-no-qualified-candidate",
                   "original": C.metadata(LONG), "best": None if stopped else C.metadata(X),
                   "best_check": {"verdict": "equivalent", "replay": "matched"},
                   "candidates": [{"candidate": C.metadata(X), "check": {"verdict": "inconclusive", "reason": "gate-budget"}
                        if stopped else {"verdict": "equivalent", "replay": "matched", "gate_pairs_reserved": 8}}]}
            trials.append({"stages": list(word), "runs": [run], "score": {"qualified": 0 if stopped else 1,
                "nodes_with_failure_penalty": 6 if stopped else 1, "gate_pairs_reserved": 0 if stopped else 8}})
        selection = {"depth": 1, "stages": []}
        report = {"status": "evaluated", "selection": selection, "selection_sha256": C.selection_digest(selection),
                  "selection_frozen_before_evaluation": True, "evaluation_influenced_selection": False,
                  "policies_generated": 5, "trials": trials}
        self.assertEqual(C.check_strategy(report), ["development-stop-selected"])
        with patch.object(C, "agree", return_value=True):
            self.assertEqual(C.check_strategy(report), [])
        # A gate-free bounded search with no candidate is an ordinary failed
        # task, whereas this same string can never hide a stopped check.
        report["trials"][-1]["runs"][0]["candidates"] = []
        self.assertEqual(C.check_strategy(report), [])

    def test_rule_improvement_labels_are_checked_against_measured_cost(self):
        report = {"status": "qualified", "original": C.metadata(X), "best": C.metadata(X),
                  "best_check": {"verdict": "equivalent", "replay": "matched"}, "rules_requalified": 0,
                  "checks": [], "applied": [], "improved": False, "result_kind": "unchanged-original"}
        self.assertEqual(C.check_rules(report), [])
        report.update(improved=True, result_kind="improved-program")
        self.assertEqual(C.check_rules(report), ["improvement-claim", "rule-result-label"])

    def test_requalified_count_and_applied_chain_are_independently_checked(self):
        matched = {"verdict": "equivalent", "replay": "matched"}
        report = {"status": "qualified", "original": C.metadata(LONG), "best": C.metadata(X),
                  "best_check": matched, "rules_requalified": 1,
                  "checks": [{"stage": "target-rule", "check": matched, "lhs": C.metadata(LONG), "rhs": C.metadata(X)}],
                  "applied": [{"before": C.metadata(LONG), "after": C.metadata(X)}]}
        self.assertEqual(C.check_rules(report), [])
        report["rules_requalified"] = 0
        report["applied"][0]["after"] = C.metadata(WRONG)
        self.assertGreaterEqual(set(C.check_rules(report)), {"requalified-rule-count", "applied-program-chain",
                                                          "final-program-chain"})
        with patch.object(C, "agree", return_value=True):
            self.assertEqual(C.check_rules(report), [])
        report["applied"][0]["after"] = C.metadata(LONG)
        self.assertIn("applied-cost", C.check_rules(report))


if __name__ == "__main__":
    unittest.main()
