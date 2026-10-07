"""Independent finite-table oracle; no production files edited.

The expected semantics is constructed from primitive outcome tables and integer
truth masks, never from behavior.Formula.value, enabled, inputs, or check.
Production parse/decide/check are only called on the actual side of assertions.
This is exhaustive within the named small families, not a proof for all ZAL.
"""
import hashlib
import itertools
import json
from pathlib import Path
import sys

SOURCE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SOURCE))
from snapshot_loader import load_behavior
sut, sha_before = load_behavior(SOURCE / 'behavior.py')


def template(initial, rules, invariant=None, facts='p', states='aa, bb'):
    text = '\n'.join([
        'zal 1;', 'machine oracle;', f'states {states};', 'events tick;',
        f'context {facts};', f'initial {initial};',
        'step atomic; frame control_only;', 'default reject no_match unchanged;',
        *rules,
    ]) + '\n'
    if invariant is not None:
        text += f'invariant safe: {invariant};\n'
    return text


def expected(table, state, p):
    i = 2 * (state == 'bb') + p
    choice = table[i]
    return {'class': 'Accept' if choice >= 2 else 'Reject',
            'state': ('aa', 'bb')[choice - 2] if choice >= 2 else state,
            'reason': None if choice >= 2 else ('no_match', 'denied')[choice],
            'rule': f'rule_{i}' if choice else 'default'}


def distances(table, initial):
    # Relational transitive closure by repeated relaxation, unlike checker BFS.
    dist = {initial: 0}
    while True:
        expanded = dict(dist)
        for state, length in dist.items():
            for p in (False, True):
                target = expected(table, state, p)['state']
                expanded[target] = min(expanded.get(target, 100), length + 1)
        if expanded == dist:
            return dist
        dist = expanded


counts = {'transition_tables': 0, 'models_with_invariants': 0,
          'decision_checks': 0, 'roundtrip_checks': 0,
          'reachable_invariant_failures': 0, 'raw_commit_failures': 0,
          'pass_models': 0, 'overlap_guard_pairs': 0, 'formula_truth_checks': 0}

for table in itertools.product(range(4), repeat=4):
    counts['transition_tables'] += 1
    rules = []
    for i, choice in enumerate(table):
        if choice == 0:
            continue
        state = ('aa', 'bb')[i // 2]
        guard = 'p' if i % 2 else '!p'
        outcome = ('', 'reject denied unchanged', 'accept aa', 'accept bb')[choice]
        rules.append(f'rule rule_{i}: {state} + tick [{guard}] -> {outcome};')
    for initial in ('aa', 'bb'):
        dist = distances(table, initial)
        for allowed_mask, inv in enumerate(('false', 'state == aa', 'state == bb', 'true')):
            model = sut.parse(template(initial, rules, inv))
            counts['models_with_invariants'] += 1
            assert sut.parse(model.render()) == model
            assert sut.parse(model.render(True), 'english') == model
            counts['roundtrip_checks'] += 2
            for state in ('aa', 'bb'):
                for p in (False, True):
                    assert model.decide(state, 'tick', {'p': p}) == expected(table, state, p)
                    counts['decision_checks'] += 1
            allowed = {s for i, s in enumerate(('aa', 'bb')) if allowed_mask & (1 << i)}
            bad_reachable = set(dist) - allowed
            bad_raw = [(s,p) for s in ('aa','bb') for p in (False,True)
                       if expected(table,s,p)['class']=='Accept'
                       and expected(table,s,p)['state'] not in allowed]
            report = sut.check(model, 'independent-table-oracle')
            if bad_reachable:
                counts['reachable_invariant_failures'] += 1
                assert report['status'] == 'counterexample' and report['obligation'] == 'invariant'
                witness = report['witness']
                assert witness['state'] in bad_reachable
                assert len(witness['trace']) == min(dist[s] for s in bad_reachable)
                state = initial
                for row in witness['trace']:
                    assert row['state'] == state and row['event'] == 'tick'
                    assert row['outcome'] == expected(table, state, row['context']['p'])
                    state = row['outcome']['state']
                assert state == witness['state']
            elif bad_raw:
                counts['raw_commit_failures'] += 1
                assert report['status'] == 'counterexample' and report['obligation'] == 'invariant-preservation'
                row = report['witness']
                assert (row['state'], row['context']['p']) in bad_raw
                assert row['outcome'] == expected(table, row['state'], row['context']['p'])
            else:
                counts['pass_models'] += 1
                assert report['status'] == 'pass-with-scope'
                assert report['reachable_states'] == sorted(dist)
                assert report['input_tuples'] == 4
                assert report['default_count'] == table.count(0)
                expected_unreachable = sorted(f'rule_{i}' for i,c in enumerate(table)
                                              if c and ('aa','bb')[i//2] not in dist)
                assert report['unreachable_rules'] == expected_unreachable

# All 16 Boolean functions over p,q represented as independently generated DNF.
# Test pairwise overlap and every truth valuation; no host eval or SUT evaluator
# is used to construct expected values.
vals = list(itertools.product((False,True), repeat=2))

def dnf(mask):
    terms = []
    for i,(p,q) in enumerate(vals):
        if mask & (1 << i):
            terms.append('(' + ('p' if p else '!p') + ' & ' + ('q' if q else '!q') + ')')
    return ' | '.join(terms) or 'false'

for a,b in itertools.product(range(16),repeat=2):
    model = sut.parse(template('aa', [
        f'rule lhs: aa + tick [{dnf(a)}] -> accept bb;',
        f'rule rhs: aa + tick [{dnf(b)}] -> reject denied unchanged;'], facts='p, q'))
    report = sut.check(model, 'independent-mask-oracle')
    counts['overlap_guard_pairs'] += 1
    if a & b:
        assert report['status'] == 'counterexample' and report['obligation'] == 'determinism'
        row = report['witness']
        assert row['state']=='aa'
        i = vals.index((row['context']['p'], row['context']['q']))
        assert (a & b) & (1 << i)
        assert report['rules'] == ['lhs', 'rhs']
    else:
        assert report['status'] == 'pass-with-scope'
    for state in ('aa', 'bb'):
        for i,(p,q) in enumerate(vals):
            enabled = [] if state == 'bb' else [name for name,mask in (('lhs',a),('rhs',b)) if mask & (1 << i)]
            if len(enabled)>1:
                try:
                    model.decide(state,'tick',{'p':p,'q':q})
                except sut.Refusal:
                    pass
                else:
                    raise AssertionError('Expected overlap refusal')
            else:
                row = model.decide(state,'tick',{'p':p,'q':q})
                assert row['rule'] == (enabled[0] if enabled else 'default')
                assert row['class'] == ('Accept' if enabled == ['lhs'] else 'Reject')
                assert row['state'] == ('bb' if enabled == ['lhs'] else state)
            counts['formula_truth_checks'] += 1

# Deliberately vacuous diagnostic fixture: declared invariant is a tautology,
# one rule is contradictory at every source/context, one source is unreachable.
diag = sut.parse(template('aa', [
    'rule contradiction: aa + tick [p & !p] -> accept bb;',
    'rule unreachable_source: bb + tick [p] -> accept bb;'],
    'state == aa | state == bb'))
diag_report = sut.check(diag, 'diagnostic-example')
assert diag_report['status'] == 'pass-with-scope'
assert diag_report['unreachable_rules'] == ['contradiction','unreachable_source']

sha_after = hashlib.sha256((SOURCE / 'behavior.py').read_bytes()).hexdigest()
assert sha_before == sha_after, 'Source changed while running; rerun against stable source'
output = {'status':'passed', 'behavior_sha256':sha_after, 'counts':counts,
          'diagnostic_example':diag_report,
          'scope':'All 256 two-state/one-event/one-fact four-outcome tables, both initial states, all 4 state invariant truth sets; all 256 pairs of two-fact Boolean guards in DNF. Exact within these families only.',
          'independence':'Expected results derive from outcome tables and integer truth masks, without production formula evaluator or traversal.',
          'not_established':['universal implementation correctness','human intent','external context truth','factory lowering','temporal liveness','browser behavior']}
print(json.dumps(output,indent=2))
