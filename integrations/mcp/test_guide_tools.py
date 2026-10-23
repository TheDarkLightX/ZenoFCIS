import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from zeno_fcis_guide import ANNOTATIONS, GuideToolError, guide_find, guide_list, guide_show


@unittest.skipUnless(os.environ.get("ZENO_FCIS_CLI"), "set ZENO_FCIS_CLI")
class GuideTests(unittest.TestCase):
    def test_offline_list_show_find(self):
        names = [card["name"] for card in guide_list()["cards"]]
        self.assertEqual(names, sorted(names))
        self.assertEqual(len(names), 20)
        self.assertIn("contract-probe", names)
        self.assertIn("## Worked example", guide_show("check")["card"])
        first = guide_find("find modulo bias")
        self.assertEqual(first, guide_find("find modulo bias"))
        self.assertEqual(first["cards"][0]["name"], "contract-distribution")

    def test_arguments_cannot_be_commands(self):
        for name in ("../check", "--help", "check;echo bad"):
            with self.assertRaises(GuideToolError):
                guide_show(name)
        self.assertEqual(guide_find("--help")["cards"], [])

    def test_lookup_needs_no_source_tree_and_writes_no_workspace(self):
        cli = Path(os.environ["ZENO_FCIS_CLI"]).resolve()
        with tempfile.TemporaryDirectory() as directory:
            env = {key: value for key, value in os.environ.items() if key not in ("ZENO_FCIS_TELEMETRY", "ZENO_FCIS_KB")}
            result = subprocess.run([str(cli), "--no-kb", "guide", "show", "trace-game"],
                cwd=directory, env=env, capture_output=True, text=True, check=False)
            self.assertEqual(result.returncode, 0)
            self.assertIn("certified safety", result.stdout)
            self.assertEqual(list(Path(directory).iterdir()), [])
        self.assertTrue(ANNOTATIONS["readOnlyHint"])
        self.assertFalse(ANNOTATIONS["destructiveHint"])
        self.assertFalse(ANNOTATIONS["openWorldHint"])


if __name__ == "__main__":
    unittest.main()
