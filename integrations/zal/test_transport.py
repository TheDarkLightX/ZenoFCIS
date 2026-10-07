"""Protocol fixtures; live account tests are an explicit separate command."""

import unittest
import json
from unittest.mock import patch

from codex_transport import AppServer, TransportError
from behavior import parse
from probe_codex import probe
from test_behavior import SOURCE


class TransportTests(unittest.TestCase):
    def setUp(self):
        self.events = []
        self.client = AppServer(self.events.append, executable="python3")
        self.sent = []
        self.client.send = self.sent.append

    def test_domain_agreement_never_follows_tool_approval(self):
        self.client._dispatch({"id": 9, "method": "item/commandExecution/requestApproval", "params": {}})
        self.assertIn("error", self.sent[0])
        self.assertNotIn("result", self.sent[0])
        self.assertEqual(len(self.events), 1)

    def test_question_needs_human_and_exact_ids(self):
        question = {"id": 8, "method": "item/tool/requestUserInput", "params": {
            "questions": [{"id": "behavior", "question": "Should cancellation be terminal?"}],
        }}
        self.client._dispatch(question)
        self.assertEqual(self.sent, [])
        with self.assertRaises(TransportError):
            self.client.answer(8, {"wrong": ["yes"]})
        self.client.answer(8, {"behavior": ["yes"]})
        self.assertEqual(self.sent[-1]["result"], {"answers": {"behavior": {"answers": ["yes"]}}})
        with self.assertRaises(TransportError):
            self.client.answer(8, {"behavior": ["yes"]})

    def test_read_only_thread_and_schema_bound_turn(self):
        requests = []
        def request(method, params):
            requests.append((method, params))
            return {"thread": {"id": "t1"}} if method == "thread/start" else {"turn": {"id": "u1"}}
        self.client.request = request
        self.client.start_thread(".")
        self.client.start_turn("selected transition at revision r1", {"type": "object"})
        self.assertEqual(requests[0][1]["sandbox"], "read-only")
        self.assertEqual(requests[0][1]["approvalPolicy"], "on-request")
        self.assertEqual(requests[1][1]["outputSchema"], {"type": "object"})
        with self.assertRaises(TransportError):
            self.client.steer("new input", "old")
        self.client.steer("new input", "u1")
        self.assertEqual(requests[-1][1]["expectedTurnId"], "u1")
        self.client.interrupt()
        self.assertEqual(requests[-1], ("turn/interrupt", {"threadId": "t1", "turnId": "u1"}))

    def test_late_response_is_discarded(self):
        self.client._dispatch({"id": 99, "result": {"fake": True}})
        self.assertEqual(self.client.responses, {})
        self.client.pending.add(7)
        self.client._dispatch({"id": 7, "result": {"ok": True}})
        self.assertEqual(self.client.responses[7]["result"], {"ok": True})

    def test_event_budget_is_bounded(self):
        with patch("codex_transport.MAX_EVENTS", 1):
            self.client._dispatch({"method": "item/agentMessage/delta", "params": {"delta": "a"}})
            with self.assertRaises(TransportError):
                self.client._dispatch({"method": "item/agentMessage/delta", "params": {"delta": "b"}})

    def test_probe_cannot_label_another_act_or_turn_as_an_explanation_pass(self):
        model = parse(SOURCE)
        for act, stale in [("explain", False), ("ask", False), ("propose", False), ("explain", True)]:
            value = {"act": act, "base_revision": model.revision, "object_ids": ["default"],
                     "message": "A test response", "syntax": "symbolic",
                     "surface": SOURCE + "unresolved liveness: pending resolves;\n" if act == "propose" else None}
            class FakeAppServer:
                def __init__(self, callback):
                    self.callback = callback
                    self.thread_id = "probe-thread"
                def connect(self):
                    return {}
                def request(self, method, params):
                    return {"data": [{"model": "fixture-only", "isDefault": True}]}
                def start_thread(self, cwd, selected):
                    return self.thread_id
                def start_turn(self, text, schema):
                    observed_turn = "stale-turn" if stale else "probe-turn"
                    self.callback({"method": "item/completed", "params": {"threadId": self.thread_id,
                        "turnId": observed_turn, "item": {"type": "agentMessage", "text": json.dumps(value)}}})
                    self.callback({"method": "turn/completed", "params": {"threadId": self.thread_id,
                        "turn": {"id": observed_turn, "status": "completed"}}})
                    return "probe-turn"
                def close(self):
                    pass
            with self.subTest(act=act, stale=stale), patch("probe_codex.AppServer", FakeAppServer), \
                    patch("probe_codex.subprocess.check_output", return_value="fixture-cli"):
                result = probe(live=True)
            expected = "failed-or-disconnected" if stale else "completed-structured-explanation-passed" if act == "explain" else "unavailable"
            self.assertEqual(result["live_model"], expected)
            self.assertEqual(result["domain_agreement"], "none")


if __name__ == "__main__":
    unittest.main()
