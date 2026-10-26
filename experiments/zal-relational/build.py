#!/usr/bin/env python3
"""Build the isolated Haskell worker from installed, pinned packages."""
import hashlib
import json
import os
from pathlib import Path
import resource
import subprocess
import time

ROOT = Path(__file__).resolve().parent

def limits():
    os.nice(10)
    os.sched_setaffinity(0, set(sorted(os.sched_getaffinity(0))[:12]))
    resource.setrlimit(resource.RLIMIT_AS, (2 * 1024**3,) * 2)
    resource.setrlimit(resource.RLIMIT_CPU, (120,) * 2)

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def main():
    if subprocess.check_output(['ghc', '--numeric-version'], text=True).strip() != '9.4.7':
        raise SystemExit('this experimental build requires GHC 9.4.7')
    required = {'base': '4.17.2.0', 'containers': '0.6.7', 'aeson': '2.1.2.1',
                'text': '2.0.2', 'bytestring': '0.11.5.2'}
    for package, wanted in required.items():
        actual = subprocess.check_output(['ghc-pkg', 'field', package, 'version', '--simple-output'], text=True).split()
        if actual != [wanted]:
            raise SystemExit('package version mismatch: ' + package + '; expected ' + wanted)
    build = ROOT / '.build'
    build.mkdir(exist_ok=True)
    evidence = ROOT / 'evidence'
    evidence.mkdir(exist_ok=True)
    sources = sorted([*ROOT.glob('src/**/*.hs'), *ROOT.glob('app/*.hs')])
    before = {str(p.relative_to(ROOT)): sha(p) for p in sources}
    command = ['ghc', '-O1', '-Wall', '-Werror', '-Wincomplete-patterns',
               '-rtsopts', '-i' + str(ROOT / 'src'), '-package', 'aeson',
               '-outputdir', str(build), '-o', str(build / 'zal-relational-worker'),
               str(ROOT / 'app/Main.hs'), '+RTS', '-M768m', '-RTS']
    started = time.monotonic()
    result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            timeout=180, preexec_fn=limits)
    (evidence / 'build.log').write_bytes(result.stdout)
    if result.returncode:
        print(result.stdout.decode(errors='replace'))
        raise SystemExit(result.returncode)
    if before != {str(p.relative_to(ROOT)): sha(p) for p in sources}:
        raise SystemExit('sources changed during build')
    packages = subprocess.check_output(['ghc-pkg', 'list', '--simple-output'], text=True).split()
    record = {'schema': 'zal/relational-build/1', 'command': command,
              'source_sha256': before, 'binary_sha256': sha(build / 'zal-relational-worker'),
              'ghc': subprocess.check_output(['ghc', '--version'], text=True).strip(),
              'installed_packages': packages, 'elapsed_seconds': time.monotonic() - started,
              'exit': result.returncode}
    (evidence / 'build.json').write_text(json.dumps(record, indent=2) + '\n')
    print(json.dumps({'built': str(build / 'zal-relational-worker'), 'source_files': len(sources),
                      'sha256': record['binary_sha256']}))

if __name__ == '__main__':
    main()
