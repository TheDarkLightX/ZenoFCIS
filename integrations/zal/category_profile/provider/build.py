"""Fresh trusted source capture and one pinned GHC build; no dependency fetching."""
import hashlib
import json
import os
from pathlib import Path
import resource
import subprocess
import sys

ROOT = Path(__file__).resolve().parent


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    if len(sys.argv) != 2:
        raise SystemExit('expected a fresh owned runtime directory')
    destination = Path(sys.argv[1]).absolute()
    if destination.exists() or destination.is_symlink():
        raise SystemExit('fresh destination required')
    if subprocess.check_output(['/usr/bin/ghc', '--numeric-version'], text=True).strip() != '9.4.7':
        raise SystemExit('GHC9.4.7 required')
    required = {'base': '4.17.2.0', 'containers': '0.6.7', 'aeson': '2.1.2.1', 'text': '2.0.2', 'bytestring': '0.11.5.2'}
    for package, wanted in required.items():
        actual = subprocess.check_output(['/usr/bin/ghc-pkg', 'field', package, 'version', '--simple-output'], text=True).strip()
        if actual != wanted:
            raise SystemExit('installed package mismatch: ' + package)
    manifest_bytes = (ROOT / 'SOURCE_MANIFEST.json').read_bytes()
    pins = json.loads(manifest_bytes)['files']
    captured = {}
    for name, digest in pins.items():
        path = ROOT / name
        if path.is_symlink() or not path.is_file():
            raise SystemExit('regular trusted source required')
        with path.open('rb') as handle:
            captured[name] = handle.read(1024**2 + 1)
        if len(captured[name]) > 1024**2 or sha(captured[name]) != digest:
            raise SystemExit('trusted source changed before capture')
    destination.mkdir()
    for name, data in captured.items():
        path = destination / name; path.parent.mkdir(parents=True, exist_ok=True)
        with path.open('xb') as handle:
            handle.write(data)
    (destination / 'SOURCE_MANIFEST.json').write_bytes(manifest_bytes)
    (destination / 'main.py').write_bytes((ROOT / 'main.py').read_bytes())
    build = destination / '.build'; build.mkdir()
    os.nice(19)
    resource.setrlimit(resource.RLIMIT_AS, (2*1024**3,)*2)
    resource.setrlimit(resource.RLIMIT_CPU, (180,)*2)
    resource.setrlimit(resource.RLIMIT_CORE, (0,0))
    command = ['/usr/bin/ghc', '-O1', '-Wall', '-Werror', '-Wincomplete-patterns', '-fforce-recomp', '-rtsopts',
               '-package-env', '-', '-hide-all-packages', '-i'+str(destination/'src')]
    for package in required:
        command += ['-package', package]
    command += ['-outputdir', str(build), '-o', str(build/'lens-worker'), str(destination/'app/Main.hs'), '+RTS', '-M768m', '-RTS']
    proc = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=180,
                          env={'PATH': '/usr/bin:/bin', 'LANG': 'C.UTF-8', 'LC_ALL': 'C.UTF-8'}, cwd=destination)
    (destination/'build.log').write_bytes(proc.stdout)
    unchanged = all(sha((destination/name).read_bytes()) == digest for name,digest in pins.items())
    record = {'schema': 'zal/lens-category-build/1', 'ghc': '9.4.7', 'packages': required, 'source_sha256': pins,
              'manifest_sha256': sha(manifest_bytes), 'command': command, 'exit': proc.returncode,
              'captured_sources_unchanged': unchanged,
              'binary_sha256': sha((build/'lens-worker').read_bytes()) if proc.returncode == 0 else None}
    (destination/'build.json').write_text(json.dumps(record,indent=2)+'\n')
    print(json.dumps({'exit': proc.returncode, 'sources_unchanged': unchanged, 'log': str(destination/'build.log')}))
    raise SystemExit(proc.returncode if unchanged else 3)


if __name__ == '__main__':
    main()
