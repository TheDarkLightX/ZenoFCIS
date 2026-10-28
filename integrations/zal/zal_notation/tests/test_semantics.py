"""Known semantic examples and adversarial adapter checks."""
import copy
import unittest

from zal_notation.builtin import get_builtin, lambda_grammar, model, term
from zal_notation.core import digest, validate_model
from zal_notation.semantics import (
    DisabledTransition,
    SemanticError,
    fsm_check,
    fsm_trace,
    lambda_trace,
    petri_fire,
)
from zal_notation.syntax import parse_text


def lambda_model(source):
    grammar = lambda_grammar()
    return grammar, model(grammar, parse_text(source, grammar))


def identity(name="x0"):
    return term("Lam", term("Var", name=name), name=name)


class LambdaSemanticsTests(unittest.TestCase):
    def test_identity_beta_reduction_and_report(self):
        grammar, original = lambda_model(r"(\x.x) (\y.y)")
        before = copy.deepcopy(original)
        report = lambda_trace(grammar, original)
        self.assertEqual(report["status"], "normal-form")
        self.assertEqual(report["steps"], 1)
        self.assertEqual(report["models"][-1]["body"], identity())
        self.assertEqual(report["source_model_digest"], digest(original))
        self.assertEqual(report["authority"], "none")
        self.assertEqual(report["evidence"], "checked-in-this-run")
        self.assertEqual(original, before)
        self.assertIsNot(report["models"][0], original)

    def test_shadowing(self):
        grammar, source = lambda_model(r"(\x.\x.x) (\z.z)")
        report = lambda_trace(grammar, source)
        self.assertEqual(report["models"][-1]["body"], identity())

    def test_capture_avoidance_under_outer_binder(self):
        grammar, source = lambda_model(r"\y.(\x.\y.x) y")
        report = lambda_trace(grammar, source)
        expected = term("Lam", term("Lam", term("Var", name="x0"), name="x1"), name="x0")
        self.assertEqual(report["status"], "normal-form")
        self.assertEqual(report["models"][-1]["body"], expected)

    def test_reduces_under_lambda(self):
        grammar, source = lambda_model(r"\z.(\x.x) z")
        report = lambda_trace(grammar, source)
        self.assertEqual(report["steps"], 1)
        self.assertEqual(report["models"][-1]["body"], identity())

    def test_outermost_discards_divergent_argument(self):
        grammar, source = lambda_model(r"(\x.\z.z) ((\u.u u) (\u.u u))")
        report = lambda_trace(grammar, source, max_steps=1)
        self.assertEqual(report["status"], "normal-form")
        self.assertEqual(report["models"][-1]["body"], identity())

    def test_full_normal_form_includes_argument_reduction(self):
        grammar, source = lambda_model(r"\a.a ((\b.b) (\c.c))")
        report = lambda_trace(grammar, source)
        expected = term("Lam", term("App", term("Var", name="x0"), identity("x1")), name="x0")
        self.assertEqual(report["steps"], 1)
        self.assertEqual(report["models"][-1]["body"], expected)

    def test_omega_stops_without_false_normal_form(self):
        grammar, source = lambda_model(r"(\x.x x) (\x.x x)")
        report = lambda_trace(grammar, source, max_steps=5)
        self.assertEqual(report["status"], "step-limit")
        self.assertEqual(report["steps"], 5)
        self.assertEqual(len(report["models"]), 6)

    def test_zero_budget_distinguishes_normal_and_reducible(self):
        grammar, normal = lambda_model(r"\x.x")
        _, reducible = lambda_model(r"(\x.x) (\y.y)")
        self.assertEqual(lambda_trace(grammar, normal, max_steps=0)["status"], "normal-form")
        self.assertEqual(lambda_trace(grammar, reducible, max_steps=0)["status"], "step-limit")
        for bound in (True, -1, 129, 1.5):
            with self.assertRaises(SemanticError):
                lambda_trace(grammar, normal, max_steps=bound)

    def test_free_variable_and_modified_grammar_refused(self):
        grammar, source = lambda_model("free")
        with self.assertRaises(SemanticError):
            lambda_trace(grammar, source)
        grammar, source = lambda_model(r"\x.x")
        grammar["description"] = "same id but a different profile"
        with self.assertRaises(SemanticError):
            lambda_trace(grammar, source)

    def test_size_limit_retains_last_valid_model(self):
        def big(leaves):
            if leaves == 1:
                return identity("a")
            left = leaves // 2
            return term("App", big(left), big(leaves-left))
        grammar = lambda_grammar()
        source = model(grammar, term("App",
            term("Lam", term("App", term("Var", name="x"), term("Var", name="x")), name="x"),
            big(100)))
        validate_model(grammar, source)
        report = lambda_trace(grammar, source)
        self.assertEqual(report["status"], "size-limit")
        self.assertEqual(report["steps"], 0)
        self.assertEqual(report["models"], [source])


class FsmSemanticsTests(unittest.TestCase):
    def setUp(self):
        self.grammar, _, self.model = get_builtin("state-machine")

    def test_declared_table_check_and_replay(self):
        original = copy.deepcopy(self.model)
        checked = fsm_check(self.grammar, self.model)
        self.assertEqual(checked["status"], "deterministic-complete")
        self.assertEqual(checked["alphabet"], ["release", "reserve"])
        report = fsm_trace(self.grammar, self.model, ["reserve", "release", "reserve"])
        self.assertEqual(report["status"], "completed")
        self.assertEqual([r["outcome"] for r in report["trace"]], ["Accept", "Accept", "Reject"])
        self.assertEqual(report["final_state"], "released")
        self.assertEqual(self.model, original)

    def test_duplicate_same_target_is_still_nondeterministic(self):
        edge = copy.deepcopy(self.model["body"]["edges"][0])
        edge["id"] = "duplicate"
        self.model["body"]["edges"].append(edge)
        checked = fsm_check(self.grammar, self.model)
        self.assertTrue(checked["structural_valid"])
        self.assertFalse(checked["deterministic"])
        report = fsm_trace(self.grammar, self.model, ["reserve"])
        self.assertEqual(report["status"], "refused")
        self.assertEqual(report["reason"], "nondeterministic-transition")
        self.assertEqual(report["trace"], [])

    def test_missing_and_unknown_events_are_distinct(self):
        self.model["body"]["edges"] = self.model["body"]["edges"][1:]
        checked = fsm_check(self.grammar, self.model)
        self.assertFalse(checked["complete"])
        self.assertIn({"state": "unseen", "event": "reserve"}, checked["missing"])
        self.assertEqual(fsm_trace(self.grammar, self.model, ["reserve"])["reason"], "missing-transition")
        self.assertEqual(fsm_trace(self.grammar, self.model, ["invented"])["reason"], "unknown-event")

    def test_exactly_one_initial_state(self):
        for start_tags in (0, 2):
            source = copy.deepcopy(self.model)
            for index, node in enumerate(source["body"]["nodes"]):
                node["tag"] = "start" if index < start_tags else "state"
            self.assertFalse(fsm_check(self.grammar, source)["initial_valid"])
            self.assertEqual(fsm_trace(self.grammar, source, [])["reason"], "initial-state-count")

    def test_empty_alphabet_is_explicit_and_vacuous(self):
        self.model["body"]["edges"] = []
        checked = fsm_check(self.grammar, self.model)
        self.assertEqual(checked["alphabet"], [])
        self.assertTrue(checked["complete"])
        self.assertEqual(checked["alphabet_origin"], "union-of-edge-event-labels")

    def test_guard_looking_event_is_literal(self):
        self.model["body"]["edges"][0]["fields"]["event"] = "x > 0"
        report = fsm_trace(self.grammar, self.model, ["x > 0"])
        self.assertEqual(report["status"], "completed")
        self.assertEqual(fsm_trace(self.grammar, self.model, ["reserve"])["reason"], "missing-transition")

    def test_trace_retains_successful_prefix(self):
        report = fsm_trace(self.grammar, self.model, ["reserve", "unknown"])
        self.assertEqual(report["status"], "refused")
        self.assertEqual(report["steps"], 1)
        self.assertEqual(report["final_state"], "held")
        self.assertEqual(report["refusal"]["index"], 1)
        with self.assertRaises(SemanticError):
            fsm_trace(self.grammar, self.model, [1])


class PetriSemanticsTests(unittest.TestCase):
    def setUp(self):
        self.grammar, _, self.model = get_builtin("petri-net")

    def test_enabled_fire_and_no_mutation(self):
        original = copy.deepcopy(self.model)
        result = petri_fire(self.grammar, self.model, "reserve")
        self.assertEqual([n["fields"]["tokens"] for n in result["body"]["nodes"] if n["tag"] == "place"], [2, 1])
        self.assertEqual(self.model, original)
        self.assertIsNot(result, self.model)

    def test_parallel_input_arcs_are_aggregated_before_enabling(self):
        self.model["body"]["nodes"][0]["fields"]["tokens"] = 1
        edge = copy.deepcopy(self.model["body"]["edges"][0])
        edge["id"] = "also-take"
        self.model["body"]["edges"].append(edge)
        with self.assertRaises(DisabledTransition):
            petri_fire(self.grammar, self.model, "reserve")
        self.model["body"]["nodes"][0]["fields"]["tokens"] = 2
        result = petri_fire(self.grammar, self.model, "reserve")
        self.assertEqual(result["body"]["nodes"][0]["fields"]["tokens"], 0)

    def test_parallel_output_arcs_are_aggregated(self):
        edge = copy.deepcopy(self.model["body"]["edges"][1])
        edge["id"] = "also-put"
        edge["fields"]["weight"] = 2
        self.model["body"]["edges"].append(edge)
        result = petri_fire(self.grammar, self.model, "reserve")
        self.assertEqual(result["body"]["nodes"][2]["fields"]["tokens"], 3)

    def test_output_cannot_fund_disabled_input(self):
        self.model["body"]["nodes"][0]["fields"]["tokens"] = 0
        self.model["body"]["edges"][1]["target"] = ["available", "in"]
        with self.assertRaises(DisabledTransition):
            petri_fire(self.grammar, self.model, "reserve")

    def test_negative_tokens_nonpositive_weights_and_nonbipartite_arcs(self):
        for change in ("tokens", "weight", "bipartite"):
            source = copy.deepcopy(self.model)
            if change == "tokens":
                source["body"]["nodes"][0]["fields"]["tokens"] = -1
            elif change == "weight":
                source["body"]["edges"][0]["fields"]["weight"] = 0
            else:
                source["body"]["edges"][0]["target"] = ["reserved", "in"]
            with self.subTest(change=change), self.assertRaises(SemanticError):
                petri_fire(self.grammar, source, "reserve")

    def test_unknown_or_place_transition_id_refused(self):
        for node in ("missing", "available", 1):
            with self.assertRaises(SemanticError):
                petri_fire(self.grammar, self.model, node)


if __name__ == "__main__":
    unittest.main()
