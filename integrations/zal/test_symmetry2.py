"""Finite symmetry regressions and reproducible mirrored-machine differential."""

from dataclasses import replace
from pathlib import Path
import json
import random
import subprocess
import sys
import unittest
from unittest.mock import patch

import behavior2 as b
import symmetry2 as s


BASE = """zal 2;
machine borrowers;
states env, user;
events left, right, tick;
context none;
var aa in 0..=1 initially 0;
var bb in 0..=1 initially 0;
initial user;
step atomic; frame control_and_variables;
default reject unavailable unchanged;
rule borrow_a: user + left [aa == 0] -> accept env, aa = 1;
rule borrow_b: user + right [bb == 0] -> accept env, bb = 1;
rule return_turn: env + tick [true] -> accept user;
player controller moves in user;
player environment moves in env;
symmetry rivals: swap (aa bb) (left right);
"""


class SymmetryDeclarationTests(unittest.TestCase):
    def test_paired_roundtrip(self):
        model = b.parse(BASE)
        self.assertEqual(model, b.parse(model.render()))
        self.assertEqual(model, b.parse(model.render(True), "english"))
        self.assertEqual(model.revision, b.parse(BASE.replace('(aa bb) (left right)', '(right left) (bb aa)')).revision)
        self.assertEqual(model.symmetries, (("rivals", (("aa", "bb"), ("left", "right"))),))

    def test_empty_declarations_do_not_change_data(self):
        model = b.parse(BASE)
        plain = replace(model, symmetries=())
        self.assertNotIn("symmetry", plain.data())
        self.assertNotEqual(model.revision, plain.revision)

    def test_symmetry_statement_is_registered_in_help(self):
        model = b.parse(BASE)
        entries = [e for e in b.legend(model)['entries'] if e['concept'] == 'symmetry']
        self.assertEqual(len(entries), 1)
        self.assertEqual(b.help_topic(model, 'symmetry:rivals')['entries'], entries)
        self.assertEqual(b.help_topic(model, 'grammar:symmetry')['entries'], entries)

    def test_declaration_refusals_name_statement(self):
        for text in ['(aa aa)', '(aa bb) (aa tick)', '(aa tick)', '(missing bb)', '(env aa)', 'aa bb']:
            with self.subTest(text=text), self.assertRaisesRegex(b.Refusal, "Symmetry rivals: declaration"):
                b.parse(BASE.replace('(aa bb) (left right)', text))

    def test_ranges_and_initial_values_match(self):
        for replacement in ['var bb in 0..=2 initially 0;', 'var bb in 0..=1 initially 1;']:
            with self.assertRaisesRegex(b.Refusal, "Symmetry rivals: declaration"):
                b.parse(BASE.replace('var bb in 0..=1 initially 0;', replacement))

    def test_generator_bound(self):
        source = BASE + '\n'.join(f'symmetry g{i}: swap (aa bb);' for i in range(4))
        with self.assertRaisesRegex(b.Refusal, "Symmetry .*: finiteness"):
            b.parse(source)

    def test_finite_domain_bound_names_statement(self):
        source = BASE.replace('in 0..=1 initially', 'in 0..=63 initially')
        with self.assertRaisesRegex(b.Refusal, 'Symmetry rivals: finiteness'):
            b.parse(source)

    def test_ambiguous_declaration_kind_refused(self):
        source = BASE.replace('states env, user;', 'states aa, env, user;')
        with self.assertRaisesRegex(b.Refusal, 'Symmetry rivals: declaration.*ambiguous'):
            b.parse(source)

    def test_owned_ir(self):
        model = b.parse(BASE)
        class TupleAlias(tuple):
            def __new__(cls, values):
                obj = super().__new__(cls, values)
                obj.values = list(values)
                return obj
            def __iter__(self):
                return iter(self.values)
        class NameAlias(str):
            pass
        pairs = model.symmetries[0][1]
        for items in [[], [('rivals', [('aa', 'bb')])], (('rivals', (['aa', 'bb'],)),),
                      TupleAlias(model.symmetries), (TupleAlias(model.symmetries[0]),),
                      ((NameAlias('rivals'), pairs),), (('rivals', TupleAlias(pairs)),),
                      (('rivals', (TupleAlias(pairs[0]), pairs[1])),),
                      (('rivals', ((NameAlias('aa'), 'bb'), pairs[1])),)]:
            with self.assertRaises(b.Refusal):
                replace(model, symmetries=items)

    def test_no_symmetry_option_is_boolean(self):
        for option in [0, 1, [], {}, 'false']:
            with self.assertRaisesRegex(b.Refusal, 'no_symmetry must be Boolean'):
                b.check(b.parse(BASE), no_symmetry=option)

    def test_automorphism_control(self):
        model = b.parse(BASE.replace('rule borrow_b: user + right [bb == 0] -> accept env, bb = 1;', ''))
        with self.assertRaisesRegex(b.Refusal, "Symmetry rivals: automorphism"):
            b.check(model)

    def test_claim_control(self):
        model = b.parse(BASE + 'property single_party: controller can force eventually (aa == 1);\n')
        with self.assertRaisesRegex(b.Refusal, "Symmetry rivals: claim-invariance.*observes a swapped name"):
            b.check(model)

    def test_every_claim_family(self):
        for claim in ['invariant observed: aa == 0;', 'require observed: aa\' == 0;',
                      'fairness observed: infinitely often (aa\' == 0);',
                      'property observed: always(aa == 0);']:
            with self.subTest(claim=claim), self.assertRaisesRegex(b.Refusal, 'claim-invariance'):
                b.check(b.parse(BASE + claim))

    def test_initial_fixed(self):
        with self.assertRaisesRegex(b.Refusal, 'Symmetry rivals: automorphism: initial'):
            b.check(b.parse(BASE.replace('(aa bb) (left right)', '(env user)')))

    def test_players_preserved(self):
        model = b.parse(BASE)
        with self.assertRaisesRegex(b.Refusal, 'Symmetry rivals: players'):
            s.check_players(model, 'rivals', {'env': 'user', 'user': 'env'})

    def test_valid_mirrored_rules(self):
        self.assertEqual(b.check(b.parse(BASE))['status'], 'pass-with-scope')

    def test_no_symmetry_still_checks_obligations(self):
        model = b.parse(BASE + 'invariant observed: aa == 0;')
        with self.assertRaisesRegex(b.Refusal, 'Symmetry rivals: claim-invariance'):
            b.check(model, no_symmetry=True)


def random_machine(rng, index):
    """Generate one half of a policy, then mirror its rules by a random swap."""
    x, y, observer = rng.sample(['aa', 'bb', 'cc'], 3)
    ex, ey, fixed_event = rng.sample(['left', 'right', 'tick'], 3)
    bound = rng.randint(1, 2)
    context = rng.choice(['none', 'ready'])
    integer_input = rng.choice([False, True])
    lines = ['zal 2;', f'machine generated_{index};', 'states env, user;',
             'events left, right, tick;', f'context {context};']
    if integer_input:
        lines.append('input amount in 0..=1;')
    lines += [f'var {n} in 0..={bound} initially 0;' for n in ['aa', 'bb', 'cc']]
    lines += ['initial user;', 'step atomic; frame control_and_variables;',
              'default reject unavailable unchanged;']
    for state in ['env', 'user']:
        guard = f'{x} {rng.choice(["==", "<=", ">="])} {rng.randint(0,bound)}'
        if context != 'none':
            guard += rng.choice([' & ready', ' & !ready'])
        if integer_input:
            guard += f' & amount == {rng.randint(0,1)}'
        target = rng.choice(['env', 'user'])
        outcome = (f'accept {target}, {x} = {rng.randint(0,bound)}, {observer} = {rng.randint(0,bound)}'
                   if rng.randrange(4) else 'reject denied unchanged')
        original = f'rule {state}_a: {state} + {ex} [{guard}] -> {outcome};'
        mirrored = original.replace(f'{state}_a:', f'{state}_b:')
        import re
        mirrored = re.sub(r'\b' + x + r'\b', y, mirrored)
        mirrored = re.sub(r'\b' + ex + r'\b', ey, mirrored)
        lines += [original, mirrored]
        lines.append(f'rule {state}_fixed: {state} + {fixed_event} [true] -> accept '
                     f'{rng.choice(["env", "user"])}, {observer} = {rng.randint(0,bound)};')
    invariant_bound = 2 * bound if index % 7 else rng.randint(0, 2 * bound)
    lines += [f'invariant total: {x} + {y} <= {invariant_bound};',
              f'require accounting: !accepted => ({x}\' + {y}\' == {x} + {y});',
              'player controller moves in user;', 'player environment moves in env;']
    if rng.choice([False, True]):
        lines.append(f'fairness visits: infinitely often ({observer}\' == {rng.randint(0,bound)});')
    for gi, objective in enumerate(b.GAME_OBJECTIVES):
        player = rng.choice(['controller', 'environment'])
        goal = rng.choice([f'{observer} == {rng.randint(0,bound)}',
                           f'{x} + {y} >= {rng.randint(0,2*bound)}',
                           f'state == {rng.choice(["user", "env"])}'])
        lines.append(f'property game_{gi}: {player} can force {objective} ({goal});')
    lines.append(f'symmetry rivals: swap ({x} {y}) ({ex} {ey});')
    return b.parse('\n'.join(lines))


def explore(graph):
    rows = [[] for _ in graph.configs]
    for ci, _, _, result in graph.raw():
        if result[0] != 'step':
            raise AssertionError(result[0])
        rows[ci].append(result[1])
    graph.explore(rows)
    return graph


class SymmetryGraphTests(unittest.TestCase):
    def test_cli_full_graph_and_owned_result(self):
        here = Path(__file__).parent
        source = here / 'examples/ladder_symmetry2.zal'
        commands = [[sys.executable, '-B', str(here / 'run.py'), 'check', str(source), *flags]
                    for flags in [[], ['--no-symmetry']]]
        results = [subprocess.run(command, text=True, capture_output=True) for command in commands]
        self.assertEqual([r.returncode for r in results], [0, 0])
        quotient, full = [json.loads(r.stdout) for r in results]
        for key in ['status', 'revision', 'properties', 'symmetry']:
            self.assertEqual(quotient[key], full[key])
        model = b.parse(source.read_text())
        revision = model.revision
        report = b.check(model)
        report['symmetry']['orbits'] = -1
        report['properties']['alice_funded']['winning_configurations'] = -1
        self.assertEqual(model.revision, revision)
        self.assertEqual(b.check(model)['symmetry']['orbits'], 36)

    def test_state_swap_and_ownership(self):
        source = '''zal 2;
machine state_swap;
states origin, xx, yy;
events left, right;
context none;
initial origin;
step atomic; frame control_and_variables;
default reject unavailable unchanged;
rule go_left: origin + left [true] -> accept xx;
rule go_right: origin + right [true] -> accept yy;
player controller moves in origin;
player environment moves in xx, yy;
property leaves: controller can force eventually (!(state == origin));
symmetry rivals: swap (xx yy) (left right);
'''
        model = b.parse(source)
        self.assertEqual(b.check(model)['properties'], b.check(model, no_symmetry=True)['properties'])
        self.assertEqual(b.check(model)['symmetry']['orbits'], 2)
        bad = source.replace('controller moves in origin;', 'controller moves in origin, xx;')
        bad = bad.replace('environment moves in xx, yy;', 'environment moves in yy;')
        with self.assertRaisesRegex(b.Refusal, 'Symmetry rivals: players'):
            b.check(b.parse(bad))

    def test_overlap_traps_and_reasons(self):
        for extra in ['rule extra_a: user + left [aa == 0] -> accept env, aa = 1;\n'
                      'rule extra_b: user + right [bb == 0] -> accept env, bb = 1;',
                      '']:
            model = b.parse(BASE + extra)
            s.check_obligations(model)
        self.assertEqual(b.check(b.parse(BASE + extra))['status'], 'pass-with-scope')
        trap = BASE.replace('[aa == 0]', '[true]').replace('[bb == 0]', '[true]')
        trap = trap.replace('aa = 1', 'aa += 1').replace('bb = 1', 'bb += 1')
        model = b.parse(trap)
        self.assertEqual(b.check(model)['obligation'], 'range')
        self.assertEqual(b.check(model, no_symmetry=True)['obligation'], 'range')
        asymmetric = trap.replace('bb += 1', 'bb = 1')
        with self.assertRaisesRegex(b.Refusal, 'Symmetry rivals: automorphism'):
            b.check(b.parse(asymmetric))
        rejection = BASE.replace('accept env, aa = 1', 'reject refused_a unchanged')
        rejection = rejection.replace('accept env, bb = 1', 'reject refused_b unchanged')
        with self.assertRaisesRegex(b.Refusal, 'Symmetry rivals: automorphism'):
            b.check(b.parse(rejection))

    def test_overlap_uses_whole_rule_signatures(self):
        source = '''zal 2;
machine overlapping_mirror;
states idle;
events tick;
context none;
var aa in 0..=1 initially 0;
var bb in 0..=1 initially 0;
initial idle;
step atomic; frame control_and_variables;
default reject unavailable unchanged;
rule r1: idle + tick [aa == 0] -> accept idle;
rule r2: idle + tick [true] -> accept idle;
rule r3: idle + tick [bb == 0] -> accept idle;
symmetry rivals: swap (aa bb);
'''
        model = b.parse(source)
        result = b.check(model)
        self.assertEqual(result['obligation'], 'determinism')
        self.assertEqual(result['rules'], ['r1', 'r2', 'r3'])
        self.assertEqual(result, b.check(model, no_symmetry=True))

    def test_example_and_legacy_revision_pins(self):
        from test_game2 import EXAMPLE_REVISIONS, parse_example
        here = Path(__file__).parent / 'examples'
        for name, revision in EXAMPLE_REVISIONS.items():
            self.assertEqual(parse_example(here / name).revision, revision)
        self.assertEqual(parse_example(here / 'withdrawal_game2.zal').revision,
                         '62f361bbe3dbdb74bdbfe7050626e26f4d029990be5ddc1d37f9187dd729b8e0')
        model = b.parse((here / 'ladder_symmetry2.zal').read_text())
        self.assertEqual(b.check(model)['properties'], b.check(model, no_symmetry=True)['properties'])
        self.assertEqual(b.check(model)['symmetry'], {'generators': 1, 'configurations': 54, 'orbits': 36})

    def test_canonical_control(self):
        graph = s.OrbitGraph(b.parse(BASE))
        for representative, members in graph.members.items():
            self.assertEqual(representative, min(members))
            self.assertEqual(graph.configs[representative], min(graph.configs[ci] for ci in members))
        self.assertEqual(len(graph.members), 6)

    def test_noncommuting_generators(self):
        model = b.parse(BASE.replace('var bb in 0..=1 initially 0;',
                                    'var bb in 0..=1 initially 0;\nvar cc in 0..=1 initially 0;')
                        .replace('symmetry rivals: swap (aa bb) (left right);',
                                 'symmetry first: swap (aa bb);\nsymmetry second: swap (bb cc);'))
        graph = s.OrbitGraph(model)
        self.assertEqual(len(graph.configs), 16)
        self.assertEqual(len(graph.members), 8)
        self.assertEqual(sorted(graph.weights.values()), [1, 1, 1, 1, 3, 3, 3, 3])

    def test_weighted_winning_counts_and_reduced_arena(self):
        model = b.parse(BASE + 'property either: controller can force eventually (aa + bb >= 1);')
        quotient, full = b.check(model), b.check(model, no_symmetry=True)
        self.assertEqual(quotient['properties'], full['properties'])
        self.assertEqual(quotient['symmetry'], {'generators': 1, 'configurations': 8, 'orbits': 6})
        graph = explore(s.OrbitGraph(model))
        self.assertLess(len(b._game_graph(graph, model, 'controller').successors),
                        len(b._game_graph(graph.full, model, 'controller').successors))

    def test_temporal_checks_receive_full_graph(self):
        model = b.parse(BASE + 'property bounded: always(within(2, aa + bb >= 0));')
        original = b._check_property
        seen = []
        def inspect(graph, *args):
            seen.append(len(graph.order))
            return original(graph, *args)
        with patch.object(b, '_check_property', inspect):
            b.check(model)
        graph = explore(s.OrbitGraph(model))
        self.assertEqual(seen, [len(graph.full.order)])
        self.assertGreater(len(graph.full.order), len(graph.order))

    def test_concrete_counterexample_trace_replays(self):
        model = b.parse(BASE + 'invariant limited: aa + bb <= 1;')
        result = b.check(model)
        self.assertEqual(result['obligation'], 'invariant')
        state, values = model.initial, model.initial_values()
        for step in result['witness']['trace']:
            self.assertEqual((state, values), (step['state'], step['variables']))
            outcome = model.decide(state, values, step['event'], step['context'])
            self.assertEqual(outcome, step['outcome'])
            state, values = outcome['state'], outcome['variables']
        self.assertEqual((state, values), (result['witness']['state'], result['witness']['variables']))

    def test_2000_mirrored_machines(self):
        rng = random.Random(260022)
        for index in range(2000):
            model = random_machine(rng, index)
            quotient = b.check(model)
            full = b.check(model, no_symmetry=True)
            for key in ['status', 'obligation', 'checks', 'properties', 'winning_configuration_count']:
                self.assertEqual(quotient.get(key), full.get(key), (index, key, model.render()))
            # Inspect all three objectives even when an earlier claim fails.
            graph = explore(s.OrbitGraph(model))
            for _, player, objective, formula in model.games:
                reduced_game = b._reduce_game(b._game_graph(graph, model, player), model, objective, formula)
                full_game = b._reduce_game(b._game_graph(graph.full, model, player), model, objective, formula)
                reduced_wins = b._fixed_point_winners(reduced_game)
                full_wins = b._fixed_point_winners(full_game)
                self.assertEqual(reduced_wins, b._parity_winners(reduced_game), index)
                self.assertEqual(full_wins, b._parity_winners(full_game), index)
                expected = {graph.full.order[node] for node in full_game.configuration_nodes if node in full_wins}
                actual = {ci for node in reduced_game.configuration_nodes if node in reduced_wins
                          for ci in graph.members[graph.order[node]]}
                self.assertEqual(actual, expected, (index, objective, model.render()))




if __name__ == '__main__':
    unittest.main()
