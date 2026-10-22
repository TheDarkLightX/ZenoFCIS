"""Light independent arithmetic, ownership, composition and comparator checks."""
from __future__ import annotations

import dataclasses
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
from spot_reference import Command, Effect, K, N, U, Pool, World, balance_step, limbs, step
from native_check import decode_command, decode_world, expected_observation, parse_observation, strict_json
from replay_corpus import expected, load_rows
from prepare_corpus import expectation, zero_payout_labels


def seed(r0=1_000_000, r1=1_000_000, supply=1_000_000, *, ingress0=0, ingress1=0, status=0):
    return World(Pool(r0, r1, supply, status), (r0, r1, ingress0, ingress1, 10**12, 10**12, 10**12, 10**12),
                 (max(supply - K, 0), 0, min(K, supply)))


class Spot(unittest.TestCase):
    def test_owner_fee_boundaries(self):
        for a, fee, protocol in ((1333, 4, 0), (1334, 5, 1), (10000, 30, 6)):
            before = seed()
            result = step(before, Command(1, a=a))
            self.assertIsNone(result.reason)
            self.assertEqual(result.outputs[2:4], (fee, protocol))
            self.assertEqual(result.post.pool.r0, 1000000 + a - protocol)
            self.assertEqual(result.post.balances[2], protocol)
        self.assertEqual(step(seed(), Command(1, a=10000)).outputs, (10000, 9871, 30, 6, 24, 9970, 0, 0))

    def test_exact_out_delivers_wanted_not_forward_quote(self):
        result = step(seed(1, 10000, 1001), Command(2, a=100, limit0=N))
        # The cap can reject imbalanced coarse quotes; a successful balanced
        # vector independently fixes the gross and all fee components.
        self.assertEqual(result.reason, "QUOTE_GAP")
        result = step(seed(), Command(2, a=10000, limit0=N))
        self.assertIsNone(result.reason)
        self.assertEqual(result.outputs, (10133, 10000, 31, 6, 25, 10102, 10000, 0))
        self.assertEqual(result.post.pool, Pool(1010127, 990000, 1000000))

    def test_conservative_gross_bound(self):
        before = seed(N-10000, 1000000)
        result = step(before, Command(1, a=10006))
        self.assertEqual(result.reason, "POST_DOMAIN")
        self.assertIs(result.post, before)
        self.assertEqual(result.effects, ())

    def test_ingress_overflow_rolls_back_everything(self):
        before = seed(ingress0=U-5)
        result = step(before, Command(1, a=10000))
        self.assertEqual(result.reason, "INGRESS_OVERFLOW")
        self.assertIs(result.post, before)
        self.assertEqual((result.effects, result.outputs), ((), ()))
        self.assertIsNone(step(seed(ingress0=U-6), Command(1, a=10000)).reason)
        self.assertIsNone(step(seed(ingress0=U), Command(1, a=1333)).reason)

    def test_balance_checks_debit_before_using_credit(self):
        self.assertEqual(balance_step(1, 2, 100), ("INSUFFICIENT_BALANCE", 1))
        self.assertEqual(balance_step(U, 0, 1), ("BALANCE_OVERFLOW", U))
        self.assertEqual(balance_step(U, 1, 1, True, 1), ("INGRESS_ROLE", U))
        for value in ((1 << 32)-1, (1 << 64)-1, (1 << 96)-1):
            self.assertEqual(balance_step(value, 0, 1), (None, value+1))
            self.assertEqual(balance_step(value+1, 1, 0), (None, value))

    def test_lock_and_zero_payout_precedence(self):
        for recipient in (0, 5):
            self.assertEqual(step(seed(), Command(4, a=1, actor=3, recipient=recipient)).reason, "RESERVED_LP_LOCK")
        for minimum in (0, 1):
            before = seed(1, 1, 1001)
            result = step(before, Command(4, a=1, limit0=minimum, limit1=minimum))
            self.assertEqual(result.reason, "ZERO_PAYOUT")
            self.assertIs(result.post, before)
        self.assertEqual(step(seed(1, 1, 1001), Command(4, a=2)).reason, "INSUFFICIENT_LP")

    def test_all_reserved_endpoint_classes(self):
        for reserved in (2, 3, 4, 5):
            self.assertEqual(step(seed(), Command(1, a=10000, actor=reserved)).reason, "RESERVED_ROLE")
            self.assertEqual(step(seed(), Command(1, a=10000, recipient=reserved)).reason, "RESERVED_ROLE")

    def test_prestate_and_status_precedence(self):
        world = seed()
        broken = dataclasses.replace(world, balances=(1000001, *world.balances[1:]))
        self.assertEqual(step(broken, Command(1, a=1, actor=4)).reason, "DOMAIN")
        for status in (1, 2):
            self.assertEqual(step(seed(0, 0, 0, status=status), Command(1, a=1)).reason, "POOL_INACTIVE")
        self.assertEqual(step(seed(0, 0, 0), Command(3, a=1, b=1)).reason, "EMPTY_POOL")
        self.assertEqual(step(World(), Command(1, a=1)).reason, "NO_POOL")

    def test_create_uses_real_lock_and_one_pool_per_pair(self):
        before = World(None, (0,0,0,0,10000,10000,0,0))
        self.assertEqual(step(before, Command(0, a=1000,b=1000,sqrt=1000)).reason, "INITIAL_TOO_SMALL")
        self.assertEqual(step(before, Command(0, a=1000,b=1000,sqrt=1001)).reason, "BAD_SQRT")
        result = step(before, Command(0, a=1001,b=1001,sqrt=1001))
        self.assertIsNone(result.reason)
        self.assertEqual(result.post.shares, (1,0,1000))
        self.assertEqual(step(result.post, Command(0, a=1001,b=1001,sqrt=1001)).reason, "POOL_EXISTS")

    def test_balance_alias_aggregation_and_two_assets(self):
        for recipient in (0, 1):
            for direction in (0, 1):
                before = seed()
                result = step(before, Command(1,direction=direction,a=10000,recipient=recipient))
                self.assertIsNone(result.reason)
                for asset in (0, 1):
                    self.assertEqual(sum(before.balances[asset::2]), sum(result.post.balances[asset::2]))
                self.assertGreaterEqual(result.post.pool.r0 * result.post.pool.r1, before.pool.r0 * before.pool.r1)

    def test_swap_round_trips(self):
        checked = 0
        for x in (7,31,1001,1000000):
            for y in (7,31,1001,1000000):
                for a in (2,33,1334,10000):
                    for direction in (0,1):
                        before = seed(x,y)
                        first = step(before, Command(1,direction=direction,a=a))
                        if first.reason is not None: continue
                        second = step(first.post, Command(1,direction=1-direction,a=first.outputs[1]))
                        if second.reason is not None: continue
                        self.assertLessEqual(second.post.balances[4+direction], before.balances[4+direction])
                        checked += 1
        self.assertGreater(checked, 20)

    def test_add_burn_round_trips(self):
        checked = 0
        for x,y in ((1001,1001),(10001,20003),(31,1001),(1000000,1000000)):
            for a,b in ((1,1),(100,300),(1334,2000)):
                before = seed(x,y,1001)
                added = step(before, Command(3,a=a,b=b))
                if added.reason is not None: continue
                burned = step(added.post, Command(4,a=added.outputs[2]))
                if burned.reason is not None: continue
                for asset in (0,1): self.assertLessEqual(burned.post.balances[4+asset], before.balances[4+asset])
                checked += 1
        self.assertGreater(checked, 2)

    def test_immutable_snapshot_and_rejected_alias_inputs(self):
        before = seed()
        with self.assertRaises(dataclasses.FrozenInstanceError): before.pool.r0 = 0
        with self.assertRaises(TypeError): World(before.pool, list(before.balances), before.shares)
        with self.assertRaises(TypeError): World(Pool(1,1,1001), (0,)*8, ([1],0,1000))
        result = step(before, Command(1,a=1))
        self.assertIs(result.post, before)
        with self.assertRaises(TypeError): result.post.balances[0] = 0

    def test_closed_decoder_and_json_canonicality(self):
        command = dataclasses.asdict(Command(1,a=1000))
        for field,value in (("fee_bps",30),("protocol_share",2000),("tier",0)):
            with self.assertRaises(ValueError): decode_command({**command,field:value})
        for value in (True,"1000",1.5):
            with self.assertRaises(ValueError): decode_command({**command,"a":value})
        for text in ('{"a":1,"a":2}', '{"a":1.0}', '{"a":NaN}'):
            with self.assertRaises(ValueError): strict_json(text)
        with self.assertRaises(ValueError): decode_world({"pool":None,"balances":[True]*8,"shares":[0]*3})
        with self.assertRaises(TypeError): Command(1,a=[1000])

    def test_prepared_corpus_is_complete_and_rejects_bad_goldens(self):
        rows=load_rows(Path(__file__).resolve().parents[1]/"corpus/c1.jsonl","C1")
        self.assertEqual(len(rows),31)
        self.assertEqual(len(zero_payout_labels()),24)
        reject=expectation(step(seed(),Command(1,a=1)))
        with self.assertRaises(ValueError): expected({"expect":{**reject,"outputs":[1]}})
        with self.assertRaises(ValueError): expected({"expect":{**reject,"reason":"UNKNOWN"}})

    def test_comparator_detects_perturbations(self):
        before = seed()
        result = step(before, Command(1,a=10000))
        observation = expected_observation(result)
        for index in range(4):
            changed = list(observation)
            changed[index] = ("reject", 200, (1,), ())[index]
            self.assertNotEqual(tuple(changed), observation)
        altered_effect = dataclasses.replace(result.effects[0], amount=result.effects[0].amount+1)
        self.assertNotEqual(expected_observation(dataclasses.replace(result,effects=(altered_effect,*result.effects[1:]))), observation)
        self.assertEqual(parse_observation('1 | reject 200 1 | -'), ('reject',200,(1,),()))

    def test_no_truncation_into_native_profile(self):
        for value in (-1,U+1,True):
            with self.assertRaises(ValueError): limbs(value)
        with self.assertRaises(ValueError): decode_world({"pool":None,"balances":[U+1,0,0,0,0,0,0,0],"shares":[0,0,0]})


if __name__ == "__main__": unittest.main()
