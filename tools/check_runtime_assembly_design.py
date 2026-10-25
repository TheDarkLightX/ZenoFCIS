#!/usr/bin/env python3
"""Replay bounded native design checks; does not assemble or authorize an app."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
MODEL = ROOT / 'examples/runtime-assembly-design'
BOUND = 100352


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def invoke(cli, kind, directory, output):
    command = [str(cli), 'contract', kind, str(directory), '--max-tuples',
               str(BOUND), '--out', str(output), '--format', 'json']
    result = subprocess.run(command, capture_output=True, text=True, timeout=300)
    if not output.is_file():
        raise RuntimeError(f'{kind}: no native packet (exit {result.returncode})')
    packet = json.loads(output.read_text())
    if packet.get('authority') != 'none':
        raise RuntimeError('unexpected authority claim')
    return result.returncode, packet


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', required=True, type=Path,
                        help='Compatible native ZenoFCIS executable; identity is recorded')
    parser.add_argument('--out', type=Path, help='Write compact source-bound check results')
    args = parser.parse_args()
    cli = args.cli.resolve(strict=True)
    files = sorted(p for p in MODEL.rglob('*') if p.is_file())
    sources = {str(p.relative_to(ROOT)): digest(p) for p in files}
    result = {'schema': 'zeno-fcis/runtime-assembly-design-check/1',
              'authority': 'none', 'scope': 'finite design abstraction, not runtime implementation',
              'cli_sha256': digest(cli), 'checker_sha256': digest(Path(__file__)), 'sources': sources, 'baseline': {}, 'mutants': []}
    with tempfile.TemporaryDirectory(prefix='runtime-assembly-design-') as temp:
        temp = Path(temp)
        for name, count, scenario_count in [('assembly', 32, 5), ('effects', BOUND, 77)]:
            code, packet = invoke(cli, 'review', MODEL / name, temp / f'{name}-review.json')
            summary = packet.get('summary', {})
            if (code != 0 or summary.get('findings') != 0 or
                    summary.get('inputs') != {'construction': 'full-domain', 'count': count} or
                    summary.get('refusals', {}).get('count') != 0):
                raise RuntimeError(f'{name}: baseline review did not fully pass')
            code, scenarios = invoke(cli, 'scenario', MODEL / name, temp / f'{name}-scenarios.json')
            if (code != 0 or scenarios.get('status') != 'passed' or
                    scenarios.get('failed_on_contract') != 0 or scenarios.get('count') != scenario_count):
                raise RuntimeError(f'{name}: baseline scenarios failed')
            result['baseline'][name] = {'review': summary, 'scenario_count': scenarios['count'],
                                        'scenario_status': scenarios['status']}
        mutations = [('assembly', f'remove_{name}', field) for name, field in
                     [('supported_profile', 160), ('source_hash', 161),
                      ('schema', 162), ('capability', 163)]]
        mutations += [('effects', f'remove_{name}', field) for name, field in
                      [('durability', 160), ('artifact', 161), ('attempt', 162),
                       ('revision', 163), ('producer', 164), ('kind', 165),
                       ('target', 166), ('operation', 167), ('sink_deduplication', 168),
                       ('authorization', 169), ('definitive_negative', 170)]]
        mutations += [('effects', 'timeout_claims_failure', None),
                      ('effects', 'cancel_claims_rollback', None)]
        for name, label, field in mutations:
            directory = temp / label
            shutil.copytree(MODEL / name, directory)
            policy_file = directory / 'v2/policy.json'
            policy = json.loads(policy_file.read_text())
            changed = False
            for case in policy['cases']:
                if field is not None:
                    token = f' && context.102.{field}'
                    old = case['when']
                    case['when'] = old.replace(token, '')
                    changed |= old != case['when']
                elif (label == 'timeout_claims_failure' and case['rule'] == 'timeout_unknown'):
                    case['post']['110'] = 5
                    changed = True
                elif (label == 'cancel_claims_rollback' and case['rule'] == 'request_cancel'):
                    case['post']['110'] = 6
                    changed = True
            if not changed:
                raise RuntimeError(f'{label}: mutation did not change the policy')
            policy_file.write_text(json.dumps(policy, indent=2) + '\n')
            code, packet = invoke(cli, 'scenario', directory, temp / f'{label}.json')
            # A parser/build error is NOT a killed semantic mutant.
            if code != 1 or packet.get('status') != 'failed' or packet.get('failed_on_contract', 0) < 1:
                raise RuntimeError(f'{label}: expected a concrete scenario failure, not exit {code}')
            result['mutants'].append({'name': label, 'status': 'killed',
                                      'failed_scenarios': packet['failed_on_contract']})
    final_files = sorted(p for p in MODEL.rglob('*') if p.is_file())
    if sources != {str(p.relative_to(ROOT)): digest(p) for p in final_files}:
        raise RuntimeError('model inputs changed during checking')
    if result['cli_sha256'] != digest(cli) or result['checker_sha256'] != digest(Path(__file__)):
        raise RuntimeError('checker or executable changed during checking')
    result['status'] = 'passed'
    text = json.dumps(result, indent=2, sort_keys=True) + '\n'
    if args.out:
        args.out.write_text(text)
    else:
        print(text, end='')


if __name__ == '__main__':
    main()
