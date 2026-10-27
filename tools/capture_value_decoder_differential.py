#!/usr/bin/env python3
"""Capture a deterministic native result stream and bind its exact bytes by SHA-256."""
from __future__ import annotations
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import subprocess
import time


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--count', type=int, default=1_000_000)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    if args.count < 1_000_000:
        parser.error('qualification requires at least 1,000,000 inputs')
    args.out.mkdir(parents=True, exist_ok=True)
    command = [str(args.binary.resolve()), str(args.count)]
    digest = hashlib.sha256()
    count = size = 0
    started = time.monotonic()
    with (args.out / 'stderr.log').open('wb') as errors, (args.out / 'results.txt.gz').open('wb') as raw:
        with gzip.GzipFile(fileobj=raw, mode='wb', mtime=0, compresslevel=1) as compressed:
            process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=errors)
            assert process.stdout is not None
            while chunk := process.stdout.read(1024 * 1024):
                digest.update(chunk)
                count += chunk.count(b'\n')
                size += len(chunk)
                compressed.write(chunk)
            exit_code = process.wait()
    receipt = {'format': 'zcve-differential-capture-v1', 'count': args.count,
               'seed': 'c2b2ae3d27d4eb4f', 'rows_including_header': count,
               'bytes': size, 'sha256': digest.hexdigest(), 'exit_code': exit_code,
               'elapsed_seconds': time.monotonic() - started}
    (args.out / 'capture.json').write_text(json.dumps(receipt, sort_keys=True, indent=2) + '\n')
    print(json.dumps(receipt, sort_keys=True), flush=True)
    return 0 if exit_code == 0 and count == args.count + 1 else 1


if __name__ == '__main__':
    raise SystemExit(main())
