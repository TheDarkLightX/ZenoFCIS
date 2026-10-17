"""A native mutation campaign must never replace the caller's Cargo artifacts."""
from pathlib import Path
from types import SimpleNamespace
import tempfile
import unittest
from unittest.mock import patch

import checker_native_controls as C


class CargoIsolation(unittest.TestCase):
    def exercise(self, crash):
        with tempfile.TemporaryDirectory() as temporary:
            work = Path(temporary)
            root, cache, scratch = work / "source", work / "cache", work / "scratch"
            for path in (root, cache / "debug", scratch):
                path.mkdir(parents=True)
            source = Path("fixture.rs")
            (root / source).write_text("original")
            caller_binary = cache / "debug/zeno-fcis"
            caller_binary.write_bytes(b"ordinary build")
            environment = {"CARGO_TARGET_DIR": str(cache), "TMPDIR": str(scratch)}
            before = dict(environment)
            targets = []

            def run_logged(command, cwd, env, out):
                target = Path(env["CARGO_TARGET_DIR"])
                targets.append(target)
                (target / "debug").mkdir(parents=True, exist_ok=True)
                (target / "debug/zeno-fcis").write_bytes(b"mutation build")
                if crash:
                    raise RuntimeError("synthetic interrupted control")
                return SimpleNamespace(returncode=101), None

            with patch.object(C, "source_files", return_value=(source,)), \
                 patch.object(C, "NATIVE_TARGETS", {"P1_probe": ("--bin", "zeno-fcis", "probe")}), \
                 patch.object(C, "native_result", return_value=True):
                args = (root, work / "out", environment,
                        {"P1_probe": (source, "changed", "native")}, run_logged)
                if crash:
                    with self.assertRaisesRegex(RuntimeError, "synthetic interrupted"):
                        C.run_controls(*args)
                else:
                    self.assertTrue(C.run_controls(*args)[0]["killed"])
            self.assertEqual(caller_binary.read_bytes(), b"ordinary build")
            self.assertEqual(environment, before)
            self.assertEqual((root / source).read_text(), "original")
            self.assertTrue(targets)
            self.assertTrue(all(target != cache and target.parent == cache for target in targets))
            self.assertTrue(all(not target.exists() for target in targets))

    def test_passed_campaign_preserves_the_callers_binary_and_reclaims_private_cache(self):
        self.exercise(False)

    def test_interrupted_campaign_preserves_the_callers_binary_and_reclaims_private_cache(self):
        self.exercise(True)


if __name__ == "__main__":
    unittest.main()
