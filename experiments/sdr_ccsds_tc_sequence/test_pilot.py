#!/usr/bin/env python3
"""Independent finite oracle and negative controls for the CCSDS TC pilot.

This is NOT the ZenoFCIS checker. Use the synth CLI to obtain ZenoFCIS
semantic and target-conformance evidence from the supplied problem JSON files.
"""

import itertools
import json
import pathlib
import unittest

HERE = pathlib.Path(__file__).resolve().parent


def oracle(sequence, bypass):
    if bypass:
        return 0, 0
    return sequence, (sequence + 1) % 256


def evaluate(graph, values, assignment=None):
    """Independent interpreter for the small, closed finite synthesis subset."""
    nodes = []
    for node in graph['nodes']:
        if isinstance(node, dict):
            if assignment is None:
                raise ValueError('Unresolved hole')
            node = assignment[node['hole']]
        op, *args = node
        if op == 'input':
            value = values[args[0]]
        elif op == 'int':
            value = args[0]
        elif op == 'bool':
            value = args[0]
        elif op == 'add':
            value = nodes[args[0]] + nodes[args[1]]
        elif op == 'sub':
            value = nodes[args[0]] - nodes[args[1]]
        elif op == 'lt':
            value = nodes[args[0]] < nodes[args[1]]
        elif op == 'eq':
            value = nodes[args[0]] == nodes[args[1]]
        elif op == 'and':
            value = nodes[args[0]] and nodes[args[1]]
        elif op == 'not':
            value = not nodes[args[0]]
        elif op == 'select':
            value = nodes[args[1]] if nodes[args[0]] else nodes[args[2]]
        else:
            raise ValueError('Unsupported operation: ' + op)
        nodes.append(value)
    return tuple(nodes[i] for i in graph['roots'])


def problem(name):
    data = json.loads((HERE / (name + '.json')).read_text())
    assert data['schema'] == 'zeno-fcis/synthesis-problem/1'
    assert data['profile'] == 'zeno-fcis/finite-i128/1'
    return data


def cases():
    return itertools.product(range(256), [False, True])


def satisfying_assignments(spec):
    holes = [node for node in spec['sketch']['nodes'] if isinstance(node, dict)]
    winners = []
    for alternatives in itertools.product(*(hole['alternatives'] for hole in holes)):
        assignment = {hole['hole']: alt for hole, alt in zip(holes, alternatives)}
        for seq, bypass in cases():
            values = (seq, bypass)
            candidate = evaluate(spec['sketch'], values, assignment)[0]
            reference = oracle(seq, bypass)[0 if spec['outputs'][0]['name'] == 'sent' else 1]
            if candidate != reference or not (0 <= candidate <= 255):
                break
        else:
            winners.append(assignment)
    return winners


class CCSDSFinitePilot(unittest.TestCase):
    def test_contract_relations_exhaustively(self):
        for name, index in (('sent', 0), ('successor', 1)):
            spec = problem(name)
            # A true relation must accept exactly the oracle's result,
            # not merely some output for every input.
            accepted = 0
            for seq, bypass in cases():
                expected = oracle(seq, bypass)[index]
                for output in range(256):
                    (valid,) = evaluate(spec['contract'], (seq, bypass, output))
                    self.assertEqual(valid, output == expected,
                                     (name, seq, bypass, output))
                    accepted += bool(valid)
            self.assertEqual(accepted, 512)

    def test_hole_search_has_unique_correct_solution(self):
        for name in ('sent', 'successor'):
            with self.subTest(name=name):
                spec = problem(name)
                winners = satisfying_assignments(spec)
                self.assertEqual(len(winners), 1, winners)

    def test_wrap_and_bypass_replay(self):
        state = 0
        for i in range(1024):
            sent, state = oracle(state, False)
            self.assertEqual(sent, i % 256)
            self.assertEqual(state, (i + 1) % 256)
        self.assertEqual(oracle(255, False), (255, 0))
        self.assertEqual(oracle(173, True), (0, 0))
        self.assertEqual(oracle(0, False), (0, 1))

    def test_planted_defects_are_detected(self):
        mutants = {
            'increment_before_emit': lambda s, b: (0, 0) if b else ((s + 1) % 256, (s + 1) % 256),
            'unbounded_increment': lambda s, b: (0, 0) if b else (s, s + 1),
            'ignore_bypass_reset': lambda s, b: (0, s) if b else (s, (s + 1) % 256),
            'bypass_emits_saved_counter': lambda s, b: (s, 0) if b else (s, (s + 1) % 256),
        }
        for name, mutant in mutants.items():
            with self.subTest(name=name):
                witnesses = [(s, b) for s, b in cases() if mutant(s, b) != oracle(s, b)]
                self.assertTrue(witnesses, name)


if __name__ == '__main__':
    unittest.main()
