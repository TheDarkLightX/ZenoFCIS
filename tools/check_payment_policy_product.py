#!/usr/bin/env python3
"""Independent small-K payment model, exact Rust trace comparison and controls.

The reference models the delivery controller from its protocol rules. It imports
no Rust-generated transition tables, simulator, policy checker or verdict data.
Only the wire-format version and explicit fixture/response domain are shared.
"""
from __future__ import annotations

import argparse
from collections import Counter
from dataclasses import dataclass
import gzip
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def u64(n):
    return n.to_bytes(8, 'big')


def node(tag, payload=b''):
    return bytes([tag]) + payload


def record(*items):
    return node(5, u64(len(items)) + b''.join(items))


def integer(n):
    return node(4, u64(n))


def hash_node(raw):
    return node(2, raw)


def byte_node(raw):
    return node(3, u64(len(raw)) + raw)


STORE_ID = bytes([8]) * 32
STORE = node(1, STORE_ID + (1).to_bytes(4, 'big'))
IDS = tuple(bytes([n]) * 32 for n in (1, 2))
HASHES = tuple(hashlib.sha256(bytes([n])).digest() for n in (1, 2))
INVOICES = tuple(record(hash_node(IDS[i]), integer(1), integer(1), integer(i),
                        byte_node(b'receiver'), byte_node(bytes([i+1])), hash_node(HASHES[i]))
                 for i in range(2))
EMPTY_STATE = u64(0).hex()


@dataclass(frozen=True)
class Profile:
    bound: int
    retries: int
    allowed: bool = True
    monitored: bool = True
    provider_idempotent: bool = False
    duplicate_copies: bool = False

    def line(self, schedule):
        return ' '.join(str(int(v)) for v in vars(self).values()) + ' ' + (''.join(map(str, schedule)) or '-') + '\n'


def operation(phase, current):
    if phase == 'read':
        return b'\x04' + record(STORE)
    if phase == 'send':
        return b'\x05' + record(STORE, INVOICES[current])
    if phase == 'ack':
        return b'\x06' + record(STORE, hash_node(IDS[current]), hash_node(HASHES[current]))
    if phase == 'clock':
        return b'\x08' + record()
    if phase == 'random':
        return b'\x09' + record(node(6, u64(0) + b'\0'), integer(1))
    return b'\x0a' + record(record(integer(1), integer(1)))


def check_actions(authorized, committed, refused, pending):
    """Precedence: unauthorized commit, duplicate commit, then silent drop."""
    # The independent acceptance rule counts occurrences, not distinct keys.
    counts = Counter(committed)
    if counts.keys() - set(authorized):
        return 'unauthorized commit'
    if any(n > 1 for n in counts.values()):
        return 'duplicate commit'
    if set(authorized) - counts.keys() - set(refused) - set(pending):
        return 'silent drop'
    return None


def model(p, schedule):
    if not 0 <= p.bound <= 12 or not 0 <= p.retries <= 2:
        return {'error': 'Profile'}
    phase, current, acked, retries, may_send = 'read', 0, 0, p.retries, False
    pending, refused, commits, reservations = [True, True], [False, False], [0, 0], ()
    trace, used, end, next_choice, outcome = [], 0, 2, 0, b''

    def finish(branch, value):
        if may_send and branch == 1:
            return bytes([3, 2])
        if may_send and branch == 2:
            return bytes([3, value])
        return bytes([branch]) + (u64(value) if branch == 0 else bytes([value]))

    for handled in range(p.bound + 1):
        if outcome:
            end = 0
            break
        if handled == p.bound:
            break
        op = operation(phase, current)
        verdicts, proposed, rejection = [], reservations, None
        if p.monitored:
            if phase == 'send' and current in reservations:
                verdicts, rejection = [24], 23
            else:
                verdicts = [0]
                if phase == 'send':
                    proposed = reservations + (current,)
                if phase == 'send' and not p.allowed:
                    verdicts.append(25)
                    rejection = 24
                    proposed = reservations
                else:
                    verdicts.append(0)
        arity = {'read': 6, 'send': 8 + int(p.duplicate_copies), 'ack': 9,
                 'clock': 4, 'random': 4, 'wait': 5}[phase]
        if rejection is not None:
            choice, response = 255, bytes([5, 3, rejection])
            refused[current] = True
            outcome = finish(1, rejection)
        else:
            if used == len(schedule):
                end, next_choice = 3, arity
                break
            choice = schedule[used]
            used += 1
            if not 0 <= choice < arity:
                return {'error': 'Schedule'}
            reservations = proposed
            response = b''
            if phase == 'read':
                if choice == 0:
                    items = [INVOICES[i] for i in range(2) if pending[i]]
                    response = b'\x04\0' + node(7, u64(len(items)) + b''.join(items))
                    phase = 'send'
                elif choice in (1, 2, 3):
                    response = bytes([4, 2, choice-1])
                    if retries:
                        retries -= 1
                    else:
                        outcome = finish(2, choice-1)
                elif choice == 4:
                    response = bytes([4, 1, 7])
                    outcome = finish(1, 7)
                else:
                    end = 1
            elif phase == 'send':
                if choice in (0, 3, 5, 7, 8):
                    copies = 2 if choice == 8 else 1
                    commits[current] = 1 if p.provider_idempotent else commits[current] + copies
                if choice in (0, 7, 8):
                    reported = bytes(32) if choice == 7 else IDS[current]
                    response = b'\x05\0' + record(hash_node(reported), hash_node(HASHES[current]))
                    may_send = True
                    if choice == 7:
                        outcome = finish(1, 21)
                    else:
                        phase = 'ack'
                elif choice in (1, 2, 3):
                    response = bytes([5, 1, 12]) if choice == 1 else bytes([5, 2, 2])
                    may_send = may_send or choice != 1
                    if retries:
                        retries -= 1
                        phase = 'clock'
                    else:
                        outcome = finish(1, 12) if choice == 1 else finish(3, 2)
                elif choice in (4, 5):
                    end = 1
                else:
                    refused[current] = True
                    response = bytes([5, 3, 12])
                    outcome = finish(1, 12)
            elif phase == 'ack':
                already = not pending[current]
                if choice in (0, 3, 5):
                    pending[current] = False
                if choice == 0:
                    response = bytes([6, 1, 15]) if already else b'\x06\0\0'
                    acked += 1
                    current += 1
                    if current == 2:
                        outcome = finish(0, acked)
                    else:
                        phase = 'send'
                elif choice in (4, 5):
                    end = 1
                elif choice == 8:
                    response = bytes([6, 1, 12])
                    outcome = finish(1, 12)
                else:
                    branch = 3 if choice in (2, 3) else 2
                    fault = {1: 0, 2: 2, 3: 2, 6: 1, 7: 2}[choice]
                    response = bytes([6, branch, fault])
                    if retries:
                        retries -= 1
                    else:
                        outcome = finish(branch, fault)
            elif phase in ('clock', 'random'):
                port = 8 if phase == 'clock' else 9
                if choice == 0:
                    response = bytes([port, 0]) + (record(integer(1), integer(0)) if phase == 'clock' else integer(0))
                    phase = 'random' if phase == 'clock' else 'wait'
                else:
                    response = bytes([port, 1, choice-1])
                    outcome = finish(2, choice-1)
            else:
                if choice == 0:
                    response = b'\x0a\0' + record(integer(1), integer(1))
                    phase = 'send'
                elif choice == 1:
                    response = b'\x0a\x02'
                    outcome = finish(1, 22)
                else:
                    response = bytes([10, 1, choice-2])
                    outcome = finish(2, choice-2)
        states = []
        if p.monitored:
            states = [(u64(len(reservations)) + b''.join(STORE_ID + IDS[i] for i in reservations)).hex(), EMPTY_STATE]
        trace.append(dict(operation=op.hex(), choice=choice, verdicts=verdicts,
                          response=response.hex(), states=states))
        if end == 1:
            break
    if used != len(schedule):
        return {'error': 'Schedule'}
    committed = [i for i, count in enumerate(commits) for _ in range(count)]
    violation = check_actions([0, 1], committed, [i for i in range(2) if refused[i]],
                             [i for i in range(2) if pending[i]])
    return dict(next=next_choice, end=end, outcome=outcome.hex(), trace=trace,
                commits=commits, pending=pending, refused=refused, violation=violation,
                replayed=end <= 1)


class Rust:
    def __init__(self, binary):
        self.process = subprocess.Popen([str(binary)], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, text=True, bufsize=1)
    def run(self, profile, schedule):
        self.process.stdin.write(profile.line(schedule))
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        if not line:
            raise RuntimeError(f'Rust runner stopped: {self.process.poll()}')
        return json.loads(line)
    def close(self):
        self.process.stdin.close()
        status = self.process.wait(timeout=30)
        if status:
            raise RuntimeError(f'Rust runner exit {status}')


def compare(binary, out, bound, max_nodes):
    runner = Rust(binary)
    rows, total, digests = [], 0, hashlib.sha256()
    evidence = out.with_suffix('.traces.jsonl.gz')
    try:
        with gzip.open(evidence, 'wt') as stream:
            for retries in range(3):
                for allowed, idem, duplicates in ((True, False, False), (False, False, False),
                                                  (True, True, True)):
                    profile = Profile(bound, retries, allowed, True, idem, duplicates)
                    pending, nodes, leaves, ends = [()], 0, 0, Counter()
                    while pending:
                        schedule = pending.pop()
                        nodes += 1
                        total += 1
                        if total > max_nodes:
                            out.with_suffix('.frontier.json').write_text(json.dumps(dict(
                                verdict='incomplete', profile=vars(profile), current=schedule,
                                frontier=pending, compared_nodes=total-1), indent=2)+'\n')
                            raise RuntimeError(f'INCOMPLETE: max nodes {max_nodes}; frontier retained in profile {vars(profile)} prefix {schedule}')
                        expected, actual = model(profile, schedule), runner.run(profile, schedule)
                        if expected != actual:
                            mismatch = dict(profile=vars(profile), schedule=schedule, expected=expected, actual=actual)
                            out.with_suffix('.mismatch.json').write_text(json.dumps(mismatch, indent=2)+'\n')
                            raise AssertionError(f'Rust/Python product mismatch at {schedule}')
                        if actual.get('violation'):
                            raise AssertionError(f'counterexample {vars(profile)} {schedule}: {actual["violation"]}')
                        row = dict(profile=vars(profile), schedule=schedule, result=actual)
                        encoded = json.dumps(row, sort_keys=True, separators=(',', ':'))+'\n'
                        digests.update(encoded.encode())
                        stream.write(encoded)
                        if actual['next']:
                            pending.extend(schedule+(choice,) for choice in reversed(range(actual['next'])))
                        else:
                            leaves += 1
                            ends[str(actual['end'])] += 1
                    row = dict(profile=vars(profile), nodes=nodes, schedules=leaves, endings=dict(ends), verdict='complete-through-K')
                    rows.append(row)
                    print(json.dumps(row), flush=True)
            witnesses = []
            for profile, schedule in ((Profile(6, 1, monitored=False), (0, 3, 0, 0, 0, 0)),
                                      (Profile(2, 0, duplicate_copies=True), (0, 8))):
                expected, actual = model(profile, schedule), runner.run(profile, schedule)
                if expected != actual or actual['violation'] != 'duplicate commit':
                    raise AssertionError('required negative environment/runtime witness missing')
                witnesses.append(dict(profile=vars(profile), schedule=schedule, result=actual))
            out.with_suffix('.witnesses.json').write_text(json.dumps(witnesses, indent=2)+'\n')
    finally:
        runner.close()
    identity = subprocess.check_output([str(binary), '--identity'], text=True).strip()
    result = dict(schema='zeno-fcis/payment-policy-comparison/1', source=identity, bound=bound,
                  profiles=rows, compared_nodes=total, trace_sha256=digests.hexdigest(),
                  trace_file=evidence.name, witnesses=out.with_suffix('.witnesses.json').name,
                  scope='complete finite prefix tree through K; request-bound leaves are incomplete actions; explicit environment assumptions only')
    out.write_text(json.dumps(result, indent=2)+'\n')
    print(f'PASS: {total} exact Rust/Python operation, verdict, state and response traces', flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--bound', type=int, default=6)
    parser.add_argument('--max-nodes', type=int, default=200000)
    args = parser.parse_args()
    if not 0 <= args.bound <= 12 or args.max_nodes < 1:
        parser.error('bound must be 0..12 and max-nodes must be positive')
    args.out.parent.mkdir(parents=True, exist_ok=True)
    compare(args.binary.resolve(), args.out, args.bound, args.max_nodes)


if __name__ == '__main__':
    main()
