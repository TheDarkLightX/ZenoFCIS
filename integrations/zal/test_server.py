"""Lifecycle tests use a controlled transport; no account or model is contacted."""

import json
from pathlib import Path
import threading
import time
import unittest

from behavior import Refusal
from server import Workspace
from test_behavior import SOURCE


class ControlledTransport:
    def __init__(self):
        self.thread_id = "thread-a"
        self.turn_id = None
        self.entered = threading.Event()
        self.release = threading.Event()
        self.interrupted = threading.Event()
        self.next_turn = "turn-a"

    def start_turn(self, text, schema):
        self.entered.set()
        if not self.release.wait(2):
            raise ValueError("Test did not release pending response")
        self.turn_id = self.next_turn
        return self.turn_id

    def interrupt(self):
        self.interrupted.set()


class ServerTests(unittest.TestCase):
    def setUp(self):
        self.work = Workspace(SOURCE, Path.cwd())
        self.transport = ControlledTransport()
        self.work.transport = self.transport
        self.revision = self.work.session.model.revision

    def emit(self, method, turn_id="turn-a", **extra):
        params = {"threadId": "thread-a", "turnId": turn_id, **extra}
        self.work.notify({"method": method, "params": params})

    def finish(self, turn="turn-a"):
        act = {"act": "propose", "base_revision": self.revision,
               "object_ids": [self.work.session.selected], "message": "A candidate.",
               "syntax": "symbolic", "surface": SOURCE + "rule cancel_approved: approved + cancel [true] -> accept cancelled;\n"}
        self.emit("item/completed", turn, item={"type": "agentMessage", "text": json.dumps(act)})
        self.emit("turn/completed", turn, turn={"id": turn, "status": "completed"})

    def wait_idle(self):
        deadline = time.monotonic() + 2
        while self.work.running and time.monotonic() < deadline:
            time.sleep(.005)
        self.assertIsNone(self.work.running)

    def start(self):
        self.work.discuss("Explain this behavior", self.revision)
        self.assertTrue(self.transport.entered.wait(1))

    def test_cancel_before_start_response_cannot_import_or_restart(self):
        self.start()
        self.work.action("interrupt", {})
        self.emit("turn/started", turn={"id": "turn-a"})
        self.finish()
        with self.assertRaises(Refusal):
            self.work.discuss("A second question", self.revision)
        self.transport.release.set()
        self.assertTrue(self.transport.interrupted.wait(1))
        self.wait_idle()
        self.assertIsNone(self.work.session.proposal)
        self.finish()
        self.assertIsNone(self.work.session.proposal)

    def test_response_binds_only_exact_turn_and_connection(self):
        self.start()
        self.finish("old-turn")
        self.finish("turn-a")
        self.transport.release.set()
        self.wait_idle()
        self.assertIsNotNone(self.work.session.proposal)
        self.assertEqual(self.work.session.agreement, "seed-example")
        before = self.work.view()
        self.work.notify({"method": "zal/transportError", "params": {"message": "Old connection"}}, -1)
        self.assertEqual(self.work.view(), before)

    def test_views_own_questions_and_stale_accept_is_refused(self):
        self.work.questions = [{"id": 4, "params": {"questions": []}}]
        view = self.work.view()
        view["questions"][0]["params"]["questions"].append("alias")
        self.assertEqual(self.work.questions[0]["params"]["questions"], [])
        with self.assertRaises(Refusal):
            self.work.action("accept", {"base_revision": self.revision, "candidate_revision": "invented"})

    def test_agreed_future_requirement_still_cannot_export(self):
        self.work.action("propose", {"surface": SOURCE + "unresolved liveness: pending resolves;\n", "syntax": "symbolic", "revision": self.revision})
        candidate = self.work.session.proposal["revision"]
        self.work.action("accept", {"base_revision": self.revision, "candidate_revision": candidate})
        self.assertEqual(self.work.session.agreement, "human-accepted")
        with self.assertRaises(Refusal):
            self.work.action("export", {})


if __name__ == "__main__":
    unittest.main()
