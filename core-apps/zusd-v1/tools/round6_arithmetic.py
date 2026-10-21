#!/usr/bin/env python3
"""Independent exact integers for ordered floors and three-factor fees."""
import argparse
from itertools import product
from pathlib import Path
import random

M=2**128-1
Q=10**18

def vectors():
    rng=random.Random(6006)
    values=[0,1,2**64-1,2**64,Q,M-1,M]
    pairs=[tuple(rng.choice(values) for _ in range(4)) for _ in range(128)]
    pairs += [tuple(rng.getrandbits(128) for _ in range(4)) for _ in range(128)]
    rows=[]
    for a,b,d,e in pairs:
        value=a*b//d//e if d and e else None
        expected=str(value) if value is not None and value<=M else 'Arithmetic'
        rows.append(f'divided\t{a}\t{b}\t{d}\t{e}\t{expected}\n')
    for gross,price,debt in product([0,1,2**64,Q,M-1,M],repeat=3):
        if gross==0:expected='Reject1045'
        elif not price or not debt:expected='Domain'
        else:
            fraction=gross*price*10**10//debt
            base=min(fraction//2,Q)
            fee=gross*min(Q//200+base,Q)//Q
            expected=('Arithmetic' if fraction>M else 'Reject1050' if not base else
                      'Reject1051' if fee>=gross else str(fee))
        rows.append(f'redemption\t{gross}\t{price}\t{debt}\t0\t{expected}\n')
    return ''.join(rows)

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--check',action='store_true');args=parser.parse_args()
    path=Path(__file__).resolve().parents[1]/'tests/test-data/round6_arithmetic.tsv'
    content=vectors()
    if args.check:assert path.read_text()==content,'independent expectation drift'
    else:
        if path.exists():raise SystemExit('refuse overwriting an established expectation')
        path.write_text(content)
    print('PASS 472 independent ordered-floor / three-factor boundary vectors')

if __name__=='__main__':main()
