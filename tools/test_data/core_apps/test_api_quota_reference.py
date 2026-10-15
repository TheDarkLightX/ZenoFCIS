"""Unit tests for the bounded API quota reference (api-quota.md rules)."""

import collections
import itertools
import unittest

import api_quota_reference as qr

QUOTA = 3
CHANNEL = 300
REASONS = {200, 201, 202, 203, 204}

SPEC_DOMAINS = (
    tuple(range(7)), tuple(range(4)), (0, 1), (0, 1, 2), (0, 1, 2),
    (0, 1, 2), (0, 1, 2), tuple(range(7)), (0, 1),
)
COMMANDS = tuple(itertools.product(*SPEC_DOMAINS[5:]))


def spec_legal(state):
    window_start, used, seen, last_key, last_payload = state
    if seen == 0:
        return (window_start, used, last_key, last_payload) == (0, 0, 0, 0)
    return seen == 1 and 1 <= used <= QUOTA


def all_rows():
    return itertools.product(*SPEC_DOMAINS)


def reachable_states():
    seen = {tuple(qr.GENESIS)}
    frontier = collections.deque(seen)
    while frontier:
        state = frontier.popleft()
        for command in COMMANDS:
            post = tuple(qr.decision(list(state) + list(command))['post'])
            if post not in seen:
                seen.add(post)
                frontier.append(post)
    return seen


class DomainTests(unittest.TestCase):
    def test_domains_match_spec_and_genesis(self):
        self.assertEqual(tuple(tuple(d) for d in qr.DOMAINS), SPEC_DOMAINS)
        self.assertEqual(qr.GENESIS, (0, 0, 0, 0, 0))
        self.assertTrue(qr.valid(qr.GENESIS))

    def test_product_size_and_uniqueness(self):
        rows = list(itertools.product(*qr.DOMAINS))
        self.assertEqual(len(rows), 63504)
        self.assertEqual(len(set(rows)), 63504)


class BoundaryTests(unittest.TestCase):
    def test_out_of_domain_inputs_raise(self):
        good = [0, 0, 0, 0, 0, 1, 1, 0, 1]
        for i, bad in enumerate([7, 4, 2, 3, 3, 3, -1, 7, 2]):
            row = list(good)
            row[i] = bad
            with self.assertRaises(ValueError):
                qr.decision(row)
        for malformed in (good[:8], good + [0], 0, "000001101", None):
            with self.assertRaises(ValueError):
                qr.decision(malformed)
        for bad_type in (0.0, False, "0"):
            row = list(good)
            row[0] = bad_type
            with self.assertRaises(ValueError):
                qr.decision(row)

    def test_valid_matches_spec_predicate(self):
        for state in itertools.product(*SPEC_DOMAINS[:5]):
            self.assertEqual(qr.valid(state), spec_legal(state), state)
        for bad in ((0, 0, 0, 0), (0, 0, 0, 0, 0, 0), (7, 1, 1, 0, 0),
                    (0, 1, True, 0, 0)):
            with self.assertRaises(ValueError):
                qr.valid(bad)


class FullProductTests(unittest.TestCase):
    def test_reject_contract_everywhere(self):
        for row in all_rows():
            d = qr.decision(row)
            self.assertEqual(set(d), {'class', 'reason', 'post', 'deliveries'})
            if d['class'] != 'Reject':
                continue
            self.assertIn(d['reason'], REASONS)
            self.assertEqual(d['post'], list(row[:5]))
            self.assertEqual(d['deliveries'], [])

    def test_commit_preserves_invariant(self):
        for row in all_rows():
            d = qr.decision(row)
            if d['class'] == 'Accept':
                self.assertIsNone(d['reason'])
                self.assertTrue(spec_legal(row[:5]), row)
                self.assertTrue(spec_legal(d['post']), (row, d))

    def test_exactly_once_charge(self):
        for row in all_rows():
            d = qr.decision(row)
            if d['class'] != 'Accept':
                continue
            window_start, used = row[0], row[1]
            key, payload, epoch = row[5], row[6], row[7]
            if d['deliveries'] == []:
                # Retry: quota view, memo view and outbox all unchanged.
                self.assertEqual(d['post'], list(row[:5]), row)
                self.assertEqual((row[2], row[3], row[4]), (1, key, payload))
                continue
            charged_from = used if epoch == window_start else 0
            self.assertGreaterEqual(epoch, window_start, row)
            self.assertEqual(d['post'][:2], [epoch, charged_from + 1], row)
            self.assertEqual(d['post'][2:], [1, key, payload], row)
            self.assertEqual(d['deliveries'],
                             [(CHANNEL, [epoch, d['post'][3], d['post'][4]])])

    def test_outbox_bound(self):
        for row in all_rows():
            self.assertLessEqual(len(qr.decision(row)['deliveries']), 1)

    def test_every_reason_produced_and_counts(self):
        counts = collections.Counter()
        delivering = 0
        for row in all_rows():
            d = qr.decision(row)
            counts[d['reason']] += 1
            delivering += bool(d['deliveries'])
        self.assertEqual(set(counts) - {None}, REASONS)
        print("\napi_quota: accepted=%d delivering=%d reasons=%s"
              % (counts[None], delivering,
                 sorted((r, n) for r, n in counts.items() if r is not None)))
        # Hand count: 189 seen states x 7 epochs exact retries = 1,323;
        # admits = 63 from genesis + 189 x 42 - 3,402 stale - 378 exhausted.
        self.assertEqual(counts[None], 5544)
        self.assertEqual(delivering, 4221)
        self.assertEqual(counts, {None: 5544, 200: 11970, 201: 2646,
                                  202: 3402, 203: 378, 204: 39564})

    def test_reachable_states_are_valid(self):
        reached = reachable_states()
        print("\napi_quota: reachable valid states=%d" % len(reached))
        for state in reached:
            self.assertTrue(qr.valid(state), state)
        self.assertEqual(len(reached), 190)


class PrecedenceTests(unittest.TestCase):
    def reason(self, row):
        d = qr.decision(row)
        return d['reason'] if d['class'] == 'Reject' else d['class']

    def test_invalid_prestate_beats_auth(self):
        self.assertEqual(self.reason([0, 1, 0, 0, 0, 1, 1, 0, 0]), 204)

    def test_auth_beats_retry_and_conflict(self):
        self.assertEqual(self.reason([0, 1, 1, 1, 1, 1, 1, 0, 0]), 200)
        self.assertEqual(self.reason([0, 1, 1, 1, 1, 1, 2, 0, 0]), 200)

    def test_conflict_and_retry_beat_stale_and_exhausted(self):
        self.assertEqual(self.reason([3, 3, 1, 1, 1, 1, 2, 0, 1]), 201)
        self.assertEqual(self.reason([3, 3, 1, 1, 1, 1, 2, 3, 1]), 201)
        self.assertEqual(self.reason([3, 3, 1, 1, 1, 1, 1, 0, 1]), 'Accept')
        self.assertEqual(self.reason([3, 3, 1, 1, 1, 1, 1, 3, 1]), 'Accept')

    def test_stale_beats_exhausted(self):
        self.assertEqual(self.reason([3, 3, 1, 1, 1, 2, 2, 2, 1]), 202)

    def test_later_epoch_resets_exhausted_window(self):
        d = qr.decision([3, 3, 1, 1, 1, 2, 2, 4, 1])
        self.assertEqual(d['post'], [4, 1, 1, 2, 2])
        self.assertEqual(d['deliveries'], [(CHANNEL, [4, 2, 2])])


class LimitTests(unittest.TestCase):
    def test_only_most_recent_request_is_remembered(self):
        state = list(qr.GENESIS)
        intents = []
        for key in (1, 2, 1):
            d = qr.decision(state + [key, 0, 0, 1])
            self.assertEqual(d['class'], 'Accept')
            intents.extend(d['deliveries'])
            state = d['post']
        self.assertEqual(state, [0, 3, 1, 1, 0])
        self.assertEqual(intents, [(CHANNEL, [0, 1, 0]), (CHANNEL, [0, 2, 0]),
                                   (CHANNEL, [0, 1, 0])])


class ImmutabilityTests(unittest.TestCase):
    def test_no_input_mutation_and_fresh_outputs(self):
        row = [0, 0, 0, 0, 0, 1, 1, 0, 1]
        snapshot = list(row)
        d1 = qr.decision(row)
        self.assertEqual(row, snapshot)
        d1['post'][0] = 99
        d1['deliveries'][0][1][0] = 99
        d2 = qr.decision(row)
        self.assertEqual(d2['post'], [0, 1, 1, 1, 1])
        self.assertEqual(d2['deliveries'], [(CHANNEL, [0, 1, 1])])
        self.assertIsNot(d1['post'], d2['post'])


class GeneratorTests(unittest.TestCase):
    def test_journey_matches_brief_table(self):
        steps = list(qr.journey())
        self.assertEqual(len(steps), 10)
        # (key, payload, epoch, authorized) -> (reason, post, intent) per brief.
        table = [
            ((1, 1, 0, 0), 200, None, None),
            ((1, 1, 0, 1), None, [0, 1, 1, 1, 1], [0, 1, 1]),
            ((1, 1, 0, 1), None, [0, 1, 1, 1, 1], None),
            ((1, 2, 0, 1), 201, None, None),
            ((2, 0, 0, 1), None, [0, 2, 1, 2, 0], [0, 2, 0]),
            ((0, 0, 0, 1), None, [0, 3, 1, 0, 0], [0, 0, 0]),
            ((1, 0, 0, 1), 203, None, None),
            ((1, 0, 2, 1), None, [2, 1, 1, 1, 0], [2, 1, 0]),
            ((2, 2, 1, 1), 202, None, None),
            ((1, 0, 2, 1), None, [2, 1, 1, 1, 0], None),
        ]
        prestate = list(qr.GENESIS)
        for step, (command, reason, post, intent) in zip(steps, table):
            self.assertEqual(step['input'], prestate + list(command))
            self.assertEqual(qr.decision(step['input']), step['expected'])
            exp = step['expected']
            self.assertEqual(exp['reason'], reason)
            self.assertEqual(exp['post'], post if post is not None else prestate)
            want = [] if intent is None else [(CHANNEL, intent)]
            self.assertEqual(exp['deliveries'], want)
            prestate = exp['post']

    def test_cases_replay_and_cover_contract(self):
        seen_reasons, seen_accepts, inputs = set(), set(), set()
        for case in qr.cases():
            inputs.add(tuple(case['input']))
            self.assertEqual(qr.decision(case['input']), case['expected'])
            exp = case['expected']
            row = case['input']
            if exp['class'] == 'Reject':
                seen_reasons.add(exp['reason'])
            elif not exp['deliveries']:
                seen_accepts.add('retry' if row[7] >= row[0] else 'stale retry')
            else:
                seen_accepts.add('new epoch' if row[7] > row[0] else 'same epoch')
        self.assertEqual(len(inputs), 16)
        self.assertEqual(seen_reasons, REASONS)
        self.assertEqual(seen_accepts,
                         {'retry', 'stale retry', 'new epoch', 'same epoch'})


if __name__ == '__main__':
    unittest.main()
