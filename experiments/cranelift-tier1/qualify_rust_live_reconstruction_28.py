#!/usr/bin/env python3
"""Source-pinned Rust/LLVM live-SSA reconstruction screen, no compiler-pass proof.

Reuses the existing executable Rust-to-Wasm producer and separately implemented
fresh-input rechecker with a different source and two explicitly independent
arithmetic oracles. Crucially, no previously qualified experiment's fixture or
receipt is overwritten; only the isolated research branch is modified.
"""
from __future__ import annotations
import argparse
import json
import random
from pathlib import Path

import qualify_rust_divrem_28 as producer
import recheck_rust_divrem_28 as auditor

HERE = Path(__file__).resolve().parent
SOURCE = HERE / "rust_live_reconstruction_28.rs"
EXPORTS = ("live_q_m_r", "live_reversed_mul", "live_original_rem",
           "live_two_chains", "wrong_multiplier")
SCHEMA = "zenofcis/tier1-rust-live-reconstruction/1"
PROFILES = {"optimized": "3"}
MOD = 1 << 64
MASK = MOD - 1

def rot(value: int, amount: int) -> int:
    v = value & MASK
    return ((v << amount) | (v >> (64 - amount))) & MASK

def oracle(name: str, x: int, y: int) -> int | None:
    """Producer oracle: fixed-width unsigned quotient then wrapping operations."""
    if name not in EXPORTS or not -(1 << 63) <= x < (1 << 63) or not -(1 << 63) <= y < (1 << 63):
        raise ValueError("out-of-domain function or i64 CLI input")
    a, b = x & MASK, y & MASK
    if b == 0:
        return None
    q = a // b
    m = (q * (a if name == "wrong_multiplier" else b)) & MASK
    r = a % b if name == "live_original_rem" else (a - m) & MASK
    result = q ^ rot(m, 7) ^ rot(r, 17)
    if name == "live_two_chains":
        a2 = (a + 17) & MASK
        q2 = a2 // b
        m2 = (q2 * b) & MASK
        r2 = (a2 - m2) & MASK
        result ^= rot(q2, 19) ^ rot(m2, 23) ^ rot(r2, 29)
    return producer.signed(result)

def independently_calculated(name: str, x: int, y: int) -> int | None:
    """Fresh-input auditor: divmod followed by modular exponent-of-two arithmetic."""
    if name not in EXPORTS:
        raise ValueError("unrecognized export")
    u = x % MOD
    v = y % MOD
    if not v:
        return None
    quotient, genuine_remainder = divmod(u, v)
    product = (quotient * (u if name == "wrong_multiplier" else v)) % MOD
    remainder = genuine_remainder if name == "live_original_rem" else (u - product) % MOD
    # Rotation expressed as modulo multiplication + a separate right-shift.
    circular = lambda z,n: ((z * (2 ** n)) % MOD) | (z // (2 ** (64 - n)))
    answer = quotient ^ circular(product, 7) ^ circular(remainder, 17)
    if name == "live_two_chains":
        u2 = (u + 17) % MOD
        quotient2, remainder2 = divmod(u2, v)
        product2 = (u2 - remainder2) % MOD
        answer ^= circular(quotient2, 19) ^ circular(product2, 23) ^ circular(remainder2, 29)
    return ((answer + (1 << 63)) % MOD) - (1 << 63)

def configure():
    # Only this process's imported modules are rebound; no files or baseline
    # artifacts in their existing independently qualified experiment are edited.
    producer.SOURCE = SOURCE
    producer.EXPORTS = EXPORTS
    producer.SCHEMA = SCHEMA
    producer.PROFILES = PROFILES
    producer.oracle = oracle
    auditor.EXPORTS = EXPORTS
    auditor.SCHEMA = SCHEMA
    auditor.PROFILES = PROFILES
    auditor.independent_oracle = independently_calculated

def self_test() -> None:
    assert SOURCE.is_file()
    src = SOURCE.read_text()
    assert all(("fn " + name + "(") in src for name in EXPORTS)
    rng = random.Random(0x20261112)
    pairs = [(7, 3), (7, 0), (0, 1), (-1, -1),
             (-(1 << 63), -1), (-(1 << 63), 1)]
    pairs += [(producer.signed(rng.getrandbits(64)),
               producer.signed(rng.getrandbits(64))) for _ in range(256)]
    for name in EXPORTS:
        for x, y in pairs:
            a, b = oracle(name, x, y), independently_calculated(name, x, y)
            if a != b:
                raise AssertionError(f"independent oracle disagreement: {name} {x} {y}: {a} vs {b}")
    assert oracle("live_q_m_r", 7, 3) == oracle("live_reversed_mul", 7, 3)
    assert oracle("live_q_m_r", 7, 3) == oracle("live_original_rem", 7, 3)
    assert oracle("live_q_m_r", 7, 3) != oracle("wrong_multiplier", 7, 3)
    assert all(oracle(name, 7, 0) is None for name in EXPORTS)
    print("Independent live-SSA producer/auditor oracles: 1310 checks, negative controls passed")

def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--results", type=Path)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--recheck", action="store_true")
    args = parser.parse_args()
    configure()
    self_test()
    if args.self_test:
        return
    if args.archive is None:
        parser.error("--archive is required")
    archive = args.archive.resolve()
    if args.recheck:
        if args.results is None:
            parser.error("--results is required for --recheck")
        result = auditor.recheck(archive, args.results.resolve())
        print(json.dumps(result, sort_keys=True))
        return
    if args.out is None:
        parser.error("--out is required")
    result = producer.qualify(archive, args.out.resolve())
    result["limitations"].extend([
        "The producer uses a new Rust source but shares the pinned producer harness.",
        "Its different fresh-input oracle runs in a separate auditable source function.",
        "All five functions share one optimized Wasm module, so whole-module cost changes are not isolated per function.",
        "The negative-control function requires subsequent per-symbol inspection before any match-selectivity claim.",
        "A source-level q,m,r use does not guarantee LLVM preserves the same CLIF SSA shape.",
        "Changes to module-wide x86 bytes/ELF text are not individual rewrite profitability or speedups.",
        "No compiler-pass safety proof, VeriISLE result, or newly synthesized ZenoFCIS sequence is claimed.",
    ])
    (args.out.resolve() / "qualification.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps({k: v["baseline_minus_patched"] for k, v in result["profiles"].items()}, sort_keys=True))

if __name__ == "__main__":
    main()
