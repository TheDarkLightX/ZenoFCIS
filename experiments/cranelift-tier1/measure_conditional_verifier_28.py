#!/usr/bin/env python3
"""Tier-1 *compile + serialize* cost screen, NOT a compiler-only timing proof.

Reuse the source-pinned post-Souper pristine/patched Wasmtime executables from
GitHub run 44056104464. Compile identical generated WAT on the same host and
audit exact native bytes plus guest results. We time process launch, frontend,
codegen, and AOT serialization together: never report this as codegen time.
"""
from __future__ import annotations

import argparse
import csv
import hashlib
import json
import re
import statistics
import subprocess
import sys
import time
from pathlib import Path

import audit_post_souper_inprocess_28 as identity

UPSTREAM = "8b19f199f89da864c529b3fa3ca2f7e40d5de609"
SCHEMA = "zenofcis/tier1-post-souper-conditional-verifier-ab/1"
TARGET = "x86_64-unknown-linux"
MODES = ("baseline", "patched", "conditional")
KINDS = ("pair", "quotient_control")
SIZES = (64, 256)
CYCLES = 4
MASK = (1 << 64) - 1
FIELDS = ("kind", "functions", "cycle", "position", "mode", "elapsed_ns", "aot_sha256", "aot_bytes")

def sha(p: Path) -> str:
    if not p.is_file() or p.is_symlink():
        raise ValueError("missing or symlinked file: " + str(p))
    return hashlib.sha256(p.read_bytes()).hexdigest()

def wat(kind: str, count: int) -> str:
    if kind not in KINDS or count not in SIZES:
        raise ValueError("outside admitted source palette")
    pieces = ["(module"]
    for i in range(count):
        ops = """local.get $x
    local.get $y
    i64.div_u"""
        if kind == "pair":
            ops += """
    local.get $x
    local.get $y
    i64.rem_u
    i64.const 17
    i64.rotl
    i64.xor"""
        pieces.append(f"""  (func (export "work{i}") (param $x i64) (param $y i64) (result i64)
    {ops}
  )""")
    pieces.append(")")
    return "\n".join(pieces) + "\n"

def signed(x: int) -> int:
    x &= MASK
    return x if x < 1 << 63 else x - (1 << 64)

def expected(kind: str, x: int, y: int) -> int:
    if y == 0:
        raise ZeroDivisionError()
    q, r = divmod(x & MASK, y & MASK)
    result = q if kind == "quotient_control" else q ^ (((r << 17) | (r >> 47)) & MASK)
    return signed(result)

def invoke(exe: Path, aot: Path, fn: int, x: int, y: int) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(exe), "run", "--allow-precompiled", "--invoke", f"work{fn}",
         str(aot), str(signed(x)), str(signed(y))],
        text=True, capture_output=True, timeout=40)

def oracle_check(exe: Path, aot: Path, kind: str, count: int) -> int:
    cases = ((0, 1), (7, 3), (MASK, 7), (1 << 63, MASK))
    observations = 0
    for fn in (0, count // 2, count - 1):
        for x, y in cases:
            p = invoke(exe, aot, fn, x, y)
            if p.returncode or not re.fullmatch(r"-?\d+", p.stdout.strip()):
                raise AssertionError(f"{kind}/{fn} unexpectedly failed: {p.stderr[-500:]}")
            if int(p.stdout.strip()) != expected(kind, x, y):
                raise AssertionError(f"{kind}/{fn} returned wrong unsigned result")
            observations += 1
        trapped = invoke(exe, aot, fn, 1, 0)
        if trapped.returncode == 0 or "trap" not in trapped.stderr.lower():
            raise AssertionError(f"{kind}/{fn} failed to trap for zero divisor")
        observations += 1
    return observations

def order(cycle: int) -> tuple[str, ...]:
    if not 0 <= cycle < CYCLES:
        raise ValueError("cycle outside sampling schedule")
    rotated = list(MODES[cycle % 3:] + MODES[:cycle % 3])
    return tuple(rotated + list(reversed(rotated)))

def audit_rows(rows: list[dict[str, str]]) -> dict:
    if len(rows) != len(KINDS) * len(SIZES) * CYCLES * 6:
        raise ValueError("wrong sample count")
    summary: dict[str, dict] = {}
    ix = 0
    for kind in KINDS:
        for size in SIZES:
            timings = {m: [] for m in MODES}
            ratios = []
            for cycle in range(CYCLES):
                block = {m: [] for m in MODES}
                for position, mode in enumerate(order(cycle)):
                    row = rows[ix]
                    ix += 1
                    try:
                        elapsed = int(row["elapsed_ns"])
                        bytes_ = int(row["aot_bytes"])
                    except (ValueError, KeyError) as err:
                        raise ValueError("missing or nonnumeric sample") from err
                    if (set(row) != set(FIELDS)
                        or row["kind"] != kind or row["functions"] != str(size)
                        or row["cycle"] != str(cycle) or row["position"] != str(position)
                        or row["mode"] != mode or str(elapsed) != row["elapsed_ns"]
                        or str(bytes_) != row["aot_bytes"]
                        or not 0 < elapsed < 2**63 or bytes_ < 100
                        or not re.fullmatch(r"[0-9a-f]{64}", row["aot_sha256"])):
                        raise ValueError("bad source/order/duration/byte or hash receipt")
                    timings[mode].append(elapsed)
                    block[mode].append(elapsed)
                ratios.append(statistics.median(block["conditional"]) /
                              statistics.median(block["patched"]))
            b = statistics.median(timings["baseline"])
            p = statistics.median(timings["patched"])
            c = statistics.median(timings["conditional"])
            summary[f"{kind}_{size}"] = {
                "baseline_median_ns": b,
                "patched_median_ns": p,
                "conditional_median_ns": c,
                "ratio": p / b,
                "conditional_over_patched_ratio": c / p,
                "conditional_over_baseline_ratio": c / b,
                "paired_cycle_ratios": ratios,
                "cycle_ratio_median": statistics.median(ratios),
                "samples_per_compiler": 2 * CYCLES,
            }
    return summary

def self_test() -> None:
    assert wat("pair", 64).count("i64.rem_u") == 64
    assert wat("quotient_control", 256).count("i64.rem_u") == 0
    assert expected("pair", MASK, 7) == signed(
        (MASK // 7) ^ (((MASK % 7 << 17) | ((MASK % 7) >> 47)) & MASK))
    assert expected("quotient_control", MASK, MASK) == 1
    rows = []
    for kind in KINDS:
        for size in SIZES:
            for cycle in range(CYCLES):
                for pos, mode in enumerate(order(cycle)):
                    rows.append(dict(zip(FIELDS, (
                        kind, str(size), str(cycle), str(pos), mode,
                        "10000", "a" * 64, "15000"))))
    assert len(audit_rows(rows)) == 4
    for idx, field, value in ((0, "mode", "patched"), (0, "elapsed_ns", "0"),
                              (3, "aot_sha256", "not-a-digest"), (0, "functions", "65"),
                              (0, "aot_bytes", "-1")):
        mutant = [dict(row) for row in rows]
        mutant[idx][field] = value
        try:
            audit_rows(mutant)
        except ValueError:
            continue
        raise AssertionError("mutant accepted: " + field)
    print("WAT palette, arithmetic contract, three-compiler sampler, five corruption controls passed")

def variant_receipt(root: Path) -> str:
    path = root / "conditional-install.json"
    data = json.loads(path.read_text())
    if (data.get("source_commit") != UPSTREAM
        or set(data.get("modified_source_paths", [])) != set(data.get("original_git_blobs", {}))
        or "tier1_changes" not in data.get("structural_validation", "")):
        raise ValueError("conditional source patch and validation receipt untrusted")
    return sha(path)


def run(root: Path, out: Path) -> None:
    if out.exists():
        raise ValueError("refusing to overwrite evidence")
    original = identity.producer(root)  # binds SHA-256 of all archived compilers/AOTs
    treatment_receipt_sha = variant_receipt(root)
    out.mkdir(parents=True)
    source = out / "source"
    source.mkdir()
    latest = out / "latest"
    latest.mkdir()
    wat_hashes = {}
    for kind in KINDS:
        for size in SIZES:
            path = source / f"{kind}_{size}.wat"
            path.write_text(wat(kind, size))
            wat_hashes[f"{kind}_{size}"] = sha(path)
    rows: list[dict[str, str]] = []
    aot_final = {}
    oracles = {}
    for kind in KINDS:
        for size in SIZES:
            key = f"{kind}_{size}"
            src = source / f"{key}.wat"
            aot_final[key] = {}
            oracles[key] = {}
            for cycle in range(CYCLES):
                for position, mode in enumerate(order(cycle)):
                    exe = (root / mode / "wasmtime").resolve()
                    target = latest / f"{key}_{mode}.cwasm"
                    command = [str(exe), "compile", "--target", TARGET, "-o",
                               str(target), str(src)]
                    begin = time.perf_counter_ns()
                    result = subprocess.run(command, text=True, capture_output=True, timeout=240)
                    elapsed = time.perf_counter_ns() - begin
                    if result.returncode or not target.is_file():
                        raise RuntimeError(f"{key}/{mode} compile failed: {result.stderr[-1200:]}")
                    digest = sha(target)
                    if mode in aot_final[key] and digest != aot_final[key][mode]["sha256"]:
                        raise ValueError("nondeterministic AOT image during timing screen")
                    aot_final[key][mode] = {"sha256": digest, "bytes": target.stat().st_size}
                    rows.append(dict(zip(FIELDS, (
                        kind, str(size), str(cycle), str(position), mode, str(elapsed),
                        digest, str(target.stat().st_size)))))
            for mode in MODES:
                oracles[key][mode] = oracle_check(
                    (root / mode / "wasmtime").resolve(), latest / f"{key}_{mode}.cwasm",
                    kind, size)
            if aot_final[key]["patched"]["sha256"] != aot_final[key]["conditional"]["sha256"]:
                raise AssertionError("conditional verification changed emitted AOT image")
            if kind == "quotient_control":
                if (aot_final[key]["baseline"]["sha256"] != aot_final[key]["patched"]["sha256"]
                    or aot_final[key]["baseline"]["sha256"] != aot_final[key]["conditional"]["sha256"]):
                    raise AssertionError("unmatched quotient-only control emitted different AOT bytes")
    with (out / "raw.csv").open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=FIELDS)
        writer.writeheader()
        writer.writerows(rows)
    manifest = {
        "schema": SCHEMA, "classification": "exploratory end-to-end CLI compile-and-serialize wall time",
        "upstream": UPSTREAM, "target": TARGET,
        "producer_run": identity.PRODUCER_RUN, "producer_head": identity.PRODUCER_HEAD,
        "producer_receipt_sha256": identity.PRODUCER_RECEIPT_SHA,
        "conditional_install_receipt_sha256": treatment_receipt_sha,
        "compiler_sha256": {m: (original["compiler_pins"][m]["sha256"]
                                if m != "conditional" else sha(root / m / "wasmtime"))
                            for m in MODES},
        "wat_sha256": wat_hashes, "aot_final": aot_final, "finite_oracle_checks": oracles,
        "scope": {"sizes": SIZES, "kinds": KINDS, "cycles": CYCLES,
                  "order": "rotating Latin-square mirrored sextuplets, twice per compiler per cycle",
                  "measured": "whole subprocess: CLI startup, Wasm frontend, codegen, AOT writing",
                  "unmeasured": "compilation-only CPU cycles, instruction retirement, cold/hot cache separation"},
        "summary": audit_rows(rows), "raw_sha256": sha(out / "raw.csv"),
        "nonclaims": [
            "One virtualized CI host; timings are noisy and not representative compile throughput",
            "Compile+serialize latency is not the internal Cranelift compilation time",
            "This screen does not prove compiler-pass soundness, undefined flags or general profitability",
            "No new synthesized sequence and no upstream patch or PR",
            "Conditional verifier is an exploratory compiler overhead mitigation, not a correctness proof",
        ],
    }
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": "three-compiler conditional verifier exploratory envelope",
                      "ratios": {k: v["conditional_over_patched_ratio"] for k, v in manifest["summary"].items()}}, sort_keys=True))

def audit(root: Path, out: Path) -> None:
    doc = json.loads((out / "manifest.json").read_text())
    original = identity.producer(root)
    if (doc.get("conditional_install_receipt_sha256") != variant_receipt(root)):
        raise ValueError("conditional source-install receipt changed")
    if (doc.get("schema") != SCHEMA or doc.get("upstream") != UPSTREAM
        or doc.get("target") != TARGET or doc.get("producer_run") != identity.PRODUCER_RUN
        or doc.get("producer_head") != identity.PRODUCER_HEAD
        or doc.get("producer_receipt_sha256") != identity.PRODUCER_RECEIPT_SHA):
        raise ValueError("wrong source pin or schema")
    if sha(out / "raw.csv") != doc["raw_sha256"]:
        raise ValueError("transcript hash mismatch")
    with (out / "raw.csv").open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    if audit_rows(rows) != doc["summary"]:
        raise ValueError("reported medians/ratios do not match raw samples")
    for m in MODES:
        digest = sha(root / m / "wasmtime")
        if (digest != doc["compiler_sha256"][m] or
            (m != "conditional" and digest != original["compiler_pins"][m]["sha256"])):
            raise ValueError("compiler identity changed")
    for kind in KINDS:
        for size in SIZES:
            key = f"{kind}_{size}"
            if doc["aot_final"][key]["patched"]["sha256"] != doc["aot_final"][key]["conditional"]["sha256"]:
                raise ValueError("conditional compiler emits different AOT")
            if sha(out / "source" / f"{key}.wat") != doc["wat_sha256"][key]:
                raise ValueError("WAT source digest changed")
            if (out / "source" / f"{key}.wat").read_text() != wat(kind, size):
                raise ValueError("unexpected WAT semantics")
            for mode in MODES:
                p = out / "latest" / f"{key}_{mode}.cwasm"
                row = doc["aot_final"][key][mode]
                if sha(p) != row["sha256"] or p.stat().st_size != row["bytes"]:
                    raise ValueError("AOT changed after timing")
                if doc["finite_oracle_checks"][key][mode] != 15:
                    raise ValueError("incomplete or fabricated finite test count")
                samples = [r for r in rows if r["kind"] == kind
                           and r["functions"] == str(size) and r["mode"] == mode]
                if not all(r["aot_sha256"] == row["sha256"] and
                           int(r["aot_bytes"]) == row["bytes"] for r in samples):
                    raise ValueError("nonrepeatable AOT hash or byte cost")
    print("Pinned source, raw balanced compilation durations, AOT hashes and finite test counts audited")

def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--mode", required=True, choices=("self-test", "run", "audit"))
    p.add_argument("--source-root", type=Path)
    p.add_argument("--out", type=Path)
    a = p.parse_args()
    if a.mode == "self-test":
        self_test()
    elif a.source_root is None or a.out is None:
        p.error("source-root and out required")
    elif a.mode == "run":
        run(a.source_root, a.out)
    else:
        audit(a.source_root, a.out)

if __name__ == "__main__":
    main()
