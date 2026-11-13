#!/usr/bin/env python3
"""Strict source-pinned pilot: allow exactly iconst + rotl between live imul and isub.

Changes ONLY a disposable checkout of pinned Wasmtime, building a separate
experimental compiler. Keeps the multiplication live for other users.
This pilot is unproved as a compiler pass and must not be submitted upstream.
"""
from __future__ import annotations
import argparse
import json
from pathlib import Path
import install_udivrem_reconstruction_28 as base

OLD = """            let Some(minst) = func.layout.next_inst(qinst) else { break; };
            let Some(rinst) = func.layout.next_inst(minst) else { break; };
            next = Some(minst);
"""
NEW = """            let Some(minst) = func.layout.next_inst(qinst) else { break; };
            let Some(midconst) = func.layout.next_inst(minst) else { break; };
            let Some(midrot) = func.layout.next_inst(midconst) else { break; };
            let Some(rinst) = func.layout.next_inst(midrot) else { break; };
            next = Some(minst);
            // Bounded same-block reachability: two explicit, nontrapping
            // integer instructions only; never skip a call, store or trap.
            if func.dfg.insts[midconst].opcode() != Opcode::Iconst
                || func.dfg.insts[midrot].opcode() != Opcode::Rotl
            { continue; }
"""
FENCE = """            let m = func.dfg.inst_results(minst)[0];
            let r = func.dfg.inst_results(rinst)[0];
"""
ADDED_FENCE = FENCE + """            // Require the rotl to be a live use of the exact intermediate
            // multiply with the exact iconst operand, not merely any rotl.
            let cv = func.dfg.inst_results(midconst);
            let rv = func.dfg.inst_args(midrot);
            if cv.len() != 1 || rv.len() != 2
                || rv[0] != m || rv[1] != cv[0]
            { continue; }
"""

def derive() -> str:
    original = base.HELPER
    if original.count(OLD) != 1 or original.count(FENCE) != 1:
        raise ValueError("unexpected base installer; refuse non-pinned edit")
    revised = original.replace(OLD, NEW, 1).replace(FENCE, ADDED_FENCE, 1)
    if revised.count("let Some(midconst)") != 1 or revised.count("let Some(midrot)") != 1:
        raise ValueError("missing or duplicate bounded gap")
    return revised.replace(
        "// Exact adjacent q=udiv(x,y), m=imul(q,y), r=isub(x,m), unsigned i64.",
        "// Exact same-block q,m,iconst,rotl(m,iconst),r pattern, unsigned i64."
    )

def self_test() -> None:
    txt = derive()
    assert "Opcode::Iconst" in txt and "Opcode::Rotl" in txt
    assert "rv[0] != m" in txt and "rv[1] != cv[0]" in txt
    assert "remove_inst(minst)" not in txt
    assert "use_srcloc(qinst)" in txt
    assert txt.count("udivrem(x, y)") == 1
    base.self_test()
    print("bounded live-gap source installer and strict controls passed")

def install(root: Path, receipt: Path) -> None:
    # Explicit per-process helper override only; base.install still validates
    # exact upstream SHA, pristine git status and every original source blob.
    base.HELPER = derive()
    base.install(root, receipt)
    data = json.loads(receipt.read_text())
    data["schema"] = "zenofcis/tier1-udivrem-live-iconst-rotl-gap-pilot/1"
    data["contract"] = (
        "same-block, unsigned i64: udiv(x,y), imul(q,y), "
        "iconst i64 c, rotl(m,c), isub(x,m); exact SSA operands"
    )
    data["opt_scope"] = "x86-64 only, non-None opt level; no general skipping of intermediate instructions"
    data["intermediate_ops"] = ["Iconst", "Rotl"]
    data["source_location"] = "udivrem inherits original udiv site"
    data["intermediate_multiply"] = "retained and its SSA uses remain live"
    data["assurance"] = "experimental compiler patch, not a compiler-pass/ISA/VeriISLE proof"
    receipt.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")

if __name__ == "__main__":
    p=argparse.ArgumentParser()
    p.add_argument("--self-test",action="store_true")
    p.add_argument("--upstream",type=Path)
    p.add_argument("--out",type=Path)
    a=p.parse_args()
    if a.self_test: self_test()
    else:
        if not a.upstream or not a.out: p.error("requires --upstream and --out")
        install(a.upstream,a.out)
