#!/usr/bin/env python3
"""Source-pinned, deliberately conservative CLIF *observation* of div/rem pairs.

No rewrite, dominance proof, trap proof, performance claim, or compiler mutation.
A same-block, same-SSA-operand pair is a candidate for downstream proof work.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import re
from pathlib import Path

SCHEMA = 'zeno-fcis/tier1-divrem-clif-observation/1'
UPSTREAM = 'b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49'
MAX_FILES = 200
MAX_BYTES = 2_000_000
BLOCK = re.compile(r'^\s*(block\d+)(?:\([^;]*\))?:\s*(?:;.*)?$')
OP = re.compile(r'^\s*(?:@[0-9a-fA-F]+\s+)?(v\d+)\s*=\s*(udiv|urem)\s+(v\d+),\s*(v\d+)\s*(?:;.*)?$')


def digest(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def inspect(text: str) -> dict:
    """Detect exact SSA-operand pairs confined to one basic block.

    Order is recorded, not assumed. Even a hit is NOT sufficient for a safe
    rewrite: only downstream compiler analysis can establish dominance,
    trap-order preservation, reachability, live ranges, and target cost.
    """
    current = None
    candidates = []
    definitions = {}
    matched_insts = 0
    for number, raw in enumerate(text.splitlines(), start=1):
        if raw.strip().startswith(';'):
            continue
        block = BLOCK.fullmatch(raw)
        if block:
            current = block.group(1)
            definitions = {}
            continue
        if raw.strip() == '}':
            current = None
            definitions = {}
            continue
        op = OP.fullmatch(raw)
        if not op or current is None:
            continue
        dst, kind, x, y = op.groups()
        matched_insts += 1
        key = (x, y)
        twin = 'urem' if kind == 'udiv' else 'udiv'
        if (key, twin) in definitions:
            other = definitions[(key, twin)]
            candidates.append({
                'block': current, 'operands': [x, y],
                'first': {'kind': twin, 'dest': other['dest'], 'line': other['line']},
                'second': {'kind': kind, 'dest': dst, 'line': number},
                'classification': 'lexical-same-block-SSA-operand-match-not-safe-rewrite',
            })
        definitions[(key, kind)] = {'dest': dst, 'line': number}
    return {'matched_divrem_instruction_lines': matched_insts,
            'candidate_pairs': candidates, 'candidate_pair_count': len(candidates),
            'scope': 'textual single-basic-block operand match; no dominance/trap/alias/cost assertion'}


def scan(root: Path, input_wat: Path, native_aot: Path, out: Path,
         upstream_sha: str) -> dict:
    if out.exists():
        raise ValueError('refusing to overwrite receipt')
    if upstream_sha != UPSTREAM:
        raise ValueError('pinned upstream revision mismatch')
    if not root.is_dir() or not input_wat.is_file() or not native_aot.is_file():
        raise ValueError('missing emitted artifacts or source WAT')
    files = sorted(p for p in root.rglob('*') if p.is_file())
    if not files or len(files) > MAX_FILES:
        raise ValueError('no CLIF files or excessive file count')
    results = []
    for path in files:
        if path.is_symlink() or not path.resolve().is_relative_to(root.resolve()):
            raise ValueError('unsafe input path')
        raw = path.read_bytes()
        if not raw or len(raw) > MAX_BYTES:
            raise ValueError('empty/oversized CLIF file')
        parsed = inspect(raw.decode('utf-8'))
        results.append({'path': path.relative_to(root).as_posix(),
                        'sha256': digest(raw), 'bytes': len(raw), **parsed})
    record = {
        'schema': SCHEMA, 'status': 'source-pinned-emitted-CLIF-candidate-observation',
        'source_commit': upstream_sha, 'wat_sha256': digest(input_wat.read_bytes()),
        'aot_sha256': digest(native_aot.read_bytes()),
        'aot_bytes': native_aot.stat().st_size,
        'scanner_sha256': digest(Path(__file__).read_bytes()),
        'results': results,
        'total_candidate_pairs': sum(r['candidate_pair_count'] for r in results),
        'limitations': [
            'Lexical CLIF observation is not ISLE rule application or a safe rewriting proof',
            'The printed CLIF optimization phase may differ from the lowering phase',
            'SSA identity alone does not establish trap ordering, dominance, or host state',
            'This records no cost savings or compiler patch',
            'No same-Wasm baseline-vs-patched compiler comparison is attempted',
        ],
    }
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(record, indent=2, sort_keys=True) + '\n')
    return record


def main() -> None:
    parser = argparse.ArgumentParser()
    for flag in ('clif-root', 'wat', 'aot', 'out'):
        parser.add_argument('--'+flag, type=Path, required=True)
    parser.add_argument('--upstream-sha', required=True)
    args = parser.parse_args()
    result = scan(args.clif_root, args.wat, args.aot, args.out, args.upstream_sha)
    print(json.dumps({'status': result['status'],
                      'candidate_pairs': result['total_candidate_pairs']}, sort_keys=True))


if __name__ == '__main__':
    main()
)


def digest(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def inspect(text: str) -> dict:
    """Detect exact SSA-operand pairs confined to one basic block.

    Order is recorded, not assumed. Even a hit is NOT sufficient for a safe
    rewrite: only downstream compiler analysis can establish dominance,
    trap-order preservation, reachability, live ranges, and target cost.
    """
    current = None
    candidates = []
    definitions = {}
    matched_insts = 0
    for number, raw in enumerate(text.splitlines(), start=1):
        if raw.strip().startswith(';'):
            continue
        block = BLOCK.fullmatch(raw)
        if block:
            current = block.group(1)
            definitions = {}
            continue
        if raw.strip() == '}':
            current = None
            definitions = {}
            continue
        op = OP.fullmatch(raw)
        if not op or current is None:
            continue
        dst, kind, x, y = op.groups()
        matched_insts += 1
        key = (x, y)
        twin = 'urem' if kind == 'udiv' else 'udiv'
        if (key, twin) in definitions:
            other = definitions[(key, twin)]
            candidates.append({
                'block': current, 'operands': [x, y],
                'first': {'kind': twin, 'dest': other['dest'], 'line': other['line']},
                'second': {'kind': kind, 'dest': dst, 'line': number},
                'classification': 'lexical-same-block-SSA-operand-match-not-safe-rewrite',
            })
        definitions[(key, kind)] = {'dest': dst, 'line': number}
    return {'matched_divrem_instruction_lines': matched_insts,
            'candidate_pairs': candidates, 'candidate_pair_count': len(candidates),
            'scope': 'textual single-basic-block operand match; no dominance/trap/alias/cost assertion'}


def scan(root: Path, input_wat: Path, native_aot: Path, out: Path,
         upstream_sha: str) -> dict:
    if out.exists():
        raise ValueError('refusing to overwrite receipt')
    if upstream_sha != UPSTREAM:
        raise ValueError('pinned upstream revision mismatch')
    if not root.is_dir() or not input_wat.is_file() or not native_aot.is_file():
        raise ValueError('missing emitted artifacts or source WAT')
    files = sorted(p for p in root.rglob('*') if p.is_file())
    if not files or len(files) > MAX_FILES:
        raise ValueError('no CLIF files or excessive file count')
    results = []
    for path in files:
        if path.is_symlink() or not path.resolve().is_relative_to(root.resolve()):
            raise ValueError('unsafe input path')
        raw = path.read_bytes()
        if not raw or len(raw) > MAX_BYTES:
            raise ValueError('empty/oversized CLIF file')
        parsed = inspect(raw.decode('utf-8'))
        results.append({'path': path.relative_to(root).as_posix(),
                        'sha256': digest(raw), 'bytes': len(raw), **parsed})
    record = {
        'schema': SCHEMA, 'status': 'source-pinned-emitted-CLIF-candidate-observation',
        'source_commit': upstream_sha, 'wat_sha256': digest(input_wat.read_bytes()),
        'aot_sha256': digest(native_aot.read_bytes()),
        'aot_bytes': native_aot.stat().st_size,
        'scanner_sha256': digest(Path(__file__).read_bytes()),
        'results': results,
        'total_candidate_pairs': sum(r['candidate_pair_count'] for r in results),
        'limitations': [
            'Lexical CLIF observation is not ISLE rule application or a safe rewriting proof',
            'The printed CLIF optimization phase may differ from the lowering phase',
            'SSA identity alone does not establish trap ordering, dominance, or host state',
            'This records no cost savings or compiler patch',
            'No same-Wasm baseline-vs-patched compiler comparison is attempted',
        ],
    }
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(record, indent=2, sort_keys=True) + '\n')
    return record


def main() -> None:
    parser = argparse.ArgumentParser()
    for flag in ('clif-root', 'wat', 'aot', 'out'):
        parser.add_argument('--'+flag, type=Path, required=True)
    parser.add_argument('--upstream-sha', required=True)
    args = parser.parse_args()
    result = scan(args.clif_root, args.wat, args.aot, args.out, args.upstream_sha)
    print(json.dumps({'status': result['status'],
                      'candidate_pairs': result['total_candidate_pairs']}, sort_keys=True))


if __name__ == '__main__':
    main()
