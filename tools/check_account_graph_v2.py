#!/usr/bin/env python3
"""Native account rule-data controls after removal of the specialized proof.

This checks retained independent composition oracles against changed declarative
subjects. It does not establish a universal correspondence theorem. Generic core
proof, opacity and resource controls remain in their existing mandatory gates.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
TEMPLATE = Path('crates/zeno-fcis-cli/templates/account-lockout')
SUBJECT = TEMPLATE / 'src/v2_contract.rs'
LEDGER = Path('verification/verus/account-stage-migration.json')
# What `zeno-fcis generate contract` reads; it writes src/v2_contract.rs.
CONTRACT_INPUTS = ('project.zeno', 'v2/policy.json', 'v2/schema.zcve', 'v2/schema-origin.json')


def once(text, before, after):
    if text.count(before) != 1:
        raise ValueError(f'expected unique account declaration anchor: {before!r}')
    return text.replace(before, after, 1)


def generated_source(root, policy):
    """The account contract the CLI generates for a changed rules file."""
    with tempfile.TemporaryDirectory(prefix='zeno-fcis-account-') as directory:
        app = Path(directory)
        for name in CONTRACT_INPUTS:
            (app / name).parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(root / TEMPLATE / name, app / name)
        (app / 'v2/policy.json').write_text(json.dumps(policy, indent=2) + '\n')
        command = ['cargo', '+1.97.1', 'run', '--locked', '--offline', '-q', '-p', 'zeno-fcis-cli', '--',
                   'generate', 'contract', str(app)]
        subprocess.run(command, cwd=root, check=True, capture_output=True, text=True)
        return (app / 'src/v2_contract.rs').read_text()


def mutations(root=ROOT):
    original = (root / SUBJECT).read_text()
    policy = json.loads((root / TEMPLATE / 'v2/policy.json').read_text())
    rows = {}
    for name, case, before, after in [
        ('lose_original_seen', 0, 'now < seen', 'now < now'),
        ('account_backwards_fact', 0, 'now < seen', 'now < until'),
        ('lock_equality_is_locked', 1, 'now < until', 'now <= until'),
        ('early_third_failure', 4, 'failed == 2', 'failed == 1'),
    ]:
        changed = json.loads(json.dumps(policy))
        changed['cases'][case]['when'] = once(changed['cases'][case]['when'], before, after)
        rows[name] = generated_source(root, changed)
    for name, expression in [('short_lock', 'now + 899'), ('wrong_arithmetic', 'now - 900')]:
        changed = json.loads(json.dumps(policy))
        changed['variables']['computed.deadline'] = expression
        rows[name] = generated_source(root, changed)
    rows['cap_legal_timestamp'] = once(original,
        'id: 130,\n                    leaf: InputLeaf::I128 {\n                        min: 0,\n                        max: 4102444800,',
        'id: 130,\n                    leaf: InputLeaf::I128 {\n                        min: 0,\n                        max: 1000,')
    rows['wrong_original_field'] = once(original,
        'id: 112,\n                    leaf: InputLeaf::I128 {',
        'id: 111,\n                    leaf: InputLeaf::I128 {')
    rows['widen_command_domain'] = once(original,
        'InputVariant { id: 122, code: 122 },',
        'InputVariant { id: 122, code: 122 }, InputVariant { id: 123, code: 123 },')
    rows['widen_command_domain'] = once(rows['widen_command_domain'],
        'type_id: 101,\n                min: 120,\n                max: 122,',
        'type_id: 101,\n                min: 120,\n                max: 123,')
    rows['account_full_deadline'] = once(original, 'roots: &[32, 34, 35],', 'roots: &[32, 35, 35],')
    if any(value == original for value in rows.values()):
        raise ValueError('account mutation left its subject unchanged')
    return rows


def source_files(root):
    paths = subprocess.check_output(['git', 'ls-files', '-co', '--exclude-standard', '-z'], cwd=root).decode().split('\0')
    return sorted({n for n in paths if n and (root/n).is_file()})


def snapshot(root, paths):
    result = {}
    for name in paths:
        path = root/name
        if path.is_symlink():
            raise ValueError(f'symlink in account specimen: {name}')
        result[name] = hashlib.sha256(path.read_bytes()).hexdigest()
    return result


def check(out):
    out.mkdir(parents=True, exist_ok=False)
    paths = source_files(ROOT)
    before = snapshot(ROOT, paths)
    (out/'source-before.json').write_text(json.dumps(before, indent=2, sort_keys=True)+'\n')
    env = {**os.environ, 'CARGO_NET_OFFLINE': 'true', 'CARGO_INCREMENTAL': '0', 'CARGO_PROFILE_DEV_DEBUG': '0', 'CARGO_PROFILE_TEST_DEBUG': '0',
           'CARGO_TARGET_DIR': str(out.resolve()/'positive'/'native-target')}
    env.pop('RUSTFLAGS', None)
    env.pop('CARGO_ENCODED_RUSTFLAGS', None)
    command = ['cargo', '+1.97.1', 'test', '--locked', '--offline', '-vv', '-p', 'zeno-fcis-synthesis',
               '--test', 'v2_abstraction', '--test', 'v2_composition']
    def run(cwd, directory):
        directory.mkdir(parents=True, exist_ok=True)
        start = time.monotonic()
        if (directory/'native-target').exists():
            raise ValueError('account specimen target must be fresh')
        local_env = {**env, 'CARGO_TARGET_DIR': str(directory.resolve()/'native-target')}
        proc = subprocess.run(command, cwd=cwd, env=local_env, capture_output=True, text=True, timeout=600)
        (directory/'native.stdout').write_text(proc.stdout)
        (directory/'native.stderr').write_text(proc.stderr)
        row = {'exit': proc.returncode, 'seconds': time.monotonic()-start, 'command': command, 'cwd': str(cwd), 'target': local_env['CARGO_TARGET_DIR']}
        (directory/'result.json').write_text(json.dumps(row, indent=2)+'\n')
        # This target was freshly created only for this specimen; preserve source/logs.
        shutil.rmtree(local_env['CARGO_TARGET_DIR'], ignore_errors=False)
        return proc
    positive = run(ROOT, out/'positive')
    if positive.returncode or not re.search(r'test result: ok\. [1-9][0-9]* passed', positive.stdout):
        raise RuntimeError('positive account oracle suite failed or ran no tests')
    outcomes = []
    for name, changed in mutations().items():
        specimen = out/'specimens'/name
        for path in paths:
            target = specimen/path
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT/path, target)
        (specimen/SUBJECT).write_text(changed)
        (out/name).mkdir(parents=True)
        (out/name/'source.json').write_text(json.dumps(snapshot(specimen, paths), indent=2, sort_keys=True)+'\n')
        proc = run(specimen, out/name)
        intended = re.search(r'^---- (?:all_reviewed_examples_bind_actual_value_bytes_without_command_reencoding|account_full_boundaries_14580_and_all_20_retained_examples|canonical_values_outside_account_schema_are_not_normalized) stdout ----$', proc.stdout, re.M)
        killed = proc.returncode == 101 and 'test result: FAILED.' in proc.stdout and bool(intended)
        row = {'name': name, 'killed': killed, 'expected': 'independent native rule-oracle assertion',
               'mutated_sha256': hashlib.sha256(changed.encode()).hexdigest(), 'exit': proc.returncode}
        outcomes.append(row)
        (out/'controls.json').write_text(json.dumps(outcomes, indent=2)+'\n')
        if not killed:
            raise RuntimeError(f'{name} survived or failed outside the intended runtime oracle')
    after = snapshot(ROOT, paths)
    if after != before or source_files(ROOT) != paths:
        raise RuntimeError('account gate source changed during replay')
    result = {'status': 'bounded_account_data_controls_passed', 'controls': outcomes,
              'universal_account_correspondence_theorem': False, 'source_stable': True}
    (out/'result.json').write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps(result))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', required=True, type=Path)
    check(parser.parse_args().out)
