"""Revision-bound shared meaning, inert declarations, and replay adversaries."""
import copy
import unittest

from zal_notation.builtin import get_builtin
from zal_notation.core import NotationError, digest, term_model
from zal_notation.examples import custom_recipe
from zal_notation.sharing import (
    acknowledge,
    add_note,
    add_reasoning,
    compare_discussions,
    dispatch_tool,
    inspect_discussion,
    make_context,
    make_discussion,
    make_transfer_bundle,
    replay_discussion,
    run_request,
    validate_context,
    validate_discussion,
)
from zal_notation.syntax import parse_text


def recipe_discussion():
    grammar, notation, model, _ = custom_recipe()
    context = make_context(
        grammar, model,
        semantics={"profile": "user/recipe-syntax/1", "parameters": {"ordered": True},
                   "description": "An ordered symbolic composition; no cooking evaluator is registered."},
        assumptions=["Ingredient labels name symbols."],
        observation={"id": "whole-recipe", "description": "Retain the entire ordered recipe.",
                     "keeps": ["ingredients", "composition"], "omits": []},
    )
    return make_discussion(context, notation, title="Alice and Bob discuss a recipe")


def executable_discussion(name="lambda", profile="zal/lambda-normal-order/1"):
    grammar, notation, model = get_builtin(name)
    if name == "lambda":
        model = term_model(grammar, parse_text(r"(\x.x) (\y.y)", grammar))
    context = make_context(
        grammar, model, semantics={"profile": profile, "parameters": {}, "description": "The registered semantics."},
        assumptions=[], observation={"id": "whole-model", "description": "Retain the whole model.",
                                     "keeps": ["model"], "omits": []},
    )
    return make_discussion(context, notation)


class SharedContextTests(unittest.TestCase):
    def test_people_can_share_one_context_with_different_custom_glyphs(self):
        alice = acknowledge(recipe_discussion(), speaker="Alice")
        alice = add_note(alice, speaker="Alice", text="Is this composition ordered?", anchor="root")
        bob = copy.deepcopy(alice)
        bob["notation"]["id"] = "bob/recipe-symbols"
        bob["notation"]["glyphs"]["Mix"]["primitives"][0]["stroke"] = "ink"
        bob["presentation"]["folded"] = ["root.children.1"]
        bob = add_note(bob, speaker="Bob", text="Yes; child positions remain part of this model.",
                       kind="explanation", anchor="root", reply_to="n1")
        bob = acknowledge(bob, speaker="Bob")
        comparison = compare_discussions(alice, bob)
        self.assertEqual(comparison["status"], "same-context")
        self.assertFalse(comparison["same_notation"])
        self.assertEqual(comparison["differences"], [])
        self.assertEqual(inspect_discussion(bob)["current_acknowledgements"], ["Alice", "Bob"])
        self.assertEqual(len(alice["notes"]), 1)
        self.assertEqual(len(alice["acknowledgements"]), 1)

    def test_all_five_context_components_participate_in_identity(self):
        original = recipe_discussion()
        mutations = {
            "grammar": lambda c: c["grammar"].update(description="A new grammar declaration."),
            "model": lambda c: c["model"]["body"]["children"][0]["fields"].update(name="rice"),
            "semantics": lambda c: c["semantics"]["parameters"].update(ordered=False),
            "assumptions": lambda c: c["assumptions"].append("Mixing is commutative."),
            "observation": lambda c: c["observation"].update(description="A differently declared observation."),
        }
        for component, mutate in mutations.items():
            with self.subTest(component=component):
                changed = copy.deepcopy(original)
                mutate(changed["context"])
                with self.assertRaises(NotationError):
                    validate_discussion(changed)
                changed["context_digest"] = digest(changed["context"])
                report = compare_discussions(original, changed)
                self.assertEqual(report["status"], "different-context")
                self.assertEqual(report["differences"], [component])
                self.assertNotEqual(report["left_context"], report["right_context"])

    def test_context_revision_does_not_silently_renew_notes_or_agreements(self):
        original = acknowledge(recipe_discussion(), speaker="Alice")
        original = add_note(original, speaker="Alice", text="I am using the ordered interpretation.", anchor="root")
        original = add_reasoning(original, {"kind": "user-unregistered-rule"})
        changed = copy.deepcopy(original)
        changed["context"]["assumptions"].append("The left input is evaluated first.")
        changed["context_digest"] = digest(changed["context"])
        changed = acknowledge(changed, speaker="Bob")
        inspection = inspect_discussion(changed)
        self.assertEqual(inspection["current_acknowledgements"], ["Bob"])
        self.assertEqual(inspection["stale_acknowledgements"], ["Alice"])
        self.assertEqual(inspection["stale_notes"], ["n1"])
        self.assertEqual(replay_discussion(changed)["records"], [{"id": "r1", "status": "stale-context"}])
        self.assertEqual(inspect_discussion(original)["current_acknowledgements"], ["Alice"])

    def test_inspection_is_not_an_alias_that_can_mutate_the_context(self):
        bundle = recipe_discussion()
        before = copy.deepcopy(bundle)
        report = inspect_discussion(bundle)
        report["assumptions"].append("Injected by a consumer of the inspection.")
        report["observation"]["keeps"].clear()
        report["semantics"]["parameters"]["ordered"] = False
        self.assertEqual(bundle, before)
        validate_discussion(bundle)

    def test_self_declared_name_never_becomes_authenticated_identity(self):
        bundle = acknowledge(recipe_discussion(), speaker="Alice")
        inspection = inspect_discussion(bundle)
        self.assertEqual(inspection["authority"], "none")
        self.assertIn("no authentication", inspection["identity"])
        bundle["acknowledgements"][0]["identity"] = "cryptographically-authenticated"
        with self.assertRaises(NotationError):
            validate_discussion(bundle)

    def test_observation_projection_is_a_different_shared_context(self):
        full = make_transfer_bundle()
        total = make_transfer_bundle(observation_id="total")
        self.assertEqual(full["context"]["model"], total["context"]["model"])
        self.assertEqual(compare_discussions(full, total)["differences"], ["observation"])
        self.assertNotEqual(full["context_digest"], total["context_digest"])

    def test_registered_transfer_profile_cannot_be_reused_for_a_different_map(self):
        context = make_transfer_bundle()["context"]
        changes = (
            lambda c: c["semantics"]["parameters"].update(max_balance=16),
            lambda c: c["assumptions"].__setitem__(0, "All integers are unbounded."),
            lambda c: c["observation"].update(keeps=["total"], omits=["outcome"]),
            lambda c: c["grammar"].update(description="A grammar with a reused profile name."),
        )
        for mutate in changes:
            changed = copy.deepcopy(context)
            mutate(changed)
            with self.assertRaises(NotationError):
                validate_context(changed)

    def test_parameterless_registered_profiles_refuse_ignored_parameters(self):
        for builtin, profile in (("lambda", "zal/lambda-normal-order/1"),
                                 ("state-machine", "zal/fsm/1"), ("petri-net", "zal/petri/1")):
            with self.subTest(profile=profile):
                context = executable_discussion(builtin, profile)["context"]
                context["semantics"]["parameters"] = {"strategy": "user-selected-alternative"}
                with self.assertRaises(NotationError):
                    validate_context(context)

    def test_exact_registered_profile_cannot_grant_semantics_to_custom_grammar(self):
        context = recipe_discussion()["context"]
        context["semantics"].update(profile="zal/lambda-normal-order/1", parameters={})
        with self.assertRaises(NotationError):
            validate_context(context)

    def test_unknown_profile_remains_shareable_but_has_no_borrowed_semantics(self):
        bundle = recipe_discussion()
        self.assertEqual(inspect_discussion(bundle)["status"], "structurally-valid")
        self.assertEqual(run_request(bundle, {"kind": "cook"})["status"], "unsupported")
        with self.assertRaises(NotationError):
            run_request(bundle, {"kind": "lambda-beta"})
        recorded = add_reasoning(bundle, {"kind": "cook"})
        self.assertEqual(recorded["reasoning"][0]["result"]["status"], "unsupported")
        self.assertEqual(replay_discussion(recorded)["records"][0]["replayed_result_status"], "unsupported")

    def test_browser_shared_context_refuses_silent_numeric_rounding(self):
        for number in (9007199254740992, 0.5, float("inf")):
            with self.subTest(number=number):
                context = recipe_discussion()["context"]
                context["semantics"]["parameters"]["quantity"] = number
                with self.assertRaises(NotationError):
                    validate_context(context)

    def test_note_anchors_and_threads_are_checked_against_current_revision(self):
        bundle = recipe_discussion()
        with self.assertRaises(NotationError):
            add_note(bundle, speaker="Alice", text="Bad anchor", anchor="root.children.9")
        with self.assertRaises(NotationError):
            add_note(bundle, speaker="Alice", text="Future reply", reply_to="n99")
        for invalid in ([], {}, None, 1):
            with self.subTest(kind=invalid), self.assertRaises(NotationError):
                run_request(bundle, {"kind": invalid})


class DiscussionReplayTests(unittest.TestCase):
    def test_checked_lambda_step_is_recomputed_without_advancing_source_model(self):
        source = executable_discussion()
        before = copy.deepcopy(source)
        result = dispatch_tool("zal_reason", {"bundle": source, "request": {"kind": "lambda-beta"}})
        receipt = result["reasoning"][0]["result"]
        self.assertEqual(receipt["status"], "checked")
        self.assertNotEqual(receipt["source_model_digest"], receipt["target_model_digest"])
        self.assertEqual(source, before)
        self.assertEqual(result["context"], source["context"])
        self.assertEqual(inspect_discussion(result)["reasoning_status"], "not-replayed")
        self.assertEqual(dispatch_tool("zal_replay", {"bundle": result})["status"], "replayed")

    def test_changing_cached_result_cannot_create_checked_evidence(self):
        bundle = add_reasoning(executable_discussion(), {"kind": "lambda-beta"})
        bundle["reasoning"][0]["result"]["authority"] = "proof"
        self.assertEqual(inspect_discussion(bundle)["reasoning_status"], "not-replayed")
        replay = replay_discussion(bundle)
        self.assertEqual(replay["status"], "unresolved")
        self.assertEqual(replay["records"][0]["status"], "result-mismatch")
        self.assertEqual(replay["authority"], "none")

    def test_request_tampering_is_detected_even_when_both_results_are_checked(self):
        bundle = executable_discussion("state-machine", "zal/fsm/1")
        bundle = add_reasoning(bundle, {"kind": "fsm-transition", "state": "unseen", "event": "reserve"})
        self.assertEqual(bundle["reasoning"][0]["result"]["status"], "checked")
        bundle["reasoning"][0]["request"]["event"] = "release"
        replay = replay_discussion(bundle)
        self.assertEqual(replay["records"][0]["replayed_result_status"], "checked")
        self.assertEqual(replay["records"][0]["status"], "result-mismatch")

    def test_fold_changes_presentation_without_creating_semantic_receipt(self):
        bundle = recipe_discussion()
        context_id = bundle["context_digest"]
        bundle["presentation"]["folded"] = ["root"]
        validate_discussion(bundle)
        self.assertEqual(bundle["context_digest"], context_id)
        self.assertEqual(run_request(bundle, {"kind": "fold"})["status"], "unsupported")
        self.assertEqual(bundle["reasoning"], [])

    def test_tool_dispatch_refuses_unadvertised_arguments_and_names(self):
        bundle = recipe_discussion()
        for name, arguments in (([], {"bundle": bundle}),
                                ("zal_inspect", {"bundle": bundle, "execute": "arbitrary code"}),
                                ("zal_unknown", {"bundle": bundle})):
            with self.subTest(name=name), self.assertRaises(NotationError):
                dispatch_tool(name, arguments)


if __name__ == "__main__":
    unittest.main()
