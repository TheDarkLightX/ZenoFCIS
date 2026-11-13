#!/usr/bin/env python3
"""Pinned Cranelift compiler A/B reachability matrix; not a compiler-pass proof.

Generate explicit CLIF SSA graphs and independently inspect same-input output
objects before and after a STRICT live-gap experimental Wasmtime patch.
Only exact native symbol byte changes count. Control changes fail closed.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import re
import subprocess
import tempfile
from pathlib import Path

UPSTREAM = 'b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49'
BASE = [
    '    v2 = udiv v0, v1',
    '    v3 = imul v2, v1',
    '    v4 = iconst.i64 7',
    '    v5 = rotl v3, v4',
    '    v6 = isub v0, v3',
    '    v7 = bxor v2, v3',
    '    v8 = bxor v7, v5',
    '    v9 = bxor v8, v6',
    '    return v9',
]

# Both positive programs satisfy the installer's precise five-op graph.
# All negative programs deliberately violate at least one essential edge.
CASE_EDITS = {
    'positive_live_product': ('expected_match', {}),
    'positive_commuted_multiply': ('expected_match', {1: '    v3 = imul v1, v2'}),
    'negative_signed_division': ('must_not_match', {0: '    v2 = sdiv v0, v1'}),
    'negative_wrong_multiplier': ('must_not_match', {1: '    v3 = imul v2, v0'}),
    'negative_reverse_subtract': ('must_not_match', {4: '    v6 = isub v3, v0'}),
    'negative_unrelated_rotation': ('must_not_match', {3: '    v5 = rotl v2, v4'}),
    'negative_subtract_wrong_dividend': ('must_not_match', {4: '    v6 = isub v1, v3'}),
    'negative_extra_pure_gap': ('must_not_match', {4: '    v10 = iadd v3, v4\n    v6 = isub v0, v3',
                                                         8: '    v11 = bxor v9, v10\n    return v11'}),
}

def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()

def make_text(name: str, edits: dict) -> str:
    ops = [edits.get(i, line) for i, line in enumerate(BASE)]
    return (f'test verifier\nset opt_level=speed_and_size\ntarget x86_64\n\n'
            f'function %tier1_{name}(i64, i64) -> i64 {{\n'
            'block0(v0: i64, v1: i64):\n'
            + '\n'.join(ops) + '\n}\n')

def generate(out: Path) -> dict:
    if out.exists():
        raise ValueError('matrix directory exists, refusing overwrite')
    out.mkdir(parents=True)
    files = {}
    for name, (admission, edits) in CASE_EDITS.items():
        path = out / (name + '.clif')
        path.write_text(make_text(name, edits))
        files[name] = {'admission': admission, 'clif_sha256': sha(path)}
    manifest = {'schema': 'zenofcis/tier1-livegap-clif-actual-compiler-matrix/1',
                'upstream': UPSTREAM, 'cases': files,
                'meaning': 'Structural reachability via emitted code, not a VeriISLE, SSA, ABI or native-machine proof'}
    (out / 'manifest.json').write_text(json.dumps(manifest, sort_keys=True, indent=2)+'\n')
    return manifest

def exe(*args: str) -> str:
    return subprocess.run(args, check=True, capture_output=True, text=True).stdout

def profile(path: Path) -> dict:
    if not path.is_file():
        raise ValueError('missing compiler object: '+str(path))
    symbols = []
    for line in exe('nm','-S','--defined-only','--format=posix',str(path)).splitlines():
        parts = line.split()
        if len(parts) == 4 and parts[1] in ('T','t'):
            symbols.append({'name': parts[0], 'size': int(parts[3], 16)})
    if len(symbols) != 1 or symbols[0]['size'] < 1:
        raise ValueError('expected exactly one sized native text symbol: '+str(path))
    dis = exe('objdump','-d','-Mintel',str(path))
    # Select only actual decoded instruction bytes, not ASCII annotations.
    enc = re.findall(r'^\s*[0-9a-f]+:\s+((?:[0-9a-f]{2}\s+)+)\s*\S+', dis, re.M)
    if not enc:
        raise ValueError('no decoded machine instructions: '+str(path))
    code = bytes.fromhex(''.join(''.join(s.split()) for s in enc))
    return {'object_sha256': sha(path), 'symbol': symbols[0],
            'decoded_instruction_sha256': hashlib.sha256(code).hexdigest(),
            'decoded_bytes': len(code), 'decoded_instruction_count': len(enc),
            'disassembly': dis}

def analyze(src: Path, baseline: Path, patched: Path, out: Path,
            baseline_compiler: Path, patched_compiler: Path, checkout: str) -> dict:
    manifest = json.loads((src / 'manifest.json').read_text())
    if manifest.get('upstream') != UPSTREAM or set(manifest.get('cases',{})) != set(CASE_EDITS):
        raise ValueError('source manifest / upstream identity mismatch')
    if out.exists():
        raise ValueError('refusing evidence overwrite')
    if sha(baseline_compiler) == sha(patched_compiler):
        raise ValueError('identical compiler binaries do not establish a patch A/B')
    results = {}
    unexpected = []
    for name, (admission, edits) in CASE_EDITS.items():
        if (src/(name+'.clif')).read_text() != make_text(name, edits):
            raise ValueError('generated CLIF contract has changed: '+name)
        if sha(src/(name+'.clif')) != manifest['cases'][name]['clif_sha256']:
            raise ValueError('source digest differs: '+name)
        a, b = profile(baseline/(name+'.o')), profile(patched/(name+'.o'))
        changed = (a['decoded_instruction_sha256'] != b['decoded_instruction_sha256'])
        delta = a['symbol']['size'] - b['symbol']['size']
        if admission == 'must_not_match' and changed:
            unexpected.append(name)
        results[name] = {'admission_expected': admission,
                         'native_code_changed': changed,
                         'symbol_bytes_saved': delta,
                         'baseline': {k:v for k,v in a.items() if k!='disassembly'},
                         'patched': {k:v for k,v in b.items() if k!='disassembly'}}
    if unexpected:
        raise ValueError('negative-control native code changed: '+','.join(unexpected))
    report = {'schema': 'zenofcis/tier1-livegap-clif-actual-compiler-matrix/1',
              'status': 'passed-structural-negative-controls-not-compiler-proof',
              'source_checkout': checkout, 'upstream': UPSTREAM,
              'compiler_sha256': {'baseline': sha(baseline_compiler), 'patched': sha(patched_compiler)},
              'manifest_sha256': sha(src/'manifest.json'), 'results': results,
              'observed_positive_changes': sum(v['native_code_changed'] for k,v in results.items()
                                               if v['admission_expected']=='expected_match'),
              'limitations': [
                  'CLIF is directly authored; LLVM/Rust reachability comes from distinct evidence',
                  'Optimizer may rewrite generated fixtures before experimental matcher runs',
                  'A native output difference establishes reachability but not root cause or pass soundness',
                  'No independence claim for complete x86 ISA, register allocation, flags or poison',
                  'No dynamic timing or workload profitability claim',
                  'Only x86-64 speed_and_size and exactly the enumerated fixture graph shapes are tested',
              ]}
    out.parent.mkdir(parents=True,exist_ok=True)
    out.write_text(json.dumps(report, sort_keys=True, indent=2)+'\n')
    return report

def self_test() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        m = generate(Path(tmp)/'matrix')
        assert len(m['cases']) == 8
        for name, (admission, edits) in CASE_EDITS.items():
            t = make_text(name, edits)
            assert t.count('function %tier1_') == 1 and '    return ' in t
            if admission == 'expected_match':
                assert 'v2 = udiv v0, v1' in t and 'v5 = rotl v3, v4' in t
            else:
                assert t != make_text(name, {})
        try:
            generate(Path(tmp)/'matrix')
        except ValueError:
            pass
        else:
            raise AssertionError('overwrite refusal failed')
    print('8 CLIF graph variants, contract and overwrite tests passed (generation only)')

if __name__ == '__main__':
    p=argparse.ArgumentParser()
    p.add_argument('--self-test',action='store_true')
    p.add_argument('--generate',type=Path)
    p.add_argument('--analyze',action='store_true')
    p.add_argument('--src',type=Path)
    p.add_argument('--baseline',type=Path)
    p.add_argument('--patched',type=Path)
    p.add_argument('--out',type=Path)
    p.add_argument('--baseline-compiler',type=Path)
    p.add_argument('--patched-compiler',type=Path)
    p.add_argument('--checkout')
    a=p.parse_args()
    if a.self_test: self_test()
    elif a.generate: print(json.dumps(generate(a.generate),sort_keys=True))
    elif a.analyze:
        required=(a.src,a.baseline,a.patched,a.out,a.baseline_compiler,a.patched_compiler,a.checkout)
        if any(v is None for v in required): p.error('analyze requires all source, both compilers and both object directories')
        r=analyze(a.src,a.baseline,a.patched,a.out,a.baseline_compiler,a.patched_compiler,a.checkout)
        print(json.dumps({'status':r['status'],'positive_native_changes':r['observed_positive_changes']},sort_keys=True))
    else: p.error('choose --self-test, --generate or --analyze')
