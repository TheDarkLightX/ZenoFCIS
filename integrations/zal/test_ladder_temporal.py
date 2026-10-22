"""Temporal ladder contract checks. Oracles and example journeys are test-only."""
import copy
from functools import lru_cache
from itertools import product
import json
import os
from pathlib import Path
import random
import re
import subprocess
import time
import unittest
from unittest.mock import patch

import ladder as l
from temporal_lowering import lower, _native

EXAMPLES=Path(__file__).with_name('examples')/'temporal'
BOUND=5
SEED=20261017
SAMPLES=20000
P=('call','P',False,())
Q=('call','Q',False,())
UNARY=('not','always','eventually','next','within0','within1','within2')
BINARY=('and','or','=>','<=>','until')


def small_data():
    data=json.loads((EXAMPLES/'vault_drain.json').read_text())
    data.update(id='temporal_signature',definitions=[],readings=[],fixtures=[],formula='forall sigma in Traces: P()')
    data['symbols']={name:{'kind':'predicate','arity':0,'args':[],'clause':'the event is '+name,
        'observation':'event','definition':'Boolean event observation '+name,'version':'1',
        'native':[{'args':[],'before':'pre.100.'+str(110+i)}]} for i,name in enumerate(('P','Q'))}
    return data


def wrap(body):return ('forall','sigma','Traces',body)


def unary(op,body):
    return ('within',int(op[-1]),body) if op.startswith('within') else (op,body)


def combined_data():
    data=small_data()
    data['domains']['D']={'member':'member','plural':'members','noun':'integer objects','integer':True,'values':[0,1]}
    data['symbols'].update({
        'S':{'kind':'predicate','arity':1,'args':['D'],'clause':'{0} is selected','noun':'selected member','observation':'event','definition':'selection of this rigid object at the active event','version':'1','native':[]},
        'Rel':{'kind':'predicate','arity':2,'args':['D','D'],'clause':'{0} relates to {1}','observation':'event','definition':'relation at the active event','version':'1','native':[]},
        'f':{'kind':'function','arity':1,'args':['D'],'noun':'level','state':True,'observation':'event','definition':'object level before or after the active event','version':'1','native':[]}})
    data['definitions']=[{'name':'k','params':[{'name':'z','domain':'D'}],'body':'f(z)+1','noun':'the shifted level','version':'1'}]
    return data


@lru_cache(maxsize=None)
def combined_trees(size,kind,depth=0,in_time=False):
    result=[]
    if size==1:
        result.extend((P,Q) if kind=='bool' else [('int',0),('int',1),*(('var',f'v{i}') for i in range(depth))])
    if size>=2:
        if kind=='int':
            for name,primed in product(('f','k'),(False,True)):
                result.extend(('call',name,primed,(a,)) for a in combined_trees(size-1,'int',depth,in_time))
            if not in_time:result.extend(('count',f'v{depth}','D',a) for a in combined_trees(size-1,'bool',depth+1,False))
        else:
            result.extend(('call','S',False,(a,)) for a in combined_trees(size-1,'int',depth,in_time))
            result.extend(('not',a) for a in combined_trees(size-1,'bool',depth,in_time))
            for op in UNARY[1:]:result.extend(unary(op,a) for a in combined_trees(size-1,'bool',depth,True))
            if not in_time:
                for op in ('forall','exists','exists1'):
                    result.extend((op,f'v{depth}','D',a) for a in combined_trees(size-1,'bool',depth+1,False))
        for left in range(1,size-1):
            right=size-1-left
            ints=product(combined_trees(left,'int',depth,in_time),combined_trees(right,'int',depth,in_time))
            for a,b in ints:
                if kind=='int':result.extend((op,a,b) for op in l.ARITH)
                else:
                    result.append(('call','Rel',False,(a,b)))
                    result.extend((op,a,b) for op in l.COMPARE)
            if kind=='bool':
                result.extend((op,a,b) for a,b in product(combined_trees(left,'bool',depth,in_time),combined_trees(right,'bool',depth,in_time)) for op in l.LOGIC)
                result.extend(('until',a,b) for a,b in product(combined_trees(left,'bool',depth,True),combined_trees(right,'bool',depth,True)))
    return tuple(result)


def combined_random(rng,kind,depth,env=(),in_time=False):
    if depth==1:
        return rng.choice((P,Q)) if kind=='bool' else rng.choice([('int',0),('int',1),*(('var',v) for v in env)])
    if kind=='int':
        op=rng.choice(('leaf','call',*l.ARITH,*(('count',) if not in_time else ())))
        if op=='leaf':return combined_random(rng,kind,1,env,in_time)
        if op=='call':return ('call',rng.choice(('f','k')),rng.choice((False,True)),(combined_random(rng,'int',depth-1,env,in_time),))
        if op=='count':
            v=f'v{len(env)}';return ('count',v,'D',combined_random(rng,'bool',depth-1,(*env,v),False))
        return (op,combined_random(rng,'int',depth-1,env,in_time),combined_random(rng,'int',depth-1,env,in_time))
    op=rng.choice(('leaf','S','Rel',*UNARY,*BINARY,*l.COMPARE,*(('forall','exists','exists1') if not in_time else ())))
    if op=='leaf':return rng.choice((P,Q))
    if op=='S':return ('call','S',False,(combined_random(rng,'int',depth-1,env,in_time),))
    if op=='Rel':return ('call','Rel',False,tuple(combined_random(rng,'int',depth-1,env,in_time) for _ in range(2)))
    if op in ('forall','exists','exists1'):
        v=f'v{len(env)}';return (op,v,'D',combined_random(rng,'bool',depth-1,(*env,v),False))
    if op in UNARY:return unary(op,combined_random(rng,'bool',depth-1,env,in_time or op!='not'))
    childkind='int' if op in l.COMPARE else 'bool'
    return (op,combined_random(rng,childkind,depth-1,env,in_time or op=='until'),combined_random(rng,childkind,depth-1,env,in_time or op=='until'))


def expected_at(tree,word,position):
    """Independent Boolean-word truth definition; never used by product code."""
    op=tree[0]
    if op=='call':return word[position][0 if tree[1]=='P' else 1]
    if op=='not':return not expected_at(tree[1],word,position)
    if op=='next':return position+1<len(word) and expected_at(tree[1],word,position+1)
    if op=='always':return all(expected_at(tree[1],word,j) for j in range(position,len(word)))
    if op=='eventually':return any(expected_at(tree[1],word,j) for j in range(position,len(word)))
    if op=='within':return any(expected_at(tree[2],word,j) for j in range(position,min(len(word),position+tree[1]+1)))
    if op=='until':
        return any(expected_at(tree[2],word,j) and all(expected_at(tree[1],word,h) for h in range(position,j)) for j in range(position,len(word)))
    a=expected_at(tree[1],word,position);b=expected_at(tree[2],word,position)
    return a and b if op=='and' else a or b if op=='or' else not a or b if op=='=>' else a==b


def native(cases):
    target=Path(os.environ.get('CARGO_TARGET_DIR','target'))/'debug/examples/ladder_temporal_parity'
    executable=Path(os.environ.get('ZAL_TEMPORAL_CLI',str(target)))
    if not executable.is_file():raise AssertionError('Build the diagnostic harness with check_ladder_temporal.py native-build')
    result=subprocess.run([str(executable.resolve())],input=json.dumps({'cases':cases}),text=True,capture_output=True,timeout=120)
    if result.returncode:raise AssertionError(result.stderr)
    output=json.loads(result.stdout)
    if output['authority']!='none':raise AssertionError('diagnostic authority changed')
    return output['results']


def case(tree,ctx,words,paths=('pre.100.110','pre.100.111'),positions=False,constants=()):
    obligation=lower(tree,ctx,constants)
    if obligation.source is None:raise AssertionError(obligation.obligations)
    return {'source':obligation.source,'words':words,'paths':paths,'positions':positions}


def alarm_journey():
    """Illustrative policy fixture, no shipped controller or decision authority."""
    remaining=2;result=[]
    for index,alarm in enumerate((True,True,True,True)):
        before=remaining
        remaining=max(remaining-1,0)
        result.append({'alarm':alarm,'before':before,'after':remaining,'enabled':remaining==0,'request':index==0,'complete':index==3 and remaining==0})
    return result


class TemporalLadderChecks(unittest.TestCase):
    def setUp(self):self.ctx=l.Context.from_data(small_data())

    def test_expanded_native_limits(self):
        def context(size):
            data=small_data()
            data['domains']['D']={'member':'object','plural':'objects','integer':True,'values':list(range(size))}
            return l.Context.from_data(data)
        for size in (250,254,255,256):
            ctx=context(size)
            for quantifier in ('forall','exists'):
                tree=l.parse(f'forall sigma in Traces: {quantifier} x in D: P()','typed',ctx)
                result=native([case(tree,ctx,[[[1,0]],[[0,0]]])])[0]
                self.assertEqual(result['finite_export'],'Ok(())')
                self.assertEqual(result['verdicts'],['Satisfied','Counterexample { step: 0 }'])
        with self.assertRaisesRegex(l.Refusal,'lowering:finite-object-domain'):
            lower(l.parse('forall sigma in Traces: forall x in D: P()','typed',context(257)),context(257))
        # Each equivalence expands to 21 native nodes, plus one conjunction
        # per member except the last: 22*n-1. 186 gives 4091; 187 gives 4113.
        ctx=context(186);tree=l.parse('forall sigma in Traces: forall x in D: P() <=> Q()','typed',ctx)
        result=native([case(tree,ctx,[[[1,1]],[[1,0]]])])[0]
        self.assertEqual(result['finite_export'],'Ok(())')
        self.assertEqual(result['verdicts'],['Satisfied','Counterexample { step: 0 }'])
        ctx=context(187);tree=l.parse('forall sigma in Traces: forall x in D: P() <=> Q()','typed',ctx)
        with self.assertRaisesRegex(l.Refusal,'lowering:native-shape-budget'):lower(tree,ctx)
        node=_native('true','bool')
        for _ in range(255):node=_native('not ('+node.text+')','not',(node,))
        with self.assertRaisesRegex(l.Refusal,'lowering:native-depth-budget'):_native('not ('+node.text+')','not',(node,))
        data=small_data();data['trace']['horizon']=256;ctx=l.Context.from_data(data)
        with self.assertRaisesRegex(l.Refusal,'lowering:native-export-operations-budget'):lower(wrap(('always',('always',P))),ctx)
        # Nested range expansion fits operation preflight but exceeds the
        # conservative intermediate-string envelope before native export.
        data['trace']['horizon']=32;ctx=l.Context.from_data(data)
        with self.assertRaisesRegex(l.Refusal,'lowering:native-export-bytes-budget'):lower(wrap(('always',('eventually',P))),ctx)
        self.assertEqual(lower(wrap(P),self.ctx),lower(wrap(P),self.ctx))

    def test_balanced_expansion_preserves_member_order(self):
        data=small_data();data['domains']['D']={'member':'object','plural':'objects','integer':True,'values':[2,0,1]}
        data['symbols']['S']={'kind':'predicate','arity':1,'args':['D'],'clause':'{0} is selected','observation':'event','definition':'selected object','version':'1','native':[{'args':[i],'before':f'pre.100.{200+i}'} for i in range(3)]}
        data['native_schema']['fields'] += [{'id':200+i,'kind':'bool'} for i in range(3)]
        ctx=l.Context.from_data(data)
        for quantifier in ('forall','exists'):
            tree=l.parse(f'forall sigma in Traces: {quantifier} x in D: S(x)','typed',ctx)
            self.assertEqual(re.findall(r'pre\.100\.20[0-2]',lower(tree,ctx).source),['pre.100.202','pre.100.200','pre.100.201'])
            result=native([case(tree,ctx,[[[1,1,1]],[[0,0,0]],[[0,1,0]]],tuple(f'pre.100.{200+i}' for i in range(3)))])[0]
            self.assertEqual(result['verdicts'],['Satisfied','Counterexample { step: 0 }', 'Satisfied' if quantifier=='exists' else 'Counterexample { step: 0 }'])

    def test_registered_readings_match_mode_and_horizon(self):
        data=l.plain_data(l.load(EXAMPLES/'alarm_withdrawal.json').data)
        texts={('finite',4):('An alarm may delay a withdrawal, but the withdrawal still completes before this trace ends.','Does every delayed withdrawal still complete before this trace ends?'),
               ('unbounded',None):('Provided A, an alarm may delay a withdrawal but never freeze it.','Is it true that provided A, an alarm may delay a withdrawal but never freeze it?')}
        rendered={}
        for (mode,horizon),expected in texts.items():
            data['trace'].update(mode=mode,horizon=horizon);ctx=l.Context.from_data(data)
            tree=l.parse(data['formula'],'typed',ctx)
            for rung,want in zip(('plain','question'),expected):
                text=l.render(tree,rung,ctx);rendered[mode,rung]=text
                self.assertEqual(text.splitlines()[0],want)
                self.assertEqual(l.normal(l.parse(text,rung,ctx),ctx),l.normal(tree,ctx))
            other='finite' if mode=='unbounded' else 'unbounded'
            if (other,'plain') in rendered:
                for rung in ('plain','question'):
                    with self.assertRaises(l.Refusal):l.parse(rendered[other,rung],rung,ctx)
        data['trace'].update(mode='finite',horizon=3);ctx=l.Context.from_data(data)
        tree=l.parse(data['formula'],'typed',ctx)
        for rung in ('plain','question'):
            self.assertNotIn('delay a withdrawal',l.render(tree,rung,ctx))
            with self.assertRaises(l.Refusal):l.parse(rendered['finite',rung],rung,ctx)
        data['trace'].update(mode='unbounded',horizon=None);ctx=l.Context.from_data(data)
        for rung in ('plain','question'):
            with self.assertRaises(l.Refusal):l.parse(rendered['finite',rung],rung,ctx)
        for change,reason in (({'mode':'other'},'reading-mode'),({'mode':'unbounded','horizon':4},'reading-horizon'),({'mode':'finite','horizon':True},'reading-horizon')):
            invalid=copy.deepcopy(data);invalid['readings'][0].update(change)
            with self.assertRaisesRegex(l.Refusal,reason):l.Context.from_data(invalid)

    def test_explicit_profile_matches_theory(self):
        data=json.loads((EXAMPLES.parent/'ladders/dex_k.json').read_text())
        original=l.Context.from_data(data);tree=l.parse(data['formula'],'typed',original)
        expected=l.ladder_text(tree,original)
        data['profile']=l.PROFILE
        self.assertEqual(l.ladder_text(tree,l.Context.from_data(data)),expected)
        for profile in (l.TEMPORAL_PROFILE,'unknown',None,True):
            data['profile']=profile
            with self.assertRaisesRegex(l.Refusal,'context:profile-theory-mismatch'):l.Context.from_data(data)
        data=small_data();data['profile']=l.PROFILE
        with self.assertRaisesRegex(l.Refusal,'context:profile-theory-mismatch'):l.Context.from_data(data)

    def test_native_refusals_are_results(self):
        good=case(wrap(P),self.ctx,[[[1,0]]])
        invalid=[]
        for source in ('zeno 1;','zeno 1; project 1 p; claim 2 c cvc5 finite 4 = '+ 'not ('*255+'true'+')'*255+';'):
            invalid.append({**good,'source':source})
        invalid.extend(({**good,'paths':['bad','post.0']},{**good,'words':[[[1]]]}, {**good,'words':[[['bad',0]]]}))
        results=native([*invalid,good])
        for result in results[:-1]:self.assertTrue(result['refusal'].startswith('replay:'),result)
        self.assertIn('ResourceLimit',results[1]['refusal'])
        self.assertEqual(results[-1]['verdicts'],['Satisfied'])
        executable=Path(os.environ.get('ZAL_TEMPORAL_CLI',str(Path(os.environ.get('CARGO_TARGET_DIR','target'))/'debug/examples/ladder_temporal_parity'))).resolve()
        for text,reason in (('{','replay:json'),('{}','replay:cases')):
            result=subprocess.run([str(executable)],input=text,text=True,capture_output=True,timeout=10)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertEqual(result.stderr,'')
            self.assertTrue(json.loads(result.stdout)['refusal'].startswith(reason))

    def roundtrip(self,body,collisions=None,ctx=None,quantifier='forall'):
        ctx=self.ctx if ctx is None else ctx
        tree=(quantifier,'sigma','Traces',body);want=l.normal(tree,ctx)
        for rung in l.RUNGS:
            text=l.render(tree,rung,ctx)
            try:got=l.parse(text,rung,ctx)
            except l.Refusal as exc:self.fail(f'{rung}: {body!r}\n{text}\n{exc}')
            self.assertEqual(l.normal(got,ctx),want,(rung,text,tree,got))
            self.assertEqual(l.render(got,rung,ctx),text)
            if collisions is not None:
                old=collisions[rung].setdefault(text,want)
                self.assertEqual(old,want,('collision',rung,text))

    def test_temporal_roundtrip_and_goldens(self):
        for op in UNARY:self.roundtrip(unary(op,P))
        for op in BINARY:self.roundtrip((op,P,Q))
        self.roundtrip(('always',('not',P)))
        gold={
            'formula':'∀σ ∈ Traces : ○(P())',
            'typed':'forall sigma in Traces: next(P())',
            'formal':'For every sigma in Traces, next open P of no arguments is true close.',
            'precise':'For every trace sigma, there is a next event and at that event (the event is P).',
            'intuitive':'For every trace sigma, at the next event, which must exist, (the event is P).'}
        for rung,text in gold.items():self.assertEqual(l.render(wrap(('next',P)),rung,self.ctx),text)
        for path in sorted(EXAMPLES.glob('*.json')):
            ctx=l.load(path);tree=l.parse(ctx.data['formula'],'typed',ctx)
            for rung in l.RUNGS:
                text=l.render(tree,rung,ctx);got=l.parse(text,rung,ctx)
                self.assertEqual(l.normal(got,ctx),l.normal(tree,ctx),(path.name,rung,text))
                self.assertEqual(l.render(got,rung,ctx),text)
            for rung in l.RUNGS:
                exported=l.ladder_text(tree,ctx,rung)
                for fragment in ('Trace source:','Mode:','Assumptions:','Evidence: obligation','version','authority: none'):
                    self.assertIn(fragment,exported)
        print('TEMPORAL rungs: constructors and 3 worked examples; 5 strong-next goldens')

    def test_temporal_exhaustive(self):
        start=time.monotonic();count=0;ctx=l.Context.from_data(combined_data());collisions={r:{} for r in l.RUNGS}
        for size in range(1,BOUND):
            for tree in combined_trees(size,'bool'):
                for quantifier in ('forall','exists'):
                    self.roundtrip(tree,collisions,ctx,quantifier);count+=1
        self.assertEqual(count,6366)
        print(f'TEMPORAL combined exhaustive: total nodes <= {BOUND}, including trace binder; {count} trees; {count*7} rung checks; zero collisions; {time.monotonic()-start:.3f}s',flush=True)

    def test_temporal_random(self):
        start=time.monotonic();rng=random.Random(SEED);ctx=l.Context.from_data(combined_data());collisions={r:{} for r in l.RUNGS}
        for _ in range(SAMPLES):
            self.roundtrip(combined_random(rng,'bool',rng.randint(1,5)),collisions,ctx,rng.choice(('forall','exists')))
        print(f'TEMPORAL combined random: seed {SEED}; {SAMPLES} trees, total depth <= 6; {SAMPLES*7} rung checks; zero collisions; {time.monotonic()-start:.3f}s',flush=True)

    def test_truth_tables_all_short_words(self):
        formulas=[P,Q,('not',P),('always',('not',P)),('not',('always',P))]
        formulas += [unary(op,a) for op in UNARY for a in (P,Q)]
        formulas += [(op,a,b) for op in BINARY for a,b in ((P,Q),(Q,P))]
        formulas += [('always',('=>',P,('eventually',Q))),('always',('=>',P,('within',2,Q))),('eventually',('next',Q)),('next',('not',P)),('not',('next',P)),('always',('eventually',P)),('until',('not',P),Q)]
        words=[word for length in range(1,5) for word in product(tuple(product((False,True),repeat=2)),repeat=length)]
        requests=[case(wrap(tree),self.ctx,words,positions=True) for tree in formulas]
        actual=native(requests);checks=0
        for tree,result in zip(formulas,actual):
            self.assertEqual(result['finite_export'],'Ok(())')
            for word,verdicts in zip(words,result['verdicts']):
                for position,verdict in enumerate(verdicts):
                    self.assertEqual(verdict=='Satisfied',expected_at(tree,word,position),(tree,word,position,verdict))
                    self.assertTrue(verdict=='Satisfied' or verdict.startswith('Counterexample'),verdict);checks+=1
        print(f'TEMPORAL independent truth tables: {len(formulas)} formulas; {len(words)} words of lengths 1..4; {checks} formula/position comparisons')

    def test_strong_next_endpoint(self):
        result=native([case(wrap(('next',P)),self.ctx,[[[1,0]]]),case(wrap(('not',('next',P))),self.ctx,[[[1,0]]])])
        self.assertEqual(result[0]['verdicts'],['Counterexample { step: 0 }'])
        self.assertEqual(result[1]['verdicts'],['Satisfied'])
        self.assertNotEqual(l.normal(wrap(('not',('next',P))),self.ctx),l.normal(wrap(('next',('not',P))),self.ctx))

    def test_until_requires_witness(self):
        result=native([case(wrap(('until',P,Q)),self.ctx,[[[1,0],[1,0]],[[1,0],[0,1]],[[0,1]]])])[0]
        self.assertEqual(result['verdicts'],['Counterexample { step: 0 }','Satisfied','Satisfied'])

    def test_deadline_now_equality_and_event_order(self):
        formulas=[('within',0,Q),('within',2,Q)]
        words=[[[0,1]],[[1,0],[0,0],[0,1]],[[1,0],[0,0],[0,0],[0,1]]]
        result=native([case(wrap(tree),self.ctx,words) for tree in formulas])
        self.assertEqual([v=='Satisfied' for v in result[0]['verdicts']],[True,False,False])
        self.assertEqual([v=='Satisfied' for v in result[1]['verdicts']],[True,True,False])
        strict=('eventually',('and',P,('next',('eventually',Q))))
        result=native([case(wrap(strict),self.ctx,[[[1,0],[0,1]],[[0,1],[1,0]],[[1,1]]])])[0]
        self.assertEqual([v=='Satisfied' for v in result['verdicts']],[True,False,False])

    def test_aliases_and_four_event_chain(self):
        for text,body in (('always P()',('always',P)),('○P()',('next',P)),('never(P())',('always',('not',P))),('(P() U Q())',('until',P,Q))):
            got=l.parse('forall sigma in Traces: '+text,'typed',self.ctx)
            self.assertEqual(l.normal(got,self.ctx),l.normal(wrap(body),self.ctx))
        a=('and',P,('not',Q));b=('and',('not',P),Q);c=('and',P,Q);loss=('and',('not',P),('not',Q))
        drift=('eventually',('and',a,('next',('eventually',('and',b,('next',('eventually',('and',c,('next',('eventually',loss))))))))))
        data=small_data();data['definitions']=[{'name':'Drift','params':[],'body':l.body_text(drift,'typed',self.ctx),'clause':'the strict four-event compromise chain occurs','version':'1'}]
        ctx=l.Context.from_data(data);target=l.parse('forall sigma in Traces: not Drift()','typed',ctx)
        for rung in l.RUNGS:
            text=l.render(target,rung,ctx)
            self.assertEqual(l.normal(l.parse(text,rung,ctx),ctx),l.normal(target,ctx))
        words=[[[1,0],[0,1],[1,1],[0,0]],[[1,0],[1,1],[0,1],[0,0]],[[0,0]]]
        results=native([case(target,ctx,words)])[0]['verdicts']
        self.assertEqual([v=='Satisfied' for v in results],[False,True,True])

    def test_mixed_terms_and_rigid_object_state(self):
        data=combined_data();data['native_schema']['fields'] += [{'id':120,'kind':'int'},{'id':121,'kind':'int'}]
        data['symbols']['f']['native']=[{'args':[v],'before':'pre.100.'+str(120+v),'after':'post.100.'+str(120+v)} for v in (0,1)]
        ctx=l.Context.from_data(data)
        formulas=[
            "forall v in D: always(f'(v) >= f(v))",
            "forall v in D: always(k'(v) >= k(v))",
            "forall v in D: S(v) => f'(v) >= f(v)",
            "always(f'(0) >= f(0))",
            "next(f(0) >= 0)",
            "#{v in D: eventually(S(v))} <= 1",
            "forall v in D: eventually(Rel(v, f(v)))",
            "always(floor(f(0) / -2) <= ceil(f'(0) / 3))",
        ]
        for formula in formulas:
            n=l.parse('forall sigma in Traces: '+formula,'typed',ctx)
            for rung in l.RUNGS:
                text=l.render(n,rung,ctx)
                self.assertEqual(l.normal(l.parse(text,rung,ctx),ctx),l.normal(n,ctx),(formula,rung,text))
        n=l.parse('forall sigma in Traces: '+formulas[0],'typed',ctx)
        text=l.render(n,'precise',ctx)
        self.assertIn('level of v after this event',text);self.assertIn('level of v before this event',text)
        self.assertIn('after this event',l.legend_text(n,ctx))
        paths=('pre.100.120','pre.100.121','post.100.120','post.100.121')
        words=[[[4,7,4,8]],[[4,7,3,8]],[[4,7,4,8],[4,8,5,9]]]
        state=native([case(n,ctx,words,paths)])[0]['verdicts']
        self.assertEqual([v=='Satisfied' for v in state],[True,False,True])
        next_target=l.parse('forall sigma in Traces: '+formulas[4],'typed',ctx)
        self.assertEqual(native([case(next_target,ctx,[words[0]],paths)])[0]['verdicts'],['Counterexample { step: 0 }'])

    def test_named_whole_requirement_definitions(self):
        for quantifier in ('forall','exists'):
            data=small_data();body=quantifier+' sigma in Traces: always(P())'
            data['definitions']=[{'name':'Requirement','params':[],'body':body,'clause':'the declared trace requirement holds','version':'1'}]
            ctx=l.Context.from_data(data)
            for source in ('Requirement()',body):
                target=l.parse(source,'typed',ctx)
                for rung in l.RUNGS:
                    text=l.render(target,rung,ctx)
                    self.assertEqual(l.normal(l.parse(text,rung,ctx),ctx),l.normal(target,ctx),(quantifier,rung,text))
                if quantifier=='exists':
                    self.assertIn('Witness search:',l.render(target,'question',ctx))
                    self.assertIsNone(lower(target,ctx).source)
                else:self.assertIsNotNone(lower(target,ctx).source)

    def test_complete_uniqueness(self):
        data=small_data();data['symbols']['Q']['clause']=data['symbols']['P']['clause'];ctx=l.Context.from_data(data)
        with self.assertRaisesRegex(l.Refusal,'ambiguous'):
            l.parse('For every trace sigma, always (the event is P).','intuitive',ctx)
        with patch.object(l,'EARLEY_MAX_ITEMS',1):
            with self.assertRaisesRegex(l.Refusal,'UNKNOWN:parse-item-budget'):
                l.parse('forall sigma in Traces: P()','typed',self.ctx)

    def test_refusals(self):
        cases=[('temporal:trace-binder-root','next(P())'),
            ('temporal:nested-trace-binder','forall sigma in Traces: forall tau in Traces: P()'),
            ('temporal:quantifier-inside-time','forall sigma in Traces: always(forall x in Traces: P())'),
            ('temporal:within-horizon','forall sigma in Traces: within(4, P())'),
            ('temporal:within-literal-range','forall sigma in Traces: within(256, P())'),
            ('syntax','forall sigma in Traces: since(P())'),
            ('syntax','forall sigma in Traces: within(-1, P())'),
            ('syntax','forall sigma in Traces: next(P()) trailing'),
            ('syntax','forall sigma in Traces: P() and Q() or P()')]
        for reason,text in cases:
            with self.subTest(text=text),self.assertRaisesRegex(l.Refusal,reason):l.parse(text,'typed',self.ctx)
        for text in ('x'*2001,' '*1+'P '*401):
            with self.assertRaisesRegex(l.Refusal,'input-bound'):l.parse(text,'typed',self.ctx)
        data=small_data();data.pop('assumptions');ctx=l.Context.from_data(data)
        with self.assertRaisesRegex(l.Refusal,'explicit-assumptions-required'):
            l.parse('forall sigma in Traces: eventually(P())','typed',ctx)
        for change,reason in (({'horizon':True},'horizon'),({'mode':'realtime'},'mode'),({'mode':'unbounded'},'unbounded-horizon'),({'projection':''},'missing-projection')):
            data=small_data();data['trace'].update(change)
            with self.assertRaisesRegex(l.Refusal,reason):l.Context.from_data(data)
        data=small_data();data['symbols']['P']['clause']='always selected'
        with self.assertRaisesRegex(l.Refusal,'reserved-lexicon'):l.Context.from_data(data)
        data=small_data();data['definitions']=[{'name':'next','params':[],'body':'P()','clause':'the named predicate holds','version':'1'}]
        with self.assertRaisesRegex(l.Refusal,'reserved-lexicon'):l.Context.from_data(data)
        data=small_data();data['symbols']['P'].pop('observation')
        with self.assertRaisesRegex(l.Refusal,'observation-kind'):l.Context.from_data(data)
        with self.assertRaisesRegex(l.Refusal,'within-literal-range'):l.validate(wrap(('within',True,P)),self.ctx)
        with self.assertRaisesRegex(l.Refusal,'tree-budget'):l.normal(wrap(('within',255,P)),l.Context.from_data({**small_data(),'trace':{**small_data()['trace'],'horizon':256}}))
        print('TEMPORAL named refusals: scope, endpoints, grouping, budgets, lexicon and observations')

    def test_scope_identity_mutations_and_aliases(self):
        data=small_data();ctx=l.Context.from_data(data);n=wrap(('always',P));before=l.ladder_text(n,ctx)
        data['trace']['projection']='changed';data['symbols']['P']['native'][0]['before']='post.100.110'
        self.assertEqual(l.ladder_text(n,ctx),before)
        with self.assertRaises(TypeError):ctx.data['symbols']['P']['native'][0]['before']='post.100.110'
        original=lower(n,ctx)
        for field,value in (('mode','unbounded'),('horizon',3),('order','timestamp order'),('source_revision','other revision'),('projection','drop retries')):
            altered=small_data();altered['trace'][field]=value
            if field=='mode':altered['trace']['horizon']=None
            other=l.Context.from_data(altered)
            self.assertNotEqual(l.scope_identity(ctx),l.scope_identity(other))
            self.assertNotEqual(original.subject,lower(n,other).subject)
        altered=small_data();altered['symbols']['P']['version']='2';self.assertNotEqual(original.subject,lower(n,l.Context.from_data(altered)).subject)
        with self.assertRaises(Exception):original.authority='approved'
        self.assertEqual(original.authority,'none')
        self.assertIsInstance(original.obligations,tuple)
        unbounded=l.Context.from_data({**small_data(),'trace':{**small_data()['trace'],'mode':'unbounded','horizon':None}})
        self.assertEqual(native([case(n,unbounded,[[]])])[0]['verdicts'],['ProofObligation'])

    def test_compiler_scratch_does_not_escape(self):
        import inspect
        seen=set()
        def inspect_value(value):
            if id(value) in seen:return
            seen.add(id(value))
            self.assertNotIsInstance(value,l.Compiler)
            if inspect.ismethod(value):inspect_value(value.__self__)
            if inspect.isfunction(value):
                for cell in value.__closure__ or ():inspect_value(cell.cell_contents)
                for default in value.__defaults__ or ():inspect_value(default)
            if isinstance(value,(tuple,list)):
                for child in value:inspect_value(child)
        for context in (self.ctx,l.Context.from_data(combined_data())):
            for rung in l.RUNGS[:-1]:
                rules=l.symbolic_grammar(True) if rung in ('formula','typed') else l.grammar(context,rung)
                for rule in rules:inspect_value(rule.action)

    def test_lowering_subject_binds_native_bytes(self):
        left=wrap(('and',P,Q));right=wrap(('and',Q,P))
        self.assertEqual(l.normal(left,self.ctx),l.normal(right,self.ctx))
        a=lower(left,self.ctx);b=lower(right,self.ctx)
        self.assertNotEqual(a.source,b.source)
        self.assertNotEqual(a.subject,b.subject)
        for change in ('trace','version','assumptions'):
            data=small_data()
            if change=='trace':data['trace']=[]
            elif change=='version':data['symbols']['P']['version']=True
            else:data['assumptions']=[]
            with self.assertRaises(l.Refusal):l.Context.from_data(data)

    def test_mutations(self):
        pairs=[(('always',P),('eventually',P)),(('next',P),P),(('always',('not',P)),('not',('always',P))),
            (('until',P,Q),('until',Q,P)),(('within',1,P),('within',2,P)),(('=>',P,Q),('=>',Q,P))]
        controls=0
        for a,b in pairs:
            self.assertNotEqual(l.normal(wrap(a),self.ctx),l.normal(wrap(b),self.ctx))
            for rung in l.RUNGS:self.assertNotEqual(l.render(wrap(a),rung,self.ctx),l.render(wrap(b),rung,self.ctx));controls+=1
            self.roundtrip(a);self.roundtrip(b)
        print(f'TEMPORAL mutations: {len(pairs)} formula pairs; {controls} rung controls; mode/horizon/assumption/identity controls separate')

    def test_examples_native_and_rigid_identity(self):
        requests=[];expected=[]
        for path in sorted(EXAMPLES.glob('*.json')):
            data=json.loads(path.read_text())
            if data['trace']['mode']=='unbounded':data['trace'].update(mode='finite',horizon=4)
            ctx=l.Context.from_data(data);tree=l.parse(ctx.data['formula'],'typed',ctx)
            for fixture in ctx.data['fixtures']:
                requests.append(case(tree,ctx,[fixture['events']],fixture['paths'],constants=(('A',True),) if 'A' in data['symbols'] else ()))
                expected.append((fixture['id'],fixture['expected']))
        for result,(name,want) in zip(native(requests),expected):
            self.assertEqual(result['verdicts'][0]=='Satisfied',want,name)
        ctx=l.load(EXAMPLES/'alarm_withdrawal.json');tree=l.parse(ctx.data['formula'],'typed',ctx)
        self.assertIsNone(lower(tree,ctx).source)
        self.assertTrue(any('unbounded-run-assumption' in reason for reason in lower(tree,ctx).obligations))
        self.assertIsNone(lower(tree,ctx,(('A',True),)).source)
        altered=l.plain_data(ctx.data);altered['formula']=altered['formula'].replace('R(r) => eventually(C(r))','C(r) => eventually(R(r))')
        other=l.Context.from_data(altered)
        for rung in l.RUNGS:self.assertNotEqual(l.render(tree,rung,ctx),l.render(l.parse(altered['formula'],'typed',other),rung,other))
        bad=l.plain_data(ctx.data);bad['assumptions']['formula']='R(r)'
        with self.assertRaisesRegex(l.Refusal,'assumption-scope'):
            l.parse(bad['formula'],'typed',l.Context.from_data(bad))
        # Partial drain and legitimate withdrawal have different independently defined D.
        disaster=lambda v,p,c:v+p<10+c
        self.assertFalse(disaster(7,3,0));self.assertTrue(disaster(7,0,0));self.assertTrue(disaster(9,0,0))
        print(f'TEMPORAL examples: {len(expected)} replay expectations; rigid request identity; pending finite response; UI modal hiding; initial and partial loss')

    def test_alarm_budget_nonresettable(self):
        journey=alarm_journey()
        self.assertEqual([event['after'] for event in journey],[1,0,0,0])
        self.assertEqual([event['enabled'] for event in journey],[False,True,True,True])
        data=json.loads((EXAMPLES/'alarm_withdrawal.json').read_text());data['trace'].update(mode='finite',horizon=4)
        ctx=l.Context.from_data(data);tree=l.parse(data['formula'],'typed',ctx)
        word=[[int(e['request']),int(e['complete']),0,0] for e in journey]
        reject_everything=[[int(e['request']),0,0,0] for e in journey]
        result=native([case(tree,ctx,[word,reject_everything],('pre.100.110','pre.100.111','pre.100.112','pre.100.113'),constants=(('A',True),))])[0]
        self.assertEqual([v=='Satisfied' for v in result['verdicts']],[True,False])

    def test_native_indeterminate_and_checked_arithmetic(self):
        simple=case(wrap(P),self.ctx,[[],[[None,0]],[[1,0]]*5])
        missing=case(wrap(P),self.ctx,[[[0,0]]]);missing['source']=missing['source'].replace('atom(pre.100.110 == 1)','atom(absent())')
        self.assertEqual(native([missing])[0]['verdicts'],['Indeterminate(MissingPredicate)'])
        result=native([simple])[0]['verdicts']
        self.assertEqual(result,['Indeterminate(EmptyTrace)','Indeterminate(MissingProjection)','Indeterminate(HorizonExceeded)'])
        data=small_data();data['symbols']['v']={'kind':'function','arity':0,'args':[],'state':True,'noun':'value','observation':'event','definition':'checked i128 scalar','version':'1','native':[{'args':[],'before':'pre.100.120','after':'post.100.120'}]}
        data['native_schema']['fields'].append({'id':120,'kind':'int'});ctx=l.Context.from_data(data)
        bodies=['v()+1 >= v()', 'floor(v() / 0) = 0', 'ceil(v() / 3) = 1', "v'() = v()+1", 'floor(v() / -2) = -1', 'ceil(v() / -2) = 0']
        requests=[]
        for body in bodies:
            tree=l.parse('forall sigma in Traces: '+body,'typed',ctx)
            requests.append(case(tree,ctx,[[[str((1<<127)-1),0]],[[1,2]]],('pre.100.120','post.100.120')))
            for rung in l.RUNGS:self.assertEqual(l.normal(l.parse(l.render(tree,rung,ctx),rung,ctx),ctx),l.normal(tree,ctx))
        results=native(requests)
        self.assertEqual(results[0]['verdicts'][0],'Indeterminate(Overflow)')
        self.assertEqual(results[1]['verdicts'],['Indeterminate(DivisionByZero)']*2)
        self.assertEqual(results[2]['verdicts'][1],'Satisfied')
        self.assertEqual(results[3]['verdicts'][1],'Satisfied')
        self.assertEqual(results[4]['verdicts'][1],'Satisfied')
        self.assertEqual(results[5]['verdicts'][1],'Satisfied')
        self.assertIn('nonzero divisor',l.legend_text(l.parse('forall sigma in Traces: floor(v() / -2) = -1','typed',ctx),ctx))
        data=small_data();data['trace']['horizon']=256;ctx=l.Context.from_data(data)
        with self.assertRaisesRegex(l.Refusal,'lowering:native-export-operations-budget'):
            lower(wrap(('always',('always',('always',P)))),ctx)
        # Keep an independent raw-native evaluator exhaustion control. The
        # producer now refuses this over-budget subject before native replay.
        source=lower(wrap(P),self.ctx).source.split(' = ',1)[0].replace('finite 4','finite 256')
        source+=' = always (always (always (atom(pre.100.110 == 1))));\n'
        result=native([{'source':source,'paths':['pre.100.110','pre.100.111'],'words':[[[1,0]]*256]}])[0]['verdicts'][0]
        self.assertEqual(result,'Indeterminate(OperationLimit)')

    def test_cli_lower_and_existential_obligation(self):
        program=Path(l.__file__).resolve()
        for name,status in (('ui_discovery',0),('alarm_withdrawal',2)):
            result=subprocess.run([os.sys.executable,'-I',str(program),'lower',str(EXAMPLES/(name+'.json'))],text=True,capture_output=True)
            self.assertEqual(result.returncode,status,result.stderr)
            self.assertIn('authority: none',result.stdout)
            self.assertIn('Obligation:',result.stdout)
        target=('exists','sigma','Traces',('eventually',P))
        for rung in l.RUNGS:
            text=l.render(target,rung,self.ctx)
            self.assertEqual(l.normal(l.parse(text,rung,self.ctx),self.ctx),l.normal(target,self.ctx))
        self.assertIsNone(lower(target,self.ctx).source)

    def test_legend_and_old_dialects(self):
        for body in [unary(op,P) for op in UNARY]+[('until',P,Q),('always',('not',P))]:
            tree=wrap(body);legend=l.legend(tree,self.ctx);keys=[entry[0] for entry in legend]
            self.assertEqual(len(keys),len(set(keys)));self.assertEqual(keys[0],'∀σ ∈ Traces :')
            self.assertEqual(keys[1:3],['σ','Traces'])
            text=l.legend_text(tree,self.ctx);self.assertIn('Theory: ordered logical events',text)
            self.assertIn('sigma',text);self.assertIn('prime',text)
            for node in l.walk_nodes(body):
                if node[0] in l.TEMPORAL:self.assertIn(l.REGISTRY['temporal_legend'][node[0]][0].format(k=node[1] if node[0]=='within' else ''),keys)
        import test_ladder
        old=l.Context.from_data(test_ladder.small_data())
        with self.assertRaises(l.Refusal):l.parse('next(P(0))','typed',old)
        old_data=test_ladder.small_data();old_data['symbols']['next']=copy.deepcopy(old_data['symbols']['P'])
        old=l.Context.from_data(old_data)
        self.assertEqual(l.parse('next(0)','typed',old)[0],'call')
        # Unbounded object domains remain visible unsupported obligations.
        data=json.loads((EXAMPLES/'ui_discovery.json').read_text());data['domains']['Actions'].pop('values')
        ctx=l.Context.from_data(data);result=lower(l.parse(data['formula'],'typed',ctx),ctx)
        self.assertIsNone(result.source);self.assertTrue(any('unbounded-object-domain' in item for item in result.obligations))


if __name__=='__main__':unittest.main()
