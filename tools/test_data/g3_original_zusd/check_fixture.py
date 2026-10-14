#!/usr/bin/env python3
"""Check that the checked-in zUSD fixture is exactly this generator's output.

It recomputes every file digest SOURCE.json lists, then regenerates the whole
fixture in an empty temporary directory and requires byte-identical files.
Regeneration also recomputes the corpus and unformatted-data digests.
"""
import hashlib
import json
import sys
import tempfile
from pathlib import Path

import emit_rust

FILES = ('data.rs', 'native-cases.txt', 'SOURCE.json')


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check(fixture):
    source = json.loads((fixture / 'SOURCE.json').read_text())
    listed = {
        'native_source_sha256': Path(emit_rust.__file__).with_name('original_zusd.py'),
        'formatted_rust_data_sha256': fixture / 'data.rs',
        'native_cases_sha256': fixture / 'native-cases.txt',
    }
    failures = [f'{key} differs from {path.name}' for key, path in listed.items() if source[key] != sha(path)]
    with tempfile.TemporaryDirectory(prefix='zeno-g3-zusd-') as temporary:
        reproduced = Path(temporary) / 'fixture'
        emit_rust.build(reproduced)
        failures += [f'{name} is not the generator output' for name in FILES
                     if (reproduced / name).read_bytes() != (fixture / name).read_bytes()]
    return failures


def main():
    failures = check(emit_rust.DEFAULT_OUT)
    for failure in failures:
        print(f'g3-original-zusd: {failure}', file=sys.stderr)
    print('g3-original-zusd: ' + ('FAILED' if failures else 'fixture reproduces byte for byte'))
    return 1 if failures else 0


if __name__ == '__main__':
    raise SystemExit(main())
