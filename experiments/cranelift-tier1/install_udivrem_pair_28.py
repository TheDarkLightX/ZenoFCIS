#!/usr/bin/env python3
"""Source-pinned prototype: adjacent same-block unsigned i64 udiv+urem -> udivrem.

This is an experimental, x86-64-only in-place IR rewrite and *not* an
upstream Cranelift patch, full-pass proof, or general-purpose optimization.
"""
from __future__ import annotations
import argparse
import json
from pathlib import Path
import install_udivrem_reconstruction_28 as base

# Rename nothing the base context hook invokes: its source/target/opt guards,
# pristine Wasmtime commit and exact source blob checks are all retained.
PAIR_HELPER = '''// TIER1 RECONSTRUCT UREM PILOT: adjacent pair, not verified compiler pass.
// Match only adjacent same-block unsigned i64 q=udiv(x,y), r=urem(x,y).
fn tier1_reconstruct_unsigned_remainder(func: &mut Function) -> usize {
    use crate::cursor::{Cursor, FuncCursor};
    use crate::ir::{InstBuilder, Opcode, types};
    let mut count = 0;
    let blocks: Vec<_> = func.layout.blocks().collect();
    for block in blocks {
        let mut next = func.layout.first_inst(block);
        while let Some(qinst) = next {
            let Some(rinst) = func.layout.next_inst(qinst) else { break; };
            next = Some(rinst);
            if func.dfg.insts[qinst].opcode() != Opcode::Udiv
                || func.dfg.insts[rinst].opcode() != Opcode::Urem
            { continue; }
            let qa = func.dfg.inst_args(qinst);
            let ra = func.dfg.inst_args(rinst);
            if qa.len() != 2 || ra.len() != 2
                || qa[0] != ra[0] || qa[1] != ra[1]
                || func.dfg.value_type(qa[0]) != types::I64
                || func.dfg.value_type(qa[1]) != types::I64
                || func.dfg.inst_results(qinst).len() != 1
                || func.dfg.inst_results(rinst).len() != 1
            { continue; }
            let (x, y) = (qa[0], qa[1]);
            let q = func.dfg.inst_results(qinst)[0];
            let r = func.dfg.inst_results(rinst)[0];
            let after = func.layout.next_inst(rinst);
            // q and r were computed from the exact same SSA inputs. If
            // y==0, q traps before r; if y!=0 neither operation traps.
            // Thus one udivrem preserves quotient/remainder and the first
            // trap location for this narrow adjacent ordered pair.
            func.dfg.clear_results(qinst);
            func.dfg.clear_results(rinst);
            {
                let mut cursor = FuncCursor::new(func).at_inst(qinst);
                cursor.use_srcloc(qinst);
                let _ = cursor.ins().with_results([Some(q), Some(r)]).udivrem(x, y);
            }
            func.layout.remove_inst(qinst);
            func.layout.remove_inst(rinst);
            count += 1;
            next = after;
        }
    }
    count
}
// END TIER1 RECONSTRUCT UREM PILOT

'''

def self_test() -> None:
    assert PAIR_HELPER.count('udivrem(x, y)') == 1
    assert 'Opcode::Urem' in PAIR_HELPER and 'qa[0] != ra[0]' in PAIR_HELPER
    assert 'qa[1] != ra[1]' in PAIR_HELPER
    assert 'func.layout.next_inst(qinst)' in PAIR_HELPER
    assert 'func.dfg.clear_results(rinst)' in PAIR_HELPER
    assert PAIR_HELPER.count('fn tier1_reconstruct_unsigned_remainder') == 1
    base.HELPER = PAIR_HELPER
    base.self_test()
    print('adjacent unsigned i64 pair installer source-pattern self-tests passed')

def install(root: Path, out: Path) -> None:
    base.HELPER = PAIR_HELPER
    base.install(root, out)
    receipt = json.loads(out.read_text())
    receipt.update({
        'schema': 'zenofcis/tier1-adjacent-udiv-urem-to-udivrem-prototype/1',
        'contract': 'same-block immediately adjacent udiv.i64(x,y); urem.i64(x,y); exact SSA operands',
        'opt_scope': 'x86-64 and non-None opt_level only; extra uses of either result preserved',
        'semantics': 'all concrete unsigned 64-bit x,y; y==0 first udiv trap; no signed division',
        'target': 'x86-64 only; experimental new CLIF opcode and lowering',
        'cost_evidence': 'NONE until identical source baseline/patch compiler measurements complete',
        'assurance': 'NOT compiler-pass or machine-ISA proof; flags/poison/metadata unqualified',
    })
    out.write_text(json.dumps(receipt, sort_keys=True, indent=2)+'\n')

if __name__ == '__main__':
    p=argparse.ArgumentParser()
    p.add_argument('--self-test',action='store_true')
    p.add_argument('--upstream',type=Path)
    p.add_argument('--out',type=Path)
    a=p.parse_args()
    if a.self_test: self_test()
    else:
        if a.upstream is None or a.out is None:
            p.error('requires --upstream and --out')
        install(a.upstream,a.out)
