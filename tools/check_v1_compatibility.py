#!/usr/bin/env python3
"""Check the retained V1 consumer and foundational protocol source baseline."""

import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def check(package_roots: dict[str, Path] | None = None) -> dict:
    baseline = json.loads((ROOT / "test-data/v1-compatibility/baseline.json").read_text())
    if baseline.get("format") != "zeno-fcis/v1-compatibility/1":
        raise RuntimeError("unknown compatibility baseline")
    for item in baseline["files"]:
        relative = Path(item["path"])
        if package_roots is not None and relative.parts[0] == "crates":
            # Packaged qualification checks the extracted archive itself. It
            # must not substitute a matching checkout file for altered bytes.
            path = package_roots[relative.parts[1]].joinpath(*relative.parts[2:])
        else:
            path = ROOT / relative
        data = path.read_bytes()
        if len(data) != item["bytes"] or hashlib.sha256(data).hexdigest() != item["sha256"]:
            raise RuntimeError(f"V1 compatibility baseline changed: {item['path']}")
    return baseline


if __name__ == "__main__":
    result = check()
    print(f"V1 compatibility: {len(result['files'])} pinned source files unchanged")
