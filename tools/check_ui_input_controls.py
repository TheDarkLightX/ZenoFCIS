#!/usr/bin/env python3
"""Plant each UI input defect in a copy, check its named test fails, then restore.

The checkout itself is never changed.
"""
from __future__ import annotations
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
CONTROLS = [
    ("invisible-characters-accepted", "tools/zenoui.py", ["python3", "tools/test_zenoui_text.py"], "invisible character accepted",
     "            if invisible(character):", "            if False and invisible(character):"),
    ("metadata-not-checked", "tools/zenoui.py", ["python3", "tools/test_zenoui_text.py"], "the build did not check ui.json text",
     "    visible(json.loads(metadata.read_text(encoding=\"utf-8\"), object_pairs_hook=unique))",
     "    json.loads(metadata.read_text(encoding=\"utf-8\"), object_pairs_hook=unique)"),
    ("label-invisible-accepted", "site/public/zenoui/app-contract.js", ["node", "site/tests/zenoui-label-text.test.mjs"],
     "invisible character accepted",
     "/[<>\\p{Cc}\\p{Cf}\\p{Cs}\\p{Co}\\p{Zl}\\p{Zp}\\u034f\\u115f\\u1160\\u17b4\\u17b5\\u180b-\\u180f\\u2065\\u3164\\ufe00-\\ufe0f\\uffa0\\ufff0-\\ufff8\\u{E0000}-\\u{E0FFF}]/u",
     "/[<>\\x00-\\x1f]/"),
]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    rows = []
    with tempfile.TemporaryDirectory(prefix="ui-text-controls-") as directory:
        source = Path(directory) / "source"
        shutil.copytree(ROOT, source, ignore=shutil.ignore_patterns(".git", "target", "node_modules", "__pycache__"))

        def run(command):
            result = subprocess.run(command, cwd=source, capture_output=True, text=True, timeout=300)
            return result.returncode, result.stdout + result.stderr

        for name, path, command, marker, before, after in CONTROLS:
            code, log = run(command)
            if code:
                raise RuntimeError(f"baseline failed: {name}\n{log[-2000:]}")
            target = source / path
            original = target.read_text()
            if original.count(before) != 1:
                raise RuntimeError(f"nonunique mutation anchor: {name}")
            target.write_text(original.replace(before, after))
            mutant, log = run(command)
            target.write_text(original)
            restored, _ = run(command)
            killed = mutant != 0 and marker in log
            row = dict(control=name, check=" ".join(command), marker=marker, mutant=mutant, baseline=restored, killed=killed)
            rows.append(row)
            args.out.write_text(json.dumps(rows, indent=2) + "\n")
            print(json.dumps(row), flush=True)
            if not killed or restored:
                raise RuntimeError(f"control did not meet fail/baseline obligation: {name}\n{log[-2000:]}")
    print(f"ui input controls: PASS ({len(rows)} planted defects; named failures; restored baselines)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
