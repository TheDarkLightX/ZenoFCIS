#!/usr/bin/env python3
"""Compositional all-u64 *contract* proof for two adjacent udiv/urem pairs.

Does not prove the Cranelift Rust pass, ISA flags, registers, or native traps.
The exact quotient/remainder Euclidean contracts are explicit assumptions.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent
PIN_UPSTREAM = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
PIN_WAT = "763d99565f7ec0530db081209bd6cea968a2bdcf58deb84b690209394cba634d"
SOURCE_BLOBS = {
    "install_udivrem_combiner_28.py": "1e5ad0cc46b8ce791b14cad6998df4f32ea474ec",
    "install_udivrem_multivalue_28.py": "01b92461269b5ee354287a48a6750143e7564dc9",
}
MAX = (1 << 64) - 1

def blob(raw: bytes) -> str:
    return hashlib.sha1(b"blob " + str(len(raw)).encode() + b"\0" + raw).hexdigest()

def pins() -> dict:
    source = {}
    for filename, expected in SOURCE_BLOBS.items():
        raw = (ROOT / filename).read_bytes()
        if blob(raw) != expected:
            raise ValueError(f"source pin changed: {filename}")
        source[filename] = {"git_blob": expected, "sha256": hashlib.sha256(raw).hexdigest()}
    from qualify_udivrem_contexts_28 import CASES
    observed = hashlib.sha256(CASES["two_live_pairs"].encode()).hexdigest()
    if observed != PIN_WAT:
        raise ValueError(f"WAT changed, re-audit required: {observed}")
    combiner = (ROOT / "install_udivrem_combiner_28.py").read_text()
    multi = (ROOT / "install_udivrem_multivalue_28.py").read_text()
    for token in ('with_results([Some(q), Some(r)]).udivrem(x, y)',
                  'func.layout.next_inst(qinst)', 'self.verify(isa)?;'):
        if token not in combiner:
            raise ValueError(f"combiner anchor absent: {token}")
    for token in ('x64_div $I64 a', 'TrapCode.INTEGER_DIVISION_BY_ZERO',
                  'output_pair (value_regs_get qr 0) (value_regs_get qr 1)'):
        if token not in multi:
            raise ValueError(f"lowering anchor absent: {token}")
    return {"installer_blobs": source, "wat_sha256": observed,
            "upstream": PIN_UPSTREAM}

def assertion(name, premises, counterexample, timeout_ms):
    import z3
    s = z3.Solver()
    s.set(timeout=timeout_ms)
    s.add(*(premises + [counterexample]))
    result = s.check()
    row = {"name": name, "counterexample_query": str(result)}
    if result == z3.unsat:
        row["classification"] = "UNSAT-conditional-contract-proof"
    elif result == z3.sat:
        row["classification"] = "SAT-witness"
        m = s.model()
        row["witness"] = {str(d): str(m[d]) for d in m.decls()}
    else:
        row["classification"] = "UNKNOWN-not-proof"
        row["reason"] = s.reason_unknown()
    return row

def prove(timeout_ms):
    import z3
    x, y, q1, r1, q2, r2 = z3.Ints("x y q1 r1 q2 r2")
    # Model exactly the two derived operands in the pinned Wasm: wrap by 2^64.
    x2 = z3.If(x == MAX, z3.IntVal(0), x + 1)
    y2 = z3.If(y == MAX, z3.IntVal(0), y + 1)
    wellformed = [x >= 0, x <= MAX, y > 0, y < MAX,
                  q1 >= 0, 0 <= r1, r1 < y, x == q1 * y + r1,
                  q2 >= 0, 0 <= r2, r2 < y2, x2 == q2 * y2 + r2]
    # Quotient*divisor cannot overflow when Euclidean constraints hold.
    duties = [
        assertion("first-remainder-exact", wellformed, x-q1*y != r1, timeout_ms),
        assertion("second-remainder-exact", wellformed, x2-q2*y2 != r2, timeout_ms),
        assertion("first-product-no-u64-overflow", wellformed,
                  z3.Or(q1*y > MAX, q1*y < 0), timeout_ms),
        assertion("second-product-no-u64-overflow", wellformed,
                  z3.Or(q2*y2 > MAX, q2*y2 < 0), timeout_ms),
    ]
    # Bitvector trace: zero divisor at first site takes priority over second.
    by = z3.BitVec("by", 64)
    first = by == z3.BitVecVal(0, 64)
    second = by + z3.BitVecVal(1, 64) == z3.BitVecVal(0, 64)
    status = z3.If(first, z3.IntVal(1), z3.If(second, z3.IntVal(2), z3.IntVal(0)))
    duties += [
        assertion("first-trap-exactly-y-zero", [], (status == 1) != first, timeout_ms),
        assertion("second-trap-exactly-y-max", [],
                  (status == 2) != (by == z3.BitVecVal(MAX,64)), timeout_ms),
        assertion("finite-exactly-nonzero-nonmax", [],
                  (status == 0) != z3.And(z3.Not(first), z3.Not(second)), timeout_ms),
    ]
    # Two deliberate wrong transformations must have actual witnesses.
    negatives = [
        assertion("MUTANT-collapse-second-site", [],
                  status != z3.If(z3.Or(first, second), z3.IntVal(1), z3.IntVal(0)), timeout_ms),
        assertion("MUTANT-no-second-divisor-wrap", [],
                  (status == 2) != z3.BoolVal(False), timeout_ms),
        assertion("MUTANT-first-divisor-swapped", [],
                  status != z3.If(second, z3.IntVal(1),
                                 z3.If(first, z3.IntVal(2), z3.IntVal(0))), timeout_ms),
    ]
    return duties, negatives, z3.get_version_string()

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--timeout-ms", type=int, default=15000)
    args = parser.parse_args()
    if args.out.exists() or not 1000 <= args.timeout_ms <= 120000:
        parser.error("new output path and timeout in [1000,120000] required")
    source = pins()
    duties, mutants, solver = prove(args.timeout_ms)
    good = all(x["counterexample_query"] == "unsat" for x in duties)
    mutants_good = all(x["counterexample_query"] == "sat" for x in mutants)
    receipt = {
        "schema": "zenofcis/tier1-two-site-u64-conditional-trace-theorem/1",
        "pins": source, "checker_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "solver": "Z3 " + solver, "logic": "mixed QF_NIA and QF_BV, separate obligations",
        "admitted_domain": "all unsigned u64 x,y; finite two-pair case excludes y=0 and y=MAX",
        "source_contract": "first udiv+urem(x,y); then udiv+urem(wrap(x+1),wrap(y+1))",
        "candidate_contract": "same quotient and remainder for each pair through one hypothetical multi-result primitive; original quotient/remainder Euclidean axioms assumed",
        "trap_priority": "first y=0; second y=2^64-1; else finite value",
        "checks": duties, "negative_mutants": mutants,
        "all_contract_obligations_proved": good,
        "all_negative_mutants_found": mutants_good,
        "classification": "conditional symbolic semantics only, NOT proof of executable compiler pass",
        "limitations": [
            "Euclidean quotient/remainder equations are assumptions, not verified x86 DIV semantics",
            "No direct 64-bit machine-state equivalence or Rust pass proof",
            "Flags, poison/undef, register allocation, SSA integration and internal Wasmtime trap-table PC maps not proved",
            "Neither generated native bytes nor runtime savings measured here",
            "UNKNOWN never counts as proof; SAT on positive obligation fails qualification",
        ],
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(receipt, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"obligations": [(r["name"],r["counterexample_query"]) for r in duties],
                      "mutants": [(r["name"],r["counterexample_query"]) for r in mutants],
                      "all_proved":good, "all_mutants_detected":mutants_good}, sort_keys=True))
    if not good or not mutants_good:
        raise SystemExit("unproved contract or mutation: do not promote receipt")

if __name__ == "__main__":
    main()
