"""Pure shell tests with recorded CLI outcomes; no evaluator or live binary."""
from __future__ import annotations

import copy
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from behavior import Refusal
import loop_language as Z
import loop_learn as L
import loop_workspace as W
import mcp


def declaration():
    return {"schema": L.SCHEMA, "inputs": [{"name": "x", "domain": {"kind": "Int", "min": 0, "max": 2}}],
            "outputs": [{"kind": "Int", "min": -10, "max": 10}], "max_size": 1,
            "vocabulary": {"constants": [0], "arithmetic": []}, "examples": []}


def recorded(status="question"):
    detail = {"enumeration": "proposal", "complete": True, "counts": [2], "attempts_spent": 0,
              "question": None, "core": None, "candidate": None}
    if status == "question":
        detail["question"] = {"input": [1], "ordinal": 1, "worst": "1", "squares": "2",
                              "buckets": [[{"observation": {"value": 0}, "remaining": 1},
                                           {"observation": {"value": 1}, "remaining": 1}]]}
    elif status == "determined":
        detail["counts"] = [1]
        detail["candidate"] = {"status": "examples-reproduced", "sha256": "ab" * 32, "program": {},
                               "examples_check": {"decided_by": "library-evaluator", "verdict": "reproduced",
                                                  "checked": 1, "vacuous": False}}
    elif status == "outside-vocabulary":
        detail.update(counts=[0], core=[0])
    elif status == "inconclusive":
        detail.update(complete=False, counts=None, reason="learn-work-budget")
    return {"schema": "zeno-fcis/loop-doors/1", "door": "learn", "status": status, "authority": "none", "detail": detail}


class Surfaces(unittest.TestCase):
    def setUp(self):
        self.ctx = Z.Context(L.subject(declaration()))

    def test_learn_terminal_initialization_refuses_other_subjects_and_version_budgets_before_io(self):
        import io
        for extra in (("--specification", "other.json"), ("--contract", "other"),
                      ("--family", "other.json"), ("--max-work", "10")):
            with self.subTest(extra=extra), patch.object(W.LoopWorkspace, "init_learn", side_effect=AssertionError("initialization ran")), \
                    patch("sys.stderr", io.StringIO()):
                self.assertEqual(W.terminal(["init", "--learn", "learn.json", "--workspace", "unused.json", *extra]), 2)

    def test_every_learn_act_round_trips_in_both_surfaces_and_full_documents(self):
        acts = [Z.act("learn", "go"), Z.act("learn-suggest", "q1", output=(-3,)),
                Z.act("learn-answer", "q1", output=(2,)),
                Z.act("learn-question", "q1", input=(1,), counts=(2,)),
                Z.act("learn-determined", "go", sha256="ab" * 32, examples=1),
                Z.act("learn-outside", "go", examples=(0, 3)),
                Z.act("learn-ambiguous", "go", counts=(2,)), Z.act("learn-inconclusive", "go")]
        for a in acts:
            author = "judge" if a.kind in L.JUDGE_KINDS else "owner" if a.kind in L.OWNER_KINDS else "model"
            d = Z.Document(self.ctx.subject_ref, author, (a,))
            for english in (False, True):
                with self.subTest(kind=a.kind, english=english):
                    self.assertEqual(Z.parse(Z.render(d, self.ctx, english), self.ctx), d)

    def test_boolean_integer_aliases_out_of_domain_unknown_fields_and_deep_json_refuse(self):
        for text in ("learn-answer q1: [true];", "learn-answer q1: [1.0];", "learn-answer q1: [11];",
                     "learn-answer q1: [1,2];", "learn-question q1: [3] remaining [2];",
                     "learn-question q1: [1] remaining [0];", "learn-question q1: [1] remaining [4097];",
                     "learn-outside q1: [1,1];", "learn-outside q1: [64];", "learn go extra;",
                     "learn-answer q1: " + "[" * 1200 + "0" + "]" * 1200 + ";"):
            with self.subTest(text=text[:50]):
                with self.assertRaises(Refusal):
                    Z.parse_statement(text, self.ctx, False)
        with self.assertRaises(Refusal):
            L.render_act(Z.act("learn", "go", unknown=True), self.ctx, False)
        with self.assertRaises(Refusal):
            Z.parse_statement("learn " + "x" * 9000 + ";", self.ctx, False)


class Workspace(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="zal-learn-test-")
        self.root = Path(self.tmp.name)
        self.cli = self.root / "zeno-fcis"
        self.cli.write_text("recorded only\n")
        self.cli.chmod(0o755)
        self.spec = self.root / "declaration.json"
        self.spec.write_text(json.dumps(declaration()))
        self.workspace = W.LoopWorkspace(self.root / "workspace.json", self.cli)
        self.responses = [recorded()]

        def run(cli, args, **_):
            self.assertEqual(cli, self.cli)
            self.assertEqual(args[:3], ["loop", "learn", "--spec"])
            report = copy.deepcopy(self.responses.pop(0))
            report["detail"]["specification_sha256"] = hashlib.sha256(Path(args[3]).read_bytes()).hexdigest()
            code = {"question": 3, "determined": 0, "outside-vocabulary": 1, "inconclusive": 2}[report["status"]]
            return code, report

        self.runner = patch.object(L.doors, "run", side_effect=run)
        self.runner.start()
        self.workspace.init_learn(self.spec)

    def tearDown(self):
        self.runner.stop()
        self.tmp.cleanup()

    def doc(self, acts, author="model"):
        ctx = Z.Context(self.workspace.load()["subject"])
        return Z.render(Z.Document(ctx.subject_ref, author, tuple(acts)), ctx)

    def test_model_suggestions_do_not_answer_and_owner_answers_through_existing_gate(self):
        self.workspace.submit(self.doc([Z.act("learn-suggest", "q1", output=(1,))]), "model")
        state = self.workspace.load()
        self.assertEqual(state["learn"]["confirmed"], [])
        self.assertEqual(state["questions"]["q1"]["status"], "open")
        with self.assertRaises(Refusal):
            self.workspace.submit(self.doc([Z.act("learn-answer", "q1", output=(1,))]), "model")
        with self.assertRaises(Refusal):
            self.workspace.submit(self.doc([Z.act("learn-answer", "q1", output=(1,))], "owner"), "model")
        self.responses.append(recorded("determined"))
        self.workspace.answer("q1", "[1]")
        state = self.workspace.load()
        self.assertEqual(state["learn"]["confirmed"], [{"input": [1], "output": [1]}])
        self.assertEqual(state["questions"]["q1"]["status"], "answered")
        self.assertEqual(state["events"][-1]["act"]["kind"], "learn-determined")
        self.assertIsNone(self.workspace.view()["incumbent"])
        self.assertIn("uniqueness is a vocabulary proposal", state["events"][-1]["english"])

    def test_answer_outside_all_buckets_is_retained_and_reports_outside_vocabulary(self):
        self.responses.append(recorded("outside-vocabulary"))
        self.workspace.answer("q1", "[7]")
        state = self.workspace.load()
        self.assertEqual(state["learn"]["confirmed"], [{"input": [1], "output": [7]}])
        self.assertEqual(state["events"][-1]["act"]["kind"], "learn-outside")

    def test_budget_stop_retains_owner_answer_and_has_no_cached_count_or_candidate(self):
        self.responses.append(recorded("inconclusive"))
        self.workspace.answer("q1", "[1]")
        state = self.workspace.load()
        self.assertEqual(state["learn"]["confirmed"], [{"input": [1], "output": [1]}])
        self.assertEqual(self.workspace.view()["open_questions"], [])
        self.assertEqual(state["events"][-1]["act"]["kind"], "learn-inconclusive")
        self.assertEqual(state["events"][-1]["detail"]["detail"]["counts"], None)
        self.assertEqual(state["events"][-1]["detail"]["detail"]["attempts_spent"], 0)

    def test_supervisor_stop_without_detail_retains_the_owner_confirmation(self):
        stopped = {"status": "inconclusive", "authority": "none",
                   "reason": "containment-unavailable:systemd-user-manager"}
        with patch.object(L.doors, "run", return_value=(2, stopped)):
            self.workspace.answer("q1", "[1]")
        state = self.workspace.load()
        self.assertEqual(state["learn"]["confirmed"], [{"input": [1], "output": [1]}])
        self.assertEqual(state["questions"]["q1"]["status"], "answered")
        self.assertEqual(state["events"][-1]["act"]["kind"], "learn-inconclusive")
        report = state["events"][-1]["detail"]
        self.assertEqual(report["detail"]["reason"], stopped["reason"])
        self.assertTrue(all(report["detail"].get(k) is None for k in ("counts", "question", "candidate", "core")))
        self.assertNotIn("usage", report["detail"])

    def test_interrupted_followup_cannot_erase_a_validated_owner_confirmation(self):
        with patch.object(L.doors, "run", side_effect=KeyboardInterrupt), self.assertRaises(KeyboardInterrupt):
            self.workspace.answer("q1", "[1]")
        state = self.workspace.load()
        self.assertEqual(state["learn"]["confirmed"], [{"input": [1], "output": [1]}])
        self.assertEqual(state["questions"]["q1"]["status"], "answered")

    def test_reanalysis_uses_fresh_ids_and_stale_questions_cannot_confirm(self):
        self.responses.append(recorded())
        self.workspace.submit(self.doc([Z.act("learn", "again")]), "model")
        state = self.workspace.load()
        self.assertEqual(state["questions"]["q1"]["status"], "superseded")
        self.assertEqual(state["questions"]["q2"]["status"], "open")
        with self.assertRaises(Refusal):
            self.workspace.answer("q1", "[1]")
        self.assertEqual(self.workspace.load()["learn"]["confirmed"], [])

    def test_judge_learn_acts_are_refused_in_learn_and_ordinary_workspaces(self):
        acts = [Z.act("learn-question", "forged", input=(1,), counts=(2,)),
                Z.act("learn-determined", "forged", sha256="ab" * 32, examples=0),
                Z.act("learn-outside", "forged", examples=()), Z.act("learn-ambiguous", "forged", counts=(2,)),
                Z.act("learn-inconclusive", "forged")]
        ctx = Z.Context(self.workspace.load()["subject"])
        for a in acts:
            with self.assertRaises(Refusal):
                self.workspace.submit(self.doc([a]), "model")
            ordinary = self.workspace.load()
            ordinary.pop("learn")
            with self.assertRaises(Refusal):
                self.workspace._submit(ordinary, ctx, Z.Document(ctx.subject_ref, "model", (a,)))

    def test_source_edits_do_not_rebind_initial_vocabulary_and_snapshot_drift_refuses(self):
        self.spec.write_text("changed original file")
        self.assertEqual(self.workspace.load()["learn"]["initial"], declaration())
        state = self.workspace.load()
        state["learn"]["initial"]["vocabulary"]["constants"] = [7]
        with self.assertRaises(Refusal):
            L.validate_snapshot(state)

    def test_view_and_subject_do_not_expose_aliases_to_the_preparation_state(self):
        state = self.workspace.load()
        view = self.workspace.view(state)
        view["open_questions"][0]["input"][0] = 2
        view["open_questions"][0]["buckets"][0][0]["remaining"] = 99
        self.assertEqual(state["questions"]["q1"]["input"], [1])
        self.assertEqual(state["questions"]["q1"]["proposal"]["buckets"][0][0]["remaining"], 1)
        spec = declaration()
        desc = L.subject(spec)
        desc["output_domains"][0]["max"] = 0
        self.assertEqual(spec["outputs"][0]["max"], 10)

    def test_every_transcript_act_round_trips_and_mcp_has_no_owner_route(self):
        self.workspace.submit(self.doc([Z.act("learn-suggest", "q1", output=(1,))]), "model")
        state = self.workspace.load()
        ctx = Z.Context(state["subject"])
        for event in state["events"]:
            a = Z.parse_statement(event["symbolic"], ctx, False)
            self.assertEqual(Z.parse_statement(event["english"], ctx, True), a)
        names = [t["name"] for t in mcp.TOOLS]
        self.assertFalse(any("answer" in n or "learn" in n or "say" in n for n in names))
        self.assertIn("learn next;", self.workspace.packet())

    def test_invalid_initialization_and_unchecked_determined_report_preserve_retry(self):
        second = W.LoopWorkspace(self.root / "second.json", self.cli)
        malformed = self.root / "invalid.json"
        malformed.write_text('{"schema":"wrong"}')
        with self.assertRaises(Refusal):
            second.init_learn(malformed)
        self.assertFalse(second.path.exists())
        self.assertFalse(second.data.exists())
        bad = recorded("determined")
        bad["detail"]["candidate"]["examples_check"]["verdict"] = "mismatch"
        self.responses.append(bad)
        self.spec.write_text(json.dumps(declaration()))
        with self.assertRaises(Refusal):
            second.init_learn(self.spec)
        self.assertFalse(second.path.exists())
        self.assertFalse(second.data.exists())


if __name__ == "__main__":
    unittest.main()
