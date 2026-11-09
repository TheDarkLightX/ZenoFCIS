#!/usr/bin/env python3
"""Fail-closed test-only Cranelift udiv/urem -> udivrem adjacent-pair combiner.

Requires a pristine Wasmtime b1f81d2 source checkout. Adds the previously
qualified experimental x64 i64 multi-result opcode and an *adjacent-pair only*
post-egraph rewrite. Never patches ZenoFCIS main or any upstream branch.
"""
import argparse
import json
from pathlib import Path
import subprocess

import install_udivrem_multivalue_28 as multi

UPSTREAM=multi.UPSTREAM
CONTEXT='cranelift/codegen/src/context.rs'
CONTEXT_BLOB='7d6026c97be09f7680a1829a0afcb2a1534608cf'
CALL='''        if opt_level != OptLevel::None {
            self.egraph_pass(isa, ctrl_plane)?;
        }

        Ok(())
'''
AFTER='''        if opt_level != OptLevel::None {
            self.egraph_pass(isa, ctrl_plane)?;
        }
        // EXPERIMENTAL: only x64 i64 adjacent unsigned div/rem with identical operands.
        if opt_level != OptLevel::None && isa.triple().architecture == Architecture::X86_64 {
            tier1_combine_adjacent_unsigned_divrem(&mut self.func);
            self.verify_if(isa)?;
        }

        Ok(())
'''
HELPER=r'''// EXPERIMENTAL TIER1 ADJACENT UDIVREM; NOT AN UPSTREAM CHANGE.
// Only adjacent same-block i64 unsigned division and remainder with exactly
// identical SSA operands. No effects/traps may intervene. The first original
// division is retained as the trap point; the second would never be reached
// with a zero divisor. Both original SSA values are reused, not globally
// substituted by text or by an unverified alias rewrite.
fn tier1_combine_adjacent_unsigned_divrem(func: &mut Function) -> usize {
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
            {
                continue;
            }
            let qa = func.dfg.inst_args(qinst);
            let ra = func.dfg.inst_args(rinst);
            if qa.len() != 2 || ra.len() != 2
                || qa[0] != ra[0] || qa[1] != ra[1]
                || func.dfg.value_type(qa[0]) != types::I64
                || func.dfg.value_type(qa[1]) != types::I64
                || func.dfg.inst_results(qinst).len() != 1
                || func.dfg.inst_results(rinst).len() != 1
            {
                continue;
            }
            let (x, y) = (qa[0], qa[1]);
            let q = func.dfg.inst_results(qinst)[0];
            let r = func.dfg.inst_results(rinst)[0];
            let after = func.layout.next_inst(rinst);
            // Detach SSA results, then reattach *the exact same identities*
            // to one multi-result instruction before the original udiv.
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
// END EXPERIMENTAL TIER1 ADJACENT UDIVREM

'''

def patch_context(s):
    if s.count('impl Context {') != 1 or s.count(CALL) != 1 or 'EXPERIMENTAL TIER1 ADJACENT UDIVREM' in s:
        raise ValueError('unexpected pinned context or repeated installation')
    return s.replace('impl Context {', HELPER+'impl Context {', 1).replace(CALL, AFTER, 1)

def self_test():
    original='a\n'+CALL+'b\nimpl Context {\nc\n'
    patched=patch_context(original)
    assert patched.count('udivrem(x, y)')==1 and 'clear_results(rinst)' in patched
    negatives=[original.replace(CALL, ''), original.replace(CALL, CALL+CALL),
               original.replace('impl Context {',''), patched]
    for s in negatives:
        try: patch_context(s)
        except ValueError: pass
        else: raise AssertionError('patch source guard accepted invalid input')
    assert multi.self_test()
    return True

def git(root,*args):
    return subprocess.check_output(['git','-C',str(root),*args], text=True).strip()

def run(root,out):
    root=root.resolve()
    if git(root,'rev-parse','HEAD')!=UPSTREAM or git(root,'status','--porcelain'):
        raise ValueError('requires pristine pinned Wasmtime checkout')
    blobs=dict(multi.BLOBS)
    blobs[CONTEXT]=CONTEXT_BLOB
    for path,expected in blobs.items():
        src=root/path
        if not src.is_file() or src.is_symlink() or git(root,'hash-object',path)!=expected:
            raise ValueError('pinned source blob mismatch: '+path)
    replacements=multi.patch(*[(root/p).read_text() for p in (multi.META,multi.X64,multi.INTERP)])
    replacements[CONTEXT]=patch_context((root/CONTEXT).read_text())
    for path,content in replacements.items():
        (root/path).write_text(content)
    out.parent.mkdir(parents=True,exist_ok=True)
    out.write_text(json.dumps({
        'status':'experimental-patched-detached-checkout-only',
        'source_commit':UPSTREAM,'original_git_blobs':blobs,
        'modified_source_paths':sorted(replacements),
        'opt_gate':'x86_64 and opt_level != None',
        'match':'adjacent same-block udiv followed by urem, exact identical SSA operands, both i64',
        'proof_status':'NOT proved as a compiler pass; finite execution and actual lowering required',
    },indent=2,sort_keys=True)+'\n')

def main():
    p=argparse.ArgumentParser()
    p.add_argument('--self-test',action='store_true')
    p.add_argument('--upstream',type=Path)
    p.add_argument('--out',type=Path)
    a=p.parse_args()
    if a.self_test:
        self_test();print('combiner installer source guards passed');return
    if a.upstream is None or a.out is None:
        p.error('--upstream and --out required')
    run(a.upstream,a.out)

if __name__=='__main__': main()
