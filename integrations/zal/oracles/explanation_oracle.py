"""Decision-explanation correspondence and ownership oracle on exact inputs."""
import copy,hashlib,itertools,json,sys
from pathlib import Path
src=Path(__file__).resolve().parents[1] / 'behavior.py'
sys.path.insert(0,str(src.parent))
from snapshot_loader import load_behavior
behavior,before=load_behavior(src)
parse,explain,Refusal=behavior.parse,behavior.explain,behavior.Refusal
vals=list(itertools.product((False,True),repeat=2))
def dnf(mask):
 terms=[]
 for i,(p,q) in enumerate(vals):
  if mask&(1<<i):
   terms.append('('+('p' if p else '!p')+' & '+('q' if q else '!q')+')')
 return ' | '.join(terms) or 'false'
def model(a,b):
 return parse(f'''zal 1;
machine explanation;
states aa, bb;
events other, tick;
context p, q;
initial aa;
step atomic; frame control_only;
default reject no_match unchanged;
rule lhs: aa + tick [{dnf(a)}] -> accept bb;
rule rhs: aa + tick [{dnf(b)}] -> reject denied unchanged;
''')
counts={'guard_pairs':0,'explanations':0,'overlap_refusals':0,'invalid_refusals':0,'nonmutation_checks':0}
for a,b in itertools.product(range(16),repeat=2):
 m=model(a,b); original=m.data();counts['guard_pairs']+=1
 for s,e,(p,q) in itertools.product(('aa','bb'),('other','tick'),vals):
  ctx={'p':p,'q':q}; untouched=copy.deepcopy(ctx)
  index=vals.index((p,q))
  enabled=[] if s!='aa' or e!='tick' else [n for n,mask in [('lhs',a),('rhs',b)] if mask&(1<<index)]
  if len(enabled)>1:
   try: explain(m,s,e,ctx,'oracle')
   except Refusal: counts['overlap_refusals']+=1
   else: raise AssertionError('Expected overlap refusal')
  else:
   out=explain(m,s,e,ctx,'oracle');counts['explanations']+=1
   chosen=enabled[0] if enabled else 'default'
   expected={'class':'Accept' if chosen=='lhs' else 'Reject',
             'state':'bb' if chosen=='lhs' else s,
             'reason':None if chosen=='lhs' else 'denied' if chosen=='rhs' else 'no_match',
             'rule':chosen}
   assert out['outcome']==expected
   assert out['revision']==m.revision and out['checker']=='oracle'
   assert out['input']=={'state':s,'event':e,'context':ctx}
   assert out['used_default']==(chosen=='default')
   assert out['frame']==('only control state may change' if chosen=='lhs' else 'control unchanged')
   for rule,mask in zip(out['rules'],(a,b)):
    assert rule['state_matches']==(s=='aa') and rule['event_matches']==(e=='tick')
    assert rule['guard']['value']==bool(mask&(1<<index))
    assert rule['enabled']==(rule['id'] in enabled)
   # Returned input must be an owned copy: modifying it cannot change caller data.
   out['input']['context']['p']=not p
  assert m.data()==original and ctx==untouched
  counts['nonmutation_checks']+=1
m=model(3,12)
for s,e,c in [('unknown','tick',{'p':False,'q':False}),('aa','unknown',{'p':False,'q':False}),
              ('aa','tick',{}),('aa','tick',{'p':False}),
              ('aa','tick',{'p':False,'q':False,'extra':True}),
              ('aa','tick',{'p':None,'q':False}),('aa','tick',{'p':0,'q':False}),
              ('aa','tick',[]),('aa','tick',None)]:
 try: explain(m,s,e,c)
 except Refusal: counts['invalid_refusals']+=1
 else: raise AssertionError('Expected invalid-input refusal')
after=hashlib.sha256(src.read_bytes()).hexdigest();assert before==after,'Source changed during test'
print(json.dumps({'status':'passed','behavior_sha256':after,'counts':counts,
 'scope':'All 256 two-fact guard pairs at both states, matching/nonmatching declared events and all contexts; independent expected decisions and guard masks. Output ownership and missing/unknown/non-Boolean inputs checked.'},indent=2))
