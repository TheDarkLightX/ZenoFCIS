#!/usr/bin/env python3
"""Exactly one source-pinned experiment rewriting Rust/LLVM signed-overflow i8.

Not a discovery claim, an unconditional ISLE proof, or a compiler-size win.
Only the one named rule is appended to a detached Wasmtime checkout.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import subprocess
from pathlib import Path
from install_isle_candidate import UPSTREAM, SOURCE, ORIGINAL_GIT_BLOB

RULE_NAME = "tier1_rust_llvm_i64_overflow_xorand"
RULE = """
;; BOUNDED EXPERIMENT: LLVM signed-add overflow predicate for i64 inputs.
;; (y <signed 0) XOR ((x+y)<signed x) = sign((s^x)&(s^y)).
;; This is total pure CLIF bitvector algebra, not target flag/ABI semantics.
(rule tier1_rust_llvm_i64_overflow_xorand
  (simplify
    (bxor $I8
          (slt $I64 y (all_zero $I64))
          (slt $I64 (iadd $I64 x y) x)))
  (slt $I64
       (band $I64
             (bxor $I64 (iadd $I64 x y) x)
             (bxor $I64 (iadd $I64 x y) y))
       (all_zero $I64)))
"""
MODES = ("baseline", "xorand")


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def command(*argv: str) -> str:
    return subprocess.check_output(argv, text=True).strip()


def original(upstream: Path) -> str:
    if command("git", "-C", str(upstream), "rev-parse", "HEAD") != UPSTREAM:
        raise ValueError("wrong pinned Wasmtime revision")
    if command("git", "-C", str(upstream), "rev-parse", "HEAD:" + SOURCE) != ORIGINAL_GIT_BLOB:
        raise ValueError("reviewed selects.isle Git blob changed")
    s = subprocess.check_output(["git", "-C", str(upstream), "show", "HEAD:" + SOURCE], text=True)
    if not s.startswith(";; `select`/"):
        raise ValueError("unrecognized original source grammar")
    return s.rstrip("\n") + "\n"


def source(upstream: Path, mode: str) -> str:
    if mode not in MODES:
        raise ValueError("candidate outside admitted vocabulary")
    return original(upstream) + (RULE if mode == "xorand" else "")


def install(upstream: Path, mode: str) -> dict:
    target = upstream / SOURCE
    permissible = (source(upstream, "baseline"), source(upstream, "xorand"))
    if target.read_text() not in permissible:
        raise ValueError("unreviewed compiler source state")
    changes = command("git", "-C", str(upstream), "diff", "--name-only").splitlines()
    if changes not in ([], [SOURCE]):
        raise ValueError("unrelated changed tracked compiler files")
    target.write_text(source(upstream, mode))
    expected_changes = [] if mode == "baseline" else [SOURCE]
    if command("git", "-C", str(upstream), "diff", "--name-only").splitlines() != expected_changes:
        raise ValueError("unexpected compiler patch scope")
    return {"schema": "zeno-fcis/rust-llvm-overflow-predicate-isle/1",
            "upstream_commit": UPSTREAM, "original_git_blob": ORIGINAL_GIT_BLOB,
            "file": SOURCE, "rule_name": RULE_NAME if mode == "xorand" else None,
            "mode": mode, "source_sha256": sha(target.read_bytes()),
            "rule_sha256": sha((RULE if mode == "xorand" else "").encode()),
            "status": "source-only-not-verified-rule"}


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--upstream", required=True, type=Path)
    p.add_argument("--mode", choices=MODES, required=True)
    p.add_argument("--out", required=True, type=Path)
    a = p.parse_args()
    if a.out.exists():
        p.error("refusing to overwrite receipt")
    result = install(a.upstream.resolve(), a.mode)
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text(json.dumps(result, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"mode": a.mode, "source_sha256": result["source_sha256"]}))


if __name__ == "__main__":
    main()
