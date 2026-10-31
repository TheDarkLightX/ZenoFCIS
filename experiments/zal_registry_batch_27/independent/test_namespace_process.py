"""Executable controls for the supplemental namespace-aware ownership helper."""
from pathlib import Path
import subprocess
import sys
import time
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import namespace_process as P


class OwnershipTests(unittest.TestCase):
    def test_completed_worker_uses_no_cleanup_signal(self):
        child = subprocess.Popen([sys.executable, "-I", "-B", "-c", "import time;time.sleep(.1)"])
        owner = P.Owned(child)
        try:
            self.assertEqual(P.wait_owned(child, owner, 5), 0)
        finally:
            result = owner.finish()
        self.assertEqual(result["signals"], [])
        self.assertEqual(result["survivors"], [])

    def test_deadline_cleans_observed_descendant_and_leaves_unrelated_process_alive(self):
        sentinel = subprocess.Popen([sys.executable, "-c", "import time;time.sleep(15)"])
        child = subprocess.Popen([sys.executable, "-c",
            "import subprocess,sys,time;subprocess.Popen([sys.executable,'-c','import time;time.sleep(15)']);time.sleep(15)"])
        owner = P.Owned(child)
        try:
            try:
                deadline = time.monotonic() + 3
                while len(owner.entries) < 2 and time.monotonic() < deadline:
                    owner.scan()
                    time.sleep(.02)
                self.assertEqual(len(owner.entries), 2)
                with self.assertRaises(TimeoutError):
                    P.wait_owned(child, owner, .1)
            finally:
                result = owner.finish()
            self.assertEqual(result["survivors"], [])
            self.assertIsNone(sentinel.poll())
            self.assertTrue(all(action["proc_pid"] in owner.entries for action in result["signals"]))
        finally:
            sentinel.terminate()
            sentinel.wait(timeout=3)

    def test_mapping_failure_kills_only_the_already_opened_child_pidfd(self):
        child = subprocess.Popen([sys.executable, "-c", "import time;time.sleep(15)"])
        with patch.object(P, "fd_target", return_value=-1):
            with self.assertRaisesRegex(RuntimeError, "cannot bind owned child"):
                P.Owned(child)
        self.assertIsNotNone(child.poll())


if __name__ == "__main__":
    unittest.main(verbosity=2)
