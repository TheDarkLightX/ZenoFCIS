"""Pinned M0 checks. Exhaustive means all closed typed trees in small_signature.

One node per AST constructor, including variables/literals/calls. Bound variable
spellings are represented once, by binding depth; no arithmetic/logic quotient
is used during enumeration. Larger idiom shapes are covered in addition.
"""
import copy
from functools import lru_cache
from itertools import product
import json
from pathlib import Path
import random
import re
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

import ladder as l

CORPUS=Path(__file__).with_name('examples')/'ladders'
N=5
RANDOM_COUNT=20000
SEED=20261016


def corpus():
    return [(l.load(p),p) for p in sorted(CORPUS.glob('*.json'))]


def small_data():
    return {'theory':'steps-over-integers','id':'small','kind':'law','authority':{'status':'OPEN','source':'bounded test signature'},
        'domains':{'D':{'member':'member','plural':'members','noun':'the domain','integer':True}},
        'symbols':{'P':{'kind':'predicate','arity':1,'args':['D'],'clause':'{0} is selected','noun':'selected member'},
                   'Q':{'kind':'predicate','arity':2,'args':['D','D'],'clause':'{0} relates to {1}'},
                   'f':{'kind':'function','arity':1,'args':['D'],'state':True,'noun':'level'}},
        'definitions':[{'name':'k','params':[{'name':'z','domain':'D'}],'body':'f(z)+1','noun':'the shifted level'}],
        'formula':'0 = 0','fixtures':[]}


@lru_cache(maxsize=None)
def enumerate_trees(size,kind,depth=0):
    out=[]
    if kind=='int' and size==1:
        out.extend([('int',0),('int',1),*(('var',f'v{i}') for i in range(depth))])
    if size>=2:
        if kind=='int':
            for nm in ('f','k'):
                for prime in (False,True):
                    out.extend(('call',nm,prime,(a,)) for a in enumerate_trees(size-1,'int',depth))
            out.extend(('count',f'v{depth}','D',a) for a in enumerate_trees(size-1,'bool',depth+1))
        else:
            out.extend(('call','P',False,(a,)) for a in enumerate_trees(size-1,'int',depth))
            out.extend(('not',a) for a in enumerate_trees(size-1,'bool',depth))
            for op in ('forall','exists','exists1'):
                out.extend((op,f'v{depth}','D',a) for a in enumerate_trees(size-1,'bool',depth+1))
        for left in range(1,size-1):
            right=size-1-left
            if kind=='int':
                for op in l.ARITH:
                    out.extend((op,a,b) for a,b in product(enumerate_trees(left,'int',depth),enumerate_trees(right,'int',depth)))
            else:
                out.extend(('call','Q',False,(a,b)) for a,b in product(enumerate_trees(left,'int',depth),enumerate_trees(right,'int',depth)))
                for op in l.COMPARE:
                    out.extend((op,a,b) for a,b in product(enumerate_trees(left,'int',depth),enumerate_trees(right,'int',depth)))
                for op in l.LOGIC:
                    out.extend((op,a,b) for a,b in product(enumerate_trees(left,'bool',depth),enumerate_trees(right,'bool',depth)))
    return tuple(out)


def random_tree(rng,kind,depth,env=()):
    if depth<=1 or kind=='bool' and depth<=2:
        terms=[('int',0),('int',1),*(('var',v) for v in env)]
        return rng.choice(terms) if kind=='int' else ('call','P',False,(rng.choice(terms),))
    if kind=='int':
        op=rng.choice(('leaf','call',*(('count',) if depth>=3 else ()),*l.ARITH))
        if op=='leaf': return random_tree(rng,kind,1,env)
        if op=='call': return ('call',rng.choice(('f','k')),rng.choice((False,True)),(random_tree(rng,'int',depth-1,env),))
        if op=='count':
            v=f'v{len(env)}';return ('count',v,'D',random_tree(rng,'bool',depth-1,(*env,v)))
        return (op,random_tree(rng,'int',depth-1,env),random_tree(rng,'int',depth-1,env))
    op=rng.choice(('P','Q','not','forall','exists','exists1',*l.COMPARE,*l.LOGIC))
    if op=='P': return ('call','P',False,(random_tree(rng,'int',depth-1,env),))
    if op=='Q': return ('call','Q',False,tuple(random_tree(rng,'int',depth-1,env) for _ in range(2)))
    if op=='not': return ('not',random_tree(rng,'bool',depth-1,env))
    if op in l.BIND:
        v=f'v{len(env)}';return (op,v,'D',random_tree(rng,'bool',depth-1,(*env,v)))
    kind='int' if op in l.COMPARE else 'bool'
    return (op,random_tree(rng,kind,depth-1,env),random_tree(rng,kind,depth-1,env))


def mutants(n,ctx):
    """All valid single-site changes from the fixed spec list; no semantic filter."""
    def local(n):
        tag=n[0];out=[]
        if tag=='>=':out.extend((op,*n[1:]) for op in ('>','<=','='))
        if tag in ('forall','exists'):out.append(('exists' if tag=='forall' else 'forall',*n[1:]))
        if tag=='not':out.append(n[1])
        # Adding a negation is a formula operation, including at a not node.
        if tag in (*l.COMPARE,*l.LOGIC,'not','forall','exists','exists1') or tag=='call' and ctx.symbol(n[1])['kind']=='predicate':
            out.append(('not',n))
        if tag=='=>':out.append(('=>',n[2],n[1]))
        if tag in ('and','or'):out.append(('or' if tag=='and' else 'and',*n[1:]))
        if tag=='call' and l.is_state(n[1],ctx):out.append(('call',n[1],not n[2],n[3]))
        if tag in ('floor','ceil'):out.append(('ceil' if tag=='floor' else 'floor',*n[1:]))
        if tag=='int':out.extend(('int',n[1]+delta) for delta in (-1,1))
        return out
    out=list(local(n))
    cs=l.children(n)
    for i,c in enumerate(cs):
        out.extend(l.rebuild(n,(*cs[:i],m,*cs[i+1:])) for m in mutants(c,ctx))
    return tuple(dict.fromkeys(out))


def tree_depth(n):
    return 1+max((tree_depth(c) for c in l.children(n)),default=0)


class LadderChecks(unittest.TestCase):
    def roundtrip(self,n,c,collisions=None):
        want=l.normal(n,c)
        for rung in l.RUNGS:
            text=l.render(n,rung,c)
            try: got=l.parse(text,rung,c)
            except Exception as exc: self.fail(f'{rung}: {n!r}\n{text}\n{exc}')
            self.assertTrue(all(re.fullmatch(l.NAME,z[1]) for z in l.walk_nodes(got) if z[0] in (*l.BIND,'var')),(rung,got))
            self.assertEqual(l.normal(got,c),want,(rung,text,n,got))
            self.assertEqual(l.render(got,rung,c),text,(rung,n,got))
            if collisions is not None:
                key=l.alpha(n) if rung in ('formula','typed','formal','precise') else want
                old=collisions[rung].setdefault(text,key)
                self.assertEqual(old,key,('collision',rung,text,n))

    def test01_corpus_roundtrip_and_goldens(self):
        count=0
        for c,p in corpus():
            n=l.parse(c.data['formula'],'typed',c);self.roundtrip(n,c);count+=len(l.RUNGS)
        c=l.load(CORPUS/'dex_k.json');n=l.syntax(c.data['formula'])
        gold={
            'formula':'∀t ∈ T : S(t) ⇒ x′(t)·y′(t) ≥ x(t)·y(t)',
            'typed':"forall t in T: S(t) => x'(t)*y'(t) >= x(t)*y(t)",
            'formal':'For every t in T, S of t is true implies x prime of t times y prime of t is at least x of t times y of t.',
            'precise':'For every step t, if t is an accepted swap, then reserve x after t times reserve y after t is at least reserve x before t times reserve y before t.',
            'intuitive':'No accepted swap decreases reserve x times reserve y.',
            'plain':'No accepted swap decreases the reserve product.',
            'question':'Is it true that no accepted swap decreases the reserve product?\nCounterexample search: find an accepted swap t such that the reserve product after t is less than the reserve product before t.'}
        for rung,text in gold.items():self.assertEqual(l.render(n,rung,c),text)
        c=l.load(CORPUS/'exists_ai.json');n=l.syntax(c.data['formula'])
        for rung,text in {'formula':'∃x ∈ X : P(x)', 'formal':'There exists an x in X such that P of x is true.',
            'intuitive':'There exists a candidate in the search space that satisfies the required property.',
            'question':'Does there exist a candidate in the search space that satisfies the required property?\nWitness search: propose a candidate x in the search space; the checker decides whether x satisfies the required property.'}.items():
            self.assertEqual(l.render(n,rung,c),text)
        print(f'CHECK 1: {count} corpus rung round trips; 11 golden strings')

    def test02_complete_uniqueness(self):
        data=small_data();data['symbols']['U']=dict(data['symbols']['P'])
        c=l.Context.from_data(data)
        with self.assertRaisesRegex(l.Refusal,'ambiguous.*token.*expected'):
            l.parse('0 is selected.','precise',c)
        # Same tree via duplicate productions remains admissible.
        c=l.Context.from_data(small_data());self.assertEqual(l.parse('P(0)','typed',c),('call','P',False,(('int',0),)))
        # Earley must retain a late alternative after a shared prefix and suffix.
        rs=(l.Rule('@F',('x','@A','z'),lambda v:v[1]),l.Rule('@A',('y',),lambda v:('int',0)),l.Rule('@A',('y',),lambda v:('int',1)))
        self.assertEqual(set(l.earley(('x','y','z'),rs)),{('int',0),('int',1)})
        print('CHECK 2: ambiguity refused; duplicate derivations accepted; both late parses retained')

    def test03_injectivity(self):
        total=0
        for c,p in corpus():
            n=l.syntax(c.data['formula']);original={r:l.render(n,r,c) for r in l.RUNGS}
            for m in mutants(n,c):
                l.validate(m,c);self.assertNotEqual(l.normal(m,c),l.normal(n,c))
                for r in l.RUNGS:self.assertNotEqual(l.render(m,r,c),original[r],(p.name,r,m))
                self.roundtrip(m,c);total+=1
        print(f'CHECK 3: {total} mutants; {total*len(l.RUNGS)} rung controls; 0 collisions')

    def test04_refusals(self):
        c=l.Context.from_data(small_data());amb=small_data();amb['symbols']['U']=dict(amb['symbols']['P']);amb=l.Context.from_data(amb)
        cases=[('unknown_symbol','typed','Z(0)'),('unknown_domain','typed','forall x in Z: P(x)'),
            ('free_variable','typed','P(z)'),('trailing_words','typed','P(0) forever'),
            ('free_prose','precise','All the nice things will work.'),('unknown_word','precise','0 is magical.'),
            ('mixed_and_or','typed','P(0) and P(1) or P(0)'),('nested_implies','typed','P(0) => P(1) => P(0)'),
            ('missing_colon','typed','forall x in D P(x)'),('unclosed_group','typed','(P(0)'),
            ('bad_prime','typed',"P'(0)"),('bad_arity','typed','P(0, 1)'),('wrong_type','typed','P(P(0))'),
            ('bare_term','typed','f(0)'),('idiom_wrong_shape','intuitive','No selected member decreases 1.'),
            ('idiom_unknown_quantity','plain','No selected member decreases happiness.'),
            ('ambiguous','precise','0 is selected.'),('missing_period','formal','P of 0 is true'),
            ('missing_rounded_direction','formal','0 equals 1 over 1 rounded.'),('wrong_count_word','intuitive','At least one member x such that x is selected.'),
            ('trailing_english','precise','0 is selected. Please accept.'),('oversize_chars','typed',' '*2001),
            ('oversize_tokens','typed',' '.join(['(']*401)),('bad_question_task','question','Is it true that 0 is selected?\nCounterexample search: evaluate 0 is selected.')]
        for name,rung,text in cases:
            with self.subTest(name=name):
                with self.assertRaisesRegex(l.Refusal,'token [0-9]+; expected'):
                    l.parse(text,rung,amb if name=='ambiguous' else c)
        print(f'CHECK 4: {len(cases)} named refusals, token positions and expected alternatives')

    def test05_fixtures(self):
        count=0
        for c,p in corpus():
            n=l.syntax(c.data['formula'])
            for f in c.data['fixtures']:
                self.assertIs(l.evaluate(n,c,f),f['expected'],f['id']);count+=1
            self.assertEqual(l.ladder_text(n,c).count('For example '),len(c.data['fixtures']))
        c=l.load(CORPUS/'fee_rounding.json')
        for f,a in zip(c.data['fixtures'][:5],(1000,1333,1334,10000,1)):
            fee=-(-a*30//10000);protocol=fee*2000//10000
            want={'a':a,'fee':fee,'protocol':protocol,'net':a-fee,'lp':fee-protocol}
            for nm,val in want.items():self.assertEqual(f['symbols'][nm]['rows'][0][1],val)
        print(f'CHECK 5: {count} fixture truths; five fee tuples independently computed')

    def test06_prime_sensitive_idiom(self):
        c=l.load(CORPUS/'dex_k.json');n=l.syntax(c.data['formula']);before=n[3][2][2]
        wrong=(*n[:3],('=>',n[3][1],('>=',before,before)))
        self.assertNotEqual(l.render(n,'intuitive',c),l.render(wrong,'intuitive',c));self.roundtrip(wrong,c)
        print('CHECK 6 guard: prime-ignoring idiom changes this test; external planted runs required')

    def test07_semantic_search(self):
        c=l.Context.from_data(small_data())
        a=l.syntax('forall x in D: P(x) and Q(x, 0)');b=l.syntax('forall y in D: Q(y, 0) and P(y)')
        self.assertEqual(l.normal(a,c),l.normal(b,c))
        self.assertEqual(l.normal(l.syntax('not not P(0)'),c),l.normal(l.syntax('P(0)'),c))
        self.assertEqual(l.normal(l.syntax('0 < f(1)'),c),l.normal(l.syntax('f(1) > 0'),c))
        self.assertNotEqual(l.normal(l.syntax('0+1 = 1'),c),l.normal(l.syntax('1 = 1'),c))
        for ctx,p in corpus():
            n=l.syntax(ctx.data['formula']);self.assertEqual(l.normal(l.parse(l.render(n,'plain',ctx),'plain',ctx),ctx),l.normal(n,ctx))
            for m in mutants(n,ctx):self.assertIsNotNone(l.first_difference(l.normal(n,ctx),l.normal(m,ctx)))
        with tempfile.TemporaryDirectory() as tmp:
            data=small_data();data['formula']=l.body_text(a,'typed',c);pa=Path(tmp)/'a.json';pa.write_text(json.dumps(data))
            data['formula']=l.body_text(b,'typed',c);pb=Path(tmp)/'b.json';pb.write_text(json.dumps(data))
            for cmd,expected in ((['same',str(pa),str(pb)],'same meaning'),(['find',str(pa),'--corpus',tmp],'a.json')):
                r=subprocess.run([sys.executable,'-I',str(Path(l.__file__).resolve()),*cmd],capture_output=True,text=True)
                self.assertEqual(r.returncode,0,r.stdout+r.stderr);self.assertIn(expected,r.stdout)
            data['formula']='P(0)';pb.write_text(json.dumps(data))
            r=subprocess.run([sys.executable,'-I',str(Path(l.__file__).resolve()),'same',str(pa),str(pb)],capture_output=True,text=True)
            self.assertEqual(r.returncode,0);self.assertIn('different; first differing subtree:',r.stdout)
        r=subprocess.run([sys.executable,'-I',str(Path(l.__file__).resolve()),'parse','--rung','plain',
            'No accepted swap decreases the reserve product.','--context',str(CORPUS/'dex_k.json')],capture_output=True,text=True)
        self.assertEqual(r.returncode,0,r.stdout+r.stderr)
        cc=l.load(CORPUS/'dex_k.json');self.assertEqual(l.normal(l.syntax(r.stdout),cc),l.normal(l.syntax(cc.data['formula']),cc))
        print('CHECK 7: alpha/AC/orientation/double negation/definitions; every mutant differs; same/find CLI')

    def test08_exhaustive(self):
        start=time.monotonic();c=l.Context.from_data(small_data());seen={r:{} for r in l.RUNGS};count=0
        for size in range(1,N+1):
            for n in enumerate_trees(size,'bool'):
                self.roundtrip(n,c,seen);count+=1
        print(f'CHECK 8 exhaustive: N={N}; {count} ASTs; {count*len(l.RUNGS)} round trips; 0 collisions; {time.monotonic()-start:.3f}s',flush=True)

    def test08_random(self):
        start=time.monotonic();c=l.Context.from_data(small_data());rng=random.Random(SEED);seen={r:{} for r in l.RUNGS}
        for i in range(RANDOM_COUNT):
            n=random_tree(rng,'bool',rng.randint(2,6));self.assertLessEqual(tree_depth(n),6);self.roundtrip(n,c,seen)
        print(f'CHECK 8 random: seed={SEED}; {RANDOM_COUNT} ASTs up to depth 6; 0 collisions; {time.monotonic()-start:.3f}s',flush=True)

    def test08_idiom_shapes_and_definitions(self):
        c=l.Context.from_data(small_data())
        forms=["forall v in D: P(v) => f'(v) >= f(v)",
            'forall v in D: P(v) => P(v)', 'forall v in D: P(v) => not P(v)',
            '#{v in D: P(v)} <= 1', '#{v in D: P(v)} = 1',
            'forall v in D: forall w in D: Q(v,w)', 'forall v in D: P(v) => Q(v,v)',
            'exists v in D: P(v) and Q(v,v)', 'exists v in D: P(v)',
            "forall v in D: P(v) => f'(v)+v >= f(v)+v"]
        forms.extend(['exists1 v0 in D: exists v1 in D: P(v1)',
            'forall v in D: P(v) => (P(v) => Q(v,v))',
            '(0+1)-(1-0) = 0+(1-0)',
            'P(floor(f(0) / 1))'])
        for f in forms:self.roundtrip(l.syntax(f),c)
        cc=l.load(CORPUS/'no_free_value.json');folded=l.fold(l.syntax(cc.data['formula']),cc)
        self.assertIn('FreeGain',[z[1] for z in l.walk_nodes(folded) if z[0]=='call'])
        self.assertEqual(l.normal(l.syntax("k'(0) = 1"),c),l.normal(l.syntax("f'(0)+1 = 1"),c))
        # Actual substitution must not capture an argument under a local binder.
        d=small_data();d['definitions']=[{'name':'C','params':[{'name':'z','domain':'D'}],
            'body':'exists y in D: Q(z,y)','clause':'{0} has a related member'}]
        cc=l.Context.from_data(d);a=l.syntax('forall y in D: C(y)');b=l.syntax('forall x in D: exists z in D: Q(x,z)')
        self.assertEqual(l.normal(a,cc),l.normal(b,cc))
        for body in ('k(z)', 'f(z)+unknown'):
            d=small_data();d['definitions'][0]['body']=body
            with self.assertRaises(l.Refusal):l.Context.from_data(d)
        data=small_data();cc=l.Context.from_data(data);data['symbols']['P']['clause']='changed'
        self.assertEqual(cc.data['symbols']['P']['clause'],'{0} is selected')
        with self.assertRaises(TypeError):cc.data['symbols']['P']['clause']='changed'
        self.assertEqual(l.syntax('∀v ∈ D : ¬P(v) ⇔ (f′(v) ≥ ⌈1/1⌉ ∧ f(v) ≠ ⌊0/1⌋)'),
            l.syntax("forall v in D: not P(v) <=> (f'(v) >= ceil(1 / 1) and f(v) != floor(0 / 1))"))
        print('CHECK 8 additional: all 9 registered idiom variants; state expression with visible variable; hygienic definitions; aliases; immutable context')

    def test09_legend(self):
        c=l.load(CORPUS/'dex_k.json');n=l.syntax(c.data['formula'])
        gold='''∀t ∈ T :  | for every t in T,      | the statement after the colon holds for each t in T; true if T is empty
T         | T                      | the steps
S(t)      | S of t                 | t is an accepted swap
⇒         | implies                | when the left side is true the right side must be; when it is false there is no demand
x′(t)     | x prime of t           | reserve x after step t (the prime marks "after"; it is not negation, which is ¬)
·         | times                  | integer multiplication
y′(t)     | y prime of t           | reserve y after step t
≥         | is at least            | greater than or equal to
x(t)      | x of t                 | reserve x before step t
y(t)      | y of t                 | reserve y before step t'''
        self.assertEqual(l.legend_text(n,c),l.theory_info(c.data)['legend_header']+'\n'+gold)
        for c,p in corpus():
            n=l.syntax(c.data['formula']);entries=l.legend(n,c);keys=[x[0] for x in entries]
            self.assertEqual(len(keys),len(set(keys)))
            formula=l.render(n,'formula',c);positions=[]
            for key,read,meaning in entries:
                self.assertTrue(read);self.assertTrue(meaning)
                if key in ('#{…}','⌈u/v⌉','⌊u/v⌋'):
                    key={'#{…}':'#{','⌈u/v⌉':'⌈','⌊u/v⌋':'⌊'}[key]
                if key=='(…)':
                    grouping=[i for i,ch in enumerate(formula) if ch=='(' and (i==0 or not (formula[i-1].isalnum() or formula[i-1] in "_′'"))]
                    self.assertTrue(grouping);positions.append(grouping[0])
                else:self.assertIn(key,formula);positions.append(formula.index(key))
            self.assertEqual(positions,sorted(positions))
            grouping=any(ch=='(' and (i==0 or not (formula[i-1].isalnum() or formula[i-1] in "_′'")) for i,ch in enumerate(formula))
            self.assertEqual('(…)' in keys,grouping)
            for op in set(z[0] for z in l.walk_nodes(n)) & set(l.COMPARE+l.LOGIC+l.ARITH+('not',)):
                if op in ('floor','ceil'):symbol='⌊u/v⌋' if op=='floor' else '⌈u/v⌉'
                else:symbol=l.REGISTRY['templates']['formula'][op].replace('{a}','').replace('{b}','').strip()
                self.assertEqual(keys.count(symbol),1)
            declared={z[1] for z in l.walk_nodes(n) if z[0]=='call'}|{z[2] for z in l.walk_nodes(n) if z[0] in l.BIND}
            for nm in declared:
                shown=l.formula_identifier(nm)
                self.assertTrue(any(k==shown or k.startswith(shown+'(') or k.startswith(shown+'′(') for k in keys),nm)
        print('CHECK 9: DEX golden; every used declaration covered once per displayed form; first appearance order')

    def test10_missing_theory(self):
        data=small_data();del data['theory']
        with self.assertRaisesRegex(l.Refusal,'^unsupported-theory:'):
            l.Context.from_data(data)
        print('THEORY 1: missing theory refused')

    def test11_unknown_theory(self):
        for theory in ('boolean-algebra','calculus','',None,{}):
            with self.subTest(theory=theory):
                data=small_data();data['theory']=theory
                with self.assertRaisesRegex(l.Refusal,'^unsupported-theory:'):
                    l.Context.from_data(data)
        print('THEORY 2: unknown theories refused, including boolean-algebra')

    def test12_theory_dictionary_and_scope(self):
        c=l.load(CORPUS/'dex_k.json');n=l.syntax(c.data['formula'])
        header=l.legend_text(n,c).splitlines()[0]
        for text in ('Theory: steps over integers','integer and Boolean values','not negation or a derivative','Boolean algebra','¬'):
            self.assertIn(text,header)
        for rung in l.RUNGS:
            text=l.ladder_text(n,c,rung)
            scope=next(line for line in text.splitlines() if line.startswith('Scope'))
            self.assertIn(c.data['theory'],scope)
            if rung=='formula':self.assertLess(text.index('Theory:'),text.index('Legend:'))
        registry=json.loads(Path(l.__file__).with_name('ladder_en.json').read_text())
        registry['theories']['steps-over-integers']['symbols']['prime']['meaning']='dictionary marker for {noun} at {step}'
        with patch.object(l,'REGISTRY',l.freeze(registry)):
            entry=next(row for row in l.legend(n,c) if row[0]=='x′(t)')
            self.assertTrue(entry[2].startswith('dictionary marker for reserve x at step t'))
        print('THEORY 3: theory dictionary supplies prime meaning; legend and every scope strip identify theory')

    def test13_cross_theory_search(self):
        with tempfile.TemporaryDirectory() as folder:
            pa,pb=Path(folder)/'a.json',Path(folder)/'b.json'
            data=small_data();pa.write_text(json.dumps(data));data['theory']='boolean-algebra'
            # A different-theory formula must never reach the M0 parser.
            data['formula']='not M0 syntax at all';pb.write_text(json.dumps(data))
            for cmd in (['same',str(pa),str(pb)],['same',str(pb),str(pa)],
                        ['find',str(pa),'--corpus',folder]):
                r=subprocess.run([sys.executable,'-I',str(Path(l.__file__).resolve()),*cmd],capture_output=True,text=True)
                self.assertEqual(r.returncode,2,r.stdout+r.stderr)
                self.assertIn('different theories, not compared',r.stdout)
                self.assertNotIn('same meaning',r.stdout)
            r=subprocess.run([sys.executable,'-I',str(Path(l.__file__).resolve()),'same',str(pb),str(pb)],capture_output=True,text=True)
            self.assertEqual(r.returncode,2);self.assertIn('unsupported-theory',r.stdout)
        print('THEORY 4: same/find refuse cross-theory comparison before parsing; same unsupported theory also refused')

    def test14_count_and_idiom_goldens(self):
        c=l.load(CORPUS/'active_vote.json');n=l.syntax(c.data['formula'])
        self.assertEqual(l.render(n,'intuitive',c),
            'For every person p and every ballot b, there is at most one record r such that r is an active counted record for p on b.')
        exact=l.rebuild(n,(l.rebuild(n[3],(('=',n[3][3][1],('int',1)),)),))
        self.assertEqual(l.render(exact,'intuitive',c),
            'For every person p and every ballot b, there is exactly one record r such that r is an active counted record for p on b.')
        self.assertEqual(l.render(n,'precise',c),
            'For every person p, for every ballot b, the number of records r such that r is an active counted record for p on b is at most 1.')
        self.roundtrip(n,c);self.roundtrip(exact,c)
        cc=l.Context.from_data(small_data())
        for formula in ('#{v in D: not P(v)} <= 0','#{v in D: not not P(v)} <= 0'):
            f=l.syntax(formula);self.roundtrip(f,cc);self.assertNotIn('such that (',l.render(f,'precise',cc))
        print('PRESENTATION: there is at most/exactly one; plural count nouns; atomic and negated-predicate bodies unwrapped')

    def test15_fixture_golden(self):
        c=l.load(CORPUS/'dex_k.json');n=l.syntax(c.data['formula'])
        self.assertEqual(l.fixture_text(n,c,c.data['fixtures'][0]),
            'For example K1: S(t0) is true; x(t0) = 10000, x′(t0) = 11000, y(t0) = 10000, y′(t0) = 9094. Verdict: true. '
            'Accepted exact-in arithmetic; other acceptance premises assumed. Domains: T = {t0}.')
        count=0
        for c,p in corpus():
            n=l.syntax(c.data['formula'])
            for f in c.data['fixtures']:
                text=l.fixture_text(n,c,f);self.assertNotIn('"',text)
                self.assertIn('Verdict: '+str(f['expected']).lower()+'.',text)
                for domain,values in f['domains'].items():
                    self.assertIn(domain+' = {'+', '.join(map(str,values))+'}',text)
                count+=1
        print(f'PRESENTATION: K1 golden and {count} readable fixture lines; explicit domains, default values, notes and verdicts')

    def test16_required_count_grouping(self):
        c=l.Context.from_data(small_data())
        a=l.syntax('#{v in D: 0 = 0}+1 = 1');b=l.syntax('#{v in D: 0 = 0+1} = 1')
        ambiguous='The number of members v such that 0 equals 0 plus 1 equals 1.'
        # Removing the typed body boundary admits two meanings. The accepted
        # grammar instead requires grouping for these compound count bodies.
        rules=tuple(r for r in l.grammar(c,'precise') if r.lhs!='@COUNT_BODY')+(l.Rule('@COUNT_BODY',('@F',),lambda v:v[0]),)
        with self.assertRaisesRegex(l.Refusal,'ambiguous'):
            l.admit(l.earley(l.tokenize(ambiguous)[:-1],rules,sentence_case=True),c)
        with self.assertRaises(l.Refusal):l.parse(ambiguous,'precise',c)
        self.roundtrip(a,c);self.roundtrip(b,c)
        self.assertNotEqual(l.render(a,'precise',c),l.render(b,'precise',c))
        self.assertIn('such that (0 equals 0)',l.render(a,'precise',c))
        x=l.syntax('0 > 0+#{v in D: P(v) or P(1)}')
        y=l.syntax('(0 > 0+#{v in D: P(v)}) or P(1)')
        self.roundtrip(x,c);self.roundtrip(y,c)
        self.assertNotEqual(l.render(x,'precise',c),l.render(y,'precise',c))
        print('PRESENTATION: ungrouped count comparison has two meanings; required grouping retained for owner review')


if __name__=='__main__':unittest.main()
