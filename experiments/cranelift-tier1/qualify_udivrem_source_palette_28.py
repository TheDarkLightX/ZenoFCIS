#!/usr/bin/env python3
"""One-compiler, two-source, bounded u64 quotient/remainder formulation search.

Experimental discovery for upstream Cranelift issue #5623, *not* a compiler
optimization patch. Two semantically equal WAT programs, deliberately DIFFERENT
source bytes, are compiled with the SAME pinned upstream Wasmtime binary.
Machine-code and complete AOT costs are directly measured, never extrapolated.
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

import qualify_wasmtime_frontend_28 as frontend
from install_isle_candidate import UPSTREAM

SCHEMA = "zeno-fcis/tier1-udiv-urem-source-palette/1"
TARGET = "x86_64-unknown-linux"
MASK = (1 << 64) - 1
ROT = 17
HERE = Path(__file__).resolve().parent
MODES = ("direct", "quotient_reuse")
WATS = {
    "direct": """(module
  (func (export "work") (param $x i64) (param $y i64) (result i64)
    local.get $x
    local.get $y
    i64.div_u
    local.get $x
    local.get $y
    i64.rem_u
    i64.const 17
    i64.rotl
    i64.xor
  )
)
""",
    "quotient_reuse": """(module
  (func (export "work") (param $x i64) (param $y i64) (result i64)
    (local $q i64)
    local.get $x
    local.get $y
    i64.div_u
    local.tee $q
    local.get $x
    local.get $q
    local.get $y
    i64.mul
    i64.sub
    i64.const 17
    i64.rotl
    i64.xor
  )
)
""",
}
ASM_LINE = re.compile(r"^ {12}([0-9a-f]{2}(?: [0-9a-f]{2})*)(?: {2,}(.*)|$)")
HEADERS = ("wasm[0]::function[0]:", "wasm[0]::function[0]::work:")
VECTORS = tuple((x, y) for x, y in frontend.VECTORS if y != 0)


def sha(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def s64(bits: int) -> int:
    v = bits & MASK
    return v if v < (1 << 63) else v - (1 << 64)


def reference(x: int, y: int) -> int:
    """Independent unsigned-64 Euclidean quotient/remainder, signed result."""
    if not (frontend.MIN <= x <= frontend.MAX and frontend.MIN <= y <= frontend.MAX):
        raise ValueError("outside admitted i64 bit-pattern domain")
    a, b = x & MASK, y & MASK
    if b == 0:
        raise ZeroDivisionError("Wasm i64.div_u/i64.rem_u must trap")
    q, r = divmod(a, b)
    rotated = ((r << ROT) | (r >> (64 - ROT))) & MASK
    return s64(q ^ rotated)


def vectors() -> tuple[tuple[int, int], ...]:
    rng = random.Random(0x56232026)
    seeded = [(s64(rng.getrandbits(64)), s64(rng.getrandbits(64)) or 1)
              for _ in range(40)]
    return tuple(dict.fromkeys((*VECTORS, *seeded)))


def disasm_cost(source: str) -> dict:
    """Static machine-code costs, NOT a dynamically executed instruction trace."""
    rows = source.splitlines()
    headers = [i for i, row in enumerate(rows) if row.startswith("wasm[0]::function[")]
    if len(headers) != 1 or rows[headers[0]] not in HEADERS:
        raise ValueError("missing or surplus exact Wasm function")
    region, thru_ret, instructions, ndiv, ret_count = 0, None, 0, 0, 0
    for line in rows[headers[0] + 1:]:
        if not line.strip() or line.strip() == "╰─╼ trap: Normal(IntegerDivisionByZero)":
            continue
        match = ASM_LINE.fullmatch(line)
        if not match:
            raise ValueError("unrecognized emitted instruction/byte line")
        data = match.group(1).split(" ")
        asm = (match.group(2) or "").strip()
        region += len(data)
        instructions += 1
        if re.match(r"^(?:div|idiv)[bwlq]?(?:\s|$)", asm):
            ndiv += 1
        if re.match(r"^retq(?:\s|$)", asm):
            ret_count += 1
            thru_ret = region
    if not thru_ret or ret_count != 1:
        raise ValueError("no unique return or empty emitted code")
    return {
        "region_bytes": region, "through_ret_bytes": thru_ret,
        "decoded_instruction_lines": instructions, "static_div_instructions": ndiv,
        "not_dynamic": True,
    }


def finite(exe: Path, obj: Path) -> dict:
    checked = 0
    for x, y in vectors():
        actual = frontend.invoke(exe, obj, "work", x, y)
        if actual != reference(x, y):
            raise ValueError(f"wrong unsigned quotient/remainder result: {x},{y}")
        checked += 1
    # Domain boundary: division by zero must trap on both formulations.
    command = [str(exe), "run", "--allow-precompiled", "--invoke", "work",
               str(obj), "1", "0"]
    result = subprocess.run(command, capture_output=True, text=True,
                            timeout=40, check=False)
    if result.returncode == 0 or "trap" not in result.stderr.lower():
        raise ValueError("missing Wasm zero-divisor trap")
    return {"passed": True, "nonzero_divisor_pairs": checked,
            "zero_divisor_trap_checked": True,
            "classification": "finite-runtime-tests-not-exhaustive-i64-proof"}


def source_pin(upstream: Path, exe: Path) -> dict:
    head = subprocess.check_output(
        ["git", "-C", str(upstream), "rev-parse", "HEAD"], text=True).strip()
    if head != UPSTREAM:
        raise ValueError("upstream pin moved")
    if subprocess.check_output(
        ["git", "-C", str(upstream), "status", "--porcelain"], text=True).strip():
        raise ValueError("unmodified upstream checkout required")
    if not exe.is_file():
        raise ValueError("missing executable")
    return {"upstream_commit": head, "binary_sha256": sha(exe.read_bytes()),
            "binary_size": exe.stat().st_size,
            "binary_version": subprocess.check_output(
                [str(exe), "--version"], text=True).strip(),
            "producer_sha256": sha(Path(__file__).read_bytes()),
            "host_machine": platform.machine()}


def qualify(upstream: Path, exe: Path, out: Path) -> dict:
    if out.exists():
        raise ValueError("refusing to overwrite evidence")
    if platform.machine() not in ("x86_64", "AMD64"):
        raise ValueError("this native experiment uses the x86_64 baseline target")
    pin = source_pin(upstream, exe)
    out.mkdir(parents=True)
    results, catalog = {}, {}
    for mode in MODES:
        folder = out / mode
        folder.mkdir()
        source = folder / "source.wat"
        source.write_text(WATS[mode])
        binary = folder / "functions.cwasm"
        compile_args = [str(exe), "compile", "--target", TARGET,
                        "-o", str(binary), str(source)]
        result = frontend.run(compile_args, timeout=300)
        (folder / "compile.stdout").write_text(result.stdout)
        (folder / "compile.stderr").write_text(result.stderr)
        disasm = frontend.run([str(exe), "objdump", "--funcs", "wasm",
                               "--bytes", str(binary)]).stdout
        (folder / "disassembly.txt").write_text(disasm)
        actual = binary.read_bytes()
        if not actual:
            raise ValueError("empty AOT")
        measured = {
            "wat_sha256": sha(source.read_bytes()),
            "binary_sha256": sha(actual), "whole_cwasm_bytes": len(actual),
            "elf_text_bytes": frontend.text_size(binary),
            "disassembly_sha256": sha(disasm.encode()),
            "instruction_cost": disasm_cost(disasm),
            "finite": finite(exe, binary),
            "compile_command": compile_args,
        }
        results[mode] = measured
        key = sha(json.dumps({
            "contract": "unsigned i64 quotient XOR rotl(unsigned i64 remainder,17), nonzero divisor; zero divisor trap",
            "target": TARGET, "compiler_sha256": pin["binary_sha256"],
            "native_sha256": measured["binary_sha256"],
        }, sort_keys=True).encode())
        if key not in catalog:
            catalog[key] = {"content_key": key, "modes": [],
                            "native_sha256": measured["binary_sha256"],
                            "whole_aot_bytes": measured["whole_cwasm_bytes"],
                            "elf_text_bytes": measured["elf_text_bytes"],
                            "instruction_cost": measured["instruction_cost"],
                            "equivalence": "conditional Euclidean law + finite native oracle; not whole-compiler proof"}
        catalog[key]["modes"].append(mode)
    b, p = [results[m] for m in MODES]
    result = {
        "schema": SCHEMA, "status": "native-source-formulation-comparison-finite-checked",
        "pins": pin, "target": TARGET,
        "candidate_vocabulary": list(MODES), "results": results,
        "baseline_minus_candidate": {
            "whole_cwasm_bytes": b["whole_cwasm_bytes"] - p["whole_cwasm_bytes"],
            "elf_text_bytes": b["elf_text_bytes"] - p["elf_text_bytes"],
            "first_ret_bytes": b["instruction_cost"]["through_ret_bytes"] - p["instruction_cost"]["through_ret_bytes"],
            "static_div_instructions": b["instruction_cost"]["static_div_instructions"] - p["instruction_cost"]["static_div_instructions"],
        },
        "catalog": sorted(catalog.values(), key=lambda entry: entry["content_key"]),
        "semantics": {
            "inputs": "two arbitrary 64-bit words supplied via signed i64 CLI arguments",
            "signedness": "unsigned i64 quotient and remainder; signed i64 output only as bit-pattern printing",
            "overflow": "multiplication and subtraction are modular i64; quotient*divisor <= dividend for nonzero divisor",
            "zero_divisor": "both Wasm implementations trap; excluded from value function domain",
            "flags_poison_UB": "no exposed host ISA flags or UB in pure admitted Wasm arithmetic",
            "target": TARGET, "scope": "this compiler revision, target and fixture, NOT a general ISLE rewrite",
        },
        "nonclaims": [
            "comparison is TWO DIFFERENT semantically equivalent WAT programs, NOT same-Wasm patched compiler",
            "static div count, bytes, AOT size are not a runtime speed or throughput measurement",
            "finite native execution is not an exhaustive 64-bit equivalence proof",
            "algebraic all-width division identity assumes specified Euclidean unsigned div/rem semantics",
            "a source-level win does not establish an automatic compiler improvement",
            "not a global minimum over all machine-code instruction sequences",
        ],
    }
    (out / "qualification.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    return result


def main() -> None:
    p = argparse.ArgumentParser()
    for name in ("upstream", "bin", "out"):
        p.add_argument("--" + name, required=True, type=Path)
    a = p.parse_args()
    result = qualify(a.upstream.resolve(), a.bin.resolve(), a.out.resolve())
    print(json.dumps({"status": result["status"],
                      "measured_deltas": result["baseline_minus_candidate"]}, sort_keys=True))


if __name__ == "__main__":
    main()
