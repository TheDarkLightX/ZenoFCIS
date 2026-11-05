#!/usr/bin/env python3
"""Switch only pinned experimental Rust/LLVM sign-mask ISLE candidates.

These are human-selected equivalent finite alternatives, NOT an enumerator
discovery or upstream patch. Native costs remain unknown until compiled.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import subprocess
from pathlib import Path
from install_isle_candidate import UPSTREAM, SOURCE, ORIGINAL_GIT_BLOB

RULES = {
    "original": "",
    "add": """
;; EXPERIMENTAL Rust/LLVM sign mask: signed >>63 XOR MIN -> ADD MIN.
;; I64 only. Pure value, not overflow flags, machine ISA, or ABI.
(rule tier1_rust_llvm_signmask_xor_to_add
  (simplify (bxor $I64
                  (sshr $I64 s (iconst_u $I64 63))
                  (iconst_s $I64 -9223372036854775808)))
  (iadd $I64
        (sshr $I64 s (iconst_u $I64 63))
        (iconst_s $I64 -9223372036854775808)))
""",
    "sub": """
;; EXPERIMENTAL Rust/LLVM sign mask: XOR MIN -> MIN - unsigned sign bit.
;; I64 only. Pure value, no flags, poison, ABI, or performance proof.
(rule tier1_rust_llvm_signmask_xor_to_sub
  (simplify (bxor $I64
                  (sshr $I64 s (iconst_u $I64 63))
                  (iconst_s $I64 -9223372036854775808)))
  (isub $I64
        (iconst_s $I64 -9223372036854775808)
        (ushr $I64 s (iconst_u $I64 63))))
""",
}
PALETTE = ("original", "add", "sub")


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def cmd(*args: str) -> str:
    return subprocess.check_output(args, text=True).strip()


def pin(upstream: Path) -> str:
    if cmd("git", "-C", str(upstream), "rev-parse", "HEAD") != UPSTREAM:
        raise ValueError("wrong Wasmtime source revision")
    if cmd("git", "-C", str(upstream), "rev-parse", "HEAD:" + SOURCE) != ORIGINAL_GIT_BLOB:
        raise ValueError("source blob differs from reviewed upstream")
    source = subprocess.check_output(
        ["git", "-C", str(upstream), "show", "HEAD:" + SOURCE], text=True)
    if not source.startswith(";; "):
        raise ValueError("unrecognized original select file")
    return source.rstrip("\n") + "\n"


def expected(upstream: Path, mode: str) -> str:
    if mode not in RULES:
        raise ValueError("palette mode not declared")
    return pin(upstream) + RULES[mode]


def install(upstream: Path, mode: str) -> dict:
    intended = expected(upstream, mode)
    path = upstream / SOURCE
    original = expected(upstream, "original")
    current = path.read_text()
    if current not in (original, expected(upstream, "add"), expected(upstream, "sub")):
        raise ValueError("unexpected source changes: refusing to overwrite")
    modified = cmd("git", "-C", str(upstream), "diff", "--name-only").splitlines()
    if modified not in ([], [SOURCE]):
        raise ValueError("unrelated upstream tracked source modified")
    path.write_text(intended)
    if path.read_text() != intended:
        raise ValueError("patch write not identical")
    diff = cmd("git", "-C", str(upstream), "diff", "--name-only").splitlines()
    if diff != ([] if mode == "original" else [SOURCE]):
        raise ValueError("unexpected ISLE patch scope")
    return {
        "schema": "zeno-fcis/tier1-rust-llvm-signmask-ISLE-palette/1",
        "mode": mode, "source_sha256": sha(intended.encode()),
        "rule_sha256": sha(RULES[mode].encode()),
        "rule_name": "tier1_rust_llvm_signmask_xor_to_" + mode,
        "upstream_commit": UPSTREAM, "original_git_blob": ORIGINAL_GIT_BLOB,
        "file": SOURCE, "status": "source-only-experimental-patch",
        "qualification": "unmeasured until genuine compiler AOT and independent checks",
    }


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--upstream", type=Path, required=True)
    p.add_argument("--mode", choices=PALETTE, required=True)
    p.add_argument("--out", type=Path, required=True)
    a = p.parse_args()
    if a.out.exists():
        p.error("refusing to overwrite source receipt")
    result = install(a.upstream.resolve(), a.mode)
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text(json.dumps(result, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"mode": a.mode, "source_sha256": result["source_sha256"]}))


if __name__ == "__main__":
    main()
