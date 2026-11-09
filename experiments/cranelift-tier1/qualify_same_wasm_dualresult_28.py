#!/usr/bin/env python3
"""Dual-result same-Wasm qualification, reusing independent existing AOT audit.

This annotates the new mechanism so the older mul/sub receipt cannot
misdescribe the dual-output DIV transformation. Both variants receive
identical Wasm from one source. Saving is not assumed in advance.
"""
import argparse
import json
from pathlib import Path
import qualify_same_wasm_udivrem_compiler_28 as base

MECHANISM='adjacent same-block i64 udiv/urem to one dual-result CLIF udivrem; x64 DIV RAX/RDX'
SEMANTICS='unsigned Euclidean quotient and remainder; quotient fits 64 bits for nonzero divisor; no modular product/subtraction rewrite'

def collect(out,source):
    delta=base.measurements(out,source)
    receipt=out/'qualification.json'
    obj=json.loads(receipt.read_text())
    assert obj['semantics']['overflow']=='mod 2^64 multiplication and subtraction'
    obj['semantics']['overflow']=SEMANTICS
    obj['semantic_scope']='unsigned i64 with zero-divisor trap; no signed overflow or LLVM poison admitted'
    obj['mechanism']=MECHANISM
    obj['experimental_opcode']='udivrem producing two SSA results from one x64 DIV'
    obj['compiler_pass_proof_status']='unproved; finite executable evidence only'
    obj['qualified_source_kind']='identical WAT passed to baseline and patched compiler'
    receipt.write_text(json.dumps(obj,indent=2,sort_keys=True)+'\n')
    return delta

def audit(out,source):
    obj=json.loads((out/'qualification.json').read_text())
    if (obj.get('mechanism')!=MECHANISM or obj.get('semantics',{}).get('overflow')!=SEMANTICS
        or obj.get('qualified_source_kind')!='identical WAT passed to baseline and patched compiler'
        or obj.get('experimental_opcode')!='udivrem producing two SSA results from one x64 DIV'):
        raise ValueError('dual-result semantics/provenance receipt mismatch')
    delta=base.audit(out,source)
    b=obj['results']['baseline']['static_cost']
    p=obj['results']['patched']['static_cost']
    if b['static_div_instructions']!=2 or p['static_div_instructions']!=1:
        raise ValueError('compiler did not visibly fuse the two static divisions')
    return delta

def main():
    ap=argparse.ArgumentParser()
    ap.add_argument('--mode',required=True,choices=['collect','audit'])
    ap.add_argument('--out',required=True,type=Path)
    ap.add_argument('--source',required=True,type=Path)
    a=ap.parse_args()
    result=collect(a.out,a.source) if a.mode=='collect' else audit(a.out,a.source)
    print(json.dumps(result,sort_keys=True))
if __name__=='__main__':main()
