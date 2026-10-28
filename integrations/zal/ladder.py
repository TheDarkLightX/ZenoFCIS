"""Experimental zal/ladder/0. Deterministic presentation only; authority: none.

Tuple trees and recursively frozen contexts own their data. Parser charts, rule
lists and evaluator bindings are invocation-local scratch, never retained. The
fixture evaluator checks stipulated finite tables, not machine reachability.
English grammar rules invert typed template slots; no target-formula lookup.
"""
from __future__ import annotations

import argparse
from dataclasses import dataclass
from functools import lru_cache
from itertools import product
from collections import defaultdict, deque
from collections.abc import Mapping
import json
from pathlib import Path
import re
from string import Formatter
import sys
from types import MappingProxyType

PROFILE = 'zal/ladder/0'
TEMPORAL_PROFILE = 'zal/ladder-temporal/1'
TEMPORAL_THEORY = 'ordered-events-over-integers/1'
TEMPORAL = ('always', 'eventually', 'next', 'until', 'within')
RUNGS = ('formula', 'typed', 'formal', 'precise', 'intuitive', 'plain', 'question')
COMPARE = ('=', '!=', '<', '<=', '>', '>=')
LOGIC = ('and', 'or', '=>', '<=>')
ARITH = ('+', '-', '*', 'floor', 'ceil')
BIND = ('forall', 'exists', 'exists1', 'count')
PREC = {'<=>': 1, '=>': 2, 'or': 3, 'and': 4, 'not': 5,
        **{k: 6 for k in COMPARE}, '+': 7, '-': 7, '*': 8}
NAME = r'[A-Za-z_][A-Za-z_0-9]*'
MAX_TEXT, MAX_TOKENS, MAX_NODES, MAX_DEPTH = 2000, 400, 1024, 80
EARLEY_MAX_ITEMS = 250000

# Exact identifier aliases, using the usual mathematical epsilon/phi variants.
# The canonical trees and declarations retain the ASCII keys.
GREEK_IDENTIFIERS = MappingProxyType({
    **dict(zip('alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega'.split(),
               'αβγδϵζηθικλμνξοπρστυϕχψω', strict=True)),
    **dict(zip('Alpha Beta Gamma Delta Epsilon Zeta Eta Theta Iota Kappa Lambda Mu Nu Xi Omicron Pi Rho Sigma Tau Upsilon Phi Chi Psi Omega'.split(),
               'ΑΒΓΔΕΖΗΘΙΚΛΜΝΞΟΠΡΣΤΥΦΧΨΩ', strict=True)),
    'varepsilon': 'ε', 'varphi': 'φ', 'vartheta': 'ϑ',
})
ASCII_GREEK_IDENTIFIERS = MappingProxyType({glyph: name for name, glyph in GREEK_IDENTIFIERS.items()})


def formula_identifier(name):
    return GREEK_IDENTIFIERS.get(name, name)


class Refusal(ValueError):
    """A named syntax/context/model refusal; exhaustion is UNKNOWN."""


def freeze(x):
    if isinstance(x, Mapping):
        return MappingProxyType({k: freeze(v) for k, v in x.items()})
    return tuple(map(freeze, x)) if isinstance(x, (list, tuple)) else x


REGISTRY = freeze(json.loads(Path(__file__).with_name('ladder_en.json').read_text()))


def theory_info(data):
    name=data.get('theory')
    if not isinstance(name,str) or name not in REGISTRY['theories']:
        raise Refusal('unsupported-theory: '+str(name))
    return REGISTRY['theories'][name]


def children(n):
    if n[0] == 'within':
        return (n[2],)
    if n[0] in TEMPORAL:
        return n[1:]
    if n[0] == 'call':
        return n[3]
    if n[0] in BIND:
        return (n[3],)
    return n[1:] if n[0] in (*ARITH, *COMPARE, *LOGIC, 'not') else ()


def rebuild(n, cs):
    if n[0] == 'within':
        return ('within', n[1], cs[0])
    if n[0] in TEMPORAL:
        return (n[0], *cs)
    if n[0] == 'call':
        return (*n[:3], tuple(cs))
    if n[0] in BIND:
        return (*n[:3], cs[0])
    return (n[0], *cs) if children(n) else n


def bounded(n):
    stack, count = [(n, 1)], 0
    while stack:
        node, depth = stack.pop()
        count += 1
        if count > MAX_NODES or depth > MAX_DEPTH:
            raise Refusal('UNKNOWN:tree-budget')
        stack.extend((c, depth + 1) for c in children(node))
    return n


def free(n):
    if n[0] == 'var':
        return {n[1]}
    names = set().union(*(free(c) for c in children(n)))
    return names - {n[1]} if n[0] in BIND else names


def fresh(used, stem='w'):
    i, name = 0, stem
    while name in used:
        i += 1
        name = f'{stem}{i}'
    return name


def names_in(n):
    return free(n) | ({n[1]} if n[0] in BIND else set()) | set().union(*(names_in(c) for c in children(n)))


def substitute(n, env):
    if n[0] == 'var':
        return env.get(n[1], n)
    if n[0] in BIND:
        v, domain, body = n[1:]
        rest = {k: val for k, val in env.items() if k != v}
        avoid = set().union(*(free(val) for val in rest.values()))
        if v in avoid:
            w = fresh(names_in(body) | avoid | set(rest), v)
            body = substitute(body, {v: ('var', w)})
            v = w
        return (n[0], v, domain, substitute(body, rest))
    return rebuild(n, [substitute(c, env) for c in children(n)])


def prime(n, ctx):
    cs = [prime(c, ctx) for c in children(n)]
    n = rebuild(n, cs)
    if n[0] == 'call' and is_state(n[1], ctx):
        return ('call', n[1], True, n[3])
    return n


@dataclass(frozen=True, eq=False)
class Context:
    data: object
    definitions: tuple

    def __post_init__(self):
        object.__setattr__(self,'data',freeze(self.data))
        object.__setattr__(self,'definitions',freeze(self.definitions))

    @classmethod
    def from_data(cls, data):
        data = freeze(data)
        theory_info(data)
        expected_profile = TEMPORAL_PROFILE if data['theory'] == TEMPORAL_THEORY else PROFILE
        if 'profile' in data and data['profile'] != expected_profile:
            raise Refusal('context:profile-theory-mismatch')
        if data['theory'] == TEMPORAL_THEORY:
            check_temporal_context(data)
        if data['kind'] not in ('law', 'invariant') or data['authority']['status'] not in ('DECIDED', 'OPEN', 'DELEGATED'):
            raise Refusal('context:kind-or-authority')
        for name, d in data['domains'].items():
            if not re.fullmatch(NAME, name) or not d['member'] or not d['plural']:
                raise Refusal('context:domain')
        defs = []
        ctx = cls(data, ())
        all_names = [d['name'] for d in data['definitions']]
        if len(set(all_names)) != len(all_names) or set(all_names) & set(data['symbols']):
            raise Refusal('context:duplicate-symbol')
        # Definitions may refer forward. Expansion checks cycles separately.
        for d in data['definitions']:
            defs.append((d['name'], d, syntax(d['body'], data['theory']==TEMPORAL_THEORY)))
        ctx = cls(data, tuple(defs))
        for name, s in data['symbols'].items():
            if not re.fullmatch(NAME, name) or s['arity'] != len(s['args']):
                raise Refusal('context:symbol-arity')
            if any(d not in data['domains'] for d in s['args']):
                raise Refusal('context:argument-domain')
            if s['kind'] not in ('function', 'predicate'):
                raise Refusal('context:symbol-kind')
            if s['kind'] == 'predicate' and s.get('state'):
                raise Refusal('context:state-predicate-needs-integer-definition')
            if s['kind'] == 'predicate':
                slots = [field for _, field, _, _ in Formatter().parse(s['clause']) if field is not None]
                if slots != [str(i) for i in range(s['arity'])] or (s['arity'] and not s['clause'].startswith('{0}')) or (not s['arity'] and data['theory'] != TEMPORAL_THEORY):
                    raise Refusal('context:subject-slot-first')
        for name, d, body in ctx.definitions:
            params = {p['name']: p['domain'] for p in d['params']}
            if len(params) != len(d['params']) or any(v not in data['domains'] for v in params.values()):
                raise Refusal('context:definition-parameters')
            expanded = expand(body, ctx, (name,))
            got = validate(expanded, ctx, params)
            if got != ('bool' if d.get('clause') else 'int'):
                raise Refusal('context:definition-type')
        return ctx

    def definition(self, name):
        return next(((d, b) for nm, d, b in self.definitions if nm == name), None)

    def symbol(self, name):
        if name in self.data['symbols']:
            return self.data['symbols'][name]
        d = self.definition(name)
        if d:
            meta, _ = d
            return freeze({**dict(meta), 'args': [p['domain'] for p in meta['params']],
                           'arity': len(meta['params']), 'kind': 'predicate' if meta.get('clause') else 'function'})
        raise Refusal('unknown-symbol:' + name)

    def domain(self, name):
        if name not in self.data['domains']:
            raise Refusal('unknown-domain:' + name)
        return self.data['domains'][name]


def load(path):
    return Context.from_data(json.loads(Path(path).read_text()))


def expand(n, ctx, stack=(), derived=True):
    if n[0] == 'call' and (definition := ctx.definition(n[1])):
        if n[1] in stack:
            raise Refusal('context:cyclic-definition')
        d, body = definition
        if len(n[3]) != len(d['params']):
            raise Refusal('type:arity')
        body = expand(body, ctx, (*stack, n[1]), derived)
        if n[2]:
            if prime(body, ctx) == body:
                raise Refusal('type:prime-needs-state')
            body = prime(body, ctx)
        args = [expand(a, ctx, stack, derived) for a in n[3]]
        return bounded(substitute(body, dict(zip((p['name'] for p in d['params']), args))))
    n = rebuild(n, [expand(c, ctx, stack, derived) for c in children(n)])
    if n[0] == 'within' and derived:
        check_within(n[1], ctx)
        result = n[2]
        for _ in range(n[1]):
            result = bounded(('or', n[2], ('next', result)))
        return result
    if n[0] == 'exists1' and derived:
        _, v, dom, body = n
        w = fresh(names_in(body) | {v})
        other = substitute(body, {v: ('var', w)})
        return ('exists', v, dom, ('and', body, ('forall', w, dom, ('=>', other, ('=', ('var', w), ('var', v))))))
    return bounded(n)


def validate(n, ctx, env=None):
    if env is None and ctx.data['theory'] == TEMPORAL_THEORY:
        check_temporal_formula(n, ctx)
    env = {} if env is None else env
    tag = n[0]
    if tag == 'int':
        return 'int'
    if tag == 'var':
        if n[1] not in env:
            raise Refusal('free-variable:' + n[1])
        d = env[n[1]]
        return 'int' if ctx.domain(d).get('integer') else d
    if tag in BIND:
        ctx.domain(n[2])
        if validate(n[3], ctx, {**env, n[1]: n[2]}) != 'bool':
            raise Refusal('type:binder-body')
        return 'int' if tag == 'count' else 'bool'
    if tag == 'call':
        s = ctx.symbol(n[1])
        if len(n[3]) != s['arity']:
            raise Refusal('type:arity')
        for arg, domain in zip(n[3], s['args']):
            expected = 'int' if ctx.domain(domain).get('integer') else domain
            if validate(arg, ctx, env) != expected:
                raise Refusal('type:argument-domain')
        if n[2] and not s.get('state'):
            d = ctx.definition(n[1])
            if not d or prime(expand(d[1], ctx), ctx) == expand(d[1], ctx):
                raise Refusal('type:prime-needs-state')
        if n[2] and s['kind'] == 'predicate' and ctx.definition(n[1]) is None:
            raise Refusal('type:state-predicate-needs-integer-definition')
        return 'bool' if s['kind'] == 'predicate' else 'int'
    kinds = [validate(c, ctx, env) for c in children(n)]
    if tag in TEMPORAL and ctx.data['theory'] == TEMPORAL_THEORY and all(k == 'bool' for k in kinds):
        if len(kinds) == (2 if tag == 'until' else 1):
            return 'bool'
    if tag in (*LOGIC, 'not') and all(k == 'bool' for k in kinds):
        return 'bool'
    if tag in ARITH and kinds == ['int', 'int']:
        return 'int'
    if tag in COMPARE and (kinds == ['int', 'int'] or tag in ('=', '!=') and kinds[0] == kinds[1] and kinds[0] != 'bool'):
        return 'bool'
    raise Refusal('type:constructor:' + tag)


def normal(n, ctx):
    """Only definition/exists1 expansion, alpha, AC logic, orientation, ¬¬."""
    def walk(n, bound):
        tag = n[0]
        if tag == 'var':
            return ('bound', bound[::-1].index(n[1])) if n[1] in bound else n
        if tag in BIND:
            return (tag, n[2], walk(n[3], (*bound, n[1])))
        cs = [walk(c, bound) for c in children(n)]
        if tag == 'not' and cs[0][0] == 'not':
            return cs[0][1]
        if tag in ('and','or'):
            flat = [v for c in cs for v in (c[1:] if c[0] == tag else (c,))]
            return (tag, *sorted(flat, key=repr))
        if tag in ('>', '>='):
            return ('<' if tag == '>' else '<=', cs[1], cs[0])
        if tag in ('=', '!='):
            return (tag, *sorted(cs, key=repr))
        return rebuild(n, cs)
    return walk(expand(n, ctx), ())


def first_difference(a, b, path='root'):
    if a == b:
        return None
    if not isinstance(a, tuple) or not isinstance(b, tuple) or len(a) != len(b) or a[0] != b[0]:
        return path, a, b
    for i, (x, y) in enumerate(zip(a[1:], b[1:]), 1):
        if x != y:
            return first_difference(x, y, f'{path}/{i}')


def article(word, override=None, variable=False):
    vowel = word.lower() in set('aefhilmnorsx') if variable and len(word) == 1 else word[0].lower() in 'aeiou'
    return override or ('an' if vowel else 'a')


def sentence(text):
    return text[0].upper() + text[1:] + '.'


def precedence(n):
    return 0 if n[0] in BIND and n[0] != 'count' else PREC.get(n[0], 9)


def needs_group(child, parent, side):
    ct, pt = child[0], parent[0]
    if ct in ('and','or') and pt in ('and','or') and ct != pt or ct == pt == '=>':
        return True
    if pt in ('floor','ceil') and ct not in ('int','var','call'):
        return True
    if ct in COMPARE and pt in COMPARE:
        return True
    if precedence(child) < precedence(parent):
        return True
    return side == 1 and precedence(child) == precedence(parent) and pt in (*ARITH, *LOGIC) and ct in (*ARITH, *LOGIC)


def fold(n, ctx):
    return _fold(expand(n,ctx,derived=False),ctx)


def _fold(n, ctx):
    """Leftmost-outermost, definition registry order, modulo binder alpha only."""
    def match(pattern, target, params, bindings, bound=None):
        bound = {} if bound is None else bound
        if pattern[0] == 'var' and pattern[1] in params and pattern[1] not in bound:
            old = bindings.setdefault(pattern[1], target)
            return old == target
        if pattern[0] == 'var':
            return target == ('var', bound.get(pattern[1], pattern[1]))
        if pattern[0] != target[0]:
            return False
        if pattern[0] in BIND:
            return pattern[2] == target[2] and match(pattern[3], target[3], params, bindings, {**bound, pattern[1]:target[1]})
        if pattern[0] == 'call' and pattern[1:3] != target[1:3]:
            return False
        pc, tc = children(pattern), children(target)
        return (pattern == target if not pc else len(pc) == len(tc) and all(match(a,b,params,bindings,bound) for a,b in zip(pc,tc)))
    for name, d, body in ctx.definitions:
        params = [p['name'] for p in d['params']]
        body = expand(body, ctx, derived=False)
        for primed in (False, True):
            # Boolean clauses do not have an after-state template. Keep their
            # primed integer expansion visible instead of dropping the prime.
            if primed and (d.get('clause') or prime(body, ctx) == body):
                continue
            bindings = {}
            if match(prime(body, ctx) if primed else body, n, params, bindings) and all(p in bindings for p in params):
                args = tuple(bindings[p] for p in params)
                result = ('call', name, primed, args)
                # A parameter cannot escape a binder in the matched body.
                if alpha(expand(result, ctx)) == alpha(expand(n, ctx)):
                    return result
    return rebuild(n, [_fold(c, ctx) for c in children(n)])


def alpha(n):
    def walk(node,bound):
        if node[0]=='var':
            return ('bound',bound[::-1].index(node[1])) if node[1] in bound else node
        if node[0] in BIND:
            return (node[0],node[2],walk(node[3],(*bound,node[1])))
        return rebuild(node,[walk(c,bound) for c in children(node)])
    return walk(n,())


def unary_clause(n, v, ctx):
    if n[0] != 'call' or n[2] or n[3] != (('var', v),):
        return None
    s = ctx.symbol(n[1])
    return s if s['kind'] == 'predicate' else None


def quantity_ok(n, v, ctx):
    return free(n) <= {v} and prime(n, ctx) != n and all(c[0] != 'call' or len(c[3]) == 1 and c[3][0] == ('var',v) for c in walk_nodes(n))


def walk_nodes(n):
    yield n
    for c in children(n):
        yield from walk_nodes(c)


def idiom(n, ctx, show, quantity):
    """Exact shape registry. First matching production wins; no algebra here."""
    tag = n[0]
    for rule in REGISTRY['idioms']:
        number, slots = rule['id'], None
        if tag == 'forall' and n[3][0] == '=>':
            v, dom, (_, guard, body) = n[1:]
            p = unary_clause(guard, v, ctx)
            if not p or not p.get('noun'):
                continue
            noun = p['noun']
            if number == 3 and body[0] == '>=':
                after, before = body[1:]
                if quantity_ok(before, v, ctx) and after == prime(before, ctx):
                    slots = {'noun':noun, 'quantity':quantity(before, v)}
            if number in (4,5):
                c = unary_clause(body[1] if number == 5 and body[0] == 'not' else body, v, ctx)
                if c and (number == 4 or body[0] == 'not'):
                    slots = {'noun':noun, 'phrase':c['clause'].format('')[1:]}
                    # Strip the one separating space after the subject slot.
                    slots['phrase'] = c['clause'].removeprefix('{0} ').format()
            if number == 1:
                slots = {'noun':noun, 'v':v, 'body':REGISTRY['templates']['precise']['group'].format(a=show(body)) if body[0]=='=>' else show(body)}
        if number == 2 and tag == 'exists' and n[3][0] == 'and':
            p = unary_clause(n[3][1], n[1], ctx)
            if p and p.get('noun'):
                slots = {'noun':p['noun'], 'v':n[1], 'article':article(p['noun'],p.get('article')), 'body':show(n[3][2])}
        if number == 6 and tag == ('<=' if rule['name'] == 'at most one' else '=') and n[1][0] == 'count' and n[2] == ('int',1):
            _, v, dom, body = n[1]
            slots = {'member':ctx.domain(dom)['member'], 'v':v, 'body':show(body)}
        if number == 7 and tag == 'forall' and n[3][0] == 'forall':
            _, w, dom2, body = n[3]
            slots = {'member':ctx.domain(n[2])['member'], 'v':n[1], 'member2':ctx.domain(dom2)['member'], 'w':w,'body':show(body)}
        if number == 8 and tag == 'exists':
            p = unary_clause(n[3], n[1], ctx)
            if p:
                d = ctx.domain(n[2])
                slots = {'article':article(d['member'],d.get('article')), 'member':d['member'], 'domain_noun':d.get('noun',d['plural']), 'phrase':p['clause'].removeprefix('{0} ')}
        if slots is not None:
            return rule['template'].format(**slots)
    return None


def count_body_needs_group(n):
    while n[0]=='not':n=n[1]
    return n[0]!='call'


def body_text(n, rung, ctx, drop=None):
    mode = 'precise' if rung in ('intuitive','plain') else rung
    templates = REGISTRY['templates'][mode]
    def show(c):
        return body_text(c, rung, ctx, drop)
    def operand(c, side):
        text = show(c)
        return templates['group'].format(a=text) if needs_group(c,n,side) or rung in ('intuitive','plain') and drop is None and idiom(c,ctx,show,lambda e,v:body_text(e,rung,ctx,v)) is not None else text
    tag = n[0]
    if ctx.data['theory'] == TEMPORAL_THEORY:
        temporal_text = temporal_body(n, rung, ctx, show)
        if temporal_text is not None:
            return temporal_text
    if rung in ('intuitive','plain') and drop is None:
        result = idiom(n, ctx, show, lambda e,v: body_text(e,rung,ctx,v))
        if result is not None:
            return result
    if tag == 'int':
        return templates['int'].format(value=n[1])
    if tag == 'var':
        return formula_identifier(n[1]) if mode == 'formula' else n[1]
    if tag in BIND:
        d = ctx.domain(n[2])
        art = article(n[1],variable=True) if mode == 'formal' else article(d['member'],d.get('article'))
        body=show(n[3])
        if tag=='count' and mode=='precise' and count_body_needs_group(n[3]):
            body=templates['group'].format(a=body)
        return templates[tag].format(v=formula_identifier(n[1]) if mode == 'formula' else n[1],
                                     domain=formula_identifier(n[2]) if mode == 'formula' else n[2],
                                     member=d['member'], plural=d['plural'], article=art, body=body)
    if tag == 'call':
        name, primed, args = n[1:]
        s = ctx.symbol(name)
        rendered = [templates['group'].format(a=show(a)) if mode not in ('formula','typed') and a[0] not in ('var','int','call') else show(a) for a in args]
        if mode in ('formula','typed'):
            return templates['prime_call' if primed else 'call'].format(
                name=formula_identifier(name) if mode == 'formula' else name, args=', '.join(rendered))
        if mode == 'formal':
            key = 'atom' if s['kind'] == 'predicate' else 'prime_call' if primed else 'call'
            if not args:
                key = 'zero_atom' if s['kind'] == 'predicate' else 'zero_call'
            if not args and primed and s['kind']=='function' and ctx.data['theory']==TEMPORAL_THEORY:
                return name+' prime of no arguments'
            return templates[key].format(name=name,args=', '.join(rendered))
        if s['kind'] == 'predicate':
            return s['clause'].format(*rendered)
        if drop is not None and args == (('var',drop),):
            return s['noun'] + (' after' if primed else '')
        state = s.get('state') or primed or ctx.definition(name) and prime(expand(n,ctx),ctx) != expand(n,ctx)
        key = 'prime_state' if primed else 'state' if state else 'function'
        if state and ctx.data['theory'] == TEMPORAL_THEORY:
            key=('zero_prime_state' if primed else 'zero_state') if not args else 'prime_state' if primed else 'state'
            return theory_info(ctx.data)['state_terms'][key].format(noun=s['noun'],args=', '.join(rendered))
        if not args:
            key = 'zero_function'
        return templates[key].format(noun=s['noun'],args=', '.join(rendered))
    if tag == 'not':
        return templates[tag].format(a=operand(n[1],0))
    if tag in ('floor','ceil'):
        return templates[tag].format(a=operand(n[1],0),b=operand(n[2],1))
    return templates[tag].format(a=operand(n[1],0),b=operand(n[2],1))


def negate(n):
    tag = n[0]
    if tag == 'not':
        return n[1]
    if tag in ('forall','exists'):
        return ('exists' if tag == 'forall' else 'forall', n[1],n[2],negate(n[3]))
    if tag in ('and','or'):
        return ('or' if tag == 'and' else 'and',negate(n[1]),negate(n[2]))
    if tag == '=>':
        return ('and',n[1],negate(n[2]))
    if tag in COMPARE:
        return ({'=':'!=','!=':'=','<':'>=','<=':'>','>':'<=','>=':'<'}[tag],n[1],n[2])
    return ('not',n)


def question(n, ctx):
    q = REGISTRY['questions']
    plain = body_text(fold(n,ctx),'plain',ctx)
    registered = registered_reading(n,ctx,'question') if ctx.data['theory']==TEMPORAL_THEORY else None
    inquiry=expand(n,ctx,derived=False) if ctx.data['theory']==TEMPORAL_THEORY else n
    if inquiry[0] == 'exists':
        _, v, dom, target = inquiry
        d = ctx.domain(dom)
        slots = {'article':article(d['member'],d.get('article')), 'member':d['member'], 'v':v,
                 'domain_noun':d.get('noun',d['plural'])}
        task = q['witness' if unary_clause(target,v,ctx) else 'general_witness'].format(**slots,condition=body_text(fold(target,ctx),'plain',ctx))
        main=q['exists'].format(tail=plain.removeprefix('there exists ')) if ctx.data['theory']!=TEMPORAL_THEORY or plain.startswith('there exists ') else q['target'].format(plain=plain)
        return main + '\n' + task
    neg = negate(expand(n,ctx,derived=ctx.data['theory']!=TEMPORAL_THEORY))
    def find_task(m):
        if m[0] != 'exists':
            return q['evaluate'].format(body=body_text(fold(m,ctx),'plain',ctx))
        _, v, dom, body = m
        d = ctx.domain(dom)
        p = unary_clause(body[1],v,ctx) if body[0] == 'and' else None
        slots = {'article':article(d['member'],d.get('article')), 'member':d['member'], 'v':v,
                 'body':body_text(fold(body,ctx),'plain',ctx)}
        key = 'find'
        if p and p.get('noun'):
            key = 'find_restricted'
            slots.update(noun=p['noun'],article=article(p['noun'],p.get('article')),body=body_text(fold(body[2],ctx),'plain',ctx))
        return q[key].format(**slots)
    return (registered or q['target'].format(plain=plain)) + '\n' + q['counter'].format(task=find_task(neg))


def render(n, rung, ctx):
    if rung not in RUNGS:
        raise Refusal('unknown-rung')
    if validate(n,ctx) != 'bool':
        raise Refusal('type:formula-required')
    if rung not in ('formula','typed'):
        n = reading_tree(n, ctx)
    if rung == 'question':
        return question(n,ctx)
    tree = fold(n,ctx) if rung == 'plain' else n
    text = body_text(tree,rung,ctx)
    return text if rung in ('formula','typed') else sentence(text)


def reading_tree(n, ctx):
    """Expand primed Boolean definitions before choosing English grouping or idioms."""
    if n[0] == 'call' and n[2] and ctx.symbol(n[1])['kind'] == 'predicate':
        return reading_tree(expand(n, ctx, derived=False), ctx)
    return rebuild(n, [reading_tree(c, ctx) for c in children(n)])


# Grammar preparation retains immutable productions only. Every Earley chart and
# semantic value set belongs exclusively to one invocation, including refusals.
@dataclass(frozen=True)
class Rule:
    lhs: str
    rhs: tuple
    action: object


def tokenize(text):
    if not isinstance(text, str) or len(text) > MAX_TEXT:
        raise Refusal('input-bound at token 0; expected at most 2000 characters')
    pattern = r"<=>|=>|!=|<=|>=|∃!|[^\W\d]\w*|[0-9]+|[^\s]"
    tokens = tuple(ASCII_GREEK_IDENTIFIERS.get(token, token) for token in re.findall(pattern, text))
    if len(tokens) > MAX_TOKENS:
        raise Refusal('input-bound at token 400; expected at most 400 tokens')
    return tokens


def ungroup(n):
    return n[1] if n[0] == 'group' else n


def clean(n):
    n = ungroup(n)
    return rebuild(n, [clean(c) for c in children(n)])


def earley(tokens, rules, start='@F', sentence_case=False):
    """Complete Earley deduction, retaining every distinct semantic derivation.

    No first-parse return or intermediate tree truncation. Epsilon-free grammar:
    a completion always has an earlier origin chart. Equivalent derivations are
    packed by item+semantic tuple; final admission counts normalized trees.
    """
    by_lhs = defaultdict(list)
    for i, rule in enumerate(rules):
        by_lhs[rule.lhs].append(i)
    chart = [set() for _ in range(len(tokens) + 1)]
    waiting = [defaultdict(set) for _ in chart]
    for i in by_lhs[start]:
        chart[0].add((i, 0, 0, ()))
    items_used=sum(map(len,chart))
    for pos in range(len(chart)):
        queue = deque(chart[pos])
        def put(item, end=pos):
            nonlocal items_used
            if item not in chart[end]:
                items_used+=1
                if items_used>EARLEY_MAX_ITEMS:
                    raise Refusal('UNKNOWN:parse-item-budget')
                chart[end].add(item)
                if end == pos:
                    queue.append(item)
        while queue:
            ri, dot, origin, values = queue.popleft()
            rule = rules[ri]
            if dot == len(rule.rhs):
                try:
                    value = rule.action(values)
                except Refusal:
                    continue
                for wi, wd, wo, wv in tuple(waiting[origin][rule.lhs]):
                    put((wi, wd + 1, wo, (*wv, value)))
                continue
            symbol = rule.rhs[dot]
            if symbol.startswith('@'):
                waiting[pos][symbol].add((ri, dot, origin, values))
                for child in by_lhs[symbol]:
                    put((child, 0, pos, ()))
            elif pos < len(tokens):
                token = tokens[pos]
                matches = (re.fullmatch(NAME, token) if symbol == '$NAME' else
                           token.isascii() and token.isdigit() if symbol == '$INT' else
                           token == symbol or sentence_case and pos == 0 and token == symbol.capitalize())
                if matches:
                    put((ri, dot + 1, origin, (*values, token)), pos + 1)
    results = []
    for ri, dot, origin, values in chart[-1]:
        rule = rules[ri]
        if rule.lhs == start and origin == 0 and dot == len(rule.rhs):
            try:
                results.append(rule.action(values))
            except Refusal:
                pass
    if not results:
        pos = max(i for i, items in enumerate(chart) if items)
        expected = sorted({rules[ri].rhs[dot] for ri, dot, _, _ in chart[pos]
                           if dot < len(rules[ri].rhs) and not rules[ri].rhs[dot].startswith('@')})
        raise Refusal(f'syntax at token {pos}; expected {", ".join(expected) or "end of input / valid grouping"}')
    return results


def binary(tag, a, b):
    for c in (a, b):
        if tag in ('and', 'or') and c[0] in ('and', 'or') and c[0] != tag:
            raise Refusal('missing-grouping')
    return (tag, ungroup(a), ungroup(b))


def symbolic_call(name,primed,args,temporal):
    if temporal and name in (*TEMPORAL,'never','since','previously'):
        raise Refusal('temporal:reserved-name')
    return ('call',name,primed,args)


class Compiler:
    """Template slots compile to typed CFG categories, never regex parsers."""
    def __init__(self, mode, ctx=None, idioms=False, temporal=False):
        self.mode, self.ctx, self.idioms = mode, ctx, idioms
        self.temporal=temporal or ctx is not None and ctx.data['theory']==TEMPORAL_THEORY
        self.rules = []
        self.templates = REGISTRY['templates'][mode]

    def rule(self, lhs, text, slots, action):
        rhs, fields = [], []
        for literal, field, fmt, conv in Formatter().parse(text):
            rhs.extend(tokenize(literal))
            if field is not None:
                value = slots[field]
                if value.startswith('@') or value.startswith('$'):
                    fields.append((field, len(rhs)))
                    rhs.append(value)
                else:
                    rhs.extend(tokenize(value))
        fields=tuple(fields)
        self.rules.append(Rule(lhs, tuple(rhs), lambda vs: action({k: vs[i] for k, i in fields})))

    def constructor(self, lhs, key, slots, action):
        self.rule(lhs, self.templates[key], slots, action)
        # Input aliases use the same typed slots and semantic constructor.
        # Rendering continues to use the canonical template above.
        for alias in REGISTRY.get('input_aliases', {}).get(self.mode, {}).get(key, ()):
            self.rule(lhs, alias, slots, action)

    def link(self, lhs, rhs):
        self.rules.append(Rule(lhs, (rhs,), lambda vs: vs[0]))

    def compile(self):
        t = self.templates
        for i in range(1, 7):
            self.link('@F' if i == 1 else f'@F{i-1}', f'@F{i}')
        self.link('@T', '@T7')
        self.link('@T7', '@T8')
        self.link('@T8', '@T9')
        primary='@T9' if self.mode in ('formula','typed') else '@ARG'
        self.link('@T9',primary) if primary!='@T9' else None
        self.constructor(primary, 'int', {'value':'$INT'}, lambda v: ('int', int(v['value'])))
        self.rule(primary, '-' + t['int'], {'value':'$INT'}, lambda v: ('int', -int(v['value'])))
        self.constructor(primary, 'var', {'name':'$NAME'}, lambda v: ('var', v['name']))
        for cat in ('@F6', primary):
            self.constructor(cat, 'group', {'a':'@F' if cat == '@F6' else '@T'}, lambda v: ('group', v['a']))
        for tag, lhs, left, right in (('<=>','@F1','@F1','@F2'), ('=>','@F2','@F3','@F3'),
                ('or','@F3','@F3','@F4'), ('and','@F4','@F4','@F5'),
                ('+','@T7','@T7','@T8'), ('-','@T7','@T7','@T8'), ('*','@T8','@T8','@T9')):
            self.constructor(lhs, tag, {'a':left,'b':right}, lambda v, op=tag: binary(op,v['a'],v['b']))
        self.constructor('@F5','not',{'a':'@F5'},lambda v: ('not',ungroup(v['a'])))
        for tag in COMPARE:
            self.constructor('@F6', tag, {'a':'@T','b':'@T'}, lambda v,op=tag: (op,ungroup(v['a']),ungroup(v['b'])))
        for tag in ('floor','ceil'):
            self.constructor('@T9',tag,{'a':'@T' if self.mode in ('formula','typed') else '@ARG','b':'@T' if self.mode in ('formula','typed') else '@ARG'},lambda v,op=tag: (op,ungroup(v['a']),ungroup(v['b'])))
        if self.temporal:
            self.compile_temporal()
        if self.mode in ('formula','typed'):
            for tag in BIND:
                self.constructor('@T9' if tag == 'count' else '@F', tag,
                    {'v':'$NAME','domain':'$NAME','body':'@F'},
                    lambda v,op=tag: (op,v['v'],v['domain'],ungroup(v['body'])))
            self.link('@ARGS','@T')
            self.rule('@ARGS','{head}, {tail}',{'head':'@T','tail':'@ARGS'},lambda v:(v['head'],v['tail']))
            def args(n):
                return (ungroup(n),) if isinstance(n[0],str) else (ungroup(n[0]),*args(n[1]))
            for primed in (False,True):
                key = 'prime_call' if primed else 'call'
                self.constructor('@T9',key,{'name':'$NAME','args':'@ARGS'},
                    lambda v,p=primed,t=self.temporal: symbolic_call(v['name'],p,args(v['args']),t))
                for cat in (('@F6','@TEMP_ATOM') if self.temporal else ('@F6',)):
                    self.constructor(cat,key,{'name':'$NAME','args':'@ARGS'},
                        lambda v,p=primed,t=self.temporal: symbolic_call(v['name'],p,args(v['args']),t))
                for cat in (('@T9','@F6','@TEMP_ATOM') if self.temporal else ('@T9','@F6')):
                    self.constructor(cat,key,{'name':'$NAME','args':''},lambda v,p=primed,t=self.temporal:symbolic_call(v['name'],p,(),t))
            return tuple(self.rules)
        self.link('@F6','@PRED')
        if self.mode=='precise':
            self.link('@COUNT_BODY','@PRED')
            self.constructor('@COUNT_BODY','not',{'a':'@COUNT_BODY'},lambda v:('not',ungroup(v['a'])))
            self.constructor('@COUNT_BODY','group',{'a':'@F'},lambda v:('group',v['a']))
        for domain,d in self.ctx.data['domains'].items():
            for tag in BIND:
                slots={'v':'$NAME','domain':domain,'member':d['member'],'plural':d['plural'],'body':'@COUNT_BODY' if tag=='count' and self.mode=='precise' else '@F','article':'a'}
                articles = ('a','an') if self.mode == 'formal' and tag == 'exists' else (article(d['member'],d.get('article')),)
                for art in articles:
                    self.constructor('@T9' if tag == 'count' else '@F',tag,{**slots,'article':art},
                        lambda v,op=tag,dom=domain: (op,v['v'],dom,ungroup(v['body'])))
        for name in (*self.ctx.data['symbols'],*(d[0] for d in self.ctx.definitions)):
            s=self.ctx.symbol(name)
            for primed in (False,True):
                if primed and (s['kind']=='predicate' or not is_state(name,self.ctx)):
                    continue
                cat='@PRED' if s['kind']=='predicate' else '@ARG'
                slots={str(i):'@ARG' for i in range(s['arity'])}
                # Symbolic calls delimit full term arguments; English calls use
                # primary slots, so compound arguments carry explicit grouping.
                if self.mode == 'formal':
                    key='atom' if s['kind']=='predicate' else 'prime_call' if primed else 'call'
                    if not s['arity']:
                        key='zero_atom' if s['kind']=='predicate' else 'zero_call'
                    text=t[key].replace('{name}',name).replace('{args}',', '.join('{'+str(i)+'}' for i in range(s['arity'])))
                    if not s['arity'] and primed and s['kind']=='function' and self.temporal:
                        text=name+' prime of no arguments'
                elif s['kind']=='predicate':
                    text=s['clause']
                else:
                    key='prime_state' if primed else 'state' if is_state(name,self.ctx) else 'function'
                    if not s['arity']:
                        key='zero_function'
                    text=t[key].replace('{noun}',s['noun']).replace('{args}',', '.join('{'+str(i)+'}' for i in range(s['arity'])))
                    if is_state(name,self.ctx) and self.temporal:
                        key=('zero_prime_state' if primed else 'zero_state') if not s['arity'] else 'prime_state' if primed else 'state'
                        text=theory_info(self.ctx.data)['state_terms'][key].replace('{noun}',s['noun']).replace('{args}',', '.join('{'+str(i)+'}' for i in range(s['arity'])))
                self.rule(cat,text,slots,lambda v,nm=name,p=primed,k=s['arity']:('call',nm,p,tuple(ungroup(v[str(i)]) for i in range(k))))
        if self.idioms:
            self.compile_idioms()
        return tuple(self.rules)

    def compile_temporal(self):
        templates = REGISTRY['temporal_templates'][self.mode]
        for tag in TEMPORAL:
            slots = {'a':'@F', 'b':'@F', 'k':'$INT'}
            def node(v, op=tag):
                if op == 'within':
                    return (op, int(v['k']), ungroup(v['a']))
                return (op, ungroup(v['a']), ungroup(v['b'])) if op == 'until' else (op, ungroup(v['a']))
            self.rule('@F6', templates[tag], slots, node)
        if self.mode in ('formula','typed'):
            self.rule('@F6', 'never({a})', {'a':'@F'}, lambda v:('always',('not',ungroup(v['a']))))
            for tag in ('always','eventually','next'):
                prefix={'always':'□','eventually':'◇','next':'○'}[tag] if self.mode=='formula' else tag+' '
                self.rule('@F6',prefix+'{a}',{'a':'@TEMP_ATOM'},lambda v,op=tag:(op,ungroup(v['a'])))
            self.rule('@F6','never {a}',{'a':'@TEMP_ATOM'},lambda v:('always',('not',ungroup(v['a']))))
            if self.mode=='formula':
                self.rule('@F6','({a} U {b})',{'a':'@F','b':'@F'},lambda v:('until',ungroup(v['a']),ungroup(v['b'])))
        if self.mode == 'precise':
            self.rule('@F2','provided {a}, {b}',{'a':'@F3','b':'@F3'},lambda v:('=>',ungroup(v['a']),ungroup(v['b'])))
        if self.idioms:
            # Exact registered full-property names, independent of the target.
            for reading in self.ctx.data.get('readings', ()):
                if not reading_applies(reading,self.ctx):
                    continue
                text = reading.get('plain' if self.idioms == 'plain' else 'intuitive')
                if text:
                    tree = syntax(reading['body'],True)
                    self.rule('@F', text, {}, lambda v, n=tree:n)
            short = REGISTRY['temporal_templates']['intuitive']
            if self.mode == 'precise':
                for tag in TEMPORAL:
                    def node(v, op=tag):
                        return (op,int(v['k']),ungroup(v['a'])) if op=='within' else (op,ungroup(v['a']),ungroup(v['b'])) if op=='until' else (op,ungroup(v['a']))
                    self.rule('@F6',short[tag],{'a':'@F','b':'@F','k':'$INT'},node)
                self.rule('@F6',short['never'],{'a':'@F'},lambda v:('always',('not',ungroup(v['a']))))

    def compile_idioms(self):
        ctx=self.ctx
        for rule in REGISTRY['idioms']:
            num=rule['id']
            if num in (1,2,3,4,5):
                for name in (*ctx.data['symbols'],*(d[0] for d in ctx.definitions)):
                    p=ctx.symbol(name)
                    if p['kind']!='predicate' or p['arity']!=1 or not p.get('noun'):
                        continue
                    dom=p['args'][0]
                    def restricted(v,op='forall',nm=name,dm=dom,join='=>'):
                        x=v['v'];return (op,x,dm,(join,('call',nm,False,(('var',x),)),ungroup(v['body'])))
                    base={'noun':p['noun'],'article':article(p['noun'],p.get('article')),'v':'$NAME','body':'@F'}
                    if num in (1,2):
                        self.rule('@F',rule['template'],base,
                            lambda v,op='forall' if num==1 else 'exists',fn=restricted,j='=>' if num==1 else 'and':fn(v,op,join=j))
                    if num in (4,5):
                        for cn in (*ctx.data['symbols'],*(d[0] for d in ctx.definitions)):
                            c=ctx.symbol(cn)
                            if c['kind']!='predicate' or c['args']!=(dom,):
                                continue
                            phrase=c['clause'].removeprefix('{0} ')
                            def every(v,nm=name,cm=cn,dm=dom,negative=num==5):
                                x=fresh(set(), '@implicit');a=('call',nm,False,(('var',x),));b=('call',cm,False,(('var',x),))
                                return ('forall',x,dm,('=>',a,('not',b) if negative else b))
                            self.rule('@F',rule['template'],{**base,'phrase':phrase},every)
                    if num==3:
                        qcat='@Q'+dom
                        def decrease(v,nm=name,dm=dom):
                            before=ungroup(v['quantity']);visible=free(before)-{'@implicit'}
                            x=next(iter(visible)) if len(visible)==1 else '@implicit'
                            before=substitute(before,{'@implicit':('var',x)})
                            if not quantity_ok(before,x,ctx):
                                raise Refusal('idiom-shape')
                            return ('forall',x,dm,('=>',('call',nm,False,(('var',x),)),('>=',prime(before,ctx),before)))
                        self.rule('@F',rule['template'],{**base,'quantity':qcat},decrease)
            if num==6:
                for dom,d in ctx.data['domains'].items():
                    op='<=' if rule['name']=='at most one' else '='
                    self.rule('@F',rule['template'],{'member':d['member'],'v':'$NAME','body':'@F'},
                        lambda v,dm=dom,o=op:(o,('count',v['v'],dm,ungroup(v['body'])),('int',1)))
            if num==7:
                for dom,d in ctx.data['domains'].items():
                    for dm,e in ctx.data['domains'].items():
                        self.rule('@F',rule['template'],{'member':d['member'],'member2':e['member'],'v':'$NAME','w':'$NAME','body':'@F'},
                            lambda v,a=dom,b=dm:('forall',v['v'],a,('forall',v['w'],b,ungroup(v['body']))))
            if num==8:
                for nm in (*ctx.data['symbols'],*(d[0] for d in ctx.definitions)):
                    p=ctx.symbol(nm)
                    if p['kind']!='predicate' or p['arity']!=1:
                        continue
                    dm=p['args'][0];d=ctx.domain(dm)
                    self.rule('@F',rule['template'],{'article':article(d['member'],d.get('article')),'member':d['member'],
                        'domain_noun':d.get('noun',d['plural']),'phrase':p['clause'].removeprefix('{0} ')},
                        lambda v,n=nm,dom=dm:('exists','@implicit',dom,('call',n,False,(('var','@implicit'),))))
        # A quantity is an integer state expression with its single step argument
        # omitted. Independent precedence categories preserve its exact tree.
        for dom in ctx.data['domains']:
            cats={i:f'@Q{dom}{i}' for i in (7,8,9)};q='@Q'+dom
            self.link(q,cats[7]);self.link(cats[7],cats[8]);self.link(cats[8],cats[9])
            self.constructor(cats[9],'var',{'name':'$NAME'},lambda v:('var',v['name']))
            self.constructor(cats[9],'int',{'value':'$INT'},lambda v:('int',int(v['value'])))
            self.rule(cats[9],'-{value}',{'value':'$INT'},lambda v:('int',-int(v['value'])))
            self.constructor(cats[9],'group',{'a':q},lambda v:('group',v['a']))
            for op in ARITH:
                level=8 if op=='*' else 9 if op in ('floor','ceil') else 7
                a=cats[level];b=cats[9 if level==8 else 8 if level==7 else 9]
                self.constructor(cats[level],op,{'a':a,'b':b},lambda v,o=op:(o,ungroup(v['a']),ungroup(v['b'])))
            for nm in (*ctx.data['symbols'],*(d[0] for d in ctx.definitions)):
                s=ctx.symbol(nm)
                if s['kind']!='function' or s['args']!=(dom,):
                    continue
                for primed in (False,True):
                    if primed and not is_state(nm,ctx):
                        continue
                    self.rule(cats[9],s['noun']+(' after' if primed else ''),{},
                        lambda v,n=nm,p=primed:('call',n,p,(('var','@implicit'),)))


def is_state(name,ctx):
    s=ctx.symbol(name)
    d=ctx.definition(name)
    return s.get('state',False) or bool(d and prime(expand(d[1],ctx),ctx)!=expand(d[1],ctx))


@lru_cache(maxsize=2)
def symbolic_grammar(temporal=False):
    return (Rule('@ROOT',('@F',),lambda v:v[0]),Rule('@ROOT',('@T',),lambda v:v[0]))+Compiler('formula',temporal=temporal).compile()+Compiler('typed',temporal=temporal).compile()


def syntax(text,temporal=False):
    trees=earley(tokenize(text),symbolic_grammar(temporal),'@ROOT')
    unique={clean(t) for t in trees}
    if len(unique)!=1:
        raise Refusal('ambiguous at token 0; expected one distinct syntax tree')
    return bounded(next(iter(unique)))


@lru_cache(maxsize=32)
def grammar(ctx,rung):
    mode='precise' if rung in ('intuitive','plain') else rung
    return Compiler(mode,ctx,rung if rung in ('intuitive','plain') and ctx.data['theory']==TEMPORAL_THEORY else rung in ('intuitive','plain')).compile()


def name_implicit(n):
    used=names_in(n)
    def walk(node):
        if node[0] in BIND and node[1]=='@implicit':
            v=fresh(used,'v');used.add(v)
            node=(node[0],v,node[2],substitute(node[3],{'@implicit':('var',v)}))
        return rebuild(node,[walk(c) for c in children(node)])
    return walk(n)


def admit(trees,ctx):
    distinct={}
    reason='type:formula-required'
    for t in trees:
        t=bounded(clean(t))
        try:
            if validate(t,ctx)!='bool':
                continue
            key=normal(t,ctx)
        except Refusal as exc:
            reason=str(exc);continue
        distinct.setdefault(key,t)
        if len(distinct)==2:
            raise Refusal('ambiguous at token 0; expected exactly one expanded normalized tree')
    if not distinct:
        raise Refusal(f'{reason} at token 0; expected well-typed closed formula')
    return name_implicit(next(iter(distinct.values())))


def parse(text,rung,ctx):
    if rung not in RUNGS:
        raise Refusal('unknown-rung at token 0; expected '+', '.join(RUNGS))
    if rung=='question':
        return parse_question(text,ctx)
    tokens=tokenize(text)
    if rung in ('formula','typed'):
        rules=symbolic_grammar(ctx.data['theory']==TEMPORAL_THEORY)
    else:
        rules=grammar(ctx,rung)
        if not tokens or tokens[-1]!='.':
            raise Refusal(f'syntax at token {len(tokens)}; expected .')
        tokens=tokens[:-1]
    return admit(earley(tokens,rules,sentence_case=rung not in ('formula','typed')),ctx)


def parse_question(text,ctx):
    # The main target is parsed by the same complete CFG. The task must be the
    # deterministic task for that target, not a second authority-bearing claim.
    tokens=tokenize(text)
    q=REGISTRY['questions'];rules=list(grammar(ctx,'plain'))
    c=Compiler('precise',ctx)
    c.rule('@QUESTION',q['target'],{'plain':'@F'},lambda v:v['plain'])
    c.rule('@QUESTION',q['exists'],{'tail':'@TAIL'},lambda v:v['tail'])
    for reading in ctx.data.get('readings',()):
        if reading_applies(reading,ctx) and reading.get('question'):
            c.rule('@QUESTION',reading['question'],{},lambda v,n=syntax(reading['body'],True):n)
    # Existential main question removes only the fixed "there exists" prefix.
    for r in rules:
        if r.lhs=='@F' and r.rhs[:2]==('there','exists'):
            c.rules.append(Rule('@TAIL',r.rhs[2:],lambda vs,rr=r:rr.action(('there','exists',*vs))))
    mark=next((i for i,t in enumerate(tokens) if t=='?'),len(tokens))
    trees=earley(tokens[:mark+1],tuple(rules+c.rules),'@QUESTION')
    # Parse the engine text too; its explicit binder recovers a variable name
    # omitted by an exact main-question idiom. No formula from context is used.
    engine=Compiler('precise',ctx)
    engine.rule('@ENGINE',q['counter'],{'task':'@TASK'},lambda v:('counter',v['task']))
    engine.rule('@TASK',q['evaluate'],{'body':'@F'},lambda v:ungroup(v['body']))
    for dm,d in ctx.data['domains'].items():
        base={'article':article(d['member'],d.get('article')),'member':d['member'],
              'v':'$NAME','domain_noun':d.get('noun',d['plural']),'condition':'@F','body':'@F'}
        engine.rule('@TASK',q['find'],base,lambda v,dom=dm:('exists',v['v'],dom,ungroup(v['body'])))
        for nm,p in ctx.data['symbols'].items():
            if p['kind']=='predicate' and p['args']==(dm,) and p.get('noun'):
                engine.rule('@TASK',q['find_restricted'],{**base,'noun':p['noun'],'article':article(p['noun'],p.get('article'))},
                    lambda v,dom=dm,n=nm:('exists',v['v'],dom,('and',('call',n,False,(('var',v['v']),)),ungroup(v['body']))))
        for key in ('witness','general_witness'):
            engine.rule('@ENGINE',q[key],base,lambda v,dom=dm:('witness',v['v'],dom))
    tasks=earley(tokens[mark+1:],tuple(rules+engine.rules),'@ENGINE')
    valid=[]
    for tree,task in product(trees,tasks):
        try:
            t=clean(tree)
            renames={}
            def mark_implicit(node):
                if node[0] in BIND and node[1]=='@implicit':
                    v=fresh(names_in(t)|set(renames),'_implicit'+str(len(renames)))
                    renames[v]=v
                    node=(node[0],v,node[2],substitute(node[3],{'@implicit':('var',v)}))
                return rebuild(node,[mark_implicit(c) for c in children(node)])
            t=mark_implicit(t)
            def align(a,b):
                if a[0]!=b[0]:return
                if a[0] in BIND and a[1] in renames and b[1]!='@implicit' and renames[a[1]]==a[1]:renames[a[1]]=b[1]
                for x,y in zip(children(a),children(b)):align(x,y)
            if task[0]=='counter':align(negate(expand(t,ctx)),expand(task[1],ctx))
            elif t[0]=='exists' and t[1] in renames:renames[t[1]]=task[1]
            def restore(node):
                if node[0] in BIND and node[1] in renames:
                    v=renames[node[1]]
                    node=(node[0],v,node[2],substitute(node[3],{node[1]:('var',v)}))
                return rebuild(node,[restore(c) for c in children(node)])
            t=restore(t)
            if validate(t,ctx)=='bool' and question(t,ctx)==text:
                valid.append(t)
        except Refusal:
            pass
    if not valid:
        raise Refusal(f'question-task at token {mark+1}; expected exact polarity-preserving engine task')
    return admit(valid,ctx)



def evaluate(n,ctx,fixture):
    """Stipulated finite tables only. No execution or reachability authority."""
    if ctx.data['theory'] == TEMPORAL_THEORY:
        raise Refusal('temporal:use-existing-native-evaluator')
    model=freeze(fixture)
    for dm in ctx.data['domains']:
        values=model['domains'][dm]
        if len(set(values))!=len(values):
            raise Refusal('fixture:duplicate-domain-member')
    def ev(n,env):
        tag=n[0]
        if tag=='int': return n[1]
        if tag=='var': return env[n[1]]
        if tag in BIND:
            vals=[ev(n[3],{**env,n[1]:x}) for x in model['domains'][n[2]]]
            return all(vals) if tag=='forall' else any(vals) if tag=='exists' else sum(vals)==1 if tag=='exists1' else sum(vals)
        if tag=='call':
            key=n[1]+("'" if n[2] else '');args=tuple(ev(a,env) for a in n[3])
            rows=model['symbols'][key]['rows'];matches=[val for xs,val in rows if tuple(xs)==args]
            if len(matches)>1 or not matches and 'default' not in model['symbols'][key]: raise Refusal('fixture:missing-or-duplicate-row:'+key)
            value=matches[0] if matches else model['symbols'][key]['default'];s=ctx.symbol(n[1])
            if type(value)!=(bool if s['kind']=='predicate' else int):
                raise Refusal('fixture:value-type:'+key)
            return value
        cs=[ev(c,env) for c in children(n)];a=cs[0];b=cs[1] if len(cs)>1 else None
        if tag=='not': return not a
        if tag=='and': return a and b
        if tag=='or': return a or b
        if tag=='=>': return not a or b
        if tag=='<=>': return a==b
        if tag=='=': return a==b
        if tag=='!=': return a!=b
        if tag=='<': return a<b
        if tag=='<=': return a<=b
        if tag=='>': return a>b
        if tag=='>=': return a>=b
        if tag=='+': return a+b
        if tag=='-': return a-b
        if tag=='*': return a*b
        if tag in ('floor','ceil'):
            if b<=0: raise Refusal('fixture:positive-divisor-required')
            return a//b if tag=='floor' else -(-a//b)
        raise Refusal('fixture:constructor')
    validate(n,ctx)
    return ev(expand(n,ctx),{})


def legend(n,ctx):
    entries={};dictionary=theory_info(ctx.data)['symbols'];prime_explained=False;symbols=REGISTRY['templates']['formula']
    def put(key,read,meaning):
        entries.setdefault(key,(key,read,meaning))
    def walk(n):
        nonlocal prime_explained
        tag=n[0]
        if tag in TEMPORAL:
            entry=REGISTRY['temporal_legend'][tag]
            if tag=='until':
                put('(…)',*dictionary['group']);walk(n[1])
                put(entry[0],entry[1],entry[2]);walk(n[2])
            else:
                put(entry[0].format(k=n[1] if tag=='within' else ''),entry[1],entry[2])
                put('(…)',*dictionary['group'])
                for c in children(n):walk(c)
            return
        if tag in BIND:
            v,dm=n[1:3];key=symbols[tag].split('{body}')[0].format(
                v=formula_identifier(v),domain=formula_identifier(dm)).rstrip()
            if tag=='count':key='#{…}'
            read,meaning=dictionary[tag];args={'v':v,'domain':dm,'article':article(v,variable=True)}
            put(key,read.format(**args),meaning.format(**args))
            if v in GREEK_IDENTIFIERS:
                put(formula_identifier(v),v,'bound variable; its domain is stated by the quantifier')
            d=ctx.domain(dm);put(formula_identifier(dm),dm,'the '+d['plural']);walk(n[3]);return
        if tag=='call':
            nm,p,args=n[1:];s=ctx.symbol(nm);key=body_text(n,'formula',ctx)
            read=nm+(' prime' if p else '')+' of '+(', '.join(body_text(a,'typed',ctx) for a in args) if args else 'no arguments' if ctx.data['theory']==TEMPORAL_THEORY else '')
            if s['kind']=='predicate':
                meaning=s['clause'].format(*(body_text(a,'typed',ctx) for a in args))
                if p:
                    meaning=body_text(reading_tree(n,ctx),'precise',ctx)
            else:
                value=', '.join((ctx.domain(dm)['member']+' '+body_text(a,'typed',ctx)) for dm,a in zip(s['args'],args))
                meaning=s['noun']+(' before ' if is_state(nm,ctx) else ' of ')+value
                if p:
                    entry=dictionary['prime']
                    read=entry['read'].format(name=nm,args=', '.join(body_text(a,'typed',ctx) for a in args))
                    meaning=entry['meaning'].format(noun=s['noun'],step=value)
                    if not args and ctx.data['theory']==TEMPORAL_THEORY:
                        read=nm+' prime of no arguments';meaning=s['noun']+' after this event'
                    if not prime_explained:
                        meaning+=entry['first_suffix'];prime_explained=True
            if s['kind']=='function' and is_state(nm,ctx) and ctx.data['theory']==TEMPORAL_THEORY:
                meaning=s['noun']+(' of '+', '.join(body_text(a,'typed',ctx) for a in args) if args else '')+(' after this event' if p else ' before this event')
                if p and len([entry for entry in entries.values() if 'prime marks' in entry[2]])==0:
                    meaning+=dictionary['prime']['first_suffix']
            put(key,read,meaning)
            for a in args:
                if a[0]!='var':walk(a)
            return
        if tag=='var':return
        if tag=='int':put(str(n[1]),str(n[1]),'integer literal');return
        cs=children(n)
        def operand(c,side):
            if needs_group(c,n,side):put('(…)',*dictionary['group'])
            walk(c)
        if len(cs)==2 and tag not in ('floor','ceil'):operand(cs[0],0)
        key='⌈u/v⌉' if tag=='ceil' else '⌊u/v⌋' if tag=='floor' else symbols[tag].replace('{a}','').replace('{b}','').strip()
        put(key,*dictionary[tag])
        for side,c in enumerate(cs):
            if len(cs)!=2 or tag in ('floor','ceil') or side==1:operand(c,side)
    walk(n)
    return tuple(entries.values())


def legend_text(n,ctx):
    return theory_info(ctx.data)['legend_header']+'\n'+'\n'.join(f'{symbol:<10}| {read:<23}| {meaning}' for symbol,read,meaning in legend(n,ctx))


def ladder_text(n,ctx,only=None):
    d=ctx.data;a=d['authority']
    profile=TEMPORAL_PROFILE if d['theory']==TEMPORAL_THEORY else PROFILE
    lines=[f"{d['id']} — {profile} (experimental; authority: none)",
           REGISTRY['kind_prefix'][d['kind']],f"Authority: {a['status']} — {a['source']}",f"Scope (theory: {d['theory']}; {theory_info(d)['label']}): {d.get('scope','declared finite domains')}"]
    if d['theory']==TEMPORAL_THEORY:
        lines.extend(temporal_scope_text(ctx))
    previous=None
    labels=('Formula','Typed','Formal reading','Precise English','Intuitive','Plain','Question')
    for rung,label in zip(RUNGS,labels):
        text=render(n,rung,ctx)
        if only is None or rung==only:
            lines.append(label+': '+('(same as above)' if text==previous and only is None else text))
        previous=text
        if rung=='formula' and only in (None,'formula'):
            theory_line,entries=legend_text(n,ctx).split('\n',1)
            lines.extend((theory_line,'Legend: symbol | read as | meaning',entries))
    for name,d,body in ctx.definitions:
        label='Definition ['+d['version']+']: ' if ctx.data['theory']==TEMPORAL_THEORY else 'Definition: '
        lines.append(label+name+'('+', '.join(p['name'] for p in d['params'])+') := '+body_text(body,'formula',ctx))
    for f in ctx.data['fixtures']:
        if ctx.data['theory']==TEMPORAL_THEORY:
            lines.append('For example '+f['id']+': '+f['note']+' Expected: '+str(f['expected']).lower()+' (fixture expectation; run native parity checks).')
        else:
            lines.append(fixture_text(n,ctx,f))
    return '\n'.join(lines)+'\n'


def fixture_text(n,ctx,fixture):
    fixture=freeze(fixture)
    predicates=[];functions=[]
    for name,table in fixture['symbols'].items():
        primed=name.endswith("'");base=name[:-1] if primed else name
        meta=ctx.symbol(base);is_predicate=meta['kind']=='predicate'
        rows=predicates if is_predicate else functions
        for args,value in table['rows']:
            call=base+('′' if primed else '')+'('+', '.join(map(str,args))+')'
            rows.append(call+(' is '+str(value).lower() if is_predicate else ' = '+str(value)))
        if 'default' in table:
            rows.append(name+(' is ' if is_predicate else ' = ')+str(table['default']).lower()+' for all other arguments')
    groups=predicates+([', '.join(functions)] if functions else [])
    domains='; '.join(name+' = {'+', '.join(map(str,values))+'}' for name,values in fixture['domains'].items())
    truth=evaluate(n,ctx,fixture)
    return ('For example '+fixture['id']+': '+'; '.join(groups)+'. Verdict: '+str(truth).lower()+'. '
            +fixture['note']+' Domains: '+domains+'.')


def file_theory(path):
    name=json.loads(Path(path).read_text()).get('theory')
    if not isinstance(name,str) or not name:
        raise Refusal('unsupported-theory: '+str(name))
    return name


def meaning_file(path):
    ctx=load(path)
    tree=normal(parse(ctx.data['formula'],'typed',ctx),ctx)
    return (scope_identity(ctx),tree)


def main(argv=None):
    parser=argparse.ArgumentParser(description=__doc__)
    sub=parser.add_subparsers(dest='command',required=True)
    p=sub.add_parser('render');p.add_argument('file');p.add_argument('--rung',choices=RUNGS)
    p=sub.add_parser('lower');p.add_argument('file')
    p=sub.add_parser('parse');p.add_argument('--rung',required=True,choices=RUNGS);p.add_argument('text');p.add_argument('--context',required=True)
    p=sub.add_parser('same');p.add_argument('a');p.add_argument('b')
    p=sub.add_parser('find');p.add_argument('a');p.add_argument('--corpus',required=True)
    args=parser.parse_args(argv)
    try:
        if args.command=='render':
            c=load(args.file);print(ladder_text(parse(c.data['formula'],'typed',c),c,args.rung),end='')
        elif args.command=='lower':
            sys.path.insert(0,str(Path(__file__).resolve().parent))
            from temporal_lowering import lower
            c=load(args.file);n=parse(c.data['formula'],'typed',c);result=lower(n,c)
            print(ladder_text(n,c,'formula'),end='')
            print('Lowering subject: '+result.subject)
            for obligation in result.obligations:print('Obligation: '+obligation)
            if result.source is None:
                print('Unsupported lowering; readable obligation retained');return 2
            print('Native source (inert; authority: none):\n'+result.source,end='')
        elif args.command=='parse':
            c=load(args.context);n=parse(args.text,args.rung,c)
            if c.data['theory']==TEMPORAL_THEORY:
                print(ladder_text(n,c,'formula'),end='')
            else:
                print(render(expand(n,c),'formula',c))
        elif args.command=='same':
            if file_theory(args.a)!=file_theory(args.b):
                print('different theories, not compared');return 2
            a,b=meaning_file(args.a),meaning_file(args.b);diff=first_difference(a,b)
            print('same meaning' if diff is None else
                  'different; first differing subtree: '+repr(diff)+
                  '\nDifferent scope or normal form; this does not establish different meanings.')
        else:
            theory=file_theory(args.a);a=meaning_file(args.a) if theory in REGISTRY['theories'] else None;status=0
            for path in sorted(Path(args.corpus).glob('*.json')):
                if file_theory(path)!=theory:
                    print(str(path)+': different theories, not compared');status=2;continue
                if a is None:a=meaning_file(args.a)
                if meaning_file(path)==a:print(path)
            if a is None:theory_info({'theory':theory})
            return status
        return 0
    except (Refusal,KeyError,ValueError,OSError) as exc:
        print('REFUSED: '+str(exc));return 2


def check_within(k, ctx):
    if type(k) is not int or not 0 <= k <= 255:
        raise Refusal('temporal:within-literal-range')
    trace=ctx.data['trace']
    if trace['mode']=='finite' and k >= trace['horizon']:
        raise Refusal('temporal:within-horizon')


def check_temporal_context(data):
    if data.get('profile') != TEMPORAL_PROFILE:
        raise Refusal('temporal:profile')
    trace=data.get('trace',{})
    if not isinstance(trace,Mapping):
        raise Refusal('temporal:trace-declaration')
    if trace.get('mode') not in ('finite','unbounded'):
        raise Refusal('temporal:mode')
    horizon=trace.get('horizon')
    if trace['mode']=='finite' and (type(horizon) is not int or not 1<=horizon<=256):
        raise Refusal('temporal:horizon')
    if trace['mode']=='unbounded' and horizon is not None:
        raise Refusal('temporal:unbounded-horizon')
    for key in ('source_revision','event_schema','order','projection','model','environment','initialization','transitions','continuity'):
        if not isinstance(trace.get(key),str) or not trace[key].strip():
            raise Refusal('temporal:missing-'+key)
    if trace.get('arithmetic')!='checked-i128' or trace.get('evidence')!='obligation':
        raise Refusal('temporal:evidence-or-arithmetic')
    domains=[name for name,d in data['domains'].items() if d.get('trace') is True]
    if len(domains)!=1 or domains[0]!=trace.get('domain'):
        raise Refusal('temporal:trace-domain')
    reserved={'always','eventually','next','until','never','within','provided','since','previously'}
    entries=(*data['domains'].items(),*data['symbols'].items(),*((d.get('name'),d) for d in data['definitions']))
    for name,meta in entries:
        words={word.lower() for word in re.findall(NAME,' '.join(str(meta.get(k,'')) for k in ('clause','noun','member','plural')))}
        if not isinstance(name,str) or name.lower() in reserved or words & reserved:
            raise Refusal('temporal:reserved-lexicon')
    for meta in data['symbols'].values():
        if type(meta.get('arity')) is not int:
            raise Refusal('temporal:arity-type')
        if meta.get('kind')=='predicate' and meta.get('state'):
            raise Refusal('temporal:prime-needs-state-function')
        if meta.get('observation') not in ('event','trace'):
            raise Refusal('temporal:observation-kind')
        if not isinstance(meta.get('definition'),str) or not meta.get('definition') or not isinstance(meta.get('version'),str) or not meta.get('version'):
            raise Refusal('temporal:observation-definition')
    for definition in data['definitions']:
        if not isinstance(definition.get('version'),str) or not definition.get('version'):
            raise Refusal('temporal:definition-version')
    for reading in data.get('readings',()):
        if not isinstance(reading.get('version'),str) or not reading.get('version') or not isinstance(reading.get('body'),str) or not reading.get('body'):
            raise Refusal('temporal:reading-version')
        if reading.get('mode') not in ('finite','unbounded'):
            raise Refusal('temporal:reading-mode')
        if 'horizon' in reading and (reading['mode']!='finite' or type(reading['horizon']) is not int or not 1<=reading['horizon']<=256):
            raise Refusal('temporal:reading-horizon')
        if any(key in reading and (not isinstance(reading[key],str) or not reading[key]) for key in ('plain','intuitive','question')):
            raise Refusal('temporal:reading-text')
    assumption=data.get('assumptions')
    if assumption is not None and (not isinstance(assumption,Mapping) or any(not isinstance(assumption.get(key),str) or not assumption.get(key) for key in ('formula','definition','version'))):
        raise Refusal('temporal:assumptions-declaration')


def check_temporal_formula(n,ctx):
    bounded(n)
    expanded=expand(n,ctx,derived=False)
    if expanded[0] not in ('forall','exists') or not ctx.domain(expanded[2]).get('trace'):
        raise Refusal('temporal:trace-binder-root')
    traces=[]
    def walk(node, active=False, in_time=False):
        tag=node[0]
        if tag in BIND:
            is_trace=ctx.domain(node[2]).get('trace') is True
            if in_time:
                raise Refusal('temporal:quantifier-inside-time')
            if is_trace:
                if active or tag not in ('forall','exists'):
                    raise Refusal('temporal:nested-trace-binder')
                traces.append(node)
            walk(node[3],active or is_trace,in_time)
            return
        if tag in TEMPORAL:
            if not active:
                raise Refusal('temporal:missing-trace-binder')
            if tag=='within':check_within(node[1],ctx)
            if tag in ('eventually','until','within') and ctx.data.get('assumptions') is None:
                raise Refusal('temporal:explicit-assumptions-required')
            in_time=True
        for child in children(node):walk(child,active,in_time)
    # Scope is checked on meaning, including definitions of whole requirements.
    walk(expanded)
    if len(traces)>1:
        raise Refusal('temporal:multiple-trace-binders')
    assumption=ctx.data.get('assumptions')
    if assumption and assumption['formula']!='true':
        if len(traces)!=1 or traces[0][3][0]!='=>' or alpha(expand(traces[0][3][1],ctx))!=alpha(expand(syntax(assumption['formula'],True),ctx)):
            raise Refusal('temporal:assumption-scope')


def reading_applies(reading,ctx):
    trace=ctx.data['trace']
    return reading['mode']==trace['mode'] and ('horizon' not in reading or reading['horizon']==trace['horizon'])


def registered_reading(n,ctx,rung):
    for reading in ctx.data.get('readings',()):
        text=reading.get(rung)
        if text and reading_applies(reading,ctx) and alpha(expand(n,ctx))==alpha(expand(syntax(reading['body'],True),ctx)):
            return text
    return None


def temporal_body(n,rung,ctx,show):
    if rung in ('intuitive','plain') and (text:=registered_reading(n,ctx,rung)):
        return text
    mode=rung if rung in ('formula','typed','formal','precise') else 'intuitive'
    templates=REGISTRY['temporal_templates'][mode]
    if rung in ('intuitive','plain') and n[0]=='always' and n[1][0]=='not':
        return templates['never'].format(a=show(n[1][1]))
    if n[0] not in TEMPORAL:
        return None
    return templates[n[0]].format(a=show(n[2] if n[0]=='within' else n[1]),
        b=show(n[2]) if n[0]=='until' else '',k=n[1] if n[0]=='within' else '')


def plain_data(value):
    if isinstance(value,Mapping):return {k:plain_data(v) for k,v in value.items()}
    if isinstance(value,tuple):return [plain_data(v) for v in value]
    return value


def scope_identity(ctx):
    # Bind full observation definitions and model restrictions, including versions.
    import hashlib
    data={k:plain_data(v) for k,v in ctx.data.items() if k not in ('id','formula','fixtures')}
    return hashlib.sha256(json.dumps(data,sort_keys=True,separators=(',',':'),ensure_ascii=True).encode()).hexdigest()


def temporal_scope_text(ctx):
    d=ctx.data;t=d['trace'];a=d.get('assumptions')
    return [
        'Trace source: '+t['source_revision']+'; schema: '+t['event_schema']+'; order: '+t['order']+'; projection: '+t['projection'],
        'Mode: '+t['mode']+(' '+str(t['horizon'])+' (all lengths 1 through horizon)' if t['mode']=='finite' else ' (natural-number-indexed observations; no final event)'),
        'Assumptions: '+(a['formula']+' := '+a['definition']+' [version '+a['version']+']' if a else 'none declared'),
        'Model: '+t['model']+'; environment: '+t['environment'],
        'Execution obligations: initialization '+t['initialization']+'; transition membership '+t['transitions']+'; continuity '+t['continuity'],
        'Evidence: obligation; authority: none; arithmetic: checked i128; scope identity: '+scope_identity(ctx),
        *('Observation '+name+' ['+meta['observation']+'; version '+meta['version']+']: '+meta['definition'] for name,meta in d['symbols'].items()),
        *('Registered reading ['+r['version']+'; '+r['mode']+(' '+str(r['horizon']) if 'horizon' in r else '')+']: '+r['body'] for r in d.get('readings',())),
    ]


if __name__=='__main__':
    sys.exit(main())
