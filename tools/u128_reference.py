#!/usr/bin/env python3
"""Independent exact-integer U128 oracle; no native arithmetic helpers imported."""
from pathlib import Path
import argparse
M = (1 << 128) - 1
H = (1 << 127) - 1
L = -(1 << 127)
DATA = Path(__file__).resolve().parents[1] / 'crates/zeno-fcis-contract/tests/test-data/u128/oracle.tsv'
def answer(kind, a, b, d):
    if kind == 'to_u128': return a if 0 <= a <= M else None
    if kind == 'to_i128': return a if L <= a <= H else None
    if kind == 'add': q = a+b
    elif kind == 'sub': q = a-b
    elif kind == 'mul': q = a*b
    elif kind in ('div_floor','div_ceil','floor','ceil'):
        if d == 0: return None
        n = a if kind.startswith('div_') else a*b
        q = n//d
        if kind.endswith('ceil') and n%d: q += 1
    else: raise ValueError(kind)
    return q if 0 <= q <= M else None

def rows():
    values = [0,1,2,H,H+1,M-1,M]
    for kind in ['add','sub','mul']:
        for a in values:
            for b in values: yield kind,a,b,1,answer(kind,a,b,1)
    for kind in ['div_floor','div_ceil','floor','ceil']:
        for a in values:
            for b in values:
                for d in [0,1,2,H,H+1,M-2,M]: yield kind,a,b,d,answer(kind,a,b,d)
    for kind,values in [('to_u128',[L,-1,0,1,H]),('to_i128',[0,1,H,H+1,M])]:
        for a in values: yield kind,a,0,1,answer(kind,a,0,1)

def products():
    # Fixed seed arithmetic generates factors independently of the native wide code.
    cases=[(M,M,M,M,M,M),(M,M,M,M,M,M-1),(M,M,M-1,M,M,M),(0,M,M,1,1,1)]
    state=0x128384
    for _ in range(256):
        row=[]
        for j in range(6):
            state=(state*6364136223846793005+1442695040888963407)&M
            row.append(state)
        cases.append(tuple(row))
    return '# a b c d e f expected\n'+''.join('\t'.join(map(str,(*r,int(r[0]*r[1]*r[2]>=r[3]*r[4]*r[5]))))+'\n' for r in cases)
def scales():
    from fractions import Fraction
    rows=[]
    for power in range(39):
        factor=10**power;a=M//factor
        n=Fraction(a*(M-1)*factor,M)
        for rounding in ['floor','ceil']:
            q=n.numerator//n.denominator
            if rounding=='ceil' and n.denominator!=1:q+=1
            rows.append((power,rounding,a,M-1,M,q))
    return '# scale rounding a b d expected\n'+''.join('\t'.join(map(str,r))+'\n' for r in rows)
def render():
    return '# kind\ta\tb\td\texpected\n'+''.join('\t'.join(map(str,(*r[:4], 'none' if r[4] is None else r[4])))+'\n' for r in rows())
if __name__ == '__main__':
    p=argparse.ArgumentParser();p.add_argument('--generate',action='store_true');args=p.parse_args()
    text=render()
    if args.generate: DATA.parent.mkdir(parents=True,exist_ok=True);DATA.write_text(text)
    scales_path=DATA.with_name('scales.tsv')
    if args.generate:scales_path.write_text(scales())
    assert scales_path.read_text()==scales(),'stale independent scale corpus'
    product_path=DATA.with_name('products.tsv')
    if args.generate: product_path.write_text(products())
    assert product_path.read_text()==products(),'stale 384-bit comparison fixture'
    assert DATA.read_text()==text,'stale unsigned oracle fixture'
    print(f'{sum(1 for _ in rows())} independent U128 integer cases PASS')
