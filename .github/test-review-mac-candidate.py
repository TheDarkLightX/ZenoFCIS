import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("candidate", Path(__file__).with_name("review-mac-candidate.py"))
candidate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(candidate)


class CandidateReviewTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        (self.root / "binaries").mkdir()
        self.name = f"zeno-fcis-2.6.0-{candidate.HOST}.tar.gz"
        self.location = f"binaries/{self.name}!zeno-fcis-2.6.0-{candidate.HOST}/zeno-fcis"
        content = b"illustrative executable"
        self.binary_sha = hashlib.sha256(content).hexdigest()
        archive = self.root / "binaries" / self.name
        with tarfile.open(archive, "w:gz") as tar:
            entry = tarfile.TarInfo(f"zeno-fcis-2.6.0-{candidate.HOST}/zeno-fcis")
            entry.size = len(content)
            tar.addfile(entry, io.BytesIO(content))
        self.record = {
            "source_commit": candidate.SOURCE, "version": "2.6.0", "host": candidate.HOST,
            "deployment_target": "11.0", "executable": {"sha256": self.binary_sha},
            "archive": {"name": self.name, "sha256": hashlib.sha256(archive.read_bytes()).hexdigest()},
        }
        self.inputs = {"source_commit": candidate.SOURCE, "version": "2.6.0",
                       "host": candidate.HOST, "executable": {"sha256": self.binary_sha}}
        (self.root / "HOST-BINARY.json").write_text(json.dumps(self.record))
        (self.root / f"COMPILED-INPUTS-{candidate.HOST}.json").write_text(json.dumps(self.inputs))
        self.finding = {"code": "model-api-token", "location": self.location, "file_sha256": self.binary_sha}
        self.log = self.root / "build.log"

    def tearDown(self):
        self.tmp.cleanup()

    def report(self, finding):
        self.log.write_text(json.dumps({"schema": "zeno-fcis/release-privacy-check/1",
                                        "status": "failed", "errors": [], "findings": [finding]}))

    def test_scanner_refusal_is_only_admitted_for_private_review(self):
        self.report(self.finding)
        self.assertEqual(candidate.check(2, self.root, self.log)["qualification"], "encrypted review only")

    def test_other_findings_and_changed_binary_are_refused(self):
        for finding in ({**self.finding, "code": "private-marker"},
                        {**self.finding, "file_sha256": "0" * 64},
                        {**self.finding, "location": "other"}):
            self.report(finding)
            with self.assertRaises(ValueError):
                candidate.check(2, self.root, self.log)
        self.report(self.finding)
        with self.assertRaises(ValueError):
            candidate.check(1, self.root, self.log)


if __name__ == "__main__":
    unittest.main()
