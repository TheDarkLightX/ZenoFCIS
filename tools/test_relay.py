#!/usr/bin/env python3
"""Tests for the reference relay and its test receiver.

The store here is a stand-in: a JSON file and a small endpoint script with
the shell's export format and its three named acknowledgment refusals. The
same relay runs against the real SQLite shell in `check_app_journey.py`, and
the shell's own refusals are tested in
`crates/zeno-fcis-shell-sqlite/tests/relay.rs`.
"""

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tempfile
import textwrap
import time
import unittest
import urllib.error
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import relay  # noqa: E402
import relay_receiver  # noqa: E402

RELAY = Path(__file__).resolve().parent / "relay.py"

ENDPOINT = textwrap.dedent('''\
    import hashlib, json, os, sys
    store = sys.argv[1]
    data = json.load(open(store))
    if sys.argv[2] == "export":
        for d in data:
            if not d["acknowledged"]:
                print(json.dumps({k: v for k, v in d.items() if k != "acknowledged"}))
        sys.exit(0)
    key, digest = sys.argv[3], sys.argv[4]
    match = [d for d in data if d["delivery_id"] == key]
    if not match:
        sys.exit(print("UnknownDelivery", file=sys.stderr) or 1)
    if match[0]["acknowledged"]:
        sys.exit(print("AlreadyAcknowledged", file=sys.stderr) or 1)
    if hashlib.sha256(bytes.fromhex(match[0]["payload"])).hexdigest() != digest:
        sys.exit(print("PayloadMismatch", file=sys.stderr) or 1)
    match[0]["acknowledged"] = True
    json.dump(data, open(store + ".tmp", "w"))
    os.replace(store + ".tmp", store)
    print(json.dumps({"status": "acknowledged"}))
''')


def delivery(index: int) -> dict:
    payload = bytes([index, 0xAB, index * 3 % 256])
    return {"schema": relay.EXPORT_SCHEMA, "commit": index + 1, "lane": 1, "ordinal": 0,
            "delivery_id": hashlib.sha256(b"id%d" % index).hexdigest(), "channel": 300,
            "destination_root": 7, "payload_root": 8, "destination": "0102",
            "payload": payload.hex(), "payload_sha256": hashlib.sha256(payload).hexdigest(),
            "entry_hash": hashlib.sha256(b"entry%d" % index).hexdigest(), "acknowledged": False}


class Store:
    def __init__(self, directory: Path, count: int):
        self.path = directory / "store.json"
        self.path.write_text(json.dumps([delivery(i) for i in range(count)]))
        script = directory / "endpoint.py"
        script.write_text(ENDPOINT)
        self.export = f"{sys.executable} {script} {self.path} export"
        self.ack = f"{sys.executable} {script} {self.path} ack"

    def ids(self) -> list[str]:
        return [d["delivery_id"] for d in json.loads(self.path.read_text())]

    def pending(self) -> list[str]:
        return [d["delivery_id"] for d in json.loads(self.path.read_text()) if not d["acknowledged"]]


def run_relay(store: Store, target: list[str], *extra: str) -> tuple[int, dict]:
    completed = subprocess.run(
        [sys.executable, str(RELAY), "--export-command", store.export, "--ack-command", store.ack,
         *target, "--backoff", "0.01", "--max-backoff", "0.05", "--timeout", "5", *extra],
        check=False, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    log = json.loads(completed.stdout) if completed.stdout.strip() else {}
    return completed.returncode, log


class RelayTest(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory(prefix="zeno-fcis-relay-test-")
        self.root = Path(self.directory.name)
        self.record = self.root / "ledger.json"

    def tearDown(self) -> None:
        self.directory.cleanup()

    def assert_each_effect_once(self, store: Store) -> dict:
        ledger = relay_receiver.ledger(self.record)
        self.assertEqual([e["key"] for e in ledger["effects"]], store.ids())
        self.assertEqual(ledger["conflicts"], 0)
        for effect in ledger["effects"]:
            sent = json.loads(effect["body"])
            self.assertEqual(sent["delivery_id"], effect["key"])
            self.assertEqual(hashlib.sha256(bytes.fromhex(sent["payload"])).hexdigest(),
                             sent["payload_sha256"])
        self.assertEqual(store.pending(), [])
        return ledger

    def test_repository_import_retains_its_namespace_and_entrypoint(self) -> None:
        from unittest import mock
        self.assertEqual(Path(relay.__file__).resolve(), RELAY.resolve())
        self.assertIs(relay.send.__globals__, vars(relay))
        args = relay.arguments(["--export-command", "x", "--ack-command", "y",
                                "--http", "http://127.0.0.1/"])
        item = delivery(0)
        with mock.patch.object(relay, "post") as post:
            self.assertIsNone(relay.send(args, item, b"body", {"attempts": {}}))
        post.assert_called_once_with(args.http, item, b"body", args.timeout)

    def test_packaged_relay_runs_without_the_repository_launcher(self) -> None:
        template = RELAY.parent.parent / "crates/zeno-fcis-cli/contract-app/tools/relay.py"
        worker = self.root / "relay.py"
        worker.write_bytes(template.read_bytes())
        config = self.root / "relay.json"
        config.write_text(json.dumps({"schema": "zeno-fcis/relay-destination/1",
                                      "queue": "sent.jsonl"}))
        item = {k: v for k, v in delivery(0).items() if k != "acknowledged"}
        completed = subprocess.run([sys.executable, str(worker), "--send-one", str(config)],
                                   input=json.dumps(item), capture_output=True, text=True,
                                   cwd=self.root, timeout=10)
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual((self.root / "sent.jsonl").read_bytes(), relay.body(item))

    def test_drains_every_delivery_once_in_order(self) -> None:
        store = Store(self.root, 3)
        with relay_receiver.Running(self.record) as url:
            code, log = run_relay(store, ["--http", url])
        self.assertEqual((code, log["status"]), (0, "drained"))
        self.assertEqual(log["acknowledged"], store.ids())
        ledger = self.assert_each_effect_once(store)
        self.assertEqual(set(ledger["requests"].values()), {1})

    def test_a_crash_at_every_step_then_a_restart_loses_no_delivery(self) -> None:
        for point in relay.CRASH_POINTS:
            for count in (1, 2, 3):
                with self.subTest(point=point, count=count):
                    self.record.unlink(missing_ok=True)
                    store = Store(self.root, 3)
                    with relay_receiver.Running(self.record) as url:
                        code, _ = run_relay(store, ["--http", url], "--crash-at", f"{point}:{count}")
                        reached = point != "after-export" or count == 1
                        self.assertEqual(code, relay.CRASH_EXIT if reached else 0)
                        code, log = run_relay(store, ["--http", url])
                        self.assertEqual((code, log["status"]), (0, "drained"))
                    ledger = self.assert_each_effect_once(store)
                    # A crash after a send and before its acknowledgment
                    # repeats exactly that send, with the same key and body.
                    repeats = {k: n for k, n in ledger["requests"].items() if n > 1}
                    if point == "after-send":
                        self.assertEqual(repeats, {store.ids()[count - 1]: 2})
                    else:
                        self.assertEqual(repeats, {})

    def test_transient_failures_are_retried_with_the_same_identity(self) -> None:
        store = Store(self.root, 2)
        with relay_receiver.Running(self.record, "--fail-first", "3") as url:
            code, log = run_relay(store, ["--http", url])
        self.assertEqual(code, 0)
        self.assertEqual(log["attempts"], {store.ids()[0]: 4, store.ids()[1]: 1})
        self.assert_each_effect_once(store)

    def test_a_receiver_timeout_leaves_the_delivery_pending(self) -> None:
        store = Store(self.root, 2)
        with relay_receiver.Running(self.record, "--hang-first", "100", "--hang-seconds", "3",
                                    "--hang-mode", "before") as url:
            code, log = run_relay(store, ["--http", url], "--attempts", "2", "--timeout", "0.5")
        self.assertEqual((code, log["status"]), (3, "pending"))
        self.assertEqual(log["delivery_id"], store.ids()[0])
        self.assertIn("timed out", log["error"])
        self.assertEqual(store.pending(), store.ids())
        self.assertEqual(relay_receiver.ledger(self.record)["effects"], [])
        # A response lost after the effect: the retry finds a duplicate, and
        # the receiver records one effect.
        with relay_receiver.Running(self.record, "--hang-first", "1", "--hang-seconds", "3") as url:
            code, log = run_relay(store, ["--http", url], "--timeout", "0.5")
        self.assertEqual(code, 0)
        self.assertGreaterEqual(log["attempts"][store.ids()[0]], 2)
        self.assert_each_effect_once(store)

    def test_the_timeout_bounds_the_whole_call_not_each_read(self) -> None:
        # The receiver records the effect, then sends its answer a byte at a
        # time over 4 s: no single read waits longer than the 0.5 s timeout,
        # but the call as a whole does, so it must count as timed out.
        store = Store(self.root, 1)
        with relay_receiver.Running(self.record, "--hang-first", "100", "--hang-seconds", "4",
                                    "--hang-mode", "drip") as url:
            started = time.monotonic()
            code, log = run_relay(store, ["--http", url], "--attempts", "2", "--timeout", "0.5")
            elapsed = time.monotonic() - started
        self.assertEqual((code, log["status"]), (3, "pending"), log)
        self.assertIn("timed out", log["error"])
        self.assertEqual(log["attempts"], {store.ids()[0]: 2})
        # Two attempts of 0.5 s, a 0.01 s backoff and the store commands.
        self.assertLess(elapsed, 3.0)
        self.assertEqual(store.pending(), store.ids())
        # The cut-off call reached the receiver; a later run sends it again
        # under the same key and the receiver records one effect.
        with relay_receiver.Running(self.record) as url:
            code, log = run_relay(store, ["--http", url])
        self.assertEqual(code, 0)
        self.assert_each_effect_once(store)

    def test_a_hung_store_command_is_killed(self) -> None:
        store = Store(self.root, 1)
        hang = f"{sys.executable} -c 'import time; time.sleep(30)'"
        for flag, status in (("--export-command", "export-failed"),
                             ("--ack-command", "acknowledgment-refused")):
            with self.subTest(flag), relay_receiver.Running(self.record) as url:
                commands = {"--export-command": store.export, "--ack-command": store.ack, flag: hang}
                started = time.monotonic()
                completed = subprocess.run(
                    [sys.executable, str(RELAY), *[x for kv in commands.items() for x in kv],
                     "--http", url, "--command-timeout", "1"],
                    check=False, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                self.assertLess(time.monotonic() - started, 10)
                log = json.loads(completed.stdout)
                self.assertEqual((completed.returncode, log["status"]), (4, status))
                self.assertIn("timed out", log["error"])
                self.assertEqual(store.pending(), store.ids())

    def test_the_receiver_refuses_a_repeated_key_with_another_body(self) -> None:
        store = Store(self.root, 1)
        with relay_receiver.Running(self.record) as url:
            code, _ = run_relay(store, ["--http", url])
            self.assertEqual(code, 0)
            key = store.ids()[0]
            request = urllib.request.Request(url, data=b"{}", method="POST",
                                             headers={"Idempotency-Key": key})
            with self.assertRaises(urllib.error.HTTPError) as refused:
                urllib.request.urlopen(request, timeout=5)
            self.assertEqual(refused.exception.code, 409)
            refused.exception.close()
            unkeyed = urllib.request.Request(url, data=b"{}", method="POST")
            with self.assertRaises(urllib.error.HTTPError) as missing:
                urllib.request.urlopen(unkeyed, timeout=5)
            self.assertEqual(missing.exception.code, 400)
            missing.exception.close()
        ledger = relay_receiver.ledger(self.record)
        self.assertEqual((len(ledger["effects"]), ledger["conflicts"]), (1, 1))

    def test_a_final_refusal_stops_the_relay_and_leaves_the_delivery_pending(self) -> None:
        store = Store(self.root, 2)
        with relay_receiver.Running(self.record) as url:
            first = delivery(0)
            request = urllib.request.Request(url, data=b"other", method="POST",
                                             headers={"Idempotency-Key": first["delivery_id"]})
            urllib.request.urlopen(request, timeout=5).close()
            code, log = run_relay(store, ["--http", url])
        self.assertEqual((code, log["status"]), (3, "refused"))
        self.assertIn("409", log["error"])
        self.assertEqual(store.pending(), store.ids())

    def test_queue_file_appends_repeat_the_same_line_after_a_crash(self) -> None:
        store = Store(self.root, 2)
        queue = self.root / "queue.jsonl"
        code, _ = run_relay(store, ["--queue", str(queue)], "--crash-at", "after-send:1")
        self.assertEqual(code, relay.CRASH_EXIT)
        code, log = run_relay(store, ["--queue", str(queue)])
        self.assertEqual((code, log["status"]), (0, "drained"))
        lines = queue.read_text().splitlines()
        keys = [json.loads(line)["delivery_id"] for line in lines]
        self.assertEqual(keys, [store.ids()[0], *store.ids()])
        self.assertEqual(lines[0], lines[1])
        self.assertEqual(store.pending(), [])

    def test_a_redirect_is_not_a_delivery(self) -> None:
        # urllib would follow a 301, 302 or 303 with a GET that has no body;
        # a 200 to that GET must not acknowledge the delivery.
        import threading
        from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
        seen: list[tuple[str, str]] = []

        class Redirecting(BaseHTTPRequestHandler):
            def log_message(self, *_: object) -> None:
                pass

            def do_POST(self) -> None:  # noqa: N802
                self.rfile.read(int(self.headers.get("Content-Length", "0")))
                seen.append(("POST", self.path))
                self.send_response(int(self.path.rsplit("/", 1)[1]))
                self.send_header("Location", "/moved")
                self.send_header("Content-Length", "0")
                self.end_headers()

            def do_GET(self) -> None:  # noqa: N802
                seen.append(("GET", self.path))
                self.send_response(200)
                self.send_header("Content-Length", "2")
                self.end_headers()
                self.wfile.write(b"ok")

        server = ThreadingHTTPServer(("127.0.0.1", 0), Redirecting)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        try:
            for code in (301, 302, 303, 307, 308):
                with self.subTest(code):
                    seen.clear()
                    store = Store(self.root, 1)
                    url = f"http://127.0.0.1:{server.server_address[1]}/deliveries/{code}"
                    status, log = run_relay(store, ["--http", url])
                    self.assertEqual((status, log["status"]), (3, "refused"))
                    self.assertIn(str(code), log["error"])
                    self.assertEqual(log["acknowledged"], [])
                    self.assertEqual(store.pending(), store.ids())
                    self.assertEqual(seen, [("POST", f"/deliveries/{code}")])
        finally:
            server.shutdown()
            server.server_close()

    def test_a_short_queue_append_leaves_no_merged_line(self) -> None:
        from unittest import mock
        queue = self.root / "queue.jsonl"
        line = b'{"delivery_id":"' + b"a" * 64 + b'","payload":"00"}\n'
        real_write = relay.os.write
        calls = {"n": 0}

        def short_first(descriptor: int, data: bytes) -> int:
            calls["n"] += 1
            return real_write(descriptor, data[:10] if calls["n"] == 1 else data)

        args = relay.arguments(["--export-command", "x", "--ack-command", "y",
                                "--queue", str(queue), "--backoff", "0", "--max-backoff", "0"])
        with mock.patch.object(relay.os, "write", short_first):
            self.assertIsNone(relay.send(args, {"delivery_id": "a" * 64}, line, {"attempts": {}}))
        self.assertEqual(calls["n"], 2)
        self.assertEqual(queue.read_bytes(), line)

    def test_an_append_after_a_cut_write_starts_a_new_line(self) -> None:
        store = Store(self.root, 2)
        queue = self.root / "queue.jsonl"
        queue.write_bytes(b'{"delivery_id":"ab')   # a writer stopped part-way
        code, log = run_relay(store, ["--queue", str(queue)])
        self.assertEqual((code, log["status"]), (0, "drained"))
        lines = queue.read_text().splitlines()
        self.assertEqual(lines[0], '{"delivery_id":"ab')
        whole = []
        for text in lines:
            try:
                whole.append(json.loads(text)["delivery_id"])
            except json.JSONDecodeError:
                continue
        self.assertEqual(whole, store.ids())

    def test_a_corrupted_export_sends_nothing(self) -> None:
        store = Store(self.root, 2)
        data = json.loads(store.path.read_text())
        data[1]["payload"] = "00"
        store.path.write_text(json.dumps(data))
        with relay_receiver.Running(self.record) as url:
            code, log = run_relay(store, ["--http", url])
        self.assertEqual((code, log["status"]), (2, "malformed-export"))
        self.assertIn("SHA-256", log["error"])
        self.assertEqual(relay_receiver.ledger(self.record)["requests"], {})

    def test_an_acknowledgment_refused_by_the_store_stops_the_relay(self) -> None:
        store = Store(self.root, 2)
        data = json.loads(store.path.read_text())
        # The store's payload changes after export would be a mismatch; model
        # it by an endpoint whose stored payload differs from the exported one.
        endpoint = self.root / "endpoint.py"
        endpoint.write_text(ENDPOINT.replace('hashlib.sha256(bytes.fromhex(match[0]["payload"]))',
                                             'hashlib.sha256(b"stored")'))
        with relay_receiver.Running(self.record) as url:
            code, log = run_relay(store, ["--http", url])
        self.assertEqual((code, log["status"]), (4, "acknowledgment-refused"))
        self.assertIn("PayloadMismatch", log["error"])
        self.assertEqual(json.loads(store.path.read_text()), data)

    def test_a_repeated_acknowledgment_counts_as_done(self) -> None:
        store = Store(self.root, 1)
        data = json.loads(store.path.read_text())
        endpoint = self.root / "endpoint.py"
        # Export still lists the entry, although another relay acknowledged it.
        data[0]["acknowledged"] = True
        store.path.write_text(json.dumps(data))
        endpoint.write_text(ENDPOINT.replace('if not d["acknowledged"]:', 'if True:'))
        with relay_receiver.Running(self.record) as url:
            code, log = run_relay(store, ["--http", url])
        self.assertEqual((code, log["acknowledged"]), (0, store.ids()))

    def test_parse_export_refuses_malformed_lines(self) -> None:
        good = {k: v for k, v in delivery(0).items() if k != "acknowledged"}
        self.assertEqual(relay.parse_export(json.dumps(good)), [good])
        cases = {
            "not json": "{",
            "missing field": json.dumps({k: v for k, v in good.items() if k != "channel"}),
            "extra field": json.dumps({**good, "extra": 1}),
            "wrong type": json.dumps({**good, "commit": "1"}),
            "wrong schema": json.dumps({**good, "schema": "other"}),
            "upper-case id": json.dumps({**good, "delivery_id": good["delivery_id"].upper()}),
            "odd hex": json.dumps({**good, "payload": "abc"}),
            "repeated id": "\n".join([json.dumps(good)] * 2),
            "out of order": "\n".join([
                json.dumps({**good, "commit": 2}),
                json.dumps({k: v for k, v in delivery(1).items() if k != "acknowledged"}
                           | {"commit": 1})]),
        }
        for name, text in cases.items():
            with self.subTest(name), self.assertRaises(relay.ExportError):
                relay.parse_export(text)

    def test_every_attempt_sends_the_same_bytes(self) -> None:
        exported = {k: v for k, v in delivery(4).items() if k != "acknowledged"}
        self.assertEqual(relay.body(exported), relay.body(dict(reversed(exported.items()))))
        self.assertEqual(relay.sent_payload_sha256(relay.body(exported)), exported["payload_sha256"])

    def test_arguments_are_checked(self) -> None:
        base = ["--export-command", "x", "--ack-command", "y", "--http", "http://h"]
        for extra in (["--attempts", "0"], ["--crash-at", "nowhere"], ["--crash-at", "after-send:0"],
                      ["--timeout", "0"], ["--command-timeout", "0"], ["--queue", "q"],
                      ["--http", "ftp://h/x"], ["--http", "http:///x"], ["--http", "http://h:99999"]):
            with self.subTest(extra), self.assertRaises(SystemExit):
                relay.arguments([*base, *extra])
        self.assertEqual(relay.arguments([*base, "--crash-at", "after-send:2"]).crash_at,
                         "after-send:2")


if __name__ == "__main__":
    unittest.main()
