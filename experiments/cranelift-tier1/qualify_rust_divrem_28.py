#!/usr/bin/env python3
"""Source-pinned Rust-generated Wasm reachability and real Cranelift A/B.

The two Wasmtime compiler binaries are previously qualified archived artifacts.
Each Rust opt profile produces a separate Wasm file; within a profile *exactly
the same file* is compiled by baseline and patched Cranelift. Positive saving
and optimizer-rule matching are NOT assumed.
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

HERE = Path(__file__).resolve().parent
SOURCE = HERE / "idiom_divrem_28.rs"
UPSTREAM = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
TARGET = "x86_64-unknown-linux"
WASM_TARGET = "wasm32-unknown-unknown"
MASK = (1 << 64) - 1
MIN = -(1 << 63)
EXPORTS = ("divrem_pair", "divrem_offset", "divrem_reverse",
           "divrem_swapped", "div_only", "signed_divrem")
PROFILES = {"debug": "0", "optimized": "3"}
SCHEMA = "zenofcis/tier1-rust-divrem-generated-wasm/1"

def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()

def signed(v: int) -> int:
    v &= MASK
    return v if v < (1 << 63) else v - (1 << 64)

def oracle(fn: str, x: int, y: int) -> int | None:
    if fn not in EXPORTS or not MIN <= x <= (1 << 63)-1 or not MIN <= y <= (1 << 63)-1:
        raise ValueError("bad function or CLI signed i64 input")
    a, b = x & MASK, y & MASK
    if y == 0 or (fn == "divrem_swapped" and x == 0):
        return None
    if fn == "signed_divrem":
        if x == MIN and y == -1:
            return None
        q = (abs(x) // abs(y)) * (-1 if ((x < 0) != (y < 0)) else 1)
        r = x - q * y
    elif fn == "divrem_swapped":
        q, r = a // b, b % a
    else:
        q, r = divmod(a, b)
    if fn == "div_only":
        return signed(q)
    if fn == "divrem_offset":
        q = (q + 3) & MASK
    r &= MASK
    return signed(q ^ (((r << 17) | (r >> 47)) & MASK))

def vectors() -> tuple[tuple[int, int], ...]:
    edge = (MIN, MIN+1, -7, -3, -1, 0, 1, 2, 3, 7, 1<<32, (1<<63)-1)
    pairs = [(x, y) for x in edge for y in (-7,-1,0,1,2,3,7)]
    rng = random.Random(0x56232026)
    pairs += [(signed(rng.getrandbits(64)), signed(rng.getrandbits(64)))
              for _ in range(24)]
    return tuple(dict.fromkeys(pairs))

def command(argv, timeout=240):
    p = subprocess.run([str(v) for v in argv], capture_output=True,
                       text=True, timeout=timeout)
    if p.returncode:
        raise RuntimeError("failed: " + " ".join(map(str, argv)) +
                           "\n" + p.stderr[:1500])
    return p.stdout

def source_archive(archive: Path) -> dict:
    record = json.loads((archive / "qualification.json").read_text())
    if record.get("upstream") != UPSTREAM or record.get("target") != TARGET:
        raise ValueError("wrong pinned source/target in compiler artifact")
    if set(record.get("results", {})) != {"baseline", "patched"}:
        raise ValueError("missing compiler pair")
    for name in ("baseline", "patched"):
        binary = archive / name / "wasmtime"
        if not binary.is_file() or sha(binary) != record["results"][name]["compiler_sha256"]:
            raise ValueError("archived compiler hash mismatch: " + name)
    return record

def execute(exe: Path, aot: Path, fn: str, x: int, y: int) -> str:
    p = subprocess.run([str(exe), "run", "--allow-precompiled",
                        "--invoke", fn, str(aot), str(x), str(y)],
                       capture_output=True, text=True, timeout=45)
    expected = oracle(fn, x, y)
    if expected is None:
        if p.returncode == 0:
            raise AssertionError(f"missing Rust panic or Wasm trap: {fn}({x},{y})")
        return "trap_or_panic"
    if p.returncode or p.stdout.strip() != str(expected):
        raise AssertionError(f"bad {fn}({x},{y}): {p.stdout!r} / {p.stderr[:160]!r}; wanted {expected}")
    return "value"

def text_size(aot: Path) -> int:
    result = command(["readelf", "-SW", aot])
    hits = re.findall(r"(?m)^\s*\[\s*\d+\]\s+\.text\s+\S+\s+\S+\s+\S+\s+([0-9a-fA-F]+)\s+", result)
    if len(hits) != 1 or int(hits[0],16) == 0:
        raise ValueError("missing/ambiguous ELF executable .text")
    return int(hits[0],16)

def qualify(archive: Path, out: Path) -> dict:
    if out.exists():
        raise FileExistsError("refuse evidence overwrite")
    if platform.machine() not in ("x86_64","AMD64"):
        raise ValueError("requires x86_64 native test host")
    rustc = command(["rustc","--version"]).strip()
    if not rustc.startswith("rustc 1.97.1 "):
        raise ValueError("wrong Rust compiler revision: " + rustc)
    source = source_archive(archive)
    out.mkdir(parents=True)
    receipt = {
        "schema": SCHEMA,
        "status": "finite-checked-generated-wasm-target-audit",
        "source_pin": UPSTREAM,
        "rustc": rustc,
        "rust_target": WASM_TARGET, "native_target": TARGET,
        "rust_source_sha256": sha(SOURCE), "producer_sha256": sha(Path(__file__)),
        "archive_receipt_sha256": sha(archive/"qualification.json"),
        "compiler_sha256": {k:source["results"][k]["compiler_sha256"] for k in ("baseline","patched")},
        "profiles": {},
        "limitations": [
            "experiment-authored Rust, not evidence of real-application prevalence",
            "debug and optimized Rust profiles are distinct Wasm programs",
            "finite native execution is not compiler-pass/ISA formal proof",
            "no unconditional named VeriISLE proof or flag/poison/ABI proof",
            "module-wide divq counts are not per-function ISLE hits",
            "ELF .text and serialized AOT bytes are not native CLIF operation costs",
            "no throughput or cross-target performance claim",
            "zero divisor and signed overflow may trigger Rust panic, not an x86 fault"
        ]
    }
    for profile, opt in PROFILES.items():
        folder = out/profile
        folder.mkdir()
        wasm = folder/"idiom.wasm"
        args = ["rustc", "--edition=2021", "--crate-name=tier1_divrem_idiom",
                "--crate-type=cdylib", "--target", WASM_TARGET,
                "-C", f"opt-level={opt}", "-C", "panic=abort",
                "-C", "debuginfo=0", "-o", wasm, SOURCE]
        command(args)
        if not wasm.is_file() or wasm.stat().st_size == 0:
            raise ValueError("missing Rust-generated Wasm")
        record = {"rust_opt":opt, "wasm_sha256":sha(wasm),
                  "wasm_bytes":wasm.stat().st_size, "rust_argv":[str(v) for v in args],
                  "results":{}}
        for name in ("baseline","patched"):
            compiler = archive/name/"wasmtime"
            dest = folder/name
            dest.mkdir()
            aot = dest/"native.cwasm"
            clif = dest/"clif"
            clif.mkdir()
            command([compiler,"compile","--target",TARGET,
                     "--emit-clif",clif,"-o",aot,wasm], timeout=360)
            dis = command([compiler,"objdump","--funcs","wasm","--bytes",aot])
            (dest/"disassembly.txt").write_text(dis)
            files = sorted(p for p in clif.rglob("*") if p.is_file())
            if not files:
                raise ValueError("no emitted CLIF")
            finite = {"value":0, "trap_or_panic":0}
            for fn in EXPORTS:
                for x, y in vectors():
                    finite[execute(compiler,aot,fn,x,y)] += 1
            record["results"][name] = {
                "compiler_sha256":sha(compiler),
                "aot_sha256":sha(aot), "whole_aot_bytes":aot.stat().st_size,
                "elf_text_bytes":text_size(aot),
                "disassembly_sha256":sha(dest/"disassembly.txt"),
                "module_static_divq":len(re.findall(r"\bdivq\b",dis)),
                "clif":[{"path":p.relative_to(clif).as_posix(),
                         "sha256":sha(p), "bytes":p.stat().st_size} for p in files],
                "finite":finite
            }
        b,p=record["results"]["baseline"],record["results"]["patched"]
        record["baseline_minus_patched"]={
            "elf_text_bytes":b["elf_text_bytes"]-p["elf_text_bytes"],
            "whole_aot_bytes":b["whole_aot_bytes"]-p["whole_aot_bytes"],
            "module_static_divq":b["module_static_divq"]-p["module_static_divq"]}
        record["aot_byte_identical"]=b["aot_sha256"]==p["aot_sha256"]
        receipt["profiles"][profile]=record
    (out/"qualification.json").write_text(json.dumps(receipt,indent=2,sort_keys=True)+"\n")
    return receipt

def self_test():
    assert oracle("divrem_pair",7,3)==131074
    assert oracle("divrem_reverse",7,3)==131074
    assert oracle("divrem_offset",7,3)==131077
    assert oracle("divrem_swapped",7,3)==393218
    assert oracle("signed_divrem",-7,3)==1
    assert oracle("div_only",-1,2)==(1<<63)-1
    assert oracle("divrem_pair",7,0) is None
    assert oracle("divrem_swapped",0,7) is None
    assert oracle("signed_divrem",MIN,-1) is None
    assert len(vectors())>=80
    print("Rust divrem arithmetic and negative-control self-test passed")

if __name__=="__main__":
    p=argparse.ArgumentParser()
    p.add_argument("--archive",type=Path)
    p.add_argument("--out",type=Path)
    p.add_argument("--self-test",action="store_true")
    a=p.parse_args()
    if a.self_test:self_test()
    else:
        if a.archive is None or a.out is None:p.error("archive/out required")
        rec=qualify(a.archive.resolve(),a.out.resolve())
        print(json.dumps({k:v["baseline_minus_patched"] for k,v in rec["profiles"].items()},sort_keys=True))
