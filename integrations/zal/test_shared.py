import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from behavior import Refusal, parse, replay
from mcp import Bridge, TOOLS
from shared import SharedWorkspace
from server import Workspace
from terminal import ReviewClient, command, human_text
from test_behavior import SOURCE
from test_server import ControlledTransport


class SharedTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.path = Path(self.temp.name) / "workspace.json"
        self.store = SharedWorkspace(self.path)
        self.store.initialize(SOURCE)
        self.bridge = Bridge(self.store)

    def tearDown(self):
        self.temp.cleanup()

    def propose(self):
        state = self.bridge.call("zal_read", {})
        self.bridge.call("zal_propose", {"base_revision": state["model"]["revision"],
             "object_ids": ["rule:approve_order"], "message": "Allow cancellation after approval.",
             "syntax": "symbolic", "surface": SOURCE + "rule cancel_approved: approved + cancel [true] -> accept cancelled;\n"})
        return self.bridge.call("zal_read", {})

    def test_model_tools_propose_human_terminal_accepts_then_refreshes(self):
        state = self.propose()
        self.assertEqual(state["agreement"], "seed-example")
        p = state["proposal"]
        self.store.use(lambda session: command(session, "accept " + p["base_revision"] + " " + p["revision"]))
        next_state = self.bridge.call("zal_read", {})
        self.assertEqual(next_state["agreement"], "human-accepted")
        self.assertEqual(next_state["realization"], "not-run")
        self.assertEqual(next_state["model"]["revision"], p["revision"])
        with self.assertRaises(Refusal):
            self.bridge.call("zal_check", {"revision": p["base_revision"]})
        self.assertFalse(any("accept" in tool["name"] for tool in TOOLS))
        with self.assertRaises(Refusal):
            self.bridge.call("zal_accept", {})

    def test_refusal_is_atomic_and_existing_workspace_is_preserved(self):
        state = self.propose()
        before = self.path.read_bytes()
        with self.assertRaises(Refusal):
            self.store.initialize(SOURCE)
        with self.assertRaises(Refusal):
            self.store.use(lambda s: command(s, "accept stale " + state["proposal"]["revision"]))
        self.assertEqual(self.path.read_bytes(), before)
        self.assertEqual(self.path.stat().st_mode & 0o777, 0o600)
        self.store.use(lambda s: command(s, "cancel"))
        self.assertIsNone(self.bridge.call("zal_read", {})["proposal"])

    def test_stdio_protocol_initialization_listing_read_and_no_accept(self):
        requests = [{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2024-11-05"}},
                    {"jsonrpc": "2.0", "method": "notifications/initialized"},
                    {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
                    {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "zal_read", "arguments": {}}},
                    {"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "zal_accept", "arguments": {}}}]
        result = subprocess.run([sys.executable, str(Path(__file__).parent / "mcp.py"), "--workspace", str(self.path)],
                    input="\n".join(json.dumps(r) for r in requests) + "\n", text=True, capture_output=True, check=True)
        replies = [json.loads(line) for line in result.stdout.splitlines()]
        self.assertEqual([r["id"] for r in replies], [1, 2, 3, 4])
        self.assertFalse(replies[2]["result"]["isError"])
        self.assertTrue(replies[3]["result"]["isError"])
        self.assertEqual(result.stderr, "")

    def test_human_terminal_read_select_legend_trace_check_and_reject(self):
        for line in ["show", "select state:pending", "legend", "check",
                     'trace [{"event":"submit","context":{"authorized":false}}]', "why"]:
            self.assertIsNotNone(self.store.use(lambda session: command(session, line)))
        state = self.propose()
        proposal = state["proposal"]
        self.store.use(lambda session: command(session, "reject " + proposal["base_revision"] + " " + proposal["revision"]))
        self.assertIsNone(self.bridge.call("zal_read", {})["proposal"])

    def test_request_identity_is_checked_before_any_proposal_mutation(self):
        state = self.bridge.call("zal_read", {})
        request = {"jsonrpc": "2.0", "method": "tools/call", "params": {
            "name": "zal_propose", "arguments": {
                "base_revision": state["model"]["revision"], "object_ids": ["rule:approve_order"],
                "message": "A candidate, not agreement.", "syntax": "symbolic",
                "surface": SOURCE + "rule cancel_approved: approved + cancel [true] -> accept cancelled;\n"}}}
        before = self.path.read_bytes()
        self.assertIsNone(self.bridge.handle(request))
        for invalid in [None, True, False, 1.5, [], {}, "x" * 257]:
            with self.subTest(request_id=invalid), self.assertRaises(Refusal):
                self.bridge.handle({**request, "id": invalid})
            self.assertEqual(self.path.read_bytes(), before)
        for invalid in [None, [], False, ""]:
            with self.subTest(params=invalid), self.assertRaises(Refusal):
                self.bridge.handle({**request, "id": 1, "params": invalid})
            self.assertEqual(self.path.read_bytes(), before)

        stream = [request, *[{**request, "id": invalid} for invalid in [None, True, {}, 1.5]],
                  {"jsonrpc": "2.0", "id": "read", "method": "tools/call",
                   "params": {"name": "zal_read", "arguments": {}}}]
        result = subprocess.run([sys.executable, str(Path(__file__).parent / "mcp.py"),
            "--workspace", str(self.path)], input="".join(json.dumps(r) + "\n" for r in stream),
            text=True, capture_output=True, check=True)
        replies = [json.loads(line) for line in result.stdout.splitlines()]
        self.assertEqual(len(replies), 5)
        self.assertTrue(all(r["id"] is None and "error" in r for r in replies[:4]))
        self.assertEqual(replies[-1]["id"], "read")
        self.assertFalse(replies[-1]["result"]["isError"])
        self.assertEqual(self.path.read_bytes(), before)

    def test_tool_schema_types_are_enforced_and_falsey_nonarrays_are_refused(self):
        revision = self.bridge.call("zal_read", {})["model"]["revision"]
        before = self.path.read_bytes()
        for value in [{}, "", None, False, 0, ()]:
            with self.subTest(inputs=value), self.assertRaises(Refusal):
                self.bridge.call("zal_trace", {"revision": revision, "inputs": value})
            with self.subTest(direct_inputs=value), self.assertRaises(Refusal):
                replay(parse(SOURCE), value)
        for inputs in [[{"event": "submit", "context": {"authorized": 0}}],
                       [{"event": ["submit"], "context": {"authorized": False}}],
                       [{"event": "submit", "context": []}], [{}], [None]]:
            with self.subTest(inputs=inputs), self.assertRaises(Refusal):
                self.bridge.call("zal_trace", {"revision": revision, "inputs": inputs})
        for name, arguments in [("zal_read", []), ("zal_read", {"accept": True}),
                ("zal_check", {"revision": []}),
                ("zal_compare", {"revision": revision, "syntax": [], "surface": SOURCE})]:
            with self.subTest(tool=name, arguments=arguments), self.assertRaises(Refusal):
                self.bridge.call(name, arguments)
        self.assertEqual(self.bridge.call("zal_trace", {"revision": revision, "inputs": []})["trace"], [])
        self.assertEqual(self.path.read_bytes(), before)

    def test_browser_terminal_and_mcp_observe_one_shared_revision(self):
        browser = Workspace(SOURCE, Path.cwd(), self.path)
        initial = browser.view()
        state = self.propose()
        proposal = state["proposal"]
        observed = browser.view()
        self.assertEqual(observed["proposal"]["revision"], proposal["revision"])
        self.assertGreater(observed["version"], initial["version"])
        self.assertEqual(observed["workspace_mode"], "shared terminal/MCP workspace")
        browser.action("accept", {"base_revision": proposal["base_revision"],
                                  "candidate_revision": proposal["revision"]})
        terminal = self.store.use(lambda s: command(s, "show"))
        self.assertEqual(terminal["revision"], proposal["revision"])
        self.assertEqual(self.bridge.call("zal_read", {})["model"]["revision"], proposal["revision"])
        before = self.path.read_bytes()
        with self.assertRaises(Refusal):
            browser.action("propose", {"revision": proposal["base_revision"],
                "syntax": "symbolic", "surface": SOURCE})
        self.assertEqual(self.path.read_bytes(), before)
        browser.action("propose", {"revision": proposal["revision"], "syntax": "symbolic", "surface": SOURCE})
        pending = self.bridge.call("zal_read", {})["proposal"]
        self.store.use(lambda s: command(s, "reject " + pending["base_revision"] + " " + pending["revision"]))
        self.assertIsNone(browser.view()["proposal"])

    def test_external_acceptance_invalidates_a_pending_browser_model_turn(self):
        browser = Workspace(SOURCE, Path.cwd(), self.path)
        class Transport:
            thread_id = "shared-turn"
        browser.transport = Transport()
        browser.running = "pending"
        browser.turn_base = browser.view()["model"]["revision"]
        staged = self.propose()["proposal"]
        self.store.use(lambda s: command(s, "accept " + staged["base_revision"] + " " + staged["revision"]))
        browser.messages["pending"] = json.dumps({"act": "explain", "base_revision": browser.turn_base,
            "object_ids": ["default"], "message": "Old meaning", "syntax": "symbolic", "surface": None})
        browser.notify({"method": "turn/completed", "params": {"threadId": "shared-turn",
            "turnId": "pending", "turn": {"id": "pending", "status": "completed"}}})
        self.assertIsNone(browser.running)
        self.assertIn("stale", browser.error)
        state = self.bridge.call("zal_read", {})
        self.assertEqual(state["model"]["revision"], staged["revision"])
        self.assertFalse(any(e["data"].get("message") == "Old meaning" for e in state["events"]))

    def test_failed_shared_write_does_not_strand_an_unstarted_model_turn(self):
        browser = Workspace(SOURCE, Path.cwd(), self.path)
        transport = ControlledTransport()
        browser.transport = transport
        revision = browser.view()["model"]["revision"]
        before = self.path.read_bytes()
        with patch.object(browser.store, "save", side_effect=OSError("Simulated write failure")):
            with self.assertRaisesRegex(OSError, "write failure"):
                browser.discuss("Explain the current rule", revision)
        self.assertFalse(transport.entered.is_set())
        self.assertIsNone(browser.running)
        self.assertFalse(browser.start_pending)
        self.assertEqual(self.path.read_bytes(), before)
        self.assertEqual(browser.view()["events"], [])
        browser.discuss("Try a deterministic explanation", revision, demo=True)
        self.assertIsNotNone(browser.view()["proposal"])

    def test_human_review_accepts_only_the_candidate_previously_displayed_here(self):
        client = ReviewClient(self.store)
        first = self.propose()["proposal"]
        with self.assertRaisesRegex(Refusal, "displayed"):
            client.execute("accept")
        output = []
        client.present(client.execute("candidate"), output.append)
        text = output[-1]
        self.assertIn("Proposed controlled-English meaning", text)
        self.assertIn("Witness:", text)
        self.assertIn("type accept or reject", text)
        client.execute("show")  # A harmless read does not invalidate the displayed candidate.
        self.bridge.call("zal_propose", {"base_revision": first["base_revision"],
            "object_ids": ["default"], "message": "A different candidate", "syntax": "symbolic",
            "surface": SOURCE.replace("not_enabled", "not_allowed")})
        before = self.path.read_bytes()
        with self.assertRaisesRegex(Refusal, "changed"):
            client.execute("accept")
        self.assertEqual(self.path.read_bytes(), before)
        displayed = client.execute("candidate")
        client.present(displayed, output.append)
        result = client.execute("accept")
        self.assertEqual(result["revision"], displayed["revision"])
        self.assertIn("human-accepted", human_text(result))
        with self.assertRaises(Refusal):
            client.execute("accept")

    def test_human_trace_and_real_terminal_transcript_need_no_json_or_hashes(self):
        client = ReviewClient(self.store)
        trace = client.execute("trace submit authorized=no then approve authorized=yes")
        text = human_text(trace)
        self.assertIn("draft -- submit", text)
        self.assertIn("Accept -> approved", text)
        self.assertIn("assumed inputs", text)
        self.propose()
        result = subprocess.run([sys.executable, str(Path(__file__).parent / "terminal.py"),
            "review", "--workspace", str(self.path)], input="candidate\naccept\nshow\nquit\n",
            text=True, capture_output=True, check=True)
        self.assertIn("Proposed controlled-English meaning", result.stdout)
        self.assertIn("Meaning: human-accepted", result.stdout)
        self.assertNotIn('"base_revision":', result.stdout)
        self.assertEqual(self.bridge.call("zal_read", {})["agreement"], "human-accepted")

    def test_terminal_output_controls_and_failed_display_never_grant_review(self):
        client = ReviewClient(self.store)
        state = self.bridge.call("zal_read", {})
        self.bridge.call("zal_propose", {"base_revision": state["model"]["revision"],
            "object_ids": ["default"], "message": "Normal text\nControl: \x1b[2J\x07\r\x7f\u202e",
            "syntax": "symbolic", "surface": SOURCE.replace("not_enabled", "not_allowed")})
        result = client.execute("candidate")
        output = human_text(result)
        self.assertIn("Normal text\n", output)
        self.assertIn("\\u001b", output)
        self.assertIn("\\u202e", output)
        self.assertFalse(any(char in output for char in ["\x1b", "\x07", "\r", "\x7f", "\u202e"]))
        for failure in [OSError("Display failed"), KeyboardInterrupt()]:
            result = client.execute("candidate")
            with patch("terminal.human_text", side_effect=failure):
                with self.assertRaises(type(failure)):
                    client.present(result, lambda text: None)
            with self.assertRaises(Refusal):
                client.execute("accept")
        result = client.execute("candidate")
        with self.assertRaises(OSError):
            client.present(result, lambda text: (_ for _ in ()).throw(OSError("Output failed")))
        with self.assertRaises(Refusal):
            client.execute("accept")
        client.present(client.execute("candidate"), lambda text: None)
        self.assertEqual(client.execute("reject")["agreement"], "seed-example")

    def test_local_help_translation_and_why_preserve_review_and_never_call_a_model(self):
        client = ReviewClient(self.store)
        self.propose()
        client.present(client.execute("candidate"), lambda text: None)
        before, binding = self.path.read_bytes(), client.displayed
        with patch.dict(os.environ, {"PATH": ""}), \
                patch("codex_transport.subprocess.Popen", side_effect=AssertionError("No child process is permitted")), \
                patch("socket.socket", side_effect=AssertionError("No network is permitted")), \
                patch("server.AppServer", side_effect=AssertionError("No model is permitted")):
            for line in ["?", "? and", "man approve_order", "help frame", "legend default", "why",
                         "translate english", "translate symbolic", "why pending approve authorized=no"]:
                result = client.execute(line)
                text = human_text(result)
                self.assertTrue(text)
                self.assertEqual(client.displayed, binding)
                self.assertEqual(self.path.read_bytes(), before)
            self.assertIn("No rule is enabled", human_text(client.execute("why pending approve authorized=no")))
            self.assertIn("authorized observed no = false", human_text(client.execute("why pending approve authorized=no")))
            self.assertIn("Accept -> approved", human_text(client.execute("why pending approve authorized=yes")))
            revision = self.bridge.call("zal_read", {})["model"]["revision"]
            self.assertEqual(self.bridge.call("zal_help", {"revision": revision, "topic": "and"})["topic"], "&")
            browser = Workspace(SOURCE, Path.cwd(), self.path)
            self.assertEqual(browser.action("help", {"revision": revision, "topic": "frame"})["topic"], "frame")
            self.assertEqual(browser.action("explain", {"revision": revision, "state": "pending", "event": "approve",
                "context": {"authorized": False}})["outcome"]["class"], "Reject")
        for line in ["why pending approve", "why pending approve authorized=0", "translate python"]:
            with self.subTest(command=line), self.assertRaises(Refusal):
                client.execute(line)
        self.assertEqual(self.path.read_bytes(), before)
        self.assertEqual(client.execute("reject")["agreement"], "seed-example")

    def test_mcp_and_companion_local_help_share_identity_and_refuse_stale_inputs(self):
        revision = self.bridge.call("zal_read", {})["model"]["revision"]
        browser = Workspace(SOURCE, Path.cwd(), self.path)
        before = self.path.read_bytes()
        args = {"revision": revision, "state": "pending", "event": "approve", "context": {"authorized": False}}
        self.assertEqual(self.bridge.call("zal_explain", args), browser.action("explain", args))
        help_args = {"revision": revision, "topic": "rule:approve_order"}
        self.assertEqual(self.bridge.call("zal_help", help_args), browser.action("help", help_args))
        self.assertEqual(self.path.read_bytes(), before)
        for tool, arguments in [("zal_help", {**help_args, "revision": "stale"}),
                                ("zal_explain", {**args, "context": {}}),
                                ("zal_explain", {**args, "context": None})]:
            with self.subTest(tool=tool), self.assertRaises(Refusal):
                self.bridge.call(tool, arguments)


if __name__ == "__main__":
    unittest.main()
