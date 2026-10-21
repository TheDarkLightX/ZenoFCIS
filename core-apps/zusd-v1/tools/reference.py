#!/usr/bin/env python3
"""Independent exact-integer data; check files without accepting native output as truth."""
from itertools import product
from pathlib import Path
import argparse

M=2**128-1
Q=10**18
ROOT=Path(__file__).resolve().parents[1]
def corpus():
    rows=[]
    values=(0,1,10**18,2**64-1,2**127-1,2**127,M-1,M)
    for a,b,d in product(values,repeat=3):
        q=None if not d else a*b//d
        rows.append(f'{a}\t{b}\t{d}\t{q if q is not None and q<=M else "refuse"}\n')
    return ''.join(rows)
def decpow(n):
    n=min(n,525600000);x=999037758833783000;y=Q
    if n==0:return Q
    def rounded(a,b):return (a*b+Q//2)//Q
    while n>1:
        if n%2:y=rounded(x,y)
        x=rounded(x,x);n//=2
    return rounded(x,y)
def fee_corpus():
    rows=[]
    for base,principal,n in product((0,Q//4,Q//2,Q),(1,500*Q,2**127,M),(0,1,59,60,719,720,721,525600000,525600001)):
        decayed=base*decpow(n)//Q;rate=min(5*10**15+decayed,Q//20);fee=rate*principal//Q
        rows.append(f'{base}\t{principal}\t{n}\t{decayed}\t{rate}\t{fee}\n')
    return ''.join(rows)
def liquidation_corpus():
    rows=[]
    for d in (2,10,100,550*Q):
        for c,sysc,available in product((0,d,d*11//10-1,d*11//10,2*d),(12*d,20*d),(0,d//2,d,2*d)):
            sysd=10*d;p=10**8;icr=c*Q//d;tcr=sysc*Q//sysd;recovery=tcr<15*Q//10
            if (not recovery and icr>=11*Q//10) or (recovery and icr>=11*Q//10 and (icr>=tcr or available<d)):
                expected='reject'
            else:
                if recovery and icr>=11*Q//10:
                    cap=d*11//10;keeper=cap//200;offset=d;sp=cap-keeper;rd=rc=0;surplus=c-cap
                else:
                    keeper=c//200;offset=0 if recovery and icr<=Q else min(d,available)
                    sp=(c-keeper)*offset//d;rd=d-offset;rc=c-keeper-sp;surplus=0
                expected=','.join(map(str,(offset,rd,sp,rc,keeper,surplus)))
            rows.append('\t'.join(map(str,(c,d,p,available,sysc,sysd,expected)))+'\n')
    return ''.join(rows)
def offset_corpus():
    rows=[]
    cap=35_000_000*Q
    for total,p in product((0,1,2,Q,1000*Q),(Q,Q//2,10**9)):
        for debt,coll,issuance in product((0,1,total//2,total),(0,1,Q),(0,1,Q)):
            args=(total,p,debt,coll,issuance)
            if total==0 or debt==0:
                out=(total,p,0,0,0,0,0,0,cap,0,0,0,0,0,0)
            elif debt>total:
                out=None
            else:
                g,eg=divmod(issuance*Q,total);a,es=divmod(coll*Q,total)
                loss=Q if debt==total else debt*Q//total+1
                el=0 if debt==total else loss*total-debt*Q
                factor=Q-loss
                epoch=scale=0
                newp=p*factor//Q
                if factor==0:epoch=1;newp=Q
                elif newp<10**9:scale=1;newp=p*factor*10**9//Q
                out=(total-debt,newp,epoch,scale,es,el,eg,coll,cap-issuance,issuance,issuance,0,0,a*p,g*p)
                if newp==0 or any(x<0 or x>(2**256-1 if j in (4,5,6,13,14) else M) for j,x in enumerate(out)):out=None
            rows.append('\t'.join(map(str,args))+'\t'+('refuse' if out is None else ','.join(f'{x>>128}:{x&M}' if j in (4,5,6,13,14) else str(x) for j,x in enumerate(out)))+'\n')
    return ''.join(rows)
def main():
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
    data={'wide.tsv':corpus(),'fees.tsv':fee_corpus(),'liquidation.tsv':liquidation_corpus(),'offset.tsv':offset_corpus()}
    for name,content in data.items():
        path=ROOT/'tests/test-data'/name
        if args.check:
            if path.read_text()!=content:raise SystemExit(f'independent corpus drift: {name}')
        else:
            path.parent.mkdir(parents=True,exist_ok=True);path.write_text(content)
    print('PASS independent integer vectors: '+', '.join(f'{name}={len(text.splitlines())}' for name,text in data.items()))
if __name__=='__main__':main()
