"""Floored valuation, exact guard and multi-account extraction controls."""
import dataclasses
import contextlib
import io
import json
import shutil
import tempfile
import inspect
import itertools
import os
import sys
import unittest
from unittest.mock import patch
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tools'))
import check_no_free_value as checker
import spot_reference as reference
import write_manifest
from spot_reference import Command, K, Pool, World, step


if os.environ.get('SPOT_STAGE3_DEFECT') == 'pool-coalition-shares':
    source = inspect.getsource(checker.valuations).replace('return floored, exact, p.supply, pooled',
                                                          'return pooled, exact, p.supply, pooled')
    namespace = dict(vars(checker))
    exec(compile(source, '<pool-coalition-shares>', 'exec'), namespace)
    checker.valuations = namespace['valuations']


def gains(before, after, coalition):
    function = checker.valuation_gains
    defect = os.environ.get('SPOT_STAGE3_DEFECT')
    if defect in ('exact-only', 'ignore-exact'):
        source = inspect.getsource(function).replace('"floored_burn_payout": dominates(a, b)',
                                                     '"floored_burn_payout": False')
        if defect == 'ignore-exact':
            source = inspect.getsource(function).replace('dominates(tuple(n * bd for n in an), tuple(n * ad for n in bn))', 'False')
        namespace = dict(vars(checker))
        exec(compile(source, '<exact-only>', 'exec'), namespace)
        function = namespace['valuation_gains']
    return function(before, after, coalition)


def implementation():
    if os.environ.get('SPOT_STAGE3_DEFECT') != 'cross-account-mint':
        return step
    source = inspect.getsource(step)
    old = 'mint = min(mf(u0, p.supply, p.r0), mf(u1, p.supply, p.r1))'
    source = source.replace(old, old + '\n        if cmd.actor == 0: mint *= 2')
    source = source.replace('lp(cmd.actor, mint)', 'lp(1 if cmd.actor == 0 else cmd.actor, mint)')
    namespace = dict(vars(reference))
    exec(compile(source, '<cross-account-mint>', 'exec'), namespace)
    return namespace['step']


class Stage3(unittest.TestCase):
    def test_manifest_retains_external_pins_when_refreshing_cli(self):
        with tempfile.TemporaryDirectory(prefix="spot-manifest-") as directory:
            target = Path(directory) / "application"
            shutil.copytree(Path(__file__).resolve().parents[1], target,
                            ignore=shutil.ignore_patterns("__pycache__"))
            before = json.loads((target / "SOURCE_PINS.json").read_text())
            cli = Path(directory) / "cli"
            cli.write_bytes(b"replacement CLI fixture")
            function = write_manifest.main
            if os.environ.get("SPOT_STAGE3_DEFECT") == "refresh-spec-pins":
                source = inspect.getsource(function).replace("manifest[name] = previous[name]",
                    'manifest[name] = {} if name == "specification" else previous[name]')
                namespace = dict(vars(write_manifest))
                exec(compile(source, "<refresh-spec-pins>", "exec"), namespace)
                function = namespace["main"]
            with patch.object(write_manifest, "APP", target), patch.object(sys, "argv", [
                    "write_manifest", "--preserve-input-pins", "--cli", str(cli)]), patch.object(
                    write_manifest.subprocess, "check_output", return_value="fixture-version"), contextlib.redirect_stdout(io.StringIO()):
                if function is not write_manifest.main:
                    function.__globals__["APP"] = target
                function()
            after = json.loads((target / "SOURCE_PINS.json").read_text())
            self.assertEqual({k: v for k, v in before.items() if k not in ("files", "cli")},
                             {k: v for k, v in after.items() if k not in ("files", "cli")})
            self.assertNotEqual(before["cli"], after["cli"])

    def test_complete_depth_counts_and_budget_refusal(self):
        run = checker.exhaustive
        if os.environ.get('SPOT_STAGE3_DEFECT') == 'credit-partial-depth':
            source = inspect.getsource(run).replace('"bound": completed', '"bound": bound')
            namespace = dict(vars(checker))
            exec(compile(source, '<credit-partial-depth>', 'exec'), namespace)
            run = namespace['exhaustive']
        with patch.object(checker, 'initial_domain', return_value=(World(),)), patch.object(
                checker, 'command_domain', side_effect=lambda a: (Command(1, a=1, actor=a, recipient=a),)):
            result = checker.exhaustive(2, coalitions=((0, 1),))
            self.assertEqual(result['bound'], 2)
            self.assertEqual(result['counts']['sequences_length_1'], 2)
            self.assertEqual(result['counts']['sequences_length_2'], 4)
            # A deterministic expired clock must never credit an unvisited depth.
            with patch.object(checker.time, 'monotonic', side_effect=(0, 121, 121)):
                result = run(5, max_seconds=120)
            self.assertTrue(result['budget_exhausted'])
            self.assertEqual(result['bound'], 0)
            self.assertEqual(result['counts'], {})

    def test_floored_primary_does_not_hide_behind_exact_guard(self):
        # A legal initialization witnesses the hypothesis; baseline is after
        # creation, with its lock already paid. No missing donation is assumed.
        initial = World(None, (0, 0, 0, 0, 200000, 300000, 0, 0))
        created = step(initial, Command(0, a=100000, b=200000, sqrt=141421))
        self.assertIsNone(created.reason)
        before = created.post
        result = step(before, Command(1, a=51))
        self.assertIsNone(result.reason)
        checker.check_step(before, Command(1, a=51), result)
        self.assertEqual(checker.holdings(before, 0), (199292, 298585))
        self.assertEqual(checker.holdings(result.post, 0), (199292, 298586))
        self.assertTrue(gains(before, result.post, (0,))['floored_burn_payout'])
        self.assertFalse(gains(before, result.post, (0,))['exact_pro_rata'])
        self.assertGreaterEqual(result.post.pool.r0 * result.post.pool.r1,
                                before.pool.r0 * before.pool.r1)

    def test_exact_guard_retains_fractional_claim_gain(self):
        before = World(Pool(1, 1, 1001), (1, 1, 0, 0, 0, 0, 0, 0), (1, 0, K))
        # A planted unauthorized LP issue is invisible to integer burn values.
        after = dataclasses.replace(before, pool=Pool(1, 1, 1002), shares=(2, 0, K))
        self.assertFalse(gains(before, after, (0,))["floored_burn_payout"])
        self.assertTrue(gains(before, after, (0,))["exact_pro_rata"])

    def test_coalition_no_free_value(self):
        # The planted bug doubles Alice's mint and credits Bob. Alice cannot
        # burn Bob's shares, and Bob alone cannot trigger the bad branch.
        before = World(Pool(1000, 1000, 1000), (1000, 1000, 0, 0, 4, 4, 4, 4))
        before = dataclasses.replace(before, shares=(0, 0, K))
        run = implementation()
        added = run(before, Command(3, a=2, b=2))
        self.assertIsNone(added.reason)
        burned = run(added.post, Command(4, a=added.outputs[2], actor=1, recipient=1))
        # The correct core gives Bob no shares, so the burn refuses unchanged.
        # The mutant pays 3 of each asset after Alice supplied only 2.
        self.assertFalse(gains(before, burned.post, (0, 1))['floored_burn_payout'])
        self.assertFalse(gains(before, burned.post, (0, 1))['exact_pro_rata'])
        self.assertFalse(gains(before, burned.post, (0, 1))['pooled_burn_upper_bound'])

    def test_cross_account_defect_requires_cooperation(self):
        before = World(Pool(1000, 1000, 1000), (1000, 1000, 0, 0, 4, 4, 4, 4), (0, 0, K))
        run = implementation()
        for actor in (0, 1):
            frontier = {before}
            commands = (Command(3, a=2, b=2, actor=actor, recipient=actor),
                        Command(4, a=4, actor=actor, recipient=actor))
            for _ in range(3):
                frontier = {run(w, c).post for w, c in itertools.product(frontier, commands)}
                for after in frontier:
                    self.assertFalse(any(gains(before, after, (actor,)).values()))

    def test_coalition_sum_of_floors_and_pooled_upper_bound(self):
        w = World(Pool(1, 1, 1002), (1, 1, 0, 0, 0, 0, 0, 0), (1, 1, 1000))
        self.assertEqual(checker.holdings(w, (0, 1)), (0, 0))
        # Sum-floor cannot silently become floor-sum (both guards are retained).
        w = dataclasses.replace(w, pool=Pool(501, 501, 1002), balances=(501, 501, 0, 0, 0, 0, 0, 0))
        self.assertEqual(checker.valuations(w, (0, 1))[0], (0, 0))
        self.assertEqual(checker.valuations(w, (0, 1))[3], (1, 1))
        for invalid in ((), (0, 0), (2,), (True,)):
            with self.assertRaises(ValueError): checker.valuations(w, invalid)


if __name__ == '__main__':
    unittest.main()
