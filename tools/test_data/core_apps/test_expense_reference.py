"""Unit tests for the bounded expense-approval reference (brief.md rules)."""

import itertools
import unittest

import expense_reference as er

AMOUNT, KEY, BUDGET, SPENT, QUORUM = 3, 1, 7, 4, 2
ENQ, VOT, EXE = 160, 161, 162
EMPTY, PENDING, EXECUTED = 0, 1, 2
CHANNEL = 300
REASONS = {200, 201, 202, 203, 204, 205}

SPEC_DOMAINS = (
    tuple(range(8)), (0, 1, 2), (0, 1), (0, 1), (0, 1),
    (ENQ, VOT, EXE), tuple(range(8)), (0, 1), (0, 1, 2), (0, 1),
)


def spec_legal(pre):
    rem, phase, v0, v1, v2 = pre
    if phase == EMPTY:
        return rem == BUDGET and v0 == v1 == v2 == 0
    if phase == PENDING:
        return rem == BUDGET
    return phase == EXECUTED and rem == SPENT and v0 + v1 + v2 >= QUORUM


class DomainTests(unittest.TestCase):
    def test_domains_match_spec_and_genesis(self):
        self.assertEqual(tuple(tuple(d) for d in er.DOMAINS), SPEC_DOMAINS)
        self.assertEqual(er.GENESIS, (7, 0, 0, 0, 0))
        self.assertTrue(er.valid(er.GENESIS))

    def test_product_size_and_uniqueness(self):
        rows = list(itertools.product(*er.DOMAINS))
        self.assertEqual(len(rows), 55296)
        self.assertEqual(len(set(rows)), 55296)


class BoundaryTests(unittest.TestCase):
    def test_out_of_domain_inputs_raise(self):
        good = [7, 0, 0, 0, 0, ENQ, 3, 1, 0, 1]
        for i, bad in enumerate([-1, 3, 2, 2, 2, 159, 8, 2, 3, 2]):
            row = list(good)
            row[i] = bad
            with self.assertRaises(ValueError):
                er.decision(row)
        for malformed in (good[:9], good + [0], 7, "7000016031", None):
            with self.assertRaises(ValueError):
                er.decision(malformed)
        for bad_type in (7.0, True, "7"):
            row = list(good)
            row[0] = bad_type
            with self.assertRaises(ValueError):
                er.decision(row)

    def test_valid_contract(self):
        self.assertEqual(er.valid(list(er.GENESIS)), er.valid(er.GENESIS))
        self.assertTrue(er.valid((7, 1, 1, 0, 1)))
        self.assertTrue(er.valid((4, 2, 0, 1, 1)))
        for illegal in ((6, 1, 0, 0, 0), (7, 0, 1, 0, 0), (7, 2, 1, 1, 0),
                        (4, 2, 1, 0, 0), (4, 1, 0, 0, 0)):
            self.assertFalse(er.valid(illegal), illegal)
        for bad in ((7, 1, 0, 0, 0, 0), (8, 0, 0, 0, 0), (7, 3, 0, 0, 0),
                    (7, 1, True, 0, 0)):
            with self.assertRaises(ValueError):
                er.valid(bad)


class FullProductTests(unittest.TestCase):
    def test_reject_contract_everywhere(self):
        for row in itertools.product(*SPEC_DOMAINS):
            d = er.decision(row)
            self.assertEqual(set(d), {'class', 'reason', 'post', 'deliveries'})
            if d['class'] != 'Reject':
                continue
            self.assertIn(d['reason'], REASONS)
            self.assertEqual(d['post'], list(row[:5]))
            self.assertEqual(d['deliveries'], [])

    def test_accept_invariants_everywhere(self):
        for row in itertools.product(*SPEC_DOMAINS):
            d = er.decision(row)
            if d['class'] != 'Accept':
                continue
            self.assertIsNone(d['reason'])
            self.assertTrue(spec_legal(d['post']), (row, d))
            pre, action, principal = row[:5], row[5], row[8]
            if action == ENQ:
                self.assertEqual(tuple(pre), er.GENESIS)
                self.assertEqual(d['post'], [7, 1, 0, 0, 0])
                self.assertEqual(d['deliveries'], [])
            elif action == VOT:
                self.assertEqual(pre[1], PENDING)
                want = list(pre)
                want[2 + principal] = 1
                self.assertEqual(d['post'], want)
                self.assertEqual(d['deliveries'], [])
            elif pre[1] == PENDING:
                self.assertGreaterEqual(sum(pre[2:5]), QUORUM)
                self.assertEqual(d['post'], [4, 2] + list(pre[2:5]))
                self.assertEqual(d['deliveries'], [(CHANNEL, [AMOUNT, KEY])])
            else:
                self.assertEqual((pre[1], d['post']), (EXECUTED, list(pre)))
                self.assertEqual(d['deliveries'], [])

    def test_every_command_has_success_witness(self):
        accepted = set()
        for row in itertools.product(*SPEC_DOMAINS):
            if er.decision(row)['class'] == 'Accept':
                accepted.add(row[5])
        self.assertEqual(accepted, {ENQ, VOT, EXE})


class PrecedenceTests(unittest.TestCase):
    def test_invalid_prestate_beats_auth_and_subject(self):
        d = er.decision([5, 0, 0, 0, 0, ENQ, 4, 0, 0, 0])
        self.assertEqual((d['class'], d['reason']), ('Reject', 204))

    def test_auth_beats_subject(self):
        d = er.decision([7, 0, 0, 0, 0, ENQ, 4, 0, 0, 0])
        self.assertEqual((d['class'], d['reason']), ('Reject', 200))

    def test_subject_beats_action_gating(self):
        d = er.decision([7, 0, 0, 0, 0, EXE, 4, 1, 0, 1])
        self.assertEqual((d['class'], d['reason']), ('Reject', 205))

    def test_retry_still_needs_auth_and_subject(self):
        pre = [4, 2, 1, 1, 0]
        for tail, reason in (([EXE, 3, 1, 0, 0], 200),
                             ([EXE, 4, 1, 0, 1], 205),
                             ([EXE, 3, 0, 0, 1], 205),
                             ([ENQ, 3, 1, 0, 1], 202),
                             ([VOT, 3, 1, 2, 1], 202)):
            d = er.decision(pre + list(tail))
            self.assertEqual((d['class'], d['reason']), ('Reject', reason))


class RuleTests(unittest.TestCase):
    def test_quorum_boundary(self):
        for votes, ok in (((0, 0, 0), 203), ((1, 0, 0), 203), ((0, 1, 0), 203),
                          ((0, 0, 1), 203), ((1, 1, 0), None),
                          ((1, 0, 1), None), ((0, 1, 1), None)):
            d = er.decision([7, 1] + list(votes) + [EXE, 3, 1, 0, 1])
            if ok is None:
                self.assertEqual(d['deliveries'], [(300, [3, 1])])
                self.assertEqual(d['post'][:2], [4, 2])
            else:
                self.assertEqual(d['reason'], ok)

    def test_duplicate_vote_only_selected(self):
        d = er.decision([7, 1, 1, 0, 0, VOT, 3, 1, 0, 1])
        self.assertEqual(d['reason'], 201)
        d = er.decision([7, 1, 1, 0, 0, VOT, 3, 1, 1, 1])
        self.assertEqual(d['post'], [7, 1, 1, 1, 0])
        d = er.decision([7, 1, 1, 0, 0, VOT, 3, 1, 2, 1])
        self.assertEqual(d['post'], [7, 1, 1, 0, 1])

    def test_budget_spent_once_with_payload(self):
        d = er.decision([7, 1, 1, 1, 0, EXE, 3, 1, 0, 1])
        self.assertEqual(d['post'][0], 7 - 3)
        self.assertEqual(d['deliveries'], [(300, [3, 1])])

    def test_executed_retry_is_harmless(self):
        d = er.decision([4, 2, 1, 1, 0, EXE, 3, 1, 0, 1])
        self.assertEqual((d['class'], d['post'], d['deliveries']),
                         ('Accept', [4, 2, 1, 1, 0], []))


class ImmutabilityTests(unittest.TestCase):
    def test_no_input_mutation_and_fresh_outputs(self):
        row = [7, 1, 1, 1, 0, EXE, 3, 1, 0, 1]
        snapshot = list(row)
        d1 = er.decision(row)
        self.assertEqual(row, snapshot)
        d1['post'][0] = 99
        d1['deliveries'][0][1][0] = 99
        d2 = er.decision(row)
        self.assertEqual(d2['post'], [4, 2, 1, 1, 0])
        self.assertEqual(d2['deliveries'], [(300, [3, 1])])
        self.assertIsNot(d1['post'], d2['post'])


class GeneratorTests(unittest.TestCase):
    def test_journey_replays_from_genesis(self):
        steps = list(er.journey())
        self.assertEqual(len(steps), 11)
        self.assertEqual(len({tuple(s['input']) for s in steps}), 11)
        reasons = [202, 200, None, None, 201, 203, None, 205, None, None, 205]
        self.assertEqual([s['expected']['reason'] for s in steps], reasons)
        prestate = list(er.GENESIS)
        for step in steps:
            self.assertEqual(step['input'][:5], prestate)
            self.assertEqual(er.decision(step['input']), step['expected'])
            prestate = step['expected']['post']
        delivered = [i for i, s in enumerate(steps) if s['expected']['deliveries']]
        self.assertEqual(delivered, [8])
        self.assertEqual(steps[8]['expected']['deliveries'], [(300, [3, 1])])
        retry = steps[9]
        self.assertEqual(retry['expected']['post'], retry['input'][:5])

    def test_cases_replay_and_cover_contract(self):
        seen_reasons, seen_accepts, inputs = set(), set(), set()
        for case in er.cases():
            inputs.add(tuple(case['input']))
            self.assertEqual(er.decision(case['input']), case['expected'])
            exp = case['expected']
            if exp['class'] == 'Accept':
                seen_accepts.add(case['input'][5])
            else:
                seen_reasons.add(exp['reason'])
        self.assertEqual(len(inputs), 19)
        self.assertEqual(seen_reasons, REASONS)
        self.assertEqual(seen_accepts, {ENQ, VOT, EXE})


if __name__ == '__main__':
    unittest.main()
