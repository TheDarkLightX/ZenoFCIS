#!/usr/bin/env python3
"""Independent bigint boundary vectors for uint256 storage and 384-bit division."""
import argparse
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
root=Path(__file__).resolve().parents[1]/'tests/test-data'
M=2**128-1;U=2**256-1;W=2**384-1;Q=10**18
values=(0,1,M,M+1,2**255-1,2**255,U-1,U)
def limbs(x):return f'{x>>128},{x&M}'
adds=[f'{limbs(a)}\t{limbs(b)}\t'+(limbs(a+b) if a+b<=U else 'refuse')+'\n' for a in values for b in values]
products=[f'{limbs(a)}\t{n}\t{d}\t'+(str(a*n//d) if d and a*n//d<=M else 'refuse')+'\n' for a in values for n in (0,1,Q,M) for d in (0,1,Q,M)]
nums=tuple(sorted(set(values+(2**256,2**383-1,2**383,W))))
divs=(0,1,M,M+1,U,2**256,2**383,W)
quotients=[f'{n:096x}\t{d:096x}\t'+(f'{n//d:096x}\t{n%d:096x}' if d else 'refuse')+'\n' for n in nums for d in divs]
for name,rows in [('accumulator_add.tsv',adds),('accumulator_product.tsv',products),('wide_division.tsv',quotients)]:
 text=''.join(rows);path=root/name
 if args.check:assert path.read_text()==text,name
 else:path.write_text(text)
 print(f'PASS {name}: {len(rows)} independent boundary vectors')
