#!/usr/bin/env python3
"""Experiment: fuse i64 quotient/remainder across exactly one pure CLIF iadd.

Installs only in a disposable pinned Wasmtime source checkout. WAT ordering
is not treated as proof of actual post-egraph CLIF ordering. Finite guest
execution and emitted x64 byte measurements do not prove compiler-pass,
machine-state, flag, ABI, or trap-map refinement.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import subprocess

import reverse_order_udivrem_28 as reverse
import stress_live_registers_28 as stress

UPSTREAM = reverse.UPSTREAM
MASK = (1 << 64) - 1
CASES = ("forward", "forward_gap_add", "reverse_gap_add", "gap_div", "mismatched")
MODES = ("baseline", "experimental")
SCHEMA = "zenofcis/tier1-pure-gap-divrem/1"


def digest(path: Path) -> str:
    return reverse.digest(path)


def gap_helper() -> str:
    helper = reverse.bidirectional_helper()
    old = """            let Some(rinst) = func.layout.next_inst(qinst) else { break; };
            next = Some(rinst);
            let forward ="""
    new = """            let Some(following) = func.layout.next_inst(qinst) else { break; };
            // Only one wrapping, pure integer addition may be skipped.
            // Every other opcode is a barrier: no broad effect predicate.
            let (rinst, skipped_iadd) = if func.dfg.insts[following].opcode() == Opcode::Iadd {
                match func.layout.next_inst(following) {
                    Some(after) => (after, true),
                    None => (following, false),
                }
            } else {
                (following, false)
            };
            next = Some(rinst);
            let forward ="""
    helper = reverse.replace_once(helper, old, new)
    old = """            count += 1;
            next = after;"""
    new = """            if std::env::var_os("TIER1_TRACE_DIVREM_ORDER").is_some() {
                eprintln!("TIER1_FUSED_GAP={}", if skipped_iadd { "one_iadd" } else { "adjacent" });
            }
            count += 1;
            next = after;"""
    return reverse.replace_once(helper, old, new)


def wat(case: str) -> str:
    if case in ("forward", "mismatched"):
        return reverse.wat(case)
    if case not in CASES:
        raise ValueError("unadmitted case")
    first, second = ("r", "q") if case == "reverse_gap_add" else ("q", "r")
    def op(which: str) -> str:
        return ("local.get $x\n    local.get $y\n    "
                + ("i64.div_u\n    local.set $q" if which == "q" else
                   "i64.rem_u\n    local.set $r"))
    if case == "gap_div":
        gap = "i64.const 1\n    local.get $x\n    i64.div_u\n    local.set $gap"
    else:
        gap = "local.get $" + first + "\n    i64.const 7\n    i64.add\n    local.set $gap"
    return ('(module\n  (func (export "work") (param $x i64) (param $y i64) (result i64)\n'
            '    (local $q i64) (local $r i64) (local $gap i64)\n    '
            + op(first) + "\n    " + gap + "\n    " + op(second)
            + '\n    local.get $q\n    local.get $r\n    i64.const 17\n'
            '    i64.rotl\n    i64.xor\n    local.get $gap\n    i64.xor\n  )\n)\n')


def oracle(case: str, x: int, y: int) -> int | None:
    if case in ("forward", "mismatched"):
        return reverse.oracle(case, x, y)
    x, y = x & MASK, y & MASK
    if y == 0 or (case == "gap_div" and x == 0):
        return None
    q, r = divmod(x, y)
    gap = (1 // x) if case == "gap_div" else (((r if case == "reverse_gap_add" else q) + 7) & MASK)
    return stress.signed(q ^ stress.rotl(r, 17) ^ gap)


def self_test() -> None:
    h = gap_helper()
    assert h.count("Opcode::Iadd") == 1
    assert "Some(after) => (after, true)" in h
    assert "TIER1_FUSED_GAP=" in h
    assert h.count("udivrem(x, y)") == 1
    assert "cursor.use_srcloc(qinst)" in h
    assert h.count("clear_results(qinst)") == h.count("clear_results(rinst)") == 1
    for case in CASES:
        src = wat(case)
        assert src.count("i64.div_u") == (2 if case == "gap_div" else 1)
        assert src.count("i64.rem_u") == 1
    assert oracle("gap_div", 0, 3) is None
    assert oracle("gap_div", 2, 3) is not None
    assert oracle("forward_gap_add", 10, 3) != oracle("reverse_gap_add", 10, 3)
    reverse.self_test()
    print("pure-iadd gap patch, semantics and negative control self-test passed")


def install(upstream: Path, out: Path) -> None:
    reverse.install(upstream, out)
    ctx = upstream / reverse.legacy.CONTEXT
    ctx.write_text(reverse.replace_once(ctx.read_text(), reverse.bidirectional_helper(), gap_helper()))
    subprocess.run(["git", "-C", str(upstream), "diff", "--check"], check=True)
    record = json.loads(out.read_text())
    record["pure_gap_experiment"] = {
        "scope": "adjacent or exactly one intervening CLIF Opcode::Iadd in either operand order",
        "guard": "identical SSA operands, both i64, same block, only Opcode::Iadd skipped",
        "trap": "first potentially trapping source location retained, one original div/rem removed",
        "limits": "No generic effects analysis, signed division, cross-block motion or machine-state proof",
        "final_context_sha256": digest(ctx),
        "trace": "actual pre-lowering TIER1_FUSED_ORDER and TIER1_FUSED_GAP diagnostics",
    }
    out.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")


def check_native(exe: Path, aot: Path, case: str) -> dict:
    good = trapped = 0
    for x, y in stress.vectors():
        p = subprocess.run([str(exe), "run", "--allow-precompiled", "--invoke",
                            "work", str(aot), str(x), str(y)],
                           capture_output=True, text=True, timeout=40)
        want = oracle(case, x, y)
        if want is None:
            if p.returncode == 0 or "trap" not in p.stderr.lower():
                raise AssertionError("missing expected native trap " + case)
            trapped += 1
        else:
            if p.returncode or not re.fullmatch(r"-?\d+", p.stdout.strip()) or int(p.stdout.strip()) != want:
                raise AssertionError("wrong native value " + case + " " + str((x, y)) + ": " + p.stderr[-250:])
            good += 1
    return {"correct_values": good, "expected_traps": trapped}


def pins(root: Path) -> dict:
    old = reverse.identity.producer(root)
    if old["compiler_pins"]["baseline"]["sha256"] != reverse.BASELINE_SHA:
        raise ValueError("baseline source/binary identity changed")
    record = json.loads((root / "pure-gap-install.json").read_text())
    if (record.get("source_commit") != UPSTREAM
        or record.get("original_git_blobs") != reverse.portable.BLOBS
        or "final_context_sha256" not in record.get("pure_gap_experiment", {})):
        raise ValueError("experimental source pin mismatch")
    return {"baseline": digest(root / "baseline" / "wasmtime"),
            "experimental": digest(root / "experimental" / "wasmtime"),
            "source_install_sha256": digest(root / "pure-gap-install.json")}


def run(root: Path, out: Path) -> None:
    out.mkdir(parents=True, exist_ok=False)
    sourcepins = pins(root)
    if sourcepins["baseline"] != reverse.BASELINE_SHA:
        raise ValueError("baseline binary changed")
    rows = {}
    for case in CASES:
        d = out / case
        d.mkdir()
        src = d / "input.wat"
        src.write_text(wat(case))
        row = {"wat_sha256": digest(src), "compilers": {}}
        for mode in MODES:
            exe = root / mode / "wasmtime"
            aot, dis, trace = d / (mode + ".cwasm"), d / (mode + ".dis"), d / (mode + ".trace")
            data = reverse.compile_one(exe, src, aot, dis, trace, mode == "experimental")
            t = trace.read_text()
            data["trace_one_iadd"] = t.count("TIER1_FUSED_GAP=one_iadd")
            data["trace_adjacent"] = t.count("TIER1_FUSED_GAP=adjacent")
            data["native"] = check_native(exe, aot, case)
            row["compilers"][mode] = data
        rows[case] = row
    for case, row in rows.items():
        b, e = [row["compilers"][mode] for mode in MODES]
        if b["trace_one_iadd"] or b["trace_adjacent"]:
            raise ValueError("pristine compiler emitted pilot trace")
        if e["trace_one_iadd"] and case not in ("forward_gap_add", "reverse_gap_add"):
            raise ValueError("skipped nonpure operation or mismatched operand")
        if case == "mismatched" and b["aot_sha256"] != e["aot_sha256"]:
            raise ValueError("negative-control output changed")
        if e["trace_one_iadd"] + e["trace_adjacent"] > 1:
            raise ValueError("unexpected multiple fusions")
    receipt = {
        "schema": SCHEMA,
        "source": {"zenofcis": os.environ.get("GITHUB_SHA", "unavailable"),
                   "wasmtime": UPSTREAM, "compiler_pins": sourcepins},
        "semantics": {"inputs": "two unsigned 64-bit words",
                      "domain": "y=0 traps; gap_div also traps on x=0",
                      "overflow": "wrapping i64 addition; signed division not admitted",
                      "flags": "no CLIF iadd flag output; full x86 flags unproved",
                      "undefined_poison": "no poison in admitted Wasm ops; full CLIF refinement unproved"},
        "qualification": "source-pinned finite guest oracle, actual pre-lowering trace, exact native instruction bytes; NOT universal pass proof",
        "rows": rows,
    }
    (out / "catalog.json").write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    print("native measurements:", json.dumps({c: {m: rows[c]["compilers"][m]["native_instruction_bytes"]
                                              for m in MODES} for c in CASES}, sort_keys=True))


def audit(root: Path, out: Path) -> None:
    receipt = json.loads((out / "catalog.json").read_text())
    if (receipt.get("schema") != SCHEMA
        or receipt["source"]["wasmtime"] != UPSTREAM
        or receipt["source"]["compiler_pins"] != pins(root)
        or set(receipt["rows"]) != set(CASES)):
        raise ValueError("receipt envelope/source/compiler changed")
    for case in CASES:
        row = receipt["rows"][case]
        d = out / case
        if digest(d / "input.wat") != row["wat_sha256"] or (d / "input.wat").read_text() != wat(case):
            raise ValueError("input changed")
        for mode in MODES:
            r = row["compilers"][mode]
            exe = root / mode / "wasmtime"
            aot, dis, trace = d / (mode + ".cwasm"), d / (mode + ".dis"), d / (mode + ".trace")
            if [digest(aot), digest(dis), digest(trace)] != [r["aot_sha256"], r["disassembly_sha256"], r["trace_sha256"]]:
                raise ValueError("original AOT/disassembly/trace changed")
            c = reverse.palette.matrix.cost(dis.read_text())
            if (r["native_instruction_bytes"] != reverse.palette.portable.rendered_bytes(dis.read_text())
                or r["decoded_instructions"] != c["decoded_instructions"]
                or r["static_divisions"] != c["static_divisions"]
                or r["trace_one_iadd"] != trace.read_text().count("TIER1_FUSED_GAP=one_iadd")
                or r["trace_adjacent"] != trace.read_text().count("TIER1_FUSED_GAP=adjacent")
                or r["native"] != check_native(exe, aot, case)):
                raise ValueError("native cost, trace or guest replay mismatch")
    print("exact original-source, native-byte, trace and execution replay passed")


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--mode", required=True, choices=("self-test", "install", "run", "audit"))
    p.add_argument("--upstream", type=Path)
    p.add_argument("--root", type=Path)
    p.add_argument("--out", type=Path)
    a = p.parse_args()
    if a.mode == "self-test":
        self_test()
    elif a.mode == "install":
        if a.upstream is None or a.out is None: p.error("--upstream and --out required")
        install(a.upstream, a.out)
    else:
        if a.root is None or a.out is None: p.error("--root and --out required")
        (run if a.mode == "run" else audit)(a.root, a.out)


if __name__ == "__main__":
    main()
