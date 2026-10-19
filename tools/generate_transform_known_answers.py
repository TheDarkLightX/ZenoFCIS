#!/usr/bin/env python3
"""Generate a new checker version's vectors through the pinned CLI.

Historical directories are read-only inputs. Reports must preserve all fields
except the explicitly versioned checker identity and its receipt digest.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def sha(data):
    return hashlib.sha256(data).hexdigest()


def differences(a, b, path=()):
    if type(a) is not type(b):
        return {path}
    if isinstance(a, dict):
        return {p for key in a.keys() | b.keys() for p in
                ({(*path, key)} if key not in a or key not in b else
                 differences(a[key], b[key], (*path, key)))}
    if isinstance(a, list):
        if len(a) != len(b):
            return {path}
        return {p for i, (x, y) in enumerate(zip(a, b))
                for p in differences(x, y, (*path, i))}
    return set() if a == b else {path}


def generate(cli, source, output, semantics):
    cli = cli.resolve()
    identity = sha(cli.read_bytes())
    manifest = json.loads((source / 'vectors.json').read_bytes())
    if manifest['semantics'] == semantics or output.exists():
        raise ValueError('select a new version and a new output directory')
    output.mkdir(parents=True)
    def name(value):
        if not isinstance(value, str) or Path(value).name != value or value in ('.', '..'):
            raise ValueError('fixture names must be single relative filenames')
        return value
    def run(args, expected=0):
        result = subprocess.run([str(cli), *map(str, args)], capture_output=True, timeout=60)
        if result.returncode != expected or result.stderr:
            raise RuntimeError((args, result.returncode, result.stderr.decode(errors='replace')))
        if sha(cli.read_bytes()) != identity:
            raise ValueError('CLI changed during generation')
        return result.stdout
    programs = sorted({name(v[key]) for v in manifest['vectors'] for key in ('original', 'candidate')})
    for binary in programs:
        text = Path(binary).with_suffix('.json').name
        (output / text).write_bytes((source / text).read_bytes())
        run(['loop', 'encode', '--program', output / text, '--out', output / binary])
        if (output / binary).read_bytes() != (source / binary).read_bytes():
            raise ValueError(f'canonical input program changed: {binary}')
    for vector in manifest['vectors']:
        command = ['transform', 'check', '--original', output / name(vector['original']),
                   '--candidate', output / name(vector['candidate']), '--step-limit', str(vector['step_limit']),
                   '--max-input-tuples', str(vector['max_input_tuples'])]
        raw = run(command, vector['exit'])
        report = json.loads(raw)
        old = json.loads((source / name(vector['expected'])).read_bytes())
        changed = differences(old, report)
        allowed = {('detail', 'receipt', 'checker', 'evaluator_identity'),
                   ('detail', 'receipt', 'checker', 'semantics'), ('detail', 'receipt_sha256')}
        if not changed <= allowed:
            raise ValueError((vector['name'], 'unexpected behavior change', sorted(changed)))
        if 'receipt' in vector:
            if report['detail']['receipt']['checker']['semantics'] != semantics:
                raise ValueError('CLI does not implement the selected checker semantics')
            receipt = output / name(vector['receipt'])
            again = run([*command, '--receipt', receipt])
            if again != raw or json.loads(receipt.read_bytes()) != report['detail']['receipt']:
                raise ValueError('receipt generation changed the report')
            if sha(receipt.read_bytes()) != report['detail']['receipt_sha256']:
                raise ValueError('receipt digest differs')
        (output / name(vector['expected'])).write_bytes(raw)
    manifest['semantics'] = semantics
    (output / 'vectors.json').write_text(json.dumps(manifest, indent=2) + '\n')
    return {'vectors': len(manifest['vectors']), 'receipts': sum('receipt' in v for v in manifest['vectors']),
            'programs': len(programs), 'cli_sha256': identity, 'semantics': semantics}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cli', type=Path, required=True)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--semantics', required=True)
    args = parser.parse_args()
    print(json.dumps(generate(args.cli, args.source, args.out, args.semantics), sort_keys=True))


if __name__ == '__main__':
    main()
