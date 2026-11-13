#!/usr/bin/env python3
"""Independently execute two exact pinned CLIF object sets against a C reference.

Finite, x86-64 SysV unsigned-i64 value checks only; no SMT/ISA/pass proof.
Excludes zero-divisor traps (and the signed min/-1 exceptional pair).
The auditor never imports a Cranelift model or trusts the compiler under test.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import subprocess
import tempfile

CASES = (
    'positive_quotient_remainder',
    'negative_reverse_order',
    'negative_signed_quotient',
    'negative_other_dividend',
    'negative_other_divisor',
    'negative_not_remainder',
    'negative_double_quotient',
    'exploratory_elidable_iconst_gap',
)


def digest(p: pathlib.Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def source_c() -> str:
    declarations = '\n'.join(
        f'extern uint64_t {which}_{name}(uint64_t, uint64_t);'
        for name in CASES for which in ('baseline', 'patched'))
    entries = ',\n'.join(
        f'{{"{name}", {i}, baseline_{name}, patched_{name}}}'
        for i, name in enumerate(CASES))
    return (r'''
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
typedef uint64_t (*fptr)(uint64_t,uint64_t);
typedef struct { const char *name; int kind; fptr baseline, patched; } Case;
''' + declarations + '\nstatic const Case cases[] = {\n' + entries + '\n};\n' + r'''
static uint64_t rng=UINT64_C(0x243f6a8885a308d3);
static uint64_t next_random(void) {
    rng^=rng>>12; rng^=rng<<25; rng^=rng>>27;
    return rng*UINT64_C(2685821657736338717);
}
static uint64_t reference(int kind,uint64_t x,uint64_t y) {
    uint64_t q=x/y, r=x%y;
    switch(kind) {
        case 0: case 1: return q^r;
        case 2: return (uint64_t)(((int64_t)x)/((int64_t)y))^r;
        case 3: case 4: return q;
        case 5: return q^(x-y);
        case 6: return 0;
        case 7: return q^r^UINT64_C(9);
        default: abort();
    }
}
static int check(const Case *c,uint64_t x,uint64_t y,
                 unsigned long long *good,unsigned long long *excluded) {
    if (y==0 || (c->kind==4 && x==0) ||
        (c->kind==2 && x==UINT64_C(0x8000000000000000) && y==UINT64_MAX)) {
        ++*excluded;
        return 0;
    }
    uint64_t expect=reference(c->kind,x,y);
    uint64_t a=c->baseline(x,y), b=c->patched(x,y);
    if (a!=expect || b!=expect) {
        fprintf(stderr,"MISMATCH %s x=%" PRIu64 " y=%" PRIu64
                       " expected=%" PRIu64 " baseline=%" PRIu64
                       " patched=%" PRIu64 "\n",c->name,x,y,expect,a,b);
        return 1;
    }
    ++*good;
    return 0;
}
int main(void) {
    const uint64_t boundary[]={0,1,2,3,7,8,9,63,64,65,127,128,255,256,257,
        UINT64_C(0x7fffffff),UINT64_C(0x80000000),
        UINT64_C(0x7fffffffffffffff),UINT64_C(0x8000000000000000),
        UINT64_MAX-1,UINT64_MAX};
    size_t n=sizeof(boundary)/sizeof(boundary[0]);
    unsigned long long total=0, excluded=0;
    for (size_t i=0;i<sizeof(cases)/sizeof(cases[0]);i++) {
        unsigned long long good=0, skipped=0;
        for (size_t x=0;x<n;x++) for (size_t y=0;y<n;y++)
            if(check(&cases[i],boundary[x],boundary[y],&good,&skipped)) return 1;
        for (size_t t=0;t<30000;t++)
            if(check(&cases[i],next_random(),next_random(),&good,&skipped)) return 1;
        if (good<30000) return 2;
        printf("%s verified=%llu excluded=%llu\n",cases[i].name,good,skipped);
        total+=good; excluded+=skipped;
    }
    printf("TOTAL cases=8 verified=%llu excluded=%llu reference=independent-C\n",
           total,excluded);
    return 0;
}
''')


def verify(baseline: pathlib.Path, patched: pathlib.Path, out: pathlib.Path) -> dict:
    with tempfile.TemporaryDirectory(prefix='tier1-pair-native-') as tmp:
        work=pathlib.Path(tmp)
        inputs={}
        objects=[]
        for name in CASES:
            inputs[name]={}
            for tag,root in (('baseline',baseline),('patched',patched)):
                src=root/(name+'.o')
                if not src.is_file():
                    raise ValueError('missing measured object: '+str(src))
                dst=work/(tag+'_'+name+'.o')
                subprocess.run(['objcopy','--redefine-sym',
                                '%tier1_'+name+'='+tag+'_'+name,
                                str(src),str(dst)],check=True)
                inputs[name][tag+'_sha256']=digest(src)
                objects.append(dst)
        code=work/'reference.c'
        code.write_text(source_c())
        binary=work/'reference'
        subprocess.run(['gcc','-std=c11','-O2','-fno-strict-overflow',
                        '-Wall','-Wextra','-Werror',str(code),
                        *(str(p) for p in objects),'-o',str(binary)],check=True)
        proc=subprocess.run([str(binary)],check=True,capture_output=True,text=True)
        lines=proc.stdout.splitlines()
        if len(lines)!=9 or not lines[-1].startswith('TOTAL cases=8 '):
            raise ValueError('native audit output unexpectedly incomplete')
        result={
            'schema':'zenofcis/tier1-adjacent-udivrem-native-independent-check/1',
            'status':'passed-finite-native-values-not-a-complete-proof',
            'checker':'independently compiled GCC C reference; exact object symbol renaming only',
            'reference_source_sha256':digest(code),
            'object_sha256':inputs,
            'stdout':lines,
            'excluded_domain':'y==0; other-divisor fixture also x==0; signed quotient fixture INT64_MIN/-1',
            'assumptions':'x86-64 Linux SysV, two-complement int64 bit reinterpretation',
            'limitations':[
                'finite execution; no exhaustive u64 proof, SMT proof or complete x86 ISA verification',
                'division-by-zero trap/metadata and signed overflow behavior not exercised',
                'no whole compiler-pass, SSA, poison, flags or register-liveness proof',
                'static savings are distinct from runtime throughput and whole-AOT size',
            ],
        }
        if out.exists():
            raise ValueError('refusing to overwrite an existing native audit receipt')
        out.parent.mkdir(parents=True,exist_ok=True)
        out.write_text(json.dumps(result,indent=2,sort_keys=True)+'\n')
        print('\n'.join(lines))
        return result


if __name__=='__main__':
    ap=argparse.ArgumentParser()
    ap.add_argument('--baseline',required=True,type=pathlib.Path)
    ap.add_argument('--patched',required=True,type=pathlib.Path)
    ap.add_argument('--out',required=True,type=pathlib.Path)
    a=ap.parse_args()
    verify(a.baseline,a.patched,a.out)
