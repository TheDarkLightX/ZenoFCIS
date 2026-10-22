"""Replay the judges tutorial's displayed commands on the current source tree."""

import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


class JudgeDocumentation(unittest.TestCase):
    def test_displayed_commands_use_an_admitted_starting_example(self):
        binary = Path(os.environ.get(
            "ZENO_FCIS_CLI", str(ROOT / "target/debug/zeno-fcis"))).resolve()
        self.assertTrue(binary.is_file(), f"build the CLI before this test: {binary}")
        document = (ROOT / "docs/CONTRACT_JUDGES.md").read_text()
        blocks = re.findall(r"^```[^\n]*\n(.*?)^```", document, re.M | re.S)
        commands = [shlex.split(line) for block in blocks for line in block.splitlines()
                    if line.startswith(("zeno-fcis ", "python3 tools/instantiate_core.py "))]
        commands += [shlex.split(command) for command in re.findall(
            r"`(zeno-fcis contract scenario examples/[^`]+)`", document)]
        self.assertTrue(commands, "the tutorial must contain executable commands")
        proposed = next(block for block in blocks if block.startswith("law 9000 "))
        with tempfile.TemporaryDirectory(prefix="judge-doc-") as raw:
            work = Path(raw)
            (work / "examples").symlink_to(ROOT / "examples", target_is_directory=True)
            (work / "proposed.zeno").write_text(proposed)
            for command in commands:
                with self.subTest(command=shlex.join(command)):
                    if command[0] == "zeno-fcis":
                        command[0] = str(binary)
                    else:
                        command[:2] = [sys.executable, str(ROOT / command[1])]
                    output = subprocess.run(command, cwd=work, capture_output=True,
                                            text=True, timeout=120)
                    expected = 1 if command[1:3] == ["contract", "lemma"] else 0
                    print(json.dumps({"command": command, "exit": output.returncode,
                                      "expected": expected}))
                    self.assertEqual(output.returncode, expected,
                                     f"{shlex.join(command)}\n{output.stdout}\n{output.stderr}")
                    if command[1:3] == ["contract", "review"]:
                        packet = json.loads((work / "review.json").read_text())
                        self.assertEqual(packet["gaps"]["status"], "measured")
                        self.assertEqual(packet["authority"], "none")
            self.assertGreaterEqual(len(commands), 8)


if __name__ == "__main__":
    unittest.main()
