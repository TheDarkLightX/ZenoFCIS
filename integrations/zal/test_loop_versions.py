"""Version-space ZAL/workspace tests. CLI calls are mocked here; compiled
checker and real workspace coverage is in the deferred heavy harness.
"""
from pathlib import Path
import json
import sys
import tempfile
import unittest
from unittest.mock import patch

from behavior import Refusal
import loop_doors
import loop_language as Z
import loop_versions as V
from loop_workspace import LoopWorkspace
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools"))
import version_space_reference as O


def specification():
    return {"schema": V.SCHEMA, "inputs": [{"name": "x", "domain": {"kind": "Bool"}}],
            "outputs": [{"kind": "Bool"}], "sketch": ["x ? ?c : ?c"], "requirements": []}


class Language(unittest.TestCase):
    def test_generic_fact_id_boundary_is_identical_in_both_surfaces(self):
        ctx = Z.Context(V._subject(specification()))
        fact = Z.act("fact", "q" + "a" * 63, text="advisory")
        for english in (False, True):
            with self.subTest(english=english):
                self.assertEqual(Z.parse_statement(Z.render_act(fact, ctx, english), ctx, english), fact)
                too_long = Z.act("fact", "q" + "a" * 64, text="advisory")
                with self.assertRaises(Refusal):
                    Z.parse_statement(Z.render_act(too_long, ctx, english), ctx, english)

    def test_version_acts_in_other_workspaces_refuse_in_both_surfaces(self):
        subject = V._subject(specification())
        subject["tag"] = "contract"
        del subject["output_domains"]
        ctx = Z.Context(subject)
        for line, english in (("version-suggest q1: [0];", False),
                              ("Version suggestion q1: [0] (advisory).", True),
                              ("versions next;", False)):
            with self.assertRaises(Refusal):
                Z.parse_statement(line, ctx, english)
        subject["output_domains"] = [{"kind": "Int"}]
        subject["outputs"] = ["Int"]
        with self.assertRaises(Refusal):
            Z.parse_statement("version-answer q1: [0];", Z.Context(subject), False)

    def test_version_hooks_do_not_claim_unrelated_acts_or_prefixes(self):
        ctx = Z.Context(V._subject(specification()))
        fact = Z.act("fact", "info", text="advisory")
        for english in (False, True):
            with patch.object(V, "render_act", side_effect=AssertionError("foreign render hook")):
                line = Z.render_act(fact, ctx, english)
            with patch.object(V, "parse_act", side_effect=AssertionError("foreign parse hook")):
                self.assertEqual(Z.parse_statement(line, ctx, english), fact)
                with self.assertRaises(Refusal):
                    Z.parse_statement("Unrelated p: input." if english else "unrelated p;", ctx, english)

    def test_every_new_act_round_trips_in_both_surfaces(self):
        ctx = Z.Context(V._subject(specification()))
        acts = (Z.act("versions", "p"), Z.act("version-count", "p", count=4),
                Z.act("version-question", "q1", input=(0,), answers=(((0,), 2), ((1,), 2))),
                Z.act("version-suggest", "q1", output=(0,)), Z.act("version-answer", "q1", output=(0,)),
                Z.act("version-determined", "p", sha256="a"*64, domain=2),
                Z.act("version-conflict", "p", requirements=("a", "b")),
                Z.act("version-conflict", "empty", requirements=()),
                Z.act("version-stop", "owner"), Z.act("version-stopped", "owner"))
        for a in acts:
            for english in (False, True):
                line = Z.render_act(a, ctx, english)
                self.assertEqual(Z.parse_statement(line, ctx, english), a, line)
                doc = Z.Document(ctx.subject_ref, "judge", (a,))
                self.assertEqual(Z.parse(Z.render(doc, ctx, english), ctx), doc)

    def test_full_tuple_and_json_bounds(self):
        ctx = Z.Context(V._subject(specification()))
        for line in ("version-answer q1: [2];", "version-answer q1: [];", "version-suggest q1: [true];",
                     "version-question q1 at [0]: [0] -> 1 | [0] -> 1;",
                     "version-answer q1: [" + "9" * 1000 + "];",
                     "version-count q1: " + "9" * 1000 + ";"):
            with self.assertRaises(Refusal):
                Z.parse_statement(line, ctx, False)
        for text in ('{"a":1,"a":2}', '{"a":1.0}', '{"a":NaN}'):
            with self.assertRaises(Refusal):
                V.strict_json(text)


class Workspace(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.given = self.root / "spec.json"
        self.given.write_text(json.dumps(specification()))
        self.calls = []
        self.patches = [patch.object(loop_doors, "resolve_cli", lambda *_: Path("fake-cli")),
                        patch.object(loop_doors, "run", self.fake_run)]
        for p in self.patches:
            p.start()
            self.addCleanup(p.stop)
        self.workspace = LoopWorkspace(self.root / "workspace.json")
        self.workspace.init(specification=self.given)

    def fake_run(self, cli, arguments):
        self.calls.append(arguments)
        s = json.loads(Path(arguments[arguments.index("--specification") + 1]).read_text())
        r = O.expected(s)
        if "--max-work" in arguments and arguments[arguments.index("--max-work") + 1] == "0":
            return 2, {"status": "inconclusive", "detail": {"count": None, "question": None, "candidate": None}}
        if r["count"] == 0:
            return 0, {"status": "no-completion", "detail": {"count": 0, "conflict": {"requirements": r["conflict"]}}}
        if r["count"] == 1:
            return 0, {"status": "determined", "detail": {"count": 1, "candidate": {"status": "checked",
                       "checks": [{"verdict": "equivalent", "replay": "matched"}], "sha256": "a"*64, "domain": 2}}}
        q = r["question"]
        q["id"] = "q" + Z.digest({"specification": s, "partition": q})[:48]
        return 0, {"status": "question", "detail": {"count": r["count"], "question": q}}

    def doc(self, statement, author="model"):
        ctx = Z.Context(self.workspace.load()["subject"])
        return f"zal fcis loop 1;\n{Z.subject_line(ctx.subject_ref, False)}\nauthor {author};\n{statement}\n"

    def question(self):
        return self.workspace.view()["open_questions"][0]["id"]

    def test_suggestion_generic_fact_round_trips_in_both_surfaces(self):
        question_id = self.question()
        self.assertEqual(len(question_id), 49)
        self.workspace.submit(self.doc(f"version-suggest {question_id}: [0];"), "model")
        state = self.workspace.load()
        fact = next(event for event in reversed(state["events"]) if event["act"]["kind"] == "fact")
        self.assertEqual(fact["act"]["id"], question_id)
        ctx = Z.Context(state["subject"])
        symbolic = Z.parse_statement(fact["symbolic"], ctx, False)
        english = Z.parse_statement(fact["english"], ctx, True)
        self.assertEqual(symbolic, english)
        self.assertEqual(symbolic.data(), fact["act"])

    def test_owner_recovers_and_transcript_surfaces_round_trip(self):
        self.workspace.answer(self.question(), "[0]")
        self.workspace.answer(self.question(), "[1]")
        state = self.workspace.load()
        self.assertEqual(len(state["version_space"]["specification"]["requirements"]), 2)
        self.assertFalse(self.workspace.view()["open_questions"])
        self.assertTrue(any(e["act"]["kind"] == "version-determined" for e in state["events"]))
        ctx = Z.Context(state["subject"])
        for event in state["events"]:
            a = Z.parse_statement(event["symbolic"], ctx, False)
            b = Z.parse_statement(event["english"], ctx, True)
            self.assertEqual(a, b)

    def test_no_model_answer_stop_or_forged_judge_path(self):
        before = self.workspace.load()["version_space"]["specification"]
        for statement, author in ((f"version-answer {self.question()}: [0];", "model"),
                                  ("version-stop owner;", "model"),
                                  (f"version-answer {self.question()}: [0];", "owner"),
                                  ("version-count fake: 1;", "model")):
            with self.assertRaises(Refusal):
                self.workspace.submit(self.doc(statement, author), "model")
            self.assertEqual(self.workspace.load()["version_space"]["specification"], before)

    def test_suggestions_stale_ids_and_owner_stop(self):
        q = self.question()
        before = self.workspace.load()["version_space"]["specification"]
        self.workspace.submit(self.doc(f"version-suggest {q}: [0];"), "model")
        self.assertEqual(self.workspace.load()["version_space"]["specification"], before)
        self.workspace.answer(q, "1")
        confirmed = self.workspace.load()["version_space"]["specification"]
        with self.assertRaises(Refusal):
            self.workspace.answer(q, "1")
        self.workspace.stop_versions()
        after = self.workspace.load()
        self.assertTrue(after["version_space"]["stopped"])
        self.assertEqual(after["version_space"]["specification"], confirmed)
        self.assertFalse(self.workspace.view()["open_questions"])
        self.assertIn("requirements-only", self.workspace.packet())

    def test_budget_stop_does_not_add_a_requirement(self):
        with self.workspace.locked():
            state = self.workspace.load()
            state["version_space"]["limits"]["max_work"] = 0
            self.workspace.save(state)
        self.workspace.answer(self.question(), "[0]")
        self.assertEqual(self.workspace.load()["version_space"]["specification"]["requirements"], [])
        self.assertIn("inconclusive", self.workspace.load()["events"][-1]["english"])

    def test_initial_spec_is_owned_and_source_edits_do_not_change_it(self):
        before = self.workspace.load()["version_space"]["specification"]
        self.given.write_text('{}')
        self.workspace.submit(self.doc("versions again;"), "model")
        self.assertEqual(self.workspace.load()["version_space"]["specification"], before)

    def test_malformed_initialization_refuses_without_blocking_retry(self):
        for index, text in enumerate((json.dumps({"schema": V.SCHEMA, "inputs": 5}),
                                      json.dumps({"schema": V.SCHEMA, "inputs": [{"name": "x"}]}),
                                      '{"schema":"' + V.SCHEMA + '","inputs":"é"}')):
            with self.subTest(index=index):
                given = self.root / f"invalid-{index}.json"
                given.write_text(text, encoding="utf-8")
                workspace = LoopWorkspace(self.root / f"retry-{index}.json")
                with self.assertRaises(Refusal):
                    workspace.init(specification=given)
                self.assertFalse(workspace.data.exists())
                self.assertFalse(workspace.path.exists())
                self.assertFalse(workspace.view_path.exists())
                given.write_text(json.dumps(specification()))
                workspace.init(specification=given)
                self.assertTrue(workspace.path.is_file())

    def test_checker_refusal_or_transport_failure_leaves_initialization_retryable(self):
        for index, response in enumerate(((1, {"status": "refused", "detail": {"reason": "invalid"}}),
                                           Refusal("transport did not finish"))):
            workspace = LoopWorkspace(self.root / f"checker-retry-{index}.json")
            with patch.object(loop_doors, "run", side_effect=response if isinstance(response, Exception) else None,
                              return_value=response), self.assertRaises(Refusal):
                workspace.init(specification=self.given)
            self.assertFalse(workspace.data.exists())
            self.assertFalse(workspace.path.exists())
            self.assertFalse(workspace.view_path.exists())
            workspace.init(specification=self.given)
            self.assertTrue(workspace.path.is_file())


if __name__ == "__main__":
    unittest.main()
