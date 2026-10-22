"""Funded fake-value fixture, complete-journey and bounded-search checks."""
import contextlib
import copy
import inspect
import io
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

TOOLS = Path(__file__).resolve().parents[1]/'tools'
APP = TOOLS.parent
sys.path.insert(0, str(TOOLS))
import check_no_free_value as nfv
import run_mounted_journey as journey
from make_test_fixture import LABEL, fixture_policy
from native_check import parse_observation
from build_contracts import REASONS
from run_native_checks import balance_boundaries
from spot_reference import Command, World, command_values, state_values, step

DEFECT = os.environ.get('SPOT_ROUND2_DEFECT')
if DEFECT in ('credit-partial-bound', 'cap-budget-at-120'):
    source = inspect.getsource(nfv.main)
    old, new = (('result["bound"] != args.bound', 'result["bound"] == 0') if DEFECT == 'credit-partial-bound'
                else ('not math.isfinite(args.max_seconds) or args.max_seconds <= 0',
                      'not 0 < args.max_seconds <= 120'))
    if source.count(old) != 1:
        raise ValueError('budget mutation anchor missing')
    exec(compile(source.replace(old, new, 1), '<'+DEFECT+'>', 'exec'), nfv.__dict__)
if DEFECT == 'credit-incomplete-journey':
    source = inspect.getsource(journey.journey_summary)
    exec(compile(source.replace('complete and accepted == 5', 'True', 1),
                 '<credit-incomplete-journey>', 'exec'), journey.__dict__)


@unittest.skipUnless(all(os.environ.get(name) for name in
    ('SPOT_MOUNT_BINARY', 'SPOT_FIXTURE_MOUNT_BINARY', 'ZENO_FCIS_CLI')),
    'select production and fake-value test fixture executables explicitly')
class Round2(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix='spot5-round2-', dir=os.environ.get('TMPDIR'))
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        self.production = Path(os.environ['SPOT_MOUNT_BINARY'])
        self.fixture = Path(os.environ['SPOT_FIXTURE_MOUNT_BINARY'])
        self.cli = Path(os.environ['ZENO_FCIS_CLI'])

    def test_fixture_only_changes_approved_genesis(self):
        original = json.loads((APP/'v2/policy.json').read_text())
        before = copy.deepcopy(original)
        fixture = fixture_policy(original)
        if DEFECT == 'fixture-funding-drift':
            fixture['genesis']['2016'] += 1
        self.assertEqual(original, before)
        self.assertTrue(all(value == 0 for value in original['genesis'].values()))
        self.assertEqual({k:v for k,v in fixture.items() if k!='genesis'},
                         {k:v for k,v in original.items() if k!='genesis'})
        state = [fixture['genesis'][key] for key in original['genesis']]
        expected = World(None, (0,0,0,0,10**12,10**12,10**12,10**12))
        self.assertEqual(tuple(state), state_values(expected))

    def test_zero_genesis_cannot_be_reported_as_accepted_journey(self):
        result = journey.run(self.production, self.root/'zero.db', self.root/'zero', 'wire')
        self.assertEqual(result['legitimate_accepted'], 0)
        self.assertNotEqual(result['status'], 'PASS')
        rows = [{'name': name, 'legitimate': True, 'response': {'class': 'Accept'}}
                for name in journey.LEGITIMATE_STEPS]
        for count in range(5):
            self.assertNotEqual(journey.journey_summary(rows[:count], 'wire')['status'], 'PASS')

    def test_funded_fixture_journey_accepts_and_replays(self):
        if DEFECT == 'fixture-http-class-mismatch':
            original = journey.Journey.post
            def changed(instance, *args, **kwargs):
                status, packet = original(instance, *args, **kwargs)
                if packet.get('class') == 'Accept':
                    packet['class'] = 'Reject'
                return status, packet
            self.enterContext(patch.object(journey.Journey, 'post', changed))
        result = journey.run(self.fixture, self.root/'fixture.db', self.root/'fixture', 'wire', True)
        self.assertEqual(result['status'], 'PASS', result)
        self.assertEqual(result['legitimate_accepted'], 5)
        self.assertEqual(result['fixture_label'], LABEL)
        self.assertTrue(result['restart_original_receipt_equal'])
        rows = [json.loads(line) for line in (self.root/'fixture/J-SPOT.jsonl').read_text().splitlines()]
        self.assertEqual([r['response']['class'] for r in rows if not r['legitimate']], ['Reject']*5)

    def test_k4_budget_records_only_completed_depth(self):
        for budget in (None, 180):
            argv = ['check_no_free_value', '--bound', '4', '--coalitions', 'pair', '--out', str(self.root/'budget.json')]
            if budget is not None:
                argv += ['--max-seconds', str(budget)]
            result = {'bound': 3, 'requested_bound': 4, 'budget_exhausted': True,
                      'measures': {'exact_pro_rata': {'first_witness': None}}}
            with patch.object(sys, 'argv', argv), patch.object(nfv, 'exhaustive', return_value=result) as search:
                with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                    try:
                        code = nfv.main()
                    except SystemExit as error:
                        self.fail('valid time budget refused: '+str(error))
            self.assertEqual(code, 1, 'partial depth must not pass')
            self.assertEqual(search.call_args.kwargs['max_seconds'], 120 if budget is None else budget)
            saved = json.loads((self.root/'budget.json').read_text())
            self.assertEqual((saved['bound'], saved['requested_bound'], saved['status']), (3, 4, 'timeout-UNKNOWN'))

    def test_component_assembly_matches_root(self):
        assembled = self.root/'assembled'
        subprocess.run([sys.executable, str(TOOLS/'build_composition.py'), '--out', str(assembled)],
                       check=True, capture_output=True, text=True)
        if DEFECT == 'composition-mapping-drift':
            path = assembled/'v2/policy.json'
            policy = json.loads(path.read_text())
            policy['cases'][-1]['post']['2016'] = '0'
            path.write_text(json.dumps(policy, indent=2)+'\n')
        for name in ('project.zeno', 'v2/policy.json'):
            self.assertEqual((assembled/name).read_bytes(), (APP/name).read_bytes(), name)

    def test_standalone_component_native_decisions(self):
        current = World(None, (0,0,0,0,10**12,10**12,10**12,10**12))
        operations = (Command(0,a=1000000,b=1000000,sqrt=1000000), Command(1,a=10000),
                      Command(2,a=10000,limit0=3000000000), Command(3,a=10000,b=10000), Command(4,a=100))
        count = 0
        for command in operations:
            for candidate in (command, Command(command.action, a=0)):
                expected = step(current, candidate)
                values = state_values(current)[:5] + command_values(candidate)[:7] + (1,0,0,current.shares[0],1)
                target = self.root/f'pool-{count}.json'
                r = subprocess.run([str(self.cli), '--no-kb', 'contract', 'probe', 'explain',
                     str(APP/'components/pool'), '--input', ' '.join(map(str, values)), '--out', str(target)],
                     capture_output=True, text=True, timeout=30)
                self.assertEqual(r.returncode, 0, r.stderr)
                packet = json.loads(target.read_text())['result']['authority']
                self.assertIn('example', packet, packet)
                actual = parse_observation(packet['example'])
                if DEFECT == 'component-decision-mismatch':
                    actual = (*actual[:2], (actual[2][0]+1,)+actual[2][1:], actual[3])
                self.assertEqual(actual, ('accept' if expected.reason is None else 'reject',
                    None if expected.reason is None else REASONS[expected.reason], state_values(expected.post)[:5], ()))
                count += 1
            current = step(current, command).post
        self.assertEqual(count, 10)
        self.assertEqual(balance_boundaries(self.root), 8)

    def test_funded_create_kill_before_commit_is_atomic(self):
        database, marker = self.root/'crash.db', self.root/'decision-ready'
        journey.invoke(self.fixture, 'init', database)
        before = journey.invoke(self.fixture, 'state', database)
        command = ' '.join(map(str, command_values(Command(0,a=1000000,b=1000000,sqrt=1000000))))
        args = [str(self.fixture), 'keyed', str(database), 'e'*64, command, '1']
        child = subprocess.Popen(args, env=dict(os.environ, SPOT_PAUSE_BEFORE_COMMIT=str(marker)),
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            deadline = time.monotonic()+30
            while not marker.exists() and child.poll() is None and time.monotonic()<deadline:
                time.sleep(.01)
            self.assertTrue(marker.exists())
            child.kill()
            child.communicate(timeout=10)
        finally:
            if child.poll() is None:
                child.kill()
                child.communicate(timeout=10)
        after = journey.invoke(self.fixture, 'state', database)
        if DEFECT == 'fixture-half-applied':
            after['commits'] += 1
        self.assertEqual(after, before)
        with sqlite3.connect(database) as connection:
            self.assertEqual(connection.execute('SELECT count(*) FROM v2_commits').fetchone()[0], 0)
            self.assertEqual(connection.execute('SELECT count(*) FROM v2_deliveries').fetchone()[0], 0)
        committed = journey.invoke(self.fixture, *args[1:])
        self.assertEqual((committed['class'], committed['commit']), ('Accept', 1))
        self.assertEqual(journey.invoke(self.fixture, *args[1:]), committed)


if __name__ == '__main__':
    unittest.main()
