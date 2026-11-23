#!/usr/bin/env python3
"""Independent artifact-only check of pinned Wasmtime signed saturating add A/B.

Usage: python3 audit_satadd_currentpin.py ARTIFACT_DIRECTORY --out audit.json
Requires clang, readelf, objdump and Linux x86_64. Never patches upstream.
The solver receipt is checked for provenance, NOT re-proved by this script.
"""
import argparse
import ctypes
import hashlib
import json
import platform
import random
import re
import subprocess
import tempfile
from pathlib import Path

LIMIT_MIN = -(1 << 63)
LIMIT_MAX = (1 << 63) - 1
NAME = '%tier1_signed_satadd'


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def cmd(*argv):
    return subprocess.run(argv, check=True, text=True, capture_output=True).stdout


def independent_object_size(path):
    lines = cmd('readelf', '-Ws', str(path)).splitlines()
    hits = [l.split() for l in lines if NAME in l and 'FUNC' in l]
    assert len(hits) == 1, hits
    return int(hits[0][2])


def independently_decoded_prefix(path):
    dis = cmd('objdump', '-dw', str(path))
    inside = False
    count = 0
    total = 0
    for line in dis.splitlines():
        if f'<{NAME}>:' in line:
            inside = True
            continue
        if not inside:
            continue
        if re.match(r'^\s*[0-9a-f]+ <.*>:$', line):
            break
        m = re.match(r'^\s*([0-9a-f]+):\s*((?:[0-9a-f]{2}\s+)+)\s*(.*)$', line)
        if not m:
            continue
        raw = m.group(2).split()
        count += 1
        total += len(raw)
        if re.search(r'\bretq?\b', m.group(3)):
            return total, count
    raise AssertionError('function RET not decoded')


def signed_satadd(a, b):
    return min(LIMIT_MAX, max(LIMIT_MIN, a+b))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('artifact_directory', type=Path)
    ap.add_argument('--out', type=Path, required=True)
    a = ap.parse_args()
    d = a.artifact_directory
    receipt = json.loads((d / 'qualification.json').read_text())
    proof = json.loads((d / 'proof.json').read_text())
    patch = json.loads((d / 'patch.json').read_text())
    assert platform.system() == 'Linux' and platform.machine() == 'x86_64', platform.platform()
    assert receipt['upstream_commit'] == patch['upstream_commit'] == proof['upstream_commit'] == (d/'wasmtime-commit.txt').read_text().strip()
    assert receipt['upstream_source_blob'] == patch['original_blob'] == proof['upstream_source_blob']
    assert proof['scope'] == 'pure signed-i64 sign clamp bits, full 2^64 input domain'
    assert proof['status'] == 'unsat' and proof['logic'] == 'QF_BV'
    assert sha(d/'proof.json') == receipt['proof_sha256']
    assert sha(d/'patch.json') == receipt['patch_receipt_sha256']
    assert sha(d/'satadd.clif') == receipt['source_fixture_sha256'] == proof['clif_sha256']
    assert sha(d/'experimental-isle.patch') != sha(d/'satadd.clif')
    objects = [('baseline','pristine'),('experimental','experimental')]
    assembled = {}
    with tempfile.TemporaryDirectory(prefix='tier1-abi-') as tmp:
        for key, prefix in objects:
            path = d / f'{prefix}.o'
            assert sha(path) == receipt[key]['object_sha256']
            symbol_bytes = independent_object_size(path)
            decoded_bytes, instructions = independently_decoded_prefix(path)
            assert symbol_bytes == receipt[key]['function']['bytes']
            assert decoded_bytes == receipt[key]['instruction_bytes_through_ret']
            assert instructions == receipt[key]['instructions_through_ret']
            out_so = Path(tmp) / f'{prefix}.so'
            cmd('clang', '-shared', '-o', str(out_so), str(path))
            lib = ctypes.CDLL(str(out_so))
            fn = getattr(lib, NAME)
            fn.argtypes = [ctypes.c_int64, ctypes.c_int64]
            fn.restype = ctypes.c_int64
            assembled[key] = {'function': fn, 'object_sha256': sha(path), 'symbol_bytes': symbol_bytes, 'through_ret_bytes': decoded_bytes, 'through_ret_instructions': instructions}

        edges = [LIMIT_MIN, LIMIT_MIN+1, LIMIT_MIN+2, LIMIT_MIN+3, LIMIT_MIN//2, -(1<<62), -(1<<32), -(1<<31), -65536, -2, -1, 0, 1, 2, 65536, 1<<31, 1<<32, 1<<62, LIMIT_MAX-3, LIMIT_MAX-2, LIMIT_MAX-1, LIMIT_MAX]
        pairs = [(a,b) for a in edges for b in edges]
        rng = random.Random(20261123)
        pairs.extend((rng.randrange(LIMIT_MIN,LIMIT_MAX+1), rng.randrange(LIMIT_MIN,LIMIT_MAX+1)) for _ in range(100000))
        for j, (x,y) in enumerate(pairs):
            expect = signed_satadd(x, y)
            for key, record in assembled.items():
                actual = record['function'](x,y)
                assert actual == expect, {'index':j, 'version':key,'x':x,'y':y,'expected':expect,'actual':actual}
        b = assembled['baseline']
        e = assembled['experimental']
        assert b['symbol_bytes']-e['symbol_bytes'] == receipt['full_symbol_byte_delta']
        assert b['through_ret_bytes']-e['through_ret_bytes'] == receipt['executable_prefix_byte_delta']
        out = {
            'schema':'tier1-satadd-artifact-independent-native-audit/1',
            'source':'https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/45107263287/artifacts/14792876411',
            'source_commit':(d/'zenofcis-commit.txt').read_text().strip(),
            'wasmtime_commit':(d/'wasmtime-commit.txt').read_text().strip(),
            'upstream_source_blob':receipt['upstream_source_blob'],
            'objects':{key:{k:v for k,v in rec.items() if k!='function'} for key,rec in assembled.items()},
            'sizes':{'whole_elf_symbol_byte_delta':b['symbol_bytes']-e['symbol_bytes'], 'decoded_bytes_through_first_ret_delta':b['through_ret_bytes']-e['through_ret_bytes']},
            'verification':{
                'method':'local link+ctypes native execution via SysV x86_64 ABI, independent Python unlimited-precision signed saturation',
                'boundary_grid_pairs':len(edges)**2,
                'pseudorandom_pairs':100000,
                'pairs_per_object':len(pairs),
                'total_native_calls':len(pairs)*2,
                'failures':0,
                'smt': 'CI unsat receipt pinned to pure sign-clamp QF_BV; receipt hash checked but solver NOT rerun',
                'unproven':['all 2^128 saturating-add inputs under native machine semantics','all ISLE rule chains','non-x86 targets','compiler ABI/flags preservation in all contexts','undefined/poison','runtime speedup','whole-program size']},
            'target':'linux x86_64 host (not a performance measurement)',
            'artifact_members_sha256':{p.name:sha(p) for p in d.iterdir() if p.is_file() and p.name not in ('independent_audit_20261123.json',) and p.suffix not in ('.so',)},
            'result':'passed'
        }
        a.out.parent.mkdir(parents=True, exist_ok=True)
        a.out.write_text(json.dumps(out,indent=2,sort_keys=True)+'\n')
        print(json.dumps({key:out[key] for key in ['result','source_commit','wasmtime_commit','sizes']},sort_keys=True))
        print('native calls:',out['verification']['total_native_calls'])

if __name__=='__main__':main()
