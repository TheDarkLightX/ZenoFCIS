#!/usr/bin/env python3
"""Independently re-execute exact unmodified/patched Cranelift ELF objects.

This is bounded native finite testing, not an x86 ISA, ABI or VeriISLE proof.
The source is the same signed i64 saturating-add CLIF function, compiled by
unmodified versus single-rule-patched Cranelift at an identical pinned revision.
"""
from __future__ import annotations
import argparse
import ctypes
import hashlib
import json
import platform
import random
import subprocess
import tempfile
from pathlib import Path

OBJECT_HASHES = {
    'original.o': '1d3b385c60fac3f0687d1b71d44a085beb09cbbeb8078510e7aebf334daf6880',
    'patched.o': 'affc09034d1bfcbff3c8c2c69913dc2da8935f39009daa97842d18ef0db53d90',
}
UPSTREAM = 'b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49'
SOURCE_EXPERIMENT = 'e9d97eb758d885cc242999991b7cda3b1eae94c8'
WORKFLOW = 'https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41382217051'
ARTIFACT = 13182951192
SYMBOL = '%tier1_signed_satadd'
SMIN, SMAX = -(1 << 63), (1 << 63) - 1
EDGES = [SMIN,SMIN+1,SMIN+2,SMIN+3,SMIN+7,SMIN+15,SMIN+16,SMIN+31,SMIN+63,SMIN+127,SMIN+255,SMIN+256,
         -(1<<62),-100000,-1024,-255,-127,-63,-31,-17,-16,-15,-8,-7,-3,-2,-1,
         0,1,2,3,7,8,15,16,17,31,63,127,255,256,1024,100000,(1<<62),
         SMAX-256,SMAX-255,SMAX-127,SMAX-63,SMAX-31,SMAX-16,SMAX-15,SMAX-7,SMAX-3,SMAX-2,SMAX-1,SMAX]
RANDOM_SEED = 0xC0FFEEFEB2026

def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()

def refuse(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)

def check_artifact_identity(folder: Path) -> dict:
    report = json.loads((folder/'patched-cost.json').read_text())
    patch = json.loads((folder/'patch-receipt.json').read_text())
    proof = json.loads((folder/'rule-proof.json').read_text())
    refuse(report['schema'] == 'zeno-fcis/cranelift-tier1-patched-compiler-cost/1', "artifact evidence identity")
    refuse(report['status'] == 'measured-patched-compiler-a-b', "artifact evidence identity")
    refuse(report['upstream_commit'] == patch['upstream_commit'] == proof['upstream_commit'] == UPSTREAM, "artifact evidence identity")
    refuse(report['rule_sha256'] == patch['rule_sha256'] == proof['rule_sha256'], "artifact evidence identity")
    refuse(patch['status'] == 'source-pinned-patch-applied', "artifact evidence identity")
    refuse(proof['status'] == 'unsat-counterexample-query', "artifact evidence identity")
    refuse(report['patch_receipt_sha256'] == sha((folder/'patch-receipt.json').read_bytes()), "artifact evidence identity")
    refuse(report['qf_bv_proof_sha256'] == sha((folder/'rule-proof.json').read_bytes()), "artifact evidence identity")
    refuse(report['whole_symbol_delta_old_minus_patched'] == 17, "artifact evidence identity")
    for name, key, sz in [('original.o','baseline',56),('patched.o','patched',39)]:
        refuse(sha((folder/name).read_bytes()) == OBJECT_HASHES[name], "artifact evidence identity")
        refuse(report[key]['object_sha256'] == OBJECT_HASHES[name], "artifact evidence identity")
        refuse(report[key]['function']['bytes'] == sz, "artifact evidence identity")
        nm = subprocess.run(['nm','-S','--defined-only',str(folder/name)],
                            check=True,text=True,capture_output=True).stdout
        size = int(next(line.split()[1] for line in nm.splitlines() if SYMBOL in line),16)
        refuse(size == sz, "artifact evidence identity")
    return {'upstream_commit':UPSTREAM,'source_experiment_commit':SOURCE_EXPERIMENT,
            'rule_sha256': report['rule_sha256'], 'object_sha256':OBJECT_HASHES,
            'rule_receipt_sha256':sha((folder/'rule-proof.json').read_bytes()),
            'compiler_patch_receipt_sha256':sha((folder/'patch-receipt.json').read_bytes()),
            'cost_receipt_sha256':sha((folder/'patched-cost.json').read_bytes())}

def run(folder: Path, random_pairs: int):
    if platform.system()!='Linux' or platform.machine().lower() not in ('x86_64','amd64'):
        raise RuntimeError('Linux x86_64 SysV required')
    identity = check_artifact_identity(folder)
    with tempfile.TemporaryDirectory(prefix='tier1-patched-recheck-') as d:
        funcs = {}
        for name in OBJECT_HASHES:
            so = Path(d)/(name+'.so')
            subprocess.run(['gcc','-shared','-o',str(so),str(folder/name)],check=True,capture_output=True,text=True)
            library=ctypes.CDLL(str(so))
            fn=getattr(library,SYMBOL)
            fn.argtypes=(ctypes.c_int64,ctypes.c_int64)
            fn.restype=ctypes.c_int64
            funcs[name]=(library,fn)
        rng=random.Random(RANDOM_SEED)
        def cases():
            for x in EDGES:
                for y in EDGES:
                    yield x,y
            for _ in range(random_pairs):
                yield rng.randrange(SMIN,SMAX+1),rng.randrange(SMIN,SMAX+1)
        for i,(x,y) in enumerate(cases()):
            want=max(SMIN,min(SMAX,x+y))
            for name,(_lib,fn) in funcs.items():
                got=fn(x,y)
                if got != want:
                    raise AssertionError(f'native mismatch case={i} name={name} x={x} y={y} expected={want} actual={got}')
    return {'schema':'zeno-fcis/2.8-isle-patched-native-recheck/1',
            'status':'passed-finite-native-oracle', **identity,
            'workflow':WORKFLOW,'workflow_artifact_id':ARTIFACT,
            'platform':{'system':platform.system(),'machine':platform.machine()},
            'seed_hex':hex(RANDOM_SEED),'boundary_grid_pairs':len(EDGES)**2,
            'pseudorandom_input_pairs':random_pairs,'tested_pairs_per_object':len(EDGES)**2+random_pairs,
            'semantic_contract':'signed 64-bit saturating add of two i64 values; SysV caller observed result',
            'original_symbol_bytes':56,'patched_symbol_bytes':39,
            'limits':['finite native testing, not exhaustive SMT or machine-code semantics proof',
                      'independent QF_BV proves only abstract clamp identity',
                      'no x86 flag or undefined/poison/liveness proof',
                      'not a VeriISLE chain proof',
                      'no speed or throughput conclusion','no cross-target qualification']}

if __name__=='__main__':
    p=argparse.ArgumentParser()
    p.add_argument('--dir',type=Path,default=Path(__file__).resolve().parent)
    p.add_argument('--random-pairs',type=int,default=250000)
    p.add_argument('--out',type=Path)
    a=p.parse_args()
    if not 0 <= a.random_pairs <= 2000000:p.error('invalid random pairs')
    if a.out is not None and a.out.exists():p.error('fresh receipt path required')
    report=run(a.dir,a.random_pairs)
    output=json.dumps(report,indent=2,sort_keys=True)+"\n"
    if a.out:
        a.out.parent.mkdir(parents=True,exist_ok=True)
        a.out.write_text(output)
    print(output)
