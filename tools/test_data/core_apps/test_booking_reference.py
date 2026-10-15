import unittest

from booking_reference import (
    CONFIRMED,
    DOMAINS,
    EXPIRED,
    FREE,
    GENESIS,
    RESERVED,
    cases,
    decision,
    journey,
    valid,
)

RESERVE, CONFIRM, TIMEOUT = 150, 151, 152
LEGAL_STATES = (FREE, RESERVED, CONFIRMED, EXPIRED)
RESULT_KEYS = {'class', 'reason', 'post', 'deliveries'}
INTENT = [(300, [2, 1])]


def row(state, action, expected, now, auth):
    return tuple(state) + (action, expected, now, auth)


class ValidationTests(unittest.TestCase):
    def test_valid_accepts_the_four_legal_states(self):
        for state in LEGAL_STATES:
            self.assertTrue(valid(state))
            self.assertTrue(valid(list(state)))

    def test_valid_rejects_in_domain_illegal_states(self):
        bad = [(2, 0, 0, 1, 0, 0), (1, 0, 1, 1, 0, 0), (0, 2, 2, 3, 4, 1),
               (1, 1, 1, 1, 0, 1), (2, 0, 1, 0, 0, 0), (1, 1, 2, 3, 4, 1),
               (2, 0, 2, 3, 0, 1), (1, 1, 0, 1, 0, 0)]
        for state in bad:
            self.assertFalse(valid(state), state)

    def test_valid_raises_on_malformed_state(self):
        bad = [(), (2, 0, 0, 0, 0), (2, 0, 0, 0, 0, 0, 0), "state", 7, None,
               (2, 0, 0, 0, 0, True), (2, 0, 0, 0, 0, 0.0),
               (2, 0, 0, 0, 0, "0"), (3, 0, 0, 0, 0, 0), (2, 0, 0, 0, 0, -1)]
        for state in bad:
            with self.assertRaises(ValueError):
                valid(state)

    def test_decision_raises_on_malformed_row(self):
        base = row(FREE, RESERVE, 0, 0, 1)
        bad = [base[:9], base + (0,), "row", None, 9,
               (True,) + base[1:], (0.0,) + base[1:], ("2",) + base[1:],
               (3,) + base[1:], base[:5] + (2,) + base[6:],
               base[:6] + (149,) + base[7:], base[:8] + (5,) + base[9:],
               base[:9] + (2,)]
        for values in bad:
            with self.assertRaises(ValueError):
                decision(values)

    def test_domains_cover_exact_product(self):
        self.assertEqual(len(DOMAINS), 10)
        total = 1
        for domain in DOMAINS:
            total *= len(domain)
        self.assertEqual(total, 97200)


class DecisionTests(unittest.TestCase):
    def test_each_command_family_has_a_successful_witness(self):
        r = decision(row(FREE, RESERVE, 0, 0, 1))
        self.assertEqual(r['class'], 'Accept')
        self.assertEqual(r['post'], list(RESERVED))
        self.assertEqual(r['deliveries'], [])
        r = decision(row(RESERVED, CONFIRM, 1, 3, 1))
        self.assertEqual(r['class'], 'Accept')
        self.assertEqual(r['post'], list(CONFIRMED))
        self.assertEqual(r['deliveries'], INTENT)
        r = decision(row(RESERVED, TIMEOUT, 1, 4, 1))
        self.assertEqual(r['class'], 'Accept')
        self.assertEqual(r['post'], list(EXPIRED))
        self.assertEqual(r['deliveries'], [])

    def test_reject_precedence_order(self):
        # Illegal prestate dominates authorization failure.
        r = decision(row((0, 0, 0, 0, 0, 0), RESERVE, 0, 0, 0))
        self.assertEqual((r['class'], r['reason']), ('Reject', 204))
        # Authorization dominates clock regression and the retry accept.
        self.assertEqual(decision(row(EXPIRED, RESERVE, 2, 3, 0))['reason'], 200)
        self.assertEqual(decision(row(CONFIRMED, CONFIRM, 1, 0, 0))['reason'], 200)
        # Clock dominates retry-accept and version checks.
        self.assertEqual(decision(row(EXPIRED, TIMEOUT, 1, 3, 1))['reason'], 206)
        self.assertEqual(decision(row(EXPIRED, TIMEOUT, 0, 3, 1))['reason'], 206)
        # Retry-accept dominates the expected!=version check.
        self.assertEqual(decision(row(CONFIRMED, CONFIRM, 1, 0, 1))['class'],
                         'Accept')
        # Version check dominates command preconditions.
        self.assertEqual(decision(row(FREE, CONFIRM, 1, 0, 1))['reason'], 205)
        self.assertEqual(decision(row(FREE, CONFIRM, 0, 0, 1))['reason'], 202)

    def test_deadline_tick_boundaries(self):
        self.assertEqual(decision(row(FREE, RESERVE, 0, 3, 1))['class'], 'Accept')
        self.assertEqual(decision(row(FREE, RESERVE, 0, 4, 1))['reason'], 203)
        self.assertEqual(decision(row(RESERVED, CONFIRM, 1, 3, 1))['class'],
                         'Accept')
        self.assertEqual(decision(row(RESERVED, CONFIRM, 1, 4, 1))['reason'], 203)
        self.assertEqual(decision(row(RESERVED, TIMEOUT, 1, 3, 1))['reason'], 203)
        self.assertEqual(decision(row(RESERVED, TIMEOUT, 1, 4, 1))['class'],
                         'Accept')

    def test_unauthorized_rejects_every_command(self):
        for state in LEGAL_STATES:
            for action in (RESERVE, CONFIRM, TIMEOUT):
                r = decision(row(state, action, state[2], 0, 0))
                self.assertEqual((r['class'], r['reason']), ('Reject', 200))
                self.assertEqual(r['post'], list(state))

    def test_cas_expected_version(self):
        self.assertEqual(decision(row(RESERVED, CONFIRM, 0, 0, 1))['reason'], 205)
        self.assertEqual(decision(row(RESERVED, CONFIRM, 2, 0, 1))['reason'], 205)
        self.assertEqual(decision(row(FREE, RESERVE, 1, 0, 1))['reason'], 205)
        # Stale expected version on a decided booking: CAS loser.
        self.assertEqual(decision(row(CONFIRMED, TIMEOUT, 1, 4, 1))['reason'],
                         205)
        self.assertEqual(decision(row(EXPIRED, CONFIRM, 1, 4, 1))['reason'], 205)
        # Retry guard dominates the plain version check on terminal states.
        self.assertEqual(decision(row(CONFIRMED, CONFIRM, 2, 4, 1))['reason'],
                         205)
        self.assertEqual(decision(row(EXPIRED, TIMEOUT, 2, 4, 1))['reason'], 205)
        # Current version passes CAS, then fails the command precondition.
        self.assertEqual(decision(row(CONFIRMED, TIMEOUT, 2, 4, 1))['reason'],
                         202)
        self.assertEqual(decision(row(EXPIRED, CONFIRM, 2, 4, 1))['reason'], 202)
        self.assertEqual(decision(row(EXPIRED, RESERVE, 2, 4, 1))['reason'], 202)
        self.assertEqual(decision(row(RESERVED, RESERVE, 1, 0, 1))['reason'], 202)

    def test_decided_booking_excludes_the_losing_command(self):
        for expected in (0, 1, 2):
            for now in range(5):
                for state, action in ((CONFIRMED, TIMEOUT),
                                      (EXPIRED, CONFIRM)):
                    r = decision(row(state, action, expected, now, 1))
                    self.assertEqual(r['class'], 'Reject')
                    self.assertEqual(r['post'], list(state))

    def test_exact_retries_do_not_advance_or_redeliver(self):
        r = decision(row(CONFIRMED, CONFIRM, 1, 3, 1))
        self.assertEqual(r['class'], 'Accept')
        self.assertEqual(r['post'], list(CONFIRMED))
        self.assertEqual(r['deliveries'], [])
        r = decision(row(EXPIRED, TIMEOUT, 1, 4, 1))
        self.assertEqual(r['class'], 'Accept')
        self.assertEqual(r['post'], list(EXPIRED))
        self.assertEqual(r['deliveries'], [])


class ResultContractTests(unittest.TestCase):
    def test_result_shape_and_fresh_nested_returns(self):
        r = decision(row(RESERVED, CONFIRM, 1, 0, 1))
        self.assertEqual(set(r), RESULT_KEYS)
        self.assertIsInstance(r['post'], list)
        r['post'][0] = 99
        r['deliveries'][0][1][0] = 99
        again = decision(row(RESERVED, CONFIRM, 1, 0, 1))
        self.assertEqual(again['post'], list(CONFIRMED))
        self.assertEqual(again['deliveries'], INTENT)

    def test_inputs_are_never_mutated(self):
        values = list(row(FREE, RESERVE, 0, 0, 1))
        snapshot = list(values)
        decision(values)
        self.assertEqual(values, snapshot)

    def test_journey_returns_fresh_structures(self):
        first, second = journey(), journey()
        self.assertEqual(first, second)
        self.assertIsNot(first, second)
        self.assertIsNot(first[0]['expected']['post'],
                         second[0]['expected']['post'])
        first[0]['expected']['post'][0] = 99
        self.assertNotEqual(first, second)


class CaseSpaceTests(unittest.TestCase):
    def test_cases_are_exact_ordered_and_unique(self):
        rows = list(cases())
        self.assertEqual(len(rows), 97200)
        inputs = [r['input'] for r in rows]
        self.assertTrue(all(isinstance(i, tuple) for i in inputs))
        self.assertEqual(inputs, sorted(inputs))
        self.assertEqual(len(set(inputs)), 97200)

    def test_every_row_satisfies_the_result_contract(self):
        for r in cases():
            result, values = r['expected'], r['input']
            self.assertEqual(set(result), RESULT_KEYS)
            self.assertEqual(len(result['post']), 6)
            if result['class'] == 'Accept':
                self.assertIsNone(result['reason'])
                self.assertTrue(valid(result['post']))
                self.assertEqual(result['post'][0] + result['post'][1], 2)
            else:
                self.assertIn(result['reason'], (200, 202, 203, 204, 205, 206))
                self.assertEqual(result['post'], list(values[:6]))
                self.assertEqual(result['deliveries'], [])

    def test_exactly_one_confirmation_intent(self):
        intents = [r for r in cases() if r['expected']['deliveries']]
        self.assertEqual(len(intents), 4)  # now in 0..3 on Reserved
        for r in intents:
            self.assertEqual(r['expected']['deliveries'], INTENT)
            self.assertEqual(tuple(r['input'][:6]), RESERVED)
            self.assertEqual(r['input'][6], CONFIRM)


class JourneyTests(unittest.TestCase):
    def test_journey_rows_match_decision(self):
        rows = journey()
        self.assertEqual(len(rows), 15)
        for r in rows:
            self.assertEqual(r['expected'], decision(r['input']))

    def test_two_sequences_start_at_separate_genesis(self):
        rows = journey()
        self.assertEqual(rows[0]['input'][:6], GENESIS)
        self.assertEqual(rows[8]['input'][:6], GENESIS)
        self.assertEqual(rows[7]['expected']['post'], list(CONFIRMED))
        self.assertEqual(rows[14]['expected']['post'], list(EXPIRED))


if __name__ == '__main__':
    unittest.main()
