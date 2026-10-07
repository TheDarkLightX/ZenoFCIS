#!/usr/bin/env python3
"""Development-only explicit fixture authoring. No optimizer or production codec."""
import json
from pathlib import Path
ROOT=Path(__file__).resolve().parent
MIN=-(1<<63); MAX=(1<<63)-1
B={'kind':'Bool'}
def D(lo,hi): return {'kind':'Int','min':str(lo),'max':str(hi)}
FULL=D(MIN,MAX)
def P(nodes,roots): return {'nodes':nodes,'roots':roots}
def inp(i): return ['Input',i]
def lit(i): return ['Int',str(i)]
def boo(b): return ['Bool',b]
class G:
    def __init__(self,n): self.nodes=[inp(i) for i in range(n)]
    def n(self,op,*args): self.nodes.append([op,*args]); return len(self.nodes)-1
    def OR(self,a,b): return self.n('Not',self.n('And',self.n('Not',a),self.n('Not',b)))
    def p(self,*roots): return P(self.nodes,list(roots))
cases=[]
def add(id,family,role,domains,outputs,p,q,why):
    profile='FunctionalBoolV1' if id.startswith('B') else 'CheckedI64V1-proposed'
    cases.append(dict(id=id,family=family,profile=profile,role=role,
      provenance={'kind':'handwritten-development-seed','source':'Astra design 2026-10-04','independence_group':family,'application_extraction':False},
      input_domains=domains,output_domains=outputs,
      abi={'inputs':['x'+str(i) for i in range(len(domains))],'outputs':['y'+str(i) for i in range(len(outputs))]},
      original=p,candidate=q,expected_relation='Different' if role=='incorrect-attractive' else 'Equivalent',
      rationale=why,witness=None,target_status='invalid-target' if role=='incorrect-attractive' else ('feasible-handwritten-baseline-not-optimal' if role=='positive' else 'control-not-an-optimality-claim'),
      eligibility={'structural':'expected-admitted','transform_profile':'proposed-not-qualified' if id.startswith('I') else 'designed-not-qualified','optimization_denominator':role!='incorrect-attractive'}))
# Boolean fixtures, all OR uses lower to Not/And.
add('B01','boolean-involution','positive',[B],[B],P([inp(0),['Not',0],['Not',1]],[2]),P([inp(0)],[0]),'Double negation; functional equality changes Step usage.')
a=G(2); r=a.OR(0,a.n('And',0,1)); add('B02','boolean-absorption','positive',[B,B],[B],a.p(r),P([inp(0)],[0]),'Absorption with an unused ABI input retained.')
a=G(3); r=a.OR(a.n('And',0,1),a.n('And',0,2)); b=G(3); s=b.n('And',0,b.OR(1,2)); add('B03','boolean-distributed-factor','positive',[B]*3,[B],a.p(r),b.p(s),'Shared factor must survive OR lowering; compare actual DAG counts.')
a=G(2); r=a.n('Select',0,a.n('And',0,1),1); add('B04','boolean-guard-implication','positive',[B]*2,[B],a.p(r),P([inp(1)],[0]),'Under a true guard its conjunction simplifies; both arms still eager.')
a=G(3); u=a.n('And',0,1); v=a.n('And',0,1); w=a.n('And',v,2); b=G(3); t=b.n('And',0,1); z=b.n('And',t,2); add('B05','boolean-shared-output','positive',[B]*3,[B]*3,a.p(u,w,u),b.p(t,z,t),'Duplicate stored conjunctions merge across three ordered outputs, including repeated root.')
a=G(2); na=a.n('Not',0); nb=a.n('Not',1); r=a.OR(a.n('And',0,nb),a.n('And',na,1)); b=G(2); t=b.n('Select',0,b.n('Not',1),1); add('B06','boolean-parity-circuit','positive',[B]*2,[B],a.p(r),b.p(t),'Two-input parity in sum-of-products versus mux form; no XOR opcode.')
a=G(3); u=a.OR(a.n('And',0,1),a.n('And',0,2)); r=a.OR(u,a.n('And',1,2)); b=G(3); t=b.n('Select',0,b.OR(1,2),b.n('And',1,2)); add('B07','boolean-majority-circuit','positive',[B]*3,[B],a.p(r),b.p(t),'Three-vote majority compares alternate circuit structures.')
a=G(3); guard=a.n('And',0,a.n('Not',1)); r=a.n('Select',guard,2,a.n('Bool',False)); b=G(3); s=b.n('And',b.n('And',0,b.n('Not',1)),2); add('B08','boolean-policy-deny-guard','positive',[B]*3,[B],a.p(r),b.p(s),'Synthetic permit-and-not-deny-and-ready policy motif; not an application extraction.')
a=G(6); u=a.n('And',0,1); v=a.n('And',2,3); w=a.n('And',4,5); r=a.n('And',a.n('And',u,v),w); z=a.n('Not',a.n('Not',r)); b=G(6); u=b.n('And',0,1);v=b.n('And',2,3);w=b.n('And',4,5);r=b.n('And',b.n('And',u,v),w); add('B09','boolean-six-input-coverage','positive',[B]*6,[B],a.p(z),b.p(r),'All 64 tuples include one isolated all-true result; catches missed terminal row.')
add('B10','boolean-zero-arity','positive',[],[B],P([boo(True),boo(False),['Not',1],['And',0,2]],[3]),P([boo(True)],[0]),'Zero inputs have exactly one empty tuple, not zero evaluations.')
a=G(2); r=a.n('Not',a.n('And',0,1)); b=G(2);t=b.n('And',b.n('Not',0),b.n('Not',1));add('B11','boolean-demorgan-defect','incorrect-attractive',[B]*2,[B],a.p(r),b.p(t),'Wrong De Morgan rewrite confuses disjunction and conjunction.')
add('B12','boolean-mux-polarity','incorrect-attractive',[B]*3,[B],P([inp(0),inp(1),inp(2),['Select',0,1,2]],[3]),P([inp(0),inp(1),inp(2),['Select',0,2,1]],[3]),'Swapping branches without negating condition.')
a=G(3); r=a.n('Select',0,1,2); b=G(3); s=b.OR(b.n('And',0,1),2);add('B13','boolean-policy-priority','incorrect-attractive',[B]*3,[B],a.p(r),b.p(s),'Synthetic priority choice cannot flatten to OR: fallback may override a selected false branch.')
add('B14','boolean-output-order','incorrect-attractive',[B]*2,[B]*3,P([inp(0),inp(1)],[0,1,0]),P([inp(0),inp(1)],[1,0,0]),'Ordered outputs differ even with identical root multiset; multiplicity retained.')
add('B15','boolean-projection-control','no-improvement-control',[B],[B],P([inp(0)],[0]),P([inp(0)],[0]),'Already one stored node; minimum node count follows from profile lower bound 1, bytes unclaimed.')
add('B16','boolean-shared-compact-control','no-improvement-control',[B]*2,[B]*3,P([inp(0),inp(1),['And',0,1]],[2,0,2]),P([inp(0),inp(1),['And',0,1]],[2,0,2]),'Compact shared circuit with duplicate output; no need to change every task.')
# Checked-i64 future profile fixtures; exact eager errors are observations.
add('I01','integer-additive-identity','positive',[D(-2,2)],[FULL],P([inp(0),lit(0),['Add',0,1]],[2]),P([inp(0)],[0]),'x+0 equals x functionally; fewer Step attempts alter budget refusal behavior.')
add('I02','integer-self-subtraction','positive',[D(MIN,MIN+2)],[FULL],P([inp(0),['Sub',0,0]],[1]),P([lit(0)],[0]),'x-x is safe even at MIN; unused input remains declared.')
add('I03','integer-cancellation','positive',[D(-2,2)],[FULL],P([inp(0),lit(1),['Add',0,1],['Sub',2,1]],[3]),P([inp(0)],[0]),'Cancellation valid on this original ordinary domain; contrast I08 without editing either domain.')
add('I04','integer-reassociation','positive',[D(-2,2),D(-2,2),D(-2,2)],[FULL],P([inp(0),inp(1),inp(2),['Add',0,1],['Add',3,2],lit(0),['Add',4,5]],[6]),P([inp(0),inp(1),inp(2),['Add',1,2],['Add',0,3]],[4]),'Safe bounded reassociation plus neutral-node removal; contrast endpoint I09.')
add('I05','integer-mixed-guard','positive',[B,D(0,2)],[D(0,2)],P([inp(0),inp(1),lit(0),['Lt',1,2],['And',0,3],['Select',4,2,1]],[5]),P([inp(1)],[0]),'Known nonnegative input makes guarded lower clamp redundant; no claim outside declared range.')
add('I06','integer-shared-offset','positive',[D(-2,2)],[FULL,FULL,FULL],P([inp(0),lit(1),['Add',0,1],['Add',0,1],['Sub',2,1]],[2,3,4]),P([inp(0),lit(1),['Add',0,1]],[2,2,0]),'Share offset across roots while preserving output multiplicity and order.')
add('I07','integer-eager-error-preserved','positive',[D(MAX-1,MAX)],[FULL],P([inp(0),lit(1),['Add',0,1],lit(0),['Add',2,3]],[4]),P([inp(0),lit(1),['Add',0,1]],[2]),'Remove trailing zero-add but retain overflow at MAX; equivalence includes Arithmetic rows.')
add('I08','integer-cancellation','incorrect-attractive',[D(MAX-1,MAX)],[FULL],P([inp(0),lit(1),['Add',0,1],['Sub',2,1]],[3]),P([inp(0)],[0]),'At MAX original Arithmetic; candidate returns MAX. No modular arithmetic.')
add('I09','integer-reassociation','incorrect-attractive',[D(MAX,MAX),D(1,1),D(-1,-1)],[FULL],P([inp(0),inp(1),inp(2),['Add',0,1],['Add',3,2]],[4]),P([inp(0),inp(1),inp(2),['Add',1,2],['Add',0,3]],[4]),'Intermediate overflow distinguishes (MAX+1)+(-1) from MAX+(1+(-1)).')
add('I10','integer-unused-overflow','incorrect-attractive',[D(MAX-1,MAX)],[FULL],P([inp(0),lit(1),['Add',0,1]],[0]),P([inp(0)],[0]),'Unused Add still executes; root-only evaluation is wrong.')
add('I11','integer-unselected-overflow','incorrect-attractive',[B,D(MAX,MAX)],[FULL],P([inp(0),inp(1),lit(1),['Add',1,2],['Select',0,3,1]],[4]),P([inp(1)],[0]),'Even false guard selecting x executes overflowing other arm first.')
add('I12','integer-min-negation','incorrect-attractive',[D(MIN,MIN+1)],[FULL],P([inp(0),lit(0),['Sub',1,0],['Sub',1,2]],[3]),P([inp(0)],[0]),'Double arithmetic negation traps at MIN; Boolean involution intuition is inapplicable.')
add('I13','integer-output-domain','incorrect-attractive',[D(0,2)],[D(0,1)],P([inp(0)],[0]),P([inp(0),lit(1),['Lt',1,0],['Select',2,1,0]],[3]),'Clamping changes OutputDomain at x=2 to successful 1; range tightening is not a repair permission.')
add('I14','integer-first-error-precedence','incorrect-attractive',[],[D(0,0)],P([lit(1),lit(MAX),lit(1),['Add',1,2]],[0]),P([lit(1)],[0]),'All nodes execute before output-domain validation: Arithmetic versus OutputDomain, not two interchangeable failures.')
add('I15','integer-compact-control','no-improvement-control',[D(-2,2),D(-2,2)],[FULL],P([inp(0),inp(1),['Add',0,1]],[2]),P([inp(0),inp(1),['Add',0,1]],[2]),'Small direct sum; no global minimum asserted.')
add('I16','integer-comparison-alternate','no-improvement-control',[D(-2,2),D(-2,2)],[B,B],P([inp(0),inp(1),['Eq',0,1],['Lt',0,1]],[2,3]),P([inp(0),inp(1),['Eq',1,0],['Lt',0,1]],[2,3]),'Commuted equality is an equivalent equal-node alternate form, not a strict improvement claim.')
# Conservative leakage groups union known same-semantics originals and paired families.
alias_groups={'B01':'boolean-projection-involution','B15':'boolean-projection-involution',
 'B02':'boolean-two-input-projection','B04':'boolean-two-input-projection',
 'B12':'boolean-mux-priority','B13':'boolean-mux-priority',
 'I01':'integer-cancellation-eager-removal','I03':'integer-cancellation-eager-removal',
 'I08':'integer-cancellation-eager-removal','I10':'integer-cancellation-eager-removal'}
for case in cases:
    case['provenance']['independence_group']=alias_groups.get(case['id'],case['family'])
assert len(cases)==32
(ROOT/'cases.json').write_text(json.dumps({'schema_version':'zenofcis-benchmark-seeds-v1','status':'public-development-seeds','enumeration':'ordered-product-last-input-fastest-v1','cases':cases},indent=2)+'\n')
print('Wrote 32 explicit seed pairs (16 Boolean, 16 checked-i64); witnesses pending reference check.')
