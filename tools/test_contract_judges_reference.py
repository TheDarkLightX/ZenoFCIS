"""Checks of the independent contract-judge reference on hand-computed cases."""

import json
import tempfile
import unittest
from pathlib import Path

import contract_judges_reference as reference
import instantiate_core as core


def instance(family, parameters, complete=False):
    files = core.source(family, parameters, include_proof=False)
    project=files["project.zeno"].decode()
    policy=json.loads(files["v2/policy.json"])
    if not complete:
        project=project.split("// Independent complete relation")[0]
        policy["law_kinds"]={k:v for k,v in policy["law_kinds"].items() if int(k)<520}
    return reference.Contract(project, policy)


def value(text, grammar=reference.rule_expression, frame=()):
    return reference.compile_tree(grammar(text), {}, {})(list(frame))


class Grammar(unittest.TestCase):
    def test_rule_expressions_bind_as_the_rules_reference_lists(self):
        self.assertEqual(value("1 + 2 == 3 && !false"), 1)
        self.assertEqual(value("2 * 3 + 1"), 7)
        # `->` groups to the right: false -> (false -> false).
        self.assertEqual(value("false -> false -> false"), 1)
        self.assertEqual(value("choose(1 == 2, 5, 6)"), 6)
        self.assertEqual(value("-3 + 1"), -2)

    def test_law_negation_takes_a_whole_relation(self):
        self.assertEqual(value("!1 == 2", reference.law_formula), 1)
        self.assertEqual(value("div_floor(-3, 2) == -2 && div_ceil(-3, 2) == -1",
                               reference.law_formula), 1)
        with self.assertRaises(reference.Undefined):
            value("div_floor(1, 0) == 0", reference.law_formula)
        with self.assertRaises(reference.Unsupported):
            reference.law_formula("forall i in 0..3 { true }")

    def test_every_node_is_evaluated(self):
        # A missing value anywhere makes the formula undefined, even where a
        # short-circuit would not reach it.
        tree = reference.law_formula("1 == 1 || post.100.110 == 0")
        function = reference.compile_tree(tree, {"post.100.110": 0}, {})
        with self.assertRaises(reference.Undefined):
            function([None])
        self.assertEqual(function([4]), 1)


class Judges(unittest.TestCase):
    def test_a_bounded_counter_has_three_reachable_minimal_states_and_no_determined_tuple(self):
        contract = instance("bounded-counter", {"C": 2})
        machine = reference.Machine(contract)
        profile = reference.profile(contract, machine)
        self.assertEqual(profile["states"], {"schema": 3, "valid": 3, "reachable": 3, "minimal": 3})
        gaps = reference.gaps(contract, machine)
        self.assertEqual(gaps["admissible"]["full"]["tuples"], 12)
        self.assertEqual(gaps["admissible"]["full"]["determined"], 0)
        self.assertEqual(gaps["admissible"]["full"]["freedom"],
                         {"class": 4, "reject_reason": 12, "successor": 0})
        self.assertTrue(gaps["reject_everything"]["admitted_where_the_contract_decides"])
        self.assertEqual(gaps["activation"]["full"], {"500": 4, "501": 4, "502": 4})

    def test_a_retry_budget_stores_a_state_its_decisions_never_show(self):
        contract = instance("retry-budget", {"C": 1})
        profile = reference.profile(contract, reference.Machine(contract))
        self.assertEqual(profile["states"], {"schema": 4, "valid": 4, "reachable": 4, "minimal": 3})

    def test_lemma_verdicts(self):
        contract = instance("logical-deadline", {"C": 1})
        machine = reference.Machine(contract)
        verdict = lambda text: reference.lemma(contract, machine, text)
        self.assertEqual(verdict("post.100.110 <= 1")["verdict"], "inductive")
        self.assertEqual(verdict("post.100.112 == 0")["verdict"], "false")
        self.assertEqual(verdict("post.100.110 == 0")["verdict"], "false")
        weak = verdict("!(post.100.110 == 0 && post.100.111 == 0 && post.100.112 == 1)")
        self.assertEqual(weak["verdict"], "not-inductive")
        self.assertEqual(weak["counterexample_to_induction"]["state"], [0, 0, 0])

    def test_a_scenario_replays_and_kills_the_reject_everything_core(self):
        contract = instance("bounded-counter", {"C": 2})
        machine = reference.Machine(contract)
        up = "# scenario up\n0 | 150 | 1 | accept - 1 | -\n1 | 150 | 1 | accept - 2 | -\n"
        result = reference.scenarios(contract, machine, up)
        self.assertEqual([item["verdict"] for item in result["scenarios"]], ["passed"])
        self.assertTrue(result["reject_everything_killed"])
        # A scenario that fails on the contract credits no kill.
        wrong = up + "# scenario wrong\n0 | 150 | 1 | reject 201 0 | -\n"
        result = reference.scenarios(contract, machine, wrong)
        self.assertEqual([item["verdict"] for item in result["scenarios"]], ["passed", "failed"])
        self.assertEqual(result["reject_everything_killed"], "withheld")

    def test_committed_failures_and_refusals_are_decided_as_the_rules_say(self):
        files = core.source("bounded-counter", {"C": 2}, include_proof=False)
        policy = json.loads(files["v2/policy.json"])
        dec = next(case for case in policy["cases"] if case["when"] == "action == 151 && value == 0")
        dec["class"] = "CommittedFailure"
        policy["law_kinds"].update({"503": "CommittedFailureEffects", "504": "DebitCreditEffectEquality"})
        policy["law_kinds"]={k:v for k,v in policy["law_kinds"].items() if int(k)<520}
        project = files["project.zeno"].decode().split("// Independent complete relation")[0] + (
            "law 503 keeps on failure = post.100.110 == pre.100.110;\n"
            "law 504 short on accept = post.100.110 <= 1;\n")
        contract = reference.Contract(project, policy)
        machine = reference.Machine(contract)
        self.assertEqual(machine.table[((0,), (151, 1))], ("CommittedFailure", 202, (0,)))
        self.assertIsNone(machine.table[((1,), (150, 1))])
        self.assertEqual(machine.refusals[((1,), (150, 1))], ("law", 504))
        gaps = reference.gaps(contract, machine)
        self.assertEqual(gaps["admissible"]["full"]["reference_refused"], 1)
        probes = reference.probes(contract, machine)
        self.assertEqual(probes["decisions"]["Refused law 504"], 1)
        self.assertEqual(probes["decisions"]["CommittedFailure 202"], 1)

    def test_complete_counter_and_budget_relations(self):
        for family in ("bounded-counter", "consumable-budget"):
            contract=instance(family, {"C":2}, complete=True)
            measured=reference.gaps(contract, reference.Machine(contract))
            self.assertEqual(measured["admissible"]["full"]["determined"], measured["admissible"]["full"]["tuples"])

    def test_deliveries_are_outside_the_reference(self):
        app = core.ROOT / "examples/dual-approval"
        with self.assertRaises(reference.Unsupported):
            reference.Contract((app / "project.zeno").read_text(),
                               json.loads((app / "v2/policy.json").read_text()))

    def test_the_command_line_prints_known_answers(self):
        files = core.source("bounded-counter", {"C": 1}, include_proof=False)
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-reference-") as raw:
            root = Path(raw)
            for name, contents in files.items():
                (root / name).parent.mkdir(parents=True, exist_ok=True)
                (root / name).write_bytes(contents)
            answers = reference.answers(root, ["true"], None)
        self.assertEqual(answers["lemmas"][0]["verdict"], "inductive")
        self.assertEqual(answers["profile"]["states"]["schema"], 2)


if __name__ == "__main__":
    unittest.main()
