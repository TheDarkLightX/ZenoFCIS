import hashlib
import json
from pathlib import Path
import tempfile
import unittest

from commons import Commons, CommonsError
from telemetry import import_telemetry, loops, summarize, validate


def run(actor="builder", subject="a", verdict="refuted", second=0, command=None):
    digest = hashlib.sha256(subject.encode()).hexdigest()
    return {"schema": "zeno-fcis/telemetry/1", "timestamp": f"2026-01-01T00:00:{second:02d}Z",
        "actor": actor, "command": command or ["check"], "inputs": [{"name": "project", "sha256": digest}],
        "subject_sha256": hashlib.sha256(digest.encode()).hexdigest(), "exit_code": 0 if verdict == "accepted" else 1,
        "verdict": verdict, "work_units": None, "duration_ms": 10}


class TelemetryImportTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.store = Commons(self.root / "store", "importer")
        self.addCleanup(self.store.close)
        self.path = self.root / "input.jsonl"

    def write(self, rows):
        self.path.write_text("".join(json.dumps(row) + "\n" for row in rows))

    def test_reimport_does_not_append_events(self):
        self.write([run(), run(verdict="accepted", second=1)])
        self.assertEqual(import_telemetry(self.store, self.path)["imported"], 2)
        head = self.store.head()
        self.assertEqual(import_telemetry(self.store, self.path)["duplicates"], 2)
        self.assertEqual(self.store.head(), head)
        self.assertTrue(self.store.verify()["ok"])
        summary = loops(self.store)
        self.assertEqual(summary["commands"][0]["runs"], 2)
        self.assertEqual(summary["commands"][0]["time_to_accept_ms"], 1010)

    def test_different_actor_and_different_subject_both_split(self):
        summary = summarize([run(), run(actor="other", second=1), run(actor="other", subject="b", verdict="accepted", second=2)])
        self.assertEqual(summary["episodes"], 3)
        self.assertEqual(summary["commands"][0]["iterations_to_accept"], 1)

    def test_same_actor_and_subject_form_one_episode(self):
        summary = summarize([run(), run(verdict="accepted", second=2)])
        self.assertEqual(summary["episodes"], 1)
        self.assertEqual(summary["commands"][0]["iterations_to_accept"], 2)
        self.assertEqual(summary["commands"][0]["failure_rate"], 0.5)

    def test_incomplete_episode_has_no_acceptance(self):
        summary = summarize([run()])
        self.assertIsNone(summary["commands"][0]["iterations_to_accept"])
        self.assertIsNone(summary["commands"][0]["time_to_accept_ms"])

    def test_acceptance_belongs_to_the_command_that_reported_it(self):
        rows = [run(second=0), run(second=1, verdict="accepted", command=["prove"]),
                run(second=2, verdict="accepted")]
        commands = {row["command"]: row for row in summarize(rows)["commands"]}
        self.assertEqual(commands["check"]["iterations_to_accept"], 2)
        self.assertEqual(commands["check"]["time_to_accept_ms"], 2010)
        self.assertEqual(commands["prove"]["iterations_to_accept"], 1)
        self.assertEqual(commands["prove"]["time_to_accept_ms"], 10)
        incomplete = {row["command"]: row for row in summarize(rows[:2])["commands"]}
        self.assertIsNone(incomplete["check"]["iterations_to_accept"])
        self.assertEqual(incomplete["check"]["accepted_episodes"], 0)

    def test_malformed_line_does_not_append(self):
        self.write([{**run(), "inputs": [{"contents": "private source"}]}])
        head = self.store.head()
        with self.assertRaises(CommonsError):
            import_telemetry(self.store, self.path)
        self.assertEqual(self.store.head(), head)

    def test_partial_line_refused(self):
        self.path.write_text(json.dumps(run()))
        with self.assertRaises(CommonsError):
            import_telemetry(self.store, self.path)


if __name__ == "__main__":
    unittest.main()
