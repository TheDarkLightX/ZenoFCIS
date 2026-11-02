#!/usr/bin/env python3
"""Evidence-bound, non-gating call-level timing of Cranelift's original ELF objects.

The two .o files MUST be supplied from the pinned CI artifact; this program never
recompiles or replaces them. Hashes bind to the independently verified A/B.
The timing is exploratory: no assertion of faster code without replicated,
controlled, architecture-specific measurement and regression checks.
"""
from __future__ import annotations
import argparse
import csv
import hashlib
import io
import json
import os
import platform
import shutil
import statistics
import subprocess
import tempfile
from pathlib import Path

EXPECTED = {
    "satadd.o": ("fd23bd8da2b5b758112bdab485a1cd1b1f45251f7705af2cc5239fb793dff107", "%tier1_signed_satadd", "zeno_tier1_baseline"),
    "satadd-shift.o": ("8b4e6f7fc04a4d028861cfcffff3b5652a6871ca7b09546dd4e4f77045605d17", "%tier1_signed_satadd_shift", "zeno_tier1_candidate"),
}
PIN = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"

def sh(args: list[str], **kwargs) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, check=True, text=True, capture_output=True, **kwargs)

def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--objects", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--calls", type=int, default=2_400_000)
    ap.add_argument("--rounds", type=int, default=11)
    ap.add_argument("--pin-cpu", action="store_true", help="pin to first allowed CPU when supported")
    a = ap.parse_args()
    if a.out.exists():
        ap.error("output already exists: new evidence requires a fresh destination")
    if a.calls < 8000 or a.calls > 200_000_000 or a.calls % 8 or not 4 <= a.rounds <= 100:
        ap.error("calls must be divisible by eight and within [8000,200000000]; rounds [4,100]")
    if platform.machine().lower() not in ("x86_64", "amd64") or platform.system() != "Linux":
        ap.error("only native Linux x86-64 SysV validation supported")
    here = Path(__file__).resolve().parent
    object_hashes = {}
    for name, (expected, _old, _new) in EXPECTED.items():
        b = (a.objects / name).read_bytes()
        observed = hashlib.sha256(b).hexdigest()
        if observed != expected:
            raise SystemExit(f"refusing unknown artifact {name}: SHA-256 {observed}")
        object_hashes[name] = observed
    with tempfile.TemporaryDirectory(prefix="tier1-perf-") as td:
        t = Path(td)
        objs = []
        for name, (_, old, new) in EXPECTED.items():
            renamed = t / name
            sh(["objcopy", "--redefine-sym", f"{old}={new}", str(a.objects / name), str(renamed)])
            objs.append(str(renamed))
        exe = t / "bench"
        flags = ["-O2", "-std=gnu11", "-Wall", "-Wextra", "-Werror", "-fno-lto", "-fno-omit-frame-pointer"]
        sh(["gcc", *flags, str(here / "benchmark_exact_objects.c"), *objs, "-o", str(exe)])
        permitted = sorted(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else []
        pinned = None
        if a.pin_cpu and permitted:
            pinned = permitted[0]
            os.sched_setaffinity(0, {pinned})
        run = sh([str(exe), str(a.calls), str(a.rounds)])
        rows = list(csv.DictReader(io.StringIO(run.stdout)))
    expected_rows = 3 * 2 * a.rounds
    if len(rows) != expected_rows:
        raise SystemExit(f"unexpected number of samples {len(rows)}, expected {expected_rows}")
    workloads = ("chain_nonoverflow", "chain_saturating", "parallel8")
    aggregate = {}
    for mode in workloads:
        by = {v: [int(r["ns"]) / a.calls for r in rows if r["mode"] == mode and r["variant"] == v] for v in ("baseline","candidate")}
        if any(len(values) != a.rounds for values in by.values()):
            raise SystemExit("incomplete candidate or baseline measurements")
        checks = {v: {r["checksum"] for r in rows if r["mode"] == mode and r["variant"] == v} for v in by}
        if len(checks["baseline"]) != 1 or checks["baseline"] != checks["candidate"]:
            raise SystemExit("mismatched deterministic workloads")
        med0, med1 = (statistics.median(by[v]) for v in ("baseline","candidate"))
        aggregate[mode] = {
            "units": "wall-clock nanoseconds per function call including call/loop overhead",
            "median_baseline": med0, "median_candidate": med1,
            "candidate_over_baseline_ratio": med1/med0,
            "range_baseline": [min(by["baseline"]), max(by["baseline"])],
            "range_candidate": [min(by["candidate"]), max(by["candidate"])],
            "samples_per_variant": a.rounds,
            "checksum": next(iter(checks["baseline"])),
        }
    uname = platform.uname()
    cpu_name = next((s.strip().split(":",1)[1].strip() for s in Path("/proc/cpuinfo").read_text().splitlines() if s.startswith("model name")), "unreported")
    result = {
        "schema":"zeno-fcis/tier1-cranelift-call-level-benchmark/1",
        "status":"exploratory-observed-not-qualified-speedup",
        "original_object_sha256":object_hashes,
        "upstream_wasmtime_commit":PIN,
        "provenance_ci_run":"https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/41380989705",
        "experiment_source_commit":"fe8a737ebdb1f9a786c318ac191505ec573d331f",
        "benchmark_source_sha256": {f: hashlib.sha256((here/f).read_bytes()).hexdigest() for f in ("benchmark_exact_objects.c", "benchmark_exact_objects.py")},
        "build_flags": flags,
        "build_tool_versions":{"gcc":sh(["gcc","--version"]).stdout.splitlines()[0], "objcopy":sh(["objcopy","--version"]).stdout.splitlines()[0]},
        "host":{"system":uname.system,"kernel":uname.release,"cpu":cpu_name,"allowed_cpu_set_before_pinning":permitted,"pinned_cpu":pinned},
        "clock":"CLOCK_MONOTONIC_RAW",
        "measurement_order":"alternating paired AB/BA, three workloads, per-round per-variant timings",
        "calls_per_sample":a.calls,"rounds":a.rounds,
        "summary":aggregate,"raw_samples":rows,
        "nonclaims":["not an ISLE or patched-compiler measurement", "not microarchitectural instruction latency/throughput", "function-call and loop overhead are included", "does not isolate frequency or noisy neighbors", "not representative Wasmtime application speed", "not proof of performance improvement on other CPUs or under other optimization flags", "does not qualify automatically applied compiler rewrites"],
    }
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text(json.dumps(result, indent=2, sort_keys=True)+"\n")
    print(json.dumps({"status":result["status"],"summary":aggregate},sort_keys=True))

if __name__=="__main__":
    main()
