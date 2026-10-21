"""Cross-language store and CLI compatibility tests for Zeno Commons."""

from __future__ import annotations

import json
import os
import re
import shlex
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from commons import Commons  # noqa: E402
from archive_test_support import ARCHIVE_REFUSAL, is_archive_refusal  # noqa: E402


def rust_prefix() -> list[str]:
    configured = os.environ.get("ZENO_FCIS_COMMONS_CLI")
    if not configured:
        raise RuntimeError("set ZENO_FCIS_COMMONS_CLI to the Rust zeno-fcis executable")
    prefix = shlex.split(configured)
    return prefix[:-1] if prefix and prefix[-1] == "commons" else prefix


def rust_cli(store: str, actor: str, *arguments: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["nice", "-n", "19", "ionice", "-c3", *rust_prefix(), "commons", "--store", store, "--actor", actor, *arguments],
        capture_output=True,
        text=True,
        timeout=60,
    )


def run_ok(*arguments: str) -> dict | list:
    result = rust_cli(*arguments)
    if result.returncode != 0:
        raise AssertionError(f"Rust CLI failed ({result.returncode}): {result.stderr}")
    return json.loads(result.stdout)


def python_cli(store: str, actor: str, *arguments: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["nice", "-n", "19", "ionice", "-c3", sys.executable, "-I", str(HERE / "cli.py"),
         "--store", store, "--actor", actor, *arguments],
        capture_output=True,
        text=True,
        timeout=60,
    )


def normalized_projections(store: str, actor: str) -> dict:
    return {
        "records": run_ok(store, actor, "records"),
        "gaps": run_ok(store, actor, "gaps"),
        "dedup": run_ok(store, actor, "dedup", "--threshold", "0.2"),
        "leases": run_ok(store, actor, "lease", "list"),
        "agents": run_ok(store, actor, "agents"),
        "messages": run_ok(store, actor, "inbox", "--all")["messages"],
    }


def remove_wall_clock_fields(projections: dict) -> dict:
    normalized = json.loads(json.dumps(projections))
    for record in normalized["records"]:
        record["body"] = re.sub(r"\[\d{4}-\d{2}-\d{2}T[^\]]+\]", "[<time>]", record["body"])
    for lease in normalized["leases"]:
        lease["expires"] = "<time>"
    for agent in normalized["agents"]:
        agent["updated"] = "<time>"
        agent["id"] = f"<id:{agent['agent']}>"
    for message in normalized["messages"]:
        message["at"] = "<time>"
    return normalized


def mcp_transcript(command: list[str]) -> list[dict]:
    requests = [
        {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
        {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}},
        {"jsonrpc": "2.0", "id": 20, "method": "tools/call", "params": {"name": "commons_register", "arguments": {"role": "test"}}},
        {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "commons_record_add", "arguments": {
            "type": "gap", "title": "No keyed collections", "tags": ["collections"]}}},
        {"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "commons_advise", "arguments": {
            "action": "build a mailbox", "tags": ["collections"]}}},
        {"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {"name": "commons_status", "arguments": {"state": "napping"}}},
    ]
    process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    output, _ = process.communicate("".join(json.dumps(request) + "\n" for request in requests), timeout=60)
    if process.returncode != 0:
        raise AssertionError(f"MCP process failed ({process.returncode}): {output}")
    replies = [json.loads(line) for line in output.splitlines()]
    for reply in replies:
        result = reply.get("result", {})
        if "content" in result:
            content = result["content"][0]["text"]
            if not result.get("isError"):
                result["content"][0]["text"] = json.loads(content)
                if reply.get("id") == 20:
                    result["content"][0]["text"]["id"] = "<id>"
    return replies


class CrossImplementationTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def test_python_writes_rust_verifies(self):
        store = self.root / "python-to-rust"
        python = Commons(store, "python-writer")
        python.emit("cross", "python", "python event", {"value": "café", "n": 7})
        python.close()
        self.assertEqual(run_ok(str(store), "rust-reader", "verify")["checked"], 1)

    def test_rust_writes_python_verifies(self):
        store = self.root / "rust-to-python"
        rust_cli(str(store), "rust-writer", "emit", "cross", "rust", "rust event", "--body",
                 '{"value":"雪","n":9}')
        python = Commons(store, "python-reader")
        self.assertEqual(python.verify()["checked"], 1)
        python.close()

    def test_alternating_writes_verify_in_both_implementations(self):
        store = self.root / "alternating"
        python = Commons(store, "python-writer")
        python.emit("cross", "python", "first", {"index": 0})
        rust_cli(str(store), "rust-writer", "emit", "cross", "rust", "second", "--body", '{"index":1}')
        python.emit("cross", "python", "third", {"index": 2})
        rust_cli(str(store), "rust-writer", "emit", "cross", "rust", "fourth", "--body", '{"index":3}')
        self.assertEqual(python.verify()["checked"], 4)
        python.close()
        self.assertEqual(run_ok(str(store), "rust-reader", "verify")["checked"], 4)

    def test_identity_table_and_registration_events_cross_both_ways(self):
        python_store = self.root / "python-identity"
        python = Commons(python_store, "python-agent")
        python_identity = python.register(kind="builder", role="cross-test")
        python.close()
        self.assertEqual(run_ok(str(python_store), "rust-reader", "whoami", python_identity["id"])["name"],
                         "python-agent")
        rust_event = run_ok(str(python_store), "rust-reader", "events", "--kind", "identity.register")
        self.assertEqual(rust_event[0]["body"]["kind"], "builder")
        self.assertTrue(run_ok(str(python_store), "rust-reader", "verify")["ok"])

        rust_store = self.root / "rust-identity"
        registered = rust_cli(str(rust_store), "rust-agent", "register", "--kind", "reviewer", "--role", "cross-test")
        self.assertEqual(registered.returncode, 0, registered.stderr)
        rust_identity = json.loads(registered.stdout)
        python = Commons(rust_store, "python-reader")
        self.assertEqual(python.identity(rust_identity["id"])["name"], "rust-agent")
        self.assertEqual(python.events(kind_prefix="identity.register")[0]["body"]["kind"], "reviewer")
        self.assertTrue(python.verify()["ok"])
        python.close()

    def test_gap_operation_events_cross_both_ways(self):
        store = self.root / "gap-operations"
        python_registration = python_cli(str(store), "python-writer", "register", "--kind", "reporter")
        rust_registration = rust_cli(str(store), "rust-writer", "register", "--kind", "reviewer")
        self.assertEqual(python_registration.returncode, 0, python_registration.stderr)
        self.assertEqual(rust_registration.returncode, 0, rust_registration.stderr)

        created = python_cli(
            str(store), "python-writer", "add", "gap", "Mailbox cannot expose unread deliveries",
            "--tag", "severity-low", "--ref", "python-log",
        )
        self.assertEqual(created.returncode, 0, created.stderr)
        canonical_id = json.loads(created.stdout)["id"]
        duplicate = rust_cli(
            str(store), "rust-writer", "add", "gap", "Unread deliveries remain hidden in mailbox",
            "--tag", "severity-high", "--ref", "rust-log",
        )
        self.assertEqual(duplicate.returncode, 0, duplicate.stderr)
        duplicate_id = json.loads(duplicate.stdout)["id"]
        merged = rust_cli(str(store), "rust-writer", "merge", duplicate_id, "--into", canonical_id)
        self.assertEqual(merged.returncode, 0, merged.stderr)

        python_verify = python_cli(str(store), "python-reader", "verify")
        self.assertEqual(python_verify.returncode, 0, python_verify.stderr)
        self.assertTrue(json.loads(python_verify.stdout)["ok"])
        python_gaps = json.loads(python_cli(str(store), "python-reader", "gaps").stdout)
        [canonical] = [gap for gap in python_gaps if gap["id"] == canonical_id]
        self.assertEqual(canonical["severity"], "high")
        self.assertEqual(canonical["reporters"], ["python-writer", "rust-writer"])

        planned = python_cli(
            str(store), "python-writer", "add", "gap", "Search needs stable cursors",
            "--status", "planned", "--tag", "severity-medium",
        )
        self.assertEqual(planned.returncode, 0, planned.stderr)
        rust_verify = rust_cli(str(store), "rust-reader", "verify")
        self.assertEqual(rust_verify.returncode, 0, rust_verify.stderr)
        self.assertTrue(json.loads(rust_verify.stdout)["ok"])
        rust_gaps = run_ok(str(store), "rust-reader", "gaps")
        [planned_gap] = [gap for gap in rust_gaps if gap["title"] == "Search needs stable cursors"]
        self.assertEqual((planned_gap["status"], planned_gap["severity"]), ("planned", "medium"))

    def test_python_archives_verify_in_rust(self):
        store = self.root / "python-archive"
        python = Commons(store, "python-archiver")
        python.register()
        for index in range(12):
            python.emit("cross", "tick", str(index), {"index": index})
        archived = python.archive(keep_events=3, vacuum=False)
        self.assertGreater(archived["archived"], 0)
        python.close()
        result = rust_cli(str(store), "rust-reader", "verify", "--archives")
        if is_archive_refusal(result):
            self.assertEqual(result.returncode, 2)
            self.assertEqual(json.loads(result.stderr)["refused"], ARCHIVE_REFUSAL)
            self.skipTest(ARCHIVE_REFUSAL)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(json.loads(result.stdout)["ok"])

    def test_python_archived_store_passes_rust_live_verify(self):
        store = self.root / "python-archive-live-verify"
        python = Commons(store, "python-archiver")
        python.register()
        for index in range(12):
            python.emit("cross", "tick", str(index), {"index": index})
        archived = python.archive(keep_events=3, vacuum=False)
        self.assertGreater(archived["archived"], 0)
        python.close()
        result = rust_cli(str(store), "rust-reader", "verify")
        self.assertEqual(result.returncode, 0, result.stderr)
        verification = json.loads(result.stdout)
        self.assertTrue(verification["ok"], verification)
        self.assertGreater(verification["from_seq"], 1)

    def test_rust_archives_verify_in_python(self):
        store = self.root / "rust-archive"
        self.assertEqual(rust_cli(str(store), "rust-archiver", "register").returncode, 0)
        for index in range(12):
            self.assertEqual(
                rust_cli(str(store), "rust-archiver", "emit", "cross", "tick", str(index),
                         "--body", json.dumps({"index": index})).returncode,
                0,
            )
        archived = rust_cli(str(store), "rust-archiver", "archive", "--keep", "3", "--no-vacuum")
        if is_archive_refusal(archived):
            self.assertEqual(archived.returncode, 2)
            self.assertEqual(json.loads(archived.stderr)["refused"], ARCHIVE_REFUSAL)
            self.skipTest(ARCHIVE_REFUSAL)
        self.assertEqual(archived.returncode, 0, archived.stderr)
        python = Commons(store, "python-reader")
        self.assertTrue(python.verify(archives=True)["ok"])
        python.close()

    def test_cli_script_has_matching_record_lease_agent_and_message_projections(self):
        python_store = str(self.root / "python-script")
        rust_store = str(self.root / "rust-script")

        def script(invoke, store):
            for actor in ("lead", "peer"):
                registered = invoke(store, actor, "register", "--kind", "builder", "--role", "parity")
                self.assertEqual(registered.returncode, 0, registered.stderr)
            status = invoke(store, "lead", "status", "working", "--step", "cross-check")
            self.assertEqual(status.returncode, 0, status.stderr)
            lease = invoke(store, "lead", "lease", "take", "branch:commons-test", "--ttl", "3600", "--note", "parity")
            self.assertEqual(lease.returncode, 0, lease.stderr)
            lesson = invoke(store, "lead", "add", "lesson", "Cross-language record", "--body", "shared text", "--tag", "parity", "--ref", "fixture/a")
            self.assertEqual(lesson.returncode, 0, lesson.stderr)
            lesson_id = json.loads(lesson.stdout)["id"]
            risk = invoke(store, "lead", "add", "risk", "Message ordering", "--tag", "parity")
            self.assertEqual(risk.returncode, 0, risk.stderr)
            risk_id = json.loads(risk.stdout)["id"]
            linked = invoke(store, "lead", "link", lesson_id, "relates", risk_id)
            self.assertEqual(linked.returncode, 0, linked.stderr)
            updated = invoke(store, "lead", "update", lesson_id, "--status", "supported", "--ref", "fixture/b")
            self.assertEqual(updated.returncode, 0, updated.stderr)
            canonical = invoke(
                store, "lead", "add", "gap", "Delivery mailbox cannot expose unread messages",
                "--body", "agents cannot inspect unread work", "--tag", "severity-low", "--ref", "log-a",
            )
            self.assertEqual(canonical.returncode, 0, canonical.stderr)
            canonical_id = json.loads(canonical.stdout)["id"]
            duplicate = invoke(
                store, "peer", "add", "gap", "Mailbox does not expose unread deliveries",
                "--body", "agents cannot inspect unread work", "--tag", "severity-high", "--ref", "log-b",
            )
            self.assertEqual(duplicate.returncode, 0, duplicate.stderr)
            duplicate_id = json.loads(duplicate.stdout)["id"]
            merged = invoke(store, "lead", "merge", duplicate_id, "--into", canonical_id)
            self.assertEqual(merged.returncode, 0, merged.stderr)
            gap_update = invoke(
                store, "lead", "update", canonical_id, "--severity", "blocker", "--tag", "triaged",
                "--ref", "log-c",
            )
            self.assertEqual(gap_update.returncode, 0, gap_update.stderr)
            planned = invoke(
                store, "lead", "add", "gap", "Search waits for stable cursors", "--status", "planned",
                "--tag", "severity-medium",
            )
            self.assertEqual(planned.returncode, 0, planned.stderr)
            peer = invoke(store, "peer", "status", "blocked", "--step", "reply", "--question", "test question")
            self.assertEqual(peer.returncode, 0, peer.stderr)
            sent = invoke(store, "peer", "send", "lead", "receipt parity")
            self.assertEqual(sent.returncode, 0, sent.stderr)
            sequence = json.loads(sent.stdout)["seq"]
            acked = invoke(store, "lead", "ack", str(sequence))
            self.assertEqual(acked.returncode, 0, acked.stderr)
            return normalized_projections(store, "lead")

        py = lambda store, actor, *args: python_cli(store, actor, *args)
        rust = lambda store, actor, *args: rust_cli(store, actor, *args)
        python_projection = script(py, python_store)
        rust_projection = script(rust, rust_store)
        self.assertEqual(remove_wall_clock_fields(python_projection), remove_wall_clock_fields(rust_projection))
        self.assertTrue(python_projection["leases"])
        self.assertEqual(len(python_projection["agents"]), 2)
        self.assertEqual(len(python_projection["messages"]), 1)
        self.assertTrue(python_projection["records"])

    def test_mcp_tool_schemas_and_replies_match(self):
        python_store = str(self.root / "python-mcp")
        rust_store = str(self.root / "rust-mcp")
        python_command = [
            "nice", "-n", "19", "ionice", "-c3", sys.executable, "-I",
            str(HERE / "mcp.py"), "--store", python_store, "--actor", "agent-1",
        ]
        rust_command = [
            "nice", "-n", "19", "ionice", "-c3", *rust_prefix(), "commons", "mcp",
            "--store", rust_store, "--actor", "agent-1",
        ]
        self.assertEqual(mcp_transcript(python_command), mcp_transcript(rust_command))


if __name__ == "__main__":
    unittest.main()
