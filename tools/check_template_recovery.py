#!/usr/bin/env python3
"""Replay constructive finite recovery witnesses and a decreasing-state lemma.

These are chosen action sequences through the actual contract Authority.
They assume the stated operator is available and no intervening action adds
new work. They grant no scheduling, authentication or external-delivery claim.
"""
import argparse
import hashlib
import itertools
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def inventory(policy):
    """Every declared stock state, including the full/full state."""
    if (policy['leaf_bindings']['105'] != ['I128', 0, 5]
            or policy['leaf_bindings']['106'] != ['I128', 1, 3]):
        raise ValueError('unsupported inventory domain: recheck the recovery bound')
    witnesses = []
    lines = []
    for available, reserved in itertools.product(range(6), repeat=2):
        start = [available, reserved]
        steps = []
        while reserved:
            quantity = min(3, reserved)
            before = [available, reserved]
            reserved -= quantity
            steps.append({'before': before, 'command': [152, quantity],
                          'context': [1], 'after': [available, reserved]})
        if len(steps) > 2:
            raise AssertionError({'state': start, 'steps': steps})
        witnesses.append({'state': start, 'steps': steps})
        if steps:
            lines.append(f'# scenario ship-{start[0]}-{start[1]}')
            for step in steps:
                before = ' '.join(map(str, step['before']))
                after = ' '.join(map(str, step['after']))
                quantity = step['command'][1]
                lines.append(f'{before} 152 {quantity} 1 | accept - {after} | 300@0:0 {quantity}')
    return witnesses, lines, (
        'command.101.120 == 152 -> '
        'post.100.111 + command.101.121 == pre.100.111 && '
        'post.100.111 < pre.100.111'
    ), 864


def library(_policy):
    """Every holder assignment satisfying the existing at-most-two invariant."""
    witnesses = []
    lines = []
    for holders in itertools.product(range(4), repeat=3):
        if holders[0] != 0 and holders[0] == holders[1] == holders[2]:
            continue
        state = list(holders)
        steps = []
        for index, member in enumerate(holders):
            if member:
                before = state.copy()
                state[index] = 0
                steps.append({'before': before, 'command': [151, index + 1, member],
                              'context': [130], 'after': state.copy()})
        if any(state) or len(steps) > 3:
            raise AssertionError({'state': holders, 'steps': steps})
        witnesses.append({'state': list(holders), 'steps': steps})
        if steps:
            lines.append('# scenario return-' + '-'.join(map(str, holders)))
            for step in steps:
                before = ' '.join(map(str, step['before']))
                command = ' '.join(map(str, step['command']))
                after = ' '.join(map(str, step['after']))
                lines.append(f'{before} {command} 130 | accept - {after} | -')
    # Clearing the selected occupied copy while every other holder stays fixed
    # decreases the number of occupied copies by exactly one.
    cases = []
    for index, field in enumerate([110, 111, 112], 1):
        unchanged = ' && '.join(f'post.100.{other} == pre.100.{other}'
                                for other in [110, 111, 112] if other != field)
        cases.append(f'(command.101.121 == {index} -> pre.100.{field} != 0 && '
                     f'post.100.{field} == 0 && {unchanged})')
    return witnesses, lines, 'command.101.120 == 151 -> (' + ' && '.join(cases) + ')', 2304


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', required=True, type=Path)
    parser.add_argument('--template', required=True, choices=['inventory-reservation', 'library-loans'])
    parser.add_argument('--out', required=True, type=Path)
    args = parser.parse_args()
    contract = (ROOT / 'examples/apps/library-loans' if args.template == 'library-loans'
                else ROOT / 'crates/zeno-fcis-cli/templates/inventory-reservation')
    policy = json.loads((contract / 'v2/policy.json').read_text())
    witnesses, lines, formula, tuples = (library(policy) if args.template == 'library-loans'
                                         else inventory(policy))
    args.out.mkdir(parents=True, exist_ok=False)
    scenarios = args.out / 'recovery-scenarios.txt'
    scenarios.write_text('\n'.join(lines) + '\n')
    sources = {name: hashlib.sha256((contract / name).read_bytes()).hexdigest()
               for name in ['project.zeno', 'v2/policy.json']}
    results = []
    for name, command in [
        ('scenario', ['contract', 'scenario', str(contract), '--file', str(scenarios)]),
        ('lemma', ['contract', 'lemma', str(contract), '--transition', '--lemma', formula]),
    ]:
        packet = args.out / (name + '.json')
        command = ['nice', '-n', '19', 'ionice', '-c3', str(args.cli.resolve()),
                   '--no-kb', *command, '--out', str(packet), '--format', 'json']
        result = subprocess.run(command, capture_output=True, text=True)
        (args.out / (name + '.stdout')).write_text(result.stdout)
        (args.out / (name + '.stderr')).write_text(result.stderr)
        results.append({'command': command, 'exit': result.returncode})
        if result.returncode != 0:
            raise AssertionError(results[-1])
        checked = json.loads(packet.read_text())
        if checked.get('status') != ('passed' if name == 'scenario' else 'checked'):
            raise AssertionError(checked)
        if (checked['sources']['project_sha256'] != sources['project.zeno']
                or checked['sources']['rules_sha256'] != sources['v2/policy.json']):
            raise AssertionError('contract changed while checking recovery')
        if name == 'scenario':
            if checked['count'] != sum(bool(w['steps']) for w in witnesses):
                raise AssertionError('incomplete recovery scenario coverage')
        elif (len(checked['lemmas']) != 1
              or checked['lemmas'][0]['tuples'] != tuples
              or checked['lemmas'][0]['refused'] != 0
              or checked['lemmas'][0]['undefined'] != 0):
            raise AssertionError('incomplete declared-domain recovery lemma')
    if any(hashlib.sha256((contract / name).read_bytes()).hexdigest() != digest
           for name, digest in sources.items()):
        raise AssertionError('contract changed while checking recovery')
    summary = {'template': args.template, 'sources': sources,
               'states': len(witnesses), 'zero_step_states': sum(not w['steps'] for w in witnesses),
               'max_actions': max(len(w['steps']) for w in witnesses),
               'witnesses': witnesses, 'checks': results,
               'scope': 'chosen operator actions, without intervening additions; local requests only'}
    (args.out / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps({key: summary[key] for key in ['template', 'states', 'zero_step_states', 'max_actions']}))


if __name__ == '__main__':
    main()
