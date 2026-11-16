#!/usr/bin/env python3
"""Direct SMT integer-division obligations for the two-site unsigned udivrem case.

A proof over SMT Int div/mod, not an assumed quotient/remainder witness.
Does NOT prove bitvector-to-Int refinement, native x86 machine-state/flags,
Cranelift pass correctness, or performance. Treat any unknown as unproved.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent
WASMTIME = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
WAT_SHA256 = "763d99565f7ec0530db081209bd6cea968a2bdcf58deb84b690209394cba634d"
PIN_INSTALLERS = {
    "install_udivrem_combiner_28.py": "1e5ad0cc46b8ce791b14cad6998df4f32ea474ec",
    "install_udivrem_multivalue_28.py": "01b92461269b5ee354287a48a6750143e7564dc9",
}
U64 = (1 << 64) - 1

def git_blob(raw: bytes) -> str:
    return hashlib.sha1(b"blob " + str(len(raw)).encode() + b"\0" + raw).hexdigest()

def source_pins() -> dict:
    from qualify_udivrem_contexts_28 import CASES
    observed = hashlib.sha256(CASES["two_live_pairs"].encode()).hexdigest()
    if observed != WAT_SHA256:
        raise ValueError("source WAT drift: " + observed)
    got = {}
    for name, expected in PIN_INSTALLERS.items():
        raw = (ROOT / name).read_bytes()
        actual = git_blob(raw)
        if actual != expected:
            raise ValueError("compiler installer drift: " + name)
        got[name] = {"git_blob": actual, "sha256": hashlib.sha256(raw).hexdigest()}
    return {"wasmtime": WASMTIME, "wat_sha256": observed, "installer_blobs": got}

def query(z3, name: str, premises: list, counterexample, timeout_ms: int) -> dict:
    s = z3.Solver()
    s.set(timeout=timeout_ms)
    s.add(*premises)
    s.add(counterexample)
    res = s.check()
    item = {"name": name, "counterexample_query": str(res),
            "classification": {"unsat": "proved-under-SMT-Int-or-BV-semantics",
                               "sat": "counterexample-found",
                               "unknown": "unknown-not-proof"}[str(res)]}
    if res == z3.sat:
        model = s.model()
        item["witness"] = {str(d): str(model[d]) for d in model.decls()}
    if res == z3.unknown:
        item["reason"] = s.reason_unknown()
    return item

def prove(timeout_ms: int):
    import z3
    a, b = z3.Ints("a b")
    domain = [a >= 0, a <= U64, b >= 1, b <= U64]
    q = a / b
    r = a % b
    duties = [
        query(z3, "direct-Euclidean-reconstruction",
              domain, a - q*b != r, timeout_ms),
        query(z3, "quotient-and-product-fit-u64",
              domain, z3.Or(q < 0, q > U64, q*b < 0, q*b > U64), timeout_ms),
        query(z3, "remainder-range",
              domain, z3.Or(r < 0, r >= b), timeout_ms),
    ]
    # Second input is the original y+1, wrapped; the first site is reached
    # only for y!=0 and the second is reached only if the first did not trap.
    x, y = z3.Ints("x y")
    finite = [x >= 0, x <= U64, y > 0, y < U64]
    x2 = z3.If(x == U64, 0, x+1)
    y2 = y+1
    duties += [
        query(z3, "second-pair-remains-in-admitted-domain", finite,
              z3.Or(x2 < 0, x2 > U64, y2 < 1, y2 > U64), timeout_ms)
    ]
    # This machine-width trace uses QF_BV without treating SMT's *total*
    # UDiv at zero as a Wasm trap. Zero-divisor trap predicates are explicit.
    by = z3.BitVec("source_y", 64)
    first = (by == z3.BitVecVal(0, 64))
    second = (by + z3.BitVecVal(1, 64) == z3.BitVecVal(0, 64))
    verdict = z3.If(first, z3.IntVal(1), z3.If(second, z3.IntVal(2), z3.IntVal(0)))
    duties += [
        query(z3, "first-site-exact", [], (verdict == 1) != first, timeout_ms),
        query(z3, "second-site-exact", [],
              (verdict == 2) != (by == z3.BitVecVal(U64, 64)), timeout_ms),
        query(z3, "sites-disjoint", [], z3.And(first, second), timeout_ms),
    ]
    mutants = [
        query(z3, "MUTANT-wrong-quotient-divisor", domain + [b < U64],
              a - (a / (b+1))*b != r, timeout_ms),
        query(z3, "MUTANT-no-second-trap", [], (verdict == 2), timeout_ms),
        query(z3, "MUTANT-signed-instead-of-unsigned-division", [],
              z3.And(z3.UGT(z3.BitVec("sx",64), z3.BitVecVal(1<<63,64)),
                     z3.BitVec("sy",64) == z3.BitVecVal(2,64),
                     z3.BitVec("sx",64) / z3.BitVec("sy",64) !=
                     z3.UDiv(z3.BitVec("sx",64), z3.BitVec("sy",64))),
              timeout_ms),
    ]
    return duties, mutants, z3.get_version_string()

def bounded_self_test() -> None:
    source_pins()
    for a in range(256):
        for b in range(1, 256):
            q, r = divmod(a, b)
            assert a-q*b == r and 0 <= q*b <= 255 and 0 <= r < b
    assert (U64+1) & U64 == 0
    print("65,280 finite 8-bit oracle pairs and source pins passed; not an SMT proof")

def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("--out", type=Path)
    ap.add_argument("--timeout-ms", type=int, default=30000)
    args = ap.parse_args()
    if args.self_test:
        bounded_self_test()
        return
    if not args.out or args.out.exists() or not 1000 <= args.timeout_ms <= 120000:
        ap.error("new --out and timeout 1000..120000 required")
    pins = source_pins()
    duties, mutants, solver = prove(args.timeout_ms)
    positive = all(r["counterexample_query"] == "unsat" for r in duties)
    negative = all(r["counterexample_query"] == "sat" for r in mutants)
    receipt = {
        "schema": "zenofcis/tier1-udivrem-direct-int-division-theorem/1",
        "source_pins": pins,
        "checker_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "solver": "Z3 " + solver,
        "admitted_domain": "all unsigned u64 numerator, nonzero u64 divisor; two-site finite path 1<=y<2^64-1",
        "definition": "SMT-LIB Int div and mod for positive divisor; no q/r Euclidean-law premises",
        "rewrite": "return (a div b, a mod b) -> (q=a div b, r=a-q*b) for b>0",
        "source_trace": "first trap if y=0; after first succeeds second trap if wrap64(y+1)=0; otherwise both finite",
        "obligations": duties, "negative_controls": mutants,
        "all_obligations_unsat": positive, "all_mutants_sat": negative,
        "scope": [
            "Direct arithmetic proof over SMT integer division/modulo semantics, not a conditional assumed-quotient theorem",
            "No proof connecting SMT Int division to Cranelift's udivrem opcode implementation or native x86 DIV",
            "No proof of compiler pass, SSA, register liveness, flags, poison/undef, target traps, or throughput",
            "Unknown, timeout, and SAT are never accepted as positive correctness evidence",
            "No new instruction sequence was discovered or benchmarked in this check"
        ],
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"positive_unsat": sum(x["counterexample_query"]=="unsat" for x in duties),
                      "positive_total":len(duties),
                      "mutants_sat":sum(x["counterexample_query"]=="sat" for x in mutants),
                      "mutants_total":len(mutants),
                      "qualified":positive and negative}, sort_keys=True))
    if not (positive and negative):
        raise SystemExit("unproved obligation or missing negative witness: inspect receipt")

if __name__ == "__main__":
    main()
