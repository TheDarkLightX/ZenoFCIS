#!/usr/bin/env python3
"""Check the frozen local source inventory; not a signature or release receipt."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

APP=Path(__file__).resolve().parents[1]


def check_sources(app=APP):
    manifest=json.loads((app/"SOURCE_PINS.json").read_text())
    for relative,wanted in manifest["files"].items():
        path=app/relative
        if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest()!=wanted:
            raise ValueError(f"source pin differs: {relative}")
    required={"project.zeno","v2/policy.json","components/pool/project.zeno","components/pool/v2/policy.json",
              "components/balance/project.zeno","components/balance/v2/policy.json","tools/spot_reference.py",
              "tools/build_contracts.py","tools/build_composition.py","tools/replay_corpus.py","tools/native_check.py"}
    if not required <= manifest["files"].keys(): raise ValueError("authority source omitted from frozen inventory")
    for path in app.rglob("*.py"):
        if "__pycache__" not in path.parts and str(path.relative_to(app)) not in manifest["files"]:
            raise ValueError(f"unchecked Python source: {path.relative_to(app)}")
    return manifest


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app",type=Path,default=APP)
    args=parser.parse_args()
    manifest=check_sources(args.app)
    print(json.dumps({"checked_files":len(manifest["files"]),"authority":"none","status":"source pins match"}))


if __name__ == "__main__": main()
