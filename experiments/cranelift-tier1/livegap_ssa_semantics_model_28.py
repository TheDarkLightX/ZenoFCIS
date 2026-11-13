#!/usr/bin/env python3
"""Bounded independent SSA/trap *model*, not Cranelift compiler-pass verification.

Original: q=udiv(x,y); m=imul(q,y); c=iconst; rot=rotl(m,c);
          r=isub(x,m), then consume q,m,rot,r.
Patched:  (q,r)=udivrem(x,y); m=imul(q,y); c=iconst;
          rot=rotl(m,c), then consume all values.
The multiply remains live. The actual experimental matcher admits only this
exact same-block unsigned-i64 shape, and only on x86-64.

Bit-serial division is intentionally independent of Python's // oracle.
No flags, machine ISA, poison, source locations, register allocation, or
arbitrary control-flow claims are derived from this model.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
import random
from pathlib import Path

PIN = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"

def rot(x, n, width):
    mask = (1 << width) - 1
    k = n % width
    return ((x << k) | (x >> (width - k))) & mask

def divide_bitserial(a, b, width):
    if b == 0:
        raise ZeroDivisionError()
    q = r = 0
    for shift in reversed(range(width)):
        r = (r << 1) | ((a >> shift) & 1)
        if r >= b:
            r -= b
            q |= 1 << shift
    return q, r

def original(x, y, c, width):
    mask = (1 << width) - 1
    a, b = x & mask, y & mask
    if not b:
        return ("trap", 0)
    q = a // b
    m = (q * b) & mask
    t = rot(m, c, width)
    r = (a - m) & mask
    result = q ^ rot(m, 1, width) ^ rot(r, 3, width) ^ t
    return ("value", (q, m, t, r, result & mask))

def transformed(x, y, c, width):
    mask = (1 << width) - 1
    a, b = x & mask, y & mask
    if not b:
        return ("trap", 0)
    q, r = divide_bitserial(a, b, width)
    m = (q * b) & mask
    t = rot(m, c, width)
    result = q ^ rot(m, 1, width) ^ rot(r, 3, width) ^ t
    return ("value", (q, m, t, r, result & mask))

def check_pair(x, y, c, width):
    lhs = original(x, y, c, width)
    rhs = transformed(x, y, c, width)
    if lhs != rhs:
        raise AssertionError(f"SSA/trap counterexample {width=} {x=} {y=} {c=} {lhs=} {rhs=}")
    return lhs[0]

def mutation_witnesses():
    """Show why operand-shape exclusions matter; NOT source-matcher tests."""
    evidence = {}
    for width in (8, 64):
        mask = (1 << width) - 1
        for x in range(1, 24):
            for y in range(1, 16):
                q = x // y
                product = q * y
                true_r = (x - product) & mask
                mutants = {
                    "imul_q_x_not_q_y": (x - q * x) & mask,
                    "isub_m_x_not_x_m": (product - x) & mask,
                    "wrong_dividend": ((x + 1) & mask) % y,
                }
                for label, wrong in mutants.items():
                    key = f"i{width}:{label}"
                    if key not in evidence and wrong != true_r:
                        evidence[key] = {"x":x,"y":y,"correct":true_r,"mutant":wrong}
    if len(evidence) != 6:
        raise AssertionError("missing negative mutation witnesses")
    return evidence

def run():
    counter = {"i8_exhaustive":{"value":0,"trap":0},
               "i64_finite":{"value":0,"trap":0}}
    for x in range(256):
        for y in range(256):
            for c in (0, 1, 3, 7):
                counter["i8_exhaustive"][check_pair(x,y,c,8)] += 1
    rng = random.Random(0x20261113)
    edge = [0,1,2,3,7,(1<<32)-1,1<<32,(1<<63)-1,
            1<<63,(1<<64)-2,(1<<64)-1]
    pairs = list(dict.fromkeys(
        [(a,b) for a in edge for b in edge] +
        [(rng.getrandbits(64),rng.getrandbits(64)) for _ in range(2048)]
    ))
    for x,y in pairs:
        for c in (0,1,7,17,63):
            counter["i64_finite"][check_pair(x,y,c,64)] += 1
    if counter["i8_exhaustive"] != {"value":261120,"trap":1024}:
        raise AssertionError("i8 domain coverage incomplete")
    if not counter["i64_finite"]["trap"] or not counter["i64_finite"]["value"]:
        raise AssertionError("i64 finite trap/value domains incomplete")
    return {
        "schema":"zenofcis/tier1-livegap-ssa-model-audit/1",
        "status":"passed-model-only-not-compiler-proof",
        "experiment_source_sha256":hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "zenofcis_checkout_commit":os.getenv("GITHUB_SHA","local-unpinned"),
        "wasmtime_source_pin":PIN,
        "contract":{
            "domain":"unsigned i8 exhaustive; unsigned i64 sampled; actual experimental patch x86-64 i64 only",
            "overflow":"imul and isub wrap modulo 2^width",
            "definedness":"division by zero traps at instruction 0 in both modeled programs",
            "live_values":"compare quotient, intermediate product, intermediate rotation, remainder and downstream result",
            "intervening_ops":"iconst then rotl(m,c), pure and nontrapping only",
            "no_poison_or_undefined":"not modeled; both assume defined concrete integer inputs",
            "flags_and_machine_state":"not modeled; no ABI, flags, physical registers, or source-locations inference"
        },
        "coverage":counter,
        "unsound_generalization_witnesses":mutation_witnesses(),
        "limitations":[
            "Python small-step equivalence is NOT a Cranelift compiler-pass, ISLE, VeriISLE or x86 proof",
            "i8 domain exhaustive; i64 only finite and not all input/rotation combinations",
            "Syntactic exclusion by the actual Rust matcher is a separate verification obligation",
            "Flags, undefined/poison semantics, target effects, SSA repair, trap metadata and register allocation remain unproved",
            "No native byte-cost or throughput measurements are made by this audit"
        ]
    }

if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--out", type=Path)
    opts = p.parse_args()
    output = run()
    if opts.out:
        if opts.out.exists():
            p.error("refusing evidence overwrite")
        opts.out.parent.mkdir(parents=True,exist_ok=True)
        opts.out.write_text(json.dumps(output,sort_keys=True,indent=2)+"\n")
    print(json.dumps({"status":output["status"],"coverage":output["coverage"],
                      "mutant_witnesses":len(output["unsound_generalization_witnesses"])},
                     sort_keys=True))
