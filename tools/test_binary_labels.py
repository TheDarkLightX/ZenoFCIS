"""Adversarial controls for the narrow compiled-public-label disposition."""
import io
from contextlib import redirect_stdout
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import check_binary_labels as labels
import check_release_privacy as privacy

ROOT = Path(__file__).resolve().parents[1]
MEMBER = "zeno-fcis-2.4.0-x86_64-unknown-linux-gnu/zeno-fcis"
COMMIT = "a" * 40


def pooled():
    return b"\x7fELF\0" + b"kb-task-key" + b"x" * 50 + b"\0"


def bound(data):
    return labels.ExecutableBinding(COMMIT, "x86_64-unknown-linux-gnu", MEMBER,
                                    labels.sha(data), "b" * 64, "c" * 64, "d" * 64)


class BinaryLabelTests(unittest.TestCase):
    def scan(self, data, *, member=MEMBER, binding=None, markers=()):
        scan = labels.ArtifactScan(ROOT, [binding or bound(data)], markers)
        scan.inspect(member, data)
        return scan.disposition()

    def test_public_label_retains_raw_failure_and_exact_receipt(self):
        data = pooled()
        raw = privacy.Scan()
        raw.inspect(MEMBER, data)
        report = self.scan(data)
        self.assertEqual(report["raw_report"], raw.report())
        self.assertEqual(report["raw_report"]["status"], "failed")
        self.assertEqual(report["effective_status"], "accepted-public-labels")
        item = report["adjudications"][0]
        self.assertEqual(item["binding"]["executable_sha256"], labels.sha(data))
        self.assertEqual(len(item["matches"]), 1)
        self.assertNotIn("x" * 35, json.dumps(report))

    def test_every_reviewed_label_and_its_source_anchor_is_usable(self):
        for _, _, literals in labels.POLICY:
            for literal in literals:
                with self.subTest(literal=literal):
                    data = b"\x7fELF\0" + literal.encode() + b"x" * 50 + b"\0"
                    self.assertEqual(self.scan(data)["effective_status"], "accepted-public-labels")

    def test_rebound_artifact_with_separate_token_is_refused(self):
        data = pooled() + b"sk-" + b"Q" * 40
        self.assertEqual(self.scan(data)["effective_status"], "failed")

    def test_macos_additional_public_label_needs_its_explicit_policy_entry(self):
        data = b"\x7fELF\0kb-task-conflict" + b"x" * 60
        old = [(p, h, tuple(s for s in literals if s != "kb-task-conflict"))
               for p, h, literals in labels.POLICY]
        with patch.object(labels, "POLICY", old):
            self.assertEqual(self.scan(data)["effective_status"], "failed")
        self.assertEqual(self.scan(data)["effective_status"], "accepted-public-labels")

    def test_rebound_overlapping_token_is_refused(self):
        data = b"\x7fELF\0kb-task-key" + b"x" * 40 + b"sk-" + b"Q" * 40
        self.assertEqual(len(list(privacy.PATTERNS["model-api-token"].finditer(data))), 1)
        self.assertEqual(self.scan(data)["effective_status"], "failed")

    def test_stale_artifact_or_member_binding_is_refused(self):
        data = pooled()
        self.assertEqual(self.scan(data + b"changed", binding=bound(data))["effective_status"], "failed")
        self.assertEqual(self.scan(data, member="other/zeno-fcis")["effective_status"], "failed")

    def test_changed_or_incomplete_literal_is_refused(self):
        for literal in (b"kb-task-kez", b"kb-task-k", b"sk-" + b"Q" * 40):
            self.assertEqual(self.scan(b"\x7fELF\0" + literal + b"x" * 50)["effective_status"], "failed")

    def test_source_change_or_symlink_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name, _, _ in labels.POLICY:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes((ROOT / name).read_bytes())
            with patch.object(labels, "anchors", wraps=lambda ignored: real_anchors(root)):
                path = root / labels.POLICY[0][0]
                path.write_bytes(path.read_bytes() + b"\n")
                self.assertEqual(self.scan(pooled())["effective_status"], "failed")
                path.unlink()
                path.symlink_to(ROOT / labels.POLICY[0][0])
                self.assertEqual(self.scan(pooled())["effective_status"], "failed")

    def test_markers_and_other_categories_still_refuse(self):
        for tail, markers in ((b"/home/" + b"private-person/source", ()),
                              (b"synthetic-private-marker", (b"synthetic-private-marker",)),
                              (b"ghp_" + b"Q" * 40, ())):
            report = self.scan(pooled() + tail, markers=markers)
            self.assertEqual(report["effective_status"], "failed")
            self.assertTrue(report["remaining_findings"])

    def test_member_names_are_never_adjudicated(self):
        scan = labels.ArtifactScan(ROOT, [])
        scan.inspect("kb-task-key" + "x" * 50, b"safe")
        self.assertEqual(scan.disposition()["effective_status"], "failed")

    def test_archive_metadata_is_never_adjudicated(self):
        data = pooled()
        bundle = io.BytesIO()
        with tarfile.open(fileobj=bundle, mode="w:gz", format=tarfile.PAX_FORMAT,
                          pax_headers={"comment": "kb-task-key" + "x" * 50}) as archive:
            item = tarfile.TarInfo(MEMBER)
            item.size = len(data)
            archive.addfile(item, io.BytesIO(data))
        scan = labels.ArtifactScan(ROOT, [bound(data)])
        scan.inspect("host.tar.gz", bundle.getvalue())
        self.assertEqual(scan.disposition()["effective_status"], "failed")

    def test_archive_record_and_compiled_inputs_are_bound(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "binaries").mkdir()
            archive_path = root / "binaries/host.tar.gz"
            data = pooled()
            with tarfile.open(archive_path, mode="w:gz") as archive:
                member = tarfile.TarInfo(MEMBER)
                member.size = len(data)
                archive.addfile(member, io.BytesIO(data))
            record_path = root / "HOST-BINARY.json"
            record = {"format": "zeno-fcis/host-binary/1", "source_commit": COMMIT,
                      "host": "x86_64-unknown-linux-gnu", "archive": {
                          "name": archive_path.name, "sha256": labels.sha(archive_path.read_bytes()),
                          "prefix": MEMBER.rsplit("/", 1)[0]},
                      "executable": {"sha256": labels.sha(data), "size": len(data)}}
            record_path.write_text(json.dumps(record))
            inputs_path = root / "COMPILED-INPUTS-x86_64-unknown-linux-gnu.json"
            inputs = {"format": "zeno-fcis/compiled-inputs/1", "source_commit": COMMIT,
                      "host": record["host"], "executable": record["executable"], "unclassified": []}
            inputs_path.write_text(json.dumps(inputs))
            result = labels.binding(record_path, archive_path, COMMIT)
            self.assertEqual(result.executable_sha256, labels.sha(data))
            self.assertEqual(result.archive_sha256, record["archive"]["sha256"])
            import rc_package
            receipt = io.StringIO()
            with redirect_stdout(receipt):
                rc_package.require_artifact_privacy(root, COMMIT)
            self.assertEqual(json.loads(receipt.getvalue())["raw_report"]["status"], "failed")
            record_bytes = record_path.read_bytes()
            record_path.unlink()
            with self.assertRaises(rc_package.RcError):
                rc_package.require_artifact_privacy(root, COMMIT)
            record_path.write_bytes(record_bytes)
            for changed in ({"source_commit": "f" * 40}, {"unclassified": ["unexpected-input"]},
                            {"executable": {"sha256": "0" * 64}}):
                inputs_path.write_text(json.dumps(inputs | changed))
                with self.assertRaises(ValueError):
                    labels.binding(record_path, archive_path, COMMIT)
            inputs_path.write_text(json.dumps(inputs))
            archive_path.write_bytes(archive_path.read_bytes() + b"changed")
            with self.assertRaises(ValueError):
                labels.binding(record_path, archive_path, COMMIT)
            with self.assertRaises(ValueError):
                labels.binding(record_path, archive_path, "f" * 40)

    def test_excessive_overlapping_starts_are_refused(self):
        data = (b"kb-task-key" + b"x" * 40 + b"\0") * (labels.MAX_STARTS + 1)
        with self.assertRaisesRegex(ValueError, "token-start-limit"):
            labels.classify(data, labels.anchors(ROOT))

    def test_optimized_python_rejects_rebound_overlapping_token(self):
        script = ("import check_binary_labels as b\n"
                  "from pathlib import Path\n"
                  "d=b'kb-task-key'+b'x'*40+b'sk-'+b'Q'*40\n"
                  "try: b.classify(d,b.anchors(Path.cwd()))\n"
                  "except ValueError: raise SystemExit(0)\n"
                  "raise SystemExit(1)\n")
        env = dict(os.environ, PYTHONPATH=str(ROOT / "tools"))
        result = subprocess.run([sys.executable, "-O", "-c", script], cwd=ROOT, env=env,
                                capture_output=True, timeout=20, check=False)
        self.assertEqual(result.returncode, 0, result.stderr)


real_anchors = labels.anchors

if __name__ == "__main__":
    unittest.main()
