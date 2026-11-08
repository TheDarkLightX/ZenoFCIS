#!/usr/bin/env python3
"""Independent x86-64 SysV native dual-result DIV feasibility witness.

This does *not* patch Cranelift. Distinct handwritten .S kernels, not identical
Wasm or CLIF; the measurements may not transfer to a compiler optimization.
"""
import argparse
import csv
import hashlib
import json
from pathlib import Path
import platform
import re
import resource
import statistics
import subprocess

SCHEMA = 'zeno-fcis/tier1-x86-dual-div-native-standalone/1'
SOURCE_FILES = ('dual_div.S', 'verify_dual_div.c')
FLAGS = ['-std=c11', '-O2', '-Wall', '-Wextra', '-Werror', '-fno-pie', '-no-pie']

def run(command, **kwargs):
    return subprocess.run(command, text=True, capture_output=True, check=True, **kwargs).stdout

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def no_cores():
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))

def collect(src, out, calls):
    if platform.machine() != 'x86_64':
        raise ValueError('x86-64 only; do not execute native binaries on another target')
    if not 10000 <= calls <= 10000000:
        raise ValueError('out-of-range call budget')
    out.mkdir(parents=True, exist_ok=True)
    binary=out/'dual_result_runner'
    sources=[src/x for x in SOURCE_FILES]
    for source in sources:
        if not source.is_file(): raise ValueError(f'missing {source}')
    run(['cc', *FLAGS, *map(str, sources), '-o', str(binary)])
    stdout=run([str(binary), str(calls)], preexec_fn=no_cores)
    disasm=run(['objdump','-d',str(binary)])
    nm=run(['nm','-S','--defined-only',str(binary)])
    (out/'raw_timing.csv').write_text(stdout)
    (out/'disassembly.txt').write_text(disasm)
    (out/'symbols.txt').write_text(nm)
    lines=stdout.splitlines()
    facts={k:v for k,v in (line.split('=',1) for line in lines[:5] if '=' in line)}
    if facts.get('schema') != SCHEMA or facts.get('finite_nonzero_pairs') != '100072' or facts.get('zero_divisor_checks') != '2 SIGFPE, fork-isolated' or facts.get('calls_per_sample') != str(calls):
        raise ValueError(f'fixture self-check failed: {facts}')
    if lines[5] != 'block,position,variant,ns':
        raise ValueError('malformed sample heading')
    samples=list(csv.DictReader(lines[5:]))
    if len(samples)!=64:raise ValueError('expected 64 balanced observations')
    per_block={}
    for i,row in enumerate(samples):
        block=int(row['block']);position=int(row['position']);elapsed=int(row['ns'])
        if block!=i//4 or position!=i%4 or elapsed <= 0:
            raise ValueError('invalid or incomplete timing position')
        wanted=('one','two','two','one') if block%2 else ('two','one','one','two')
        if row['variant']!=wanted[position]:raise ValueError('unbalanced observation order')
        per_block.setdefault(block,{'one':[],'two':[]})[row['variant']].append(elapsed)
    ratios=[sum(v['one'])/sum(v['two']) for v in per_block.values()]
    sizes={}
    for line in nm.splitlines():
        m=re.fullmatch(r'([0-9a-f]+) ([0-9a-f]+) [Tt] (tier1_(?:one|two)_div)',line.strip())
        if m:sizes[m.group(3)]=int(m.group(2),16)
    if set(sizes)!={'tier1_one_div','tier1_two_div'}:raise ValueError('missing native symbols')
    div_counts={}
    instruction_counts={}
    for name in sizes:
        data=run(['objdump','-d','--disassemble='+name,str(binary)])
        parsed=[line for line in data.splitlines() if re.match(r'^\s*[0-9a-f]+:\s+[0-9a-f ]+\s+',line)]
        instruction_counts[name]=len(parsed)
        div_counts[name]=sum(bool(re.search(r'\bdiv\s',line)) for line in parsed)
    if (div_counts['tier1_one_div'],div_counts['tier1_two_div']) != (1,2):
        raise ValueError(f'div count violated: {div_counts}')
    if sizes['tier1_one_div'] >= sizes['tier1_two_div']:
        raise ValueError('dual-result candidate is not smaller in this native fixture')
    result={
        'schema':SCHEMA,
        'status':'finite-native-fixture-qualified; no Cranelift source rewrite',
        'semantics':{
            'input':'SysV x86-64 uint64_t (rdi=x, rsi=y)',
            'value_domain':'y != 0, all uint64_t x,y bit patterns in declared domain',
            'function':'(x/y) XOR rotl64(x%y,17)',
            'zero_divisor':'SIGFPE checked in isolated child for both kernels',
            'division_overflow':'impossible for unsigned divq with RDX cleared and divisor nonzero',
            'signedness':'unsigned only; signed overflow, idiv, poison, wasm traps not verified',
            'flags':'x86 DIV flags undefined; no flags consumed; caller-visible flags not ABI-preserved',
            'side_effects':'no memory writes inside asm kernels; return value in rax',
        },
        'toolchain':{'cc':run(['cc','--version']).splitlines()[0], 'objdump':run(['objdump','--version']).splitlines()[0], 'flags':FLAGS, 'host':platform.platform()},
        'pins':{'source_sha256':{x:sha(src/x) for x in SOURCE_FILES}, 'binary_sha256':sha(binary), 'disassembly_sha256':sha(out/'disassembly.txt')},
        'cost':{'unit':'function-symbol bytes, x86-64 executable', 'bytes':sizes, 'div_instructions':div_counts, 'decoded_instructions':instruction_counts, 'bytes_saved':sizes['tier1_two_div']-sizes['tier1_one_div']},
        'finite_execution':{'pairs_per_kernel':100072,'zero_divisor_SIGFPE_checks':2,'reference':'separate C unsigned / and % oracle; native execution'},
        'exploratory_timing':{'host_cpu':next((l.split(':',1)[1].strip() for l in Path('/proc/cpuinfo').read_text().splitlines() if l.startswith('model name')), 'unknown') if Path('/proc/cpuinfo').exists() else 'unknown', 'calls_per_sample':calls,'balanced_blocks':16,'samples':64,'median_one_over_two_ratio':statistics.median(ratios),'min_block_ratio':min(ratios),'max_block_ratio':max(ratios),'blocks_favoring_one':sum(x<1 for x in ratios),'qualifier':'single-host standalone assembly microbenchmark, not a Wasmtime throughput estimate'},
        'nonclaims':['not produced by ZenoFCIS search','not a Cranelift compiler patch','not identical source-Wasm compiler comparison','not a compiler-pass proof','does not establish benefit under register pressure or all hardware']
    }
    (out/'receipt.json').write_text(json.dumps(result,sort_keys=True,indent=2)+'\n')
    return result

if __name__=='__main__':
    p=argparse.ArgumentParser()
    p.add_argument('--source-dir',type=Path,default=Path(__file__).resolve().parent)
    p.add_argument('--out',type=Path,required=True)
    p.add_argument('--calls',type=int,default=100000)
    a=p.parse_args()
    o=collect(a.source_dir,a.out,a.calls)
    print(json.dumps({'status':o['status'],'cost':o['cost'],'timing':o['exploratory_timing']},sort_keys=True))
