"""L1/L2 light tests: independent controls and required authority-path structure.

Native semantic tests are separate. These guards enforce the named construction
path and deliberately make no assertion that Rust compiled or receipts replayed.
"""
from copy import deepcopy
from pathlib import Path
import re
import unittest

import loop_completion_controls as R
import loop_learn_reference as L

ROOT = Path(__file__).resolve().parent.parent


def function_body(source, name):
    # Strip comments and literals so a quoted or commented call cannot pass.
    clean = re.sub(r'//[^\n]*|/\*.*?\*/|"(?:\\.|[^"\\])*"', '', source, flags=re.S)
    match = re.search(r'\bfn\s+' + re.escape(name) + r'\b', clean)
    if not match:
        return ''
    start = clean.find('{', match.end())
    depth = 0
    for end in range(start, len(clean)):
        depth += (clean[end] == '{') - (clean[end] == '}')
        if depth == 0:
            return clean[start:end + 1]
    return ''


def flow_guard(learn, split, pipeline, doors):
    learner = function_body(learn, 'run')
    strategy = function_body(split, 'command')
    capture = function_body(pipeline, 'capture_and_gate')
    exact = function_body(doors, 'checked')
    return all((bool(re.search(r'\.capture_and_gate\s*\(', learner)),
                bool(re.search(r'\.capture_and_gate\s*\(', strategy)),
                bool(re.search(r'ReferenceTable::from_program\s*\(', capture)),
                bool(re.search(r'\.program\s*\(', capture)),
                len(re.findall(r'self\.gate\s*\(', capture)) == 2,
                bool(re.search(r'accepted\s*\(\s*&capture_check\s*\)', capture)),
                bool(re.search(r'crate::transform::check\s*\(', exact)),
                bool(re.search(r'crate::transform::replay\s*\(', exact))))


class CompletionControls(unittest.TestCase):
    def test_planted_report_faults_are_killed_and_disabling_comparison_fails(self):
        self.assertEqual(R.self_test(), [])
        self.assertGreater(len(R.self_test(stub=True)), 8)
        self.assertEqual(R.self_test(), [])

    def test_actual_source_crosses_capture_then_the_existing_checker_and_replay(self):
        paths = ['loop_learn.rs', 'loop_strategy_profiles.rs', 'loop_pipeline.rs', 'loop_doors_command.rs']
        sources = [(ROOT / 'crates/zeno-fcis-cli/src' / path).read_text() for path in paths]
        self.assertTrue(flow_guard(*sources))
        for index in (0, 1):
            mutant = sources.copy()
            mutant[index] = mutant[index].replace('.capture_and_gate(', '.gate(')
            self.assertFalse(flow_guard(*mutant), paths[index])
        mutant = sources.copy()
        mutant[2] = mutant[2].replace('ReferenceTable::from_program(', 'unchecked_table(')
        mutant[2] += '\n// ReferenceTable::from_program(&reference, max_tuples)\n'
        self.assertFalse(flow_guard(*mutant))
        mutant = sources.copy()
        mutant[3] = mutant[3].replace('crate::transform::replay(', 'unchecked_replay(')
        self.assertFalse(flow_guard(*mutant))

    def test_baseline_sources_fail_the_new_flow_guard(self):
        # The original fit-only workflow is retained as a manual comparator;
        # it cannot pass a guard for the new automatic finishing mechanism.
        source = 'fn run() { return examples_fit(); }'
        self.assertFalse(flow_guard(source, source, source, source))

    def test_four_node_macro_is_outside_size_three_and_inside_size_four(self):
        spec = {'schema': L.SCHEMA, 'inputs': [{'name': name, 'domain': {
            'kind': 'Int', 'min': -1, 'max': 1}} for name in ('x', 'y')],
            'outputs': [{'kind': 'Int', 'min': 0, 'max': 2}],
            'vocabulary': {'constants': [], 'arithmetic': ['+'], 'macros': ['abs']},
            'max_size': 3, 'examples': [{'input': [x, y], 'output': [abs(x+y)]}
                                     for x in range(-1, 2) for y in range(-1, 2)]}
        before = deepcopy(spec)
        self.assertEqual(L.analyze(spec)['status'], 'outside-vocabulary')
        self.assertEqual(spec, before)
        larger = deepcopy(spec)
        larger['max_size'] = 4
        after = L.analyze(larger)
        self.assertEqual(after['status'], 'determined')
        self.assertEqual(after['minimum_sizes'], [4])

    def test_native_comparison_fixtures_and_budget_are_frozen_and_complete(self):
        for identifier, kind, problem, program in R.fixtures():
            encoded = R.C.encoding(R.C.program(program))
            rows = R.IR.table(R.IR.decode(encoded))
            self.assertLessEqual(len(rows), R.BOUNDS['max_tuples'])
            self.assertLessEqual(4 * len(rows), R.BOUNDS['max_gate_pairs'])
            if kind == 'learn':
                analyzed = L.analyze(problem)
                self.assertEqual(analyzed['status'], 'determined', identifier)
                self.assertEqual(analyzed['counts'], [1], identifier)
            else:
                self.assertEqual(problem['learn']['original_sha256'], R.hashlib.sha256(encoded).hexdigest())


if __name__ == '__main__':
    unittest.main()
