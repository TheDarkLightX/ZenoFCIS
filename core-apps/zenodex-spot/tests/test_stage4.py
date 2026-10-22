"""Accounting theorem premises, dust witnesses and search defect controls."""
import dataclasses
from fractions import Fraction
import inspect
import os
from pathlib import Path
import sys
import subprocess
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tools'))
import check_exact_value as accounting
import search_compounding as search
import run_stage4_checks as runner
from check_no_free_value import valuations
from spot_reference import Command, Decision, Pool, step

search_defect = os.environ.get('SPOT_STAGE4_DEFECT')
if search_defect in ('credit-unvisited-search', 'omit-excess-measurement'):
    source = inspect.getsource(search.search)
    if search_defect == 'credit-unvisited-search':
        source = source.replace("counts['selected_steps'] += 1", "counts['selected_steps'] += 0")
    else:
        source = source.replace("'largest_cumulative_signed_rounding_excess_by_asset': maximum_excess",
                                "'largest_cumulative_signed_rounding_excess_by_asset': [0, 0]")
    namespace = dict(vars(search))
    exec(compile(source, '<planted-search-defect>', 'exec'), namespace)
    search.search = namespace['search']
if search_defect == 'ignore-growing-cycle':
    search.repeated_configuration_gain = lambda *_: False
if search_defect == 'omit-timeout-record':
    source = inspect.getsource(runner.run).replace("'status': 'timeout-UNKNOWN'", "'status': 'completed'")
    namespace = dict(vars(runner))
    exec(compile(source, '<planted-timeout-defect>', 'exec'), namespace)
    runner.run = namespace['run']
if search_defect in ('wrong-lean-pin', 'wrong-lean-tree', 'unchecked-proof', 'low-disk-proof', 'changed-proof-source'):
    source = inspect.getsource(runner.proof)
    message = {'wrong-lean-pin': 'wrong Lean pin',
               'wrong-lean-tree': 'Lean runtime tree differs from release pin',
               'unchecked-proof': 'untrusted or failed proof',
               'low-disk-proof': 'below 6 GiB: stop builds',
               'changed-proof-source': 'proof source changed during checks; rerun'}[search_defect]
    source = source.replace(f"raise ValueError('{message}')", 'pass')
    namespace = dict(vars(runner))
    exec(compile(source, '<planted-proof-gate-defect>', 'exec'), namespace)
    runner.proof = namespace['proof']


def checked(name):
    function = getattr(accounting, name)
    defect = os.environ.get('SPOT_STAGE4_DEFECT')
    if defect:
        source = inspect.getsource(function)
        source = source.replace(f'raise AssertionError("{defect}")', 'pass')
        if defect == 'one-atom-coalition':
            source = source.replace('len(members)', '1')
        namespace = dict(vars(accounting))
        exec(compile(source, '<planted-accounting-defect>', 'exec'), namespace)
        function = namespace[name]
    return function


class Stage4(unittest.TestCase):
    def setUp(self):
        self.before = search.initialized(search.seeds()[0])
        self.command = Command(1, a=51)
        self.result = step(self.before, self.command)

    def transition(self, before=None, command=None, result=None, coalition=(0,)):
        return checked('check_transition')(before or self.before, command or self.command,
                                           result or self.result, coalition)

    def test_isolated_actor_control(self):
        with self.assertRaisesRegex(AssertionError, '^isolated_actor$'):
            self.transition(command=Command(1, a=51, actor=1))

    def test_reject_is_no_op_control(self):
        with self.assertRaisesRegex(AssertionError, '^reject_is_no_op$'):
            self.transition(result=Decision('DOMAIN', self.result.post))

    def test_asset_conservation_control(self):
        balances = list(self.result.post.balances)
        balances[4] += 1
        after = dataclasses.replace(self.result.post, balances=tuple(balances))
        with self.assertRaisesRegex(AssertionError, '^asset_conservation$'):
            self.transition(result=dataclasses.replace(self.result, post=after))

    def test_outside_liquid_control(self):
        balances = list(self.result.post.balances)
        balances[6] -= 1
        balances[4] += 1
        after = dataclasses.replace(self.result.post, balances=tuple(balances))
        with self.assertRaisesRegex(AssertionError, '^outside_liquid_non_decreasing$'):
            self.transition(result=dataclasses.replace(self.result, post=after))

    def test_positive_locked_pool_control(self):
        after = dataclasses.replace(self.result.post, shares=(140421, 1, 999))
        with self.assertRaisesRegex(AssertionError, '^positive_locked_pool$'):
            self.transition(result=dataclasses.replace(self.result, post=after))

    def test_creation_cost_control(self):
        genesis, prefix, _ = search.seeds()[0]
        result = step(genesis, prefix[0])
        after = dataclasses.replace(result.post, shares=(141422, 0, 1000))
        with self.assertRaisesRegex(AssertionError, '^creation_cost$'):
            self.transition(genesis, prefix[0], dataclasses.replace(result, post=after))

    def test_fixed_outside_shares_control(self):
        after = dataclasses.replace(self.result.post, shares=(140420, 1, 1000))
        with self.assertRaisesRegex(AssertionError, '^fixed_positive_outside_shares$'):
            self.transition(result=dataclasses.replace(self.result, post=after))

    def test_product_control(self):
        balances = list(self.before.balances)
        balances[0] -= 1
        balances[4] += 1
        after = dataclasses.replace(self.before, pool=Pool(99999, 200000, 141421), balances=tuple(balances))
        with self.assertRaisesRegex(AssertionError, '^reserve_per_share_product$'):
            self.transition(result=Decision(None, after))

    def test_liquidity_ratio_control(self):
        balances = list(self.before.balances)
        balances[0] -= 1
        balances[4] += 1
        balances[1] += 10
        balances[5] -= 10
        after = dataclasses.replace(self.before, pool=Pool(99999, 200010, 141421), balances=tuple(balances))
        with self.assertRaisesRegex(AssertionError, '^liquidity_reserves_per_share$'):
            self.transition(command=Command(3, a=1, b=10), result=Decision(None, after))

    def test_floor_error_range_control(self):
        with patch.object(accounting, 'errors', return_value=(Fraction(1), Fraction(0))):
            with self.assertRaisesRegex(AssertionError, '^sum_of_member_floor_errors$'):
                self.transition()

    def test_endpoint_dust_control(self):
        # Deliberately inconsistent error oracle violates the bound, while
        # also making the dust identity true for the synthetic observations.
        with patch.object(accounting, 'errors', side_effect=((Fraction(2), Fraction(0)), (0, 0))), patch.object(
                accounting, 'exact', return_value=(0, 0)), patch.object(
                accounting, 'valuations', side_effect=(((0, 0), (), 1, ()), ((2, 0), (), 1, ()))):
            with self.assertRaisesRegex(AssertionError, '^endpoint_dust$'):
                checked('endpoint')(self.before, self.result.post, (0,))

    def test_dust_telescope_identity_control(self):
        with patch.object(accounting, 'errors', return_value=(0, 0)):
            with self.assertRaisesRegex(AssertionError, '^dust_telescope_identity$'):
                checked('endpoint')(self.before, self.result.post, (0,))

    def test_exact_no_free_value_control(self):
        after = dataclasses.replace(self.before, balances=tuple(
            b+1 if i == 4 else b for i, b in enumerate(self.before.balances)))
        with self.assertRaisesRegex(AssertionError, '^exact_no_free_value$'):
            checked('endpoint')(self.before, after, (0,))

    def test_weighted_bound_control(self):
        # Incomparable vector but positive value at the initial price.
        balances = list(self.before.balances)
        balances[4] += 1
        balances[5] -= 1
        after = dataclasses.replace(self.before, balances=tuple(balances))
        with self.assertRaisesRegex(AssertionError, '^cumulative_weighted_dust_bound$'):
            checked('endpoint')(self.before, after, (0,))

    def test_two_member_one_atom_bound_is_false(self):
        before = search.initialized(search.seeds()[1])
        command = search.seeds()[1][2]
        result = step(before, command)
        self.assertIsNone(result.reason)
        checked('check_transition')(before, command, result, (0, 1))
        observation = checked('endpoint')(before, result.post, (0, 1))
        self.assertEqual(observation['rounding_excess'][1], Fraction(170371, 154371))
        self.assertEqual(observation['floored_delta'], (1, 1))

    def test_other_account_floor_can_fund_the_gain(self):
        before = search.initialized(search.seeds()[2])
        result = step(before, search.seeds()[2][2])
        self.assertIsNone(result.reason)
        self.assertEqual(checked('endpoint')(before, result.post, (0,))['floored_delta'], (1, 0))
        bob_before, bob_after = valuations(before, 1)[0], valuations(result.post, 1)[0]
        self.assertEqual(tuple(a-b for a, b in zip(bob_after, bob_before)), (-1, 0))
        def residual(world):
            return tuple(r-sum(world.shares[a]*r//world.pool.supply for a in (0, 1))
                         for r in (world.pool.r0, world.pool.r1))
        self.assertEqual(residual(before), residual(result.post))
        self.assertEqual(before.balances[2:4], result.post.balances[2:4])

    def test_long_search_counts_and_all_three_measures(self):
        result = search.search(16, trials=12, width=3)
        self.assertEqual(result['counts']['selected_steps'], 192)
        self.assertEqual(result['completed_trace_length'], 16)
        self.assertEqual(set(result['selected_accepted_action_counts']), {'1', '2', '3', '4'})
        self.assertIsNone(result['growing_cycle'])
        self.assertEqual(len(result['largest_cumulative_signed_rounding_excess_by_asset']), 2)
        self.assertGreater(max(map(Fraction, result['largest_cumulative_signed_rounding_excess_by_asset'])), 0)
        self.assertTrue(all(result['cumulative_excess_witnesses']))

    def test_growing_cycle_detector_control(self):
        after = dataclasses.replace(self.before, balances=tuple(
            b+1 if i == 4 else b for i, b in enumerate(self.before.balances)))
        self.assertTrue(search.repeated_configuration_gain(self.before, after, (0,)))
        self.assertFalse(search.repeated_configuration_gain(self.before, self.result.post, (0,)))

    def test_positive_marks_are_not_net_extraction(self):
        result = search.search(32, trials=3, width=2)
        self.assertGreater(max(result['largest_sum_positive_intermediate_marks']),
                           max(result['largest_weakly_dominating_net_gain_by_asset']))

    def test_runner_timeout_retains_measured_work(self):
        import json
        with tempfile.TemporaryDirectory(prefix='spot4-timeout-') as folder:
            with patch.object(runner.subprocess, 'run', side_effect=subprocess.TimeoutExpired(
                    ['owned-child'], 1, output=b'partial', stderr=b'bounded refusal')):
                with self.assertRaisesRegex(ValueError, 'measured work retained'):
                    runner.run(['owned-child'], Path(folder), 'timeout')
            record = json.loads((Path(folder) / 'timeout.command.json').read_text())
            self.assertEqual((record['exit_code'], record['status']), (124, 'timeout-UNKNOWN'))
            self.assertEqual((record['stdout'], record['stderr']), ('partial', 'bounded refusal'))
            self.assertGreaterEqual(record['seconds'], 0)

    def test_proof_tool_and_resource_gates(self):
        # Synthetic process responses test the receipt's fail-closed gates;
        # these responses never count as real proof evidence.
        from types import SimpleNamespace
        pin = (runner.ROOT / 'release/lean-4.30.0-tree.sha256').read_text().strip()
        for scenario, message in (
                ('version', 'wrong Lean pin'), ('tree', 'runtime tree differs'),
                ('unchecked', 'untrusted or failed proof'), ('disk', 'below 6 GiB'),
                ('changed', 'source changed during checks')):
            def response(argv, out, label, **kwargs):
                stdout = 'error: planted negative' if kwargs.get('expected') == 1 else ''
                if label == 'lean-version':
                    stdout = ('wrong' if scenario == 'version' else
                              'Lean (version 4.30.0, commit d024af099ca4bf2c86f649261ebf59565dc8c622)')
                if label == 'lean-tree':
                    stdout = 'wrong' if scenario == 'tree' else f'lean tree_sha256 {pin}'
                if label == 'lean-positive' and scenario == 'unchecked':
                    stdout = 'depends on axioms: [sorryAx]'
                return {'stdout': stdout, 'exit_code': kwargs.get('expected', 0)}
            with self.subTest(scenario=scenario), tempfile.TemporaryDirectory(prefix='spot4-gates-') as folder:
                with patch.object(runner, 'run', side_effect=response), patch.object(
                        runner.shutil, 'disk_usage', return_value=SimpleNamespace(
                            free=0 if scenario == 'disk' else 7*1024**3)), patch.object(
                        runner, 'sha', return_value='test-only'), patch.dict(
                        runner.proof.__globals__, {'run': response, 'sha': lambda _: 'test-only'}):
                    with self.assertRaisesRegex(ValueError, message):
                        runner.proof(Path(folder), Path('fake-cli'), Path(folder))


if __name__ == '__main__':
    unittest.main()
