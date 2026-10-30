"""Paired timing of an emitted x86 kernel against GCC's code for the same expression.

This measures; it never decides. A kernel is timed only after its artifact replays: the exact bytes and the
full-word equivalence check are rechecked first. The baseline is generated from the problem's own expression as C
and compiled with `gcc -O2`. Both functions are called through the same harness, through a volatile function
pointer, in two modes:
- latency: each result feeds the next call's first argument;
- throughput: independent inputs, with the results combined.

The repeats alternate kernel and baseline (ABAB) to cancel slow drift. Times are RDTSC reference ticks between
LFENCEs, divided by the call count; the medians are reported. Results hold for this machine, this load and this
call ABI only. They are not a portable speed claim. Frequency scaling, turbo and other load can move both sides.
"""

from __future__ import annotations

import json
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess

HARNESS = r'''
#include <stdint.h>
#include <stdio.h>
#include <x86intrin.h>
typedef uint64_t (*fn_t)(uint64_t, uint64_t);
extern uint64_t zeno_kernel(uint64_t, uint64_t);
extern uint64_t zeno_baseline(uint64_t, uint64_t);
static fn_t volatile kernel_ptr = zeno_kernel, baseline_ptr = zeno_baseline;
static uint64_t inputs[1024];
static uint64_t ticks(void) { _mm_lfence(); uint64_t t = __rdtsc(); _mm_lfence(); return t; }
static double latency(fn_t f, unsigned long n) {
    uint64_t x = inputs[0], y = inputs[1]; uint64_t t0 = ticks();
    for (unsigned long i = 0; i < n; i++) x = f(x, y);
    uint64_t t1 = ticks(); if (x == 42) puts(""); return (double)(t1 - t0) / n;
}
static double throughput(fn_t f, unsigned long n) {
    uint64_t sum = 0; uint64_t t0 = ticks();
    for (unsigned long i = 0; i < n; i++) sum ^= f(inputs[i & 1023], inputs[(i + 7) & 1023]);
    uint64_t t1 = ticks(); if (sum == 42) puts(""); return (double)(t1 - t0) / n;
}
int main(int argc, char **argv) {
    unsigned long n = 1000000UL, repeats = 21;
    if (argc > 2) { n = strtoul(argv[1], 0, 10); repeats = strtoul(argv[2], 0, 10); }
    uint64_t s = 0x9e3779b97f4a7c15ULL;
    for (int i = 0; i < 1024; i++) { s = s * 6364136223846793005ULL + 1442695040888963407ULL; inputs[i] = s; }
    for (int i = 0; i < 1024; i++) if (kernel_ptr(inputs[i], inputs[1023 - i]) != baseline_ptr(inputs[i], inputs[1023 - i])) {
        printf("{\"error\": \"kernel and baseline disagree\"}\n"); return 3; }
    latency(kernel_ptr, n / 10); latency(baseline_ptr, n / 10);
    printf("{\"rows\": [");
    for (unsigned long r = 0; r < repeats; r++) {
        double kl = latency(kernel_ptr, n), bl = latency(baseline_ptr, n);
        double kt = throughput(kernel_ptr, n), bt = throughput(baseline_ptr, n);
        printf("%s[%.4f, %.4f, %.4f, %.4f]", r ? ", " : "", kl, bl, kt, bt);
    }
    printf("]}\n"); return 0;
}
'''


def c_expression(node) -> str:
    op = node[0]
    if op == "input":
        return node[1]
    if op == "const":
        return f"UINT64_C({int(node[1])})"
    if op == "not":
        return f"(~{c_expression(node[1])})"
    if op == "shl":
        return f"({c_expression(node[1])} << {int(node[2])})"
    if op == "lshr":
        return f"({c_expression(node[1])} >> {int(node[2])})"
    symbol = {"add": "+", "sub": "-", "mul": "*", "and": "&", "or": "|", "xor": "^"}[op]
    return f"({c_expression(node[1])} {symbol} {c_expression(node[2])})"


def _run(arguments, cwd):
    result = subprocess.run(arguments, cwd=cwd, capture_output=True, text=True, timeout=600, check=False,
                            env={"PATH": "/usr/bin:/bin", "LANG": "C", "LC_ALL": "C"})
    if result.returncode:
        raise ValueError(f"{Path(arguments[0]).name} failed: {result.stderr[-1500:] or result.stdout[-500:]}")
    return result.stdout


def bench(artifact: Path, output: Path, iterations=1_000_000, repeats=21, timeout_ms=1000) -> dict:
    from .session import replay_artifact
    from .model import read_json
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        return {"status": "inconclusive", "reason": "paired timing needs an actual Linux x86-64 host"}
    if not (1_000 <= iterations <= 100_000_000 and 3 <= repeats <= 101):
        raise ValueError("iterations must be 1,000..100,000,000 and repeats 3..101")
    gcc = shutil.which("gcc", path="/usr/bin:/bin")
    if gcc is None:
        return {"status": "inconclusive", "reason": "gcc is required for the baseline"}
    replayed = replay_artifact(artifact, timeout_ms)
    if replayed.get("status") != "replayed":
        return {"status": "inconclusive", "reason": "artifact did not replay", "detail": replayed}
    problem = read_json(artifact / "problem.json")
    if problem["target"] != "x86_64-sysv-register/1":
        return {"status": "inconclusive", "reason": "only x86-64 kernels can be executed on this host"}
    output.mkdir(mode=0o700)
    expression = c_expression(problem["expression"])
    (output / "baseline.c").write_text("#include <stdint.h>\n__attribute__((noinline)) uint64_t "
                                       f"zeno_baseline(uint64_t x, uint64_t y) {{ (void)x; (void)y; return {expression}; }}\n")
    (output / "harness.c").write_text(HARNESS.replace("#include <stdio.h>", "#include <stdio.h>\n#include <stdlib.h>"))
    _run([gcc, "-std=gnu11", "-O2", "-fno-lto", "-c", "baseline.c", "-o", "baseline.o"], output)
    _run([gcc, "-std=gnu11", "-O2", "-fno-lto", "-Wl,-z,noexecstack", "harness.c", "baseline.o",
          str((artifact / "kernel.o").resolve()), "-o", "bench"], output)
    rows = json.loads(_run([str(output / "bench"), str(iterations), str(repeats)], output))["rows"]
    median = lambda column: statistics.median(row[column] for row in rows)
    report = {"schema": "zeno-fcis/machine-bench/1", "status": "measured", "authority": "none",
              "artifact_code_hex": (artifact / "kernel.bin").read_bytes().hex(), "baseline_c": expression,
              "ticks_per_call": {"kernel_latency": median(0), "baseline_latency": median(1),
                                 "kernel_throughput": median(2), "baseline_throughput": median(3)},
              "ratio_kernel_over_baseline": {"latency": median(0) / median(1), "throughput": median(2) / median(3)},
              "iterations": iterations, "repeats": repeats,
              "environment": {"cpu": _cpu(), "load_average": os.getloadavg(), "compiler": _run([gcc, "--version"], output).splitlines()[0]},
              "scope": ("RDTSC reference ticks per call through a function pointer, median of alternating repeats on "
                        "this machine and load. Not a portable speedup claim; call overhead is included equally.")}
    (output / "report.json").write_text(json.dumps(report, indent=1, sort_keys=True) + "\n")
    return report


def _cpu() -> str:
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                return line.split(":", 1)[1].strip()
    except OSError:
        pass
    return platform.processor() or platform.machine()
