"""Cross-module controls for source-retaining story and notation APIs."""
import copy
import unittest

from zal_notation import abstractions, clock, sharing, story
from zal_notation.core import NotationError, digest
from zal_notation.examples import custom_recipe


def draft(identifier, statement):
    return abstractions.make_entry(
        identifier, identifier, story=statement,
        requirements=[{"id": "choice", "statement": statement, "anchors": []}],
        open_questions=["How is this interpretation implemented?"],
    )


def fold_menu(options):
    return {
        "id": "interpret_owner_car",
        "base": None,
        "scene": "An author asks for a precise interpretation.",
        "question": "Which meaning was intended?",
        "operation": "generative-fold",
        "source_text": "Every owner has a car.\nDoes this mean at least one or exactly one?",
        "options": options,
    }


def option(identifier, statement):
    return {"id": identifier, "label": statement, "entry": draft(identifier, statement)}


class MeaningIntegrationTests(unittest.TestCase):
    def test_generative_fold_retains_source_alternatives_and_unfinished_decision(self):
        initial = story.create("Interpret an informal requirement", ["alice", "bob"])
        before = copy.deepcopy(initial)
        menu = fold_menu([option("some", "At least one car"), option("one", "Exactly one car")])
        proposed = sharing.dispatch_tool("zal_sdd_propose", {
            "session": initial, "expected_revision": story.revision(initial), "menu": menu,
        })
        self.assertEqual(initial, before)
        menu_digest = digest(menu)
        first = story.vote(proposed, story.revision(proposed), menu_digest, "alice", digest(menu["options"][0]))
        split = story.vote(first, story.revision(first), menu_digest, "bob", digest(menu["options"][1]))
        view = story.replay(split)
        self.assertEqual(view["status"], "awaiting-choices")
        self.assertEqual(view["pending"], menu)
        self.assertEqual(len(view["ballots"]), 2)
        self.assertEqual(view["identity"], "self-declared local participants; no authentication")
        with self.assertRaises(NotationError):
            story.export_contract(split)

        view["pending"]["options"][0]["entry"]["story"] = "consumer mutation"
        self.assertEqual(story.replay(split)["pending"], menu)
        decided = story.vote(split, story.revision(split), menu_digest, "bob", digest(menu["options"][0]))
        packet = story.export_contract(decided)
        self.assertEqual(packet["session"]["events"][0]["menu"], menu)
        self.assertEqual(packet["selected"], menu["options"][0]["entry"])
        self.assertEqual(packet["formalization"]["status"], "draft")
        self.assertEqual(packet["authority"], "none")
        self.assertEqual(story.import_contract(packet), decided)
        with self.assertRaises(NotationError):
            story.execute(packet["selected"], {"kind": "lambda-beta"})

        changed = copy.deepcopy(packet)
        changed["session"]["events"][0]["menu"]["source_text"] = "An altered source."
        with self.assertRaises(NotationError):
            story.import_contract(changed)

    def test_generative_fold_requires_a_source_and_distinct_options(self):
        session = story.create("Explore ambiguity", ["alice"])
        menu = fold_menu([option("some", "At least one"), option("one", "Exactly one")])
        for replacement in (None, ""):
            altered = copy.deepcopy(menu)
            if replacement is None:
                del altered["source_text"]
            else:
                altered["source_text"] = replacement
            with self.subTest(replacement=replacement), self.assertRaises(NotationError):
                story.propose(session, story.revision(session), altered)
        duplicate = copy.deepcopy(menu)
        duplicate["options"][1]["id"] = duplicate["options"][0]["id"]
        with self.assertRaises(NotationError):
            story.propose(session, story.revision(session), duplicate)

    def test_changed_formal_context_cannot_reuse_a_ballot(self):
        initial = story.create("Choose a clock boundary", ["alice", "bob"])
        choices = [{"id": "wrap", "label": "Wrap", "entry": clock.entry(3, "wrap")},
                   {"id": "stop", "label": "Stop", "entry": clock.entry(3, "stop")}]
        menu = fold_menu(choices)
        proposed = story.propose(initial, story.revision(initial), menu)
        cast = story.vote(proposed, story.revision(proposed), digest(menu), "alice", digest(choices[0]))
        tampered = copy.deepcopy(cast)
        tampered["events"][0]["menu"]["options"][0]["entry"] = clock.entry(4, "wrap")
        with self.assertRaises(NotationError):
            story.replay(tampered)

        replacement = copy.deepcopy(menu)
        replacement["options"][0]["entry"] = clock.entry(4, "wrap")
        fresh = story.propose(initial, story.revision(initial), replacement)
        with self.assertRaises(NotationError):
            story.vote(fresh, story.revision(fresh), digest(replacement), "alice", digest(choices[0]))
        with self.assertRaises(NotationError):
            story.vote(cast, story.revision(proposed), digest(menu), "bob", digest(choices[0]))

    def test_custom_semantic_profile_is_retained_without_a_borrowed_evaluator(self):
        grammar, notation, model, source = custom_recipe()
        context = sharing.make_context(
            grammar, model,
            semantics={"profile": "user/exotic-machine/1", "parameters": {"ordering": "declared"},
                       "description": "A newly invented composition interpretation."},
            assumptions=["The author retains this interpretation for review."],
            observation={"id": "whole", "description": "Retain the full model.",
                         "keeps": ["model"], "omits": []},
        )
        discussion = sharing.make_discussion(context, notation)
        entry = abstractions.make_entry("custom", "Custom machine", story=source, discussion=discussion)
        self.assertEqual(abstractions.inspect_entry(entry)["status"], "model-linked")
        self.assertEqual(entry["discussion"]["context"]["semantics"]["profile"], "user/exotic-machine/1")
        before = copy.deepcopy(entry)
        self.assertEqual(story.execute(entry, {"kind": "invented-operation"})["status"], "unsupported")
        self.assertEqual(entry, before)
        with self.assertRaises(NotationError):
            story.execute(entry, {"kind": "lambda-beta"})

    def test_tool_discovery_matches_dispatch_and_excludes_vote(self):
        advertised = sharing.toolkit()
        names = [item["name"] for item in advertised["tools"]]
        story_names = {item["name"] for item in story.toolkit()["tools"]}
        self.assertEqual(len(names), len(set(names)))
        self.assertTrue(story_names <= set(names))
        self.assertNotIn("zal_sdd_vote", names)
        self.assertEqual(advertised["human_operations"], ["vote"])
        self.assertNotIn("tools-stdio", advertised["transport"])
        created = sharing.dispatch_tool("zal_sdd_create", {"intent": "Explore", "participants": ["alice"]})
        self.assertEqual(story.replay(created)["status"], "exploring")
        for name in story_names:
            with self.subTest(name=name), self.assertRaisesRegex(NotationError, "missing keys"):
                sharing.dispatch_tool(name, {})
        for name, args in (("zal_sdd_vote", {}), ("zal_library_unknown", {}),
                           ("zal_sdd_create", {"intent": "x", "participants": ["alice"], "extra": True})):
            with self.subTest(name=name), self.assertRaises(NotationError):
                sharing.dispatch_tool(name, args)

    def test_drafts_and_library_values_remain_detached_and_bounded(self):
        original = draft("unfinished", "An unfinished machine")
        collection = abstractions.register(abstractions.new_library(), original)
        original["story"] = "Changed after registration"
        retained = collection["entries"][0]
        self.assertEqual(retained["story"], "An unfinished machine")
        self.assertEqual(abstractions.inspect_entry(retained)["status"], "draft")
        self.assertIn("lexical retrieval", abstractions.search(collection, "machine")["scope"])
        with self.assertRaises(NotationError):
            abstractions.require_formal(retained)
        session = story.create("Bounded history", ["alice"])
        session["events"] = [{}] * (story.MAX_EVENTS + 1)
        with self.assertRaisesRegex(NotationError, "event budget"):
            story.replay(session)


if __name__ == "__main__":
    unittest.main()
