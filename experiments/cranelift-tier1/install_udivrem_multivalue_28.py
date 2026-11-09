#!/usr/bin/env python3
"""Experimental source-pinned Cranelift i64 udivrem multi-result opcode probe.

This adds a new CLIF opcode + x86-64 lowering ONLY in a temporary, detached
upstream checkout. It neither rewrites Wasm input nor implements issue #5623.
"""
import argparse
import json
from pathlib import Path
import subprocess

UPSTREAM='b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49'
META='cranelift/codegen/meta/src/shared/instructions.rs'
X64='cranelift/codegen/src/isa/x64/lower.isle'
INTERP='cranelift/interpreter/src/step.rs'
BLOBS={
 META:'d7122622a292c0300d4b6816c0424c0c25761f25',
 X64:'4fde7e22208bb952cb4e998ce2dca3cab4784ec4',
 INTERP:'cad9149b32edb08781bd7666953312762a4e64ae',
}
META_ANCHOR='    ig.push(\n        Inst::new(\n            "sdiv",\n'
X64_ANCHOR=';; Rules for `sdiv` ;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;;\n'
INTERP_ANCHOR='        Opcode::Udiv => binary_can_trap(DataValueExt::udiv, arg(0), arg(1))?,\\n'
INTERP_NEW='''        Opcode::Udivrem => {
            // This interpreter fallback is part of the disposable opcode pilot.
            // Preserve the same integer-divide-by-zero trap as Udiv and Urem.
            let x = arg(0);
            let y = arg(1);
            match x.clone().udiv(y.clone()) {
                Ok(q) => assign_multiple(&[q, x.urem(y)?]),
                Err(ValueError::IntegerDivisionByZero) => ControlFlow::Trap(
                    CraneliftTrap::User(TrapCode::INTEGER_DIVISION_BY_ZERO)),
                Err(e) => return Err(e.into()),
            }
        }
'''
META_NEW='''    // EXPERIMENTAL TIER1 UDIVREM: scoped x64 i64 feasibility, not upstream.
    ig.push(
        Inst::new(
            "udivrem",
            r#"
        Unsigned integer quotient and remainder simultaneously.
        Traps when the divisor is zero, with both results from one division.
        The quotient and remainder have the same scalar integer width.
        "#,
            &formats.binary,
        )
        .operands_in(&[Operand::new("x", iB), Operand::new("y", iB)])
        .operands_out(&[Operand::new("quotient", iB), Operand::new("remainder", iB)])
        .can_trap()
        .side_effects_idempotent(),
    );

'''
X64_NEW=''';; EXPERIMENTAL TIER1 UDIVREM, only i64; other targets are NOT supported.
;; The existing x64_div emits a physical DIV with two virtual result registers.
;; Quotient is result 0, remainder result 1; never split into two DIVs.
(rule 1 (lower (udivrem _ a @ (value_type $I64) b))
  (x64_div $I64 a (imm $I64 0) (put_in_gpr b) (TrapCode.INTEGER_DIVISION_BY_ZERO)))

'''

def patch(meta, lower, interp):
    if (meta.count(META_ANCHOR)!=1 or lower.count(X64_ANCHOR)!=1
        or interp.count(INTERP_ANCHOR)!=1
        or 'EXPERIMENTAL TIER1 UDIVREM' in meta
        or 'EXPERIMENTAL TIER1 UDIVREM' in lower
        or 'Opcode::Udivrem' in interp):
        raise ValueError('unpinned, duplicate, or already-patched source')
    return {META:meta.replace(META_ANCHOR, META_NEW+META_ANCHOR,1),
            X64:lower.replace(X64_ANCHOR,X64_NEW+X64_ANCHOR,1),
            INTERP:interp.replace(INTERP_ANCHOR,INTERP_NEW+INTERP_ANCHOR,1)}

def self_test():
    orig=patch('PRE\n'+META_ANCHOR+'POST\n','PRE\n'+X64_ANCHOR+'POST\n',INTERP_ANCHOR)
    if 'udivrem' not in orig[META] or 'x64_div' not in orig[X64] or 'Opcode::Udivrem' not in orig[INTERP]:
        raise AssertionError('missing pilot patch')
    for m,l,i in [
        (orig[META],orig[X64]),
        ('no anchor','PRE\n'+X64_ANCHOR),
        ('PRE\n'+META_ANCHOR,'missing'),
        (META_ANCHOR+META_ANCHOR,X64_ANCHOR),
        (META_ANCHOR,X64_ANCHOR+X64_ANCHOR)
    ]:
        try: patch(m,l,i)
        except ValueError: continue
        raise AssertionError('corrupted-source guard failed')
    return True

def run(root,*args):
    return subprocess.run(['git','-C',str(root),*args],check=True,
                          text=True,capture_output=True).stdout.strip()

def main():
    a=argparse.ArgumentParser()
    a.add_argument('--upstream',type=Path)
    a.add_argument('--out',type=Path)
    a.add_argument('--self-test',action='store_true')
    args=a.parse_args()
    if args.self_test:
        self_test()
        print('source-patch positive and five negative controls passed')
        return
    if args.upstream is None or args.out is None:
        raise ValueError('must pass upstream and out')
    root=args.upstream.resolve()
    if run(root,'rev-parse','HEAD')!=UPSTREAM or run(root,'status','--porcelain'):
        raise ValueError('upstream not pristine pinned commit')
    for path,expected in BLOBS.items():
        if run(root,'hash-object',path)!=expected:
            raise ValueError('upstream source blob mismatch: '+path)
        if not (root/path).is_file() or (root/path).is_symlink():
            raise ValueError('source missing or symlink: '+path)
    replacement=patch((root/META).read_text(),(root/X64).read_text(),(root/INTERP).read_text())
    for path,data in replacement.items():
        (root/path).write_text(data)
    args.out.parent.mkdir(parents=True,exist_ok=True)
    args.out.write_text(json.dumps({
        'status':'patched-temporary-checkout-not-upstream',
        'upstream':UPSTREAM,'original_blobs':BLOBS,
        'changed_files':sorted(replacement),
        'scope':'prototype opcode only; x64 i64; no same-Wasm compiler rewrite, proof or optimization claim',
    },sort_keys=True,indent=2)+'\n')
    print('installed experimental multi-result udivrem opcode in detached checkout')

if __name__=='__main__':
    main()
