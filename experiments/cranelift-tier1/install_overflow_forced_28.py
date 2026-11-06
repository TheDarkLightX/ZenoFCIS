#!/usr/bin/env python3
"""One semantics-preserving *forced* predicate experiment, NOT upstream advice.

The earlier exact-match wrong-value canary proves the matcher can observably
affect these Rust workloads. Distinguish whether a *correct* RHS with explicit
subsume changes the native code. This is a research cost/soundness experiment,
not a recommended directional optimization.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import subprocess
from pathlib import Path
from install_isle_candidate import UPSTREAM, SOURCE, ORIGINAL_GIT_BLOB
from install_rust_overflow_predicate_28 import RULE, RULE_NAME, original

MODES = ("baseline", "unforced", "forced")
NEW_NAME = RULE_NAME + "_forced"
START = "\n  (slt $I64\n"
if RULE.count(START) != 1 or not RULE.rstrip().endswith("(all_zero $I64)))"):
    raise RuntimeError("original proven rule grammar changed")
FORCED_RULE = (RULE.replace(RULE_NAME, NEW_NAME, 1)
                  .replace(START, "\n  (subsume (slt $I64\n", 1)
                  .rstrip() + ")\n")
if FORCED_RULE.count("(subsume ") != 1 or FORCED_RULE.count("(rule ") != 1:
    raise RuntimeError("forced rule is malformed")


def digest(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def source(upstream: Path, mode: str) -> str:
    if mode not in MODES:
        raise ValueError("out-of-vocabulary compiler mode")
    return original(upstream) + {
        "baseline": "", "unforced": RULE, "forced": FORCED_RULE}[mode]


def install(upstream: Path, mode: str) -> dict:
    if mode not in MODES:
        raise ValueError("unsupported compiler mode")
    path = upstream / SOURCE
    permitted = {source(upstream, m) for m in MODES}
    if path.read_text() not in permitted:
        raise ValueError("unexpected source contents before transition")
    diff = subprocess.check_output(
        ["git", "-C", str(upstream), "diff", "--name-only"], text=True).splitlines()
    if diff not in ([], [SOURCE]):
        raise ValueError("other tracked files changed in detached Wasmtime")
    path.write_text(source(upstream, mode))
    after = subprocess.check_output(
        ["git", "-C", str(upstream), "diff", "--name-only"], text=True).splitlines()
    if after != ([] if mode == "baseline" else [SOURCE]):
        raise ValueError("unexpected ISLE source mutation footprint")
    rule = {"baseline": "", "unforced": RULE, "forced": FORCED_RULE}[mode]
    return {
        "schema": "zeno-fcis/tier1-overflow-forced-correct-rule/1",
        "status": "source-only-requires-independent-native-and-VeriISLE-verification",
        "upstream_commit": UPSTREAM, "original_blob": ORIGINAL_GIT_BLOB,
        "mode": mode, "source_path": SOURCE,
        "source_sha256": digest(path.read_bytes()),
        "rule_name": {"baseline": None, "unforced": RULE_NAME,
                      "forced": NEW_NAME}[mode],
        "rule_sha256": digest(rule.encode()),
        "limitation": "subsume deliberately forces an equivalent but potentially more costly RHS",
    }


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--upstream", required=True, type=Path)
    p.add_argument("--mode", choices=MODES, required=True)
    p.add_argument("--out", required=True, type=Path)
    args = p.parse_args()
    if args.out.exists():
        p.error("receipt exists")
    result = install(args.upstream.resolve(), args.mode)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"mode": args.mode, "rule_sha256": result["rule_sha256"]}))


if __name__ == "__main__":
    main()
