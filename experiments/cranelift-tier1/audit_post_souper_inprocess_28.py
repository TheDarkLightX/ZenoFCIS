#!/usr/bin/env python3
"""Independent replay of *post-Souper*, identical-AOT in-process timing.

Do not interpret a CI VM timing ratio as a general speedup. Refuse stale
producer receipts, tampered AOT binaries, changed controls and malformed timing
transcripts before publishing any comparison.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import statistics
from pathlib import Path

import bench_udivrem_post_souper_28 as original

UPSTREAM = "8b19f199f89da864c529b3fa3ca2f7e40d5de609"
PRODUCER_HEAD = "f6edf2bdeea71f2f2b43111370496857601aa65f"
PRODUCER_RUN = 44056104464
PRODUCER_ARTIFACT = 14316481158
PRODUCER_RECEIPT_SHA = "4a5ebf4f50c43338f663138fe870bc5bc6095bd8aeb7e54b696b2eacc302bf1e"
SCHEMA = "zenofcis/tier1-post-souper-inprocess/1"
TARGET = "x86_64-unknown-linux"
SEED = 781234567891


def sha(path: Path) -> str:
    if not path.is_file() or path.is_symlink():
        raise ValueError(f"missing or linked evidence: {path}")
    return hashlib.sha256(path.read_bytes()).hexdigest()


def producer(root: Path) -> dict:
    if sha(root / "runtime_screen.json") != PRODUCER_RECEIPT_SHA:
        raise ValueError("archived runtime receipt hash drift")
    doc = json.loads((root / "runtime_screen.json").read_text())
    if (doc.get("schema") != "zenofcis/tier1-post-souper-runtime-screen/1"
        or doc.get("source_pin") != UPSTREAM
        or doc.get("zenofcis_source_pin") != PRODUCER_HEAD
        or doc.get("target") != TARGET
        or set(doc.get("fixtures", {})) != set(original.TYPES)
        or doc.get("method", {}).get("seed") != SEED
        or doc.get("method", {}).get("iterations") != 5_000_000):
        raise ValueError("wrong runtime source/target/fixture contract")
    if (root / "wasmtime-head.txt").read_text().strip() != UPSTREAM:
        raise ValueError("compiler source revision changed")
    if (root / "zenofcis-head.txt").read_text().strip() != PRODUCER_HEAD:
        raise ValueError("producer experiment revision changed")
    for mode in ("baseline", "patched"):
        if sha(root / mode / "wasmtime") != doc["compiler_pins"][mode]["sha256"]:
            raise ValueError("untrusted compiled runtime: " + mode)
    for kind in original.TYPES:
        fixture = doc["fixtures"][kind]
        wat = root / "runtime-screen" / (kind + ".wat")
        if (sha(wat) != fixture["wat_sha256"]
            or wat.read_text() != original.source(kind)):
            raise ValueError("not the identical declared WAT fixture: " + kind)
        for mode in ("baseline", "patched"):
            recorded = fixture["aot"][mode]
            aot = root / "runtime-screen" / f"{kind}-{mode}.cwasm"
            dis = root / "runtime-screen" / f"{kind}-{mode}.disassembly.txt"
            if (sha(aot) != recorded["sha256"] or aot.stat().st_size != recorded["size"]
                or sha(dis) != recorded["disassembly_sha256"]):
                raise ValueError(f"{kind}/{mode}: changed compiled bytes or disassembly")
    if (doc["fixtures"]["pair"]["costs"]["baseline"]["static_divisions"] != 2
        or doc["fixtures"]["pair"]["costs"]["patched"]["static_divisions"] != 1
        or any(doc["fixtures"]["quotient_control"]["costs"][m]["static_divisions"] != 1
               for m in ("baseline", "patched"))):
        raise ValueError("expected experimental and negative-control code pattern missing")
    if (doc["fixtures"]["quotient_control"]["aot"]["baseline"]["sha256"]
        != doc["fixtures"]["quotient_control"]["aot"]["patched"]["sha256"]):
        raise ValueError("no-fusion control binaries are not identical")
    return doc


def parse_rows(lines: list[str], iterations: int, cycles: int) -> dict:
    if not (10_000 <= iterations <= 8_000_000 and 2 <= cycles <= 12):
        raise ValueError("unadmitted timing budget")
    kinds = original.TYPES
    if (len(lines) != 3 + 8 * cycles or
        lines[0] != f"META,{SCHEMA},{iterations},{cycles},{SEED}"):
        raise ValueError("missing/incomplete ordered timing transcript")
    values = {k: original.expected(k, iterations, SEED) for k in kinds}
    for i, kind in enumerate(kinds):
        small = original.expected(kind, 37, SEED)
        if lines[i + 1] != f"CHECK,{kind},37,{small},{iterations},{values[kind]}":
            raise ValueError("finite check transcript does not match independent oracle")
    samples = []
    summary = {}
    offset = 3
    for kind in kinds:
        times = {"baseline": [], "patched": []}
        cycle_ratios = []
        for cycle in range(cycles):
            order = ("baseline", "patched", "patched", "baseline") if cycle % 2 == 0 else (
                     "patched", "baseline", "baseline", "patched")
            within = {"baseline": [], "patched": []}
            for pos, mode in enumerate(order):
                parts = lines[offset].split(",")
                offset += 1
                if (len(parts) != 7 or parts[:5] !=
                    ["SAMPLE", kind, str(cycle), str(pos), mode]):
                    raise ValueError("unbalanced, out-of-order or mislabelled sample")
                try:
                    elapsed, value = int(parts[5]), int(parts[6])
                except ValueError as error:
                    raise ValueError("non-numeric duration/output") from error
                if (str(elapsed) != parts[5] or str(value) != parts[6]
                    or not (0 < elapsed < 2**63) or value != values[kind]):
                    raise ValueError("invalid duration or independent result checksum")
                within[mode].append(elapsed)
                times[mode].append(elapsed)
                samples.append({"fixture": kind, "cycle": cycle, "position": pos,
                                "compiler": mode, "duration_ns": elapsed, "result": value})
            cycle_ratios.append(statistics.median(within["patched"]) /
                                statistics.median(within["baseline"]))
        b, p = statistics.median(times["baseline"]), statistics.median(times["patched"])
        summary[kind] = {"median_ns": {"baseline": b, "patched": p},
                         "patched_over_baseline_ratio": p / b,
                         "median_paired_cycle_ratio": statistics.median(cycle_ratios),
                         "per_cycle_ratios": cycle_ratios,
                         "samples_per_compiler": 2 * cycles}
    return {"fixtures": summary, "samples": samples}


def audit(root: Path, raw: Path, lock: Path, iterations: int, cycles: int) -> dict:
    doc = producer(root)
    if not lock.is_file() or b'name = "wasmtime"' not in lock.read_bytes():
        raise ValueError("not the pinned Rust runtime lockfile")
    transcript = [s.strip() for s in raw.read_text().splitlines() if s.strip()]
    parsed = parse_rows(transcript, iterations, cycles)
    source = Path(__file__).resolve().parent
    sources = [source / "inprocess_post_souper_28" / "Cargo.toml",
               source / "inprocess_post_souper_28" / "src" / "main.rs",
               source / "bench_udivrem_post_souper_28.py", Path(__file__)]
    return {
        "schema": "zenofcis/tier1-post-souper-same-aot-inprocess-audit/1",
        "status": "finite-arithmetic-and-balanced-inprocess-exploratory-timing",
        "upstream": UPSTREAM, "producer_head": PRODUCER_HEAD,
        "producer_run": PRODUCER_RUN, "producer_artifact": PRODUCER_ARTIFACT,
        "producer_receipt_sha256": PRODUCER_RECEIPT_SHA,
        "source_sha256": {str(p.relative_to(source)): sha(p) for p in sources},
        "rust_lock_sha256": sha(lock), "raw_transcript_sha256": sha(raw),
        "compiler_sha256": {m: doc["compiler_pins"][m]["sha256"] for m in ("baseline", "patched")},
        "aot_sha256": {k: {m: doc["fixtures"][k]["aot"][m]["sha256"]
                           for m in ("baseline", "patched")} for k in original.TYPES},
        "method": {"iterations": iterations, "cycles": cycles,
                   "samples": 8 * cycles, "includes_process_start_or_aot_load": False,
                   "one Rust process, one Wasmtime Engine, four Stores, balanced ABBA/BAAB"},
        **parsed,
        "nonclaims": [
            "Single virtualized CI host and CPU: no representative workload or universal speedup",
            "Wasm store/call overhead and OS scheduling remain inside timed regions",
            "Finite arithmetic checks, not full compiler-pass, x86 flags/ABI or poison/undef proof",
            "Same-WAT static bytes and actual execution times are different cost evidence",
            "No new ZenoFCIS-synthesized instruction sequence or upstream patch",
        ],
    }


def self_test() -> None:
    assert original.expected("pair", 1, SEED) == original.expected("pair", 1, SEED)
    n, cycles = 10_000, 2
    rows = [f"META,{SCHEMA},{n},{cycles},{SEED}"]
    for k in original.TYPES:
        rows.append(f"CHECK,{k},37,{original.expected(k,37,SEED)},{n},{original.expected(k,n,SEED)}")
    for k in original.TYPES:
        for cycle in range(cycles):
            order = ("baseline", "patched", "patched", "baseline") if cycle % 2 == 0 else (
                "patched", "baseline", "baseline", "patched")
            for pos, mode in enumerate(order):
                rows.append(f"SAMPLE,{k},{cycle},{pos},{mode},{1000 + pos},{original.expected(k,n,SEED)}")
    assert len(parse_rows(rows, n, cycles)["samples"]) == 16
    broken = [list(rows) for _ in range(4)]
    broken[0][3] = broken[0][3].replace(",baseline,", ",patched,")
    broken[1][3] = broken[1][3].replace(",1000,", ",0,")
    broken[2][3] = broken[2][3][:-1] + "9"
    broken[3].pop()
    for case in broken:
        try:
            parse_rows(case, n, cycles)
        except ValueError:
            continue
        raise AssertionError("corrupted timing transcript accepted")
    print("balanced in-process schema, deterministic oracle and four negative mutations passed")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--source-root", type=Path)
    parser.add_argument("--raw", type=Path)
    parser.add_argument("--lock", type=Path)
    parser.add_argument("--iterations", type=int, default=5_000_000)
    parser.add_argument("--cycles", type=int, default=6)
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    if args.self_test:
        self_test()
    else:
        if not all((args.source_root, args.raw, args.lock, args.out)) or args.out.exists():
            parser.error("new --out and --source-root --raw --lock required")
        result = audit(args.source_root, args.raw, args.lock, args.iterations, args.cycles)
        args.out.write_text(json.dumps(result, sort_keys=True, indent=2) + "\n")
        print(json.dumps({"status": result["status"],
                          "ratios": {k: v["patched_over_baseline_ratio"]
                                     for k, v in result["fixtures"].items()}}, sort_keys=True))
