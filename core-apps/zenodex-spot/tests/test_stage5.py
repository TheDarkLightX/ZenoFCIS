"""Mounted authority, identity, publication and recovery regression portfolio."""
import copy
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile
import time
import unittest

TOOLS = Path(__file__).resolve().parents[1] / 'tools'
sys.path.insert(0, str(TOOLS))
from adopter_fixture import inspect
from native_check import expected_observation, decode_command, decode_world
from replay_corpus import expected, load_rows
from run_mounted_journey import compare
from run_stage4_checks import premises
from spot_http import Binding, Identity, Mount, local_http
from spot_reference import Command, World, command_values, state_values, step

DEFECT = os.environ.get('SPOT_STAGE5_DEFECT')


def invoke(binary, *arguments, env=None):
    result = subprocess.run([str(binary), *map(str, arguments)], capture_output=True,
                            text=True, timeout=35, env=env, check=False)
    if result.returncode:
        raise ValueError(result.stderr)
    packet = json.loads(result.stdout)
    if record := os.environ.get('SPOT_STAGE5_TRANSCRIPT'):
        with Path(record).open('a') as out:
            out.write(json.dumps({'binary': str(binary), 'arguments': list(map(str, arguments)),
                                  'exit_code': result.returncode, 'response': packet}, sort_keys=True)+'\n')
    return packet


def request(mount, body, key='1'*64, token='a'*24):
    data = json.dumps(body, separators=(',', ':')).encode()
    wire = (f'POST /commands HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {token}\r\n'
            f'Idempotency-Key: {key}\r\nContent-Length: {len(data)}\r\n\r\n').encode()+data
    raw = local_http(mount, wire)
    head, answer = raw.split(b'\r\n\r\n', 1)
    return int(head.split()[1]), json.loads(answer)


def packet_observation(packet):
    return ('accept' if packet['class'] == 'Accept' else 'reject', packet['reason'],
            tuple(packet['post']), tuple((ch, tuple(payload)) for ch, payload in packet['deliveries']))


@unittest.skipUnless(os.environ.get('SPOT_MOUNT_BINARY') and os.environ.get('COUNTER_MOUNT_BINARY'),
                     'select generated native mount binaries explicitly')
class Stage5(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.spot = Path(os.environ['SPOT_MOUNT_BINARY'])
        cls.counter = Path(os.environ['COUNTER_MOUNT_BINARY'])

    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix='spot5-', dir=os.environ.get('TMPDIR'))
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        self.binding = Binding((Identity('alice', 0, 'a'*24), Identity('bob', 1, 'b'*24)))

    def spot_mount(self):
        db = self.root / 'spot.db'
        invoke(self.spot, 'init', db)
        return Mount(self.spot, db, self.binding)

    def test_authenticated_identity_and_closed_ingress(self):
        mount = self.spot_mount()
        before = invoke(self.spot, 'state', mount.database)
        if DEFECT == 'trust-body-actor':
            # Planted source-level defect: accepting a body actor bypasses this
            # binding. The isolated module uses the real handler and native path.
            import spot_http
            original = spot_http.Binding.command
            def trust(binding, identity, body):
                return Command(**{k: v for k, v in body.items() if k != 'recipient'},
                               recipient=0)
            spot_http.Binding.command = trust
            self.addCleanup(setattr, spot_http.Binding, 'command', original)
        bad = ({'action': 1, 'a': 10, 'actor': 1}, {'action': 1, 'a': 10, 'recipient': 2},
               {'action': 1, 'a': True}, {'action': 1, 'a': '10'}, {'action': 1, 'fee': 30})
        for body in bad:
            self.assertEqual(request(mount, body)[0], 400, body)
        self.assertEqual(request(mount, {'action': 1}, token='x'*24)[0], 401)
        self.assertEqual(invoke(self.spot, 'state', mount.database), before)
        a = self.binding.command(self.binding.actor('Bearer '+'b'*24), {'action': 1, 'recipient': 'alice'})
        self.assertEqual((a.actor, a.recipient), (1, 0))

    def test_identity_graph_and_request_aliases(self):
        with self.assertRaises((ValueError, TypeError)):
            Binding(list(self.binding.identities))
        with self.assertRaises(ValueError):
            Binding((Identity('alice', 0, 'a'*250), self.binding.identities[1]))
        identity = self.binding.actor('Bearer '+'a'*24)
        body = {'action': 1, 'a': 10}
        command = self.binding.command(identity, body)
        body['a'] = 200
        if DEFECT == 'retained-command-alias':
            command = self.binding.command(identity, body)
        self.assertEqual(command.a, 10)
        mount = self.spot_mount()
        for body, extra in ((b'{"action":1,"action":2}', b''), (b'{"action":1}', b'Content-Length: 12\r\n'),
                            (b'{"action":1}', b'Transfer-Encoding: chunked\r\n')):
            wire = (b'POST /commands HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer '+b'a'*24+
                    b'\r\nIdempotency-Key: '+b'1'*64+b'\r\nContent-Length: '+str(len(body)).encode()+
                    b'\r\n'+extra+b'\r\n'+body)
            self.assertIn(b' 400 ', local_http(mount, wire).split(b'\r\n', 1)[0])

    def test_zero_genesis_legitimate_creation_is_blocked(self):
        mount = self.spot_mount()
        body = {'action': 0, 'a': 1000000, 'b': 1000000, 'sqrt': 1000000}
        status, actual = request(mount, body)
        expected = expected_observation(step(World(), Command(0, a=1000000, b=1000000, sqrt=1000000)))
        if DEFECT == 'invent-genesis-funds':
            actual['class'] = 'Accept'
        self.assertEqual(status, 200)
        self.assertEqual(packet_observation(actual), expected)
        self.assertEqual(actual['class'], 'Reject')
        self.assertEqual(invoke(self.spot, 'state', mount.database)['commits'], 0)

    def test_all_zero_genesis_http_decisions_match_native(self):
        mount = self.spot_mount()
        for action in range(5):
            body = {'action': action, 'a': 1001, 'b': 1001, 'sqrt': 1001}
            _, actual = request(mount, body, key=f'{action:064x}')
            expected = invoke(self.spot, 'probe', ' '.join(map(str, state_values(World()))),
                              ' '.join(map(str, command_values(Command(action, a=1001, b=1001, sqrt=1001)))), '1')
            if DEFECT == 'wrong-http-reason':
                actual['reason'] = 999999
            self.assertEqual(actual, expected)

    def test_c1_l207_native_mount_and_fee_event(self):
        app = TOOLS.parent
        rows = load_rows(app/'corpus/c1.jsonl', 'C1') + load_rows(app/'corpus/l207.jsonl', 'L207')
        matched, excluded = 0, 0
        for row in rows:
            if row['disposition'] == 'inapplicable':
                excluded += 1
                continue
            pre, command, wanted = decode_world(row['pre']), decode_command(row['command']), expected(row)
            packet = invoke(self.spot, 'probe', ' '.join(map(str, state_values(pre))),
                            ' '.join(map(str, command_values(command))), '1')
            native = copy.deepcopy(packet)
            if DEFECT == 'fee-event-mismatch' and any(ch == 303 for ch, _ in packet['deliveries']):
                next(p for ch, p in packet['deliveries'] if ch == 303)[2] += 1
            compare(packet, wanted, native)
            self.assertEqual(packet_observation(packet), expected_observation(step(pre, command)))
            matched += 1
        self.assertEqual((matched, excluded), (136, 104))

    def counter_submit(self, db, key, command, context=1):
        return invoke(self.counter, 'keyed', db, key, command, context)

    def test_no_free_value_native_representatives(self):
        summary, candidates = premises(self.root/'premises')
        self.assertEqual(summary['counterexamples'], [])
        self.assertEqual(summary['counts']['coalition_edge_checks'], 44800)
        self.assertEqual(len(candidates), 37)
        for pre, command, expected in candidates:
            packet = invoke(self.spot, 'probe', ' '.join(map(str, state_values(pre))),
                            ' '.join(map(str, command_values(command))), '1')
            if DEFECT == 'native-transition-mismatch':
                packet['post'][0] += 1
            self.assertEqual(packet_observation(packet), expected_observation(expected))

    def test_restart_retry_and_all_decision_bundle_replay(self):
        db = self.root / 'counter.db'
        replay = self.root / 'adopter.db'
        invoke(self.counter, 'init', db)
        invoke(self.counter, 'init', replay)
        accepted = self.counter_submit(db, '1'*64, 120)
        failed = self.counter_submit(db, '2'*64, 121)
        rejected = self.counter_submit(db, '3'*64, 120, 0)
        self.assertEqual([p['class'] for p in (accepted, failed, rejected)], ['Accept', 'CommittedFailure', 'Reject'])
        original_history = (('1'*64, (120,), (1,)), ('2'*64, (121,), (1,)), ('3'*64, (120,), (0,)))
        for packet, key, command, context in ((accepted, '1'*64, 120, 1), (failed, '2'*64, 121, 1),
                                               (rejected, '3'*64, 120, 0)):
            native = invoke(self.counter, 'probe', ' '.join(map(str, packet['pre'])), command, context)
            inspect(packet, native, binary=self.counter, history=original_history, replay_key=key)
            independent = self.counter_submit(replay, key, command, context)
            if DEFECT == 'substituted-commit-plan':
                independent['commit_plan'] = '00'
            self.assertEqual(packet, independent)
        before = invoke(self.counter, 'state', db)
        retry = self.counter_submit(db, '1'*64, 120)
        if DEFECT == 'retry-latest-receipt':
            retry = failed
        self.assertEqual(retry, accepted)
        self.assertEqual(invoke(self.counter, 'state', db), before)
        self.assertEqual(invoke(self.counter, 'history', db), {'entries': 2, 'commits': 2})
        with self.assertRaises(ValueError):
            self.counter_submit(db, '1'*64, 121)
        self.assertEqual(invoke(self.counter, 'state', db), before)

    def test_adopter_refuses_missing_or_substituted_artifacts(self):
        db = self.root / 'counter.db'
        invoke(self.counter, 'init', db)
        original_history = ()
        for key, command in (('1'*64, 120), ('2'*64, 121)):
            packet = self.counter_submit(db, key, command)
            original_history += ((key, (command,), (1,)),)
            native = invoke(self.counter, 'probe', ' '.join(map(str, packet['pre'])), command, 1)
            for field in ('patch', 'commit_plan', 'outbox', 'receipt', 'authorization', 'deliveries'):
                for replacement in (None, '00'):
                    bad = copy.deepcopy(packet)
                    bad[field] = replacement
                    if DEFECT == 'adopter-skips-artifact-check':
                        bad = packet
                    with self.assertRaises(ValueError, msg=field):
                        inspect(bad, native, binary=self.counter, history=original_history, replay_key=key)

    def test_process_kill_before_commit_is_atomic(self):
        db = self.root / 'counter.db'
        marker = self.root / 'decision-ready'
        invoke(self.counter, 'init', db)
        before = invoke(self.counter, 'state', db)
        env = dict(os.environ, SPOT_PAUSE_BEFORE_COMMIT=str(marker))
        child = subprocess.Popen([str(self.counter), 'keyed', str(db), '1'*64, '121', '1'],
                                 env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            deadline = time.monotonic()+30
            while not marker.exists() and child.poll() is None and time.monotonic() < deadline:
                time.sleep(.01)
            self.assertTrue(marker.exists(), 'native process never reached post-decision barrier')
            child.kill()
            child.communicate(timeout=10)
        finally:
            if child.poll() is None:
                child.kill()
                child.communicate(timeout=10)
        after = invoke(self.counter, 'state', db)
        if DEFECT == 'half-applied-state':
            after['commits'] += 1
        self.assertEqual(after, before)
        with sqlite3.connect(db) as connection:
            self.assertEqual(connection.execute('SELECT count(*) FROM v2_commits').fetchone()[0], 0)
            self.assertEqual(connection.execute('SELECT count(*) FROM v2_deliveries').fetchone()[0], 0)
        result = self.counter_submit(db, '1'*64, 121)
        self.assertEqual(result['class'], 'CommittedFailure')
        self.assertEqual(self.counter_submit(db, '1'*64, 121), result)


if __name__ == '__main__':
    unittest.main()
