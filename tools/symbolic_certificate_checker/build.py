#!/usr/bin/env python3
"""Build the pinned external checker in a separate disk directory.

The checker verifies Int/Bool Alethe certificates. It does not generate proofs,
prove compiler correctness, authenticate callers or authorize publication.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tarfile
import urllib.request

HERE = Path(__file__).resolve().parent


def digest(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--build-dir', type=Path, required=True)
    parser.add_argument('--archive', type=Path, help='Existing pinned archive for offline source intake')
    parser.add_argument('--offline', action='store_true', help='Also require cached Cargo dependencies')
    options = parser.parse_args()
    pin = json.loads((HERE / 'PIN.json').read_text())
    destination = options.build_dir.resolve()
    if destination.exists(): raise ValueError('build directory already exists')
    destination.mkdir(parents=True)
    if options.archive:
        raw = options.archive.read_bytes()
    else:
        with urllib.request.urlopen(pin['archive_url'], timeout=30) as response:
            raw = response.read(16 * 1024 * 1024 + 1)
    if len(raw) > 16 * 1024 * 1024 or digest(raw) != pin['archive_sha256']:
        raise ValueError('source archive does not match the pinned digest')
    archive = destination / 'source.tar.gz'; archive.write_bytes(raw)
    upstream = destination / 'carcara'; upstream.mkdir()
    inventory = {}
    with tarfile.open(archive) as source:
        members = source.getmembers()
        prefix = 'carcara-' + pin['revision']
        for item in members:
            parts = Path(item.name).parts
            if not parts or parts[0] != prefix or '..' in parts or Path(item.name).is_absolute():
                raise ValueError('unsafe source archive path')
            if item.isdir(): continue
            if not item.isfile() or item.size > 8 * 1024 * 1024:
                raise ValueError('unsupported source archive member')
            relative = Path(*parts[1:]); name = relative.as_posix()
            if not name or name in inventory: raise ValueError('duplicate source archive member')
            content = source.extractfile(item).read(item.size + 1)
            if len(content) != item.size: raise ValueError('incomplete source archive member')
            target = upstream / relative; target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(content); inventory[name] = digest(content)
    wrapper = destination / 'wrapper'; (wrapper / 'src').mkdir(parents=True)
    for origin, target in [('Cargo.toml.in', 'Cargo.toml'), ('Cargo.lock', 'Cargo.lock'), ('src/main.rs', 'src/main.rs')]:
        shutil.copyfile(HERE / origin, wrapper / target)
    compiled_inputs = {name: digest((wrapper / name).read_bytes())
                       for name in ('Cargo.toml', 'Cargo.lock', 'src/main.rs')}
    command = ['cargo', '+' + pin['rust'], 'build', '--locked', '-j', '1', '--manifest-path', str(wrapper / 'Cargo.toml')]
    if options.offline: command.append('--offline')
    subprocess.run(command, check=True)
    if any(digest((wrapper / name).read_bytes()) != expected
           for name, expected in compiled_inputs.items()) or any(
           digest((upstream / name).read_bytes()) != expected
           for name, expected in inventory.items()):
        raise ValueError('compiled source changed during the build')
    target = Path(__import__('os').environ.get('CARGO_TARGET_DIR', str(wrapper / 'target'))).resolve()
    executable = target / 'debug/zfcis-carcara-root-check'
    version = subprocess.run([str(executable), '--version'], check=True, capture_output=True).stdout
    expected = ('zfcis-carcara-root-check ' + pin['wrapper_version'] + ' ' + pin['revision'] + '\n').encode()
    if version != expected: raise ValueError('unexpected checker version')
    report = {'schema': 'zeno-fcis/certificate-checker-build/1', 'source': pin,
              'source_files': inventory, 'wrapper': compiled_inputs,
              'argv': command, 'executable': str(executable), 'sha256': digest(executable.read_bytes()),
              'version': version.decode(), 'qualification': 'unrun'}
    (destination / 'BUILD.json').write_text(json.dumps(report, indent=2, sort_keys=True) + '\n')
    print(json.dumps({'executable': str(executable), 'sha256': report['sha256'], 'qualification': 'unrun'}))


if __name__ == '__main__': main()
