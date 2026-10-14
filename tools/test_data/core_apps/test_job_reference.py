import unittest
import job_reference as jr
G = jr.GENESIS
START, FAIL, SUCCEED = 150, 151, 152

def acc(post, deliv=()):
    return {"class": "Accept", "reason": None, "post": list(post), "deliveries": list(deliv)}

def rej(pre, why):
    return {"class": "Reject", "reason": why, "post": list(pre), "deliveries": []}
_ALL = None

def all_cases():
    global _ALL
    if _ALL is None:
        _ALL = list(jr.cases())
    return _ALL

class BoundaryTests(unittest.TestCase):
    def test_malformed(self):
        for bad in (None, 5, "x", {}, iter(range(9)), (1,) * 8, (1,) * 10):
            with self.assertRaises(ValueError, msg=repr(bad)):
                jr.decision(bad)
    def test_strict_ints(self):
        base = [3, 0, 0, 0, 0, START, 1, 0, 1]
        for i in range(9):
            for v in (True, 0.0, "0"):
                bad = list(base)
                bad[i] = v
                with self.assertRaises(ValueError):
                    jr.decision(bad)
    def test_field_bounds(self):
        bounds = [(0, 3), (0, 3), (0, 4), (0, 1), (0, 4), (150, 152), (0, 3), (0, 4), (0, 1)]
        base = [3, 0, 0, 0, 0, START, 1, 0, 1]
        for i, (lo, hi) in enumerate(bounds):
            for v in (lo - 1, hi + 1):
                bad = list(base)
                bad[i] = v
                with self.assertRaises(ValueError, msg=(i, v)):
                    jr.decision(bad)
            for v in (lo, hi):
                good = list(base)
                good[i] = v
                self.assertIsInstance(jr.decision(good), dict)
    def test_state_bounds(self):
        for bad in ([4, 0, 0, 0, 0], [3, 4, 0, 0, 0], [3, 0, 5, 0, 0],
                    [3, 0, 0, 2, 0], [3, 0, 0, 0, 5], [3, 0, 0, 0], [3, 0, 0, 0, 0, 0], [3, 0, 0, 0, True]):
            with self.assertRaises(ValueError, msg=bad):
                jr.valid(bad)
        self.assertTrue(jr.valid(list(G)))

class LegalityTests(unittest.TestCase):
    def test_legal_states(self):
        for s in (G, (2, 1, 0, 0, 0), (2, 2, 0, 0, 3), (1, 1, 4, 1, 4),
                  (0, 3, 4, 1, 4), (0, 2, 0, 0, 4), (0, 1, 0, 0, 0)):
            self.assertTrue(jr.valid(list(s)), s)
    def test_illegal_states(self):
        for s in ((3, 1, 0, 0, 0), (2, 0, 0, 0, 0), (0, 0, 0, 0, 0),
                  (2, 1, 0, 1, 0), (2, 1, 4, 0, 4), (2, 1, 4, 1, 3), (3, 3, 0, 0, 0), (2, 1, 3, 0, 3)):
            self.assertFalse(jr.valid(s), s)

class PrecedenceTests(unittest.TestCase):
    def test_illegal_prestate_first(self):
        r = jr.decision((2, 0, 0, 0, 4, FAIL, 0, 3, 0))
        self.assertEqual(r, rej([2, 0, 0, 0, 4], 204))
    def test_unauthorized_beats_stale_clock_and_retries(self):
        self.assertEqual(jr.decision((2, 1, 0, 0, 4, START, 1, 3, 0)), rej([2, 1, 0, 0, 4], 200))
        self.assertEqual(jr.decision((1, 3, 4, 1, 4, SUCCEED, 2, 4, 0)), rej([1, 3, 4, 1, 4], 200))
        self.assertEqual(jr.decision((2, 1, 0, 0, 0, START, 1, 0, 0)), rej([2, 1, 0, 0, 0], 200))
    def test_stale_clock_beats_action_rules(self):
        self.assertEqual(jr.decision((2, 1, 0, 0, 4, FAIL, 0, 3, 1)), rej([2, 1, 0, 0, 4], 206))
        self.assertEqual(jr.decision((1, 3, 4, 1, 4, SUCCEED, 2, 3, 1)), rej([1, 3, 4, 1, 4], 206))

class StartTests(unittest.TestCase):
    def test_running_exact_retry(self):
        self.assertEqual(jr.decision((2, 1, 0, 0, 0, START, 1, 0, 1)), acc([2, 1, 0, 0, 0]))
    def test_running_wrong_attempt(self):
        for a in (0, 2, 3):
            self.assertEqual(jr.decision((2, 1, 0, 0, 0, START, a, 0, 1)), rej([2, 1, 0, 0, 0], 205))
    def test_done_rejects_start(self):
        self.assertEqual(jr.decision((1, 3, 4, 1, 4, START, 2, 4, 1)), rej([1, 3, 4, 1, 4], 202))
    def test_exhausted_before_attempt_binding(self):
        for a in (0, 2, 3):
            self.assertEqual( jr.decision((0, 2, 4, 1, 4, START, a, 4, 1)), rej([0, 2, 4, 1, 4], 201))
    def test_idle_wrong_attempt(self):
        for a in (0, 2, 3):
            self.assertEqual(jr.decision(G + (START, a, 0, 1))["reason"], 205)
    def test_waiting_backoff_gate(self):
        for now in range(4):
            self.assertEqual(jr.decision((2, 2, 0, 0, 0, START, 2, now, 1)), rej([2, 2, 0, 0, 0], 203))
        self.assertEqual(jr.decision((2, 2, 0, 0, 0, START, 2, 4, 1)), acc([1, 1, 4, 1, 4], [(300, [2, 1])]))
    def test_deadline_stamped_once_reused(self):
        self.assertEqual(jr.decision((1, 2, 4, 1, 4, START, 3, 4, 1)), acc([0, 1, 4, 1, 4], [(300, [3, 1])]))
    def test_idle_start_intent_and_clock(self):
        self.assertEqual(jr.decision(G + (START, 1, 2, 1)), acc([2, 1, 0, 0, 2], [(300, [1, 1])]))

class FailTests(unittest.TestCase):
    def test_phase_gate(self):
        for pre in (G, (2, 2, 0, 0, 0), (1, 3, 4, 1, 4)):
            self.assertEqual( jr.decision(tuple(pre) + (FAIL, 1, 4, 1))["reason"], 202)
    def test_attempt_gate(self):
        for a in (0, 2, 3):
            self.assertEqual( jr.decision((2, 1, 0, 0, 0, FAIL, a, 0, 1))["reason"], 205)
    def test_preserves_budget_and_deadline(self):
        self.assertEqual(jr.decision((2, 1, 0, 0, 1, FAIL, 1, 3, 1)), acc([2, 2, 0, 0, 3]))
        self.assertEqual(jr.decision((1, 1, 4, 1, 4, FAIL, 2, 4, 1)), acc([1, 2, 4, 1, 4]))

class SucceedTests(unittest.TestCase):
    def test_phase_gate(self):
        for pre in (G, (2, 2, 0, 0, 0)):
            self.assertEqual( jr.decision(tuple(pre) + (SUCCEED, 1, 4, 1))["reason"], 202)
    def test_attempt_gate_running_and_done(self):
        self.assertEqual( jr.decision((2, 1, 0, 0, 0, SUCCEED, 2, 0, 1))["reason"], 205)
        self.assertEqual( jr.decision((1, 3, 4, 1, 4, SUCCEED, 3, 4, 1))["reason"], 205)
    def test_done_retry_unchanged(self):
        self.assertEqual(jr.decision((1, 3, 4, 1, 4, SUCCEED, 2, 4, 1)), acc([1, 3, 4, 1, 4]))
    def test_running_closes_budget(self):
        self.assertEqual(jr.decision((0, 1, 4, 1, 4, SUCCEED, 3, 4, 1)), acc([0, 3, 4, 1, 4]))

class FreshnessTests(unittest.TestCase):
    def test_input_immutable(self):
        inp = [3, 0, 0, 0, 0, START, 1, 0, 1]
        jr.decision(inp)
        self.assertEqual(inp, [3, 0, 0, 0, 0, START, 1, 0, 1])
    def test_fresh_nested_result(self):
        inp = (3, 0, 0, 0, 0, START, 1, 0, 1)
        r1, r2 = jr.decision(inp), jr.decision(inp)
        self.assertIsNot(r1["post"], r2["post"])
        self.assertIsNot(r1["deliveries"], r2["deliveries"])
        self.assertIsNot(r1["deliveries"][0][1], r2["deliveries"][0][1])
        r1["post"][0] = 9
        r1["deliveries"][0][1][0] = 9
        self.assertEqual(jr.decision(inp), acc([2, 1, 0, 0, 0], [(300, [1, 1])]))

class ExhaustiveTests(unittest.TestCase):
    def test_product_exact_unique_sorted(self):
        inputs = [c["input"] for c in all_cases()]
        self.assertEqual(len(inputs), 96000)
        self.assertEqual(len(set(inputs)), 96000)
        self.assertEqual(inputs, sorted(inputs))
        self.assertEqual(inputs[0], (0, 0, 0, 0, 0, 150, 0, 0, 0))
        self.assertEqual(inputs[-1], (3, 3, 4, 1, 4, 152, 3, 4, 1))
    def test_row_shape_and_determinism(self):
        for c in all_cases():
            e = c["expected"]
            self.assertEqual(set(e), {"class", "reason", "post", "deliveries"})
            self.assertTrue(all(type(x) is int for x in e["post"]))
            self.assertEqual(e, jr.decision(c["input"]))
    def test_every_reject_echoes_prestate(self):
        for c in all_cases():
            e = c["expected"]
            if e["class"] == "Reject":
                self.assertIs(type(e["reason"]), int)
                self.assertEqual(e["post"], list(c["input"][:5]))
                self.assertEqual(e["deliveries"], [])
    def test_every_accept_transition(self):
        for c in all_cases():
            inp, e = c["input"], c["expected"]
            if e["class"] != "Accept":
                continue
            pre, now = list(inp[:5]), inp[7]
            fresh = inp[5] == START and inp[1] in (0, 2)
            if fresh:
                dl = [4, 1] if inp[3] == 0 and inp[1] == 2 else pre[2:4]
                want = [pre[0] - 1, 1] + dl + [now]
            elif inp[5] == FAIL:
                want = [pre[0], 2, pre[2], pre[3], now]
            elif inp[5] == SUCCEED and inp[1] == 1:
                want = [pre[0], 3, pre[2], pre[3], now]
            else:
                want = pre
            deliv = [(300, [inp[6], 1])] if fresh else []
            self.assertTrue(jr.valid(e["post"]), inp)
            self.assertEqual((e["post"], e["deliveries"]), (want, deliv))
    def test_nonvacuity(self):
        cases = all_cases()
        reasons = {c["expected"]["reason"] for c in cases if c["expected"]["class"] == "Reject"}
        self.assertEqual(reasons, {200, 201, 202, 203, 204, 205, 206})
        accepts = [c for c in cases if c["expected"]["class"] == "Accept"]
        for action in (START, FAIL, SUCCEED):
            self.assertTrue(any(c["input"][5] == action for c in accepts))
        self.assertTrue(any(c["expected"]["deliveries"] for c in accepts))
        last_slot = (1, 2, 4, 1, 4, START, 3, 4, 1)
        self.assertTrue(any(c["input"] == last_slot for c in accepts))
        # Quorum-free completion: Succeed accepts with no intent emitted.
        done = (0, 1, 4, 1, 4, SUCCEED, 3, 4, 1)
        self.assertTrue(any(c["input"] == done for c in accepts))

class JourneyTests(unittest.TestCase):
    WANT = [ ("Reject", 202, [3, 0, 0, 0, 0], []), ("Reject", 200, [3, 0, 0, 0, 0], []),
        ("Accept", None, [2, 1, 0, 0, 0], [(300, [1, 1])]), ("Accept", None, [2, 1, 0, 0, 0], []),
        ("Accept", None, [2, 2, 0, 0, 0], []), ("Reject", 203, [2, 2, 0, 0, 0], []),
        ("Accept", None, [1, 1, 4, 1, 4], [(300, [2, 1])]), ("Reject", 205, [1, 1, 4, 1, 4], []),
        ("Accept", None, [1, 3, 4, 1, 4], []), ("Accept", None, [1, 3, 4, 1, 4], []),
        ("Accept", None, [2, 1, 0, 0, 0], [(300, [1, 1])]), ("Accept", None, [2, 2, 0, 0, 0], []),
        ("Accept", None, [1, 1, 4, 1, 4], [(300, [2, 1])]), ("Accept", None, [1, 2, 4, 1, 4], []),
        ("Accept", None, [0, 1, 4, 1, 4], [(300, [3, 1])]), ("Accept", None, [0, 2, 4, 1, 4], []),
        ("Reject", 201, [0, 2, 4, 1, 4], []), ]
    def test_journey_explicit_rows(self):
        rows = list(jr.journey())
        self.assertEqual(len(rows), len(self.WANT))
        for r, w in zip(rows, self.WANT):
            e = r["expected"]
            self.assertEqual( (e["class"], e["reason"], e["post"], e["deliveries"]), w)
            self.assertEqual(e, jr.decision(r["input"]))
        inputs = [r["input"] for r in rows]
        self.assertEqual(inputs[2], inputs[10])
    def test_journey_persistence(self):
        rows = list(jr.journey())
        for i in list(range(1, 10)) + list(range(11, 17)):
            self.assertEqual(list(rows[i]["input"][:5]), rows[i - 1]["expected"]["post"])
        self.assertEqual(tuple(rows[0]["input"][:5]), G)
        self.assertEqual(tuple(rows[10]["input"][:5]), G)

if __name__ == "__main__":
    unittest.main()
