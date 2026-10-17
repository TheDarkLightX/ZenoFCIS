"""Typed strategy acts and existing owner gate; native CLI explicitly mocked."""
from copy import deepcopy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from behavior import Refusal
import loop_language as Z
import loop_workspace as W
import loop_strategy_modes as S
from test_loop_language import COUNTER_SUBJECT

PROBLEM = {"schema": "zeno-fcis/loop-strategy-problem/1", "query": "count",
           "readings": [{"id": "s1", "alternatives": [{"marker": "a"}, {"marker": "b"}]}]}
QUESTION = {"statement": 0, "id": "s1", "ordinal": 0, "world": [0], "holds": [0], "does_not_hold": [1]}

class Modes(unittest.TestCase):
    def test_both_surfaces_round_trip_one_strategy_mode(self):
        ctx = Z.Context(COUNTER_SUBJECT)
        for kind in S.KINDS:
            a = Z.act("strategy-run", "s", mode=kind, problem=json.dumps(PROBLEM,sort_keys=True,separators=(",", ":")))
            for english in (False, True):
                self.assertEqual(Z.parse_statement(Z.render_act(a,ctx,english),ctx,english),a)
        for body in ['{"x":1,"x":2}', '{"owner_answers":[]}', '{"primitives":[]}', '{"v":0.5}', '{"v":NaN}', '{"v":Infinity}', '[' * 2000]:
            with self.assertRaises(Refusal): S.strict_problem(body)

    def fixture(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        workspace = W.LoopWorkspace(Path(temp.name) / "w.json")
        workspace.data.mkdir()
        state = {"subject":deepcopy(COUNTER_SUBJECT),"events":[],"questions":{},"next_question":1,
                 "next_document":1,"incumbent":{"untouched":True}}
        ctx = Z.Context(state["subject"])
        self.addCleanup(patch.stopall)
        patch.object(workspace,"_contract",return_value=workspace.data).start()
        patch.object(workspace,"cli",return_value=Path("test-cli")).start()
        # These fixtures isolate preparation-state durability; presentation is
        # checked by the full workspace suite with complete incumbents.
        patch.object(workspace,"view",return_value={"authority":"none"}).start()
        return workspace,state,ctx

    def document(self,ctx,author,acts): return Z.Document(ctx.subject_ref,author,tuple(acts))
    def reply(self,status="question"):
        return {"door":"strategy","status":status,"detail":{"question":deepcopy(QUESTION),
                "binding":{"problem_sha256":"a"*64,"implementation_sha256":"b"*64,"plan_sha256":"c"*64}}}

    def test_model_cannot_answer_a_strategy_question(self):
        workspace,state,ctx=self.fixture()
        with patch.object(S.D,"run",return_value=(0,self.reply())):
            a=Z.act("strategy-run","s",mode="ask-when-it-matters",problem=json.dumps(PROBLEM))
            workspace._submit(state,ctx,self.document(ctx,"model",[a]))
        self.assertEqual(state["questions"]["q1"]["status"],"open")
        self.assertEqual(state["questions"]["q1"]["asked_by"],"proposer")
        with self.assertRaises(Refusal): workspace._submit(state,ctx,self.document(ctx,"model",[Z.act("answer","q1",choice="any")]))
        self.assertEqual(state["questions"]["q1"]["status"],"open")

    def test_owner_filters_only_the_fresh_question_and_preserves_incumbent(self):
        workspace,state,ctx=self.fixture()
        with patch.object(S.D,"run",return_value=(0,self.reply())):
            workspace._submit(state,ctx,self.document(ctx,"model",[Z.act("strategy-run","s",mode="ask-when-it-matters",problem=json.dumps(PROBLEM))]))
        answered=self.reply("proposed-answer");answered["detail"]["question"]=None
        with patch.object(S.D,"run",side_effect=[(0,self.reply()),(0,answered)]):
            workspace._submit(state,ctx,self.document(ctx,"owner",[Z.act("answer","q1",choice="none")]))
        q=state["questions"]["q1"]
        self.assertEqual(q["status"],"answered")
        self.assertFalse(q["owner_reading_confirmation"]["holds"])
        self.assertEqual(state["incumbent"],{"untouched":True})
        self.assertNotIn("confirmed",state)
        self.assertEqual(q["confirmed_readings_problem"]["readings"][0]["alternatives"],[{"marker":"b"}])
        self.assertEqual(PROBLEM["readings"][0]["alternatives"],[{"marker":"a"},{"marker":"b"}])

    def test_owner_confirmation_survives_a_later_budget_stop_without_an_answer(self):
        workspace,state,ctx=self.fixture()
        with patch.object(S.D,"run",return_value=(0,self.reply())):
            workspace._submit(state,ctx,self.document(ctx,"model",[Z.act("strategy-run","s",mode="ask-when-it-matters",problem=json.dumps(PROBLEM))]))
        stopped={"door":"strategy","status":"inconclusive","detail":{"attempts_spent":0,"reason":"strategy-work-budget"}}
        with patch.object(S.D,"run",side_effect=[(0,self.reply()),(2,stopped)]):
            workspace._submit(state,ctx,self.document(ctx,"owner",[Z.act("answer","q1",choice="any")]))
        self.assertEqual(state["questions"]["q1"]["status"],"answered")
        self.assertTrue(state["questions"]["q1"]["owner_reading_confirmation"]["holds"])
        self.assertEqual(state["questions"]["q1"]["confirmed_readings_problem"]["readings"][0]["alternatives"],[{"marker":"a"}])
        self.assertEqual(state["incumbent"],{"untouched":True})

    def test_stale_implementation_or_question_cannot_record_owner_confirmation(self):
        workspace,state,ctx=self.fixture()
        with patch.object(S.D,"run",return_value=(0,self.reply())):
            workspace._submit(state,ctx,self.document(ctx,"model",[Z.act("strategy-run","s",mode="ask-when-it-matters",problem=json.dumps(PROBLEM))]))
        stale=self.reply();stale["detail"]["binding"]["implementation_sha256"]="d"*64
        with patch.object(S.D,"run",return_value=(0,stale)):
            events=workspace._submit(state,ctx,self.document(ctx,"owner",[Z.act("answer","q1",choice="any")]))
        self.assertEqual(state["questions"]["q1"]["status"],"open")
        self.assertNotIn("owner_reading_confirmation",state["questions"]["q1"])
        self.assertIn("question-changed",events[-1]["symbolic"])

    def test_validated_owner_confirmation_is_durable_before_a_raised_followup_timeout(self):
        workspace, state, ctx = self.fixture()
        with patch.object(S.D, "run", return_value=(0, self.reply())):
            workspace._submit(state, ctx, self.document(ctx, "model", [Z.act(
                "strategy-run", "s", mode="ask-when-it-matters", problem=json.dumps(PROBLEM))]))
        workspace.save(state)
        with patch.object(S.D, "run", side_effect=[(0, self.reply()), Refusal("zeno-fcis did not finish within its time limit")]):
            events = workspace._submit(state, ctx, self.document(ctx, "owner", [Z.act("answer", "q1", choice="any")]))
        saved = workspace.load()
        question = saved["questions"]["q1"]
        self.assertEqual(question["status"], "answered")
        self.assertTrue(question["owner_reading_confirmation"]["holds"])
        self.assertEqual(question["confirmed_readings_problem"]["readings"][0]["alternatives"], [{"marker": "a"}])
        self.assertIn("time limit", events[-1]["symbolic"])
        self.assertEqual(saved["incumbent"], {"untouched": True})

if __name__ == "__main__": unittest.main()
