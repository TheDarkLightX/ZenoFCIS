#!/usr/bin/env python3
"""Independently audit archived identical-CLIF dual-result compiler objects.

Requires the published GitHub Actions artifact ZIP and local x86-64 Linux
binutils/GCC. No build-tool speed claims, compiler-pass proof, or full-AOT claim.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import zipfile

EXPECTED_ARTIFACT_SHA256='c7b6835c34f6e020d09b9bdecd897a70f4f00cc489e84f284b14a4f5c69acd9a'
EXPECTED_ZENO='0cd02a2aff0fe37ebeba7503510b2a7e55299e7e'
EXPECTED_WASMTIME='b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49'
EXPECTED_OBJECTS={
    'original':'ba5f6e72efa5675fa7882d3db05043bbeb2440020a17981d5838d6d75fc7e217',
    'patched':'111548052bfa67b8431373f86ce5fac2cd97ee94ccb9871972fc9e462f382599',
}
PREFIX='tier1-udivrem-combiner/'

def sha(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()

def cmd(*args):
    return subprocess.check_output(args,text=True,stderr=subprocess.PIPE).strip()

def decode_object(obj):
    names=cmd('nm','-S','--defined-only',str(obj)).splitlines()
    sizes=[int(m.group(1),16) for line in names
           if (m:=re.search(r'\s([0-9a-fA-F]+)\sT\s%tier1_udivrem$',line))]
    if len(sizes)!=1: raise AssertionError('expected exactly one named function')
    disasm=cmd('objdump','-d',str(obj))
    body=disasm.split('<%tier1_udivrem>:',1)
    if len(body)!=2: raise AssertionError('named function disassembly absent')
    ins=[]
    for line in body[1].splitlines():
        match=re.match(r'^\s*[0-9a-f]+:\s+((?:[0-9a-f]{2}\s+)+)([a-zA-Z]\w*)\b',line)
        if match: ins.append((len(match.group(1).split()),match.group(2)))
        elif ins: break
    if not ins or ins[-1][1]!='ret':raise AssertionError('expected straight-line return')
    total=sum(n for n,_ in ins)
    if total!=sizes[0]:raise AssertionError('function-symbol size != decoded instructions')
    return {'function_symbol_bytes':sizes[0], 'decoded_instructions':len(ins),
            'static_div_instructions':sum(op=='div' for _,op in ins),
            'bytes_through_ret':total,'whole_object_bytes':obj.stat().st_size}

def run(zippath,harness):
    if os.uname().machine!='x86_64':raise RuntimeError('native host must be x86_64')
    if sha(zippath)!=EXPECTED_ARTIFACT_SHA256:raise ValueError('artifact ZIP digest mismatch')
    with tempfile.TemporaryDirectory(prefix='tier1-adjacent-audit-') as d:
        root=Path(d)
        with zipfile.ZipFile(zippath) as z:
            for name in z.namelist():
                if name.startswith('/') or '..' in Path(name).parts:raise ValueError('unsafe archive entry')
            z.extractall(root)
        p=root/PREFIX
        if (p/'zenofcis-head.txt').read_text().strip()!=EXPECTED_ZENO:
            raise ValueError('ZenoFCIS revision mismatch')
        for name in ('original-head.txt','patched-head.txt'):
            if (p/name).read_text().strip()!=EXPECTED_WASMTIME:
                raise ValueError('compiler revision mismatch')
        installer=json.loads((p/'installer.json').read_text())
        if (installer['source_commit']!=EXPECTED_WASMTIME
            or installer['opt_gate']!='x86_64 and opt_level != None'):
            raise ValueError('installer pin/scope mismatch')
        costs={}
        for variant,digest in EXPECTED_OBJECTS.items():
            obj=p/f'{variant}.o'
            if sha(obj)!=digest:raise ValueError(f'{variant} object digest mismatch')
            costs[variant]=decode_object(obj)
            archived=(p/f'{variant}-objdump.txt').read_text()
            actual=cmd('objdump','-dr',str(obj))
            # Normalize absolute-vs-relative filename header; require entire instruction body match.
            if (archived.split('Disassembly of section .text:',1)[-1].strip()
                !=actual.split('Disassembly of section .text:',1)[-1].strip()):
                raise ValueError('archived disassembly does not agree with independent objdump')
        b,c=costs['original'],costs['patched']
        if (b['function_symbol_bytes'],c['function_symbol_bytes'])!=(40,25):
            raise AssertionError('unexpected function size')
        if (b['decoded_instructions'],c['decoded_instructions'])!=(15,10):
            raise AssertionError('unexpected instruction count')
        if (b['static_div_instructions'],c['static_div_instructions'])!=(2,1):
            raise AssertionError('unexpected static division counts')
        # Rename ONLY exported ELF symbol names; compiled instruction bytes unchanged.
        obj1=p/'original_renamed.o';obj2=p/'patched_renamed.o'
        cmd('objcopy','--redefine-sym','%tier1_udivrem=tier1_original',str(p/'original.o'),str(obj1))
        cmd('objcopy','--redefine-sym','%tier1_udivrem=tier1_patched',str(p/'patched.o'),str(obj2))
        exe=p/'native_witness'
        cmd('gcc','-O2','-Wall','-Wextra',str(harness),str(obj1),str(obj2),'-o',str(exe))
        output=cmd(str(exe))
        if output!='native_nonzero_input_pairs=100240; per_variant_executions=100240; original_divzero_SIGFPE=1; patched_divzero_SIGFPE=1':
            raise AssertionError('native oracle/trap witness check failed: '+output)
        return {'classification':'independent finite native execution and cost remeasurement',
                'ci_run':42558925610,'artifact_sha256':EXPECTED_ARTIFACT_SHA256,
                'zenofcis':EXPECTED_ZENO,'upstream':EXPECTED_WASMTIME,
                'identical_clif':True,'identical_wasm':False,
                'cost':costs,'saved_function_bytes':b['function_symbol_bytes']-c['function_symbol_bytes'],
                'saved_decoded_instructions':b['decoded_instructions']-c['decoded_instructions'],
                'saved_static_divs':b['static_div_instructions']-c['static_div_instructions'],
                'nonzero_pairs_per_variant':100240,'total_native_nonzero_calls':200480,
                'zero_divisor_SIGFPE_checks':2,
                'status':'passed',
                'nonclaims':['not machine-code formal proof','not full compiler-pass verification',
                             'not target-portable','not identical-Wasm qualification',
                             'not measured throughput or speedup','not whole Wasmtime AOT footprint saving']}

def main():
    p=argparse.ArgumentParser()
    p.add_argument('--artifact-zip',type=Path,required=True)
    p.add_argument('--out',type=Path)
    a=p.parse_args()
    result=run(a.artifact_zip.resolve(),Path(__file__).with_name('native_witness_adjacent_28.c').resolve())
    text=json.dumps(result,indent=2,sort_keys=True)+'\n'
    if a.out:a.out.write_text(text)
    print(text)

if __name__=='__main__':main()
