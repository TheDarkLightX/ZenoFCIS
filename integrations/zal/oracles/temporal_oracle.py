"""Independent temporal oracle for zal/finite-fsm/2; no production files edited.

Expected verdicts come from primitive outcome tables, this file's own atom
evaluator, brute-force lasso enumeration and the ZAL-T macro expansion for
finite words. They never use behavior2's holds, graph search, fixpoints or
evaluate_finite. Production parse/check/trace_properties are only called on
the actual side of assertions. Exhaustive within the named small families,
and bounded in lasso length; this is not a proof for all ZAL.
"""
import hashlib
import itertools
import json
from pathlib import Path
import random
import sys

SOURCE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SOURCE))
from snapshot_loader import load_behavior
sut, sha_before = load_behavior(SOURCE / 'behavior2.py')


def term(node, step):
    op = node[0]
    if op == 'int':
        return node[1]
    if op in ('var', 'var_next'):
        return (step['outcome'] if op == 'var_next' else step)['variables'][node[1]]
    if op == 'input':
        return step['context'][node[1]]
    left, right = term(node[1], step), term(node[2], step)
    return left + right if op == 'add' else left - right


def atom(node, step):
    op = node[0]
    if op in ('true', 'false'):
        return op == 'true'
    if op == 'fact':
        return step['context'][node[1]]
    if op == 'state':
        return step['state'] == node[1]
    if op == 'state_next':
        return step['outcome']['state'] == node[1]
    if op == 'event':
        return step['event'] == node[1]
    if op == 'accepted':
        return step['outcome']['class'] == 'Accept'
    if op == 'cmp':
        left, right = term(node[2], step), term(node[3], step)
        return {'==': left == right, '!=': left != right, '<': left < right,
                '<=': left <= right, '>': left > right, '>=': left >= right}[node[1]]
    raise AssertionError('not an atom: ' + op)


def connective(node, value):
    """Boolean connectives over a position-indexed evaluator; None when node is temporal or atomic."""
    op = node[0]
    if op == 'not':
        return not value(node[1])
    if op == 'and':
        return all(value(child) for child in node[1:])
    if op == 'or':
        return any(value(child) for child in node[1:])
    if op == 'implies':
        return (not value(node[1])) or value(node[2])
    if op == 'iff':
        return value(node[1]) == value(node[2])
    return None


def on_lasso(node, steps, loop, i):
    """Truth at position i of the infinite word steps[:loop] (steps[loop:])^omega."""
    successor = lambda j: j + 1 if j + 1 < len(steps) else loop
    def future(j):
        seen = []
        while j not in seen:
            seen.append(j)
            j = successor(j)
        return seen
    op = node[0]
    here = lambda child: on_lasso(child, steps, loop, i)
    result = connective(node, here)
    if result is not None:
        return result
    if op == 'always':
        return all(on_lasso(node[1], steps, loop, j) for j in future(i))
    if op == 'never':
        return not any(on_lasso(node[1], steps, loop, j) for j in future(i))
    if op == 'eventually':
        return any(on_lasso(node[1], steps, loop, j) for j in future(i))
    if op == 'next':
        return on_lasso(node[1], steps, loop, successor(i))
    if op == 'within':
        j = i
        for _ in range(node[1] + 1):
            if on_lasso(node[2], steps, loop, j):
                return True
            j = successor(j)
        return False
    if op == 'until':
        for j in future(i):
            if on_lasso(node[2], steps, loop, j):
                return True
            if not on_lasso(node[1], steps, loop, j):
                return False
        return False
    return atom(node, steps[i])


def on_word(node, word, i):
    """ZAL-T finite word by recursive unfolding: strong next; W0(P)=P, W(k+1)(P)=P or next Wk(P)."""
    op = node[0]
    result = connective(node, lambda child: on_word(child, word, i))
    if result is not None:
        return result
    has_next = i + 1 < len(word)
    if op == 'next':
        return has_next and on_word(node[1], word, i + 1)
    if op == 'within':
        if node[1] == 0:
            return on_word(node[2], word, i)
        return on_word(node[2], word, i) or has_next and on_word(['within', node[1] - 1, node[2]], word, i + 1)
    if op == 'always':
        return on_word(node[1], word, i) and (not has_next or on_word(node, word, i + 1))
    if op == 'never':
        return not on_word(node[1], word, i) and (not has_next or on_word(node, word, i + 1))
    if op == 'eventually':
        return on_word(node[1], word, i) or has_next and on_word(node, word, i + 1)
    if op == 'until':
        return on_word(node[2], word, i) or on_word(node[1], word, i) and has_next and on_word(node, word, i + 1)
    return atom(node, word[i])


HEADER = ['zal 2;', 'machine oracle;', 'states aa, bb;', 'events tick;', 'context p;',
          'step atomic; frame control_and_variables;', 'default reject no_match unchanged;']
# Property text, its kind, and (for the var family) whether it reads the variable.
PROPERTIES = [
    ('always(p | state == aa)', 'safety'), ('never(state == bb & accepted)', 'safety'),
    ('always(state == aa => next(state\' == bb))', 'next'), ('next(state\' == aa)', 'next'),
    ('always(p => within(0, state\' == bb))', 'bounded'), ('always(p => within(1, state\' == bb))', 'bounded'),
    ('always(state == aa => within(2, state == bb))', 'bounded'), ('within(1, state\' == bb)', 'bounded'),
    ('always(state == aa => eventually(state\' == bb))', 'liveness'), ('eventually(state == bb)', 'liveness'),
    ('always(p => until(state == aa, state\' == bb))', 'liveness'), ('until(!p, state == bb)', 'liveness'),
]
VAR_PROPERTIES = [('always(x == 0 | state == bb)', 'safety'), ('always(p => within(1, x\' == 1))', 'bounded'),
                  ('always(state == aa => eventually(x\' == 1))', 'liveness'),
                  ('always(x == 1 => next(state == bb))', 'next')]
FAIRNESS = ['p', 'state == bb', 'accepted']


def machine_steps(table, variable):
    """Primitive transition function from the outcome table, independent of the checker."""
    configs = [(s, x) for s in ('aa', 'bb') for x in ((0, 1) if variable else (0,))]
    def step(config, p):
        state, x = config
        slot = configs.index(config) * 2 + p
        choice = table[slot]
        target, post_x = [(state, x), ('aa', 0 if variable else x), ('bb', 1 if variable else x),
                          ('aa', x), ('bb', x)][choice]
        variables = {'x': x} if variable else {}
        post = {'x': post_x} if variable else {}
        return {'state': state, 'variables': variables, 'event': 'tick', 'context': {'p': bool(p)},
                'outcome': {'class': 'Accept' if choice else 'Reject', 'state': target, 'variables': post}}, (target, post_x)
    return configs, step


def source_for(table, variable, initial, prop, fairness):
    configs, _ = machine_steps(table, variable)
    rules = []
    for slot, choice in enumerate(table):
        if choice == 0:
            continue
        (state, x), p = configs[slot // 2], slot % 2
        guard = ('p' if p else '!p') + (f' & x == {x}' if variable else '')
        outcome = {1: 'accept aa' + (', x = 0' if variable else ''), 2: 'accept bb' + (', x = 1' if variable else ''),
                   3: 'accept aa', 4: 'accept bb'}[choice]
        rules.append(f'rule r{slot}: {state} + tick [{guard}] -> {outcome};')
    lines = HEADER + [f'initial {initial};'] + (['var x in 0..=1 initially 0;'] if variable else []) + rules
    lines.append(f'property target: {prop};')
    if fairness:
        lines.append(f'fairness fair: infinitely often ({fairness});')
    return '\n'.join(lines) + '\n'


def lassos(table, variable, initial, prefix_bound, cycle_bound):
    _, step = machine_steps(table, variable)
    start = (initial, 0)
    for length in range(prefix_bound + 1):
        for choices in itertools.product((0, 1), repeat=length):
            prefix, config = [], start
            for p in choices:
                row, config = step(config, p)
                prefix.append(row)
            for cycle_length in range(1, cycle_bound + 1):
                for loop_choices in itertools.product((0, 1), repeat=cycle_length):
                    cycle, current = [], config
                    for p in loop_choices:
                        row, current = step(current, p)
                        cycle.append(row)
                    if current == config:
                        yield prefix + cycle, len(prefix)


def genuine(steps, table, variable, initial):
    """Every witness step is the table's own step and the steps are continuous from the initial configuration."""
    _, step = machine_steps(table, variable)
    config = (initial, 0)
    for row in steps:
        expected, after = step(config, int(row['context']['p']))
        if (any(row[k] != expected[k] for k in ('state', 'variables', 'event', 'context'))
                or {k: row['outcome'][k] for k in ('class', 'state', 'variables')} != expected['outcome']):
            return False
        config = after
    return True


def run(tables, variable, initials, properties, prefix_bound, cycle_bound, counts):
    for table in tables:
        for initial in initials:
            words = list(lassos(table, variable, initial, prefix_bound, cycle_bound))
            for prop, kind in properties:
                for fairness in (FAIRNESS if kind == 'liveness' else [None]):
                    model = sut.parse(source_for(table, variable, initial, prop, fairness))
                    formula = model.properties[0][1].data()
                    report = sut.check(model, 'temporal-oracle')
                    fair = ['infinitely_often', model.fairness[0][1].data()] if fairness else None
                    violated = any(not on_lasso(formula, w, loop, 0) and
                                   (fair is None or any(atom(fair[1], w[j]) for j in range(loop, len(w))))
                                   for w, loop in words)
                    actual = report['status'] == 'counterexample'
                    assert actual == violated, (table, initial, prop, fairness, report.get('witness'))
                    counts['machine_verdicts'] += 1
                    if not actual:
                        continue
                    counts['counterexamples'] += 1
                    witness = report['witness']
                    if 'cycle' in witness:
                        word = witness['prefix'] + witness['cycle']
                        loop = len(witness['prefix'])
                        assert witness['cycle'] and genuine(word, table, variable, initial)
                        last = witness['cycle'][-1]['outcome']
                        assert (last['state'], last['variables']) == (witness['cycle'][0]['state'], witness['cycle'][0]['variables'])
                        assert not on_lasso(formula, word, loop, 0)
                        assert any(atom(fair[1], row) for row in witness['cycle'])
                        counts['lassos_validated'] += 1
                    else:
                        assert genuine(witness['trace'], table, variable, initial)
                        word = witness['trace']
                        # A safety-style witness is a bad prefix: the property fails on the finite word itself.
                        assert not on_word(formula, word, 0)
                        counts['traces_validated'] += 1


def finite_truth_table(tables, counts):
    """Every word of 1..5 events over a few machines, compared with trace_properties."""
    formulas = [p for p, _ in PROPERTIES] + ['always(p => next(p))', 'always(p => within(2, !p))', 'within(3, p)']
    for table in tables:
        for prop in formulas:
            model = sut.parse(source_for(table, False, 'aa', prop, 'p' if 'eventually' in prop or 'until' in prop else None))
            formula = model.properties[0][1].data()
            for length in range(1, 6):
                for choices in itertools.product((False, True), repeat=length):
                    inputs = [{'event': 'tick', 'context': {'p': p}} for p in choices]
                    actual = sut.trace_properties(model, inputs)['properties']['target']
                    if any(bound >= length for bound in within_bounds(formula)):
                        assert actual['result'] == 'refused'
                        counts['finite_refusals'] += 1
                        continue
                    word = [{**row['pre'], 'event': row['event'], 'context': row['context'], 'outcome': row['outcome']}
                            for row in sut.replay(model, inputs)]
                    expected = on_word(formula, word, 0)
                    assert (actual['result'] == 'satisfied') == expected, (table, prop, choices, actual)
                    counts['finite_verdicts'] += 1


def within_bounds(node):
    found = [node[1]] if node[0] == 'within' else []
    return found + [b for child in node[1:] if isinstance(child, list) for b in within_bounds(child)]


def main(quick=False):
    counts = {'machine_verdicts': 0, 'counterexamples': 0, 'lassos_validated': 0, 'traces_validated': 0,
              'finite_verdicts': 0, 'finite_refusals': 0}
    plain = list(itertools.product(range(3), repeat=4))
    rng = random.Random(20261019)
    with_var = [tuple(rng.randrange(5) for _ in range(8)) for _ in range(12 if quick else 60)]
    run(plain[::9] if quick else plain, False, ('aa',) if quick else ('aa', 'bb'), PROPERTIES, 4, 4, counts)
    run(with_var, True, ('aa',), VAR_PROPERTIES + PROPERTIES[:4], 5, 4, counts)
    finite_truth_table(plain[::27] if quick else plain[::5], counts)
    sha_after = hashlib.sha256((SOURCE / 'behavior2.py').read_bytes()).hexdigest()
    assert sha_before == sha_after, 'Source changed while running; rerun against stable source'
    return {'status': 'passed', 'behavior2_sha256': sha_after, 'counts': counts,
            'scope': ('Two-state one-event one-fact machines: all 81 outcome tables from both initial states and 12 '
                      'properties with 3 fairness choices for liveness; 60 seeded tables with one variable in 0..=1; '
                      'lassos with prefix at most 4 or 5 and cycle at most 4 steps; every finite word of 1..5 '
                      'events over 17 tables and 15 formulas.' if not quick else 'Reduced family for the unit suite.'),
            'independence': ('Expected verdicts come from outcome tables, this file\'s atom evaluator, brute-force '
                             'lassos and the recursive ZAL-T macro expansion, not from the checker.'),
            'not_established': ['universal implementation correctness', 'larger machines', 'lassos longer than the '
                                'bounds', 'human intent', 'external context truth', 'factory lowering']}


if __name__ == '__main__':
    print(json.dumps(main('--quick' in sys.argv), indent=2))
