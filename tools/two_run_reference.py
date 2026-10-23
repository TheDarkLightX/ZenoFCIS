#!/usr/bin/env python3
"""Independent small-domain oracle for two-run tests.

LLM-generated from the two-run specification and CONTRACT_RULES documentation.
This interpreter does not call or translate the Rust compiler/evaluator. It
uses Python integer arithmetic, an expression parser, and all-pairs comparison
(including for noninterference). It supports the generated corpus subset, not
all contract syntax; unsupported syntax raises an error. No sampling of inputs.
"""
import argparse
import itertools
import json
import random
import re
from pathlib import Path

MIN_I128, MAX_I128 = -(2**127), 2**127 - 1


def fit(n):
    if not MIN_I128 <= n <= MAX_I128:
        raise ArithmeticError('overflow')
    return n


class Expression:
    precedence = {'->': 1, '||': 2, '&&': 3, '==': 4, '!=': 4,
                  '<': 5, '<=': 5, '>': 5, '>=': 5, '+': 6, '-': 6, '*': 7}

    def __init__(self, text):
        if type(text) in (int, bool):
            self.tree = ('literal', int(text))
            return
        pattern = r'\s*(->|\|\||&&|==|!=|<=|>=|[!+*(),<>-]|[0-9]+|[A-Za-z_][\w.]*)'
        self.tokens = []
        pos = 0
        while pos < len(text):
            m = re.match(pattern, text[pos:])
            if not m:
                if not text[pos:].strip():
                    break
                raise ValueError(f'unsupported expression: {text[pos:]}')
            self.tokens.append(m[1]); pos += m.end()
        self.index = 0
        self.tree = self.parse(1)
        if self.index != len(self.tokens):
            raise ValueError('trailing tokens')

    def take(self):
        t = self.tokens[self.index]; self.index += 1
        return t

    def parse(self, level):
        t = self.take()
        if t in ('!', '-'):
            left = (t, self.parse(8))
        elif t == '(':
            left = self.parse(1)
            assert self.take() == ')'
        elif t.isdigit() or t in ('true', 'false'):
            left = ('literal', int(t) if t.isdigit() else int(t == 'true'))
        elif self.index < len(self.tokens) and self.tokens[self.index] == '(':
            self.take(); args = [self.parse(1)]
            while self.tokens[self.index] == ',':
                self.take(); args.append(self.parse(1))
            assert self.take() == ')'
            left = ('call', t, *args)
        else:
            left = ('name', t)
        while self.index < len(self.tokens):
            op = self.tokens[self.index]; p = self.precedence.get(op, 0)
            if p < level:
                break
            self.take(); right = self.parse(p if op == '->' else p + 1)
            left = (op, left, right)
        return left

    def evaluate(self, env, variables):
        def run(t):
            op = t[0]
            if op == 'literal': return fit(t[1])
            if op == 'name':
                if t[1] in env: return env[t[1]]
                return variables[t[1]].evaluate(env, variables)
            if op == 'call':
                args = [run(a) for a in t[2:]]  # eager even for choose
                name = t[1]
                if name == 'choose': return args[1] if args[0] == 1 else args[2]
                if name in ('div_floor', 'div_ceil'):
                    a, b = args
                    if b == 0: raise ArithmeticError('division by zero')
                    return fit(a // b if name == 'div_floor' else -(-a // b))
                if name in ('mul_div_floor', 'mul_div_ceil'):
                    a, b, d = args
                    if a < 0 or b < 0 or d <= 0: raise ArithmeticError('fused domain')
                    return fit((a*b)//d if name.endswith('floor') else -(-(a*b)//d))
                if name == 'products_ge':
                    n = len(args)//2
                    if any(a < 0 for a in args): raise ArithmeticError('product domain')
                    import math
                    return int(math.prod(args[:n]) >= math.prod(args[n:]))
                raise ValueError(f'unsupported function {name}')
            a = run(t[1])
            if len(t) == 2: return int(a != 1) if op == '!' else fit(-a)
            b = run(t[2])
            if op == '+': return fit(a+b)
            if op == '-': return fit(a-b)
            if op == '*': return fit(a*b)
            return int({'==':a == b,'!=':a != b,'<':a < b,'<=':a <= b,
                        '>':a > b,'>=':a >= b,'&&':a == 1 and b == 1,
                        '||':a == 1 or b == 1,'->':a != 1 or b == 1}[op])
        return run(self.tree)


class Contract:
    def __init__(self, project, policy):
        self.project, self.policy = project, policy
        self.domains, self.boolean = {}, set()
        for tid, kind, rest in re.findall(r'type (\d+) (int|bool|state|command|context|data|destination|payload) \w+([^;]*);', project):
            if kind == 'int':
                lo, hi = map(int, re.search(r'in (-?\d+)\.\.=(\d+)', rest).groups())
                self.domains[int(tid)] = list(range(lo, hi+1))
            elif kind == 'bool':
                self.domains[int(tid)] = [0, 1]; self.boolean.add(int(tid))
        for tid, binding in policy['leaf_bindings'].items():
            if binding[0] == 'Bool':
                self.domains[int(tid)] = [0, 1]; self.boolean.add(int(tid))
            elif binding[0] == 'I128': self.domains[int(tid)] = list(range(binding[1], binding[2]+1))
            elif binding[0] != 'Text': raise ValueError('unsupported leaf')
        for v, tid in re.findall(r'variant (\d+) (\d+) \w+ none;', project):
            self.domains.setdefault(int(tid), []).append(int(v))
        self.fields = {}
        for field, root, name, tid in re.findall(r'field (\d+) (\d+) (\w+) (\d+);', project):
            self.fields.setdefault(int(root), []).append((int(field), int(tid)))
        for fs in self.fields.values(): fs.sort()
        self.names, self.types, factors = [], [], []
        for root, prefix in [(100,'pre'), (101,'command'), (102,'context')]:
            for field, tid in self.fields.get(root, [(None, root)]):
                self.names.append(f'{prefix}.{root}' + (f'.{field}' if field else ''))
                self.types.append(tid); factors.append(self.domains[tid])
        self.width = len(self.fields[100])
        self.states = list(itertools.product(*factors[:self.width]))
        self.inputs = list(itertools.product(*factors[self.width:]))
        self.variables = {k:Expression(v) for k,v in policy['variables'].items()}
        self.cases = [(Expression(c['when']),c,{k:Expression(v) for k,v in c['post'].items()},
                       [{k:Expression(v) for k,v in d['payload'].items()} for d in c['outbox']]) for c in policy['cases']]
        self.laws = []
        for lid, scope, formula in re.findall(r'law (\d+) \w+ on ([^=]+?) = ([^;]+);', project):
            self.laws.append((int(lid),scope.strip(),Expression(formula)))
        self.admissible = [s for s in self.states if self.valid(s)]

    def valid(self, state):
        env = {n.replace('pre.', 'post.'):v for n,v in zip(self.names,state)}
        for lid, scope, law in self.laws:
            if 'genesis' in scope and ('commit' in scope or 'any' in scope) and self.policy['law_kinds'][str(lid)] != 'InitialCondition':
                try:
                    if law.evaluate(env, {}) != 1: return False
                except ArithmeticError: return False
        return True

    def outcome(self, state, inp):
        env = dict(zip(self.names, state+inp))
        try:
            evaluated = []
            for condition, case, post, payloads in self.cases:
                evaluated.append((condition.evaluate(env,self.variables),case,
                    {k:v.evaluate(env,self.variables) for k,v in post.items()},
                    [{k:v.evaluate(env,self.variables) for k,v in ps.items()} for ps in payloads]))
        except ArithmeticError:
            return {'class':'Refused','refusal':'arithmetic','law':None}
        _, case, values, payloads = next(c for c in evaluated if c[0] == 1)
        cls = case['class']; successor = []
        outbox = []
        if cls != 'Reject':
            for (fid, tid), old in zip(self.fields[100], state):
                n = values.get(str(fid), old)
                if n not in self.domains[tid]: return {'class':'Refused','refusal':'domain','law':None}
                successor.append(bool(n) if tid in self.boolean else str(n))
            for delivery, payload in zip(case['outbox'],payloads):
                # Payloads are schema values too; invalid values never become deliveries.
                if any(payload[str(fid)] not in self.domains[tid] for fid,tid in self.fields[104]):
                    return {'class':'Refused','refusal':'domain','law':None}
                outbox.append({'ordinal':delivery['ordinal'],'channel':delivery['channel'],
                    'destination':delivery['destination'],'idempotency':str(delivery['idempotency_ordinal']),
                    'payload':{str(fid): bool(payload[str(fid)]) if tid in self.boolean else str(payload[str(fid)]) for fid,tid in self.fields[104]}})
        postenv = dict(env, decision_reason=case['reason'] or 0)
        postenv.update({f'post.100.{fid}':int(n) for (fid,_),n in zip(self.fields[100],successor)})
        for lid, scope, law in self.laws:
            applies = 'any' in scope or ('commit' in scope and cls != 'Reject') or ('accept' in scope and cls == 'Accept') or ('reject' in scope and cls == 'Reject') or ('failure' in scope and cls == 'CommittedFailure')
            if applies:
                try:
                    if law.evaluate(postenv,{}) != 1: return {'class':'Refused','refusal':'law','law':lid}
                except ArithmeticError: return {'class':'Refused','refusal':'law','law':lid}
        return {'class':cls,'reason':case['reason'],'post':{str(fid):n for (fid,_),n in zip(self.fields[100],successor)},'outbox':outbox}

    def after(self, state, outcome):
        if outcome['class'] in ('Reject','Refused'): return state
        return tuple(int(outcome['post'][str(fid)]) for fid,_ in self.fields[100])

    def order(self, first, second, observe, within):
        a,b = Expression(first),Expression(second)
        left = [i for i in self.inputs if a.evaluate(dict(zip(self.names[self.width:],i)),self.variables)==1]
        right = [i for i in self.inputs if b.evaluate(dict(zip(self.names[self.width:],i)),self.variables)==1]
        indices = [self.names.index(p.replace('post.','pre.')) for p in observe]
        comparisons = 0; max_distance = 0; different = False
        for state, i, j in itertools.product(self.admissible,left,right):
            x=self.outcome(state,i); sx=self.after(state,x); y=self.outcome(sx,j); final1=self.after(sx,y)
            v=self.outcome(state,j); sv=self.after(state,v); w=self.outcome(sv,i); final2=self.after(sv,w)
            comparisons += 1
            distances = [abs(final1[k]-final2[k]) for k in indices]
            max_distance = max([max_distance]+distances)
            different |= x['class']!=w['class'] or y['class']!=v['class'] or any(d>within for d in distances)
        return ('refuted' if different else 'checked') if comparisons else 'UNKNOWN', comparisons, max_distance

    def noninterference(self, secret, public):
        if not self.admissible:
            return 'UNKNOWN', 0
        secrets = {self.names.index(p.replace('post.','pre.')) for p in secret}
        public = [p.rsplit('.',1)[1] for p in public]
        different = False; comparisons = 0
        # Deliberately quadratic, independent of the optimized Rust grouping.
        for inp in self.inputs:
            for a,b in itertools.combinations(self.admissible,2):
                if any(x!=y for k,(x,y) in enumerate(zip(a,b)) if k not in secrets): continue
                comparisons += 1
                def signature(state):
                    o=self.outcome(state,inp)
                    return (o['class'],o.get('reason',o.get('refusal')),o.get('law'),tuple(o.get('post',{}).get(f) for f in public),o.get('outbox',[]))
                different |= signature(a)!=signature(b)
        return ('refuted' if different else 'checked'), comparisons


def main():
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--corpus',type=Path,required=True)
    ap.add_argument('--random',type=int,default=320)
    ap.add_argument('--seed',type=int,default=26022)
    args=ap.parse_args()
    from two_run_test_data import contracts
    totals={'contracts':0,'random_contracts':args.random,'input_outcomes':0,'order_pairs':0,'ni_pairs':0,'seed':args.seed,'examples':{}}
    args.corpus.parent.mkdir(parents=True,exist_ok=True)
    with args.corpus.open('w') as stream:
        for name, project, policy, questions in contracts(random.Random(args.seed),args.random):
            c=Contract(project,policy)
            outcomes=[{'tuple':[str(n) for n in s+i],'outcome':c.outcome(s,i)} for s,i in itertools.product(c.states,c.inputs)]
            answers=[]
            for q in questions:
                if q['kind']=='commute':
                    verdict,n,d=c.order(q['first'],q['second'],q['observe'],q['within']);totals['order_pairs']+=n
                    detail={'maximum_observed_distance':d}
                else:
                    verdict,n=c.noninterference(q['secret'],q['public']);totals['ni_pairs']+=n;detail={}
                answers.append(dict(q,verdict=verdict,**detail))
            row={'name':name,'project':project,'policy':json.dumps(policy),'outcomes':outcomes,'questions':answers,'admissible_states':len(c.admissible)}
            stream.write(json.dumps(row)+'\n')
            totals['contracts']+=1;totals['input_outcomes']+=len(outcomes)
            if name.startswith('example:'): totals['examples'][name]=answers
    print(json.dumps(totals,indent=2))

if __name__=='__main__': main()
