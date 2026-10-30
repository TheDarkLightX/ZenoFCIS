"""The source pin producer records only bytes that exist at the named revision."""
from hashlib import sha256
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import record_arithmetic_handoff_sources as producer


class SourcePinProducerTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        (self.root / "bound.rs").write_bytes(b"fn bound() {}\n")
        for argv in (["init", "-q"], ["add", "bound.rs"],
                     ["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "bound"]):
            subprocess.run(["git", *argv], cwd=self.root, check=True)
        for module in (producer, producer.adapter):
            patcher = patch.object(module, "ROOT", self.root)
            patcher.start()
            self.addCleanup(patcher.stop)
        self.pin = {"schema": "zeno-fcis/arithmetic-handoff-sources/1", "repository": "r",
                    "base_revision": "0" * 40, "files": {"bound.rs": "stale"}}

    def test_records_the_full_revision_and_the_adapter_digest(self):
        fresh = producer.produce(self.pin, "HEAD")
        head = subprocess.run(["git", "rev-parse", "HEAD"], cwd=self.root, capture_output=True,
                              text=True, check=True).stdout.strip()
        self.assertEqual(fresh["base_revision"], head)
        self.assertEqual(fresh["files"], {"bound.rs": sha256(b"fn bound() {}\n").hexdigest()})
        self.assertEqual({k: v for k, v in fresh.items() if k not in ("base_revision", "files")},
                         {"schema": self.pin["schema"], "repository": "r"})

    def test_refuses_bytes_that_differ_at_the_revision(self):
        (self.root / "bound.rs").write_bytes(b"fn changed() {}\n")
        with self.assertRaisesRegex(ValueError, "bound source differs"):
            producer.produce(self.pin, "HEAD")

    def test_refuses_paths_outside_the_inventory_rules(self):
        for name in ("../bound.rs", str(self.root / "bound.rs")):
            with self.assertRaises(producer.adapter.core.NotationError):
                producer.produce({**self.pin, "files": {name: "stale"}}, "HEAD")


if __name__ == "__main__":
    unittest.main()
