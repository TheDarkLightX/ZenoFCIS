#!/usr/bin/env python3
"""Partition complete compiled Miri test inventories without dropping cases."""
from __future__ import annotations

from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def logical_row(row: dict) -> dict:
    return {key: value for key, value in row.items() if key not in {'part', 'parts'}}


def logical_rows(rows: list[dict]) -> list[dict]:
    """Require every declared part exactly once and identical original selectors."""
    grouped: dict[str, list[dict]] = {}
    for row in rows:
        grouped.setdefault(row['group'], []).append(row)
    result = []
    for group, entries in grouped.items():
        first = entries[0]
        if not any('part' in r or 'parts' in r for r in entries):
            if len(entries) != 1:
                raise ValueError(f'{group}: duplicate unpartitioned group')
        else:
            count = first.get('parts')
            if type(count) is not int or not 2 <= count <= 16:
                raise ValueError(f'{group}: invalid part count')
            if any(type(r.get('part')) is not int or type(r.get('parts')) is not int
                   or r['parts'] != count for r in entries):
                raise ValueError(f'{group}: malformed part')
            if Counter(r['part'] for r in entries) != Counter(range(1, count + 1)):
                raise ValueError(f'{group}: missing, repeated or extra part')
            if any(logical_row(r) != logical_row(first) for r in entries):
                raise ValueError(f'{group}: part changes the original test selection')
            if first.get('command', 'test') != 'test' or 'test' in first:
                raise ValueError(f'{group}: partition only a test remainder')
        result.append(logical_row(first))
    return result


def partition(names: list[str], count: int) -> list[list[str]]:
    if type(count) is not int or not 2 <= count <= 16:
        raise ValueError('invalid part count')
    result: list[list[str]] = [[] for _ in range(count)]
    for name in names:
        if not name or name.startswith('-') or any(c in name for c in '\0\r\n'):
            raise ValueError('invalid compiled test name')
        key = hashlib.sha256(b'zeno-fcis/miri-test-part/1\0' + name.encode()).digest()
        result[int.from_bytes(key[:8], 'big') % count].append(name)
    if Counter(name for part in result for name in part) != Counter(names):
        raise ValueError('partitions do not cover the original multiset')
    return result


def base_arguments(row: dict, rows: list[dict]) -> list[str]:
    excluded = list(row.get('miri_skip', []))
    if row['packages'] == '-p zeno-fcis-synthesis':
        excluded += [r['test'] for r in rows if r['packages'] == row['packages']
                     and r.get('target') == row.get('target') and 'test' in r]
    if not excluded:
        return []
    return ['--', '--exact'] + [arg for name in excluded for arg in ('--skip', name)]


def listed(command: list[str], arguments: list[str]) -> list[str]:
    output = subprocess.run(command + arguments + ['--list', '--format', 'terse'],
                            cwd=ROOT, check=True, text=True, stdout=subprocess.PIPE).stdout
    names = []
    for line in output.splitlines():
        if not line.endswith(': test'):
            raise ValueError(f'unexpected compiled test-list line: {line!r}')
        names.append(line.removesuffix(': test'))
    return names


def inventory(command: list[str], arguments: list[str]) -> tuple[list[str], list[str]]:
    names = listed(command, arguments)
    ignored = listed(command, arguments + ['--ignored'])
    if Counter(ignored) - Counter(names):
        raise ValueError('ignored tests are absent from the full inventory')
    return names, ignored


def part_arguments(original: list[str], names: list[str], chosen: list[str]) -> list[str]:
    # Rustdoc and libtest do not compose multiple positive filters identically.
    # Keep the original skips and exclude the exact complement instead.
    arguments = list(original)
    if '--exact' not in arguments:
        arguments.append('--exact')
    return arguments + [arg for name in sorted(set(names) - set(chosen))
                        for arg in ('--skip', name)]


def execute(selected: dict) -> int:
    source = (ROOT / '.github/workflows/miri.yml').read_text()
    raw = json.loads(source.split('        include: ', 1)[1].split('    env:', 1)[0])
    rows = logical_rows(raw)
    if selected not in raw or 'part' not in selected:
        raise ValueError('selected part is absent from the complete matrix')
    base = base_arguments(selected, rows)
    if shlex.split(os.environ.get('MIRI_TEST_ARGS', '')) != base:
        raise ValueError('existing exclusion/exact-test arguments changed')
    command = ['cargo', '+nightly-2026-07-21', 'miri', 'test', '--locked', '--offline',
               *shlex.split(selected['packages']), *shlex.split(selected.get('target', ''))]
    original = base or ['--']
    names, ignored = inventory(command, original)
    if not Counter(names) - Counter(ignored):
        raise ValueError('test inventory has no runnable cases')
    parts = partition(names, selected['parts'])
    chosen = parts[selected['part'] - 1]
    chosen_ignored = partition(ignored, selected['parts'])[selected['part'] - 1]
    if not Counter(chosen) - Counter(chosen_ignored):
        raise ValueError('part has no runnable cases; choose fewer parts')
    arguments = part_arguments(original, names, chosen)
    actual, actual_ignored = inventory(command, arguments)
    if Counter(actual) != Counter(chosen) or Counter(actual_ignored) != Counter(chosen_ignored):
        raise ValueError('actual libtest selection differs from the declared part')
    audit = {'schema': 'zeno-fcis/miri-test-part/1', 'row': selected,
             'flags': os.environ.get('MIRIFLAGS', ''), 'command': command + arguments,
             'original_tests': names, 'selected_tests': chosen,
             'original_ignored': ignored, 'selected_ignored': chosen_ignored,
             'all_part_sizes': [len(p) for p in parts],
             'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
             'lock_sha256': hashlib.sha256((ROOT / 'Cargo.lock').read_bytes()).hexdigest(),
             'helper_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
             'workflow_sha256': hashlib.sha256(source.encode()).hexdigest(),
             'status': 'running', 'exit': None,
             'nonclaim': 'Partitions preserve cases; separate test processes have different interpreter histories.'}
    path = Path(os.environ['RUNNER_TEMP']) / 'miri-test-part.json'
    path.write_text(json.dumps(audit, indent=2) + '\n')
    print(f'Miri {selected["group"]} part {selected["part"]}/{selected["parts"]}: '
          f'{len(chosen)} of {len(names)} compiled tests; all existing filters retained.', flush=True)
    code = subprocess.call(command + arguments, cwd=ROOT)
    audit.update(status='passed' if code == 0 else 'failed', exit=code)
    path.write_text(json.dumps(audit, indent=2) + '\n')
    return code


if __name__ == '__main__':
    try:
        raise SystemExit(execute(json.loads(os.environ['SELECTED_GROUP'])))
    except (KeyError, ValueError, OSError, subprocess.SubprocessError) as error:
        print(f'miri-test-part: REFUSED: {error}', file=sys.stderr)
        raise SystemExit(1)
