#!/usr/bin/env python3
"""Independent, explicitly scoped semantics check for the pinned udiv/mul/sub pilot.

Checks unsigned i64 value semantics only, not Cranelift pass correctness, x86
flags, undefined/poison lowering, register allocation, or measurable profit.
Unknown SMT results are recorded as UNKNOWN, never promoted to proofs.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
INSTALLER = HERE / "install_udivrem_reconstruction_28.py"
INSTALLER_BLOB = "466a4608fca3a19e063a86303521257ae45ba589"
UPSTREAM = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
SCHEMA = "zenofcis/tier1-udiv-mul-sub-independent-semantics/1"


def github_blob(raw: bytes) -> str:
    return hashlib.sha1(b"blob " + str(len(raw)).encode() + b"\x00" + raw).hexdigest()


def source_guard(raw: bytes) -> dict:
    if github_blob(raw) != INSTALLER_BLOB:
        raise ValueError("source change requires a fresh semantic audit and pin")
    text = raw.decode("utf-8")
    anchors = (
        "Opcode::Udiv", "Opcode::Imul", "Opcode::Isub",
        "func.dfg.value_type(qa[0]) != types::I64",
        "func.dfg.value_type(qa[1]) != types::I64",
        "ma[0] == q && ma[1] == y", "ma[0] == y && ma[1] == q",
        "ra[0] != x || ra[1] != m",
        "with_results([Some(q), Some(r)]).udivrem(x, y)",
        "func.layout.remove_inst(qinst)",
        "func.layout.remove_inst(rinst)",
    )
    if any(a not in text for a in anchors):
        raise ValueError("expected exact SSA matcher or replacement absent")
    if "func.layout.remove_inst(minst)" in text:
        raise ValueError("intermediate multiply unexpectedly removed")
    if UPSTREAM not in text and "UPSTREAM = multi.UPSTREAM" not in text:
        raise ValueError("unrecognized upstream pin")
    return {"github_blob": github_blob(raw), "sha256": hashlib.sha256(raw).hexdigest(),
            "positive_source_anchors": len(anchors),
            "negative_source_controls": 1}


def finite(width: int) -> dict:
    if width not in (8,):
        raise ValueError("finite exhaustive width must be 8")
    mask = (1 << width) - 1
    count = 0
    traps = 0
    for x in range(1 << width):
        for y in range(1 << width):
            if y == 0:
                traps += 1
                continue
            q = x // y
            m = (q * y) & mask
            old_r = (x - m) & mask
            new_r = x % y
            if old_r != new_r or q != x // y or m != (x // y) * y:
                raise AssertionError((x, y, q, m, old_r, new_r))
            count += 1
    # Deliberately incorrect multiplier must be rejected.
    x, y = 7, 3
    mutant = (x - (x // y) * x) & mask
    if mutant == x % y:
        raise AssertionError("mutant negative control unexpectedly equivalent")
    return {"width": width, "classification": "exhaustive-finite-model",
            "nontrapping_pairs": count, "zero_divisor_pairs": traps,
            "incorrect_multiplier_witness": {"x": x, "y": y,
                "mutant_remainder": mutant, "true_remainder": x % y}}


def qfbv64(timeout_ms: int) -> dict:
    import z3
    x, y = z3.BitVecs("x y", 64)
    quotient = z3.UDiv(x, y)
    old_mul = quotient * y
    old_rem = x - old_mul
    new_rem = z3.URem(x, y)
    solver = z3.SolverFor("QF_BV")
    solver.set(timeout=timeout_ms)
    solver.add(y != z3.BitVecVal(0, 64), old_rem != new_rem)
    result = solver.check()
    status = str(result)
    row = {"logic": "QF_BV", "width": 64,
           "admitted_domain": "all x:uint64 and nonzero y:uint64",
           "counterexample_query_result": status, "timeout_ms": timeout_ms,
           "z3_version": z3.get_version_string()}
    if result == z3.unsat:
        row["classification"] = "SMT-proof-of-pure-result-equivalence"
    elif result == z3.sat:
        row["classification"] = "counterexample-found-not-equivalent"
        m = solver.model()
        row["counterexample"] = {
            "x": str(m.eval(x, model_completion=True)),
            "y": str(m.eval(y, model_completion=True))}
    else:
        row["classification"] = "UNKNOWN-not-proved"
        row["reason"] = solver.reason_unknown()
    # At divisor zero both original udiv and replacement udivrem trap by
    # the declared CLIF operation contracts, not by SMT's total BV division.
    row["zero_divisor"] = "excluded from value query; both operations must trap"
    return row


def self_test() -> None:
    raw = INSTALLER.read_bytes()
    assert source_guard(raw)["github_blob"] == INSTALLER_BLOB
    for corrupt in (raw + b"\n", raw.replace(b"Opcode::Isub", b"Opcode::Iadd", 1)):
        try:
            source_guard(corrupt)
        except ValueError:
            pass
        else:
            raise AssertionError("failed closed-source mutation test")
    data = finite(8)
    assert data["nontrapping_pairs"] == 65280
    assert data["zero_divisor_pairs"] == 256
    print("exact-source guards, two mutations and 65,280 exhaustive unsigned 8-bit pairs passed")


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--out", type=Path)
    p.add_argument("--timeout-ms", type=int, default=120000)
    p.add_argument("--self-test", action="store_true")
    a = p.parse_args()
    if not 1000 <= a.timeout_ms <= 180000:
        p.error("timeout out of range")
    if a.self_test:
        self_test()
        return
    if a.out is None or a.out.exists():
        p.error("--out required and must not exist")
    src = source_guard(INSTALLER.read_bytes())
    f = finite(8)
    proof = qfbv64(a.timeout_ms)
    receipt = {
        "schema": SCHEMA, "target": "pinned Cranelift x86-64 unsigned i64 pilot",
        "wasmtime_upstream_source": UPSTREAM,
        "source": src, "finite": f, "smt": proof,
        "semantics": {
            "old": "q=udiv(x,y); m=imul(q,y); r=isub(x,m)",
            "new": "q,r=udivrem(x,y); m=imul(q,y) remains in graph",
            "signedness": "unsigned quotient, remainder; bitvectors modulo 2^64",
            "division_by_zero": "both must trap; value proof excludes y=0",
            "overflow": "nonzero y implies mathematical q*y<=x, no unsigned product overflow; modular CLIF imul/isub modeled",
            "flags": "pure CLIF value graph only; x86 flags not checked",
            "undef_poison": "not admitted by modeled total CLIF integer operators; no LLVM poison theorem",
            "target_assumptions": "x86-64-only compiler patch, though the pure arithmetic identity is target independent",
        },
        "limitations": [
            "Source anchored by exact Git blob and selected literals, not a formal Rust/SSA pass proof",
            "Exhaustive 8-bit checks cannot establish 64-bit equivalence",
            "Only an UNSAT 64-bit QF_BV counterexample query is an SMT proof; UNKNOWN is not a proof",
            "No whole-program trap-order, debug metadata, liveness, flag, ABI, register allocation, or target instruction proof",
            "Does not verify native code identity or derive any byte/throughput saving",
        ],
    }
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text(json.dumps(receipt, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"finite_pairs": f["nontrapping_pairs"],
                      "smt_result": proof["counterexample_query_result"],
                      "smt_classification": proof["classification"]}, sort_keys=True))
    if proof["counterexample_query_result"] == "sat":
        raise SystemExit("counterexample: this candidate cannot be qualified")


if __name__ == "__main__":
    main()
