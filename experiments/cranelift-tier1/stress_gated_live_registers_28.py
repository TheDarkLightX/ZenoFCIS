#!/usr/bin/env python3
"""Recheck conditional-verifier Cranelift on previously qualified live-value WAT.

Three compilers, identical WAT, pinned upstream and exact executable hashes.
Separate finite guest-value/trap evidence from native decoded-byte cost.
This is NOT generic pass, flags, poison, register-allocator or ISA proof.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path

import stress_live_registers_28 as stress
import post_souper_portability_28 as portable
import qualify_udivrem_contexts_28 as matrix

SCHEMA = "zenofcis/tier1-gated-register-crosscheck/1"
UPSTREAM = "8b19f199f89da864c529b3fa3ca2f7e40d5de609"
SOURCE_PARENT = "b6540eedc033591024cca32975710f9c8f6edee7"
PRODUCER_RUN = 44056104464
GATED_PRODUCER_RUN = 44230813420
EXPECTED = {
    "baseline": "b9f0d8ffcb759369ac53d8cf51729438b775f8fe2c3b85edc9532b370ac109e1",
    "patched": "7b9428dbe94a13aeab304fbc05abebb27b65a3ed39846c27ba6e1fa3a1c8beb5",
    "conditional": "206fbef6048ac6ec0b2eb3a2eb334ac5bbd3895bfa64f85c617c5a65004afc8a",
}
MODES = tuple(EXPECTED)

def sha(path: Path) -> str:
    if not path.is_file() or path.is_symlink():
        raise ValueError("missing/linked input: " + str(path))
    return hashlib.sha256(path.read_bytes()).hexdigest()

def pins(root: Path) -> dict:
    producer = stress.identity.producer(root)
    for mode in ("baseline", "patched"):
        if producer["compiler_pins"][mode]["sha256"] != EXPECTED[mode]:
            raise ValueError("original producer compiler pin drift")
    receipt = root / "conditional-install.json"
    d = json.loads(receipt.read_text())
    if (d.get("source_commit") != UPSTREAM or
        d.get("original_git_blobs") != portable.BLOBS or
        "tier1_changes" not in d.get("structural_validation", "")):
        raise ValueError("conditional source installation not pinned")
    for mode, expected in EXPECTED.items():
        if sha(root / mode / "wasmtime") != expected:
            raise ValueError("compiler binary SHA-256 mismatch: " + mode)
    return {
        "producer_run": PRODUCER_RUN,
        "conditional_producer_run": GATED_PRODUCER_RUN,
        "conditional_install_sha256": sha(receipt),
        "sha256": EXPECTED,
    }

def compile_one(exe: Path, source: Path, out: Path, dis: Path) -> dict:
    p = subprocess.run(
        [str(exe), "compile", "--target", stress.TARGET, "-o", str(out), str(source)],
        text=True, capture_output=True, timeout=180)
    if p.returncode or not out.is_file():
        raise RuntimeError("AOT compile failure: " + p.stderr[-1000:])
    p = subprocess.run(
        [str(exe), "objdump", "--funcs", "wasm", "--bytes", str(out)],
        text=True, capture_output=True, timeout=120)
    if p.returncode:
        raise RuntimeError("native objdump failure: " + p.stderr[-1000:])
    dis.write_text(p.stdout)
    cost = matrix.cost(p.stdout)
    return {
        "aot_sha256": sha(out), "aot_bytes": out.stat().st_size,
        "disassembly_sha256": sha(dis),
        "decoded_native_instruction_bytes": portable.rendered_bytes(p.stdout),
        "decoded_instructions": cost["decoded_instructions"],
        "static_divisions": cost["static_divisions"],
    }

def run(root: Path, out: Path) -> None:
    if out.exists():
        raise ValueError("refuse to overwrite evidence")
    compiler = pins(root)
    out.mkdir(parents=True)
    cases = {}
    for kind in stress.KINDS:
        folder = out / kind
        folder.mkdir()
        source = folder / "source.wat"
        source.write_text(stress.wats(kind))
        vals = {}
        for mode in MODES:
            exe = (root / mode / "wasmtime").resolve()
            aot = folder / (mode + ".cwasm")
            dis = folder / (mode + ".disassembly.txt")
            entry = compile_one(exe, source, aot, dis)
            entry["finite"] = stress.finite_check(exe, aot, kind)
            vals[mode] = entry
        for metric in ("aot_sha256", "disassembly_sha256", "decoded_native_instruction_bytes",
                       "decoded_instructions", "static_divisions", "aot_bytes"):
            if vals["patched"][metric] != vals["conditional"][metric]:
                raise AssertionError("conditional verifier changed native output: " + kind + "/" + metric)
        if kind == "mismatched_operands":
            if len({vals[m]["aot_sha256"] for m in MODES}) != 1:
                raise AssertionError("unmatched negative-control output drift")
        cases[kind] = {
            "source_sha256": sha(source),
            "modes": vals,
            "native_byte_savings": (
                vals["baseline"]["decoded_native_instruction_bytes"] -
                vals["conditional"]["decoded_native_instruction_bytes"]),
            "native_div_savings": (
                vals["baseline"]["static_divisions"] -
                vals["conditional"]["static_divisions"]),
        }
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    doc = {
        "schema": SCHEMA, "classification": "finite native-value/trap and measured static-byte equivalence",
        "zenofcis_commit": head, "base_source_commit": SOURCE_PARENT, "upstream": UPSTREAM,
        "compiler_pins": compiler, "target": stress.TARGET,
        "source_script_sha256": sha(Path(__file__)),
        "shared_fixture_script_sha256": sha(Path(stress.__file__)),
        "cases": cases, "vectors_per_context_per_compiler": len(stress.vectors()),
        "limits": [
            "One x86-64 target, one pinned compiler build; no complete compiler-pass or machine-state proof",
            "No portable source-level undefined/poison or x86 flags or ABI equivalence theorem",
            "Each numeric cost is native decoded function instruction bytes, not CLIF cost",
            "Matching AOT images do not prove equivalence for all possible input states",
            "Unmodified compiler is byte-for-byte pinned; gated compiler is rebuilt under source-pinned workflow",
            "No newly synthesized search candidate or production-throughput conclusion",
        ],
    }
    (out / "manifest.json").write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": "three compiler native stress qualified",
        "cases": {k: [v["native_byte_savings"], v["native_div_savings"]] for k, v in cases.items()},
        "finite_executions": len(stress.KINDS) * len(MODES) * len(stress.vectors())}, sort_keys=True))

def audit(root: Path, out: Path) -> None:
    doc = json.loads((out / "manifest.json").read_text())
    compiler = pins(root)
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    if (doc.get("schema") != SCHEMA or doc.get("zenofcis_commit") != head
        or doc.get("base_source_commit") != SOURCE_PARENT or doc.get("upstream") != UPSTREAM
        or doc.get("compiler_pins") != compiler or doc.get("target") != stress.TARGET
        or doc.get("source_script_sha256") != sha(Path(__file__))
        or doc.get("shared_fixture_script_sha256") != sha(Path(stress.__file__))
        or doc.get("vectors_per_context_per_compiler") != len(stress.vectors())
        or set(doc.get("cases", {})) != set(stress.KINDS)):
        raise ValueError("source/compiler/fixture manifest drift")
    for kind in stress.KINDS:
        folder = out / kind
        src = folder / "source.wat"
        row = doc["cases"][kind]
        if src.read_text() != stress.wats(kind) or row["source_sha256"] != sha(src):
            raise ValueError("WAT contract drift")
        for mode in MODES:
            entry = row["modes"][mode]
            exe = (root / mode / "wasmtime").resolve()
            aot = folder / (mode + ".cwasm")
            dis = folder / (mode + ".disassembly.txt")
            data = dis.read_text()
            native = matrix.cost(data)
            if (sha(aot) != entry["aot_sha256"] or sha(dis) != entry["disassembly_sha256"]
                or aot.stat().st_size != entry["aot_bytes"]
                or portable.rendered_bytes(data) != entry["decoded_native_instruction_bytes"]
                or native["static_divisions"] != entry["static_divisions"]
                or native["decoded_instructions"] != entry["decoded_instructions"]
                or stress.finite_check(exe, aot, kind) != entry["finite"]):
                raise ValueError("AOT, disassembly, cost or native output replay drift")
        b, p, c = (row["modes"][m] for m in MODES)
        if (p["aot_sha256"] != c["aot_sha256"]
            or p["disassembly_sha256"] != c["disassembly_sha256"]
            or row["native_byte_savings"] !=
                b["decoded_native_instruction_bytes"] - c["decoded_native_instruction_bytes"]
            or row["native_div_savings"] != b["static_divisions"] - c["static_divisions"]):
            raise ValueError("fabricated native cost or conditional compiler discrepancy")
        if kind == "mismatched_operands" and b["aot_sha256"] != c["aot_sha256"]:
            raise ValueError("negative-control identity lost")
    print("Three compiler conditional-verifier native-output source/byte/cost/trap audit passed")

def self_test() -> None:
    stress.self_test()
    assert set(MODES) == set(EXPECTED) and len({*EXPECTED.values()}) == 3
    assert set(stress.KINDS) == {
        "pressure", "arithmetic_reuse", "incoming_live_args", "mismatched_operands"}
    assert all(re.fullmatch(r"[0-9a-f]{64}", d) for d in EXPECTED.values())
    assert UPSTREAM == stress.PIN == portable.UPSTREAM
    print("Three-mode compiler identity and live-argument oracle preflight passed")

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--mode", choices=("self-test", "run", "audit"), required=True)
    parser.add_argument("--root", type=Path)
    parser.add_argument("--out", type=Path)
    opts = parser.parse_args()
    if opts.mode == "self-test":
        self_test()
    elif opts.root is None or opts.out is None:
        parser.error("--root and --out required for run/audit")
    elif opts.mode == "run":
        run(opts.root, opts.out)
    else:
        audit(opts.root, opts.out)
