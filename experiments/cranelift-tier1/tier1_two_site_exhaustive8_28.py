#!/usr/bin/env python3
"""Independent exhaustive 8-bit model for the pinned two-pair Wasmtime WAT.

This is not a proof of the actual x64 compiler pass or of all 64-bit values.
It exercises two quotient/remainder pairs, wrapping derived operands, and
FIRST vs SECOND zero-divisor precedence with distinct independent algorithms.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path

PIN_WASMTIME = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
PIN_WAT_SHA256 = "763d99565f7ec0530db081209bd6cea968a2bdcf58deb84b690209394cba634d"
WIDTH = 8
MASK = (1 << WIDTH) - 1


def roll(v: int, count: int) -> int:
    n = count % WIDTH
    return ((v << n) | (v >> (WIDTH - n))) & MASK if n else (v & MASK)


def baseline(x: int, y: int):
    """Separate unsigned quotient and remainder operations (Python integer oracle)."""
    if y == 0:
        return "trap:first", None
    q1 = x // y
    r1 = x % y
    xx, yy = (x + 1) & MASK, (y + 1) & MASK
    if yy == 0:
        return "trap:second", None
    q2 = xx // yy
    r2 = xx % yy
    return "value", ((q1 ^ roll(r1, 17)) ^ (q2 ^ roll(r2, 17))) & MASK


def bitserial_divide(dividend: int, divisor: int):
    """Separate restoring-division implementation; no //, %, or divmod."""
    if divisor == 0:
        raise ZeroDivisionError("unsigned division by zero")
    q, r = 0, 0
    for bit in range(WIDTH - 1, -1, -1):
        r = (r << 1) | ((dividend >> bit) & 1)
        if r >= divisor:
            r -= divisor
            q |= 1 << bit
    return q, r


def candidate(x: int, y: int):
    """One quotient-and-remainder-producing primitive per operand pair."""
    if y == 0:
        return "trap:first", None
    q1, r1 = bitserial_divide(x, y)
    xx, yy = (x + 1) & MASK, (y + 1) & MASK
    if yy == 0:
        return "trap:second", None
    q2, r2 = bitserial_divide(xx, yy)
    return "value", ((q1 ^ roll(r1, 17)) ^ (q2 ^ roll(r2, 17))) & MASK


def mutants():
    """Wrong candidates must have observable witnesses, never proof authority."""
    def wrong_second_divisor(x, y):
        if y == 0: return "trap:first", None
        q1, r1 = bitserial_divide(x, y)
        xx = (x + 1) & MASK
        q2, r2 = bitserial_divide(xx, y)  # error: not (y+1) & MASK
        return "value", ((q1 ^ roll(r1, 17)) ^ (q2 ^ roll(r2, 17))) & MASK

    def wrong_second_trap(x, y):
        if y == 0: return "trap:first", None
        if y == MASK: return "trap:first", None  # error: collapsed source site
        return candidate(x, y)

    def wrong_live_remainder(x, y):
        status, value = candidate(x, y)
        if status != "value": return status, value
        q1, r1 = bitserial_divide(x, y)
        q2, _ = bitserial_divide((x+1)&MASK, (y+1)&MASK)
        return "value", ((q1 ^ roll(r1, 17)) ^ (q2 ^ roll(r1, 17))) & MASK

    def wrong_wrap(x, y):
        if y == 0: return "trap:first", None
        q1, r1 = bitserial_divide(x, y)
        q2, r2 = bitserial_divide(x+1, y+1)  # error: no modular operand wrap
        return "value", ((q1 ^ roll(r1, 17)) ^ (q2 ^ roll(r2, 17))) & MASK

    return {"wrong_second_divisor": wrong_second_divisor,
            "wrong_second_trap_site": wrong_second_trap,
            "wrong_live_remainder": wrong_live_remainder,
            "wrong_operand_wrap": wrong_wrap}


def audit():
    # Bind the semantic tests to the exact source WAT used in compiler A/B.
    from qualify_udivrem_contexts_28 import CASES
    actual = hashlib.sha256(CASES["two_live_pairs"].encode()).hexdigest()
    if actual != PIN_WAT_SHA256:
        raise ValueError(f"source WAT changed: {actual}")
    distribution = {"value": 0, "trap:first": 0, "trap:second": 0}
    tested_mutants = mutants()
    witnesses = {}
    for x in range(1 << WIDTH):
        for y in range(1 << WIDTH):
            expected = baseline(x, y)
            observed = candidate(x, y)
            if observed != expected:
                raise AssertionError(f"positive semantic mismatch x={x} y={y}: {expected} != {observed}")
            distribution[expected[0]] += 1
            for name, faulty in tested_mutants.items():
                if name not in witnesses and faulty(x, y) != expected:
                    witnesses[name] = {"x": x, "y": y,
                                       "expected_status": expected[0],
                                       "faulty_status": faulty(x, y)[0]}
    if distribution != {"value": 65024, "trap:first": 256, "trap:second": 256}:
        raise AssertionError(f"unexpected trap distribution {distribution}")
    if set(witnesses) != set(tested_mutants):
        raise AssertionError(f"negative mutant survived: {set(tested_mutants)-set(witnesses)}")
    return {"schema": "zenofcis/tier1-two-site-exhaustive-bitserial/1",
            "wasmtime_pin": PIN_WASMTIME,
            "wat_sha256": actual,
            "checker_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
            "result": "passed-exhaustive-8-bit-model",
            "admitted_width": WIDTH,
            "all_input_pairs": 1 << (2 * WIDTH),
            "case_counts": distribution,
            "negative_mutation_witnesses": witnesses,
            "reference": "Python unsigned // and %; sequential pair traps",
            "candidate": "independent bit-serial restoring unsigned division producing both results",
            "semantics": {"signedness": "unsigned", "overflow": "8-bit wrapping derived numerator and denominator",
                          "trap_order": "first y==0; then (y+1 mod 256)==0",
                          "flags": "not observed by selected WebAssembly function",
                          "poison_undefined": "none modeled; source WebAssembly operations are defined except divide-by-zero"},
            "limits": ["This is exhaustive only for the 8-bit scaled model, not SMT over all u64 inputs",
                       "Not a proof of compiler pass, native flag/ABI/register effects, trap tables, cross-target validity or speed",
                       "Emitted x86 byte savings and execution are separate archived evidence"]}


if __name__ == "__main__":
    print(json.dumps(audit(), indent=2, sort_keys=True))
