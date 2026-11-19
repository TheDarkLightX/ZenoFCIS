#!/usr/bin/env python3
"""Experimental reverse-order urem/udiv fusion, against identical pinned Wasm inputs.

Installs into a *disposable* upstream checkout. Source tracing distinguishes
actual pre-lowering pair order from WAT textual order. Reports unknown rather
than asserting an optimization if upstream canonicalizes the order.
No upstream patch, generic refinement proof, or machine-ISA proof is claimed.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
from pathlib import Path

import audit_post_souper_inprocess_28 as identity
import install_udivrem_combiner_28 as legacy
import post_souper_portability_28 as portable
import search_quotrem_expressions_28 as palette
import stress_live_registers_28 as stress

SCHEMA = "zenofcis/tier1-bidirectional-divrem/1"
UPSTREAM = portable.UPSTREAM
CASES = ("forward", "reverse", "mismatched")
MODES = ("baseline", "experimental")
TARGET = palette.TARGET
MASK = (1 << 64) - 1
BASELINE_SHA = "b9f0d8ffcb759369ac53d8cf51729438b775f8fe2c3b85edc9532b370ac109e1"

def digest(p: Path) -> str:
    if not p.is_file() or p.is_symlink():
        raise ValueError("missing/symlinked input: " + str(p))
    return hashlib.sha256(p.read_bytes()).hexdigest()

def replace_once(s: str, old: str, new: str) -> str:
    if s.count(old) != 1:
        raise ValueError("pinned helper anchor missing, repeated or drifted")
    return s.replace(old, new, 1)

def bidirectional_helper() -> str:
    h = legacy.HELPER
    h = replace_once(
        h,
        "// Only adjacent same-block i64 unsigned division and remainder with exactly",
        "// Only adjacent same-block i64 unsigned division and remainder in either order with exactly")
    h = replace_once(
        h,
        "// division is retained as the trap point; the second would never be reached",
        "// operation's source location is retained as the trap point; the second never runs")
    h = replace_once(
        h,
        """            if func.dfg.insts[qinst].opcode() != Opcode::Udiv
                || func.dfg.insts[rinst].opcode() != Opcode::Urem
            {
                continue;
            }""",
        """            let forward = func.dfg.insts[qinst].opcode() == Opcode::Udiv
                && func.dfg.insts[rinst].opcode() == Opcode::Urem;
            let reverse = func.dfg.insts[qinst].opcode() == Opcode::Urem
                && func.dfg.insts[rinst].opcode() == Opcode::Udiv;
            if !forward && !reverse {
                continue;
            }""")
    h = replace_once(
        h,
        """            let q = func.dfg.inst_results(qinst)[0];
            let r = func.dfg.inst_results(rinst)[0];""",
        """            let first_result = func.dfg.inst_results(qinst)[0];
            let second_result = func.dfg.inst_results(rinst)[0];
            let (q, r) = if forward {
                (first_result, second_result)
            } else {
                (second_result, first_result)
            };""")
    h = replace_once(
        h,
        """            count += 1;
            next = after;""",
        """            // Diagnostic is disabled during normal benchmarks; do not infer
            // pre-lowering order from the textual order of WebAssembly ops.
            if std::env::var_os("TIER1_TRACE_DIVREM_ORDER").is_some() {
                eprintln!("TIER1_FUSED_ORDER={}", if forward { "forward" } else { "reverse" });
            }
            count += 1;
            next = after;""")
    return h

def self_test() -> None:
    h = bidirectional_helper()
    for snippet in ("let forward =", "let reverse =", "(second_result, first_result)",
                    "TIER1_FUSED_ORDER=", "use_srcloc(qinst)", "with_results([Some(q), Some(r)])"):
        if snippet not in h:
            raise AssertionError("bidirectional helper misses requirement " + snippet)
    assert h.count("udivrem(x, y)") == 1
    assert h.count("clear_results(qinst)") == 1
    assert h.count("clear_results(rinst)") == 1
    for kind in CASES:
        assert wat(kind).count("i64.div_u") == 1
        assert wat(kind).count("i64.rem_u") == 1
    assert wat("reverse").index("i64.rem_u") < wat("reverse").index("i64.div_u")
    assert wat("forward").index("i64.div_u") < wat("forward").index("i64.rem_u")
    assert oracle("reverse", 10, 3) == oracle("forward", 10, 3)
    assert oracle("mismatched", 10, 2) != oracle("forward", 10, 2)
    # Upstream source anchors must still be checked by the pinned installer.
    portable.self_test()
    print("reverse-pair source shape, patch anchors, signed/unsigned oracle controls passed")

def install(upstream: Path, out: Path) -> None:
    if out.exists():
        raise ValueError("refuse to overwrite source receipt")
    portable.preflight(upstream)
    out.parent.mkdir(parents=True, exist_ok=True)
    portable.install(upstream, out)
    ctx = upstream / legacy.CONTEXT
    original = ctx.read_text()
    ctx.write_text(replace_once(original, legacy.HELPER, bidirectional_helper()))
    modified = set(portable.git(upstream, "diff", "--name-only").splitlines())
    if modified != portable.CHANGED:
        raise ValueError("modified unexpected upstream compiler paths")
    subprocess.run(["git", "-C", str(upstream), "diff", "--check"], check=True)
    record = json.loads(out.read_text())
    record["bidirectional_experiment"] = {
        "scope": "adjacent Udiv/Urem and Urem/Udiv, identical i64 SSA operands, x86_64 only",
        "source_order": "first trap-capable CLIF operation's srcloc carried to combined op",
        "observable": "source-gated TIER1_FUSED_ORDER diagnostic during compilation",
        "compiler_context_sha256": digest(ctx),
        "assurance": "post-rewrite structural verifier and finite tests; NOT semantic compiler-pass or ISA refinement",
    }
    out.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
    print("Installed bidirectional pilot only in pinned disposable Wasmtime checkout")

def wat(kind: str) -> str:
    if kind not in CASES:
        raise ValueError("outside admitted order palette")
    first = "q" if kind == "forward" else "r"
    second = "r" if kind == "forward" else "q"
    def op(which: str) -> str:
        if which == "q":
            return "local.get $x\n    local.get $y\n    i64.div_u\n    local.set $q"
        if kind == "mismatched":
            return ("local.get $x\n    local.get $y\n    i64.const 1\n"
                    "    i64.or\n    i64.rem_u\n    local.set $r")
        return "local.get $x\n    local.get $y\n    i64.rem_u\n    local.set $r"
    return ("""(module
  (func (export "work") (param $x i64) (param $y i64) (result i64)
    (local $q i64) (local $r i64)
    """ + op(first) + "\n    " + op(second) + """
    local.get $q
    local.get $r
    i64.const 17
    i64.rotl
    i64.xor
  )
)
""")

def oracle(kind: str, x: int, y: int) -> int | None:
    ux, uy = x & MASK, y & MASK
    if uy == 0:
        return None
    q = ux // uy
    divisor = (uy | 1) if kind == "mismatched" else uy
    r = ux % divisor
    return stress.signed(q ^ stress.rotl(r, 17))

def finite(exe: Path, aot: Path, kind: str) -> dict:
    good = trapped = 0
    for x, y in stress.vectors():
        p = subprocess.run(
            [str(exe), "run", "--allow-precompiled", "--invoke",
             "work", str(aot), str(x), str(y)],
            capture_output=True, text=True, timeout=45)
        want = oracle(kind, x, y)
        if want is None:
            if p.returncode == 0 or "trap" not in p.stderr.lower():
                raise AssertionError("zero divisor did not trap")
            trapped += 1
        else:
            if p.returncode != 0 or not re.fullmatch(r"-?\d+", p.stdout.strip()):
                raise AssertionError("unexpected guest error: " + p.stderr[-400:])
            if int(p.stdout.strip()) != want:
                raise AssertionError("wrong quotient/remainder result")
            good += 1
    return {"values": good, "expected_traps": trapped}

def compile_one(exe: Path, src: Path, out: Path, dis: Path, trace: Path,
                experimental: bool) -> dict:
    env = os.environ.copy()
    if experimental:
        env["TIER1_TRACE_DIVREM_ORDER"] = "1"
    p = subprocess.run(
        [str(exe), "compile", "--target", TARGET, "-o", str(out), str(src)],
        capture_output=True, text=True, env=env, timeout=180)
    trace.write_text(p.stderr)
    if p.returncode or not out.is_file():
        raise RuntimeError("native compilation failed: " + p.stderr[-1000:])
    p = subprocess.run([str(exe), "objdump", "--funcs", "wasm", "--bytes", str(out)],
                       capture_output=True, text=True, timeout=120)
    if p.returncode:
        raise RuntimeError("native object disassembly failed: " + p.stderr[-1000:])
    dis.write_text(p.stdout)
    cost = palette.matrix.cost(p.stdout)
    return {
        "aot_sha256": digest(out), "aot_bytes": out.stat().st_size,
        "disassembly_sha256": digest(dis), "trace_sha256": digest(trace),
        "native_instruction_bytes": palette.portable.rendered_bytes(p.stdout),
        "decoded_instructions": cost["decoded_instructions"],
        "static_divisions": cost["static_divisions"],
        "diagnostic_forward": trace.read_text().count("TIER1_FUSED_ORDER=forward"),
        "diagnostic_reverse": trace.read_text().count("TIER1_FUSED_ORDER=reverse"),
    }

def pins(root: Path) -> dict:
    old = identity.producer(root)
    if old["compiler_pins"]["baseline"]["sha256"] != BASELINE_SHA:
        raise ValueError("pre-qualified baseline SHA mismatch")
    receipt = json.loads((root / "bidirectional-install.json").read_text())
    pilot = receipt.get("bidirectional_experiment", {})
    if (receipt.get("source_commit") != UPSTREAM
        or receipt.get("original_git_blobs") != portable.BLOBS
        or "source_order" not in pilot
        or not re.fullmatch(r"[0-9a-f]{64}", pilot.get("compiler_context_sha256", ""))):
        raise ValueError("experimental source-patch provenance mismatch")
    if digest(root / "baseline" / "wasmtime") != BASELINE_SHA:
        raise ValueError("baseline binary pin changed")
    return {"baseline": BASELINE_SHA, "experimental": digest(root / "experimental" / "wasmtime"),
            "source_install_sha256": digest(root / "bidirectional-install.json")}

def validate_invariants(rows: dict) -> None:
    for kind in CASES:
        b, e = (rows[kind]["cost"][m] for m in MODES)
        if b["diagnostic_forward"] or b["diagnostic_reverse"]:
            raise ValueError("pristine compiler emitted experimental diagnostics")
        if kind == "mismatched" and (e["diagnostic_forward"] or e["diagnostic_reverse"]):
            raise ValueError("mismatched-operand negative control incorrectly combined")
        if e["diagnostic_forward"] + e["diagnostic_reverse"] > 1:
            raise ValueError("unexpected multi-fusion of one source pair")
        if kind == "mismatched" and b["aot_sha256"] != e["aot_sha256"]:
            raise ValueError("negative-control native output changed")
    # The reverse WAT cannot establish reverse compiler IR order by itself.
    # Only exact patch trace supports that claim; native-byte savings remain
    # conditional on actual reverse fusion.
    if rows["reverse"]["cost"]["experimental"]["diagnostic_reverse"] == 1:
        row = rows["reverse"]
        if row["cost"]["experimental"]["static_divisions"] != 1:
            raise ValueError("reverse trace did not produce one native DIV")

def run(root: Path, dest: Path) -> None:
    if dest.exists():
        raise ValueError("refuse to overwrite existing catalog")
    compilers = pins(root)
    dest.mkdir(parents=True)
    rows = {}
    for kind in CASES:
        folder = dest / kind
        folder.mkdir()
        src = folder / "input.wat"
        src.write_text(wat(kind))
        measured = {}
        for mode in MODES:
            exe = (root / mode / "wasmtime").resolve()
            aot = folder / (mode + ".cwasm")
            dis = folder / (mode + ".disassembly.txt")
            trace = folder / (mode + ".compiler-stderr.txt")
            cost = compile_one(exe, src, aot, dis, trace, mode == "experimental")
            cost["finite"] = finite(exe, aot, kind)
            measured[mode] = cost
        rows[kind] = {
            "source_sha256": digest(src), "cost": measured,
            "native_byte_delta": (measured["baseline"]["native_instruction_bytes"]
                                  - measured["experimental"]["native_instruction_bytes"]),
            "static_div_delta": (measured["baseline"]["static_divisions"]
                                 - measured["experimental"]["static_divisions"]),
            "provenance": "human-authored WAT order, not synthesized x86 machine code",
        }
    validate_invariants(rows)
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    doc = {
        "schema": SCHEMA, "source_commit": head, "wasmtime_commit": UPSTREAM,
        "script_sha256": digest(Path(__file__)), "compiler_pins": compilers,
        "target": TARGET, "palette": list(CASES), "max_candidates": len(CASES),
        "stopping_rule": "all three order/operand contexts evaluated",
        "semantics": {
            "inputs": "two i64 bit patterns interpreted as unsigned words",
            "defined": "y != 0; q=x div_u y; r=x rem_u y (mismatched: r=x rem_u (y|1))",
            "trap": "y == 0 traps at the earliest guest unsigned division/remainder",
            "output": "q XOR rotl64(r,17), i64 modular operations",
            "flags": "Wasm flags unobservable; ISA flags, machine-state, ABI, poison and trap-map equivalence not proved",
        },
        "cases": rows,
        "reverse_ir_fusion_observed": rows["reverse"]["cost"]["experimental"]["diagnostic_reverse"] == 1,
        "scope_limits": [
            "Compiler pair order is established by an explicit diagnostic, not assumed from WAT ordering",
            "Source installer uses same pinned multi-result opcode and structural verifier as prior pilot",
            "Finite native executions and a structural verifier are not an all-input compiler-pass or ISA proof",
            "One x86-64 AOT compiler, one output expression and a small non-representative palette",
            "Decoded native instruction bytes are neither whole-AOT sizes nor CLIF operation costs",
            "This is a human-designed generalization, not newly ZenoFCIS-synthesized native instruction code",
        ],
    }
    (dest / "catalog.json").write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": "experimental-reverse-order-measured",
        "reverse_ir_fusion_observed": doc["reverse_ir_fusion_observed"],
        "native_deltas": {k: v["native_byte_delta"] for k, v in rows.items()}}, sort_keys=True))

def audit(root: Path, dest: Path) -> None:
    d = json.loads((dest / "catalog.json").read_text())
    if (d.get("schema") != SCHEMA
        or d.get("source_commit") != subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
        or d.get("wasmtime_commit") != UPSTREAM or d.get("target") != TARGET
        or d.get("script_sha256") != digest(Path(__file__))
        or d.get("compiler_pins") != pins(root)
        or d.get("palette") != list(CASES) or set(d.get("cases", {})) != set(CASES)):
        raise ValueError("manifest provenance/source/stop scope mismatch")
    for kind in CASES:
        folder = dest / kind
        src = folder / "input.wat"
        row = d["cases"][kind]
        if src.read_text() != wat(kind) or digest(src) != row["source_sha256"]:
            raise ValueError("WAT source drift")
        for mode in MODES:
            exe = (root / mode / "wasmtime").resolve()
            aot = folder / (mode + ".cwasm")
            dis = folder / (mode + ".disassembly.txt")
            trace = folder / (mode + ".compiler-stderr.txt")
            data = dis.read_text()
            c = palette.matrix.cost(data)
            recorded = row["cost"][mode]
            if (digest(aot) != recorded["aot_sha256"]
                or aot.stat().st_size != recorded["aot_bytes"]
                or digest(dis) != recorded["disassembly_sha256"]
                or digest(trace) != recorded["trace_sha256"]
                or palette.portable.rendered_bytes(data) != recorded["native_instruction_bytes"]
                or c["decoded_instructions"] != recorded["decoded_instructions"]
                or c["static_divisions"] != recorded["static_divisions"]
                or trace.read_text().count("TIER1_FUSED_ORDER=forward") != recorded["diagnostic_forward"]
                or trace.read_text().count("TIER1_FUSED_ORDER=reverse") != recorded["diagnostic_reverse"]
                or finite(exe, aot, kind) != recorded["finite"]):
                raise ValueError("native output, trace, cost or finite oracle changed")
        b, e = (row["cost"][m] for m in MODES)
        if (row["native_byte_delta"] != b["native_instruction_bytes"] - e["native_instruction_bytes"]
            or row["static_div_delta"] != b["static_divisions"] - e["static_divisions"]):
            raise ValueError("fabricated native cost delta")
    validate_invariants(d["cases"])
    if d["reverse_ir_fusion_observed"] != (
        d["cases"]["reverse"]["cost"]["experimental"]["diagnostic_reverse"] == 1):
        raise ValueError("reverse IR observation was fabricated")
    print("Pinned source, AOT, native cost, compiler-order trace and finite semantics replay passed")

if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--mode", required=True, choices=("self-test", "install", "run", "audit"))
    p.add_argument("--upstream", type=Path)
    p.add_argument("--root", type=Path)
    p.add_argument("--out", type=Path)
    a = p.parse_args()
    if a.mode == "self-test":
        self_test()
    elif a.mode == "install":
        if a.upstream is None or a.out is None:
            p.error("--upstream and --out required")
        install(a.upstream, a.out)
    else:
        if a.root is None or a.out is None:
            p.error("--root and --out required")
        if a.mode == "run":
            run(a.root, a.out)
        else:
            audit(a.root, a.out)
