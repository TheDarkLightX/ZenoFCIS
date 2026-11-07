#!/usr/bin/env python3
"""Independently check udiv/urem quotient reuse with explicit division domain.

A true direct QF_BV UDiv/URem identity check is attempted at small and 64-bit
widths. Unknown is NEVER called a proof. Separately establish a trivial
all-width algebraic consequence of the Euclidean quotient/remainder law;
that result is CONDITIONAL on the standardized unsigned-div/rem semantics.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import z3

SCHEMA = "zeno-fcis/tier1-udivrem-semantics/1"


def check_direct(width: int, timeout_ms: int) -> dict:
    x = z3.BitVec(f"x_{width}", width)
    y = z3.BitVec(f"y_{width}", width)
    q, r = z3.UDiv(x, y), z3.URem(x, y)
    solver = z3.SolverFor("QF_BV")
    solver.set("timeout", timeout_ms)
    solver.add(y != z3.BitVecVal(0, width))
    solver.add(r != x - (q * y))
    verdict = solver.check()
    output = {
        "width": width, "domain": "all nonzero unsigned divisors at this width",
        "solver_result": str(verdict),
        "proved": verdict == z3.unsat,
        "query_smt2_sha256": hashlib.sha256(solver.to_smt2().encode()).hexdigest(),
        "query_smt2": solver.to_smt2(),
    }
    if verdict == z3.sat:
        m = solver.model()
        output["counterexample"] = {"x": m.eval(x).as_long(),
                                    "y": m.eval(y).as_long()}
    if verdict == z3.unknown:
        output["reason_unknown"] = solver.reason_unknown()
    return output


def check_conditional_euclidean(width: int) -> dict:
    x, y, q, r = z3.BitVecs("x_e y_e q_e r_e", width)
    solver = z3.SolverFor("QF_BV")
    # The division contract says that x == y*q+r (with ordinary arithmetic)
    # and 0 <= r < y. Its modular consequence below needs only equality.
    # This is not by itself an unconditional proof of UDiv/URem's identity.
    solver.add(y != 0)
    solver.add(x == y * q + r)
    solver.add(r != x - (y * q))
    verdict = solver.check()
    return {
        "width": width, "solver_result": str(verdict),
        "proved_conditional_on_euclidean_identity": verdict == z3.unsat,
        "assumption": "unsigned Euclidean quotient-remainder law x=y*q+r for y!=0",
        "not_claimed": "not a proof that arbitrary unconstrained machine DIV implements UDiv/URem",
        "query_smt2_sha256": hashlib.sha256(solver.to_smt2().encode()).hexdigest(),
    }


def qualify() -> dict:
    direct = [check_direct(8, 10000), check_direct(16, 30000),
              check_direct(64, 15000)]
    conditional = check_conditional_euclidean(64)
    if not direct[0]["proved"] or not conditional["proved_conditional_on_euclidean_identity"]:
        raise ValueError("minimum signed-off source semantics proof failed")
    return {
        "schema": SCHEMA, "status": "small-width-direct-and-conditional-all-width-law",
        "z3_version": z3.get_version_string(),
        "source_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "direct_queries": direct, "euclidean_law": conditional,
        "semantics": {
            "unsigned": "UDiv/URem on 64-bit words, divisor nonzero",
            "output": "q XOR rol64(r,17)",
            "candidate_remainder": "x - q*y modulo 2^64",
            "zero_divisor": "Wasm must trap, not a defined value; executable tests separate",
            "poison_flags_UB": "no CLIF poison or observable host flags in admitted Wasm contract",
            "scope": "the 64-bit arithmetic identity is separately conditional on Euclidean quotient/remainder semantics",
        },
        "nonclaims": [
            "unknown and timeout outcomes in direct division SMT are not proofs",
            "the conditional proof is not an unconditional x86 ISA or whole compiler theorem",
            "no code size, latency, speed, throughput or application win established by semantics",
        ],
    }


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--out", type=Path, required=True)
    args = p.parse_args()
    if args.out.exists():
        p.error("refusing to overwrite proof receipt")
    result = qualify()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, sort_keys=True, indent=2) + "\n")
    print(json.dumps({
        "status": result["status"],
        "direct": [{k: x[k] for k in ("width", "solver_result", "proved")}
                   for x in result["direct_queries"]],
        "conditional": result["euclidean_law"]["solver_result"],
    }, sort_keys=True))


if __name__ == "__main__":
    main()
