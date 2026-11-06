#!/usr/bin/env python3
"""Independent QF_BV proof for two complete Rust/LLVM overflow predicates.

Admitted domain is all 2^128 (x,y) signed-i64 bitvector input pairs;
not a Cranelift/VeriISLE rule proof or a native speed comparison.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import z3
from install_rust_overflow_predicate_28 import RULE, RULE_NAME, sha

W = 64
MIN = -(1 << 63)
MAX = (1 << 63) - 1
SCHEMA = "zeno-fcis/rust-llvm-i64-overflow-qfbv/1"


def original(x, y):
    s = x + y
    return z3.Xor(y < z3.BitVecVal(0, W), s < x)


def xorand(x, y):
    s = x + y
    return ((s ^ x) & (s ^ y)) < z3.BitVecVal(0, W)


def xorandnot(x, y):
    s = x + y
    return ((s ^ x) & ~(x ^ y)) < z3.BitVecVal(0, W)


def saturated(x, y, flag):
    s = x + y
    clamp = (s >> 63) ^ z3.BitVecVal(MIN, W)
    return z3.If(flag, clamp, s)


def signed_wide(x, y):
    total = z3.SignExt(1, x) + z3.SignExt(1, y)
    hi = z3.BitVecVal(MAX, 65)
    lo = z3.BitVecVal(MIN, 65)
    return z3.Extract(63, 0, z3.If(total > hi, hi, z3.If(total < lo, lo, total)))


def check(label, formula, expected, timeout_ms=120000):
    sol = z3.SolverFor("QF_BV")
    sol.set(timeout=timeout_ms)
    sol.add(formula)
    found = str(sol.check())
    if found != expected:
        raise ValueError(label + ": expected " + expected + " got " + found
                         + (": " + sol.reason_unknown() if found == "unknown" else ""))
    item = {"name": label, "result": found,
            "query": str(formula),
            "qualification": "all-input-UNSAT-proof" if found == "unsat"
                             else "negative-control-real-counterexample"}
    if found == "sat":
        model = sol.model()
        item["counterexample"] = {str(d): str(model[d]) for d in model.decls()}
    return item


def qualify():
    x = z3.BitVec("x", 64)
    y = z3.BitVec("y", 64)
    ref = signed_wide(x, y)
    bad = ((x ^ (x + y)) & (x ^ y)) < z3.BitVecVal(0, W)
    proofs = [
        check("original_predicate_matches_xorand", original(x, y) != xorand(x, y), "unsat"),
        check("original_predicate_matches_xorandnot", original(x, y) != xorandnot(x, y), "unsat"),
        check("original_full_saturation_vs_65bit_oracle", saturated(x, y, original(x, y)) != ref, "unsat"),
        check("candidate_full_saturation_vs_65bit_oracle", saturated(x, y, xorand(x, y)) != ref, "unsat"),
        check("deliberately_wrong_mask", original(x, y) != bad, "sat"),
    ]
    return {"schema": SCHEMA, "status": "four-QF_BV-proofs-and-SAT-refutation",
            "solver": "Z3", "solver_version": z3.get_version_string(),
            "source_sha256": sha(Path(__file__).read_bytes()),
            "rule_name": RULE_NAME, "rule_sha256": sha(RULE.encode()),
            "width": W,
            "semantics": {
                "input": "all signed i64 x and y; modular two-complement i64 addition",
                "baseline": "signed_lt(y,0) XOR signed_lt((x+y) mod 2^64,x)",
                "candidate": "signed_lt(((sum XOR x) AND (sum XOR y)),0)",
                "alternative_proof_only": "signed_lt(((sum XOR x) AND NOT(x XOR y)),0)",
                "clamp": "(signed_arithmetic_shift_right(sum,63)) XOR INT64_MIN",
                "reference": "exact signed 65-bit addition mathematically clamped to i64",
                "poison_flags_UB": "none in admitted total scalar bitvector expressions; no machine flags observed",
            },
            "checks": proofs,
            "nonclaims": [
                "not proof the ISLE matcher reaches actual LLVM CLIF",
                "not an unconditional VeriISLE proof or arbitrary ISA/ABI preservation",
                "not target machine code size or runtime evidence",
                "finite hand-proposed alternatives not global minimum",
            ]}


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--out", required=True, type=Path)
    args = p.parse_args()
    if args.out.exists():
        p.error("refusing to overwrite proof receipt")
    receipt = qualify()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": receipt["status"],
                      "proofs": [(x["name"], x["result"]) for x in receipt["checks"]]}))


if __name__ == "__main__":
    main()
