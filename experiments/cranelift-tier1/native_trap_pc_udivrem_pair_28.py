#!/usr/bin/env python3
"""Differential x86-64 Linux native fault-PC/si_code audit for exact Cranelift objects.

Independent GCC process+sigaction harness. Verifies host faulting instruction
class and signal provenance for specific functions; does NOT validate Wasmtime
trap tables, guest source locations, SSA, register liveness or all machine states.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tempfile

CASES = (
    'positive_quotient_remainder', 'negative_reverse_order',
    'negative_signed_quotient', 'negative_other_dividend',
    'negative_other_divisor', 'negative_not_remainder',
    'negative_double_quotient', 'exploratory_elidable_iconst_gap',
)
SPECIAL = {'negative_signed_quotient': ('signed_min_div_minus_one',),
           'negative_other_divisor': ('zero_second_divisor_only',)}
EXPECTED = 2 * len(CASES) + sum(len(x) for x in SPECIAL.values())
PATTERN = re.compile(r'^PASS case=(\w+) scenario=(\w+) x=(\d+) y=(\d+) '
                     r'baseline_sig=(\d+) baseline_code=(-?\d+) baseline_pc=(\d+) '
                     r'patched_sig=(\d+) patched_code=(-?\d+) patched_pc=(\d+)$')
LABEL = re.compile(r'^\s*([0-9a-f]+) <([^>]+)>:$')
INSTR = re.compile(r'^\s*([0-9a-f]+):\s+(?:(?:[0-9a-f]{2})\s+)+([a-zA-Z][\w.]*)\b')
TRAPS = {'div', 'divq', 'idiv', 'idivq', 'ud2'}

def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_c() -> str:
    declarations = '\n'.join(f'extern uint64_t {tag}_{name}(uint64_t,uint64_t);'
                             for name in CASES for tag in ('baseline', 'patched'))
    entries = ',\n'.join(f'{{"{name}", baseline_{name}, patched_{name}}}'
                        for name in CASES)
    return r'''
#define _GNU_SOURCE 1
#include <errno.h>
#include <inttypes.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <sys/types.h>
#include <sys/ucontext.h>
#include <sys/wait.h>
#include <unistd.h>
typedef uint64_t (*fptr)(uint64_t, uint64_t);
typedef struct { const char *name; fptr baseline, patched; } Case;
''' + declarations + '\nstatic const Case cases[] = {\n' + entries + '\n};\n' + r'''
typedef struct {
    uint64_t magic;
    uint64_t pc_offset;
    int32_t signum;
    int32_t si_code_value;
} Report;
static volatile sig_atomic_t child_fd = -1;
static uintptr_t child_function = 0;
static void on_fault(int signum, siginfo_t *info, void *user) {
    ucontext_t *u = (ucontext_t*)user;
    uintptr_t pc = (uintptr_t)u->uc_mcontext.gregs[REG_RIP];
    Report r = {UINT64_C(0x6352414e454c4946), pc - child_function,
                signum, info->si_code};
    /* POSIX async-signal-safe operations; no stdio/malloc in handler. */
    ssize_t n = write((int)child_fd, &r, sizeof(r));
    _exit(n == (ssize_t)sizeof(r) ? 0 : 121);
}
static int observe(fptr fn, uint64_t x, uint64_t y, Report *r) {
    int p[2];
    if (pipe(p) != 0) { perror("pipe"); return 1; }
    pid_t pid = fork();
    if (pid < 0) { perror("fork"); return 1; }
    if (pid == 0) {
        close(p[0]);
        struct rlimit limit = {0, 0};
        if (setrlimit(RLIMIT_CORE, &limit) != 0) _exit(122);
        child_fd = p[1];
        child_function = (uintptr_t)fn; /* Linux x86-64 SysV only */
        struct sigaction sa;
        memset(&sa, 0, sizeof(sa));
        sa.sa_sigaction = on_fault;
        sigemptyset(&sa.sa_mask);
        sa.sa_flags = SA_SIGINFO;
        if (sigaction(SIGFPE, &sa, NULL) || sigaction(SIGILL, &sa, NULL)) _exit(123);
        alarm(5);
        volatile uint64_t value = fn(x, y);
        (void)value;
        _exit(124); /* normal returns are not permitted on trap scenarios */
    }
    close(p[1]);
    ssize_t n = read(p[0], r, sizeof(*r));
    close(p[0]);
    int status = 0;
    pid_t ended;
    do { ended = waitpid(pid, &status, 0); }
    while (ended == -1 && errno == EINTR);
    if (ended != pid || n != (ssize_t)sizeof(*r)
        || !WIFEXITED(status) || WEXITSTATUS(status) != 0
        || r->magic != UINT64_C(0x6352414e454c4946)
        || (r->signum != SIGFPE && r->signum != SIGILL)
        || r->pc_offset > 4096) {
        fprintf(stderr, "missing/invalid fault PC: status=%d bytes=%zd\n", status, n);
        return 1;
    }
    return 0;
}
static int check(const Case *c, const char *scenario, uint64_t x,uint64_t y) {
    Report a = {0}, b = {0};
    if (observe(c->baseline, x, y, &a) || observe(c->patched, x, y, &b)
        || a.signum != b.signum || a.si_code_value != b.si_code_value) {
        fprintf(stderr,"FAIL signal/provenance mismatch case=%s scenario=%s\n",
                c->name,scenario);
        return 1;
    }
    printf("PASS case=%s scenario=%s x=%" PRIu64 " y=%" PRIu64
           " baseline_sig=%d baseline_code=%d baseline_pc=%" PRIu64
           " patched_sig=%d patched_code=%d patched_pc=%" PRIu64 "\n",
           c->name,scenario,x,y,a.signum,a.si_code_value,a.pc_offset,
           b.signum,b.si_code_value,b.pc_offset);
    return 0;
}
int main(void) {
    for (size_t i=0;i<sizeof(cases)/sizeof(cases[0]);i++) {
        const Case *c=&cases[i];
        if (check(c,"zero_denominator_nonzero_numerator",7,0)) return 1;
        if (check(c,"zero_denominator_zero_numerator",0,0)) return 1;
        if (i==2 && check(c,"signed_min_div_minus_one",
                          UINT64_C(0x8000000000000000),UINT64_MAX)) return 1;
        if (i==4 && check(c,"zero_second_divisor_only",0,7)) return 1;
    }
    return 0;
}
'''

def extract_instructions(text: str) -> dict[str, dict[int, str]]:
    symbols: dict[str, dict[int, str]] = {}
    base = 0
    active: str | None = None
    for line in text.splitlines():
        label = LABEL.match(line)
        if label:
            base = int(label.group(1), 16)
            name = label.group(2)
            active = name if any(name == f'{tag}_{case}'
                                      for tag in ('baseline','patched') for case in CASES) else None
            if active is not None:
                if active in symbols:
                    raise ValueError('duplicate native function label '+active)
                symbols[active] = {}
            continue
        m = INSTR.match(line)
        if m and active is not None:
            symbols[active][int(m.group(1),16) - base] = m.group(2).lower()
    return symbols

def parse_audit(stdout: str, disasm: str) -> list[dict]:
    symbols = extract_instructions(disasm)
    if len(symbols) != 2 * len(CASES):
        raise ValueError(f'expected 16 native symbols, found {len(symbols)}')
    result = []
    for line in stdout.splitlines():
        m=PATTERN.fullmatch(line)
        if not m:
            raise ValueError('unrecognized signal record: '+line)
        name, scenario, x,y,asig,acode,apc,bsig,bcode,bpc = m.groups()
        if name not in CASES or scenario not in (
            'zero_denominator_nonzero_numerator', 'zero_denominator_zero_numerator',
            *SPECIAL.get(name, ())):
            raise ValueError('unexpected case/scenario')
        row={'case':name,'scenario':scenario,'x':int(x),'y':int(y)}
        for tag,sig,code,pc in (('baseline',asig,acode,apc),('patched',bsig,bcode,bpc)):
            key=f'{tag}_{name}'
            off=int(pc)
            opcode=symbols[key].get(off)
            if opcode not in TRAPS:
                raise ValueError(f'{key} reported fault PC {off}, not a div/idiv/ud2 instruction: {opcode}')
            row[tag]={'signal':int(sig),'si_code':int(code),'pc_offset':off,
                      'faulting_mnemonic':opcode}
        if row['baseline']['signal'] != row['patched']['signal'] or \
           row['baseline']['si_code'] != row['patched']['si_code']:
            raise ValueError('trap signal or si_code mismatch')
        result.append(row)
    if len(result) != EXPECTED or len({(r['case'],r['scenario']) for r in result}) != EXPECTED:
        raise ValueError(f'incomplete or duplicate trap scenario coverage {len(result)} != {EXPECTED}')
    return result

def run(baseline: Path, patched: Path, out: Path) -> dict:
    if out.exists() or out.with_name('fault-pc-audit-source.c').exists():
        raise ValueError('refusing to overwrite fault-PC evidence')
    out.parent.mkdir(parents=True,exist_ok=True)
    object_hashes = {}
    with tempfile.TemporaryDirectory(prefix='tier1-fault-pc-') as tmp:
        work=Path(tmp)
        objects=[]
        for name in CASES:
            object_hashes[name]={}
            for tag,root in (('baseline',baseline),('patched',patched)):
                src=root/(name+'.o')
                if not src.is_file():
                    raise FileNotFoundError(src)
                renamed=work/(tag+'_'+name+'.o')
                subprocess.run(['objcopy','--redefine-sym',
                                '%tier1_'+name+'='+tag+'_'+name,
                                str(src),str(renamed)],check=True)
                object_hashes[name][tag]=sha256(src)
                objects.append(renamed)
        source=out.with_name('fault-pc-audit-source.c')
        source.write_text(source_c())
        program=work/'fault-pc-audit'
        subprocess.run(['gcc','-std=c11','-O2','-Wall','-Wextra','-Werror',
                        str(source),*(str(x) for x in objects),'-o',str(program)],check=True)
        result=subprocess.run([str(program)],capture_output=True,text=True)
        if result.returncode:
            raise RuntimeError(f'fault-PC host oracle failed {result.returncode}: {result.stderr}')
        disasm=subprocess.run(['objdump','-d','-Mintel',str(program)],
                              check=True,capture_output=True,text=True).stdout
        rows=parse_audit(result.stdout,disasm)
        receipt={'schema':'zenofcis/tier1-udivrem-native-signal-pc-differential/1',
                 'status':'passed-finite-host-signal-code-and-fault-pc-class-only',
                 'target':'x86-64 Linux SysV; GCC-generated independent signal/context harness',
                 'checker_source_sha256':sha256(source),
                 'compiled_binary_sha256':sha256(program),
                 'objects':object_hashes,'scenarios':rows,
                 'count':len(rows),'native_child_calls':2*len(rows),
                 'assurance':'same SIGFPE/SIGILL and si_code; each RIP at decoded div/idiv/ud2 in own function',
                 'limitations':[
                     'Function-relative fault offsets need not match due to distinct native code layout',
                     'Host si_code and fault RIP do not establish Wasmtime guest trap metadata, source-map, handler behavior or catchability',
                     'Finite x86-64 host probes do not prove full ISA, SSA, liveness, poison, flags, register allocation or target correctness',
                     'Only the hash-bound exact native CLIF object pairs and 18 scenario inputs are observed',
                 ]}
        out.write_text(json.dumps(receipt,indent=2,sort_keys=True)+'\n')
        print(json.dumps({'status':receipt['status'],'scenarios':len(rows),
                          'native_child_calls':2*len(rows)},sort_keys=True))
        return receipt

def self_test() -> None:
    assert EXPECTED == 18 and len(CASES) == 8
    assert source_c().count('extern uint64_t ') == 16
    assert 'REG_RIP' in source_c() and 'SA_SIGINFO' in source_c()
    assert 'a.si_code_value != b.si_code_value' in source_c()
    sample = '''0000000000001130 <baseline_positive_quotient_remainder>:
    1130: 48 89 f8              mov    rax,rdi
    1133: 48 f7 f6              div    rsi
0000000000001140 <patched_positive_quotient_remainder>:
    1140: 48 89 f8              mov    rax,rdi
    1143: 48 f7 f6              div    rsi'''
    s = extract_instructions(sample)
    assert s['baseline_positive_quotient_remainder'][3] == 'div'
    assert s['patched_positive_quotient_remainder'][3] == 'div'
    print('native fault-PC signal harness and disassembly parser self-tests passed; no compiler objects executed')

if __name__ == '__main__':
    ap=argparse.ArgumentParser()
    ap.add_argument('--self-test',action='store_true')
    ap.add_argument('--baseline',type=Path)
    ap.add_argument('--patched',type=Path)
    ap.add_argument('--out',type=Path)
    args=ap.parse_args()
    if args.self_test:self_test()
    elif args.baseline and args.patched and args.out:run(args.baseline,args.patched,args.out)
    else:ap.error('require --self-test or --baseline --patched --out')
