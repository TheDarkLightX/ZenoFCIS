"""Replay, observation loss, and direction-sensitive finite reasoning tests."""
import copy
import unittest

from zal_notation.builtin import get_builtin
from zal_notation.core import NotationError, canonical, digest, model_digest, term_model
from zal_notation.reasoning import (
    check_relation,
    check_transfer_claim,
    reasoning_step,
    transfer_definition,
    transfer_demo,
    transfer_snapshot,
    transfer_table,
    transfer_total_table,
    validate_table,
)
from zal_notation.syntax import parse_text


def table(id_, inputs, outputs, rows):
    return {"schema": "zal.finite-table/1", "id": id_, "version": 1,
            "inputs": inputs, "outputs": outputs, "rows": rows}


class SemanticStepTests(unittest.TestCase):
    def test_single_lambda_step_is_replayable_and_identity_bound(self):
        grammar, _, _ = get_builtin("lambda")
        source = term_model(grammar, parse_text(r"(\x.x) (\y.y)", grammar))
        before = copy.deepcopy(source)
        request = {"kind": "lambda-beta"}
        result = reasoning_step(grammar, source, request)
        self.assertEqual(result["status"], "checked")
        self.assertEqual(result["source_model_digest"], model_digest(grammar, source))
        self.assertEqual(result["target_model_digest"], model_digest(grammar, result["target_model"]))
        self.assertEqual(reasoning_step(grammar, source, result["replay_request"]), result)
        self.assertEqual(source, before)
        self.assertEqual(result["authority"], "none")
        self.assertEqual(result["target_model"]["body"]["tag"], "Lam")

    def test_no_redex_refutes_rule_precondition(self):
        grammar, _, source = get_builtin("lambda")
        result = reasoning_step(grammar, source, {"kind": "lambda-beta"})
        self.assertEqual(result["status"], "refuted")
        self.assertEqual(result["reason"], "no-beta-redex")
        self.assertNotIn("target_model", result)

    def test_fold_is_not_a_semantic_rewrite(self):
        grammar, _, source = get_builtin("zal-folds")
        result = reasoning_step(grammar, source, {"kind": "fold"})
        self.assertEqual(result["status"], "unsupported")
        self.assertEqual(result["reason"], "folding-is-a-view-operation")
        self.assertNotIn("target_model", result)

    def test_fsm_moves_configuration_without_rewriting_machine(self):
        grammar, _, source = get_builtin("state-machine")
        before = copy.deepcopy(source)
        result = reasoning_step(grammar, source,
                                {"kind": "fsm-transition", "state": "held", "event": "release"})
        self.assertEqual(result["status"], "checked")
        self.assertEqual(result["target_model"], source)
        self.assertEqual(result["source_model_digest"], result["target_model_digest"])
        self.assertEqual(result["source_configuration"]["state"], "held")
        self.assertEqual(result["target_configuration"]["state"], "released")
        self.assertNotEqual(result["source_configuration_digest"], result["target_configuration_digest"])
        self.assertEqual(source, before)
        self.assertEqual([n["id"] for n in result["target_model"]["body"]["nodes"] if n["tag"] == "start"], ["unseen"])

    def test_fsm_missing_and_duplicate_transitions_refuted(self):
        grammar, _, source = get_builtin("state-machine")
        result = reasoning_step(grammar, source, {"kind": "fsm-transition", "event": "unknown"})
        self.assertEqual(result["status"], "refuted")
        extra = copy.deepcopy(source["body"]["edges"][0])
        extra["id"] = "duplicate"
        source["body"]["edges"].append(extra)
        result = reasoning_step(grammar, source, {"kind": "fsm-transition", "event": "reserve"})
        self.assertEqual(result["reason"], "transition-is-not-unique")
        self.assertEqual(len(result["counterexample"]["candidate_edges"]), 2)

    def test_petri_firing_and_disabled_counterexample(self):
        grammar, _, source = get_builtin("petri-net")
        result = reasoning_step(grammar, source, {"kind": "petri-fire", "transition": "reserve"})
        self.assertEqual(result["status"], "checked")
        self.assertEqual(result["target_model"]["body"]["nodes"][0]["fields"]["tokens"], 2)
        source["body"]["nodes"][0]["fields"]["tokens"] = 0
        refused = reasoning_step(grammar, source, {"kind": "petri-fire", "transition": "reserve"})
        self.assertEqual(refused["status"], "refuted")
        self.assertEqual(refused["reason"], "transition-disabled")

    def test_arbitrary_grammar_does_not_inherit_semantics(self):
        grammar, _, source = get_builtin("zal-folds")
        result = reasoning_step(grammar, source, {"kind": "lambda-beta"})
        self.assertEqual(result["status"], "unsupported")
        self.assertEqual(result["reason"], "unsupported-lambda-grammar")


class FiniteRelationTests(unittest.TestCase):
    def test_equivalence_aligns_input_order_and_replays(self):
        left = table("test/left", ["a", "b"], [10, 20], [[0], [1]])
        right = table("test/right", ["b", "a"], [20, 10], [[0], [1]])
        result = check_relation(left, right)
        self.assertEqual(result["status"], "checked")
        self.assertEqual(result["checked_cases"], 2)
        self.assertEqual(result["replay_request"]["input_map"], [1, 0])
        self.assertEqual(check_relation(left, right, **result["replay_request"]), result)
        self.assertEqual(result["endpoints"]["left"]["table_digest"], digest(left))

    def test_refinement_has_direction_and_concrete_witness(self):
        implementation = table("test/implementation", ["x"], [1], [[0]])
        specification = table("test/specification", ["x"], [1, 2], [[0, 1]])
        good = check_relation(implementation, specification, relation="refinement")
        self.assertEqual(good["status"], "checked")
        self.assertEqual(good["comparison"], "left-subset-right")
        bad = check_relation(specification, implementation, relation="refinement")
        self.assertEqual(bad["status"], "refuted")
        self.assertEqual(bad["counterexample"]["unmatched_left"], [2])

    def test_refinement_cannot_drop_admitted_inputs(self):
        left = table("test/left", ["a"], [0], [[0]])
        right = table("test/right", ["a", "b"], [0], [[0], [0]])
        result = check_relation(left, right, relation="refinement")
        self.assertEqual(result["status"], "unsupported")
        self.assertEqual(result["reason"], "input-map-does-not-cover-the-right-domain")

    def test_empty_outcome_set_is_not_vacuous_refinement(self):
        left = table("test/left", ["a"], [0], [[]])
        with self.assertRaises(NotationError):
            check_relation(left, table("test/right", ["a"], [0], [[0]]), relation="refinement")

    def test_sound_and_exact_abstraction_are_distinct(self):
        left = table("test/concrete", ["a", "b"], [0], [[0], [0]])
        right = table("test/abstract", ["merged"], [0, 1], [[0, 1]])
        sound = check_relation(left, right, relation="abstraction", input_map=[0, 0])
        exact = check_relation(left, right, relation="abstraction", exact=True, input_map=[0, 0])
        self.assertEqual(sound["status"], "checked")
        self.assertEqual(exact["status"], "refuted")
        self.assertEqual(exact["counterexample"]["unmatched_right"], [1])
        self.assertEqual(check_relation(left, right, input_map=[0, 0])["status"], "unsupported")

    def test_observation_maps_can_forget_meaning_deliberately(self):
        left = table("test/left", ["request"], [{"count": 5, "outcome": "Accept"}], [[0]])
        right = table("test/right", ["request"], [{"count": 5, "outcome": "Reject"}], [[0]])
        self.assertEqual(check_relation(left, right)["status"], "refuted")
        observed = check_relation(left, right, left_observation=[5], right_observation=[5])
        self.assertEqual(observed["status"], "checked")
        self.assertEqual(observed["replay_request"]["left_observation"], [5])

    def test_json_true_is_not_integer_one(self):
        left = table("test/left", ["x"], [True], [[0]])
        right = table("test/right", ["x"], [1], [[0]])
        self.assertEqual(check_relation(left, right)["status"], "refuted")

    def test_malformed_maps_and_tables_refused(self):
        base = table("test/base", ["x"], [0], [[0]])
        for malformed in (
            table("test/bad", ["x", "x"], [0], [[0], [0]]),
            table("test/bad", ["x"], [0, 0], [[0]]),
            table("test/bad", ["x"], [0], [[0, 0]]),
            table("test/bad", ["x"], [0], [[True]]),
            table("test/bad", [], [0], []),
        ):
            with self.assertRaises(NotationError):
                validate_table(malformed)
        for options in ({"input_map": [True]}, {"input_map": []}, {"left_observation": []}):
            with self.assertRaises(NotationError):
                check_relation(base, base, **options)
        self.assertEqual(check_relation(base, base, relation="all-programs-equal")["status"], "unsupported")

    def test_changed_endpoint_invalidates_prior_binding(self):
        left = table("test/left", [0], [1], [[0]])
        right = table("test/right", [0], [1], [[0]])
        original = check_relation(left, right)
        right["outputs"] = [2]
        revised = check_relation(left, right)
        self.assertEqual(revised["status"], "refuted")
        self.assertNotEqual(original["endpoints"]["right"]["table_digest"],
                            revised["endpoints"]["right"]["table_digest"])


class TransferReasoningTests(unittest.TestCase):
    def test_snapshot_boundary_and_policy_observations(self):
        accepted = transfer_snapshot(7, 4, 3)
        self.assertEqual((accepted["next_available"], accepted["next_reserved"], accepted["outcome"]), (4, 7, "Accept"))
        rejected = transfer_snapshot(7, 4, 3, policy="reject-all")
        self.assertTrue(rejected["guard"])
        self.assertFalse(rejected["accepted"])
        self.assertEqual(rejected["outcome"], "Reject")
        self.assertEqual(rejected["total_after"], accepted["total_after"])
        self.assertEqual(transfer_snapshot(7, 4, 7)["outcome"], "Accept")
        self.assertEqual(transfer_snapshot(7, 4, 8)["outcome"], "Reject")
        self.assertEqual(transfer_snapshot(1, 15, 1)["outcome"], "Reject")
        self.assertEqual(transfer_snapshot(7, 4, 0)["outcome"], "Reject")

    def test_full_domain_and_reject_all_counterexample(self):
        demo = transfer_demo()
        canonical(demo)  # Compact receipts fit the ordinary exchange budget.
        self.assertEqual(demo["tables"]["guarded"]["input_count"], 4096)
        checks = demo["checks"]
        self.assertEqual(checks["full_equivalence"]["status"], "refuted")
        witness = checks["full_equivalence"]["counterexample"]
        self.assertEqual(witness["left_input"], [1, 0, 1])
        self.assertEqual(witness["left_outputs"], [[0, 1, "Accept"]])
        self.assertEqual(witness["right_outputs"], [[1, 0, "Reject"]])
        for label in ("total_equivalence", "guarded_total_abstraction", "reject_all_total_abstraction"):
            self.assertEqual(checks[label]["status"], "checked")
            self.assertEqual(checks[label]["checked_cases"], 4096)
        self.assertEqual(demo["authority"], "none")

    def test_generated_table_matches_independent_small_domain_oracle(self):
        for policy in ("guarded", "reject-all"):
            result = transfer_table(policy, maximum=3)
            for (available, reserved, requested), row in zip(result["inputs"], result["rows"]):
                can_move = (policy == "guarded" and requested != 0
                            and available >= requested and reserved + requested <= 3)
                expected = ([available - requested, reserved + requested, "Accept"] if can_move
                            else [available, reserved, "Reject"])
                self.assertEqual(result["outputs"][row[0]], expected)

    def test_zero_domain_bound_and_invalid_inputs(self):
        self.assertEqual(transfer_demo(maximum=0)["checks"]["full_equivalence"]["status"], "checked")
        self.assertEqual(transfer_total_table(maximum=0)["inputs"], [[0, 0]])
        self.assertEqual(transfer_definition()["maximum"], 15)
        for args, options in (((True, 0, 0), {}), ((1, -1, 0), {}), ((0, 0, 0), {"maximum": 16}),
                              ((0, 0, 0), {"policy": "eval"})):
            with self.assertRaises(NotationError):
                transfer_snapshot(*args, **options)

    def test_compact_transfer_claims_keep_scope_identity_and_honest_counts(self):
        for claim in ("total-preserved", "same-total-as-reject-all", "full-equivalence-reject-all"):
            result = check_transfer_claim(claim)
            self.assertEqual(result["replay_request"], {"kind": "transfer-check", "claim": claim})
            self.assertEqual(result["profile_digest"], digest(transfer_definition()))
            self.assertEqual(result["admitted_cases"], 4096)
            self.assertLess(len(canonical(result).encode("utf-8")), 12000)
            self.assertEqual(result["authority"], "none")
            if claim == "full-equivalence-reject-all":
                self.assertEqual(result["status"], "refuted")
                self.assertEqual(result["checked_cases"], 258)
                self.assertEqual(result["counterexample"]["left_input"], [1, 0, 1])
            else:
                self.assertEqual(result["status"], "checked")
                self.assertEqual(result["checked_cases"], 4096)
        with self.assertRaises(NotationError):
            check_transfer_claim("arbitrary injected theorem")


if __name__ == "__main__":
    unittest.main()
