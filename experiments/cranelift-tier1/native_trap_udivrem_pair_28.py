#!/usr/bin/env python3
"""Independent native trap-signal differential audit for pinned udiv/urem objects.

Runs the exact baseline/patched function objects under Linux x86-64 SysV using
fork() to isolate SIGFPE/SIGILL. This checks a concrete trap outcome only:
it does NOT prove trap provenance, source locations, Wasm handlers, compiler
correctness, or the entire x86 machine-state equivalence.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import re
import subprocess
import tempfile

CASES = (
    "positive_quotient_remainder",
    "negative_reverse_order",
    "negative_signed_quotient",
    "negative_other_dividend",
    "negative_other_divisor",
    "negative_not_remainder",
    "negative_double_quotient",
    "exploratory_elidable_iconst_gap",
)
SPECIAL = {
    "negative_signed_quotient": ("signed_min_div_minus_one",),
    "negative_other_divisor": ("zero_second_divisor_only",),
}
EXPECTED_ROWS = len(CASES) * 2 + sum(map(len, SPECIAL.values()))
LINE = re.compile(
    r"^PASS case=(\w+) scenario=(\w+) x=(\d+) y=(\d+) signal=(\d+)$"
)


def sha256(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source_c() -> str:
    decl = "\n".join(
        f"extern uint64_t {tag}_{name}(uint64_t,uint64_t);"
        for name in CASES for tag in ("baseline", "patched")
    )
    entries = ",\n".join(
        f'{{"{name}",baseline_{name},patched_{name}}}' for name in CASES
    )
    prefix = r"""
#define _POSIX_C_SOURCE 200809L
#include <stdint.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <signal.h>
#include <errno.h>
#include <sys/resource.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>
typedef uint64_t (*fptr)(uint64_t, uint64_t);
typedef struct { const char *name; fptr baseline; fptr patched; } Case;
"""
    body = r"""
static int trap_signal(fptr fn, uint64_t x, uint64_t y) {
    pid_t child = fork();
    if (child < 0) { perror("fork"); return -1; }
    if (child == 0) {
        struct rlimit limit = {0,0};
        if (setrlimit(RLIMIT_CORE,&limit) != 0) _exit(125);
        alarm(5);
        volatile uint64_t result = fn(x,y);
        (void)result;
        _exit(0);
    }
    int status = 0;
    pid_t ended;
    do { ended = waitpid(child,&status,0); }
    while (ended == -1 && errno == EINTR);
    if (ended != child) { perror("waitpid"); return -1; }
    if (!WIFSIGNALED(status)) return -2;
    int signum = WTERMSIG(status);
    if (signum != SIGFPE && signum != SIGILL) return -3;
    return signum;
}
static int check(const Case *c, const char *scenario, uint64_t x,uint64_t y) {
    int a=trap_signal(c->baseline,x,y);
    int b=trap_signal(c->patched,x,y);
    if (a<=0 || b<=0 || a!=b) {
        fprintf(stderr,"FAIL case=%s scenario=%s x=%" PRIu64
                " y=%" PRIu64 " baseline_signal=%d patched_signal=%d\n",
                c->name,scenario,x,y,a,b);
        return 1;
    }
    printf("PASS case=%s scenario=%s x=%" PRIu64
           " y=%" PRIu64 " signal=%d\n",c->name,scenario,x,y,a);
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
"""
    return prefix + decl + "\nstatic const Case cases[] = {\n" + entries + "\n};\n" + body


def audit(baseline: pathlib.Path, patched: pathlib.Path,
          output: pathlib.Path) -> dict:
    source_out = output.parent / "native-trap-audit-source.c"
    if output.exists() or source_out.exists():
        raise ValueError("refusing overwrite of native trap evidence")
    output.parent.mkdir(parents=True, exist_ok=True)
    object_hashes: dict[str, dict[str, str]] = {}
    with tempfile.TemporaryDirectory(prefix="tier1-native-traps-") as td:
        work = pathlib.Path(td)
        objects = []
        for name in CASES:
            object_hashes[name] = {}
            for tag, root in (("baseline", baseline), ("patched", patched)):
                original = root / (name + ".o")
                if not original.is_file():
                    raise FileNotFoundError("missing exact compiled object: " + str(original))
                renamed = work / (tag + "_" + name + ".o")
                subprocess.run(
                    ["objcopy", "--redefine-sym",
                     "%tier1_" + name + "=" + tag + "_" + name,
                     str(original), str(renamed)], check=True)
                object_hashes[name][tag] = sha256(original)
                objects.append(renamed)
        source_out.write_text(source_c())
        program = work / "native-traps"
        subprocess.run(
            ["gcc", "-std=c11", "-O2", "-Wall", "-Wextra", "-Werror",
             str(source_out), *(str(x) for x in objects), "-o", str(program)],
            check=True)
        p = subprocess.run([str(program)], capture_output=True, text=True)
        rows = []
        for line in p.stdout.splitlines():
            match = LINE.fullmatch(line)
            if match is None:
                raise ValueError("unrecognized output: " + line)
            name, scenario, x, y, signum = match.groups()
            if name not in CASES:
                raise ValueError("unrecognized case: " + name)
            rows.append(dict(case=name, scenario=scenario, x=int(x),
                             y=int(y), shared_signal=int(signum)))
        if p.returncode or len(rows) != EXPECTED_ROWS:
            raise RuntimeError(
                "native trap comparison failed: code=" + str(p.returncode)
                + " rows=" + str(len(rows)) + " stderr=" + p.stderr)
        if len({(r["case"],r["scenario"]) for r in rows}) != EXPECTED_ROWS:
            raise ValueError("duplicate scenario or missing trap check")
        receipt = {
            "schema": "zenofcis/tier1-adjacent-udivrem-native-trap-audit/1",
            "status": "passed-native-trap-signal-differential-only",
            "target": "x86-64 Linux SysV; forked direct native calls",
            "source_sha256": sha256(source_out),
            "reference": "GCC-compiled independent process-isolating signal oracle",
            "objects": object_hashes,
            "checked_scenarios": rows,
            "trapping_call_pairs": len(rows),
            "native_calls_in_child_processes": 2*len(rows),
            "expected": "both artifacts raise the same SIGFPE or SIGILL, never return",
            "scope": "division-by-zero for all eight CLIF functions; signed overflow and separate second-divisor traps for relevant negatives",
            "limitations": [
                "Passing asserts equality of observed host signal numbers, not exact trap PC, trap metadata or guest-visible trap code",
                "Compiler-produced signal behavior alone does not prove Cranelift SSA, regalloc, flags, poison, UB or full compiler-pass safety",
                "No portable ABI/ISA guarantee, non-x86-64 semantics or representative throughput conclusion",
                "Only the exact source-pinned native objects under test are covered",
            ],
        }
        output.write_text(json.dumps(receipt, indent=2, sort_keys=True)+"\n")
        print(json.dumps({
            "status": receipt["status"], "trapping_call_pairs":len(rows),
            "native_calls":2*len(rows), "source_sha256":receipt["source_sha256"]
        }, sort_keys=True))
        return receipt


def self_test() -> None:
    text = source_c()
    assert len(CASES) == 8 and EXPECTED_ROWS == 18
    assert text.count("extern uint64_t ") == len(CASES)*2
    assert "signed_min_div_minus_one" in text
    assert "zero_second_divisor_only" in text
    assert "WIFSIGNALED" in text and "SIGFPE" in text and "SIGILL" in text
    assert "alarm(5)" in text and "RLIMIT_CORE" in text
    print("native trap audit source fixture assertions passed (no real objects executed)")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--baseline", type=pathlib.Path)
    parser.add_argument("--patched", type=pathlib.Path)
    parser.add_argument("--out", type=pathlib.Path)
    args = parser.parse_args()
    if args.self_test:
        self_test()
    elif args.baseline and args.patched and args.out:
        audit(args.baseline,args.patched,args.out)
    else:
        parser.error("provide --self-test or --baseline, --patched, and --out")
