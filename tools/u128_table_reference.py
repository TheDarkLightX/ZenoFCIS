#!/usr/bin/env python3
"""Independent Python integer oracle for unsigned law sums and NICR quotients."""
import argparse
import itertools
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
DEST=ROOT/'crates/zeno-fcis-synthesis/tests/test-data/u128-tables'
M=2**128-1

def corpus():
    sums=[]
    for n in range(5):
        for xs in itertools.product((0,1,2**127,M),repeat=n):
            total=sum(xs)
            sums.append(f"{','.join(map(str,xs)) or '-'}\t{total%(2**128)}\t{total//(2**128)}\n")
    nicrs=[]
    for c,d in itertools.product((0,1,2**127-1,2**127,M),repeat=2):
        q=None if d==0 else c*10**20//d
        nicrs.append(f"{c}\t{d}\t{q if q is not None and q<=M else 'refuse'}\n")
    return {'sums.tsv':''.join(sums),'nicr.tsv':''.join(nicrs)}
def main():
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');a=p.parse_args()
    for name,content in corpus().items():
        path=DEST/name
        if a.check:
            if path.read_text()!=content:raise SystemExit(f'oracle corpus changed: {name}')
        else:
            path.parent.mkdir(parents=True,exist_ok=True);path.write_text(content)
    print('341 exact sum vectors; 25 NICR success/refusal vectors')
if __name__=='__main__':main()
