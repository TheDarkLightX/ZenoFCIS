#!/usr/bin/env python3
"""Produce the arithmetic handoff source pin from the bound repository bytes.

The reviewed inventory (the pin's file list) is kept; only the digests and the
base revision are recomputed, using the consuming adapter's own digest. Every
bound file must equal its bytes at the named revision, so the recorded label
names bytes that exist there. Review the diff, then regenerate the requirement
projection with ontology_arithmetic_oracle.py and rerun the handoff
qualification. --check only compares the pin with the current bytes.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "integrations/zal"))
import arithmetic_handoff_tools as adapter


def git(*argv):
    return subprocess.run(["git", *argv], cwd=ROOT, capture_output=True, check=True).stdout


def produce(pin, revision):
    """The pin for the current bytes, refusing any file that differs at revision."""
    commit = git("rev-parse", "--verify", revision + "^{commit}").decode().strip()
    files = {}
    for name in pin["files"]:
        files[name] = adapter.source_digest(name)
        if adapter._read(ROOT / name) != git("show", f"{commit}:{name}"):
            raise ValueError(f"bound source differs from {commit}: {name}")
    return {**pin, "base_revision": commit, "files": files}


def render(pin):
    return json.dumps(pin, indent=2, sort_keys=True) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--revision", help="the revision whose bytes the pin records")
    mode.add_argument("--check", action="store_true")
    args = parser.parse_args()
    pin = json.loads(adapter.PIN.read_text())
    if args.check:
        stale = sorted(n for n, d in pin["files"].items() if adapter.source_digest(n) != d)
        for name in stale:
            print("stale:", name)
        print(f"source pin: {len(pin['files'])} files, {len(stale)} stale")
        return 1 if stale else 0
    fresh = produce(pin, args.revision)
    changed = sorted(n for n in pin["files"] if fresh["files"][n] != pin["files"][n])
    adapter.PIN.write_text(render(fresh))
    for name in changed:
        print("changed:", name)
    print(f"source pin: {len(fresh['files'])} files at {fresh['base_revision']}, {len(changed)} changed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
