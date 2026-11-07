#!/usr/bin/env python3
"""Source-pinned, deliberately experimental Cranelift same-block div/rem pass."""
import argparse
import subprocess
from pathlib import Path

UPSTREAM="b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
BLOB="7d6026c97be09f7680a1829a0afcb2a1534608cf"
REL="cranelift/codegen/src/context.rs"
ANCHOR="impl Context {\n"
CALL="""        if opt_level != OptLevel::None {
            self.egraph_pass(isa, ctrl_plane)?;
        }

        Ok(())
"""
REPL="""        if opt_level != OptLevel::None {
            self.egraph_pass(isa, ctrl_plane)?;
        }
        // Experimental, opt-in by compiler source patch; not an upstream optimization.
        if opt_level != OptLevel::None && isa.triple().architecture == Architecture::X86_64 {
            tier1_reuse_previous_unsigned_quotient(&mut self.func);
            self.verify_if(isa)?;
        }

        Ok(())
"""
HELPER="""// EXPERIMENTAL TIER1 DIVREM; DO NOT SHIP WITHOUT PROOF AND TARGET CHECKS.
// Existing earlier udiv in SAME BLOCK remains and preserves its zero trap.
// Use only exact SSA operand equality, i64, with quotient before remainder.
fn tier1_reuse_previous_unsigned_quotient(func: &mut Function) -> usize {
    use crate::cursor::{Cursor, FuncCursor};
    use crate::ir::{InstBuilder, Opcode, Value, types};
    let mut count = 0;
    let blocks: Vec<_> = func.layout.blocks().collect();
    for block in blocks {
        let mut quotients: Vec<(Value, Value, Value)> = Vec::new();
        let mut next = func.layout.first_inst(block);
        while let Some(inst) = next {
            next = func.layout.next_inst(inst);
            match func.dfg.insts[inst].opcode() {
                Opcode::Udiv => {
                    let args = func.dfg.inst_args(inst);
                    if args.len() == 2 && func.dfg.value_type(args[0]) == types::I64 {
                        let results = func.dfg.inst_results(inst);
                        if results.len() == 1 {
                            quotients.push((args[0], args[1], results[0]));
                        }
                    }
                }
                Opcode::Urem => {
                    let args = func.dfg.inst_args(inst);
                    if args.len() != 2 || func.dfg.value_type(args[0]) != types::I64 {
                        continue;
                    }
                    let (x, y) = (args[0], args[1]);
                    if let Some((_, _, q)) = quotients.iter().rev().copied()
                        .find(|(a, b, _)| *a == x && *b == y)
                    {
                        let mut cursor = FuncCursor::new(func).at_inst(inst);
                        cursor.use_srcloc(inst);
                        let product = cursor.ins().imul(q, y);
                        // In-place replacement retains the original urem result value.
                        cursor.func.replace(inst).isub(x, product);
                        count += 1;
                    }
                }
                _ => {}
            }
        }
    }
    count
}
// END EXPERIMENTAL TIER1 DIVREM

"""
def patch(text):
    if (text.count(ANCHOR)!=1 or text.count(CALL)!=1
            or "EXPERIMENTAL TIER1 DIVREM" in text):
        raise ValueError("compiler source mismatch/reapply refused")
    return text.replace(ANCHOR,HELPER+ANCHOR,1).replace(CALL,REPL,1)

def main():
    a=argparse.ArgumentParser()
    a.add_argument("--upstream",type=Path,required=True)
    root=a.parse_args().upstream.resolve()
    def git(*args):
        return subprocess.check_output(("git","-C",str(root),*args),text=True).strip()
    if git("rev-parse","HEAD")!=UPSTREAM or git("status","--porcelain"):
        raise ValueError("not a pristine pinned upstream checkout")
    if git("hash-object",REL)!=BLOB:
        raise ValueError("unpinned context source")
    file=root/REL
    if file.is_symlink() or not file.is_file():
        raise ValueError("missing or symlinked compiler file")
    file.write_text(patch(file.read_text()))
    print("EXPERIMENTAL TIER1 DIVREM installed in pinned Cranelift context")

if __name__=="__main__":
    main()
