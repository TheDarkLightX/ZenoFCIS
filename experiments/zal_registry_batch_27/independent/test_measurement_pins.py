"""Exercise refusal of stale measurement bindings before any worker launch."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

RECIPE = None


class PinTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="zal-measurement-pin-tests-")
        self.addCleanup(self.tmp.cleanup)
        self.recipe = Path(self.tmp.name)
        for name in ("worker.py", "run_measurements.py", "owned_process.py", "FROZEN_PLAN_02.json", "MEASURE_PINS_01.json"):
            shutil.copyfile(RECIPE / name, self.recipe / name)
        shutil.copytree(RECIPE / "workloads", self.recipe / "workloads")

    def check(self, expected, descriptor_hash=None):
        descriptor = self.recipe / "MEASURE_PINS_01.json"
        digest = descriptor_hash or hashlib.sha256(descriptor.read_bytes()).hexdigest()
        command = [sys.executable, "-I", "-B", str(self.recipe / "run_measurements.py"),
                   "--pins-sha256", digest, "--check-only"]
        result = subprocess.run(command, capture_output=True, text=True, timeout=10)
        if expected is None:
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout)["status"], "pins-verified")
        else:
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(expected, result.stderr)
        self.assertFalse((self.recipe / "measures").exists())

    def test_exact_bindings_pass_without_launch(self):
        self.check(None)

    def test_wrong_descriptor_hash_refuses_without_launch(self):
        self.check("Reviewed pins changed", "0" * 64)

    def test_worker_recipe_drift_refuses_without_launch(self):
        path = self.recipe / "worker.py"
        path.write_bytes(path.read_bytes() + b"\n# drift control\n")
        self.check("Measurement helper changed: worker.py")

    def test_workload_drift_refuses_without_launch(self):
        path = self.recipe / "workloads/heldout-latch.json"
        path.write_bytes(path.read_bytes() + b" ")
        self.check("Workload changed: heldout-latch.json")

    def test_interpreter_digest_drift_refuses_without_launch(self):
        path = self.recipe / "MEASURE_PINS_01.json"
        value = json.loads(path.read_text())
        value["python"]["sha256"] = "0" * 64
        path.write_text(json.dumps(value))
        self.check("Python changed")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--recipe", required=True, type=Path)
    args = parser.parse_args()
    RECIPE = args.recipe.resolve()
    result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(PinTests))
    raise SystemExit(0 if result.wasSuccessful() else 1)
