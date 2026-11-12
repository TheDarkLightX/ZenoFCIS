#!/usr/bin/env python3
"""Same-WAT native SSA-use/trap screen for the pinned *reconstruction* compiler.

Tests quotient, intermediate multiply, and remainder all being live, plus
out-of-scope multiplier and a stateful intervening operation. The compiler
pair is obtained from the already-qualified, source-pinned reconstruction
producer; neither Wasmtime source nor protected repository branches are edited.
Finite native testing; NOT a compiler-pass / register-allocation proof.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import platform
import random
import re
import subprocess
from pathlib import Path

from qualify_rust_divrem_28 import source_archive, signed, TARGET, UPSTREAM

MASK = (1 << 64) - 1
CASES = ("live_q_m_r", "reversed_mul", "wrong_multiplier",
         "stateful_between", "double_reconstruction")
OUT_OF_SCOPE = {"wrong_multiplier", "stateful_between"}
SOURCE_RUN = 42951667233
SCHEMA = "zenofcis/tier1-reconstruction-ssa-contexts/1"


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def rot(v: int, n: int) -> int:
    return ((v << n) | (v >> (64 - n))) & MASK


def oracle(case: str, x: int, y: int) -> int | None:
    if case not in CASES:
        raise ValueError("unknown contract")
    a, b = x & MASK, y & MASK
    if b == 0:
        return None  # exactly one division-by-zero trap, no numeric result
    q = a // b
    multiplier = a if case == "wrong_multiplier" else b
    m = (q * multiplier) & MASK
    r = (a - m) & MASK
    value = q ^ rot(m, 7) ^ rot(r, 17)
    if case == "stateful_between":
        value ^= rot(q, 31)  # observed mutable global written after division
    if case == "double_reconstruction":
        a2 = (a + 17) & MASK
        q2, r2 = divmod(a2, b)
        m2 = q2 * b
        value ^= rot(q2, 19) ^ rot(m2, 23) ^ rot(r2, 29)
    return signed(value & MASK)


def wat(case: str) -> str:
    if case not in CASES:
        raise ValueError("unknown case")
    global_def = ('  (global $marker (export "marker") (mut i64) (i64.const 0))\n'
                  if case == "stateful_between" else "")
    decl = ['    (local $q i64)', '    (local $m i64)', '    (local $r i64)']
    if case == "double_reconstruction":
        decl += ['    (local $x2 i64)', '    (local $q2 i64)',
                 '    (local $m2 i64)', '    (local $r2 i64)']
    body = [
        '    local.get $x', '    local.get $y', '    i64.div_u',
        '    local.set $q',
    ]
    if case == "stateful_between":
        body += ['    local.get $q', '    global.set $marker']
    if case == "reversed_mul":
        body += ['    local.get $y', '    local.get $q']
    elif case == "wrong_multiplier":
        body += ['    local.get $q', '    local.get $x']
    else:
        body += ['    local.get $q', '    local.get $y']
    body += ['    i64.mul', '    local.set $m',
             '    local.get $x', '    local.get $m', '    i64.sub',
             '    local.set $r',
             '    local.get $q',
             '    local.get $m', '    i64.const 7', '    i64.rotl', '    i64.xor',
             '    local.get $r', '    i64.const 17', '    i64.rotl', '    i64.xor']
    if case == "stateful_between":
        body += ['    global.get $marker', '    i64.const 31', '    i64.rotl',
                 '    i64.xor']
    if case == "double_reconstruction":
        # The second reconstruction uses a distinct x value and the same
        # divisor, while keeping both quotient and product live.
        body += ['    local.get $x', '    i64.const 17', '    i64.add',
                 '    local.set $x2',
                 '    local.get $x2', '    local.get $y', '    i64.div_u',
                 '    local.set $q2',
                 '    local.get $q2', '    local.get $y', '    i64.mul',
                 '    local.set $m2',
                 '    local.get $x2', '    local.get $m2', '    i64.sub',
                 '    local.set $r2',
                 '    local.get $q2', '    i64.const 19', '    i64.rotl',
                 '    i64.xor',
                 '    local.get $m2', '    i64.const 23', '    i64.rotl',
                 '    i64.xor',
                 '    local.get $r2', '    i64.const 29', '    i64.rotl',
                 '    i64.xor']
    return ('(module\n' + global_def +
            '  (func (export "work") (param $x i64) (param $y i64) (result i64)\n' +
            '\n'.join(decl + body) + '\n  )\n)\n')


def vectors() -> tuple[tuple[int, int], ...]:
    edges = (0, 1, 2, 3, 7, 17, (1 << 32) - 1, (1 << 63) - 1,
             -(1 << 63), -1)
    divisors = (0, 1, 2, 3, -1, -(1 << 63))
    pairs = [(x, y) for x in edges for y in divisors]
    rng = random.Random(0x56232026)
    pairs += [(signed(rng.getrandbits(64)), signed(rng.getrandbits(64)))
              for _ in range(16)]
    return tuple(dict.fromkeys(pairs))


def cmd(args: list[str], timeout=240) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, text=True, capture_output=True, timeout=timeout)


def native_cost(disasm: str) -> dict:
    # Count just the exported guest's decoded instructions, not the runtime,
    # AOT metadata, literal data, or any other function.
    sections = re.split(r"(?m)^wasm\[\d+\]::function\[\d+\]:\s*$", disasm)
    headings = re.findall(r"(?m)^wasm\[(\d+)\]::function\[(\d+)\]:\s*$", disasm)
    indices = [i for i, pair in enumerate(headings) if pair == ("0", "0")]
    if len(indices) != 1:
        raise ValueError("missing or ambiguous wasm[0]::function[0] native code")
    block = sections[indices[0] + 1]
    rows = [line for line in block.splitlines()
            if re.match(r"^\s{12}[0-9a-f]{2}(?:\s|$)", line)]
    if not rows:
        raise ValueError("no guest machine instructions")
    lengths = []
    for row in rows:
        count = 0
        for token in row.strip().split():
            if re.fullmatch(r"[0-9a-f]{2}", token):
                count += 1
            else:
                break
        if count == 0:
            raise ValueError("unparsed machine instruction")
        lengths.append(count)
    return {
        "decoded_guest_instruction_bytes": sum(lengths),
        "decoded_guest_instruction_count": len(rows),
        "static_divq": sum(bool(re.search(r"\bdivq\b", line)) for line in rows),
    }


def run(archive: Path, out: Path) -> dict:
    if out.exists():
        raise FileExistsError("refusing to overwrite existing evidence")
    if platform.machine() not in ("x86_64", "AMD64"):
        raise ValueError("requires native x86-64")
    producer = source_archive(archive)  # exact pinned compiler SHA-256s
    out.mkdir(parents=True)
    report = {
        "schema": SCHEMA, "status": "finite-native-SSA-context-screen",
        "source_producer_run": SOURCE_RUN, "wasmtime_source_commit": UPSTREAM,
        "target": TARGET, "source_script_sha256": sha(Path(__file__)),
        "producer_receipt_sha256": sha(archive / "qualification.json"),
        "compiler_sha256": {k: producer["results"][k]["compiler_sha256"]
                            for k in ("baseline", "patched")},
        "cases": {}, "limitations": [
            "author-designed WAT, not a general corpus or newly ZenoFCIS-synthesized candidate",
            "finite native Wasm value/trap tests, not whole-pass or machine-ISA verification",
            "negative controls are conservative; CLIF egraph may transform a WAT source idiom",
            "guest decoded x86 bytes are neither CLIF cost nor whole-AOT size or throughput",
            "Wasm has specified integer arithmetic and divide-by-zero trap; no poison or flag observables modeled",
            "does not establish register allocation, debug-location, trap-position, or cross-target correctness",
        ],
    }
    for name in CASES:
        folder = out / name
        folder.mkdir()
        w = folder / "input.wat"
        w.write_text(wat(name))
        row = {"wat_sha256": sha(w), "contract": name, "results": {}}
        for mode in ("baseline", "patched"):
            exe = archive / mode / "wasmtime"
            target = folder / mode
            target.mkdir()
            aot = target / "work.cwasm"
            clif = target / "clif"
            clif.mkdir()
            p = cmd([str(exe), "compile", "--target", TARGET,
                     "--emit-clif", str(clif), "-o", str(aot), str(w)])
            if p.returncode:
                raise RuntimeError(f"{name}/{mode} compile failed: {p.stderr[:1200]}")
            p = cmd([str(exe), "objdump", "--funcs", "wasm", "--bytes", str(aot)])
            if p.returncode:
                raise RuntimeError(f"{name}/{mode} disassembly failed: {p.stderr[:1200]}")
            dis = target / "native.txt"
            dis.write_text(p.stdout)
            tally = {"values": 0, "zero_divisor_traps": 0}
            for x, y in vectors():
                p = cmd([str(exe), "run", "--allow-precompiled", "--invoke",
                         "work", str(aot), str(x), str(y)], timeout=40)
                want = oracle(name, x, y)
                if want is None:
                    if p.returncode == 0 or ("trap" not in p.stderr.lower()
                                            and "divide by zero" not in p.stderr.lower()):
                        raise AssertionError(f"{name}/{mode}: no division trap at {x},{y}: {p.stderr[:200]}")
                    tally["zero_divisor_traps"] += 1
                elif p.returncode or p.stdout.strip() != str(want):
                    raise AssertionError(f"{name}/{mode} at {x},{y}: {p.stdout!r} {p.stderr[:200]!r}; expected {want}")
                else:
                    tally["values"] += 1
            clif_files = sorted(f for f in clif.rglob("*") if f.is_file())
            if not clif_files:
                raise ValueError("missing compiler-produced CLIF")
            row["results"][mode] = {
                "compiler_sha256": sha(exe), "aot_sha256": sha(aot),
                "whole_aot_bytes": aot.stat().st_size,
                "native_disassembly_sha256": sha(dis), "native_cost": native_cost(p.stdout if False else dis.read_text()),
                "clif_hashes": {f.relative_to(clif).as_posix(): sha(f) for f in clif_files},
                "finite": tally,
            }
        b, p = row["results"]["baseline"], row["results"]["patched"]
        row["baseline_minus_patched"] = {
            "decoded_guest_instruction_bytes": b["native_cost"]["decoded_guest_instruction_bytes"] - p["native_cost"]["decoded_guest_instruction_bytes"],
            "decoded_guest_instruction_count": b["native_cost"]["decoded_guest_instruction_count"] - p["native_cost"]["decoded_guest_instruction_count"],
            "static_divq": b["native_cost"]["static_divq"] - p["native_cost"]["static_divq"],
            "whole_aot_bytes": b["whole_aot_bytes"] - p["whole_aot_bytes"],
        }
        row["aot_byte_identical"] = b["aot_sha256"] == p["aot_sha256"]
        if name in OUT_OF_SCOPE and not row["aot_byte_identical"]:
            raise AssertionError(f"{name}: unanticipated change in a declared out-of-scope fixture")
        report["cases"][name] = row
    report["total_finite_values"] = sum(r["finite"]["values"] for row in report["cases"].values()
                                        for r in row["results"].values())
    report["total_zero_divisor_traps"] = sum(r["finite"]["zero_divisor_traps"] for row in report["cases"].values()
                                            for r in row["results"].values())
    (out / "qualification.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return report


def self_test() -> None:
    assert set(CASES) == {"live_q_m_r", "reversed_mul", "wrong_multiplier",
                          "stateful_between", "double_reconstruction"}
    assert len(vectors()) >= 70
    for case in CASES:
        source = wat(case)
        assert source.count("i64.div_u") == (2 if case == "double_reconstruction" else 1)
        assert source.count("i64.mul") == (2 if case == "double_reconstruction" else 1)
        assert oracle(case, 7, 0) is None
        for x, y in vectors():
            if y != 0:
                assert oracle(case, x, y) is not None
    assert oracle("live_q_m_r", 7, 3) != oracle("wrong_multiplier", 7, 3)
    assert oracle("live_q_m_r", 7, 3) == oracle("reversed_mul", 7, 3)
    # Verify cost parser with one synthetic target symbol.
    text = "wasm[0]::function[0]:\n            48    decq %rax\n            48 89    movq %rax, %rcx\n"
    assert native_cost(text)["decoded_guest_instruction_bytes"] == 3
    for bad in ("", "wasm[0]::function[1]:\n" + text.split("\n", 1)[1]):
        try:
            native_cost(bad)
        except ValueError:
            pass
        else:
            raise AssertionError("failed closed on missing guest symbol")
    print("SSA context WAT, all contracts, zero-divisor and cost parser self-tests passed")


if __name__ == "__main__":
    a = argparse.ArgumentParser()
    a.add_argument("--self-test", action="store_true")
    a.add_argument("--archive", type=Path)
    a.add_argument("--out", type=Path)
    args = a.parse_args()
    if args.self_test:
        self_test()
    else:
        if args.archive is None or args.out is None:
            a.error("requires --archive and --out")
        receipt = run(args.archive.resolve(), args.out.resolve())
        print(json.dumps({"status": receipt["status"],
                          "cases": {k: v["baseline_minus_patched"] for k,v in receipt["cases"].items()},
                          "finite_values": receipt["total_finite_values"],
                          "zero_divisor_traps": receipt["total_zero_divisor_traps"]},
                         sort_keys=True))
