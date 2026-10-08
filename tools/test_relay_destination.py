"""The generated-app relay's closed configuration and one-delivery worker."""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import relay
from test_relay import delivery


class DestinationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="zeno-fcis-destination-")
        self.root = Path(self.temporary.name)
        self.config = self.root / "relay.json"
        self.source = delivery(0)
        del self.source["acknowledged"]

    def tearDown(self):
        self.temporary.cleanup()

    def write(self, **overrides):
        self.config.write_text(json.dumps({"schema": "zeno-fcis/relay-destination/1",
                                          "queue": "queue.jsonl", **overrides}))

    def run_worker(self, source=None):
        return subprocess.run([sys.executable, str(Path(relay.__file__)), "--send-one", str(self.config)],
                              input=json.dumps(self.source if source is None else source) + "\n",
                              text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=10)

    def test_queue_receives_the_exact_canonical_body_relative_to_configuration(self):
        self.write()
        result = self.run_worker()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.root / "queue.jsonl").read_bytes(), relay.body(self.source))

    def test_crash_after_send_does_not_report_success_and_restart_keeps_the_same_id(self):
        self.write(crash_at="after-send")
        self.assertEqual(self.run_worker().returncode, 75)
        self.write()
        self.assertEqual(self.run_worker().returncode, 0)
        lines = (self.root / "queue.jsonl").read_text().splitlines()
        self.assertEqual(len(lines), 2)  # Queue transport is at least once.
        self.assertEqual(lines[0], lines[1])

    def test_malformed_export_cannot_reach_the_queue(self):
        self.write()
        result = self.run_worker({**self.source, "payload_sha256": "0" * 64})
        self.assertEqual(result.returncode, 3)
        self.assertIn("payload's SHA-256", result.stderr)
        self.assertFalse((self.root / "queue.jsonl").exists())

    def test_invalid_limits_and_unknown_settings_are_refused_without_a_send(self):
        for invalid in ({"timeout": float("nan")}, {"timeout": float("inf")},
                        {"attempts": True}, {"attempts": 1.5}, {"attempts": 9},
                        {"timeout": 0}, {"backoff": -1}, {"command": "anything"},
                        {"http": "http://127.0.0.1"}, {"crash_at": "after-acknowledge"}):
            with self.subTest(invalid=invalid):
                self.write(**invalid)
                result = self.run_worker()
                self.assertEqual(result.returncode, 3, result.stderr)
                self.assertFalse((self.root / "queue.jsonl").exists())

    def test_duplicate_configuration_keys_are_refused(self):
        self.config.write_text('{"schema":"zeno-fcis/relay-destination/1",'
                               '"queue":"one","queue":"two"}')
        result = self.run_worker()
        self.assertEqual(result.returncode, 3)
        self.assertIn("duplicate", result.stderr)


if __name__ == "__main__":
    unittest.main()
