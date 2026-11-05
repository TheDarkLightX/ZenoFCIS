#!/usr/bin/env python3
"""Bounded, fail-closed QF_BV palette for Rust/LLVM's observed sign mask.

This is a finite HUMAN-PROPOSED candidate vocabulary (xor, add, sub).
It proves pure-result 64-bit bitvector semantics, not optimizer matching,
ISLE proof coverage, machine-code cost, ABI, flags, or poison states.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import z3
from install_rust_signmask_palette_28 import PALETTE, RULES, sha

SCHEMA = "zeno-fcis/tier1-rust-llvm-signmask-palette-qfbv/1"
M64 = -(1 << 63)
WIDTH = 64


def expression(mode: str, s):
    if mode not in PALETTE:
        raise ValueError("unadmitted candidate")
    lower = z3.BitVecVal(M64, WIDTH)
    if mode == "original":
        return (s >> 63) ^ lower
    if mode == "add":
        return (s >> 63) + lower
    return lower - z3.LShR(s, 63)


def check(name: str, eq, expected: str, time_limit_ms=120000) -> dict:
    solver = z3.SolverFor("QF_BV")
    solver.set(timeout=time_limit_ms)
    solver.add(z3.Not(eq))
    result = solver.check()
    status = str(result)
    if status != expected:
        raise ValueError(f"{name}: expected {expected}, got {status}; "
                         + (solver.reason_unknown() if status == "unknown" else ""))
    row = {"name": name, "logic": "QF_BV", "counterexample_query": str(z3.Not(eq)),
           "solver_result": status, "classification":
           "universally-equal-over-total-i64-bitvectors" if status == "unsat"
           else "deliberate-negative-control-counterexample"}
    if status == "sat":
        row["model"] = {str(d): str(solver.model()[d]) for d in solver.model().decls()}
    return row


def run() -> dict:
    value = z3.BitVec("sign_mask_operand", WIDTH)
    baseline = expression("original", value)
    results = [check("equivalent_" + mode, baseline == expression(mode, value), "unsat")
               for mode in PALETTE[1:]]
    wrong = z3.BitVecVal(M64, WIDTH) + z3.LShR(value, 63)
    results.append(check("negative_control_min_plus_logical_sign_bit",
                         baseline == wrong, "sat"))
    return {
        "schema": SCHEMA, "source_sha256": sha(Path(__file__).read_bytes()),
        "palette_script_sha256": sha((Path(__file__).parent / "install_rust_signmask_palette_28.py").read_bytes()),
        "rule_sha256": {k: sha(v.encode()) for k, v in RULES.items()},
        "z3_version": z3.get_version_string(), "width": WIDTH,
        "status": "two-universal-pure-value-proofs-and-one-refutation",
        "declared_domain": "every 64-bit bitvector s; signed arithmetic right shift; total wrapping +,-,^; masked unsigned logical shift; MIN = 2^63",
        "declared_scope": "Wasm/CLIF defined scalar bitvector operations, no poison, nondeterminism, flags, or UB admitted",
        "results": results,
        "nonclaims": ["not executable Cranelift/VeriISLE machine-code proof",
                      "not a native cost or runtime speed result",
                      "finite human-proposed palette is not global cost optimality",
                      "the LLVM graph linkage is only the earlier independent CLIF inspection"],
    }


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--out", type=Path, required=True)
    args = p.parse_args()
    if args.out.exists():
        p.error("refusing to overwrite a proof receipt")
    result = run()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": result["status"],
                      "checks": [(r["name"], r["solver_result"]) for r in result["results"]]}))


if __name__ == "__main__":
    main()
