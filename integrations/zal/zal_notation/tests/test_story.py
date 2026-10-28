"""Story decisions, abstraction reuse, normalization and adversarial replay."""
import copy
import itertools
import unittest

from zal_notation import abstractions as lib
from zal_notation import clock, normalizer, story
from zal_notation.builtin import generic_pack, get_builtin
from zal_notation.core import NotationError, canonical, digest, term_model
from zal_notation.semantics import lambda_trace
from zal_notation.sharing import dispatch_tool, make_context, make_discussion
from zal_notation.syntax import parse_text


def draft(label="first"):
    return lib.make_entry(label, label, story="A person describes a machine.",
        requirements=[{"id": "purpose", "statement": "The machine keeps time.", "anchors": []}],
        open_questions=["What supplies a tick?"])


def menu(session, identifier="choice", options=None):
    selected = story.replay(session)["selected"]
    return {"id": identifier, "base": digest(selected) if selected else None, "scene": "A consequential situation occurs.",
            "question": "Which rule should the system follow?", "options": options or [
                {"id": "a", "label": "First interpretation", "entry": draft("a")},
                {"id": "b", "label": "Second interpretation", "entry": draft("b")}]}


def ballot(session, who, index=0):
    pending = story.replay(session)["pending"]
    return story.vote(session, story.revision(session), digest(pending), who, digest(pending["options"][index]))


def lambda_entry(source, identifier="lambda"):
    g, p, _ = get_builtin("lambda")
    context = make_context(g, term_model(g, parse_text(source, g)),
        semantics={"profile": "zal/lambda-normal-order/1", "parameters": {}, "description": "Closed lambda reduction."},
        assumptions=[], observation={"id": "term", "description": "Observe the complete term.", "keeps": ["term"], "omits": []})
    return lib.make_entry(identifier, identifier, story="A transformation has a precise lambda definition.", discussion=make_discussion(context, p))


class StoryTests(unittest.TestCase):
    def test_unanimous_choice_and_exact_export(self):
        original = story.create("We want a clock.", ["Alice", "Bob"])
        proposed = story.propose(original, story.revision(original), menu(original))
        one = ballot(proposed, "Alice")
        self.assertIsNone(story.replay(one)["selected"])
        selected = ballot(one, "Bob")
        self.assertEqual(story.replay(selected)["selected"], draft("a"))
        self.assertEqual(story.import_contract(story.export_contract(selected)), selected)
        self.assertEqual(original["events"], [])

    def test_import_refuses_type_only_packet_changes(self):
        initial = story.create("Choose a clock boundary", ["A"])
        options = [{"id": mode, "label": mode, "entry": clock.entry(2, mode)}
                   for mode in ("wrap", "stop")]
        selected = ballot(story.propose(initial, story.revision(initial), menu(initial, options=options)), "A")
        packet = story.export_contract(selected)
        self.assertEqual(canonical(story.import_contract(packet)), canonical(selected))
        self.assertEqual(packet["decisions"][0]["event_index"], 1)

        changes = (
            lambda p: p["decisions"][0].update(event_index=True),
            lambda p: p["decisions"][0].update(event_index=1.0),
            lambda p: p["selected"]["discussion"]["context"]["grammar"].update(version=True),
        )
        for change in changes:
            altered = copy.deepcopy(packet)
            change(altered)
            # Python's container equality conflates these values; JSON does not.
            self.assertEqual(packet, altered)
            self.assertNotEqual(canonical(packet), canonical(altered))
            with self.assertRaises(NotationError):
                story.import_contract(altered)

        for invalid in (float("nan"), float("inf"), object()):
            altered = copy.deepcopy(packet)
            altered["decisions"][0]["event_index"] = invalid
            with self.assertRaises(NotationError):
                story.import_contract(altered)

    def test_conflict_revote_and_closure(self):
        s = story.create("Build a clock", ["A", "B"])
        s = story.propose(s, story.revision(s), menu(s))
        s = ballot(ballot(s, "A", 0), "B", 1)
        self.assertIsNone(story.replay(s)["selected"])
        pending = story.replay(s)["pending"]
        s = ballot(s, "B", 0)
        with self.assertRaises(NotationError):
            story.vote(s, story.revision(s), digest(pending), "A", digest(pending["options"][0]))

    def test_stale_and_unauthorized_ballots(self):
        s = clock.story_demo(["A", "B"])
        p = story.replay(s)["pending"]
        cases = [("bad", "A", digest(p["options"][0])),
                 (story.revision(s), "outsider", digest(p["options"][0])),
                 (story.revision(s), "A", "0" * 64)]
        for rev, who, option in cases:
            with self.subTest(who=who, option=option), self.assertRaises(NotationError):
                story.vote(s, rev, digest(p), who, option)

    def test_changed_genesis_or_option_cannot_reuse_votes(self):
        s = ballot(clock.story_demo(["A", "B"]), "A")
        for mutate in [lambda x: x.update(rule="majority"), lambda x: x["participants"].append("C"),
                       lambda x: x["events"][0]["menu"]["options"][0].update(label="Changed promise")]:
            changed = copy.deepcopy(s); mutate(changed)
            with self.assertRaises(NotationError): story.replay(changed)

    def test_withdrawal_retains_history_but_not_ballots(self):
        s = ballot(clock.story_demo(["A", "B"]), "A")
        p = story.replay(s)["pending"]
        s = story.withdraw(s, story.revision(s), digest(p), "Add a third interpretation")
        replacement = menu(s, "replacement", p["options"])
        s = story.propose(s, story.revision(s), replacement)
        self.assertEqual(story.replay(s)["ballots"], {})
        self.assertEqual(len(story.replay(s)["withdrawals"][0]["ballots"]), 1)
        s = ballot(s, "B")
        self.assertIsNone(story.replay(s)["selected"])

    def test_draft_then_formal_meaning_preserves_provenance(self):
        s = story.create("We want a clock", ["A"])
        s = story.propose(s, story.revision(s), menu(s))
        s = ballot(s, "A")
        with self.assertRaises(NotationError): lib.require_formal(story.replay(s)["selected"])
        old = story.replay(s)["selected"]
        options = [{"id": mode, "label": mode, "entry": clock.entry(60, mode)} for mode in ("wrap", "stop")]
        s = story.propose(s, story.revision(s), menu(s, "formalize", options))
        s = ballot(s, "A")
        now = story.replay(s)
        self.assertEqual(len(now["decisions"]), 2)
        self.assertEqual(now["selected_status"]["status"], "model-linked")
        self.assertTrue(story.requirement_diff(old, now["selected"])["removed"])
        packet = story.export_contract(s); packet["selected"]["story"] = "Changed story"
        with self.assertRaises(NotationError): story.import_contract(packet)

    def test_finite_vote_oracle(self):
        # Independent expected policy over all complete three-person assignments,
        # including abstention, compared with every prefix that can be recorded.
        for rule, assignment in itertools.product(("unanimous", "majority"), itertools.product((None, 0, 1), repeat=3)):
            s = story.create("Choose a rule", ["A", "B", "C"], rule=rule)
            s = story.propose(s, story.revision(s), menu(s))
            for who, option in zip(s["participants"], assignment):
                if option is not None and story.replay(s)["pending"]:
                    s = ballot(s, who, option)
            if rule == "unanimous":
                expected = assignment[0] if assignment[0] is not None and len(set(assignment)) == 1 else None
            else:
                expected = 0 if assignment.count(0) >= 2 else 1 if assignment.count(1) >= 2 else None
            selected = story.replay(s)["selected"]
            self.assertEqual(selected["id"] if selected else None, None if expected is None else ("a", "b")[expected])

    def test_no_vote_tool_and_no_mutable_result_alias(self):
        s = clock.story_demo(["A"])
        report = story.replay(s)
        report["pending"]["scene"] = "Mutation"
        self.assertNotEqual(story.replay(s)["pending"]["scene"], "Mutation")
        with self.assertRaises(NotationError): story.dispatch("zal_sdd_vote", {})
        for voters in ([], [""], ["A", "A"], 4):
            with self.assertRaises(NotationError): story.create("Intent", voters)


class AbstractionTests(unittest.TestCase):
    def test_custom_grammar_through_proposal_and_reuse(self):
        from zal_notation.examples import custom_recipe
        from zal_notation.render import render_svg
        g, p, m, _ = custom_recipe()
        g["id"] = "user/new-process-language"
        p["grammar"]["id"] = m["grammar"]["id"] = g["id"]
        context = make_context(g, m, semantics={"profile": "user/process/1", "parameters": {}, "description": "A proposed interpretation."},
            assumptions=[], observation={"id": "whole", "description": "Retain the whole construction.", "keeps": ["model"], "omits": []})
        entry = lib.make_entry("new-process", "New process", story="A new story vocabulary.", discussion=make_discussion(context, p))
        s = story.create("Develop a new abstraction", ["A"])
        proposal = menu(s, options=[{"id": "new", "label": "New language", "entry": entry}, {"id": "draft", "label": "Keep exploring", "entry": draft()}])
        s = dispatch_tool("zal_sdd_propose", {"session": s, "expected_revision": story.revision(s), "menu": proposal})
        selected = story.replay(ballot(s, "A"))["selected"]
        library = dispatch_tool("zal_library_register", {"library": lib.new_library(), "entry": selected})
        self.assertEqual(library["entries"][0], entry)
        self.assertIn("<svg", render_svg(g, m, p))
        self.assertEqual(story.execute(entry, {"kind": "unknown"})["status"], "unsupported")

    def test_composition_and_fold_retains_definition(self):
        first, second = lambda_entry(r"\a.a", "first"), lambda_entry(r"\b.b", "second")
        combined = lib.compose_lambda(first, second)
        b = combined["discussion"]
        result = lambda_trace(b["context"]["grammar"], b["context"]["model"], max_steps=20)
        self.assertEqual(result["status"], "normal-form")
        self.assertEqual(result["models"][-1]["body"], parse_text(r"\x0.x0", b["context"]["grammar"]))
        self.assertEqual(combined["dependencies"][0]["entry_digest"], digest(first))
        from zal_notation.exchange import recover_source
        from zal_notation.render import render_svg
        svg = render_svg(b["context"]["grammar"], b["context"]["model"], generic_pack(b["context"]["grammar"]), folded=("root",))
        self.assertEqual(recover_source(svg)["model"], b["context"]["model"])

    def test_collection_graph_search_and_selective_sharing(self):
        a, b = draft("clock"), lambda_entry(r"\x.x", "identity")
        library = lib.register(lib.register(lib.new_library(), a), b)
        library = lib.relate(library, digest(a), digest(b), "analogy", "A declared relationship", ["Discuss structure only."])
        diagram = lib.library_diagram(library)
        self.assertIn("declared", diagram["svg"])
        self.assertEqual(len(diagram["nodes"]), 2)
        self.assertEqual(len(lib.select(library, [digest(a)])["relations"]), 0)
        self.assertEqual(lib.search(library, "clock")["matches"][0]["entry_digest"], digest(a))
        library["relations"][0]["status"] = "proved"
        with self.assertRaises(NotationError): lib.validate_library(library)


class NormalizerClockTests(unittest.TestCase):
    def test_alpha_binding_shadowing_and_idempotence(self):
        for left, right in [(r"\x.x", r"\y.y"), (r"\x.\x.x", r"\a.\b.b")]:
            a = normalizer.alpha_normalize(lambda_entry(left))
            b = normalizer.alpha_normalize(lambda_entry(right))
            self.assertEqual(a["binding_digest"], b["binding_digest"])
            self.assertEqual(a["result"], normalizer.alpha_normalize(a["result"])["result"])
        a = normalizer.alpha_normalize(lambda_entry(r"\x.\y.x"))
        b = normalizer.alpha_normalize(lambda_entry(r"\x.\y.y"))
        self.assertNotEqual(a["binding_digest"], b["binding_digest"])
        self.assertEqual(a["source"]["discussion"]["context"]["model"]["body"], parse_text(r"\x.\y.x", get_builtin("lambda")[0]))
        with self.assertRaises(NotationError): normalizer.alpha_normalize(lambda_entry("x"))
        with self.assertRaises(NotationError): normalizer.alpha_normalize(clock.entry())

    def test_canonical_json_retains_data(self):
        result = normalizer.canonical_json(' {"b": [2, 1], "a": 3} ')
        self.assertEqual(result["result"], '{"a":3,"b":[2,1]}')
        self.assertEqual(normalizer.canonical_json(result["result"])["result"], result["result"])
        with self.assertRaises(NotationError): normalizer.canonical_json('{"a":1,"a":2}')

    def test_clock_domain_boundary_and_trace_separation(self):
        for boundary in ("wrap", "stop"):
            entry = clock.entry(60, boundary); context = entry["discussion"]["context"]
            for position in range(60):
                next_value = clock.step(context, "running", position, "tick")
                self.assertTrue(next_value["accepted"])
                self.assertEqual(next_value["position"], position + 1 if position < 59 else 0 if boundary == "wrap" else 59)
                self.assertEqual(next_value["state"], "stopped" if position == 59 and boundary == "stop" else "running")
                self.assertEqual(clock.step(context, "stopped", position, "tick"),
                    {"accepted": False, "reason": "not_enabled", "state": "stopped", "position": position})
            self.assertIn("0..=59", clock.native_source(context))
            self.assertEqual(story.execute(entry, {"kind": "clock-transition", "state": "running", "position": 59, "event": "tick"})["status"], "executed")
        changed = copy.deepcopy(context); changed["model"]["body"]["children"][0]["fields"]["definition"] = "Skip a tick"
        with self.assertRaises(NotationError): clock.step(changed, "running", 0, "tick")


if __name__ == "__main__": unittest.main()
