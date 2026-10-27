#!/usr/bin/env python3
"""Requirement examples and strict-format controls for the independent map."""
import copy
import json
from pathlib import Path
import tempfile
import unittest

import priority_reference as ref


class PriorityReferenceTests(unittest.TestCase):
    def accept(self, before, q, after, payload):
        original = copy.deepcopy(before)
        actual = ref.step(before, q)
        self.assertEqual(before, original)
        self.assertEqual(actual['class'], 'Accept')
        self.assertIsNone(actual['reason'])
        self.assertEqual(actual['successor'], after)
        self.assertEqual(actual['effects'], [])
        self.assertEqual(actual['outbox'], [dict(ordinal=0, channel=300,
            destination='priority-events', idempotency=0,
            payload=[[140 + i, x] for i, x in enumerate(payload)])])
        return actual['successor']

    def reject(self, before, q, reason):
        self.assertEqual(ref.step(before, q), dict(successor=before,
            **{'class': 'Reject'}, reason=reason, effects=[], outbox=[]))

    def test_complete_carrier_order_rows_and_observations(self):
        states = ref.carrier()
        self.assertEqual([sum(x[0] == n for x in states) for n in range(3)], [1, 7, 37])
        self.assertEqual(states[0], (0, ()))
        self.assertEqual(states[1], (1, ()))
        self.assertEqual(states[2], (1, ((2, 0, 0),)))
        self.assertEqual(states, tuple(sorted(states, key=ref.state_order)))
        data = ref.relation()
        self.assertEqual(len(data['states']), 45)
        self.assertEqual(len(data['commands']), 16)
        self.assertEqual(len(data['rows']), 720)
        self.assertEqual(ref.report(data)['observations'], 29)
        self.assertEqual([(r['state'], r['input']) for r in data['rows']],
                         [(s, q) for s in range(45) for q in range(16)])

    def test_every_command_has_the_declared_meaning(self):
        self.assertEqual(ref.command(0), dict(q=0, kind='Insert', id=0, priority=0))
        self.assertEqual(ref.command(5), dict(q=5, kind='Insert', id=2, priority=1))
        self.assertEqual(ref.command(8), dict(q=8, kind='Cancel', id=2, priority=None))
        self.assertEqual(ref.command(14), dict(q=14, kind='Change', id=2, priority=1))
        self.assertEqual(ref.command(15), dict(q=15, kind='RemoveLowest', id=None, priority=None))
        for d in range(3):
            for p in range(2):
                self.assertEqual(ref.command(2*d+p)['id'], d)
                self.assertEqual(ref.command(9+2*d+p)['priority'], p)

    def test_insert_cancel_change_remove_and_frames(self):
        self.accept((0, ()), 3, (1, ((1, 1, 0),)), (0, 1, 1, 0))
        two = (2, ((0, 1, 1), (2, 0, 0)))
        self.accept(two, 6, (2, ((2, 0, 0),)), (1, 0, 2, 2))
        self.accept(two, 14, (2, ((0, 1, 1), (2, 1, 0))), (2, 2, 1, 2))
        self.accept(two, 15, (2, ((0, 1, 1),)), (3, 2, 2, 2))

    def test_overlap_precedence_and_missing_operations(self):
        full = (2, ((0, 0, 0), (1, 1, 1)))
        self.reject(full, 1, 201)
        self.reject(full, 4, 202)
        self.reject((2, ()), 0, 203)
        self.reject((0, ()), 6, 204)
        self.reject((0, ()), 9, 204)
        self.reject((0, ()), 15, 205)
        self.accept(full, 6, (2, ((1, 1, 1),)), (1, 0, 2, 2))
        self.accept(full, 12, (2, ((0, 0, 0), (1, 1, 1))), (2, 1, 1, 2))

    def test_same_priority_event_and_preserved_arrival(self):
        old = (2, ((0, 0, 1), (2, 0, 0)))
        self.accept(old, 9, old, (2, 0, 0, 2))
        self.accept(old, 10, (2, ((0, 1, 1), (2, 0, 0))), (2, 0, 1, 2))
        self.accept(old, 15, (2, ((0, 0, 1),)), (3, 2, 2, 2))

    def test_two_lifetime_insertions_identity_reuse_and_exhaustion(self):
        s = self.accept((0, ()), 0, (1, ((0, 0, 0),)), (0, 0, 0, 0))
        s = self.accept(s, 6, (1, ()), (1, 0, 2, 2))
        s = self.accept(s, 1, (2, ((0, 1, 1),)), (0, 0, 1, 1))
        s = self.accept(s, 6, (2, ()), (1, 0, 2, 2))
        self.reject(s, 0, 203)
        self.reject(s, 15, 205)

    def test_all_rows_close_over_carrier_and_reject_frames(self):
        carrier = set(ref.carrier())
        for s in ref.carrier():
            for q in range(16):
                row = ref.step(s, q)
                self.assertIn(row['successor'], carrier)
                if row['class'] == 'Reject':
                    self.assertEqual(row['successor'], s)
                    self.assertEqual(row['outbox'], [])
                else:
                    self.assertEqual(len(row['outbox']), 1)
                    self.assertIsNone(row['reason'])

    def test_invalid_states_commands_and_numeric_kinds(self):
        for n, entries in [(True, ()), (0, ((0, 0, 0),)), (2, ((0, 0, 0), (0, 1, 1))),
                           (2, ((0, 0, 0), (1, 1, 0))), (1, ((0, 1.0, 0),)),
                           (2, ((1, 0, 0), (0, 1, 1)))]:
            with self.assertRaises(ValueError): ref.state(n, entries)
        for q in [-1, 16, True, 1.0]:
            with self.assertRaises(ValueError): ref.command(q)

    def test_closed_reference_and_row_adversaries(self):
        good = ref.relation()
        ref.validate(good)
        mutations = [lambda d: d.update(extra=0), lambda d: d.update(genesis=False),
            lambda d: d['rows'].reverse(), lambda d: d['rows'].pop(),
            lambda d: d['rows'].__setitem__(1, copy.deepcopy(d['rows'][0])),
            lambda d: d['rows'][0].update(successor=True),
            lambda d: d['rows'][0].update(input=0.0),
            lambda d: d['commands'][0].update(id=False),
            lambda d: d['states'][0].update(n=0.0),
            lambda d: d['rows'][0]['outbox'][0].update(ordinal=False),
            lambda d: d['rows'][0]['outbox'][0]['payload'][0].__setitem__(1, True)]
        for mutate in mutations:
            bad = copy.deepcopy(good)
            mutate(bad)
            with self.assertRaises(ValueError): ref.validate(bad)

    def test_fresh_output_exact_recomputation_and_duplicate_json_keys(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp) / 'reference'
            result = ref.emit(directory)
            before = {p.name: p.read_bytes() for p in directory.iterdir()}
            self.assertEqual(ref.check(directory), result)
            self.assertEqual({p.name: p.read_bytes() for p in directory.iterdir()}, before)
            with self.assertRaises(FileExistsError): ref.emit(directory)
            p = directory / 'relation.json'
            p.write_text('{"schema":"x","schema":"y"}')
            with self.assertRaises(ValueError): ref.check(directory)
            p.write_bytes(b' ' * (ref.LIMIT + 1))
            with self.assertRaises(ValueError): ref.check(directory)


if __name__ == '__main__':
    unittest.main(verbosity=2)
