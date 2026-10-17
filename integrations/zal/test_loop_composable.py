"""Both surfaces and shell routing; Rust calls are explicitly mocked here."""
from __future__ import annotations

from copy import deepcopy
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from behavior import Refusal
import loop_composable as C
import loop_doors as D
import loop_language as Z
import loop_workspace as W
from test_loop_language import COUNTER_SUBJECT, FAMILY_SUBJECT


class Language(unittest.TestCase):
    def test_composable_acts_round_trip_in_both_surfaces(self):
        for subject, expression in [(COUNTER_SUBJECT, "value + ?c[0..1]"), (FAMILY_SUBJECT, "$C")]:
            ctx = Z.Context(subject)
            acts = (Z.act("pipeline", "p", output="out0", expr=Z.read(expression, ctx, False)),
                    Z.act("stages", "p", stages=("repair", "complete", "canonical", "resubstitute")),
                    Z.act("stages", "empty", stages=()),
                    Z.act("strategy", "s", output="out0", expr=("keep",)),
                    Z.act("explore", "s", depth=1),
                    Z.act("mine", "r", output="out1", expr=("int", 0)))
            doc = Z.Document(ctx.subject_ref, "model", acts)
            for english in (False, True):
                self.assertEqual(Z.parse(Z.render(doc, ctx, english), ctx), doc)

    def test_grammar_concrete_term_and_unknown_output_refusals(self):
        ctx = Z.Context(COUNTER_SUBJECT)
        for line in ["stages p: weaken-check;", "stages p: repair, repair, repair, repair, repair, repair;",
                     "explore p: depth 4;", "explore p: depth 2;", "explore p: depth 3;", "mine r out0 = ?c;", "mine r out0 = keep;",
                     "pipeline p out99 = 0;"]:
            with self.assertRaises(Refusal, msg=line):
                Z.parse_statement(line, ctx, False)


    def test_shared_parse_hook_never_claims_another_family_prefix(self):
        import loop_composable_language as language
        ctx = Z.Context(COUNTER_SUBJECT)
        with patch.object(language, "parse", side_effect=AssertionError("foreign prefix intercepted")):
            self.assertEqual(Z.parse_statement("program p out0 = keep;", ctx, False).kind, "program")
            self.assertEqual(Z.parse_statement(Z.render_act(Z.act("program", "p", output="out0", expr=("keep",)), ctx, True), ctx, True).kind, "program")
            for line, english in [("version-suggest q1: [0];", False), ("capture c1 out0 = keep;", False),
                                  ("Version suggestion q1 is zero.", True)]:
                with self.assertRaises(Refusal):
                    Z.parse_statement(line, ctx, english)

    def test_shared_render_hook_never_claims_another_family_kind(self):
        import loop_composable_language as language
        ctx = Z.Context(COUNTER_SUBJECT)
        with patch.object(language, "render", side_effect=AssertionError("foreign kind intercepted")):
            for english in (False, True):
                rendered = Z.render_act(Z.act("program", "p", output="out0", expr=("keep",)), ctx, english)
                self.assertTrue(rendered)
                version = Z.render_act(Z.act("version-suggest", "q1", output=(0,)), ctx, english)
                self.assertEqual(version, "Version suggestion q1: [0] (advisory)." if english else
                                 "version-suggest q1: [0];")



class Shell(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.workspace = W.LoopWorkspace(Path(self.temporary.name) / "workspace.json")
        self.workspace.data.mkdir()
        self.original = self.workspace.data / "original.zcve"
        self.original.write_bytes(b"owned program snapshot")
        subject = deepcopy(COUNTER_SUBJECT)
        subject["original_sha256"] = hashlib.sha256(self.original.read_bytes()).hexdigest()
        self.state = {"subject": subject, "original_program": "original.zcve", "events": [],
                      "next_document": 1, "incumbent": {"unchanged": True}, "questions": {}, "family": None}
        self.ctx = Z.Context(subject)
        self.calls = []
        self.patches = [patch.object(self.workspace, "_contract", return_value=self.workspace.data),
                        patch.object(self.workspace, "cli", return_value=Path("test-cli")),
                        patch.object(D, "run", side_effect=self.run_cli)]
        for p in self.patches:
            p.start()
        self.reply = {"door": "pipeline", "status": "qualified", "authority": "none", "detail": {
            "best_check": {"verdict": "equivalent", "replay": "matched", "domain_tuples": 36},
            "best": {"nodes": 17}, "binding": {"original_sha256": subject["original_sha256"]}}}

    def tearDown(self):
        for p in self.patches:
            p.stop()
        self.temporary.cleanup()

    def run_cli(self, cli, arguments):
        self.calls.append(arguments)
        if "--packet" in arguments:
            return 0, {"status": "packet", "detail": {"original_sha256": self.state["subject"]["original_sha256"],
                                                        "implementation_sha256": "a" * 64}}
        return 0, deepcopy(self.reply)

    def doc(self, body: str, author="model"):
        prefix = Z.render(Z.Document(self.ctx.subject_ref, author, ()), self.ctx)
        return Z.parse(prefix + body, self.ctx)

    def test_pipeline_routes_without_spending_session_attempt_or_replacing_incumbent(self):
        events = self.workspace._submit(self.state, self.ctx, self.doc("pipeline p out0 = keep;\nstages p: canonical;\n"))
        self.assertIn("accepted", [e["act"]["kind"] for e in events])
        self.assertEqual(self.state["incumbent"], {"unchanged": True})
        self.assertEqual(len(self.calls), 2)
        request = json.loads(Path(self.calls[-1][-1]).read_text())
        self.assertEqual(request["stages"], ["canonical"])
        self.assertEqual(request["original_sha256"], self.state["subject"]["original_sha256"])
        for event in events:
            for english, key in [(False, "symbolic"), (True, "english")]:
                self.assertEqual(Z.parse_statement(event[key], self.ctx, english).data(), event["act"])

    def test_model_answer_and_owner_pipeline_are_refused_before_routing(self):
        for body, author in [("pipeline p out0 = keep;\nanswer q: any;\n", "model"),
                             ("pipeline p out0 = keep;\n", "owner")]:
            with self.assertRaises(Refusal):
                self.workspace._submit(self.state, self.ctx, self.doc(body, author))
        self.assertEqual(self.calls, [])
        self.assertEqual(self.state["events"], [])

    def test_a_missing_replay_never_yields_an_accepted_act(self):
        self.reply["detail"]["best_check"]["replay"] = "differs"
        events = self.workspace._submit(self.state, self.ctx, self.doc("pipeline p out0 = keep;\n"))
        self.assertNotIn("accepted", [e["act"]["kind"] for e in events])
        self.assertIn("refused", events[-1]["symbolic"])

    def test_fresh_packet_for_a_changed_original_does_not_rebind_the_workspace(self):
        def changed_packet(cli, arguments):
            self.calls.append(arguments)
            return 0, {"status": "packet", "detail": {"original_sha256": "b" * 64,
                                                         "implementation_sha256": "a" * 64}}
        with patch.object(D, "run", side_effect=changed_packet):
            events = self.workspace._submit(self.state, self.ctx, self.doc("pipeline p out0 = keep;\n"))
        self.assertEqual(len(self.calls), 1)
        self.assertNotIn("accepted", [e["act"]["kind"] for e in events])
        self.assertIn("binding-changed", events[-1]["symbolic"])

    def test_replayed_result_on_a_changed_subject_never_yields_acceptance(self):
        self.reply["detail"]["binding"]["original_sha256"] = "b" * 64
        for body in ("pipeline p out0 = keep;\n", "mine r out1 = value;\n"):
            events = self.workspace._submit(self.state, self.ctx, self.doc(body))
            self.assertNotIn("accepted", [e["act"]["kind"] for e in events])
            self.assertIn("result-subject-changed", events[-1]["symbolic"])

    def test_unchanged_checked_rule_result_is_a_fact_not_an_accepted_rewrite(self):
        self.reply["door"] = "rules"
        self.reply["detail"].update(improved=False, result_kind="unchanged-original")
        events = self.workspace._submit(self.state, self.ctx, self.doc("mine r out1 = value;\n"))
        self.assertNotIn("accepted", [event["act"]["kind"] for event in events])
        self.assertIn("no improving rewrite checked", events[-1]["symbolic"])
        self.reply["detail"].update(improved=True, result_kind="improved-program")
        events = self.workspace._submit(self.state, self.ctx, self.doc("mine r2 out1 = value;\n"))
        self.assertIn("accepted", [event["act"]["kind"] for event in events])

    def test_original_drift_is_refused_before_any_cli_call(self):
        self.original.write_bytes(b"changed")
        events = self.workspace._submit(self.state, self.ctx, self.doc("pipeline p out0 = keep;\n"))
        self.assertEqual(self.calls, [])
        self.assertIn("binding-changed", events[-1]["symbolic"])

    def test_mining_only_writes_typed_terms_and_calls_rules(self):
        self.reply["door"] = "rules"
        self.workspace._submit(self.state, self.ctx, self.doc("mine r out1 = value + value - value;\n"))
        self.assertEqual(self.calls[0][:2], ["loop", "rules"])
        terms = json.loads(Path(self.calls[0][-1]).read_text())
        self.assertEqual(terms["terms"][0]["kind"], "Int")
        self.assertEqual(self.state["incumbent"], {"unchanged": True})

    def test_strategy_on_a_single_subject_refuses_without_inventing_a_holdout(self):
        events = self.workspace._submit(self.state, self.ctx, self.doc("strategy s out0 = keep;\nexplore s: depth 1;\n"))
        self.assertEqual(self.calls, [])
        self.assertIn("distinct-instances", events[-1]["symbolic"])

    def test_strategy_refuses_more_than_two_family_instances_before_search(self):
        family = self.workspace.data / "family"
        family.mkdir()
        (family / "manifest.json").write_text(json.dumps({"instances": [{}, {}, {}]}))
        self.state["family"] = {"manifest": "family/manifest.json"}
        events = self.workspace._submit(self.state, self.ctx, self.doc("strategy s out0 = keep;\n"))
        self.assertEqual(self.calls, [])
        self.assertIn("requires-two-instances", events[-1]["symbolic"])

    def test_family_strategy_routes_frozen_snapshots_and_a_declared_holdout(self):
        family = self.workspace.data / "family"
        family.mkdir()
        (family / "a.zcve").write_bytes(b"development snapshot")
        (family / "b.zcve").write_bytes(b"evaluation snapshot")
        manifest = {"vocabulary": W.vocabulary(self.state["subject"]), "instances": [
            {"program": "a.zcve", "parameters": {"C": 2}},
            {"program": "b.zcve", "parameters": {"C": 8}, "fit": False}]}
        (family / "manifest.json").write_text(json.dumps(manifest))
        self.state["family"] = {"manifest": "family/manifest.json"}
        self.reply = {"door": "strategy", "status": "evaluated", "detail": {"selection": {"stages": []}}}

        def call(cli, arguments):
            self.calls.append(arguments)
            if "--packet" in arguments:
                body = Path(arguments[arguments.index("--original") + 1]).read_bytes()
                return 0, {"status": "packet", "detail": {"original_sha256": hashlib.sha256(body).hexdigest(),
                                                             "implementation_sha256": "a" * 64}}
            return 0, self.reply

        with patch.object(D, "run", side_effect=call):
            self.workspace._submit(self.state, self.ctx, self.doc("strategy s out0 = keep;\nexplore s: depth 1;\n"))
        self.assertEqual(self.calls[-1][:2], ["loop", "strategy"])
        task_path = Path(self.calls[-1][self.calls[-1].index("--tasks") + 1])
        tasks = json.loads(task_path.read_text())
        self.assertEqual((len(tasks["development"]), len(tasks["evaluation"])), (1, 1))
        for group, expected in [("development", b"development snapshot"), ("evaluation", b"evaluation snapshot")]:
            task = tasks[group][0]
            snapshot = (task_path.parent / task["original"]).read_bytes()
            self.assertEqual(snapshot, expected)
            self.assertEqual(task["proposal"]["original_sha256"], hashlib.sha256(snapshot).hexdigest())
        self.assertEqual(self.state["incumbent"], {"unchanged": True})


if __name__ == "__main__":
    unittest.main()
