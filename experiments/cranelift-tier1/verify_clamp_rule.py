"""Independently prove the pure one-input i64 clamp subexpression in QF_BV."""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import z3
from install_isle_candidate import RULE, RULE_NAME, SOURCE, UPSTREAM

def prove() -> dict:
    x = z3.BitVec("x", 64)
    hi = z3.BitVecVal((1 << 63) - 1, 64)
    lo = z3.BitVecVal(1 << 63, 64)
    left = z3.If(x < z3.BitVecVal(0, 64), lo, hi)
    right = hi + z3.LShR(x, z3.BitVecVal(63,64))
    solver = z3.SolverFor("QF_BV")
    solver.set(timeout=30000)
    solver.add(left != right)
    result = solver.check()
    if result != z3.unsat:
        raise ValueError(f"rule not proven; solver={result}; unknown={solver.reason_unknown()}")
    return {"schema": "zeno-fcis/tier1-clamp-identity-proof/1",
            "status": "unsat-counterexample-query",
            "rule": RULE_NAME,
            "rule_sha256": hashlib.sha256(RULE.encode()).hexdigest(),
            "upstream_commit": UPSTREAM, "upstream_source_path": SOURCE,
            "interpretation": "pure signed i64 clamp bits, all 2^64 inputs",
            "smt": {"name": "Z3", "version": z3.get_version_string(),
                    "logic": "QF_BV", "timeout_ms": 30000},
            "nonclaims": ["not a VeriISLE rule-chain or actual compiler lowering proof",
                          "does not establish optimality or portable target cost"]}

def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--out", type=Path, required=True)
    a = p.parse_args()
    if a.out.exists():
        raise SystemExit("fresh proof receipt required")
    receipt = prove()
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text(json.dumps(receipt,sort_keys=True,indent=2)+"\n")
    print(json.dumps({"status":receipt["status"],"rule":receipt["rule"]}))

if __name__=="__main__":
    main()
