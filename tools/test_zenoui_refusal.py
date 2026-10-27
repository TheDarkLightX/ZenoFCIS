"""Developer refusals preserve the oracle's reason without a Python traceback."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


class OracleRefusalTests(unittest.TestCase):
    def test_u128_refusal_from_real_child_process_is_one_line(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "ui.json").write_text(json.dumps({}))
            oracle = root / "oracle"
            oracle.write_text("#!/bin/sh\nprintf '%s\\n' 'ContractError scalar consumer binding U128SubjectUnsupported' >&2\nexit 1\n")
            oracle.chmod(0o700)
            result = subprocess.run(
                ["nice", "-n", "19", "ionice", "-c3", sys.executable,
                 str(Path(__file__).with_name("zenoui.py")), str(root),
                 "--oracle", str(oracle), "--out", str(root / "application")],
                capture_output=True, text=True,
            )
            self.assertEqual(result.returncode, 1)
            self.assertEqual(result.stderr, "UI build refused: UiOracleRefused:U128SubjectUnsupported\n")
            self.assertFalse((root / "application").exists())


if __name__ == "__main__":
    unittest.main()
