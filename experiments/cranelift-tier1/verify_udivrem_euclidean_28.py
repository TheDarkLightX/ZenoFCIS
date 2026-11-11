#!/usr/bin/env python3
"""Source-pinned Euclidean arithmetic theorem for Cranelift udiv reconstruction.

This is an Int/Euclidean-divide theorem, distinct from the separately archived
64-bit QF_BV query (which may return UNKNOWN). It does not verify Rust,
CLIF pass implementation, regalloc, x86 flags, poison, ABI, or native cost.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

import z3
from verify_udivrem_reconstruction_qfbv_28 import (
    INSTALLER, UPSTREAM, source_guard,
)

MAX = (1 << 64) - 1
SCHEMA = "zenofcis/tier1-unsigned-remainder-euclidean-theorem/1"


def check(name: str, assumptions: list, bad, timeout: int) -> dict:
    solver = z3.Solver()
    solver.set(timeout=timeout)
    solver.add(*assumptions, bad)
    verdict = solver.check()
    row = {"obligation": name, "counterexample_query": str(verdict),
           "timeout_ms": timeout}
    if verdict == z3.unsat:
        row["classification"] = "proved-in-integer-Euclidean-semantics"
    elif verdict == z3.sat:
        model = solver.model()
        row["classification"] = "counterexample-found"
        row["model"] = {str(d): str(model[d]) for d in model.decls()}
    else:
        row["classification"] = "UNKNOWN-not-proved"
        row["reason"] = solver.reason_unknown()
    return row


def qualify(timeout: int) -> dict:
    guard = source_guard(INSTALLER.read_bytes())
    x, y = z3.Ints("unsigned_x unsigned_y")
    q = x / y     # Euclidean integer quotient for nonnegative x and y>0
    r = x % y     # Euclidean remainder
    m = q * y
    constraints = [x >= 0, x <= MAX, y >= 1, y <= MAX]
    obligations = [
        check("reconstructed-remainder-equals-urem",
              constraints, x-m != r, timeout),
        check("quotient-times-divisor-does-not-overflow-u64",
              constraints, z3.Or(m < 0, m > x, m > MAX), timeout),
        check("remainder-is-bounded-by-divisor",
              constraints, z3.Or(x-m < 0, x-m >= y), timeout),
        check("quotient-fits-u64", constraints,
              z3.Or(q < 0, q > MAX), timeout),
    ]
    return {
        "schema": SCHEMA,
        "upstream": UPSTREAM,
        "installer_pin": guard,
        "proof_source_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "z3_version": z3.get_version_string(),
        "logic": "quantifier-free nonlinear integer arithmetic with Euclidean div/mod",
        "domain": "all mathematical unsigned x,y in [0,2^64-1] with y>0",
        "exact_old": "q = x div y; m = q*y; r_old=x-m",
        "exact_new": "q=x div y; r_new=x mod y; m retained",
        "modular_bridge": "q*y<=x<=2^64-1 means old multiplication and subtraction do not wrap, so the exact-integer relation transfers to CLIF unsigned 64-bit values under their declared total arithmetic semantics",
        "zero_divisor": "excluded; both old udiv and new udivrem must trap",
        "signedness": "unsigned only; signed i64 divisor overflow excluded",
        "unknown_policy": "only UNSAT rows count as proofs; SAT and UNKNOWN never do",
        "checks": obligations,
        "all_proved": all(c["counterexample_query"]=="unsat" for c in obligations),
        "limitations": [
            "Integer semantic theorem, not a direct proof of a 64-bit QF_BV circuit",
            "Structural source pin does not prove that the Rust optimization pass matches only those conditions",
            "No compiler liveness, trap-order, poison, debug metadata, ABI, flags, ISA or target cost proof",
            "Does not change or replace the finite native differential qualification",
        ]
    }


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--timeout-ms", type=int, default=30000)
    a = p.parse_args()
    if a.out.exists() or not 1000 <= a.timeout_ms <= 120000:
        p.error("refuse overwrite or invalid timeout")
    receipt = qualify(a.timeout_ms)
    a.out.parent.mkdir(parents=True,exist_ok=True)
    a.out.write_text(json.dumps(receipt,indent=2,sort_keys=True)+"\n")
    print(json.dumps({"all_proved":receipt["all_proved"],
                      "checks":[[x["obligation"],x["counterexample_query"]]
                                for x in receipt["checks"]]},sort_keys=True))
    if any(c["counterexample_query"]=="sat" for c in receipt["checks"]):
        raise SystemExit("counterexample: unsigned arithmetic theorem failed")


if __name__ == "__main__":
    main()
