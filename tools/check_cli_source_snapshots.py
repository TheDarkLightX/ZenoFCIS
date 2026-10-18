#!/usr/bin/env python3
"""Keep the CLI's compiled source text byte-identical to its workspace originals."""
from __future__ import annotations

import argparse
import json
from pathlib import Path, PurePosixPath
import re

ROOT = Path(__file__).resolve().parents[1]
CLI = Path('crates/zeno-fcis-cli')
MANIFEST = CLI / 'source-snapshots.json'
DIRECTORY = CLI / 'source-snapshots'


def check(root: Path = ROOT, *, write: bool = False) -> dict:
    document = json.loads((root / MANIFEST).read_text())
    sources = document['sources']
    if document['format'] != 'zeno-fcis/cli-source-snapshots/1' or sources != sorted(set(sources)):
        raise ValueError('invalid CLI source snapshot manifest')
    expected = set()
    size = 0
    for name in sources:
        path = PurePosixPath(name)
        if path.is_absolute() or '..' in path.parts or path.as_posix() != name:
            raise ValueError(f'unsafe CLI source snapshot: {name}')
        original = root / 'crates' / name
        snapshot = root / DIRECTORY / (name + '.txt')
        if original.is_symlink() or snapshot.is_symlink():
            raise ValueError(f'symlinked CLI source snapshot: {name}')
        data = original.read_bytes()
        if write:
            snapshot.parent.mkdir(parents=True, exist_ok=True)
            snapshot.write_bytes(data)
        if snapshot.read_bytes() != data:
            raise ValueError(f'CLI source snapshot differs: {name}')
        expected.add(name + '.txt')
        size += len(data)
    retained = {p.relative_to(root / DIRECTORY).as_posix() for p in (root / DIRECTORY).rglob('*') if p.is_file()}
    used = set()
    for path in (root / CLI / 'src').rglob('*.rs'):
        used.update(re.findall(r'include_(?:str|bytes)!\s*\(\s*"\.\./source-snapshots/([^"\n]+)"', path.read_text()))
    if retained != expected or used != expected:
        raise ValueError('CLI source snapshot files, references and manifest differ')
    return {'status': 'passed', 'files': len(sources), 'bytes': size}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--write', action='store_true', help='copy exact original bytes, then check')
    options = parser.parse_args()
    print(json.dumps(check(write=options.write), sort_keys=True))


if __name__ == '__main__':
    main()
