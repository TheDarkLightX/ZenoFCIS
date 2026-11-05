#!/usr/bin/env python3
"""Pinned Rust scalar idiom -> real Wasm -> same-source Cranelift compiler A/B.

Unlike the upstream external WAT corpus, this is ZenoFCIS-authored idiomatic
Rust source. It tests LLVM's lowering of real language constructs, not their
frequency in real applications. Each opt profile generates its own *fixed*
Wasm module; within a profile both Wasmtime compilers get identical bytes.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import platform
import subprocess
from pathlib import Path

from install_isle_candidate import UPSTREAM, RULE
import qualify_wasmtime_frontend_28 as frontend

HERE = Path(__file__).resolve().parent
SOURCE = HERE / "idiom_saturating_add_28.rs"
SCHEMA = "zeno-fcis/tier1-rust-idiom-wasm-aot/1"
PROFILES = {"debug": "0", "optimized": "3"}
TARGET = "x86_64-unknown-linux"
RUST_TARGET = "wasm32-unknown-unknown"
VECTORS = frontend.VECTORS
U64 = (1 << 64) - 1
MIN, MAX = frontend.MIN, frontend.MAX


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def signed(value: int) -> int:
    return ((value & U64) ^ (1 << 63)) - (1 << 63)


def value(function: str, x: int, y: int | None = None) -> int:
    if not MIN <= x <= MAX:
        raise ValueError("out-of-domain x")
    if function == "sign_clamp":
        return MIN if x < 0 else MAX
    if y is None or not MIN <= y <= MAX:
        raise ValueError("out-of-domain y")
    if function == "sat_add":
        return frontend.signed_sat(x, y)
    if function == "sat_add_xor":
        return signed(frontend.signed_sat(x, y) ^ 1)
    if function == "sat_add_mix":
        return signed(frontend.signed_sat(x, y) + ((x & U64) >> 7))
    if function == "wrap_add":
        return signed(x + y)
    raise ValueError("unrecognized function")


def compile_rust(folder: Path, optimization: str) -> dict:
    if optimization not in PROFILES.values():
        raise ValueError("not an admitted Rust optimization profile")
    obj = folder / "idiom.wasm"
    command = [
        "rustc", "--edition=2021", "--crate-type=cdylib",
        "--crate-name=tier1_rust_idiom",
        "--target", RUST_TARGET,
        "-C", f"opt-level={optimization}", "-C", "panic=abort",
        "-C", "debuginfo=0", "-o", str(obj), str(SOURCE),
    ]
    process = subprocess.run(
        command, text=True, capture_output=True, check=True, timeout=180)
    (folder / "rustc.stdout").write_text(process.stdout)
    (folder / "rustc.stderr").write_text(process.stderr)
    if not obj.is_file() or obj.stat().st_size == 0:
        raise ValueError("Rust did not emit source Wasm")
    return {"sha256": digest(obj.read_bytes()), "bytes": obj.stat().st_size,
            "argv": command}


def compile_aot(bin_path: Path, wasm: Path, folder: Path) -> dict:
    obj = folder / "idiom.cwasm"
    command = [
        str(bin_path), "compile", "--target", TARGET,
        "-o", str(obj), str(wasm),
    ]
    process = frontend.run(command, timeout=300)
    (folder / "compile.stdout").write_text(process.stdout)
    (folder / "compile.stderr").write_text(process.stderr)
    if not obj.is_file() or obj.stat().st_size <= 0:
        raise ValueError("no Wasmtime AOT generated")
    disassembly = frontend.run(
        [str(bin_path), "objdump", "--funcs", "wasm",
         "--bytes", str(obj)], timeout=180).stdout
    if not disassembly.strip():
        raise ValueError("empty machine disassembly")
    (folder / "disassembly.txt").write_text(disassembly)
    binary = obj.read_bytes()
    return {"sha256": digest(binary), "aot_bytes": len(binary),
            "elf_text_bytes": frontend.text_size(obj),
            "disassembly_sha256": digest(disassembly.encode()),
            "compile_argv": command}


def finite(bin_path: Path, obj: Path) -> dict:
    checks = {}
    for function in ("sat_add", "sat_add_xor", "sat_add_mix", "wrap_add"):
        sample = 0
        for x, y in VECTORS:
            got = frontend.invoke(bin_path, obj, function, x, y)
            if got != value(function, x, y):
                raise ValueError(f"incorrect {function}({x},{y})")
            sample += 1
        checks[function] = sample
    check_sign = sorted(set(x for x, _ in VECTORS))
    for x in check_sign:
        if frontend.invoke(bin_path, obj, "sign_clamp", x) != value("sign_clamp", x):
            raise ValueError(f"incorrect sign_clamp({x})")
    checks["sign_clamp"] = len(check_sign)
    return {"passed": True, "functions": checks,
            "type": "finite-Wasm-execution-only-not-proof"}


def qualify(upstream: Path, bins: dict, out: Path) -> dict:
    if out.exists():
        raise ValueError("refusing to overwrite existing experiment")
    if platform.machine() not in ("x86_64", "AMD64"):
        raise ValueError("requires x86-64 native Wasmtime comparison")
    if set(bins) != {"baseline", "patched"}:
        raise ValueError("exactly two compilers required")
    rv = subprocess.run(["rustc", "--version"], capture_output=True,
                        text=True, check=True, timeout=20).stdout.strip()
    if not rv.startswith("rustc 1.97.1 "):
        raise ValueError("wrong pinned Rust toolchain: " + rv)
    pins = frontend.source_pins(upstream, bins)
    original = SOURCE.read_bytes()
    if b"fn sat_add(" not in original or b"saturating_add" not in original:
        raise ValueError("Rust source no longer contains the specified idiom")
    out.mkdir(parents=True)
    rows = []
    for profile, opt in PROFILES.items():
        directory = out / profile
        directory.mkdir()
        rust = compile_rust(directory, opt)
        wasm = directory / "idiom.wasm"
        compiled = {}
        for label in ("baseline", "patched"):
            folder = directory / label
            folder.mkdir()
            record = compile_aot(bins[label], wasm, folder)
            record["finite"] = finite(bins[label], folder / "idiom.cwasm")
            compiled[label] = record
        a, b = compiled["baseline"], compiled["patched"]
        rows.append({
            "profile": profile, "opt_level": opt,
            "rust_wasm": rust, "results": compiled,
            "baseline_minus_patched": {
                "elf_text_bytes": a["elf_text_bytes"] - b["elf_text_bytes"],
                "whole_cwasm_bytes": a["aot_bytes"] - b["aot_bytes"],
            },
            "binary_identical": a["sha256"] == b["sha256"],
            "disassembly_identical":
                a["disassembly_sha256"] == b["disassembly_sha256"],
        })
    return {
        "schema": SCHEMA,
        "status": "idiomatic-Rust-to-Wasm-compile-and-finite-check",
        "upstream_commit": UPSTREAM, "wasmtime_source_pins": pins,
        "rustc_version": rv, "rust_target": RUST_TARGET,
        "compiler_target": TARGET,
        "rust_source_sha256": digest(original),
        "rust_source_bytes": len(original),
        "producer_script_sha256": digest(Path(__file__).read_bytes()),
        "rule_sha256": digest(RULE.encode()),
        "cases": rows,
        "nonclaims": [
            "Rust source is authored for this experiment, not an external application",
            "two Rust opt profiles produce different Wasm; never compare them as the same program",
            "unmodified and single-rule-patched Wasmtime receive exact same Wasm within each profile",
            "no requirement the Rust compiler emit the hand-written saturation WAT pattern",
            "finite outputs do not constitute all-input or full-machine semantics proof",
            "existing VeriISLE select-model limitation remains",
            "no throughput or general production benefit claimed",
        ],
    }


def main() -> None:
    p = argparse.ArgumentParser()
    for flag in ("upstream", "baseline-bin", "patched-bin", "out"):
        p.add_argument("--" + flag, type=Path, required=True)
    a = p.parse_args()
    result = qualify(a.upstream.resolve(),
        {"baseline": a.baseline_bin.resolve(),
         "patched": a.patched_bin.resolve()}, a.out.resolve())
    (a.out / "qualification.json").write_text(
        json.dumps(result, sort_keys=True, indent=2) + "\n")
    print(json.dumps({
        "status": result["status"],
        "cases": [{
            "profile": row["profile"],
            "deltas": row["baseline_minus_patched"],
            "binary_identical": row["binary_identical"],
        } for row in result["cases"]]}, sort_keys=True))


if __name__ == "__main__":
    main()
