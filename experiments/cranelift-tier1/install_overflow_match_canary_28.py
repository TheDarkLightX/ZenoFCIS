#!/usr/bin/env python3
"""Diagnostic-only canaries for an ISLE matcher: intentionally UNSOUND.

Do NOT use the emitted compiler for any non-diagnostic code, and do not
upstream, benchmark, or call the mutation an optimization. The only question:
does an exact named overflow-pattern rewrite produce any observable impact?
A broad i8-bxor canary calibrates whether this codegen path is responsive.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import subprocess
from pathlib import Path

from install_isle_candidate import UPSTREAM, SOURCE, ORIGINAL_GIT_BLOB
from install_rust_overflow_predicate_28 import RULE, RULE_NAME, original

MODES = ("baseline", "exact_canary", "broad_canary")
RHS_START = "\n  (slt $I64\n       (band $I64\n"
if RULE.count(RHS_START) != 1 or not RULE.rstrip().endswith("(all_zero $I64)))"):
    raise RuntimeError("reviewed exact overflow matcher source changed")
EXACT_RULE = RULE.split(RHS_START, 1)[0] + "\n  (subsume (all_zero $I8)))\n"
BROAD_RULE = """
;; INTENTIONALLY INVALID: debug-only control for whether any i8 XOR rewrite
;; affects actual generated code. Never measure this as an optimization.
(rule tier1_probe_any_i8_bxor
  (simplify (bxor $I8 a b))
  (subsume (all_zero $I8)))
"""


def sha(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def source(upstream: Path, mode: str) -> str:
    if mode not in MODES:
        raise ValueError("not a declared probe mode")
    root = original(upstream)
    return root + {"baseline": "", "exact_canary": EXACT_RULE,
                   "broad_canary": BROAD_RULE}[mode]


def install(upstream: Path, mode: str) -> dict:
    if mode not in MODES:
        raise ValueError("unexpected canary mode")
    path = upstream / SOURCE
    allowed = {source(upstream, m) for m in MODES}
    if path.read_text() not in allowed:
        raise ValueError("source drift outside exact diagnostic palettes")
    modified = subprocess.check_output(
        ["git", "-C", str(upstream), "diff", "--name-only"], text=True).splitlines()
    if modified not in ([], [SOURCE]):
        raise ValueError("unrelated compiler source changes")
    target = source(upstream, mode)
    path.write_text(target)
    actual = subprocess.check_output(
        ["git", "-C", str(upstream), "diff", "--name-only"], text=True).splitlines()
    if actual != ([] if mode == "baseline" else [SOURCE]):
        raise ValueError("unexpected mutation footprint")
    return {
        "schema": "zeno-fcis/tier1-overflow-match-sentinel/1",
        "mode": mode, "upstream_commit": UPSTREAM, "path": SOURCE,
        "upstream_blob": ORIGINAL_GIT_BLOB,
        "source_sha256": sha(target.encode()),
        "rule_sha256": sha({"baseline": "", "exact_canary": EXACT_RULE,
                            "broad_canary": BROAD_RULE}[mode].encode()),
        "verifier": "NOT RUN: canary is intentionally semantically incorrect",
        "status": "untrusted-diagnostic-only-compiler-NOT-FOR-USE",
        "exact_original_lhs": RULE_NAME if mode == "exact_canary" else None,
    }


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--upstream", type=Path, required=True)
    p.add_argument("--mode", required=True, choices=MODES)
    p.add_argument("--out", required=True, type=Path)
    args = p.parse_args()
    if args.out.exists():
        p.error("will not overwrite canary source receipt")
    report = install(args.upstream.resolve(), args.mode)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"mode": args.mode, "status": report["status"]}))


if __name__ == "__main__":
    main()
