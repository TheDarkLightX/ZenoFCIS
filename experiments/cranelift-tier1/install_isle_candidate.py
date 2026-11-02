"""Apply one source-pinned, experimental Cranelift mid-end ISLE rule.

Only changes a detached checkout of the exact reviewed upstream revision.
Never writes ZenoFCIS main or the user's upstream Wasmtime repository.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import subprocess
from pathlib import Path

UPSTREAM = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
SOURCE = "cranelift/codegen/src/opts/selects.isle"
ORIGINAL_GIT_BLOB = "6a6a9129f382d5cd45ae0919ae18683973b31db2"
RULE_NAME = "tier1_i64_signed_saturation_clamp"
RULE = "\n;; EXPERIMENTAL TIER1: recognize the common signed-i64 saturating-add clamp\n;; component select(x < 0, SMIN, SMAX) and reconstruct it from x's sign bit.\n;; The enclosing sadd_overflow select is unchanged. x64 size wins are target-\n;; specific; do not upstream an architecture-neutral rewrite without regressions.\n(rule tier1_i64_signed_saturation_clamp\n  (simplify (select $I64\n                     (slt $I64 x (all_zero $I64))\n                     (iconst_s $I64 -9223372036854775808)\n                     (iconst_s $I64 9223372036854775807)))\n  (iadd $I64\n        (ushr $I64 x (iconst_u $I64 63))\n        (iconst_s $I64 9223372036854775807)))\n"

def install(upstream: Path) -> dict:
    if subprocess.check_output(["git", "-C", str(upstream), "rev-parse", "HEAD"], text=True).strip() != UPSTREAM:
        raise ValueError("unexpected upstream Git revision")
    current = subprocess.check_output(["git", "-C", str(upstream), "hash-object", SOURCE], text=True).strip()
    if current != ORIGINAL_GIT_BLOB:
        raise ValueError("upstream ISLE source blob does not match reviewed pin")
    path = upstream / SOURCE
    old = path.read_text()
    if RULE_NAME in old or not old.startswith(";; `select`/`bitselect`-related rewrites"):
        raise ValueError("rule already exists or original source shape is unrecognized")
    new = old.rstrip("\n") + "\n" + RULE
    path.write_text(new)
    return {"schema": "zeno-fcis/tier1-isle-experimental-patch/1",
            "status": "source-pinned-patch-applied",
            "upstream_commit": UPSTREAM, "upstream_source_path": SOURCE,
            "original_git_blob": current,
            "original_source_sha256": hashlib.sha256(old.encode()).hexdigest(),
            "rule_name": RULE_NAME,
            "rule_sha256": hashlib.sha256(RULE.encode()).hexdigest(),
            "patched_source_sha256": hashlib.sha256(new.encode()).hexdigest(),
            "scope": "experimental ISLE mid-end rule; no upstream patch or verification claimed"}

def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--upstream", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    a = p.parse_args()
    if a.out.exists():
        raise SystemExit("refusing to overwrite patch receipt")
    receipt = install(a.upstream)
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text(json.dumps(receipt, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"status":receipt["status"],"rule":RULE_NAME}))

if __name__ == "__main__":
    main()
