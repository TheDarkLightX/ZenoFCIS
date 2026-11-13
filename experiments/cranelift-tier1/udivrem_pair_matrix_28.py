#!/usr/bin/env python3
"""Bounded adjacent-udiv/urem CLIF palette and reproducible pinned Cranelift A/B.

Two proposals: separate unsigned quotient/remainder versus one experimental
multi-result udivrem. This is not a search for a globally optimal sequence,
and the new opcode/lowering is NOT a certified upstream-ready compiler pass.
"""
from __future__ import annotations
import argparse
import json
import re
import tempfile
from pathlib import Path
import livegap_clif_matrix_28 as engine

SCHEMA = 'zenofcis/tier1-adjacent-divrem-native-palette/1'
ENGINE_BASE = engine.BASE
ENGINE_CASES = engine.CASE_EDITS
BASE = [
    '    v2 = udiv v0, v1',
    '    v3 = urem v0, v1',
    '    v4 = bxor v2, v3',
    '    return v4',
]
CASES = {
    'positive_quotient_remainder': ('expected_match', {}),
    'negative_reverse_order': ('must_not_match', {
        0: '    v2 = urem v0, v1', 1: '    v3 = udiv v0, v1'}),
    'negative_signed_quotient': ('must_not_match', {0: '    v2 = sdiv v0, v1'}),
    'negative_other_dividend': ('must_not_match', {1: '    v3 = urem v1, v1'}),
    'negative_other_divisor': ('must_not_match', {1: '    v3 = urem v0, v0'}),
    'negative_not_remainder': ('must_not_match', {1: '    v3 = isub v0, v1'}),
    'negative_double_quotient': ('must_not_match', {1: '    v3 = udiv v0, v1'}),
    'negative_intervening_live_iconst': ('must_not_match', {
        1: '    v5 = iconst.i64 9\n    v3 = urem v0, v1',
        3: '    v6 = bxor v4, v5\n    return v6'}),
}

def configure():
    engine.BASE = BASE
    engine.CASE_EDITS = CASES

def generator(path: Path) -> dict:
    configure()
    manifest = engine.generate(path)
    manifest['schema'] = SCHEMA
    manifest['candidate_palette'] = ['udiv.i64+urem.i64', 'udivrem.i64']
    manifest['provenance'] = 'human-authored bounded pair; not discovered by an optimality search'
    (path/'manifest.json').write_text(json.dumps(manifest,sort_keys=True,indent=2)+'\n')
    return manifest

def bitserial(a: int,b: int,width: int):
    if b==0:
        raise ZeroDivisionError()
    q=r=0
    for i in range(width-1,-1,-1):
        r=(r<<1)|((a>>i)&1)
        if r>=b:
            q|=1<<i
            r-=b
    return q,r

def finite_semantics():
    v=t=0
    for x in range(256):
        for y in range(256):
            if not y:
                t+=1
                continue
            q,r=bitserial(x,y,8)
            assert q==x//y and r==x%y and q*y+r==x and 0<=r<y
            v+=1
    assert (v,t)==(65280,256)
    return {'unsigned_i8_values_exhaustive':v,'unsigned_i8_first_division_traps':t,
            'signed_overflow':'excluded; unsigned i64 only',
            'i64':'mathematical identity but not exhaustively or SMT-checked by this helper'}

def inspect(src:Path,base:Path,patched:Path,out:Path,basebin:Path,patchbin:Path,checkout:str):
    configure()
    report=engine.analyze(src,base,patched,out,basebin,patchbin,checkout)
    counts={}
    for name in CASES:
        a=engine.profile(base/(name+'.o'))['disassembly']
        b=engine.profile(patched/(name+'.o'))['disassembly']
        div=lambda s:len(re.findall(r'\bdiv\s+',s))
        counts[name]={'baseline_div_instructions':div(a),'patched_div_instructions':div(b)}
    report['schema']=SCHEMA
    report['candidate_palette']=['udiv.i64+urem.i64','udivrem.i64']
    report['finite_semantics']=finite_semantics()
    report['division_counts']=counts
    report['status']='bounded-palette-compiled-measured-not-formal-pass-proof'
    report['limitations']+=[
        'New CLIF Udivrem opcode is experimental, implemented only in temporary x64 compiler',
        'The source-pinned pass changes only ordered adjacent equal-operand unsigned i64 pairs',
        'A successful direct-CLIF A/B does not establish optimized Rust/Wasm reachability',
        'SMT equivalence, native ABI, trap-code provenance and general compiler correctness remain unproved',
        'Exact same input CLIF A/B establishes static bytes/division count, not runtime throughput',
    ]
    out.write_text(json.dumps(report,sort_keys=True,indent=2)+'\n')
    return report

def self_test():
    with tempfile.TemporaryDirectory() as tmp:
        path=Path(tmp)/'matrix'
        m=generator(path)
        assert len(m['cases'])==8
        assert m['schema']==SCHEMA
        assert 'v2 = udiv v0, v1\n    v3 = urem v0, v1' in (path/'positive_quotient_remainder.clif').read_text()
        for name,(scope,_) in CASES.items():
            if scope=='must_not_match':
                assert (path/(name+'.clif')).read_text() != (path/'positive_quotient_remainder.clif').read_text()
        try: generator(path)
        except ValueError: pass
        else: raise AssertionError('overwrite accepted')
    assert finite_semantics()['unsigned_i8_values_exhaustive']==65280
    print('8 source graphs + 65,280 exhaustive i8 quotient/remainder pairs and 256 modeled traps passed')

if __name__=='__main__':
    p=argparse.ArgumentParser()
    p.add_argument('--self-test',action='store_true')
    p.add_argument('--generate',type=Path)
    p.add_argument('--analyze',action='store_true')
    p.add_argument('--src',type=Path)
    p.add_argument('--baseline',type=Path)
    p.add_argument('--patched',type=Path)
    p.add_argument('--baseline-compiler',type=Path)
    p.add_argument('--patched-compiler',type=Path)
    p.add_argument('--checkout')
    p.add_argument('--out',type=Path)
    a=p.parse_args()
    if a.self_test: self_test()
    elif a.generate: print(json.dumps(generator(a.generate),sort_keys=True))
    elif a.analyze:
        if any(v is None for v in (a.src,a.baseline,a.patched,a.out,a.baseline_compiler,a.patched_compiler,a.checkout)):
            p.error('analyze requires src, both object directories, both compilers, checkout and out')
        r=inspect(a.src,a.baseline,a.patched,a.out,a.baseline_compiler,a.patched_compiler,a.checkout)
        print(json.dumps({'status':r['status'],'positive_native_changes':r['observed_positive_changes'],
                          'divisions':r['division_counts']['positive_quotient_remainder']},sort_keys=True))
    else: p.error('choose --self-test, --generate, or --analyze')
