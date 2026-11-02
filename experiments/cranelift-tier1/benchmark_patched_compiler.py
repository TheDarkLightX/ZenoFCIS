#!/usr/bin/env python3
"""Source- and hash-checked exploratory A/B on *actual* ISLE-patched Cranelift objects.

Not an instruction-cycle experiment, not a speedup proof. All comparisons use
identical source CLIF compiled from unmodified vs one-rule-patched Cranelift.
"""
from __future__ import annotations
import argparse
import csv
import hashlib
import io
import json
import os
import platform
import subprocess
import tempfile
from pathlib import Path
from benchmark_order_balanced import check_rows
from recheck_patched_compiler import OBJECT_HASHES, UPSTREAM, WORKFLOW, ARTIFACT, check_artifact_identity

SYMBOL='%tier1_signed_satadd'
EXPECTED_SOURCE_HASHES={
'benchmark_exact_objects.c':'f1ce8d1c798ad735256e65f3db2f23c2af743c0a4c48b4b1bdd019c0e34d941d',
'benchmark_order_balanced.c':'084c3bc8fc1e7365fa5ad4b5a41a0f4a4bfa873a5d7ca9c43f79d0ccdba967a2'}

def sha(b:bytes)->str:return hashlib.sha256(b).hexdigest()
def run(args):
    if platform.system()!='Linux' or platform.machine().lower() not in ('x86_64','amd64'):
        raise RuntimeError('Linux x86-64 SysV only')
    if not 4<=args.rounds<=100 or not 8000<=args.calls<=200_000_000 or args.calls%8:
        raise RuntimeError('invalid bounded experiment workload')
    identity=check_artifact_identity(args.artifacts)
    sources={}
    for name,expected in EXPECTED_SOURCE_HASHES.items():
        value=sha((args.source/name).read_bytes())
        if value!=expected:raise RuntimeError('source identity mismatch: '+name)
        sources[name]=value
    with tempfile.TemporaryDirectory(prefix='cranelift-patched-bench-') as tmp:
        tmp=Path(tmp)
        renamed=[]
        for name,new_symbol in [('original.o','zeno_tier1_baseline'),('patched.o','zeno_tier1_candidate')]:
            out=tmp/name
            subprocess.run(['objcopy','--redefine-sym',f'{SYMBOL}={new_symbol}',str(args.artifacts/name),str(out)],check=True,capture_output=True,text=True)
            renamed.append(str(out))
        exe=tmp/'bench'
        flags=['-O2','-std=gnu11','-Wall','-Wextra','-Werror','-fno-lto','-fno-omit-frame-pointer']
        cmd=['gcc',*flags,str(args.source/'benchmark_order_balanced.c'),*renamed,'-o',str(exe)]
        subprocess.run(cmd,check=True,capture_output=True,text=True)
        allowed=sorted(os.sched_getaffinity(0)) if hasattr(os,'sched_getaffinity') else []
        pinned=None
        if args.pin_cpu and allowed:
            pinned=allowed[0];os.sched_setaffinity(0,{pinned})
        output=subprocess.run([str(exe),str(args.calls),str(args.rounds)],capture_output=True,text=True,check=True).stdout
    rows=list(csv.DictReader(io.StringIO(output)))
    summary=check_rows(rows,args.calls,args.rounds)
    cpu=next((s.split(':',1)[1].strip() for s in Path('/proc/cpuinfo').read_text().splitlines() if s.startswith('model name')),'unknown')
    return {'schema':'zeno-fcis/2.8-isle-patched-compiler-balanced-call-screen/1',
            'status':'exploratory-actual-compiler-patch-no-qualified-speed',
            'upstream_wasmtime_commit':UPSTREAM,
            'experiment_source_commit':identity['source_experiment_commit'],
            'workflow':WORKFLOW,'workflow_artifact_id':ARTIFACT,
            'object_sha256':OBJECT_HASHES,
            'benchmark_C_source_sha256':sources,
            'host':{'system':platform.system(),'cpu':cpu,'kernel':platform.release(),
                    'pinned_cpu':pinned,'allowed_cpus':allowed},
            'clock':'CLOCK_MONOTONIC_RAW','measurement_order':'ABBA/BAAB rotating modes',
            'calls_per_sample':args.calls,'rounds':args.rounds,
            'raw_samples':rows,'summary':summary,
            'provenance':'same pinned scalar CLIF; before versus after applied experimental mid-end ISLE rule',
            'nonclaims':['not VeriISLE rule chain proof','not a speedup or slowdown claim',
                         'virtualized host; timing variance','call/loop overhead, not isolated latency/throughput',
                         'not a multi-target or full Wasmtime application test']}

if __name__=='__main__':
    p=argparse.ArgumentParser()
    p.add_argument('--artifacts',type=Path,required=True)
    p.add_argument('--source',type=Path,required=True)
    p.add_argument('--out',type=Path,required=True)
    p.add_argument('--rounds',type=int,default=16)
    p.add_argument('--calls',type=int,default=1600000)
    p.add_argument('--pin-cpu',action='store_true')
    args=p.parse_args()
    if args.out.exists():p.error('fresh output path required')
    result=run(args)
    args.out.write_text(json.dumps(result,indent=2,sort_keys=True)+"\n")
    print(json.dumps({'status':result['status'],'observed_paired_ratios':
        {k:round(v['median_balanced_candidate_over_baseline_ratio'],4) for k,v in result['summary'].items()},
        'range':{k:[round(v['min_paired_ratio'],3),round(v['max_paired_ratio'],3)]
                 for k,v in result['summary'].items()}},sort_keys=True))
