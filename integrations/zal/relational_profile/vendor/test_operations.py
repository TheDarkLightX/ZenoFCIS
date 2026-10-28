"""Public relation/evaluation contracts checked against original Python semantics."""
import copy
import json
import subprocess
import sys
from pathlib import Path
import oracle
import run
from test_backend import atom, binary, quant, count, mod, unary, query, corpus, expect

ROOT = Path(__file__).resolve().parent
TRUE = {'op': 'true'}
FALSE = {'op': 'false'}

def comparison(q, right, assumptions=TRUE, relation='equivalence'):
    return {k: q[k] for k in ('logic', 'signature', 'domain_size', 'context_id')} | {
        'schema': 'zal/relational-comparison/1', 'left': q['formula'], 'right': right,
        'assumptions': assumptions, 'relation': relation}

def model_json(model):
    return {p: [[str(v) for v in row] for row in sorted(rows)] for p, rows in model.items()}

def evaluation(q, model):
    return {k: q[k] for k in ('logic', 'signature', 'domain_size', 'context_id', 'formula')} | {
        'schema': 'zal/relational-evaluation/1', 'model': model_json(model)}

def expected_comparison(p):
    hist = dict(both_true=0, left_only=0, right_only=0, both_false=0)
    total = 0
    for m in oracle.structures(p):
        total += 1
        n = int(p['domain_size'])
        if not oracle.evaluate(p['assumptions'], m, n):
            continue
        a = oracle.evaluate(p['left'], m, n)
        b = oracle.evaluate(p['right'], m, n)
        key = 'both_true' if a and b else 'left_only' if a else 'right_only' if b else 'both_false'
        hist[key] += 1
    admitted = sum(hist.values())
    fail = hist['left_only'] > 0 or (p['relation'] == 'equivalence' and hist['right_only'] > 0)
    status = 'VacuousAt' if admitted == 0 else 'RefutedAt' if fail else 'EquivalentAt' if p['relation'] == 'equivalence' else 'ImpliesAt'
    return {'partition': {k: str(v) for k, v in hist.items()}, 'enumerated_models': str(total),
            'admitted_models': str(admitted), 'status': status}

def check_comparison(p, a, expected):
    for k, v in expected.items():
        expect(a[k] == v, (k, v, a))
    run.validate_comparison(p, a, run.DEFAULT_LIMITS)
    if a['counterexample'] is not None:
        w = a['counterexample']
        m = {p: {tuple(map(int, row)) for row in rows} for p, rows in w['model'].items()}
        n = int(p['domain_size'])
        expect(oracle.evaluate(p['assumptions'], m, n), ('witness violates assumptions', p, w))
        expect(oracle.evaluate(p['left'], m, n) == w['left'], ('left witness mismatch', p, w))
        expect(oracle.evaluate(p['right'], m, n) == w['right'], ('right witness mismatch', p, w))

def main():
    cases = []
    seen = set()
    selected = []
    for _, q in corpus():
        key = run.canonical(q['formula'])
        if q['domain_size'] == '2' and q['signature'] == [{'id': 'R', 'arity': 2}] and key not in seen:
            seen.add(key); selected.append(q)
    for i, q in enumerate(selected):
        same = copy.deepcopy(q['formula'])
        other = same if i % 4 == 0 else unary('not', same) if i % 4 == 1 else selected[(i + 7) % len(selected)]['formula']
        assumptions = FALSE if i % 11 == 0 else quant('exists', 'x', atom('R', 'x', 'x')) if i % 3 == 0 else TRUE
        cases.append(comparison(q, other, assumptions, 'implication' if i % 2 else 'equivalence'))
    p = quant('exists', 'x', atom('P', 'x'))
    qf = quant('exists', 'x', atom('Q', 'x'))
    equal_counts = comparison(query(p, 2, [('P', 1), ('Q', 1)]), qf)
    cases.append(equal_counts)
    for n in (1, 2, 3):
        exactly = count('x', 1, atom('P', 'x'))
        cases.extend([comparison(query(exactly, n, [('P', 1)]), p, TRUE, 'implication'),
                      comparison(query(p, n, [('P', 1)]), exactly, TRUE, 'implication'),
                      comparison(query(mod('x', 0, 2, atom('P', 'x')), n, [('P', 1)]), unary('not', mod('x', 1, 2, atom('P', 'x'))))])
    cases += [comparison(query(TRUE, 3, []), TRUE), comparison(query(TRUE, 2, [('Unused', 1)]), FALSE, FALSE)]
    expected = [expected_comparison(c) for c in cases]
    answers, binding = run.worker([{'mode': 'compare', 'request': c, 'limits': run.DEFAULT_LIMITS} for c in cases])
    for c, a, e in zip(cases, answers, expected): check_comparison(c, a, e)
    # Equal unweighted counts do not establish equal accepted model sets.
    eq = answers[cases.index(equal_counts)]
    expect(eq['partition'] == {'both_true': '9', 'left_only': '3', 'right_only': '3', 'both_false': '1'}, eq)

    evals = []
    eval_expected = []
    for q in selected[::5]:
        models = list(oracle.structures(q))
        for m in (models[0], models[-1]):
            evals.append(evaluation(q, m)); eval_expected.append(oracle.evaluate(q['formula'], m, 2))
    evaluated, _ = run.worker([{'mode': 'evaluate-public', 'request': c, 'limits': run.DEFAULT_LIMITS} for c in evals])
    for a, e in zip(evaluated, eval_expected): expect(a.get('status') == 'Evaluated' and a.get('value') is e, (a, e))

    malformed = []
    def bad(label, source, change, status='Invalid'):
        altered = copy.deepcopy(source); change(altered)
        mode = 'compare' if altered.get('schema') == 'zal/relational-comparison/1' else 'evaluate-public'
        malformed.append((label, {'mode': mode, 'request': altered, 'limits': run.DEFAULT_LIMITS}, status))
    bad('unknown field', cases[0], lambda p: p.update(unreviewed=True))
    bad('weights forbidden', equal_counts, lambda p: p.update(weights={'P': ['1', '-1'], 'Q': ['0', '1']}))
    bad('third variable', equal_counts, lambda p: p.update(right=quant('exists', 'z', atom('P', 'z'))))
    bad('open assumption', equal_counts, lambda p: p.update(assumptions=atom('P', 'x')))
    bad('wrong context type', equal_counts, lambda p: p.update(context_id=3))
    bad('unknown relation', equal_counts, lambda p: p.update(relation='similar'))
    bad('zero modulus', equal_counts, lambda p: p.update(right=mod('x', 0, 0, atom('P', 'x'))))
    bad('undeclared predicate', equal_counts, lambda p: p.update(right=quant('exists', 'x', atom('Lost', 'x'))))
    bad('empty domain', equal_counts, lambda p: p.update(domain_size='0'))
    bad('missing model vocabulary', evals[0], lambda p: p.update(model={}))
    bad('model out of domain', evals[0], lambda p: p.update(model={'R': [['0', '2']]}))
    bad('model wrong arity', evals[0], lambda p: p.update(model={'R': [['0']]}))
    bad('model duplicate tuple', evals[0], lambda p: p.update(model={'R': [['0', '1'], ['0', '1']]}))
    bad('Boolean model entity', evals[0], lambda p: p.update(model={'R': [[True, '1']]}))
    for label, payload, resource in [('model budget', equal_counts, 'candidates'), ('comparison work', equal_counts, 'work'), ('evaluation work', evals[0], 'work')]:
        mode = 'compare' if payload['schema'] == 'zal/relational-comparison/1' else 'evaluate-public'
        malformed.append((label, {'mode': mode, 'request': payload, 'limits': dict(run.DEFAULT_LIMITS, **{resource: '1'})}, 'ResourceExhausted'))
    bad_answers, _ = run.worker([p for _, p, _ in malformed])
    for (label, _, status), a in zip(malformed, bad_answers):
        expect(a.get('status') == status and not any(k in a for k in ('value', 'holds', 'partition')), (label, a))

    # Actual public CLI: fresh bindings, truth result vs semantic refutation,
    # vacuity and duplicate raw keys remain distinct outcomes.
    cli = []
    for label, c in [('different', equal_counts), ('same', comparison(query(p, 2, [('P', 1)]), p)),
                     ('vacuous', comparison(query(p, 2, [('P', 1)]), p, FALSE)), ('evaluation', evals[0])]:
        result = subprocess.run([sys.executable, str(ROOT/'run.py'), '-'], input=run.canonical(c), capture_output=True, timeout=15)
        a = json.loads(result.stdout)
        expect(result.returncode == run.exit_status(a), (label, result.returncode, a))
        expect(a['request_sha256'] == run.sha(run.canonical(c)) and a['binary_sha256'] == binding['binary_sha256'], (label, a))
        if label == 'vacuous': expect(result.returncode == 2 and a['holds'] is None, a)
        if label == 'different': expect(result.returncode == 1 and a['status'] == 'RefutedAt', a)
        if label == 'same': expect(result.returncode == 0 and a['status'] == 'EquivalentAt', a)
        if label == 'evaluation': expect(a['value'] is eval_expected[0], a)
        cli.append({'case': label, 'request': c, 'exit': result.returncode, 'answer': a})
    raw = run.canonical(equal_counts).replace(b'"relation":"equivalence"', b'"relation":"equivalence","relati\\u006fn":"implication"')
    result = subprocess.run([sys.executable, str(ROOT/'run.py'), '-'], input=raw, capture_output=True, timeout=15)
    expect(result.returncode == 2 and json.loads(result.stdout)['status'] == 'Invalid', result.stdout)

    response_controls = []
    for label, change in [('forged equivalence', lambda a: a.update(status='EquivalentAt', holds=True)),
                          ('partial coverage', lambda a: a.update(enumerated_models='8')),
                          ('wrong witness direction', lambda a: a['counterexample'].update(left=False, right=False)),
                          ('omitted witness', lambda a: a.update(counterexample=None))]:
        a = copy.deepcopy(eq); change(a)
        try: run.validate_comparison(equal_counts, a, run.DEFAULT_LIMITS)
        except RuntimeError as error: response_controls.append({'case': label, 'refusal': str(error)})
        else: raise AssertionError(('bad response admitted', label))
    report = {'schema': 'zal/operation-qualification/1', 'comparisons': len(cases), 'evaluations': len(evals),
              'malformed_or_resource_controls': len(malformed), 'public_cli_runs': len(cli) + 1,
              'response_controls': response_controls, 'binary_sha256': binding['binary_sha256'],
              'sources': {str(f.relative_to(ROOT)): run.sha(f.read_bytes()) for f in [Path(__file__), ROOT/'run.py', ROOT/'oracle.py', ROOT/'test_backend.py']},
              'comparison_records': [{'request': c, 'expected': e, 'actual': a} for c, e, a in zip(cases, expected, answers)],
              'evaluation_records': [{'request': c, 'expected': e, 'actual': a} for c, e, a in zip(evals, eval_expected, evaluated)],
              'refusals': [{'case': label, 'expected': status, 'actual': a} for (label, _, status), a in zip(malformed, bad_answers)],
              'cli': cli, 'proof_boundary': 'complete finite comparisons; implementation proof remains open'}
    (ROOT/'evidence/operations.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps({k: v for k, v in report.items() if k not in ('comparison_records', 'evaluation_records', 'refusals', 'cli', 'sources')}, indent=2))

if __name__ == '__main__': main()
