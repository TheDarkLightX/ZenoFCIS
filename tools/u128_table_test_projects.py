#!/usr/bin/env python3
"""U128 declarations and exact decimal atoms for bounded table qualification."""
import json
import re
import sys
from pathlib import Path
from table_test_projects import generic
from ordered_test_projects import ordered
MAX=2**128-1

def unsigned_generic(capacity,kind="map"):
    source,rules=generic(kind,capacity)
    source=source.replace(f'tables_{kind}_{capacity}',f'tables_unsigned_{kind}_{capacity}')
    source=re.sub(r'type 200 int Key in [^;]+;',f'type 200 u128 Key in 0..={MAX};',source)
    source=re.sub(r'type 201 int EntryValue in [^;]+;',f'type 201 u128 EntryValue in 0..={MAX};',source)
    def rewrite(x):
        if isinstance(x,dict):return {k:rewrite(v) for k,v in x.items()}
        if not isinstance(x,list):return x
        if len(x)==3 and x[0]=='literal' and x[1] in (200,201):return [x[0],x[1],str(x[2])]
        if x and x[0] in ('add','sub') and x[1][0]=='len':
            return [x[0],rewrite(x[1]),['literal',202,x[2][2]]]
        return [('sum_u128' if v=='sum_i128' else v) if isinstance(v,str) else rewrite(v) for v in x]
    return source,rewrite(rules)

def unsigned_ordered(capacity):
    source,rules=ordered(2,capacity,3)
    source=source.replace(f'ordered_2_{capacity}',f'ordered_unsigned_{capacity}')
    source=source.replace('type 201 int Priority in 1..=2;',f'type 201 u128 Priority in 1..={MAX};')
    return source,rules

def write(root):
    root=Path(root)
    for capacity in range(4):
        for name,make in [('map',unsigned_generic),('set',lambda c:unsigned_generic(c,'set')),('ordered',unsigned_ordered)]:
            source,rules=make(capacity);folder=root/f'{name}-{capacity}'
            (folder/'v2').mkdir(parents=True,exist_ok=True)
            (folder/'project.zeno').write_text(source)
            (folder/'v2/policy.json').write_text(json.dumps(rules,sort_keys=True,indent=2)+'\n')
if __name__=='__main__':write(sys.argv[1])
