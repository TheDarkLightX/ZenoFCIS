#!/usr/bin/env python3
"""Produce the byte counts and SHA-256 pins of the V1 compatibility baseline.

Each entry is what check_v1_compatibility.py compares: the file's length and
SHA-256. The declared closure is the baseline at the base revision: its file
list in order, the baseline commit, version, format and scope must be
unchanged. Review the listed files against the documented migration first.
"""
from __future__ import annotations

import hashlib
from pathlib import Path
import sys

import pin_producer as pins

BASELINE = "test-data/v1-compatibility/baseline.json"
ENTRY = ("path", "bytes", "sha256")


def outside_pins(baseline: dict) -> dict:
    return {**baseline, "files": [(list(item), item["path"]) for item in baseline["files"]]}


def plan(root: Path, base: str):
    baseline = pins.load(root, BASELINE)
    pins.require_same(BASELINE, outside_pins(baseline), outside_pins(pins.load_at_base(root, base, BASELINE)),
                      "file list or a non-pin field")
    if any(tuple(item) != ENTRY for item in baseline["files"]):
        raise pins.Refusal(BASELINE + ": entries must have exactly path, bytes and sha256")
    changes, entries = [], []
    for item in baseline["files"]:
        data = pins.read(root, item["path"])
        fresh = {"path": item["path"], "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
        if fresh != item:
            changes.append(pins.Change(BASELINE, item["path"], f"{item['sha256']} ({item['bytes']} bytes)",
                                       f"{fresh['sha256']} ({fresh['bytes']} bytes)"))
        entries.append(fresh)
    files = {BASELINE: pins.render({**baseline, "files": entries})} if changes else {}
    return changes, files


if __name__ == "__main__":
    sys.exit(pins.main(plan, __doc__))
