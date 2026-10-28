"""Independent direct semantics. No Haskell normalizer/recurrence is imported."""
from fractions import Fraction
from itertools import product

def compare(op, a, b):
    return {'eq': a == b, 'le': a <= b, 'ge': a >= b}[op]

def evaluate(formula, model, n, env=None):
    env = {} if env is None else env
    op = formula['op']
    if op in ('true', 'false'):
        return op == 'true'
    if op == 'atom':
        return tuple(env[v] for v in formula['args']) in model[formula['predicate']]
    if op == 'equal':
        a, b = formula['args']
        return env[a] == env[b]
    if op == 'not':
        return not evaluate(formula['body'], model, n, env)
    if op in ('and', 'or', 'implies', 'iff'):
        left = evaluate(formula['left'], model, n, env)
        right = evaluate(formula['right'], model, n, env)
        return {'and': left and right, 'or': left or right,
                'implies': not left or right, 'iff': left == right}[op]
    values = [evaluate(formula['body'], model, n, dict(env, **{formula['var']: a})) for a in range(n)]
    if op == 'forall':
        return all(values)
    if op == 'exists':
        return any(values)
    cardinality = sum(values)
    if op == 'count':
        return compare(formula['cmp'], cardinality, int(formula['threshold']))
    if op == 'mod_count':
        return compare(formula['cmp'], cardinality % int(formula['modulus']), int(formula['residue']))
    raise ValueError('unsupported oracle node: ' + op)

def structures(query, max_models=65536):
    n = int(query['domain_size'])
    atoms = [(p['id'], t) for p in query['signature'] for t in product(range(n), repeat=p['arity'])]
    if len(atoms) > max_models.bit_length() - 1:
        raise ValueError('oracle enumeration budget')
    for bits in product((False, True), repeat=len(atoms)):
        model = {p['id']: set() for p in query['signature']}
        for (p, t), bit in zip(atoms, bits):
            if bit:
                model[p].add(t)
        yield model

def weight(query, model):
    n = int(query['domain_size'])
    w = Fraction(1)
    for p in query['signature']:
        true, false = map(Fraction, query['weights'][p['id']])
        for args in product(range(n), repeat=p['arity']):
            w *= true if args in model[p['id']] else false
    return w

def count(query, max_models=65536):
    total = Fraction(0)
    for model in structures(query, max_models):
        if evaluate(query['formula'], model, int(query['domain_size'])):
            total += weight(query, model)
    return str(total)
