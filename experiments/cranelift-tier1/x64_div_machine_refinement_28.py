#!/usr/bin/env python3
"""Independent, bounded x86-64 DIV bridge qualification for the Cranelift Udivrem pilot.

Do not mistake this for verification of the Rust rewrite, whole ISA, flags,
register allocation, signal-to-Wasm trap mapping, or ABI. The tested native
kernel is standalone, not extracted from the compiler's emitted code.
"""
from __future__ import annotations

import argparse
import ctypes
import hashlib
import json
import platform
import random
import resource
import signal
import subprocess
from pathlib import Path

PIN = "8b19f199f89da864c529b3fa3ca2f7e40d5de609"
UPSTREAM_PATH = "cranelift/codegen/src/isa/x64/lower.isle"
UPSTREAM_BLOB = "654189182a09c1e22478faa157cd421ff8ed8894"
MASK = (1 << 64) - 1
SCHEMA = "zenofcis/tier1-x64-div-machine-bridge/1"
SOURCE = r"""
#include <stdint.h>
#include <inttypes.h>

/* Exact 64-bit unsigned DIV with explicitly zeroed high dividend.
 * ABI: SysV x86-64; implicit outputs RAX=quotient, RDX=remainder.
 * DIV leaves arithmetic flags undefined. The caller does not inspect them.
 */
__attribute__((visibility("default"),noinline))
uint64_t native_udivrem(uint64_t dividend, uint64_t divisor, uint64_t *rem) {
    uint64_t quotient, remainder;
    __asm__ volatile(
        "xor %%edx, %%edx\n\t"
        "divq %[divisor]"
        : "=a"(quotient), "=&d"(remainder)
        : "0"(dividend), [divisor]"r"(divisor)
        : "cc"
    );
    *rem = remainder;
    return quotient;
}

#ifdef ZERO_DIVISOR_SIGNAL
int main(void) {
    uint64_t rem = 0;
    volatile uint64_t q = native_udivrem(7, 0, &rem);
    (void)q;
    return 0;
}
#endif
"""

def digest(path: Path) -> str:
    if not path.is_file() or path.is_symlink():
        raise ValueError("missing or linked input " + str(path))
    return hashlib.sha256(path.read_bytes()).hexdigest()

def cmd(*args: str, cwd: Path | None = None) -> str:
    return subprocess.check_output(args, text=True, cwd=cwd).strip()

def source_guard(upstream: Path) -> dict:
    if cmd("git", "-C", str(upstream), "rev-parse", "HEAD") != PIN:
        raise ValueError("different upstream revision")
    path = upstream / UPSTREAM_PATH
    if cmd("git", "-C", str(upstream), "hash-object", UPSTREAM_PATH) != UPSTREAM_BLOB:
        raise ValueError("different upstream x64 lowering source blob")
    source = path.read_text()
    # Both existing scalar operations have the same zero-high x64_div helper.
    scalar = "(x64_div ty a (imm $I64 0) (put_in_gpr b) (TrapCode.INTEGER_DIVISION_BY_ZERO))"
    if source.count(scalar) != 2:
        raise ValueError("scalar lowering anchor drifted or changed multiplicity")
    import install_udivrem_multivalue_28 as pilot
    if ("(lower (udivrem _ a @ (value_type $I64) b))" not in pilot.X64_NEW
        or "(x64_div $I64 a (imm $I64 0) (put_in_gpr b" not in pilot.X64_NEW
        or "(output_pair (value_regs_get qr 0) (value_regs_get qr 1))" not in pilot.X64_NEW):
        raise ValueError("experimental combined lowering differs from declared pair")
    return {"upstream_commit": PIN, "upstream_blob": UPSTREAM_BLOB,
            "experimental_rule_sha256": hashlib.sha256(pilot.X64_NEW.encode()).hexdigest(),
            "anchor_class": "textual source guard, not compiled lowering proof"}

def compile_native(out: Path) -> None:
    out.mkdir(parents=True, exist_ok=False)
    (out / "div-bridge.c").write_text(SOURCE)
    for flag, name, extra in [
        ("shared", "bridge.so", ["-fPIC", "-shared"]),
        ("zero", "zero-divisor", ["-DZERO_DIVISOR_SIGNAL"])]:
        p = subprocess.run(["cc", "-O2", "-std=c11", "-Wall", "-Wextra", "-Werror",
                            *extra, str(out / "div-bridge.c"), "-o", str(out / name)],
                           capture_output=True, text=True)
        if p.returncode:
            raise RuntimeError("native "+flag+" compilation failed: "+p.stderr)

def vectors():
    edges = (0, 1, 2, 3, 7, 255, 256, (1 << 31)-1, 1 << 31,
             (1 << 63)-1, 1 << 63, MASK-1, MASK)
    seen = set()
    for x in range(256):
        for y in range(1, 256):
            seen.add((x, y))
            yield x, y
    for x in edges:
        for y in edges:
            if y and (x, y) not in seen:
                seen.add((x, y))
                yield x, y
    r = random.Random(0xC1A7F_2026)
    for _ in range(10000):
        x, y = r.getrandbits(64), r.getrandbits(64) or 1
        if (x, y) not in seen:
            seen.add((x, y))
            yield x, y

def native_check(out: Path) -> dict:
    if platform.machine().lower() not in ("x86_64", "amd64"):
        raise RuntimeError("not native x86-64: no execution claim")
    lib = ctypes.CDLL(str((out / "bridge.so").resolve()))
    fn = lib.native_udivrem
    fn.argtypes = (ctypes.c_uint64, ctypes.c_uint64,
                   ctypes.POINTER(ctypes.c_uint64))
    fn.restype = ctypes.c_uint64
    checks = 0
    edges = 0
    for x, y in vectors():
        rem = ctypes.c_uint64(0xDEADBEEFDEADBEEF)
        q = int(fn(x, y, ctypes.byref(rem)))
        expected = divmod(x, y)
        if (q, int(rem.value)) != expected:
            raise AssertionError(("machine DIV differs from independent integer oracle", x, y, q, int(rem.value), expected))
        checks += 1
        edges += x in (0, MASK) or y in (1, MASK)
    def disable_cores():
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    trap = subprocess.run([str((out / "zero-divisor").resolve())],
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                          timeout=5, preexec_fn=disable_cores)
    if trap.returncode != -signal.SIGFPE:
        raise AssertionError("expected real x86 divide error SIGFPE for zero divisor: "
                             + str(trap.returncode))
    # A nonzero high half is deliberately OUT OF DOMAIN. Demonstrate why.
    bad_hi, bad_x, bad_y = 1, MASK, 1
    assert ((bad_hi << 64) | bad_x) // bad_y > MASK
    return {"classification": "finite native x86-64 hardware execution",
            "nonzero_divisor_pairs": checks, "boundary_pairs": edges,
            "zero_divisor": "SIGFPE from standalone DIV",
            "outside_domain_counterexample": {
                "high_dividend": bad_hi, "low_dividend": bad_x, "divisor": bad_y,
                "reason": "128/64 quotient overflow; zero-high premise is necessary"},
            "undefined_flags": "not read or compared", "register_abi": "standalone GCC extended asm, NOT actual Cranelift register allocation"}

def smt_check(timeout_ms: int) -> dict:
    import z3
    x, y = z3.BitVecs("dividend divisor", 64)
    big = z3.ZeroExt(64, x)
    wide_y = z3.ZeroExt(64, y)
    qwide, rwide = z3.UDiv(big, wide_y), z3.URem(big, wide_y)
    q64, r64 = z3.UDiv(x, y), z3.URem(x, y)
    s = z3.Solver()
    s.set(timeout=timeout_ms)
    s.add(y != 0, z3.Or(z3.UGT(qwide, z3.BitVecVal(MASK, 128)),
                          z3.Extract(63, 0, qwide) != q64,
                          z3.Extract(63, 0, rwide) != r64))
    result = str(s.check())
    receipt = {"classification": "64/128-bit QF_BV SMT query under zero-high dividend and nonzero divisor",
               "solver": z3.get_full_version(), "timeout_ms": timeout_ms,
               "mismatch_query_result": result,
               "all_input_proof": result == "unsat"}
    if result == "unknown":
        receipt["reason"] = s.reason_unknown()
    if result == "sat":
        receipt["counterexample"] = str(s.model())
        raise AssertionError("found a machine DIV arithmetic mismatch")
    return receipt

def checked_payload(out: Path, upstream: Path) -> dict:
    return {"schema": SCHEMA, "upstream": source_guard(upstream),
            "source_sha256": digest(out / "div-bridge.c"),
            "native_binary_sha256": digest(out / "bridge.so"),
            "trap_binary_sha256": digest(out / "zero-divisor"),
            "compiler": cmd("cc", "--version").splitlines()[0],
            "architecture": platform.machine(),
            "native": native_check(out), "smt": smt_check(10000),
            "semantics": {
                "signedness": "unsigned 64-bit x,y",
                "dividend": "RDX:RAX = 0:x (zero-high precondition)",
                "precondition": "y != 0 for defined quotient/remainder; y=0 raises #DE/SIGFPE",
                "results": "RAX = floor(x/y), RDX = x mod y",
                "quotient_overflow": "impossible when high=0, x<=2^64-1, and y>=1",
                "flags": "x86 DIV CF/PF/AF/ZF/SF/OF undefined; no flag equivalence claimed",
                "poison": "no LLVM/CLIF poison modeled; exact scalar unsigned semantics only",
                "scope": "standalone ISA arithmetic bridge; NOT compiled Wasmtime pass, machine-state, ABI, stack map, or guest trap-offset proof"},
            "related_previous_cost": {
                "qualification": "https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/44449965859",
                "measurement": "43 to 25 decoded x86 function instruction bytes, earlier independently qualified only",
                "new_cost": "not measured in this proof experiment"}}

def main():
    p = argparse.ArgumentParser()
    p.add_argument("--mode", choices=("self-test","run","audit"), required=True)
    p.add_argument("--upstream", type=Path)
    p.add_argument("--out", type=Path)
    a = p.parse_args()
    if a.mode == "self-test":
        assert len(tuple(vectors())) >= 65280
        assert (MASK // MASK, MASK % MASK) == (1, 0)
        assert ((1 << 64) // 1) > MASK
        print("x86 unsigned DIV finite domain and invalid high-half control passed")
        return
    if a.upstream is None or a.out is None:
        p.error("--upstream and --out required")
    if a.mode == "run":
        if a.out.exists():
            raise ValueError("never overwrite a prior receipt")
        compile_native(a.out)
        receipt = checked_payload(a.out, a.upstream)
        (a.out / "receipt.json").write_text(json.dumps(receipt, indent=2, sort_keys=True)+"\n")
    else:
        old = json.loads((a.out / "receipt.json").read_text())
        now = checked_payload(a.out, a.upstream)
        if old != now:
            raise AssertionError("source hashes/native results/SMT evidence do not replay")
    print(json.dumps({"mode": a.mode, "native_pairs": receipt["native"]["nonzero_divisor_pairs"] if a.mode == "run" else now["native"]["nonzero_divisor_pairs"],
                      "smt": receipt["smt"]["mismatch_query_result"] if a.mode == "run" else now["smt"]["mismatch_query_result"]}, sort_keys=True))

if __name__ == "__main__":
    main()
