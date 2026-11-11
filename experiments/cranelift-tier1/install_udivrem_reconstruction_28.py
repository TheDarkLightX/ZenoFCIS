#!/usr/bin/env python3
"""Source-pinned experimental Cranelift reconstruction of LLVM's q/mul/sub form.
Matches only three adjacent same-block unsigned i64 operations and retains the
intermediate multiply, which might have other users. This is NOT a proved pass.
Applies ONLY to a disposable Wasmtime checkout, never upstream or ZenoFCIS main.
"""
import argparse
import json
import subprocess
from pathlib import Path
import install_udivrem_multivalue_28 as multi

UPSTREAM = multi.UPSTREAM
CONTEXT = 'cranelift/codegen/src/context.rs'
CONTEXT_BLOB = '7d6026c97be09f7680a1829a0afcb2a1534608cf'
CALL = """        if opt_level != OptLevel::None {
            self.egraph_pass(isa, ctrl_plane)?;
        }

        Ok(())
"""
AFTER = "        if opt_level != OptLevel::None {\n            self.egraph_pass(isa, ctrl_plane)?;\n        }\n        // TIER1 EXPERIMENT: bounded unsigned-i64 quotient reconstruction.\n        if opt_level != OptLevel::None && isa.triple().architecture == Architecture::X86_64 {\n            tier1_reconstruct_unsigned_remainder(&mut self.func);\n            self.verify_if(isa)?;\n        }\n\n        Ok(())\n"
HELPER = "// TIER1 RECONSTRUCT UREM PILOT: experiment, not verified compiler pass.\n// Exact adjacent q=udiv(x,y), m=imul(q,y), r=isub(x,m), unsigned i64.\nfn tier1_reconstruct_unsigned_remainder(func: &mut Function) -> usize {\n    use crate::cursor::{Cursor, FuncCursor};\n    use crate::ir::{InstBuilder, Opcode, types};\n    let mut count = 0;\n    let blocks: Vec<_> = func.layout.blocks().collect();\n    for block in blocks {\n        let mut next = func.layout.first_inst(block);\n        while let Some(qinst) = next {\n            let Some(minst) = func.layout.next_inst(qinst) else { break; };\n            let Some(rinst) = func.layout.next_inst(minst) else { break; };\n            next = Some(minst);\n            if func.dfg.insts[qinst].opcode() != Opcode::Udiv\n                || func.dfg.insts[minst].opcode() != Opcode::Imul\n                || func.dfg.insts[rinst].opcode() != Opcode::Isub\n            { continue; }\n            let qa = func.dfg.inst_args(qinst);\n            let ma = func.dfg.inst_args(minst);\n            let ra = func.dfg.inst_args(rinst);\n            if qa.len() != 2 || ma.len() != 2 || ra.len() != 2\n                || func.dfg.value_type(qa[0]) != types::I64\n                || func.dfg.value_type(qa[1]) != types::I64\n                || func.dfg.inst_results(qinst).len() != 1\n                || func.dfg.inst_results(minst).len() != 1\n                || func.dfg.inst_results(rinst).len() != 1\n            { continue; }\n            let (x, y) = (qa[0], qa[1]);\n            let q = func.dfg.inst_results(qinst)[0];\n            let m = func.dfg.inst_results(minst)[0];\n            let r = func.dfg.inst_results(rinst)[0];\n            if !((ma[0] == q && ma[1] == y) || (ma[0] == y && ma[1] == q))\n                || ra[0] != x || ra[1] != m\n            { continue; }\n            let after = func.layout.next_inst(rinst);\n            // Original udiv is the trap site for y == 0; unsigned\n            // quotient/remainder satisfy x - (x/y)*y = x%y for y != 0.\n            func.dfg.clear_results(qinst);\n            func.dfg.clear_results(rinst);\n            {\n                let mut cursor = FuncCursor::new(func).at_inst(qinst);\n                cursor.use_srcloc(qinst);\n                let _ = cursor.ins().with_results([Some(q), Some(r)]).udivrem(x, y);\n            }\n            func.layout.remove_inst(qinst);\n            func.layout.remove_inst(rinst);\n            // Keep imul, possibly with other users; no speculative DCE.\n            count += 1;\n            next = after;\n        }\n    }\n    count\n}\n// END TIER1 RECONSTRUCT UREM PILOT\n\n"

def patch_context(source):
    if (source.count('impl Context {') != 1 or source.count(CALL) != 1
            or 'TIER1 RECONSTRUCT UREM PILOT' in source):
        raise ValueError('unpinned, malformed or already-patched source')
    return source.replace('impl Context {', HELPER + 'impl Context {', 1).replace(CALL, AFTER, 1)

def self_test():
    source = 'HEAD
' + CALL + 'TAIL
impl Context {
'
    assert 'udivrem(x, y)' in patch_context(source)
    assert 'remove_inst(minst)' not in HELPER
    for bad in (source.replace(CALL, ''), source.replace(CALL, CALL + CALL),
                source.replace('impl Context {', ''), patch_context(source)):
        try: patch_context(bad)
        except ValueError: pass
        else: raise AssertionError('failed corruption refusal')
    multi.self_test()
    print('source-pinned reconstruction installer self-tests passed')

def git(root, *args):
    return subprocess.check_output(['git', '-C', str(root), *args], text=True).strip()

def install(root, out):
    root = root.resolve()
    if git(root, 'rev-parse', 'HEAD') != UPSTREAM or git(root, 'status', '--porcelain'):
        raise ValueError('expected pristine pinned upstream source')
    blobs = dict(multi.BLOBS)
    blobs[CONTEXT] = CONTEXT_BLOB
    for path, sha in blobs.items():
        p = root / path
        if not p.is_file() or p.is_symlink() or git(root, 'hash-object', path) != sha:
            raise ValueError('pinned source blob mismatch: ' + path)
    patches = multi.patch(*[(root / p).read_text() for p in (multi.META, multi.X64, multi.INTERP)])
    patches[CONTEXT] = patch_context((root / CONTEXT).read_text())
    for path, new_text in patches.items(): (root / path).write_text(new_text)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps({
        'schema':'zenofcis/tier1-udiv-mul-sub-reconstruction-pilot/1',
        'wasmtime_commit': UPSTREAM, 'original_git_blobs': blobs,
        'modified_source_paths':sorted(patches),
        'contract':'adjacent same-block i64 udiv(x,y), imul(q,y), isub(x,m); y=0 trap unchanged',
        'target':'x86-64, unsigned i64 only; no signed or poison assumptions',
        'intermediate_imul':'retained; its other SSA uses are not assumed absent',
        'assurance':'NOT compiler-pass proved; executable qualification required',
    }, indent=2, sort_keys=True)+'
')

def main():
    p = argparse.ArgumentParser()
    p.add_argument('--self-test', action='store_true')
    p.add_argument('--upstream', type=Path)
    p.add_argument('--out', type=Path)
    args = p.parse_args()
    if args.self_test: return self_test()
    if args.upstream is None or args.out is None: p.error('requires upstream and out')
    install(args.upstream, args.out)

if __name__ == '__main__': main()
