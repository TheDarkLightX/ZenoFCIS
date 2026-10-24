#!/usr/bin/env python3
"""Compare every K=6 payment prefix with the proved executable abstract model.

The existing independent payment reference first compares the production runner.
This differential then interprets every event using effect_protocol_runtime.rs,
whose admission is proved equal to the two-handler abstract specialization.
Receiver effects and the conditional physical-copy bound are compared separately
from admissions. Atomic receiver deduplication is an environment contract.
"""
from __future__ import annotations

import argparse
import gzip
import hashlib
import json
from pathlib import Path
import subprocess

import check_payment_policy_product as payment


class Model:
    def __init__(self, binary):
        self.process = subprocess.Popen([str(binary)], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, text=True, bufsize=1)

    def run(self, keys, *, key=0, send=False, allowed=True, enabled=True, retry=False,
            suspended=False, may=True, copies=0, capacity=2):
        values = [capacity, key, int(send), int(allowed), int(enabled), int(retry),
                  int(suspended), int(may), copies, len(keys), *keys]
        self.process.stdin.write(' '.join(map(str, values)) + '\n')
        self.process.stdin.flush()
        observed = self.process.stdout.readline()
        if not observed:
            raise RuntimeError(f'abstract runner stopped: {self.process.poll()}')
        return json.loads(observed)

    def close(self):
        self.process.stdin.close()
        code = self.process.wait(timeout=30)
        if code:
            raise RuntimeError(f'abstract runner exit {code}')


def state_bytes(keys):
    return (payment.u64(len(keys)) + b''.join(payment.STORE_ID + payment.IDS[k] for k in keys)).hex()


def event_input(event):
    raw = bytes.fromhex(event['operation'])
    if not raw:
        raise ValueError('empty operation')
    if raw[0] != 5:
        return False, 0
    for index in range(2):
        if raw == payment.operation('send', index):
            return True, index
    raise ValueError('Send outside the exact product fixture')


def compare_row(runner, row):
    profile, result = row['profile'], row['result']
    keys, physical, attempts = [], [0, 0], 0
    for event in result['trace']:
        send, key = event_input(event)
        copies = (2 if event['choice'] == 8 else 1) if send and event['choice'] in (0, 3, 5, 7, 8) else 0
        got = runner.run(keys, key=key, send=send, allowed=profile['allowed'],
                         enabled=profile['monitored'], retry=send and key in keys, copies=copies)
        if got['verdicts'] != event['verdicts']:
            raise AssertionError(f'admission differs: {row["schedule"]} {event} {got}')
        expected_states = [state_bytes(got['reserved']), payment.EMPTY_STATE] if profile['monitored'] else []
        if expected_states != event['states']:
            raise AssertionError(f'reservation state differs: {row["schedule"]} {event} {got}')
        if got['copies'] != copies:
            raise AssertionError('abstract copy accounting differs')
        keys = got['reserved']
        if send:
            physical[key] += got['copies']
        attempts += 1
    expected_commits = [min(1, n) if profile['provider_idempotent'] else n for n in physical]
    if expected_commits != result['commits']:
        raise AssertionError(f'effect accounting differs: {row["schedule"]} {expected_commits}')
    return attempts, max(physical) > 1


def supplementary_traces(runner):
    suspended = runner.run([], key=0, send=True, suspended=True, may=False)
    assert suspended == dict(reserved=[], verdicts=[0, 0], copies=0)
    retry = runner.run(suspended['reserved'], key=0, send=True, retry=True, copies=1)
    assert retry == dict(reserved=[0], verdicts=[0, 0], copies=1)
    partial = runner.run([], key=0, send=True, suspended=True, may=True, copies=1)
    assert partial == dict(reserved=[0], verdicts=[0, 0], copies=1)
    blocked = runner.run(partial['reserved'], key=0, send=True, retry=True)
    assert blocked == dict(reserved=[0], verdicts=[24], copies=0)
    veto = runner.run([0], key=1, send=True, allowed=False)
    assert veto == dict(reserved=[0], verdicts=[0, 25], copies=0)
    return dict(suspend=suspended, retry=retry, composed_suspend=partial,
                blocked_retry=blocked, later_veto=veto)


def check(payment_binary: Path, model_binary: Path, out: Path):
    out.parent.mkdir(parents=True, exist_ok=True)
    product_path = out.with_name('payment-product.json')
    payment.compare(payment_binary.resolve(), product_path, 6, 200000)
    product = json.loads(product_path.read_text())
    trace_path = product_path.with_suffix('.traces.jsonl.gz')
    runner = Model(model_binary.resolve())
    rows = events = outside_copy_premise = 0
    digest = hashlib.sha256()
    try:
        with gzip.open(trace_path, 'rt') as stream:
            for line in stream:
                row = json.loads(line)
                checked, duplicate = compare_row(runner, row)
                rows += 1
                events += checked
                outside_copy_premise += duplicate
                digest.update(line.encode())
        witnesses = json.loads(product_path.with_suffix('.witnesses.json').read_text())
        for row in witnesses:
            compare_row(runner, row)
        supplementary = supplementary_traces(runner)
    finally:
        runner.close()
    if rows != product['compared_nodes'] or digest.hexdigest() != product['trace_sha256']:
        raise AssertionError('reference trace inventory changed')
    receipt = dict(schema='zeno-fcis/effect-protocol-traces/1', status='passed', bound=6,
                   production_source=product['source'], traces=rows, events=events,
                   traces_with_multiple_physical_copies=outside_copy_premise,
                   trace_sha256=digest.hexdigest(), negative_reference_witnesses=len(witnesses),
                   model_binary_sha256=hashlib.sha256(model_binary.read_bytes()).hexdigest(),
                   supplementary=supplementary,
                   scope='All reference prefixes have the same admissions and final reservation states; '
                         'duplicate physical copies are permitted model traces outside physical-copy E1 premises.')
    out.write_text(json.dumps(receipt, indent=2) + '\n')
    print(f'PASS: {rows} prefixes, {events} exact admission/state steps')
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--payment-binary', type=Path, required=True)
    parser.add_argument('--model-binary', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    check(args.payment_binary, args.model_binary, args.out)


if __name__ == '__main__':
    main()
