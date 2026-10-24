#!/usr/bin/env python3
"""Keep each published crate's compiled source text byte-identical to its workspace originals.

A published crate builds from its own archive, so text it embeds from a sibling crate or from the repository is
compiled from an exact copy in its `source-snapshots/` folder. Each crate that embeds such text lists it in its
`source-snapshots.json`: `sources` names files under `crates/`, and `repository_sources` names repository files,
copied under `source-snapshots/repository/`.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path, PurePosixPath
import re

ROOT = Path(__file__).resolve().parents[1]
CLI = Path('crates/zeno-fcis-cli')
MANIFEST_NAME = 'source-snapshots.json'
DIRECTORY_NAME = 'source-snapshots'
MANIFEST = CLI / MANIFEST_NAME
DIRECTORY = CLI / DIRECTORY_NAME
FORMAT = 'zeno-fcis/cli-source-snapshots/1'
REFERENCE = re.compile(r'include_(?:str|bytes)!\s*\(\s*"((?:\.\./)+' + DIRECTORY_NAME + r'/[^"\n]+)"')


def safe(name: str) -> None:
    path = PurePosixPath(name)
    if not name or path.is_absolute() or '..' in path.parts or path.as_posix() != name:
        raise ValueError(f'unsafe source snapshot: {name}')


def check_crate(root: Path, crate: Path, *, write: bool) -> tuple[int, int]:
    document = json.loads((root / crate / MANIFEST_NAME).read_text())
    sources = document.get('sources', [])
    repository = document.get('repository_sources', [])
    if (document.get('format') != FORMAT or set(document) - {'format', 'sources', 'repository_sources'}
            or not isinstance(sources, list) or not isinstance(repository, list) or not sources + repository
            or sources != sorted(set(sources)) or repository != sorted(set(repository))):
        raise ValueError(f'invalid source snapshot manifest: {crate}')
    pairs = [(name, root / 'crates' / name, name + '.txt') for name in sources]
    pairs += [(name, root / name, 'repository/' + name + '.txt') for name in repository]
    directory = root / crate / DIRECTORY_NAME
    expected = set()
    size = 0
    for name, original, stored in pairs:
        safe(name)
        snapshot = directory / stored
        if original.is_symlink() or snapshot.is_symlink():
            raise ValueError(f'symlinked source snapshot: {crate}: {name}')
        data = original.read_bytes()
        if write:
            snapshot.parent.mkdir(parents=True, exist_ok=True)
            snapshot.write_bytes(data)
        if snapshot.read_bytes() != data:
            raise ValueError(f'source snapshot differs: {crate}: {name}')
        expected.add(stored)
        size += len(data)
    retained = {p.relative_to(directory).as_posix() for p in directory.rglob('*') if p.is_file()}
    used = set()
    for path in (root / crate / 'src').rglob('*.rs'):
        for reference in REFERENCE.findall(path.read_text()):
            resolved = Path(os.path.normpath(path.parent / reference))
            if directory not in resolved.parents:
                raise ValueError(f'source snapshot reference leaves its crate: {path.relative_to(root)}: {reference}')
            used.add(resolved.relative_to(directory).as_posix())
    if retained != expected or used != expected:
        raise ValueError(f'source snapshot files, references and manifest differ: {crate}')
    return len(pairs), size


def check(root: Path = ROOT, *, write: bool = False) -> dict:
    crates = sorted(path.parent.relative_to(root) for path in (root / 'crates').glob('*/' + MANIFEST_NAME))
    if CLI not in crates:
        raise ValueError('the CLI source snapshot manifest is missing')
    files = size = 0
    for crate in crates:
        count, length = check_crate(root, crate, write=write)
        files += count
        size += length
    return {'status': 'passed', 'crates': len(crates), 'files': files, 'bytes': size}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--write', action='store_true', help='copy exact original bytes, then check')
    options = parser.parse_args()
    print(json.dumps(check(write=options.write), sort_keys=True))


if __name__ == "__main__":
    try:
        from telemetry import run_cli
    except ImportError:
        # Reporting is optional, including for a standalone copy of this tool.
        run_cli = lambda function: function()
    def _entrypoint():
        main()
    run_cli(_entrypoint)
