"""Regression and negative controls for the closed-strategy test checker.

These unit checks do not receive native coverage credit. Run check_round_trips.py
with an explicitly source-built CLI for the native campaign.
"""
from dataclasses import replace
from fractions import Fraction
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]/'tools'))
from check_round_trips import amounts, campaign, closed_score, funded, rational_check
from native_check import decode_command
from spot_reference import Command, Decision, step


class RoundTrips(unittest.TestCase):
    def setUp(self):
        self.before = funded()
        self.created = step(self.before, Command(0, a=100000, b=200000, sqrt=141421))
        self.assertIsNone(self.created.reason)

    def test_cash_gain_control(self):
        balances = list(self.before.balances)
        balances[4] += 1
        after = replace(self.before, balances=tuple(balances))
        with self.assertRaisesRegex(AssertionError, '^closed_spendable_profit$'):
            closed_score(self.before, after)

    def test_open_positions_never_receive_closure_credit(self):
        with self.assertRaisesRegex(AssertionError, '^positions_not_restored$'):
            closed_score(self.before, self.created.post)

    def test_rational_fee_and_poststate_controls(self):
        world = self.created.post
        command = Command(1, a=1000)
        result = step(world, command)
        self.assertIsNone(result.reason)
        rational_check(world, command, result)
        wrong_fee = list(result.outputs)
        wrong_fee[2] -= 1
        with self.assertRaises(AssertionError):
            rational_check(world, command, replace(result, outputs=tuple(wrong_fee)))
        with self.assertRaises(AssertionError):
            rational_check(world, command, replace(result, post=replace(result.post,
                pool=replace(result.post.pool, r0=result.post.pool.r0+1))))

    def test_rejected_mutation_control(self):
        with self.assertRaisesRegex(AssertionError, '^reject_is_no_op$'):
            rational_check(self.before, Command(1, a=1), Decision('NO_POOL', self.created.post))

    def test_rational_and_quoted_wealth_are_distinct(self):
        world = self.created.post
        cash, exact, quoted = amounts(world)
        self.assertEqual(cash, tuple(
            world.balances[4+i]+world.balances[6+i] for i in (0, 1)))
        self.assertTrue(all(isinstance(x, Fraction) for x in exact))
        self.assertTrue(all(0 <= e-q < 2 for e, q in zip(exact, quoted)))

    def test_command_surface_refuses_extra_fields(self):
        command = {name: getattr(Command(1, a=100), name) for name in Command.__dataclass_fields__}
        for field in ('fee', 'fee_switch', 'donation', 'flash', 'debt', 'transfer'):
            with self.assertRaisesRegex(ValueError, 'closed command'):
                decode_command(dict(command, **{field: 1}))

    def test_all_rejected_campaign_cannot_pass(self):
        # Mocked probes earn no native evidence; a rejection-only oracle must
        # fail before a summary or accepted-call coverage can be produced.
        with tempfile.TemporaryDirectory(prefix='spot-coverage-control-') as folder:
            with patch('check_round_trips.probe'), patch('check_round_trips.step',
                    side_effect=lambda world, command: Decision('DOMAIN', world)):
                with self.assertRaises(AssertionError):
                    campaign(Path(__file__), Path(folder), length=16)
            self.assertFalse((Path(folder)/'summary.json').exists())


if __name__ == '__main__':
    unittest.main()
