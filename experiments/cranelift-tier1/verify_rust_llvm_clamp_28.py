#!/usr/bin/env python3
"""Fresh independent all-input QF_BV check for exact Rust-emitted i64 clamp.

Scope: two's-complement modular 64-bit CLIF *value* arithmetic only.
DOES NOT prove a Wasmtime ISLE rule applied, any machine-code sequence,
flags, ABI, undefined/poison side conditions outside admitted total ops,
or target cost. The two alternative clamps are not cost-qualified.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

import z3

SCHEMA = "zeno-fcis/tier1-rust-llvm-clamp-qfbv/1"
WIDTH = 64
MIN = -(1 << 63)
MAX = (1 << 63) - 1
UPSTREAM = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
CLIF_PINS = {
    "optimized_sat_add": "16d25e0750eb38358de6d3e21e300f656077daa89cc9924e7e3b26451345ef54",
    "debug_sat_add": "b12b55e73d770871044ec71cd5d85cada6422064ca81f32fafbd6a78965855cd",
    "wat_positive_control": "41fe7c06dac196d36b18250e7ff2406d0b6fa811aa32d5075e71c455abfa60ec",
}


def sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def signed_wide_reference(x, y):
    """Independent exact mathematical signed clamp, via 65-bit extension."""
    wide_sum = z3.SignExt(1, x) + z3.SignExt(1, y)
    mx = z3.BitVecVal(MAX, WIDTH + 1)
    mn = z3.BitVecVal(MIN, WIDTH + 1)
    result65 = z3.If(wide_sum > mx, mx,
                     z3.If(wide_sum < mn, mn, wide_sum))
    return z3.Extract(WIDTH - 1, 0, result65)


def emitted_expression(x, y):
    """Exactly the source-audited CLIF computation graph, not source Rust."""
    total = x + y
    overflow = z3.Xor(y < z3.BitVecVal(0, WIDTH), total < x)
    mask = total >> (WIDTH - 1)  # signed arithmetic right shift
    clamp = mask ^ z3.BitVecVal(MIN, WIDTH)
    return z3.If(overflow, clamp, total)


def checked_counterexample(name: str, assertion, expected: str,
                           timeout_ms: int = 120_000) -> dict:
    solver = z3.SolverFor("QF_BV")
    solver.set(timeout=timeout_ms)
    solver.add(assertion)
    result = solver.check()
    outcome = str(result)
    if outcome != expected:
        raise ValueError(f"{name}: expected {expected}, got {outcome} "
                         f"(reason={solver.reason_unknown() if outcome == 'unknown' else 'n/a'})")
    row = {"name": name, "counterexample_query": str(assertion),
           "solver": "Z3", "logic": "QF_BV", "result": outcome,
           "classification": ("proof-by-UNSAT-counterexample-query"
                              if outcome == "unsat" else "expected-negative-control-counterexample")}
    if outcome == "sat":
        model = solver.model()
        row["model"] = {str(d): str(model[d]) for d in model.decls()}
    return row


def qualify() -> dict:
    x = z3.BitVec("x", WIDTH)
    y = z3.BitVec("y", WIDTH)
    total = x + y
    emitted = emitted_expression(x, y)
    ref = signed_wide_reference(x, y)
    minv = z3.BitVecVal(MIN, WIDTH)
    maxv = z3.BitVecVal(MAX, WIDTH)

    checks = [
        checked_counterexample("rust_llvm_core_vs_wide_signed_saturation",
                               emitted != ref, "unsat"),
        checked_counterexample("two_clamp_expressions_equal",
                               (minv ^ (total >> 63))
                               != (minv + (total >> 63)), "unsat"),
        # Deliberately incorrect formula must be disproved by a real witness.
        checked_counterexample("negative_control_logical_right_shift",
                               z3.If(
                                   z3.Xor(y < z3.BitVecVal(0, WIDTH), total < x),
                                   minv ^ z3.LShR(total, 63), total
                               ) != ref, "sat"),
        checked_counterexample("negative_control_wrong_overflow_predicate",
                               z3.If(
                                   z3.And(y < z3.BitVecVal(0, WIDTH), total < x),
                                   minv ^ (total >> 63), total
                               ) != ref, "sat"),
    ]
    return {
        "schema": SCHEMA,
        "status": "two-independent-QF_BV-proofs-and-two-negative-controls",
        "upstream_source_revision": UPSTREAM,
        "proof_source_sha256": sha(Path(__file__).read_bytes()),
        "z3_version": z3.get_version_string(),
        "clif_artifact_sha256": CLIF_PINS,
        "width": WIDTH,
        "signed_range": [MIN, MAX],
        "declared_semantics": [
            "x and y are unrestricted signed two's-complement 64-bit values",
            "addition wraps modulo 2^64 in the emitted CLIF graph",
            "comparison in overflow flag uses signed less-than",
            "shift in clamp uses sign-preserving arithmetic right shift",
            "reference uses signed 65-bit exact sum and mathematically clamps to signed i64 range",
            "all admitted integer and bitvector operations are total, no poison or undefined input states",
        ],
        "checks": checks,
        "nonclaims": [
            "proof of the pure-result graph only, not a Cranelift/VeriISLE rule proof",
            "not a verified machine-code sequence, flag preservation, calling convention or performance result",
            "structural link to actual CLIF is based on separate hashed emitted artifacts, not a verified compiler translation theorem",
            "arith-shift XOR vs ADD is a semantic candidate only, no measured native-cost win",
        ],
    }


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--out", type=Path, required=True)
    a = p.parse_args()
    if a.out.exists():
        p.error("refusing to overwrite prior proof receipt")
    receipt = qualify()
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": receipt["status"], "checks": [
        {"name": row["name"], "result": row["result"]} for row in receipt["checks"]]},
        sort_keys=True))


if __name__ == "__main__":
    main()
