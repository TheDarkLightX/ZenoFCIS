"""D7 source specimen and bounded model controls; native/proof kills remain separate."""
from pathlib import Path
from scalar_arithmetic_controls import replace_once
from signed_arithmetic_reference import MIN, MAX

SOURCE=Path('crates/zeno-fcis-synthesis/src/finite/value/scalar.rs')
NAME='signed_negated_floor'
TARGET='signed_mul_div_floor'
WITNESS=(-1,1,2)
ANCHOR='wide::mul_div_ceil(x, y, den as u128)'
FAULT='wide::mul_div_floor(x, y, den as u128)'

def specimens(root: Path) -> dict:
    source=(root/SOURCE).read_text()
    return {NAME:(SOURCE,replace_once(source,ANCHOR,FAULT),'proof')}

def targets() -> dict:
    return {NAME:TARGET}

def magnitude_model(a:int,b:int,d:int,*,fault:bool=False)->int|None:
    # Follows the magnitude construction, independently of the Fraction oracle.
    if d<=0:return None
    p=abs(a)*abs(b);q,r=divmod(p,d)
    negative=(a<0)!=(b<0)
    if negative and r and not fault:q+=1
    if q>(1<<128)-1:return None
    q=-q if negative else q
    return q if MIN<=q<=MAX else None

def compare(actual,expected,enabled=True):
    return actual==expected if enabled else True

def grade(rows,oracle,*,comparison=True):
    baseline=all(compare(magnitude_model(*row),oracle(*row),comparison) for row in rows)
    admitted=len(rows)>0 and WITNESS in rows
    killed=any(not compare(magnitude_model(*row,fault=True),oracle(*row),comparison) for row in rows)
    return {'baseline':baseline,'admitted_model_mutant':admitted,'mutant_killed':killed,
            'passed':baseline and admitted and killed,'kind':'python-model-and-source-only'}
