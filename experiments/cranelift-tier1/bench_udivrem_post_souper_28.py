#!/usr/bin/env python3
"""Post-Souper unsigned div/rem AOT timing screen (finite, NOT a speedup proof).

Compile identical WAT using two pinned Wasmtime CLI binaries; verify an independent
integer oracle, retain AOT/disassembly hashes, and record raw balanced wall-clock
samples. A noisy CI VM cannot establish representative native throughput. No
compiler modification happens in this script, and upstream files remain untouched.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import platform
import re
import statistics
import subprocess
import time
from pathlib import Path

MASK = (1 << 64) - 1
STEP = -7046029254386353131
PIN = "8b19f199f89da864c529b3fa3ca2f7e40d5de609"
TARGET = "x86_64-unknown-linux"
SEED = 781234567891
TYPES = ("pair", "quotient_control")


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def source(kind: str) -> str:
    if kind not in TYPES:
        raise ValueError("unknown fixture")
    # The pair's quotient AND remainder remain observable. The changing x/y
    # preclude loop-invariant division. y starts odd and increases by 2;
    # admitted iterations <= 8 million never produce a zero divisor.
    expression = """local.get $x
local.get $y
i64.div_u
"""
    if kind == "pair":
        expression += """local.get $x
local.get $y
i64.rem_u
i64.const 17
i64.rotl
i64.xor
"""
    return """(module
  (func (export "work") (param $n i64) (param $seed i64) (result i64)
    (local $x i64) (local $y i64) (local $i i64) (local $acc i64)
    local.get $seed
    local.set $x
    i64.const 3
    local.set $y
    block $exit
      loop $repeat
        local.get $i
        local.get $n
        i64.ge_u
        br_if $exit
        local.get $acc
""" + expression + """i64.xor
        local.set $acc
        local.get $x
        i64.const -7046029254386353131
        i64.add
        local.set $x
        local.get $y
        i64.const 2
        i64.add
        local.set $y
        local.get $i
        i64.const 1
        i64.add
        local.set $i
        br $repeat
      end
    end
    local.get $acc
  )
)
"""


def signed(n: int) -> int:
    n &= MASK
    return n if n < (1 << 63) else n - (1 << 64)


def expected(kind: str, n: int, seed: int) -> int:
    if kind not in TYPES or not (0 <= n <= 8_000_000):
        raise ValueError("outside admitted finite benchmark domain")
    x, y, acc = seed & MASK, 3, 0
    for _ in range(n):
        q, r = divmod(x, y)
        term = q
        if kind == "pair":
            term ^= ((r << 17) | (r >> 47)) & MASK
        acc ^= term
        x = (x + STEP) & MASK
        y += 2
    return signed(acc)


def run(command: list[str], timeout: int = 180) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, capture_output=True, text=True, timeout=timeout)


def invoke(binary: Path, aot: Path, n: int, seed: int) -> tuple[int, int]:
    command = [str(binary), "run", "--allow-precompiled", "--invoke", "work",
               str(aot), str(n), str(seed)]
    start = time.perf_counter_ns()
    result = run(command, timeout=90)
    elapsed = time.perf_counter_ns() - start
    if result.returncode or not re.fullmatch(r"-?\d+", result.stdout.strip()):
        raise AssertionError(f"wasmtime failed: {result.returncode}: {result.stderr[-1000:]}")
    return int(result.stdout.strip()), elapsed


def cpu_name() -> str | None:
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                return line.split(":", 1)[1].strip()
    except OSError:
        pass
    return None


def benchmark(out: Path, iters: int, cycles: int) -> None:
    if not (10_000 <= iters <= 8_000_000 and 2 <= cycles <= 12):
        raise ValueError("admitted parameters: 10000..8000000 iterations, 2..12 cycles")
    if (out / "runtime_screen.json").exists() or (out / "runtime-screen").exists():
        raise ValueError("refusing to overwrite an existing benchmark")
    pins = {}
    for mode in ("baseline", "patched"):
        exe = (out / mode / "wasmtime").resolve()
        if not exe.is_file():
            raise ValueError(f"missing executable for {mode}")
        version = run([str(exe), "--version"])
        if version.returncode:
            raise ValueError(f"compiler binary unavailable for {mode}")
        pins[mode] = {"sha256": sha(exe), "version": version.stdout.strip()}
    if pins["baseline"]["version"] != pins["patched"]["version"]:
        raise ValueError("compilers report different versions")
    if (out / "wasmtime-head.txt").read_text().strip() != PIN:
        raise ValueError("not the intended post-Souper Wasmtime source")
    if not (out / "post_souper_cost_audit.json").is_file():
        raise ValueError("first run the independent static-cost/native audit")

    root = out / "runtime-screen"
    root.mkdir()
    fixtures = {}
    observations = []
    for kind in TYPES:
        wat = root / f"{kind}.wat"
        wat.write_text(source(kind))
        data = {"wat_sha256": sha(wat), "aot": {}, "costs": {}, "oracle": {}}
        aots = {}
        for mode in ("baseline", "patched"):
            exe = (out / mode / "wasmtime").resolve()
            aot = root / f"{kind}-{mode}.cwasm"
            compilation = run([str(exe), "compile", "--target", TARGET,
                               "-o", str(aot), str(wat)])
            if compilation.returncode:
                raise RuntimeError(f"{kind}/{mode}: compile failed: {compilation.stderr[-1000:]}")
            dis = run([str(exe), "objdump", "--funcs", "wasm", "--bytes", str(aot)])
            if dis.returncode:
                raise RuntimeError(f"{kind}/{mode}: objdump failed: {dis.stderr[-1000:]}")
            disfile = root / f"{kind}-{mode}.disassembly.txt"
            disfile.write_text(dis.stdout)
            rows = [line for line in dis.stdout.splitlines()
                    if re.match(r"^ {12}[0-9a-fA-F]{2}(?:\s|$)", line)]
            if not rows:
                raise ValueError("no decoded instructions")
            divisions = sum(bool(re.search(r"\b(?:idiv|div)(?:q|l)?\b", line))
                            for line in rows)
            data["aot"][mode] = {"sha256": sha(aot), "size": aot.stat().st_size,
                                 "disassembly_sha256": sha(disfile)}
            data["costs"][mode] = {"decoded_instructions": len(rows),
                                   "static_divisions": divisions}
            aots[mode] = aot.resolve()
        # Reject gross transformations and controls that alter unrelated code.
        bv, pv = [data["costs"][v]["static_divisions"] for v in ("baseline", "patched")]
        if kind == "pair" and not (bv == 2 and pv == 1):
            raise AssertionError("expected dual-result DIV saving missing")
        if kind == "quotient_control" and not (bv == pv == 1):
            raise AssertionError("quotient-only no-fusion control failed")

        # An independent Python arithmetic oracle checks both implementations
        # on a small canary and on the *full workload*, before timing.
        for n in (37, iters):
            want = expected(kind, n, SEED)
            data["oracle"][str(n)] = want
            for mode in ("baseline", "patched"):
                got, _ = invoke((out / mode / "wasmtime").resolve(),
                                aots[mode], n, SEED)
                if got != want:
                    raise AssertionError(f"{kind}/{mode}({n}): {got} != {want}")
        # Warmup is intentionally unmeasured. Then balanced ABBA ordering
        # reduces systematic drift but cannot remove host/thermal noise.
        for mode in ("baseline", "patched"):
            got, _ = invoke((out / mode / "wasmtime").resolve(), aots[mode], iters, SEED)
            if got != data["oracle"][str(iters)]:
                raise AssertionError(f"{kind}/{mode}: warmup mismatch")
        times = {"baseline": [], "patched": []}
        for cycle in range(cycles):
            sequence = ("baseline", "patched", "patched", "baseline") if cycle % 2 == 0 else (
                "patched", "baseline", "baseline", "patched")
            for order, mode in enumerate(sequence):
                value, elapsed = invoke((out / mode / "wasmtime").resolve(),
                                        aots[mode], iters, SEED)
                if value != data["oracle"][str(iters)]:
                    raise AssertionError("timed result differs from independent oracle")
                times[mode].append(elapsed)
                observations.append({"fixture": kind, "cycle": cycle, "order": order,
                                     "mode": mode, "elapsed_ns": elapsed, "value": value})
        b, p = (statistics.median(times[k]) for k in ("baseline", "patched"))
        data["elapsed_median_ns"] = {"baseline": b, "patched": p}
        data["patched_over_baseline_median_ratio"] = p / b
        data["elapsed_sample_ns"] = times
        fixtures[kind] = data

    receipt = {
        "schema": "zenofcis/tier1-post-souper-runtime-screen/1",
        "classification": "finite native-AOT correctness with noisy process-wall-clock timing",
        "source_pin": PIN,
        "zenofcis_source_pin": (out / "zenofcis-head.txt").read_text().strip(),
        "compiler_pins": pins, "target": TARGET,
        "machine": {"platform": platform.platform(), "processor": cpu_name(),
                    "python": platform.python_version()},
        "method": {"iterations": iters, "seed": SEED, "cycles": cycles,
                   "samples_per_compiler_per_fixture": cycles * 2,
                   "order": "alternating ABBA / BAAB",
                   "includes_process_start_and_AOT_load": True},
        "fixtures": fixtures, "raw_observations": observations,
        "limits": [
            "One shared CI VM and one x86-64 host; do not claim representative speedup",
            "Wall-clock samples include process startup, AOT loading, and OS scheduling",
            "Neither CLIF cost nor global instruction-sequence optimality is measured",
            "Native arithmetic oracle checks values on this bounded loop, not compiler soundness",
            "Static DIV and encoded bytes are separate from runtime timing",
            "No statistically supported performance conclusion from this limited screen",
        ],
    }
    (out / "runtime_screen.json").write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": "finite-oracle-and-balanced-native-timing-passed",
                      "ratios": {k: round(v["patched_over_baseline_median_ratio"], 4)
                                 for k, v in fixtures.items()},
                      "samples": len(observations)}, sort_keys=True))


def self_test() -> None:
    assert "i64.div_u" in source("pair") and "i64.rem_u" in source("pair")
    assert "i64.rem_u" not in source("quotient_control")
    assert expected("pair", 0, SEED) == 0
    assert expected("quotient_control", 0, SEED) == 0
    assert expected("pair", 1, 7) == 2 ^ (1 << 17)
    assert expected("quotient_control", 1, 7) == 2
    for bad in ("signed", "reverse"):
        try:
            source(bad)
        except ValueError:
            continue
        raise AssertionError("accepted invalid fixture kind")
    for invalid in (-1, 8_000_001):
        try:
            expected("pair", invalid, SEED)
        except ValueError:
            continue
        raise AssertionError("accepted invalid oracle domain")
    print("WAT generator, independent arithmetic model and domain guards passed")


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--self-test", action="store_true")
    p.add_argument("--out", type=Path)
    p.add_argument("--iterations", type=int, default=5_000_000)
    p.add_argument("--cycles", type=int, default=5)
    a = p.parse_args()
    if a.self_test:
        self_test()
    elif a.out:
        benchmark(a.out, a.iterations, a.cycles)
    else:
        p.error("--self-test or --out required")
