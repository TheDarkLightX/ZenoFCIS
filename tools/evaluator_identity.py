"""Version 1 evaluator identity: SHA256 of framed sorted source/pin hashes.

Encoding: ASCII ZENO-FCIS-EVALUATOR followed by NUL, u32-BE version 1,
u64-BE row count, then (u64-BE UTF-8 path length, path, 32-byte SHA256(payload))
for each row, sorted lexicographically by its ASCII path. The generated Rust
constant is intentionally excluded. Source membership is approved separately.
"""
from __future__ import annotations
import hashlib
import json
from pathlib import Path
import re

GENERATED = 'crates/zeno-fcis-synthesis/src/finite/execution_v2/authority/evaluator.rs'
PIN_NAMES = ['@pin/core-profile', '@pin/runtime-toolchain', '@pin/verifier-toolchain']
PREFIX = b'ZENO-FCIS-EVALUATOR\0' + (1).to_bytes(4, 'big')


def read_regular(path: Path) -> bytes:
    if any(p.is_symlink() for p in (path, *path.parents)) or not path.is_file():
        raise ValueError(f'missing, nonregular or symlink source: {path}')
    return path.read_bytes()


def safe_name(name: str, pin: bool = False) -> bool:
    pattern = r'@pin/[A-Za-z0-9_./-]+' if pin else r'[A-Za-z0-9_./-]+'
    return (isinstance(name, str) and re.fullmatch(pattern, name) is not None
            and not name.startswith('/') and all(p not in ('', '.', '..') for p in name.split('/')))


def manifest(root: Path, path: Path) -> dict:
    raw = json.loads(read_regular(path))
    if set(raw) != {'schema', 'paths', 'pins'} or raw['schema'] != 'zeno-fcis/authority-source-closure/1':
        raise ValueError('invalid evaluator manifest schema')
    paths = raw['paths']
    if (not isinstance(paths, list) or not paths or not all(safe_name(p) for p in paths)
            or paths != sorted(set(paths)) or GENERATED in paths):
        raise ValueError('unsafe, duplicate, unsorted or self-including evaluator paths')
    for name in paths:
        read_regular(root / name)
    pins = raw['pins']
    if (not isinstance(pins, list) or not all(isinstance(p, dict) and set(p) == {'path', 'bytes'}
            and safe_name(p['path'], True) and isinstance(p['bytes'], str)
            and all(32 <= ord(c) <= 126 or c in '\n\r\t' for c in p['bytes']) for p in pins)
            or [p['path'] for p in pins] != PIN_NAMES):
        raise ValueError('missing, extra, duplicate or unsafe evaluator pin')
    if pins[2]['bytes'].encode() != read_regular(root / 'verification/verus/toolchain.json'):
        raise ValueError('verifier pin differs from pinned toolchain')
    return raw


def encode(rows: list[tuple[str, bytes]]) -> bytes:
    names = [name for name, _ in rows]
    if len(set(names)) != len(names) or not all(safe_name(n, n.startswith('@pin/')) for n in names):
        raise ValueError('duplicate or unsafe evaluator row')
    result = bytearray(PREFIX + len(rows).to_bytes(8, 'big'))
    for name, payload in sorted(rows):
        path = name.encode('ascii')
        result.extend(len(path).to_bytes(8, 'big'))
        result.extend(path)
        result.extend(hashlib.sha256(payload).digest())
    return bytes(result)


def digest(root: Path, raw: dict) -> bytes:
    rows = [(name, read_regular(root / name)) for name in raw['paths']]
    rows.extend((pin['path'], pin['bytes'].encode()) for pin in raw['pins'])
    return hashlib.sha256(encode(rows)).digest()


def render(root: Path, raw: dict) -> str:
    # Fixed-width hex, 16 per line, is rustfmt's own layout for every digest,
    # so the generated file passes `cargo fmt --check` unchanged.
    value = digest(root, raw)
    rows = ''.join('    ' + ', '.join(f'0x{byte:02x}' for byte in value[start:start + 16]) + ',\n'
                   for start in (0, 16))
    return ('//! Generated evaluator digest; tools/check_authority_v2.py checks approved source binding.\n'
            '//! SHA-256 is computed outside the verified core; the release gates are required.\n'
            '#[cfg(verus_keep_ghost)]\n'
            'use vstd::prelude::*;\n'
            '/// SHA-256 of the approved versioned evaluator source manifest and build pins.\n'
            '#[cfg_attr(verus_keep_ghost, verus_spec)]\n'
            'pub const EVALUATOR: [u8; 32] = [\n' + rows + '];\n')
