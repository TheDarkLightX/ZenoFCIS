"""Controls for the pure parts of `qualify_ocaml_consumer.py`: pins, review facts, overflow controls and reports,
and its temporary work folder."""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import qualify_ocaml_consumer as qualify

ROOT = qualify.ROOT
ENDPOINT_POLICY = ROOT / "integrations/ocaml/tests/endpoint/v2/policy.json"


def review(inputs: int = 16, examples: int = 16) -> dict:
    return {"inputs": {"construction": "full-domain", "count": inputs},
            "summary": {"examples": {"compared": examples, "disagreements": 0}, "refusals": {"count": 0},
                        "findings": 0}}


def overflow_review() -> dict:
    return {"inputs": {"construction": "full-domain", "count": 32},
            "summary": {"refusals": {"count": 16, "by_class": {"arithmetic": 16},
                                     "on_law_consistent_states": {"by_class": {"arithmetic": 16}}}}}


class PinTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="zeno-fcis-ocaml-test-")
        self.root = Path(self.temporary.name)

    def tearDown(self):
        self.temporary.cleanup()

    def test_the_committed_pins_are_well_formed(self):
        pins = qualify.read_pins()
        self.assertEqual(len(pins), 15)
        self.assertIn(Path("/usr/bin/ocamlopt.opt"), [path for _, path in pins])
        self.assertTrue(all(path.is_absolute() for _, path in pins))

    def test_malformed_pin_files_are_refused(self):
        digest = "a" * 64
        for label, text in {"empty": "", "comment": f"# pins\n{digest}  /usr/bin/x\n",
                            "relative": f"{digest}  usr/bin/x\n", "one space": f"{digest} /usr/bin/x\n",
                            "short digest": f"{digest[:63]}  /usr/bin/x\n",
                            "uppercase digest": f"{digest.upper()}  /usr/bin/x\n",
                            "twice": f"{digest}  /usr/bin/x\n{'b' * 64}  /usr/bin/x\n"}.items():
            with self.subTest(label):
                path = self.root / f"{label}.sha256"
                path.write_text(text)
                with self.assertRaises(qualify.QualificationError):
                    qualify.read_pins(path)

    def test_pin_problems_name_missing_and_changed_files(self):
        present, changed = self.root / "present", self.root / "changed"
        present.write_bytes(b"pinned")
        changed.write_bytes(b"pinned")
        digest = hashlib.sha256(b"pinned").hexdigest()
        self.assertEqual(qualify.pin_problems([(digest, present)]), [])
        changed.write_bytes(b"changed")
        self.assertEqual(qualify.pin_problems([(digest, present), (digest, changed), (digest, self.root / "gone")]),
                         [f"pinned file differs: {changed}", f"missing pinned file {self.root / 'gone'}"])


class FactTest(unittest.TestCase):
    def test_review_facts(self):
        self.assertEqual(qualify.review_problems(review(), 16, 16), [])
        self.assertEqual(qualify.review_problems(review(32), 32, 16), [])
        drifted = review()
        drifted["summary"]["refusals"]["count"] = 1
        drifted["inputs"]["construction"] = "sampled"
        self.assertEqual(len(qualify.review_problems(drifted, 16, 16)), 2)
        self.assertEqual(len(qualify.review_problems({}, 16, 16)), 6)
        self.assertTrue(qualify.review_problems(review(), 32, 16))

    def test_overflow_facts(self):
        self.assertEqual(qualify.overflow_problems(overflow_review()), [])
        for path in (("summary", "refusals", "count"), ("summary", "refusals", "by_class", "arithmetic"),
                     ("summary", "refusals", "on_law_consistent_states", "by_class", "arithmetic"),
                     ("inputs", "count")):
            with self.subTest(path):
                drifted = overflow_review()
                node = drifted
                for key in path[:-1]:
                    node = node[key]
                node[path[-1]] = 15
                self.assertEqual(len(qualify.overflow_problems(drifted)), 1)

    def test_overflow_controls_change_one_case_of_the_endpoint_policy(self):
        policy = json.loads(ENDPOINT_POLICY.read_text())
        for _, guard, field, expression in qualify.OVERFLOW_CONTROLS:
            with self.subTest(guard):
                changed = qualify.overflow_policy(policy, guard, field, expression)
                differing = [index for index, (old, new) in enumerate(zip(policy["cases"], changed["cases"]))
                             if old != new]
                self.assertEqual(len(differing), 1)
                self.assertEqual(changed["cases"][differing[0]]["post"][field], expression)
                self.assertEqual({key: value for key, value in changed.items() if key != "cases"},
                                 {key: value for key, value in policy.items() if key != "cases"})
        self.assertEqual(json.loads(ENDPOINT_POLICY.read_text()), policy)
        with self.assertRaises(qualify.QualificationError):
            qualify.overflow_policy(policy, "no such guard", "111", "high + 1")
        with self.assertRaises(qualify.QualificationError):
            qualify.overflow_policy(policy, qualify.OVERFLOW_CONTROLS[0][1], "999", "high + 1")

    def test_codec_negative_frames(self):
        frames = qualify.codec_negative_frames()
        self.assertEqual(len(frames), 18)
        self.assertEqual(frames, sorted(frames))
        self.assertFalse({frame.removesuffix(".frame") for frame in frames} & qualify.NEGATIVE_WORD_CASES)
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-ocaml-test-") as raw:
            corpus = json.loads((ROOT / "integrations/ocaml/tests/expected.json").read_text())
            corpus["native_negatives"] = corpus["native_negatives"][:-1]
            path = Path(raw) / "expected.json"
            path.write_text(json.dumps(corpus))
            with self.assertRaises(qualify.QualificationError):
                qualify.codec_negative_frames(path)

    def test_consumer_report_facts(self):
        report = {"status": "passed", "c3_tuples": 16, "endpoint_tuples": 16, "native_negatives": 24,
                  "client_negative_groups": 11, "unknown_submission_recovered": True}
        self.assertEqual(qualify.consumer_report_problems(report), [])
        self.assertEqual(len(qualify.consumer_report_problems({**report, "native_negatives": 23})), 1)
        self.assertEqual(len(qualify.consumer_report_problems({**report, "unknown_submission_recovered": False})), 1)
        self.assertEqual(len(qualify.consumer_report_problems({})), 6)


class TemporaryWorkTest(unittest.TestCase):
    def test_a_run_without_work_dir_removes_its_temporary_folder(self):
        """A missing CLI fails the preconditions after the work folder exists; nothing may remain of it."""
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-ocaml-test-") as directory:
            scratch = Path(directory)
            report = scratch / "report.json"
            completed = subprocess.run(
                [sys.executable, "-B", str(ROOT / "tools/qualify_ocaml_consumer.py"), "--cli", str(scratch / "no-cli"),
                 "--report", str(report)],
                env={**os.environ, "TMPDIR": str(scratch)}, capture_output=True, text=True, check=False, timeout=120)
            self.assertEqual(completed.returncode, 1, completed.stderr)
            self.assertEqual(sorted(path.name for path in scratch.iterdir()), ["report.json"])
            self.assertEqual(json.loads(report.read_text())["status"], "failed")


if __name__ == "__main__":
    unittest.main(verbosity=2)
