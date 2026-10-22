#!/usr/bin/env python3
"""Native metadata, liveness and exact-observation regression checks.

Run with an explicitly selected local CLI. These probes grant no publication
capability. Hostile policies are written only under the selected output path.
"""
from __future__ import annotations
import argparse
import copy
import hashlib
import itertools
import json
import os
import subprocess
import unittest
from pathlib import Path

from build_contracts import APP, REASONS, source_hash
from native_check import command_values, parse_observation, probe, state_values
from run_native_checks import seed
from spot_reference import Command, K, Pool, U, World, balance_step, limbs, step


class NativeChecks(unittest.TestCase):
    def call(self, app, world, command, label):
        out = self.out / label
        out.mkdir(parents=True, exist_ok=True)
        target = out / 'probe.json'
        argv = [str(self.cli), '--no-kb', 'contract', 'probe', 'explain', str(app),
                '--input', ' '.join(map(str, state_values(world) + command_values(command) + (1,))),
                '--out', str(target)]
        result = subprocess.run(argv, text=True, capture_output=True, timeout=20, check=False)
        (out / 'command.json').write_text(json.dumps({'argv': argv, 'exit_code': result.returncode,
            'stdout': result.stdout, 'stderr': result.stderr}, indent=2) + '\n')
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(target.read_text())['result']['authority']

    def mutant(self, label, change, *, remove_law=None):
        target = self.out / label
        (target / 'v2').mkdir(parents=True, exist_ok=True)
        source = (APP / 'project.zeno').read_text()
        policy = json.loads((APP / 'v2/policy.json').read_text())
        change(policy)
        if remove_law is not None:
            source = '\n'.join(line for line in source.splitlines() if not line.startswith(f'law {remove_law} ')) + '\n'
            policy['law_kinds'].pop(str(remove_law))
            policy.get('delivery_laws', {}).pop(str(remove_law), None)
        (target / 'project.zeno').write_text(source)
        (target / 'v2/policy.json').write_text(json.dumps(policy, indent=2) + '\n')
        return target

    def test_fee_event_law_refuses_fee_zero_100(self):
        remove = 537 if os.environ.get('SPOT_NATIVE_DEFECT') == 'remove-fee-event-law' else None
        app = self.mutant('fee-zero-100', lambda p: p['variables'].__setitem__('fee', '0'), remove_law=remove)
        authority = self.call(app, seed(1001, 1001, 1001), Command(1, a=100), 'fee-zero-result')
        self.assertEqual((authority.get('class'), authority.get('law')), ('Refused', 537), authority)

    def test_fee_event_all_fields_and_presence_are_constrained(self):
        for field in range(3010, 3018):
            def change(policy):
                for case in policy['cases']:
                    for delivery in case['outbox']:
                        if delivery['channel'] == 303:
                            delivery['payload'][str(field)] = '(' + delivery['payload'][str(field)] + ') + 1'
            app = self.mutant(f'wrong-field-{field}', change)
            authority = self.call(app, seed(), Command(1, a=100), f'field-{field}-result')
            self.assertEqual((authority.get('class'), authority.get('law')), ('Refused', 537), authority)
        for mode in ('missing', 'duplicate'):
            def change(policy):
                for case in policy['cases']:
                    # Modify swap plans only; retain the five-delivery source cap.
                    if any('gross' in str(d['payload']) for d in case['outbox']):
                        if mode == 'missing':
                            case['outbox'] = [d for d in case['outbox'] if d['channel'] != 303]
                        else:
                            extra = copy.deepcopy(next(d for d in case['outbox'] if d['channel'] == 303))
                            extra['ordinal'] = len(case['outbox'])
                            extra['idempotency_ordinal'] = len(case['outbox'])
                            case['outbox'].append(extra)
            app = self.mutant(f'{mode}-event', change)
            authority = self.call(app, seed(), Command(1, a=100), f'{mode}-result')
            self.assertEqual((authority.get('class'), authority.get('law')), ('Refused', 537), authority)

    def test_reject_everything_fails_declared_law_set(self):
        def change(policy):
            for case in policy['cases']:
                if case['class'] == 'Accept':
                    case.update({'class': 'Reject', 'reason': REASONS['DOMAIN'], 'post': {}, 'outbox': []})
        remove = 538 if os.environ.get('SPOT_NATIVE_DEFECT') == 'remove-acceptance-law' else None
        app = self.mutant('reject-everything', change, remove_law=remove)
        w, c = World(None, (0, 0, 0, 0, 1001, 1001, 0, 0)), Command(0, a=1001, b=1001, sqrt=1001)
        probe(w, c, step(w, c), self.out, 'legitimate-creation', cli=self.cli)
        authority = self.call(app, w, c, 'reject-everything-result')
        self.assertEqual((authority.get('class'), authority.get('law')), ('Refused', 538), authority)

    def test_all_54_known_accepts_remain_accepted(self):
        rows = []
        for exists, a, b, sqrt in itertools.product((0, 1), (1000, 1001), (1000, 1001), (1000, 1001)):
            w = seed(1001, 1001, 1001) if exists else World(None, (0, 0, 0, 0, 10**12, 10**12, 10**12, 10**12))
            c = Command(0, a=a, b=b, sqrt=sqrt)
            if step(w, c).reason is None: rows.append((w, c))
        for action in (1, 2):
            group = []
            for x, y, direction, maximum in itertools.product(range(1, 5), range(1, 5), (0, 1), (1, 2, 3) if action == 2 else (0,)):
                w, c = seed(x, y, K), Command(action, direction=direction, a=1 if action == 2 else 2, limit0=maximum)
                r = step(w, c)
                if r.reason is None and max(r.post.pool.r0, r.post.pool.r1) <= 4: group.append((w, c))
            self.assertEqual(len(group), 10 if action == 1 else 16)
            rows.extend(group)
        group = []
        for supply, action, amount, lo0, lo1 in itertools.product((1000, 1001, 1002), (3, 4), (1, 2), (0, 1), (0, 1)):
            w = seed(supply, supply, supply)
            c = Command(action, a=amount, b=amount if action == 3 else 0, limit0=lo0, limit1=lo1)
            r = step(w, c)
            if r.reason is None and max(r.post.pool.r0, r.post.pool.r1, r.post.pool.supply) <= 1002: group.append((w, c))
        self.assertEqual(len(group), 24)
        rows.extend(group)
        self.assertEqual(len(rows), 51)
        for i, (w, c) in enumerate(rows):
            probe(w, c, step(w, c), self.out / 'known-accepts', f'complete-{i:02}', cli=self.cli)
        count = 0
        for balance, debit, credit, expected in itertools.product((U - 1, U), (0, 1), (0, 1, 2), (0, 1, 2)):
            reason, post = balance_step(balance, debit, credit, True, expected)
            if reason is not None: continue
            label = f'balance-{count}'
            values = limbs(balance) + limbs(debit) + limbs(credit) + (1, expected)
            target = self.out / 'known-accepts' / (label + '.json')
            argv = [str(self.cli), '--no-kb', 'contract', 'probe', 'explain', str(APP / 'components/balance'),
                    '--input', ' '.join(map(str, values)), '--out', str(target)]
            run = subprocess.run(argv, capture_output=True, text=True, timeout=20, check=False)
            (target.with_suffix('.command.json')).write_text(json.dumps({'argv': argv, 'exit_code': run.returncode,
                'stdout': run.stdout, 'stderr': run.stderr}, indent=2) + '\n')
            self.assertEqual(run.returncode, 0, run.stderr)
            actual = json.loads(target.read_text())['result']['authority']
            self.assertEqual(parse_observation(actual['example']), ('accept', None, limbs(post), ()))
            count += 1
        self.assertEqual(count, 3)
        (self.out / 'known-accepts/summary.json').write_text(json.dumps({'complete': 51, 'balance': 3,
            'matched': 54, 'mismatches': 0, 'qualification': 'same accepted tuples, complete-profile funded embedding for 51 Pool cases'}, indent=2) + '\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--test', choices=tuple(n for n in dir(NativeChecks) if n.startswith('test_')))
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    NativeChecks.cli, NativeChecks.out = args.cli.resolve(), args.out.resolve()
    identity = hashlib.sha256(args.cli.read_bytes()).hexdigest()
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(NativeChecks) if args.test is None else unittest.TestSuite([NativeChecks(args.test)])
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    if hashlib.sha256(args.cli.read_bytes()).hexdigest() != identity: raise ValueError('CLI identity changed')
    (args.out / 'result.json').write_text(json.dumps({'tests': result.testsRun, 'failures': len(result.failures),
        'errors': len(result.errors), 'cli_sha256': identity, 'sources': source_hash(APP), 'authority': 'none'}, indent=2) + '\n')
    return 0 if result.wasSuccessful() else 1


if __name__ == '__main__':
    raise SystemExit(main())
