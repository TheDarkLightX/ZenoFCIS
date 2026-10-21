#!/usr/bin/env python3
"""Semantic and mutation controls for the finite refinement reference."""
# SPDX-License-Identifier: MIT OR Apache-2.0
# LLM-generated implementation; checked by the repository's qualification gates.
import json
import unittest
from dataclasses import replace

import make_refinement_example as fixture
import refinement_reference as ref


def changed(table, ordinal, *, successor=None, observation=None):
    rows = list(table.rows)
    post, observed = rows[ordinal]
    rows[ordinal] = (post if successor is None else successor,
                     observed if observation is None else observation)
    return replace(table, rows=tuple(rows))


class RefinementTests(unittest.TestCase):
    def setUp(self):
        self.a, self.c, self.mapping, self.invalid = ref.reservation()

    def answer(self, concrete=None, mapping=None, budget=262144):
        return ref.check(self.a, concrete or self.c, mapping or self.mapping, self.invalid, budget)

    def test_reservation_all_384_pairs_and_228_invalid_pairs(self):
        self.assertEqual((self.a.states, self.c.states, self.c.inputs), (26, 64, 6))
        self.assertEqual(sum(x is None for x in self.mapping) * 6, 228)
        self.assertEqual(self.answer(), ref.Answer("refines", 384))
        for budget in (0, 1, 383, 384):
            self.assertEqual(self.answer(budget=budget).verdict,
                             "refines" if budget == 384 else "inconclusive")
            self.assertEqual(self.answer(budget=budget).checked_rows, budget)

    def test_generated_contracts_independently_match_every_semantic_row(self):
        for label, table in (("abstract", self.a), ("concrete", self.c)):
            directory = fixture.EXAMPLE / label
            actual = ref.fixture_table((directory / "project.zeno").read_text(),
                                       json.loads((directory / "v2/policy.json").read_text()))
            self.assertEqual(actual, table)
        mapping = json.loads((fixture.EXAMPLE / "abstraction.json").read_text())
        self.assertEqual(mapping["abstraction"], list(self.mapping))

    def test_lost_retirement_and_fault_beyond_budget(self):
        mutant = changed(self.c, 9 * 6 + 3, successor=0)
        self.assertEqual(self.answer(mutant), ref.Answer("counterexample", 58, "successor-refinement", 9, 3))
        self.assertEqual(self.answer(mutant, budget=57), ref.Answer("inconclusive", 57))

    def test_each_invalid_row_is_checked(self):
        count = 0
        for state, abstract in enumerate(self.mapping):
            if abstract is not None:
                continue
            for command in range(6):
                ordinal = state * 6 + command
                mutant = changed(self.c, ordinal, observation=ref.Observation("Reject", 203))
                self.assertEqual(self.answer(mutant), ref.Answer("counterexample", ordinal + 1,
                                                                "malformed-state", state, command))
                count += 1
        self.assertEqual(count, 228)

    def test_initial_coverage_valid_to_invalid_and_invalid_movement(self):
        self.assertEqual(self.answer(replace(self.c, initial=9)).obligation, "initial-state")
        m = tuple(0 if x == 1 else x for x in self.mapping)
        self.assertEqual(self.answer(mapping=m).obligation, "abstract-state-coverage")
        self.assertEqual(self.answer(changed(self.c, 0, successor=1)).obligation, "successor-refinement")
        self.assertEqual(self.answer(changed(self.c, 6, successor=0)).obligation, "malformed-state")

    def test_every_observation_part_and_smallest_state_major_witness(self):
        original = self.c.rows[0][1]
        event = original.deliveries[0]
        alternatives = [replace(original, kind="CommittedFailure"), replace(original, reason=201)]
        for slot, value in enumerate((1, 301, b"other", ((140, 1),), 1)):
            bad = list(event)
            bad[slot] = value
            alternatives.append(replace(original, deliveries=(tuple(bad),)))
        for bad in alternatives:
            mutant = changed(self.c, 0, observation=bad)
            mutant = changed(mutant, 57, successor=0)
            self.assertEqual(self.answer(mutant), ref.Answer("counterexample", 1, "observation-refinement", 0, 0))
        second = (1, 301, b"two", ((140, 2),), 1)
        a = changed(self.a, 0, observation=replace(original, deliveries=(event, second)))
        c = changed(self.c, 0, observation=replace(original, deliveries=(second, event)))
        self.assertEqual(ref.check(a, c, self.mapping, self.invalid).obligation, "observation-refinement")

    def test_duplicate_representatives_and_reject_cannot_move_between_them(self):
        allow, deny = ref.Observation("Accept", None), ref.Observation("Reject", 200)
        a = ref.Table(2, 2, 0, ((1, allow), (0, deny), (0, allow), (1, deny)))
        c = ref.Table(5, 2, 0, ((1, allow), (0, deny), (2, allow), (1, deny),
                              (3, allow), (2, deny), (0, allow), (3, deny),
                              (4, self.invalid), (4, self.invalid)))
        mapping = (0, 1, 0, 1, None)
        self.assertEqual(ref.check(a, c, mapping, self.invalid).verdict, "refines")
        bad = changed(c, 1, successor=2)
        self.assertEqual(ref.check(a, bad, mapping, self.invalid).obligation, "reject-is-no-op")
        permutation = (4, 2, 0, 3, 1)
        rows, mapped = [None] * 10, [None] * 5
        for old, new in enumerate(permutation):
            mapped[new] = mapping[old]
            for command in range(2):
                successor, observation = c.rows[old * 2 + command]
                rows[new * 2 + command] = (permutation[successor], observation)
        self.assertEqual(ref.check(a, replace(c, initial=permutation[0], rows=tuple(rows)),
                                   tuple(mapped), self.invalid).verdict, "refines")
        for ordinal, (original, observation) in enumerate(c.rows):
            state, command = divmod(ordinal, 2)
            for successor in range(5):
                allowed = successor == state if mapping[state] is None or command else mapping[successor] == 1 - mapping[state]
                answer = ref.check(a, changed(c, ordinal, successor=successor), mapping, self.invalid)
                self.assertEqual(answer.verdict == "refines", allowed)

    def test_comparison_stub_and_nested_boolean_alias_are_rejected(self):
        import check_contract_refinement as gate
        gate.self_test()
        expected = gate.projection(ref.Answer("counterexample", 7, "malformed-state", 1, 0))
        altered = dict(expected, witness=dict(expected["witness"], state=True))
        self.assertEqual(gate.compare(altered, expected), ["witness"])
        with self.assertRaises(gate.Mismatch):
            gate.comparison_controls(lambda *_: [])

    def test_budget_types(self):
        for invalid in (True, -1, 262145, 1.0):
            with self.assertRaises(ValueError):
                self.answer(budget=invalid)


if __name__ == "__main__":
    unittest.main()
