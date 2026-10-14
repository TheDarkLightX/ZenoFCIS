"""Unit tests for the bounded document release reference (document-release.md)."""

import collections
import itertools
import unittest

import document_release_reference as dr

DRAFT, REVIEW, RELEASED = 0, 1, 2
EDIT, SUBMIT, APPROVE, RELEASE = 160, 161, 162, 163
CHANNEL = 300
REASONS = {200, 201, 202, 203, 204, 205, 206}

SPEC_DOMAINS = (
    (0, 1, 2), (0, 1, 2), (0, 1, 2), (0, 1), (0, 1), (0, 1),
    (EDIT, SUBMIT, APPROVE, RELEASE), (0, 1, 2), (0, 1, 2), (0, 1, 2), (0, 1),
)
COMMANDS = tuple(itertools.product(*SPEC_DOMAINS[6:]))


def spec_legal(state):
    content, version, phase, v0, v1, v2 = state
    votes = v0 + v1 + v2
    if version == 0 and content != 0:
        return False
    if phase == DRAFT:
        return votes == 0
    if phase == RELEASED:
        return votes >= 2
    return phase == REVIEW


def all_rows():
    return itertools.product(*SPEC_DOMAINS)


def reachable_states():
    seen = {tuple(dr.GENESIS)}
    frontier = collections.deque(seen)
    while frontier:
        state = frontier.popleft()
        for command in COMMANDS:
            post = tuple(dr.decision(list(state) + list(command))['post'])
            if post not in seen:
                seen.add(post)
                frontier.append(post)
    return seen


class DomainTests(unittest.TestCase):
    def test_domains_match_spec_and_genesis(self):
        self.assertEqual(tuple(tuple(d) for d in dr.DOMAINS), SPEC_DOMAINS)
        self.assertEqual(dr.GENESIS, (0, 0, DRAFT, 0, 0, 0))
        self.assertTrue(dr.valid(dr.GENESIS))

    def test_product_size_and_uniqueness(self):
        rows = list(itertools.product(*dr.DOMAINS))
        self.assertEqual(len(rows), 46656)
        self.assertEqual(len(set(rows)), 46656)


class BoundaryTests(unittest.TestCase):
    def test_out_of_domain_inputs_raise(self):
        good = [0, 0, 0, 0, 0, 0, EDIT, 0, 1, 0, 1]
        for i, bad in enumerate([3, 3, 3, 2, 2, 2, 164, 3, 3, 3, 2]):
            row = list(good)
            row[i] = bad
            with self.assertRaises(ValueError):
                dr.decision(row)
        for malformed in (good[:10], good + [0], 0, "000000160", None):
            with self.assertRaises(ValueError):
                dr.decision(malformed)
        for bad_type in (0.0, False, "0"):
            row = list(good)
            row[0] = bad_type
            with self.assertRaises(ValueError):
                dr.decision(row)

    def test_valid_matches_spec_predicate(self):
        for state in itertools.product(*SPEC_DOMAINS[:6]):
            self.assertEqual(dr.valid(state), spec_legal(state), state)
        for bad in ((0, 0, 0, 0, 0), (0, 0, 0, 0, 0, 0, 0), (0, 0, 3, 0, 0, 0),
                    (0, 0, 0, True, 0, 0)):
            with self.assertRaises(ValueError):
                dr.valid(bad)


class FullProductTests(unittest.TestCase):
    def test_reject_contract_everywhere(self):
        for row in all_rows():
            d = dr.decision(row)
            self.assertEqual(set(d), {'class', 'reason', 'post', 'deliveries'})
            if d['class'] != 'Reject':
                continue
            self.assertIn(d['reason'], REASONS)
            self.assertEqual(d['post'], list(row[:6]))
            self.assertEqual(d['deliveries'], [])

    def test_commit_preserves_invariant(self):
        for row in all_rows():
            d = dr.decision(row)
            if d['class'] == 'Accept':
                self.assertIsNone(d['reason'])
                self.assertTrue(spec_legal(row[:6]), row)
                self.assertTrue(spec_legal(d['post']), (row, d))

    def test_locked_review(self):
        for row in all_rows():
            d = dr.decision(row)
            if d['class'] == 'Accept' and row[2] in (REVIEW, RELEASED):
                self.assertEqual(d['post'][:2], list(row[:2]), row)

    def test_release_equals_reviewed_revision(self):
        for row in all_rows():
            d = dr.decision(row)
            released_now = (d['class'] == 'Accept' and row[2] == REVIEW
                            and d['post'][2] == RELEASED)
            if released_now:
                self.assertEqual(d['post'][:2], list(row[:2]), row)
                self.assertEqual(d['deliveries'], [(CHANNEL, list(row[:2]))])
            else:
                self.assertEqual(d['deliveries'], [], row)

    def test_vote_conservation(self):
        for row in all_rows():
            d = dr.decision(row)
            if d['class'] != 'Accept' or row[6] != APPROVE:
                continue
            principal = row[9]
            self.assertEqual(row[3 + principal], 0, row)
            self.assertEqual(d['post'][3 + principal], 1, row)
            for other in {0, 1, 2} - {principal}:
                self.assertEqual(d['post'][3 + other], row[3 + other], row)
            self.assertEqual(d['post'][:3], list(row[:3]), row)

    def test_every_reason_produced_and_counts(self):
        counts = collections.Counter()
        delivering = 0
        for row in all_rows():
            d = dr.decision(row)
            counts[d['reason']] += 1
            delivering += bool(d['deliveries'])
        self.assertEqual(set(counts) - {None}, REASONS)
        print("\ndocument_release: accepted=%d delivering=%d reasons=%s"
              % (counts[None], delivering,
                 sorted((r, n) for r, n in counts.items() if r is not None)))
        # Hand count over the 91 valid states (7 content/version pairs):
        # edit 4x9 + submit 7x9 + approve 7x12x9 + release 7x4x9
        # + released retry 7x4x9 = 1,359; only the 252 releases deliver.
        self.assertEqual(counts[None], 1359)
        self.assertEqual(delivering, 252)
        self.assertEqual(counts, {None: 1359, 200: 9828, 201: 756, 202: 6174,
                                  203: 252, 204: 27000, 205: 1260, 206: 27})

    def test_reachable_states_are_valid(self):
        reached = reachable_states()
        print("\ndocument_release: reachable valid states=%d" % len(reached))
        for state in reached:
            self.assertTrue(dr.valid(state), state)
        self.assertEqual(len(reached), 91)


class PrecedenceTests(unittest.TestCase):
    def reason(self, row):
        d = dr.decision(row)
        return d['reason'] if d['class'] == 'Reject' else d['class']

    def test_invalid_prestate_beats_auth(self):
        self.assertEqual(self.reason([1, 0, 0, 0, 0, 0, EDIT, 0, 1, 0, 0]), 204)

    def test_auth_beats_release_retry(self):
        self.assertEqual(self.reason([2, 2, 2, 1, 1, 0, RELEASE, 2, 0, 0, 0]), 200)

    def test_stale_release_after_release_is_wrong_phase(self):
        self.assertEqual(self.reason([2, 2, 2, 1, 1, 0, RELEASE, 0, 0, 0, 1]), 202)

    def test_wrong_phase_beats_stale_and_limit(self):
        self.assertEqual(self.reason([2, 2, 1, 0, 0, 0, EDIT, 0, 0, 0, 1]), 202)
        self.assertEqual(self.reason([1, 1, 0, 0, 0, 0, RELEASE, 0, 0, 0, 1]), 202)

    def test_stale_beats_limit_and_quorum(self):
        self.assertEqual(self.reason([2, 2, 0, 0, 0, 0, EDIT, 1, 0, 0, 1]), 205)
        self.assertEqual(self.reason([2, 2, 1, 0, 0, 0, RELEASE, 1, 0, 0, 1]), 205)

    def test_approve_ignores_expected_version(self):
        for expected in (0, 1, 2):
            d = dr.decision([2, 2, 1, 0, 0, 0, APPROVE, expected, 0, 2, 1])
            self.assertEqual(d['post'], [2, 2, 1, 0, 0, 1])

    def test_new_content_ignored_outside_edit(self):
        for action in (SUBMIT, APPROVE, RELEASE):
            pre = [1, 1, DRAFT if action == SUBMIT else REVIEW, 0, 0, 0]
            if action == RELEASE:
                pre[3:6] = [1, 1, 0]
            outs = {repr(dr.decision(pre + [action, 1, n, 2, 1])) for n in (0, 1, 2)}
            self.assertEqual(len(outs), 1, action)


class ImmutabilityTests(unittest.TestCase):
    def test_no_input_mutation_and_fresh_outputs(self):
        row = [2, 2, 1, 1, 1, 0, RELEASE, 2, 0, 0, 1]
        snapshot = list(row)
        d1 = dr.decision(row)
        self.assertEqual(row, snapshot)
        d1['post'][0] = 99
        d1['deliveries'][0][1][0] = 99
        d2 = dr.decision(row)
        self.assertEqual(d2['post'], [2, 2, 2, 1, 1, 0])
        self.assertEqual(d2['deliveries'], [(CHANNEL, [2, 2])])
        self.assertIsNot(d1['post'], d2['post'])


class GeneratorTests(unittest.TestCase):
    def test_journey_matches_brief_table(self):
        steps = list(dr.journey())
        self.assertEqual(len(steps), 14)
        # (action, expected, new, principal, authorized) -> (reason, post, delivery).
        table = [
            ((EDIT, 0, 1, 0, 0), 200, None, None),
            ((EDIT, 0, 1, 0, 1), None, [1, 1, DRAFT, 0, 0, 0], None),
            ((EDIT, 0, 2, 0, 1), 205, None, None),
            ((EDIT, 1, 2, 0, 1), None, [2, 2, DRAFT, 0, 0, 0], None),
            ((SUBMIT, 2, 0, 0, 1), None, [2, 2, REVIEW, 0, 0, 0], None),
            ((EDIT, 2, 1, 0, 1), 202, None, None),
            ((APPROVE, 0, 0, 0, 1), None, [2, 2, REVIEW, 1, 0, 0], None),
            ((APPROVE, 0, 0, 0, 1), 201, None, None),
            ((RELEASE, 2, 0, 0, 1), 203, None, None),
            ((APPROVE, 0, 0, 1, 1), None, [2, 2, REVIEW, 1, 1, 0], None),
            ((RELEASE, 1, 0, 0, 1), 205, None, None),
            ((RELEASE, 2, 0, 0, 1), None, [2, 2, RELEASED, 1, 1, 0], [2, 2]),
            ((RELEASE, 2, 0, 0, 1), None, [2, 2, RELEASED, 1, 1, 0], None),
            ((EDIT, 2, 0, 0, 1), 202, None, None),
        ]
        prestate = list(dr.GENESIS)
        for step, (command, reason, post, payload) in zip(steps, table):
            self.assertEqual(step['input'], prestate + list(command))
            self.assertEqual(dr.decision(step['input']), step['expected'])
            exp = step['expected']
            self.assertEqual(exp['reason'], reason)
            self.assertEqual(exp['post'], post if post is not None else prestate)
            want = [] if payload is None else [(CHANNEL, payload)]
            self.assertEqual(exp['deliveries'], want)
            prestate = exp['post']

    def test_cases_replay_and_cover_contract(self):
        seen_reasons, seen_accepts, inputs = set(), set(), set()
        for case in dr.cases():
            inputs.add(tuple(case['input']))
            self.assertEqual(dr.decision(case['input']), case['expected'])
            exp = case['expected']
            if exp['class'] == 'Reject':
                seen_reasons.add(exp['reason'])
            elif case['input'][2] == RELEASED:
                seen_accepts.add('retry')
            else:
                seen_accepts.add(case['input'][6])
        self.assertEqual(len(inputs), 25)
        self.assertEqual(seen_reasons, REASONS)
        self.assertEqual(seen_accepts, {EDIT, SUBMIT, APPROVE, RELEASE, 'retry'})


if __name__ == '__main__':
    unittest.main()
