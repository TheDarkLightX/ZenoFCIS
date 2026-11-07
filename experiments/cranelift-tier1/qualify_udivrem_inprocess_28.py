#!/usr/bin/env python3
"""Fail-closed exploratory Wasmtime call-timing audit, not a speedup claim."""
from __future__ import annotations
import argparse
import hashlib
import json
import math
import statistics
from pathlib import Path

from install_isle_candidate import UPSTREAM

SCHEMA = "zeno-fcis/udivrem-source-palette-call-timing/1"
SOURCE_SCHEMA = "zeno-fcis/tier1-udiv-urem-source-palette/1"
EXPECTED_SOURCE_SHA256 = "47d6b0bb6bc2b2c4254ad78ac7959d3d79638b92155f34f9e7dc17237d72c61a"
AOT = {
    "direct": "f105ac2a7440cc0a8223376cd5c9bdcd7417abc67a89c0ef0c65c4f118bca191",
    "quotient_reuse": "feba0e3ff9503ae41f3c30cdfc83b76ef5936937e602818e11349a51bae0968b",
}
MASK = (1 << 64) - 1


def sha(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def signed(value: int) -> int:
    value &= MASK
    return value if value < (1 << 63) else value - (1 << 64)


def inputs() -> list[tuple[int, int]]:
    pairs = [
        (0, 1), (7, 3), (-(1 << 63), -1), ((1 << 63) - 1, 1),
        (-1, -1), ((1 << 63) - 1, -(1 << 63)),
        (-(1 << 63), (1 << 63) - 1), (123456789, -7),
        (0, -1), (-1, 1), (-(1 << 63), 3), ((1 << 63) - 1, 7),
    ]
    state = 0x9e3779b97f4a7c15
    for _ in range(256):
        state ^= (state << 13) & MASK
        state ^= state >> 7
        state ^= (state << 17) & MASK
        state &= MASK
        x = signed(state)
        state ^= (state << 13) & MASK
        state ^= state >> 7
        state ^= (state << 17) & MASK
        state &= MASK
        y = signed(state if state else 1)
        pairs.append((x, y))
    return pairs


def expected_checksum(calls: int) -> int:
    pairs = inputs()
    total = 0
    for n in range(calls):
        x, y = pairs[n % len(pairs)]
        a, b = x & MASK, y & MASK
        if not b:
            raise ValueError("reference contains zero divisor")
        q, r = divmod(a, b)
        val = q ^ (((r << 17) | (r >> 47)) & MASK)
        total = (total + val) & MASK
    return total


def audit(source_root: Path, raw: Path, cargo_lock: Path, calls: int, blocks: int) -> dict:
    receipt_bytes = (source_root / "qualification.json").read_bytes()
    src = json.loads(receipt_bytes)
    if (sha(receipt_bytes) != EXPECTED_SOURCE_SHA256
            or src.get("schema") != SOURCE_SCHEMA
            or src.get("status") != "native-source-formulation-comparison-finite-checked"
            or src.get("pins", {}).get("upstream_commit") != UPSTREAM
            or set(src.get("results", {})) != set(AOT)):
        raise ValueError("not exact successful source-program comparison")
    verified = {}
    for variant, expected in AOT.items():
        p = source_root / variant / "functions.cwasm"
        data = p.read_bytes()
        if (not data or sha(data) != expected
                or src["results"][variant].get("binary_sha256") != expected
                or len(data) != src["results"][variant].get("whole_cwasm_bytes")
                or src["results"][variant].get("finite", {}).get("zero_divisor_trap_checked") is not True):
            raise ValueError("wrong AOT object provenance or missing trap result")
        verified[variant] = {"sha256": expected, "bytes": len(data)}
    lock = cargo_lock.read_bytes()
    if b'name = "wasmtime"' not in lock:
        raise ValueError("missing locked Wasmtime dependency")
    if not (10_000 <= calls <= 2_000_000 and blocks >= 4 and blocks <= 64
            and blocks % 2 == 0):
        raise ValueError("invalid bounded timing parameters")
    lines = [line.strip() for line in raw.read_text().splitlines() if line.strip()]
    if not lines or lines[0] != f"META,{SCHEMA},{calls},{blocks},5623":
        raise ValueError("unrecognized timing transcript")
    coverage = [f"CHECK,direct,{len(inputs())}", f"CHECK,reuse,{len(inputs())}"]
    if lines[1:3] != coverage:
        raise ValueError("missing or inconsistent native verification coverage")
    if len(lines) != 3 + 4 * blocks:
        raise ValueError("incomplete or extra timing samples")
    checksum = expected_checksum(calls)
    samples = []
    block_ratios = []
    for block in range(blocks):
        names = ["direct", "reuse", "reuse", "direct"] if block % 2 == 0 else [
            "reuse", "direct", "direct", "reuse"]
        measured = {"direct": [], "reuse": []}
        for pos, name in enumerate(names):
            fields = lines[3 + 4 * block + pos].split(",")
            if len(fields) != 6 or fields[:4] != ["SAMPLE", str(block), str(pos), name]:
                raise ValueError("sample missing, shifted, or position-order confounded")
            if not (fields[4].isdigit() and fields[5].isdigit()):
                raise ValueError("invalid elapsed time or unsigned checksum")
            ns, chk = int(fields[4]), int(fields[5])
            if not (0 < ns < 2**63) or chk != checksum:
                raise ValueError("invalid raw duration or incorrect timed guest output")
            measured[name].append(ns)
            samples.append({"block": block, "pos": pos, "variant": name,
                            "elapsed_ns": ns, "checksum": chk})
        baseline = statistics.median(measured["direct"])
        candidate = statistics.median(measured["reuse"])
        block_ratios.append(candidate / baseline)
    source_dir = Path(__file__).resolve().parent
    source_paths = (
        source_dir / "inprocess_divrem_28" / "src" / "main.rs",
        source_dir / "inprocess_divrem_28" / "Cargo.toml",
        source_dir / "qualify_udivrem_inprocess_28.py",
    )
    return {
        "schema": "zeno-fcis/tier1-udivrem-balanced-call-timing-audit/1",
        "status": "source-bound-native-witnesses-and-balanced-exploratory-timing",
        "upstream_commit": UPSTREAM, "source_receipt_sha256": sha(receipt_bytes),
        "source_artifacts": verified, "cargo_lock_sha256": sha(lock),
        "source_file_sha256": {str(p.relative_to(source_dir)): sha(p.read_bytes())
                               for p in source_paths},
        "raw_sha256": sha(raw.read_bytes()),
        "checks": {"verified_pairs_per_variant": len(inputs()),
                   "zero_divisor_trap_on_original_AOT": True,
                   "sample_checksum": checksum, "calls_per_sample": calls,
                   "balanced_blocks": blocks},
        "timing": {
            "reuse_over_direct_median_of_within_block_ratios": statistics.median(block_ratios),
            "block_ratio_min": min(block_ratios), "block_ratio_max": max(block_ratios),
            "per_block_ratios": block_ratios,
            "sample_count": len(samples),
            "classification": "exploratory-virtualized-call-level-not-throughput-proof",
        },
        "samples": samples,
        "nonclaims": [
            "WAT programs differ; no automatic compiler optimizer improvement is shown",
            "not isolated x86 division latency, throughput, instructions retired or power",
            "time includes Wasmtime typed-function calls, reference-free guest evaluation, host loop overhead",
            "one shared virtualized runner; frequency/scheduling/cache effects unqualified",
            "zero AOT footprint savings in the source-formulation experiment",
        ],
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    for key in ("source-root", "raw", "cargo-lock", "out"):
        parser.add_argument("--" + key, required=True, type=Path)
    parser.add_argument("--calls", required=True, type=int)
    parser.add_argument("--blocks", required=True, type=int)
    a = parser.parse_args()
    if a.out.exists():
        parser.error("refusing to overwrite timing qualification")
    report = audit(a.source_root.resolve(), a.raw.resolve(),
                   a.cargo_lock.resolve(), a.calls, a.blocks)
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text(json.dumps(report, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"status": report["status"], "timing": report["timing"]}, sort_keys=True))


if __name__ == "__main__":
    main()
