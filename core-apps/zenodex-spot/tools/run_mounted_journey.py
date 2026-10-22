#!/usr/bin/env python3
"""Independent J-SPOT runner; wire and TCP results are distinguished explicitly."""
from __future__ import annotations

import argparse
import dataclasses
import http.client
import json
import os
from pathlib import Path
import secrets
import subprocess
import sys

from native_check import expected_observation
from make_test_fixture import LABEL
from spot_http import Binding, Identity, Mount, local_http
from spot_reference import Command, Pool, World, command_values, state_values, step

LEGITIMATE_STEPS = ('CreatePool', 'SwapExactIn', 'SwapExactOut', 'AddLiquidity', 'Burn')


def journey_summary(rows, transport, fixture=False):
    legitimate = [r for r in rows if r['legitimate']]
    accepted = sum(r['response']['class'] == 'Accept' for r in legitimate)
    complete = len(legitimate) == 5 and tuple(r['name'] for r in legitimate) == LEGITIMATE_STEPS
    return {'transport': transport, 'real_tcp': transport == 'tcp', 'steps': len(rows),
            'legitimate_accepted': accepted, 'legitimate_required': 5,
            'status': 'PASS' if complete and accepted == 5 else 'UNFINISHED-legitimate-acceptance',
            'fixture_label': LABEL if fixture else None, 'actor_spoof_refused': True}


def invoke(binary, *arguments):
    r = subprocess.run([str(binary), *map(str, arguments)], capture_output=True, text=True,
                       check=False, timeout=35)
    if r.returncode:
        raise ValueError('native command refused: ' + r.stderr)
    return json.loads(r.stdout)


def world(numbers):
    if len(numbers) != 40:
        raise ValueError('complete Spot snapshot required')
    balances = tuple(sum(numbers[8+4*i+j] << (32*j) for j in range(4)) for i in range(8))
    pool = Pool(*numbers[:3], numbers[4]) if numbers[3] else None
    return World(pool, balances, tuple(numbers[5:8]))


def compare(packet, expected, native):
    actual = ('accept' if packet['class'] == 'Accept' else 'reject', packet['reason'],
              tuple(packet['post']), tuple((ch, tuple(p)) for ch, p in packet['deliveries']))
    if actual != expected_observation(expected):
        raise ValueError('independent economic decision mismatch')
    for key in ('class', 'reason', 'pre', 'post', 'deliveries', 'authorization', 'patch'):
        if packet[key] != native[key]:
            raise ValueError('native decision mismatch: ' + key)


class Journey:
    def __init__(self, binary, database, out, transport='wire'):
        self.binary, self.database, self.out = binary, database, out
        self.transport = transport
        self.token = secrets.token_hex(24)
        self.binding = Binding((Identity('alice', 0, self.token), Identity('bob', 1, secrets.token_hex(24))))
        self.mount = Mount(binary, database, self.binding)
        self.server = None
        self.rows = []

    def start(self):
        if self.transport != 'tcp':
            return
        identities = self.out / 'test-identities.json'
        identities.write_text(json.dumps([dataclasses.asdict(i) for i in self.binding.identities]))
        identities.chmod(0o600)
        self.server = subprocess.Popen([sys.executable, str(Path(__file__).with_name('spot_http.py')),
            '--binary', str(self.binary), '--database', str(self.database), '--identities', str(identities)],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        line = self.server.stdout.readline()
        if not line:
            _, error = self.server.communicate(timeout=5)
            raise RuntimeError('HTTP server unavailable: ' + error)
        self.address = json.loads(line)['listening']

    def stop(self):
        if self.server is not None:
            self.server.terminate()
            try:
                self.server.communicate(timeout=5)
            except subprocess.TimeoutExpired:
                self.server.kill()
                self.server.communicate(timeout=5)
            self.server = None

    def post(self, body, key, token=None):
        token = token or self.token
        data = json.dumps(body, separators=(',', ':')).encode()
        if self.transport == 'wire':
            raw = (f'POST /commands HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {token}\r\n'
                   f'Idempotency-Key: {key}\r\nContent-Length: {len(data)}\r\n\r\n').encode()+data
            answer = local_http(self.mount, raw)
            head, data = answer.split(b'\r\n\r\n', 1)
            return int(head.split()[1]), json.loads(data)
        connection = http.client.HTTPConnection(*self.address, timeout=35)
        try:
            connection.request('POST', '/commands', data, {'Authorization': 'Bearer '+token, 'Idempotency-Key': key})
            response = connection.getresponse()
            return response.status, json.loads(response.read())
        finally:
            connection.close()

    def command(self, name, command, legitimate=False):
        before = invoke(self.binary, 'state', self.database)
        pre = world(before['state'])
        body = dataclasses.asdict(command)
        del body['actor']
        body['recipient'] = self.binding.identities[command.recipient].name
        token = self.binding.identities[command.actor].token
        key = f'{len(self.rows)+1:064x}'
        status, packet = self.post(body, key, token)
        if status != 200:
            raise ValueError(f'{name}: HTTP {status}: {packet}')
        expected = step(pre, command)
        native = invoke(self.binary, 'probe', ' '.join(map(str, state_values(pre))),
                        ' '.join(map(str, command_values(command))), '1')
        compare(packet, expected, native)
        after = invoke(self.binary, 'state', self.database)
        retry_status, retried = self.post(body, key, token)
        if retry_status != 200 or retried != packet or invoke(self.binary, 'state', self.database) != after:
            raise ValueError(name + ': retry differs or double-applied')
        row = {'name': name, 'legitimate': legitimate, 'request': body, 'retry_key': key,
               'http_status': status, 'response': packet, 'retry_equal': True,
               'native_equal': True, 'oracle_equal': True, 'head_before': before, 'head_after': after}
        self.rows.append(row)
        (self.out / 'J-SPOT.jsonl').write_text(''.join(json.dumps(r, sort_keys=True)+'\n' for r in self.rows))
        return row


def run(binary, database, out, transport, fixture=False):
    out.mkdir(parents=True, exist_ok=True)
    invoke(binary, 'init', database)
    journey = Journey(binary, database, out, transport)
    try:
        journey.start()
        steps = [('CreatePool', Command(0, a=1000000, b=1000000, sqrt=1000000)),
                 ('SwapExactIn', Command(1, a=10000)),
                 ('SwapExactOut', Command(2, a=10000, limit0=3000000000)),
                 ('AddLiquidity', Command(3, a=10000, b=10000)),
                 ('Burn', Command(4, a=100))]
        for index, (name, command) in enumerate(steps):
            journey.command(name, command, True)
            if index == 1:
                # A fresh server and fresh native process continue on the same DB.
                before_restart = invoke(binary, 'state', database)
                first = journey.rows[0]
                first_body = first['request']
                journey.stop()
                journey.start()
                if invoke(binary, 'state', database) != before_restart:
                    raise ValueError('restart changed state, commit count or outbox')
                status, retried = journey.post(first_body, first['retry_key'])
                if status != 200 or retried != first['response']:
                    raise ValueError('restart lost the original CreatePool receipt')
            journey.command(name+'-domain-reject', dataclasses.replace(command, a=0))
        status, packet = journey.post({'action': 1, 'a': 10, 'actor': 1}, 'f'*64)
        if status != 400:
            raise ValueError('body-supplied actor accepted')
        result = journey_summary(journey.rows, transport, fixture)
        result['restart_original_receipt_equal'] = journey.rows[0]['response']['receipt'] is not None
    except Exception as error:
        result = {'transport': transport, 'status': 'UNFINISHED', 'error': str(error),
                  'fixture_label': LABEL if fixture else None}
    finally:
        journey.stop()
        # Credentials are temporary acquisition inputs, never evidence attachments.
        (out / 'test-identities.json').unlink(missing_ok=True)
    (out / 'summary.json').write_text(json.dumps(result, indent=2)+'\n')
    return result


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', type=Path, required=True)
    p.add_argument('--database', type=Path, required=True)
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--transport', choices=('wire', 'tcp'), default='tcp')
    p.add_argument('--test-fixture', action='store_true', help=LABEL)
    a = p.parse_args()
    result = run(a.binary.resolve(), a.database.resolve(), a.out.resolve(), a.transport, a.test_fixture)
    print(json.dumps(result))
    raise SystemExit(0 if result['status'] == 'PASS' else 1)
