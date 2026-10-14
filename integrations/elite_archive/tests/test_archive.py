"""Adapter tests against a deliberately fake CLI. NOT native ZenoFCIS validation."""
from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from elite_archive import (  # noqa: E402
    ArchiveError, bounded_file, check_and_replay, cli_binary, digest, init_archive,
    offer, read_manifest, replay_archive, sweep, view, review_candidate, export_elite,
)


def prog(not_count: int, output_bit: int = 0):
    nodes = [["Input", 0]]
    for i in range(not_count):
        nodes.append(["Not", i])
    return {"inputs": [{"kind": "Bool"}], "outputs": [{"kind": "Bool"}],
            "nodes": nodes, "roots": [len(nodes) - 1 - output_bit]}


class ArchiveTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.path = Path(self.tmp.name)
        self.root = self.path / "archive"
        self.cli = self.path / "zeno-fcis"
        shutil.copy(ROOT / "tests/fake_zeno.py", self.cli)
        self.cli.chmod(0o755)
        self.original = self.file("original.zcve", prog(2))

    def file(self, name, value):
        p = self.path / name
        p.write_bytes((json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode())
        return p

    def init(self, **kwargs):
        return init_archive(self.root, self.original, self.cli, kwargs.get("spec"),
                            kwargs.get("max_input_tuples", 100),
                            kwargs.get("node_bin_width", 2), kwargs.get("byte_bin_width", 64))

    def test_two_distinct_elites_and_replay(self):
        self.init()
        small = self.file("small.zcve", prog(0))
        large = self.file("large.zcve", prog(4))
        a = offer(self.root, self.cli, small, "LLM")
        b = offer(self.root, self.cli, large, "egraph")
        self.assertEqual(a["status"], "elite")
        self.assertEqual(b["status"], "elite")
        self.assertNotEqual(a["cell"], b["cell"])
        found = view(self.root)
        self.assertEqual(found["archive_coverage"], 2)
        self.assertEqual(found["status"], "stored-evidence-not-freshly-replayed")
        verified = replay_archive(self.root, self.cli)
        self.assertEqual(verified["status"], "all-elites-replayed")
        self.assertEqual(verified["elite_count"], 2)

    def test_smaller_candidate_replaces_same_cell(self):
        self.init(node_bin_width=4, byte_bin_width=256)
        two_nodes = prog(0)
        two_nodes["nodes"].append(["Input", 0])
        two_nodes["roots"] = [1]
        bigger = self.file("bigger.zcve", two_nodes)
        smaller = self.file("smaller.zcve", prog(0))
        first = offer(self.root, self.cli, bigger, "first")
        second = offer(self.root, self.cli, smaller, "second")
        self.assertEqual(first["cell"], second["cell"])
        self.assertEqual(second["status"], "elite")
        self.assertEqual(view(self.root)["archive_coverage"], 1)
        self.assertEqual(view(self.root)["elites"][0]["sha256"], second["sha256"])
        self.assertEqual(replay_archive(self.root, self.cli)["elite_count"], 1)

    def test_rejected_counterexample_not_archived(self):
        self.init()
        bad = self.file("bad.zcve", prog(1))
        result = offer(self.root, self.cli, bad, "failed")
        self.assertEqual(result["status"], "rejected")
        self.assertEqual(result["checker_status"], "counterexample")
        self.assertEqual(view(self.root)["accepted_candidates"], 0)
        self.assertEqual(view(self.root)["attempts"], 1)

    def test_inconclusive_not_archived(self):
        self.init(max_input_tuples=1)
        good = self.file("small.zcve", prog(0))
        result = offer(self.root, self.cli, good, "too-small-cap")
        self.assertEqual(result["checker_status"], "inconclusive")
        self.assertEqual(view(self.root)["archive_coverage"], 0)

    def test_replay_failure_refused(self):
        self.init()
        candidate = self.file("small.zcve", prog(0))
        offer(self.root, self.cli, candidate, "test")
        with patch.dict(os.environ, {"FAKE_REPLAY_FAIL": "1"}):
            with self.assertRaises(ArchiveError):
                replay_archive(self.root, self.cli)

    def test_checker_claim_without_receipt_refused(self):
        self.init()
        candidate = self.file("small.zcve", prog(0))
        with patch.dict(os.environ, {"FAKE_SKIP_RECEIPT": "1"}):
            with self.assertRaisesRegex(ArchiveError, "without a receipt"):
                offer(self.root, self.cli, candidate, "test")
        self.assertEqual(len(read_manifest(self.root)["attempts"]), 0)

    def test_checker_drift_refused(self):
        self.init()
        with self.cli.open("ab") as f:
            f.write(b"\n# unrelated change\n")
        with self.assertRaisesRegex(ArchiveError, "differs from pinned checker"):
            offer(self.root, self.cli, self.file("small.zcve", prog(0)), "test")

    def test_original_drift_refused(self):
        self.init()
        (self.root / "original.zcve").write_text("corrupt", encoding="utf-8")
        with self.assertRaisesRegex(ArchiveError, "pinned original has changed"):
            view(self.root)

    def test_receipt_tampering_refused(self):
        self.init()
        candidate = self.file("small.zcve", prog(0))
        result = offer(self.root, self.cli, candidate, "test")
        target = self.root / "receipts" / (result["sha256"] + ".json")
        target.write_bytes(target.read_bytes() + b"\n")
        with self.assertRaisesRegex(ArchiveError, "receipt has changed"):
            replay_archive(self.root, self.cli)

    def test_spec_drift_refused(self):
        spec = self.path / "requirements.zeno"
        spec.write_text("reviewed requirements")
        self.init(spec=spec)
        (self.root / "spec.snapshot").write_text("other")
        with self.assertRaisesRegex(ArchiveError, "specification snapshot has changed"):
            view(self.root)

    def test_duplicate_not_claimed_fresh(self):
        self.init()
        p = self.file("small.zcve", prog(0))
        offer(self.root, self.cli, p, "test")
        result = offer(self.root, self.cli, p, "test")
        self.assertEqual(result["status"], "duplicate-recorded")
        self.assertEqual(view(self.root)["attempts"], 1)

    def test_sweep_uses_independent_transform_check(self):
        self.init()
        summary = sweep(self.root, self.cli, ["default"], 30)
        self.assertEqual(summary["results"][0]["status"], "elite")
        self.assertEqual(replay_archive(self.root, self.cli)["elite_count"], 1)

    def test_missing_file_symlink_rejected(self):
        self.init()
        link = self.path / "link.zcve"
        link.symlink_to(self.original)
        with self.assertRaisesRegex(ArchiveError, "symlinks"):
            offer(self.root, self.cli, link, "test")

    def test_lock_refuses_concurrent_writer(self):
        self.init()
        (self.root / ".writer-lock").mkdir()
        with self.assertRaisesRegex(ArchiveError, "locked"):
            offer(self.root, self.cli, self.file("small.zcve", prog(0)), "test")

    def test_empty_file_refused(self):
        self.init()
        empty = self.path / "empty.zcve"
        empty.write_bytes(b"")
        with self.assertRaisesRegex(ArchiveError, "empty candidate"):
            offer(self.root, self.cli, empty, "test")

    def test_review_and_export_requires_curated_elite(self):
        self.init()
        candidate = self.file("small.zcve", prog(0))
        a = offer(self.root, self.cli, candidate, "LLM")
        export = self.path / "out"
        with self.assertRaisesRegex(ArchiveError, "human 'preferred' review"):
            export_elite(self.root, self.cli, a["sha256"], export)
        note = self.path / "human-note.txt"
        note.write_text("Shorter and easier to explain: the function is identity.\n")
        review_candidate(self.root, a["sha256"], "preferred", note, "human")
        self.assertEqual(view(self.root)["elites"][0]["human_review"]["decision"], "preferred")
        bundle = export_elite(self.root, self.cli, a["sha256"], export)
        self.assertEqual(bundle["status"], "exported-for-manual-consideration")
        self.assertEqual((export / "candidate.zcve").read_bytes(), candidate.read_bytes())
        self.assertEqual(digest((export / "receipt.json").read_bytes()), a["receipt_sha256"])
        with self.assertRaisesRegex(ArchiveError, "already exists"):
            export_elite(self.root, self.cli, a["sha256"], export)

    def test_review_reject_export_refuses(self):
        self.init()
        a = offer(self.root, self.cli, self.file("small.zcve", prog(0)), "LLM")
        note = self.path / "note.txt"
        note.write_text("Not clear enough")
        review_candidate(self.root, a["sha256"], "rejected", note, "human")
        with self.assertRaisesRegex(ArchiveError, "human 'preferred' review"):
            export_elite(self.root, self.cli, a["sha256"], self.path / "out")

    def test_manifest_elite_forgery_refused(self):
        self.init(node_bin_width=4, byte_bin_width=256)
        larger = prog(0)
        larger["nodes"].append(["Input", 0])
        larger["roots"] = [1]
        a = offer(self.root, self.cli, self.file("first.zcve", larger), "LLM")
        b = offer(self.root, self.cli, self.file("second.zcve", prog(0)), "LLM")
        archive_file = self.root / "archive.json"
        obj = json.loads(archive_file.read_text())
        obj["cells"][b["cell"]] = a["sha256"]
        archive_file.write_text(json.dumps(obj))
        with self.assertRaisesRegex(ArchiveError, "do not equal best"):
            view(self.root)

    def test_empty_archive_not_reported_as_verified(self):
        self.init()
        with self.assertRaisesRegex(ArchiveError, "empty archive"):
            replay_archive(self.root, self.cli)

    def test_cli_entry_point(self):
        from elite_archive import main
        self.assertEqual(main(["--cli", str(self.cli), "init", "--archive", str(self.root),
                               "--original", str(self.original)]), 0)
        good = self.file("good.zcve", prog(0))
        self.assertEqual(main(["--cli", str(self.cli), "offer", "--archive", str(self.root),
                               "--candidate", str(good)]), 0)
        self.assertEqual(main(["show", "--archive", str(self.root)]), 0)
        self.assertEqual(main(["--cli", str(self.cli), "verify", "--archive", str(self.root)]), 0)


if __name__ == "__main__":
    unittest.main()
