#!/usr/bin/env python3
"""Requirement-led comparisons against original-formula truth enumeration."""
import copy
import json
import random
import time
from pathlib import Path
from fractions import Fraction
import oracle
import run

ROOT = Path(__file__).resolve().parent

def atom(p, *vs): return {'op': 'atom', 'predicate': p, 'args': list(vs)}
def unary(op, body): return {'op': op, 'body': body}
def binary(op, a, b): return {'op': op, 'left': a, 'right': b}
def quant(op, v, body): return {'op': op, 'var': v, 'body': body}
def count(v, k, body, cmp='eq'):
    return dict(quant('count', v, body), cmp=cmp, threshold=str(k))
def mod(v, r, m, body, cmp='eq'):
    return dict(quant('mod_count', v, body), cmp=cmp, residue=str(r), modulus=str(m))
def query(f, n=2, signature=None, weight=('1', '1')):
    sig = signature if signature is not None else [('R', 2)]
    return {'schema': 'zal/relational-query/1', 'operation': 'count', 'logic': 'zal/c2mod/1',
            'domain_size': str(n), 'signature': [{'id': p, 'arity': a} for p, a in sig],
            'weights': {p: list(weight) for p, _ in sig}, 'formula': f, 'context_id': 'qualification'}
def request(q, mode='count', model=None, limits=None):
    if model is not None:
        model = {p: [[str(a) for a in t] for t in sorted(ts)] for p, ts in model.items()}
    return {'query': q, 'mode': mode, 'model': model, 'limits': limits or run.DEFAULT_LIMITS}

def corpus():
    r = atom('R', 'x', 'y')
    formulas = []
    for cmp in ('eq', 'le', 'ge'):
        for k in (0, 1, 2, 4):
            row = count('y', k, r, cmp)
            formulas.extend([quant('forall', 'x', row), quant('forall', 'x', unary('not', row)), quant('exists', 'x', row)])
    for m in (1, 2, 3, 5):
        for cmp in ('eq', 'le', 'ge'):
            formulas.append(quant('forall', 'x', mod('y', m-1, m, r, cmp)))
    formulas += [
        quant('forall', 'x', quant('forall', 'y', binary('implies', r, atom('R', 'y', 'x')))),
        quant('forall', 'x', unary('not', atom('R', 'x', 'x'))),
        quant('exists', 'x', count('y', 1, atom('R', 'y', 'x'))),
        quant('forall', 'y', count('x', 1, atom('R', 'y', 'x'))),
        count('x', 1, quant('exists', 'y', r)),
        mod('x', 0, 2, count('y', 1, r)),
        quant('forall', 'x', binary('iff', count('y', 1, r), mod('y', 1, 2, r))),
        binary('and', quant('forall', 'x', count('y', 1, r)), quant('forall', 'x', mod('y', 1, 2, r))),
        quant('exists', 'x', binary('and', atom('R', 'x', 'x'), quant('forall', 'x', atom('R', 'x', 'x')))),
        quant('forall', 'x', binary('or', quant('exists', 'y', r), quant('exists', 'x', atom('R', 'x', 'x')))),
        quant('forall', 'x', quant('forall', 'x', unary('not', atom('R', 'x', 'x')))),
        {'op': 'true'}, {'op': 'false'},
    ]
    cases = []
    for i, f in enumerate(formulas):
        for n in (1, 2, 3):
            for w in (('1', '1'), ('3/2', '2/3'), ('1', '0'), ('1', '-1')):
                # Keep the full set at n=2; vary the n=1/3 companions economically.
                if n != 2 and w != ('1', '1'):
                    continue
                cases.append((f'formula-{i}-n{n}-w{w}', query(f, n, weight=w)))
    for value in (True, False):
        for n in (1, 2, 3):
            cases.append(('empty-signature', query({'op': str(value).lower()}, n, [])))
    # Explicit full-signature / false-weight contribution with an unused predicate.
    cases.append(('unused-unary', query(quant('forall', 'x', atom('P', 'x')), 2, [('P', 1), ('Unused', 1)], ('2', '3'))))
    # Two relations remain correlated; equality, reverse arguments and nested binders.
    mixed = quant('forall', 'x', binary('iff', count('y', 1, binary('and', r, atom('S', 'y', 'x'))), mod('y', 0, 2, atom('S', 'x', 'y'))))
    cases.append(('correlated-two-relations', query(mixed, 2, [('R', 2), ('S', 2)])))
    cases.append(('equality-witness', query(quant('forall', 'x', count('y', 1, {'op': 'equal', 'args': ['x', 'y']})), 3, [])))
    # Deterministic generated formula diversity, not generated expected values.
    rng = random.Random(260526)
    leaves = [r, atom('R', 'y', 'x'), atom('R', 'x', 'x'), {'op': 'equal', 'args': ['x', 'y']}]
    for i in range(32):
        body = binary(rng.choice(['and', 'or', 'implies', 'iff']), rng.choice(leaves), unary('not', rng.choice(leaves)))
        inner = count('x' if i%2 else 'y', i%3, body, rng.choice(['eq', 'le', 'ge']))
        free = 'y' if i%2 else 'x'
        outer = count(free, i%3, unary('not', inner)) if i%3 else mod(free, i%2, 2, inner)
        cases.append((f'generated-{i}', query(outer)))
    for path in sorted((ROOT / 'examples').glob('*-count.json')):
        cases.append((path.stem, json.loads(path.read_text())))
    return cases

def expect(condition, message):
    if not condition:
        raise AssertionError(message)

def main():
    started = time.monotonic()
    cases = corpus()
    expected = [oracle.count(q) for _, q in cases]
    answers, binding = run.worker([request(q) for _, q in cases], seconds=60)
    records = []
    for (name, q), want, answer in zip(cases, expected, answers):
        expect(answer.get('status') == 'ExactCount' and answer.get('value') == want,
               (name, q, want, answer))
        records.append({'case': name, 'query_sha256': run.sha(run.canonical(q)), 'expected': want, 'actual': answer})
    # Full original-model evaluation and exact number of definitional expansions.
    semantic_requests = []
    semantic_expected = []
    selected = [q for i, (_, q) in enumerate(cases) if int(q['domain_size']) == 2 and i % 19 == 0][:20]
    selected += [q for name, q in cases if name.startswith('generated-')][:12]
    for q in selected:
        for model in oracle.structures(q):
            truth = oracle.evaluate(q['formula'], model, 2)
            for mode in ('evaluate', 'expansion'):
                semantic_requests.append(request(q, mode, model))
                semantic_expected.append((mode, truth))
    semantic_answers, _ = run.worker(semantic_requests, seconds=60)
    for expected_pair, answer in zip(semantic_expected, semantic_answers):
        mode, truth = expected_pair
        if mode == 'evaluate':
            expect(answer == {'status': 'Evaluated', 'value': truth}, (mode, truth, answer))
        else:
            expect(answer == {'status': 'ExpansionCount', 'original': truth, 'expansions': str(int(truth))}, (mode, truth, answer))
    # Independent all-n graph identities exercised beyond direct oracle sizes.
    graph = json.loads((ROOT / 'examples/even-graph-count.json').read_text())
    graph_requests, graph_expected = [], []
    for n in range(1, 13):
        for family in ('even', 'odd', 'matching'):
            q = copy.deepcopy(graph)
            q['domain_size'] = str(n)
            # Locate the only modular count without depending on conjunct order.
            todo = [q['formula']]
            while todo:
                f = todo.pop()
                if f['op'] == 'mod_count':
                    if family == 'odd': f['residue'] = '1'
                    if family == 'matching':
                        f['op'] = 'count'; f['threshold'] = '1'; del f['residue']; del f['modulus']
                todo.extend(f[k] for k in ('left', 'right', 'body') if k in f)
            val = 2**((n-1)*(n-2)//2)
            if family == 'odd' and n%2: val = 0
            if family == 'matching':
                val = 0 if n%2 else 1
                if not n%2:
                    for k in range(1, n, 2): val *= k
            graph_requests.append(request(q)); graph_expected.append((family, n, str(val)))
    graph_answers, _ = run.worker(graph_requests, seconds=60)
    for (family, n, val), answer in zip(graph_expected, graph_answers):
        expect(answer.get('status') == 'ExactCount' and answer.get('value') == val, (family, n, val, answer))
    # Malformed inputs and each resource class must produce a distinct nonexact state.
    base = query(quant('forall', 'x', count('y', 1, atom('R', 'x', 'y'))))
    invalid = []
    for field, value in [('domain_size', '0'), ('domain_size', '02'), ('domain_size', '10001'), ('weights', {}), ('unknown', True)]:
        q = copy.deepcopy(base); q[field] = value; invalid.append(request(q))
    for f in [atom('R', 'x', 'y'), quant('forall', 'x', atom('R', 'x')),
              quant('forall', 'x', atom('Missing', 'x')),
              quant('forall', 'z', atom('R', 'z', 'z')),
              mod('x', 0, 0, {'op': 'true'}), mod('x', 2, 2, {'op': 'true'})]:
        q = copy.deepcopy(base); q['formula'] = f; invalid.append(request(q))
    q = copy.deepcopy(base); q['signature'] *= 2; invalid.append(request(q))
    q = copy.deepcopy(base); q['logic'] = 'zal/c2/1'; q['formula'] = mod('x', 0, 2, {'op': 'true'}); invalid.append(request(q))
    q = copy.deepcopy(base); q['weights']['R'] = ['2/2', '1']; invalid.append(request(q))
    for resource in ('entries', 'work', 'candidates', 'bits'):
        lim = dict(run.DEFAULT_LIMITS, **{resource: '1'})
        invalid.append(request(base, limits=lim))
    invalid_answers, _ = run.worker(invalid)
    expect(all(a.get('status') != 'ExactCount' for a in invalid_answers), invalid_answers)
    ingress = [b'{"a":1,"a":2}', b'{"a":1,"\\u0061":2}', b'{"a":NaN}', b'['*301+b']'*301, b' '* (run.MAX_BYTES+1)]
    for raw in ingress:
        try: run.strict_loads(raw)
        except ValueError: pass
        else: raise AssertionError('malformed ingress admitted')
    # Hand-derived counterexamples to frequently plausible wrong rules.
    negated = query(quant('forall', 'x', unary('not', count('y', 1, atom('R', 'x', 'y')))))
    expect(oracle.count(negated) == '4', 'strict overflow witness')
    controls = {'strict_cap_under_negation': {'correct': '4', 'wrong': '1'},
                'always_zero': {'correct': '137', 'wrong': '0'},
                'signed_zero_is_not_unsat': {'models': 2, 'weighted_sum': '0'}}
    signed = query({'op': 'true'}, 1, [('P', 1)], ('1', '-1'))
    expect(oracle.count(signed) == '0' and len(list(oracle.structures(signed))) == 2, 'signed zero')
    record = {'schema': 'zal/relational-qualification/1', 'binary_sha256': binding['binary_sha256'],
              'source_sha256': binding['source_sha256'], 'test_sha256': run.sha(Path(__file__).read_bytes()),
              'oracle_sha256': run.sha((ROOT/'oracle.py').read_bytes()),
              'count_comparisons': len(records), 'supplied_model_evaluations': len(semantic_requests)//2,
              'pointwise_unique_expansions': len(semantic_requests)//2,
              'closed_form_graph_comparisons': len(graph_requests),
              'invalid_and_resource_checks': len(invalid), 'ingress_checks': len(ingress),
              'negative_witnesses': controls, 'seconds': time.monotonic()-started,
              'counts': records, 'graph_results': [{'case': c, 'result': a} for c,a in zip(graph_expected,graph_answers)],
              'refusals': invalid_answers, 'formal_implementation_proof': 'open'}
    (ROOT/'evidence/qualification.json').write_text(json.dumps(record, indent=2)+'\n')
    print(json.dumps({k:v for k,v in record.items() if k not in ('counts','graph_results','source_sha256','refusals')}, indent=2))

if __name__ == '__main__': main()
