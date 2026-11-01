"""Independent QF_BV proof of a CLIF-only signed i64 saturating-add rewrite.

No claim is made about a compiled x86 instruction sequence or equivalence of
different ABI/prologue, flags-liveness or poison observations. This proves the
declared pure result semantics of *two exact pinned CLIF expression bodies*.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import random
from pathlib import Path

import z3

HERE = Path(__file__).resolve().parent
BASE = "satadd.clif"
VARIANT = "satadd_shift.clif"
PINNED_UPSTREAM = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
ZENO_BASE = "31afaef8d9fb723b44918f430e10d63f0cb93ccf"
MASK = (1 << 64) - 1
HI = (1 << 63) - 1
LO = -(1 << 63)

ORIGINAL = (
    "v2, v3 = sadd_overflow v0, v1",
    "v4 = iconst.i64 9223372036854775807",
    "v5 = iconst.i64 -9223372036854775808",
    "v6 = iconst.i64 0",
    "v7 = icmp slt v0, v6",
    "v8 = select v7, v5, v4",
    "v9 = select v3, v8, v2",
    "return v9",
)
SHIFT = (
    "v2 = iconst.i64 63",
    "v3 = ushr v0, v2",
    "v4 = iconst.i64 9223372036854775807",
    "v5 = iadd v4, v3",
    "v6, v7 = sadd_overflow v0, v1",
    "v8 = select v7, v5, v6",
    "return v8",
)


# A manual SMT model cannot validate a CLIF program that has a different
# signature, hidden instructions, extra blocks, or a different compilation policy.
# Pin the entire executable, non-comment structure rather than a body substring.
EXPECTED_HEADERS = {
    BASE: ("test compile precise-output", "set opt_level=speed_and_size",
           "target x86_64", "function %tier1_signed_satadd(i64, i64) -> i64 {"),
    VARIANT: ("test compile precise-output", "set opt_level=speed_and_size",
              "target x86_64", "function %tier1_signed_satadd_shift(i64, i64) -> i64 {"),
    "satadd_run.clif": ("test run", "target x86_64",
                        "set opt_level=speed_and_size",
                        "function %tier1_signed_satadd_run(i64, i64) -> i64 {"),
    "satadd_shift_run.clif": ("test run", "set opt_level=speed_and_size",
                              "target x86_64",
                              "function %tier1_signed_satadd_shift_run(i64, i64) -> i64 {"),
}


def function_body(path: Path) -> tuple[str, ...]:
    """Refuse unmodeled executable CLIF drift, not merely expression drift."""
    if path.name not in EXPECTED_HEADERS:
        raise ValueError(f"{path.name}: unregistered CLIF fixture")
    lines = tuple(line.strip() for line in path.read_text().splitlines()
                  if line.strip() and not line.lstrip().startswith(";"))
    expected_header = EXPECTED_HEADERS[path.name]
    if lines[:4] != expected_header:
        raise ValueError(f"{path.name}: exact header, function signature or ISA settings changed")
    if lines[4:5] != ("block0(v0: i64, v1: i64):",) or lines[-1:] != ("}",):
        raise ValueError(f"{path.name}: CLIF block/terminator or function boundaries changed")
    return lines[5:-1]


def ensure_source_binding() -> dict[str, str]:
    for name, body in ((BASE, ORIGINAL), (VARIANT, SHIFT),
                       ("satadd_run.clif", ORIGINAL),
                       ("satadd_shift_run.clif", SHIFT)):
        if function_body(HERE / name) != body:
            raise ValueError(f"{name}: CLIF body changed; proof no longer applies")
    return {name: hashlib.sha256((HERE / name).read_bytes()).hexdigest()
            for name in (BASE, VARIANT, "satadd_run.clif",
                         "satadd_shift_run.clif")}


def signed(value: int) -> int:
    return value - (1 << 64) if value >> 63 else value


def oracle(x: int, y: int) -> int:
    return min(HI, max(LO, signed(x) + signed(y))) & MASK


def candidate(x: int, y: int) -> int:
    s = (x + y) & MASK
    neg_x, neg_y, neg_sum = bool(x >> 63), bool(y >> 63), bool(s >> 63)
    over = (neg_x == neg_y) and (neg_sum != neg_x)
    return ((HI + (x >> 63)) & MASK) if over else s


def prove(timeout_ms: int = 30000) -> dict:
    sources = ensure_source_binding()
    rng = random.Random(0xC0FFEE)
    edges = [0, 1, 2, MASK, MASK - 1, (1 << 63), (1 << 63) + 1,
             (1 << 63) - 1, (1 << 62), (1 << 62) - 1]
    vectors = [(x, y) for x in edges for y in edges]
    vectors.extend((rng.getrandbits(64), rng.getrandbits(64))
                   for _ in range(10000))
    for x, y in vectors:
        if candidate(x, y) != oracle(x, y):
            raise ValueError(f"independent integer-oracle mismatch x={x} y={y}")

    x, y = z3.BitVecs("x y", 64)
    sum_word = x + y
    sign_x = z3.Extract(63, 63, x) == z3.BitVecVal(1, 1)
    sign_y = z3.Extract(63, 63, y) == z3.BitVecVal(1, 1)
    sign_sum = z3.Extract(63, 63, sum_word) == z3.BitVecVal(1, 1)
    of = z3.And(sign_x == sign_y, sign_x != sign_sum)
    old = z3.If(of,
                z3.If(sign_x, z3.BitVecVal(1 << 63, 64),
                      z3.BitVecVal(HI, 64)),
                sum_word)
    new = z3.If(of,
                z3.BitVecVal(HI, 64) +
                z3.LShR(x, z3.BitVecVal(63, 64)),
                sum_word)
    solver = z3.SolverFor("QF_BV")
    solver.set(timeout=timeout_ms)
    solver.add(old != new)
    result = solver.check()
    if result != z3.unsat:
        raise ValueError(f"equivalence not proven; solver={result}, reason={solver.reason_unknown()}")

    return {
        "schema": "zeno-fcis/cranelift-tier1-clif-equivalence/1",
        "status": "unsat-counterexample-query",
        "interpretation": "QF_BV proof for signed i64 saturating result on all 2^128 input pairs",
        "evidence_type": "independent SMT equivalence of exact bound CLIF expression bodies",
        "proof_condition": "exists x,y: original_result(x,y) != shift_result(x,y)",
        "solver": {"name": "Z3", "version": z3.get_version_string(),
                   "logic": "QF_BV", "timeout_ms": timeout_ms},
        "source_files_sha256": sources,
        "source_pins": {"zenofcis_base": ZENO_BASE, "wasmtime": PINNED_UPSTREAM},
        "finite_oracle_checks": len(vectors),
        "machine_code_qualification": "not established",
        "cranelift_native_byte_savings": "not established",
        "nonclaims": [
            "No machine-code, register-allocation, flag-liveness, ABI or poison proof",
            "No guarantee that the alternative produces fewer bytes or cycles",
            "CLIF run fixtures and pinned Cranelift compile required independently",
        ],
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if args.out.exists():
        raise SystemExit("refuse existing output: use a new receipt path")
    try:
        receipt = prove()
    except (ValueError, OSError) as error:
        raise SystemExit(f"SMT qualification refused: {error}") from error
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(receipt, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"status": receipt["status"],
                      "finite_checks": receipt["finite_oracle_checks"],
                      "cranelift_savings": "not established"}))


if __name__ == "__main__":
    main()
