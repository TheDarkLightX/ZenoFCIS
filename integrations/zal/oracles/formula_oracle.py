"""Independent bit-mask oracle for parser and Boolean evaluator composition.

Does not call production evaluator to compute expected truth functions.
Ground truth masks are built directly from coordinate truth values.
"""
import hashlib,itertools,json,sys
from pathlib import Path
source=Path(__file__).resolve().parents[1] / 'behavior.py'
sys.path.insert(0,str(source.parent))
from snapshot_loader import load_behavior
behavior,before=load_behavior(source)
parse_formula=behavior.parse_formula
vals=list(itertools.product(('aa','bb'),(False,True),(False,True)))
full=(1<<len(vals))-1
atoms=[]
for text,truths in [('p',[p for s,p,q in vals]),('q',[q for s,p,q in vals]),
                    ('state == aa',[s=='aa' for s,p,q in vals]),
                    ('state == bb',[s=='bb' for s,p,q in vals]),
                    ('true',[True]*8),('false',[False]*8)]:
 atoms.append((text,sum((1<<i) for i,b in enumerate(truths) if b)))
small=atoms+[(f'!({s})',full^m) for s,m in atoms]
small += [(f'({a}) {op} ({b})',am&bm if op=='&' else am|bm)
          for a,am in atoms for b,bm in atoms for op in ('&','|')]
expressions=small+[(f'!({s})',full^m) for s,m in small]
expressions += [(f'({a}) {op} ({b})',am&bm if op=='&' else am|bm)
                for a,am in small for b,bm in small for op in ('&','|')]
p,q=atoms[0][1],atoms[1][1]
expressions += [('p | q & !p',p|(q&(full^p))),
                ('!p & q | p',(full^p)&q|p),
                ('!p | !q & p',(full^p)|((full^q)&p)),
                ('!!p',p),('!(p | q)',full^(p|q)),('!(p & q)',full^(p&q))]
checked=roundtrips=0
for text,mask in expressions:
 f=parse_formula(text,('p','q'),('aa','bb'))
 for english in (False,True):
  r=parse_formula(f.render(english),('p','q'),('aa','bb'),english=english)
  assert r==f
  roundtrips+=1
 for i,(s,p,q) in enumerate(vals):
  assert f.value(s,{'p':p,'q':q}) == bool(mask&(1<<i)), (text,s,p,q)
  checked+=1
after=hashlib.sha256(source.read_bytes()).hexdigest()
assert before==after,'Source changed during oracle run'
print(json.dumps({'status':'passed','behavior_sha256':after,
 'formula_cases':len(expressions),'valuation_checks':checked,'roundtrips':roundtrips,
 'scope':'All binary compositions of 84 small formulas over p,q,state aa/bb,true,false, negations, plus precedence fixtures. Expected truth masks use independent bit operations.',
 'not_established':'Not all admitted syntax strings or universal parser/evaluator correctness.'},indent=2))
