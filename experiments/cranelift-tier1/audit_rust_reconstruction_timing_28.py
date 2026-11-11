#!/usr/bin/env python3
"""Independent source/hash, checksum, and balanced-timing audit.

Consumes one archived CI producer for identical optimized Rust-generated Wasm
compiled with pristine/patched Wasmtime. No throughput or portable speedup
claims; native code byte costs come from the distinct archived disassembly audit.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import statistics
from pathlib import Path

HERE = Path(__file__).resolve().parent
UPSTREAM = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
PRODUCER_HEAD = "e99ab17875bbb6d294c9f0cfef80c2b51d28a4f6"
PRODUCER_RUN = 42951667233
PRODUCER_ARTIFACT = 13825142647
SCHEMA = "zenofcis/tier1-rust-reconstruction-call-timing/2"
FUNCS = ("divrem_pair", "divrem_offset", "divrem_reverse",
         "divrem_swapped", "div_only", "signed_divrem")
MASK = (1 << 64) - 1
MIN = -(1 << 63)


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def signed(v: int) -> int:
    v &= MASK
    return v if v < 1 << 63 else v - (1 << 64)


def rotl17(v: int) -> int:
    v &= MASK
    return ((v << 17) | (v >> 47)) & MASK


def inputs() -> list[tuple[int, int]]:
    v = [(1,1),(7,3),(MIN,1),((1<<63)-1,1),(-1,-1),
         ((1<<63)-1,MIN),(MIN,(1<<63)-1),(123456789,-7),
         (1,-1),(-1,1),(MIN,3),((1<<63)-1,7),(7,2),
         (MIN,2),(123456789,98765),(-987654321,31337)]
    s = 0x5623202611
    for _ in range(256):
        s ^= (s << 13) & MASK
        s ^= s >> 7
        s ^= (s << 17) & MASK
        s &= MASK
        x = signed(s or 1)
        s ^= (s << 13) & MASK
        s ^= s >> 7
        s ^= (s << 17) & MASK
        s &= MASK
        y = signed(s or 1)
        if x == MIN and y == -1:
            continue
        v.append((x, y))
    assert all(x and y and not (x == MIN and y == -1) for x,y in v)
    return v


def oracle(name: str, x: int, y: int) -> int:
    if y == 0 or (name == "divrem_swapped" and x == 0):
        raise ValueError("excluded denominator")
    if name == "signed_divrem":
        if x == MIN and y == -1:
            raise ValueError("excluded signed overflow")
        q = (abs(x) // abs(y)) * (-1 if (x < 0) != (y < 0) else 1)
        r = x - q * y
        return signed((q & MASK) ^ rotl17(r))
    a, b = x & MASK, y & MASK
    q = a // b
    r = b % a if name == "divrem_swapped" else a % b
    if name == "div_only":
        return signed(q)
    if name == "divrem_offset":
        q = (q + 3) & MASK
    if name not in FUNCS:
        raise ValueError("unrecognized export")
    return signed(q ^ rotl17(r))


def expected_checksum(name: str, calls: int) -> int:
    pairs = inputs()
    total = 0
    for k in range(calls):
        total = (total + oracle(name, *pairs[k % len(pairs)])) & MASK
    return total


def source_root(folder: Path) -> Path:
    hits = [p for p in (folder, folder/"tier1-reconstruction")
            if (p/"results/qualification.json").is_file()
            and (p/"compilers/qualification.json").is_file()]
    if len(hits) != 1:
        raise ValueError("missing or ambiguous archived producer root")
    return hits[0]


def validate_producer(folder: Path) -> dict:
    root = source_root(folder)
    q = json.loads((root/"results/qualification.json").read_text())
    c = json.loads((root/"compilers/qualification.json").read_text())
    if (q.get("schema") != "zenofcis/tier1-rust-divrem-generated-wasm/1"
        or q.get("source_pin") != UPSTREAM
        or not str(q.get("rustc","")).startswith("rustc 1.97.1 ")
        or q.get("native_target") != "x86_64-unknown-linux"
        or q.get("rust_target") != "wasm32-unknown-unknown"
        or q.get("status") != "finite-checked-generated-wasm-target-audit"
        or c.get("upstream") != UPSTREAM
        or q.get("archive_receipt_sha256") != sha(root/"compilers/qualification.json")
        or q.get("rust_source_sha256") != sha(HERE/"idiom_divrem_28.rs")
        or q.get("producer_sha256") != sha(HERE/"qualify_rust_divrem_28.py")):
        raise ValueError("producer identity/source pin mismatch")
    opt = q.get("profiles",{}).get("optimized",{})
    if opt.get("rust_opt") != "3":
        raise ValueError("wrong Rust optimization profile")
    wasm = root/"results/optimized/idiom.wasm"
    if opt.get("wasm_sha256") != sha(wasm) or opt.get("wasm_bytes") != wasm.stat().st_size:
        raise ValueError("Rust-generated Wasm mismatch")
    results = opt.get("results",{})
    if set(results) != {"baseline", "patched"}:
        raise ValueError("compiler pair absent")
    archive = {}
    for side in ("baseline","patched"):
        row = results[side]
        binary = root/f"compilers/{side}/wasmtime"
        aot = root/f"results/optimized/{side}/native.cwasm"
        if (sha(binary) != row.get("compiler_sha256")
            or row.get("compiler_sha256") != c["results"][side]["compiler_sha256"]
            or sha(aot) != row.get("aot_sha256")
            or aot.stat().st_size != row.get("whole_aot_bytes")
            or row.get("finite",{}).get("value",0) <= 0
            or row.get("finite",{}).get("trap_or_panic",0) <= 0):
            raise ValueError("compiler/AOT/proof corpus source mismatch: "+side)
        archive[side] = {"aot_sha256": sha(aot), "compiler_sha256": sha(binary),
                         "aot_bytes": aot.stat().st_size}
    if archive["baseline"]["aot_sha256"] == archive["patched"]["aot_sha256"]:
        raise ValueError("no candidate AOT difference")
    return {"archive": archive, "root": str(root),
            "wasm_sha256": sha(wasm), "producer_receipt_sha256": sha(root/"results/qualification.json"),
            "producer_head": PRODUCER_HEAD, "run": PRODUCER_RUN,
            "artifact": PRODUCER_ARTIFACT, "upstream": UPSTREAM}


def audit(folder: Path, raw: Path, cargo_lock: Path, calls: int, blocks: int, mode: str) -> dict:
    supported_modes = (
        "ab", "ab_rev", "aa", "aa_rev", "bb", "bb_rev",
        "aa_shared", "aa_shared_rev", "bb_shared", "bb_shared_rev",
    )
    base_mode = mode.removesuffix("_rev").removesuffix("_shared")
    shared_module = "_shared" in mode
    if base_mode not in ("ab","aa","bb") or mode not in supported_modes or not (20_000 <= calls <= 500_000 and 4 <= blocks <= 32
            and blocks % 2 == 0):
        raise ValueError("invalid timing bounds")
    src = validate_producer(folder)
    if b'name = "wasmtime"' not in cargo_lock.read_bytes():
        raise ValueError("missing runtime dependency lock")
    lines = [line.strip() for line in raw.read_text().splitlines() if line.strip()]
    if not lines or lines[0] != f"META,{SCHEMA},{calls},{blocks},5623,{mode}":
        raise ValueError("bad timing metadata")
    cursor = 1
    ratios = {}
    sample_count = 0
    for name in FUNCS:
        count = len(inputs())
        if lines[cursor:cursor+2] != [
            f"CHECK,{name},baseline,{count}",
            f"CHECK,{name},patched,{count}"]:
            raise ValueError("missing native independent-oracle checks: "+name)
        cursor += 2
        checksum = expected_checksum(name, calls)
        blocks_ratios = []
        for block in range(blocks):
            order = ("baseline","patched","patched","baseline") if block % 2 == 0 \
                else ("patched","baseline","baseline","patched")
            measured = {"baseline":[], "patched":[]}
            for position, side in enumerate(order):
                f = lines[cursor].split(",") if cursor < len(lines) else []
                cursor += 1
                if f[:5] != ["SAMPLE", name, str(block), str(position), side] \
                    or len(f) != 7 or not f[5].isdigit() or not f[6].isdigit():
                    raise ValueError("missing/misaligned timing pair: "+name)
                ns, got = int(f[5]), int(f[6])
                if ns <= 0 or ns >= 2**63 or got != checksum:
                    raise ValueError("timed value or duration corruption: "+name)
                measured[side].append(ns)
                sample_count += 1
            blocks_ratios.append(statistics.median(measured["patched"])
                                 / statistics.median(measured["baseline"]))
        ratios[name] = {
            "median_of_paired_block_ratios": statistics.median(blocks_ratios),
            "block_ratios": blocks_ratios,
            "patched_faster_blocks": sum(r < 1 for r in blocks_ratios)}
    if cursor != len(lines):
        raise ValueError("unexpected extra or missing timing entries")
    # This heuristic is a CAUSAL INTERPRETATION CHECK, not a statistical
    # significance test. In the archived byte audit all three controls have
    # unchanged native instruction bytes. Large control timing shifts must
    # invalidate any compiler-optimization speedup interpretation.
    native_path = HERE / "evidence/rust_reconstruction_native_audit_20261111.json"
    native = json.loads(native_path.read_text())
    if (native.get("experimental_zenofcis_commit") != PRODUCER_HEAD
            or native.get("upstream_source_commit") != UPSTREAM
            or native.get("artifact_id") != PRODUCER_ARTIFACT):
        raise ValueError("native code cost provenance mismatch")
    cost = native["native_code_bytes"]["profile_optimized"]
    expected_savings = {"divrem_pair":13, "divrem_offset":13, "divrem_reverse":13,
                        "divrem_swapped":0, "div_only":0, "signed_divrem":0}
    if any(cost[n].get("saved_instruction_bytes") != v
           for n,v in expected_savings.items()):
        raise ValueError("negative control changed or native cost audit mismatch")
    controls = ("divrem_swapped","div_only","signed_divrem") if base_mode == "ab" else FUNCS
    out_of_band = {n:ratios[n]["median_of_paired_block_ratios"]
                   for n in controls
                   if not 0.98 <= ratios[n]["median_of_paired_block_ratios"] <= 1.02}
    attribution = (("AB-CONFOUNDED" if base_mode == "ab" else "SHAM-INSTANCE-LOCATION-DRIFT")
                   if out_of_band else
                   ("AB-controls-within-heuristic-NOT-speedup-proof" if base_mode == "ab"
                    else "SHAM-within-heuristic-NOT-a-performance-proof"))
    return {
        "schema": SCHEMA, "status": "source-bound-finite-checked-exploratory-runtime",
        "producer": src, "host_lock_sha256": sha(cargo_lock),
        "comparison_mode": mode,
        "source_mapping": {"ab":["baseline","patched"],"aa":["baseline","baseline"],"bb":["patched","patched"]}[base_mode],
        "module_construction_order": "right-then-left" if mode.endswith("_rev") else "left-then-right",
        "same_deserialized_module_for_both_instances": shared_module,
        "module_code_placement_control": (
            "two distinct Stores/Instances sharing one deserialized Module and compiled guest code"
            if shared_module else
            "two separately deserialized Module objects; guest code placement may differ"
        ),
        "timing_harness_source_sha256": sha(HERE/"inprocess_rust_reconstruction_28/src/main.rs"),
        "raw_sha256": sha(raw),
        "cost_audit_source_sha256": sha(native_path),
        "unchanged_instruction_byte_controls": list(controls),
        "causal_attribution_status": attribution,
        "control_drift_heuristic": {"tolerance": "ratio in [0.98,1.02] for each unchanged-code function",
                                    "out_of_band_ratios": out_of_band,
                                    "basis": "heuristic only; not a formal causal/statistical guarantee"},
        "native_instruction_byte_savings_by_function": expected_savings if base_mode == "ab"
            else {n:0 for n in FUNCS},
        "timing": {
            "calls_per_sample": calls, "balanced_blocks": blocks,
            "sample_count": sample_count, "functions": ratios},
        "contract": "identical optimized Rust WebAssembly per baseline/patched compiler; each output verified by independent signed/unsigned Python oracle",
        "negative_controls": ["divrem_swapped", "div_only", "signed_divrem"],
        "nonclaims": [
            "one virtualized host, no portable throughput or steady-state speedup conclusion",
            "even with balanced ABBA/BAAB order, unchanged-code control drift can confound causality",
            "sham runs compare identical AOT files in separate module instances with distinct code locations",
            "reversing module construction order probes one code-placement confound but does not fix page layout or native function alignment",
            "shared-module shams reuse compiled guest code, but do not fix Store/Instance, trampoline, VMContext or call-site layout",
            "elapsed call time includes Wasmtime invocation and loop overhead",
            "proof-and-cost artifact separately establishes only per-function native byte savings",
            "no compiler-pass proof, machine-ISA proof, register-flag theorem or general target coverage",
            "the experimental compiler patch was manually designed, not synthesized by ZenoFCIS",
        ],
    }


def self_test() -> None:
    assert oracle("divrem_pair",7,3) == 131074
    assert oracle("divrem_offset",7,3) == 131077
    assert oracle("divrem_reverse",7,3) == 131074
    assert oracle("divrem_swapped",7,3) == 393218
    assert oracle("signed_divrem",-7,3) == 1
    assert oracle("div_only",-1,2) == (1<<63)-1
    assert len(inputs()) >= 270
    assert expected_checksum("div_only",20_000) >= 0
    supported = ("ab","ab_rev","aa","aa_rev","bb","bb_rev",
                 "aa_shared","aa_shared_rev","bb_shared","bb_shared_rev")
    assert len(supported) == len(set(supported)) == 10
    assert {m.removesuffix("_rev").removesuffix("_shared") for m in supported} == {"ab","aa","bb"}
    assert all(m.startswith(("aa","bb")) for m in supported if "_shared" in m)
    print("independent Rust native-value oracle and 10-mode timing self-tests passed")


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--source-root", type=Path)
    p.add_argument("--raw", type=Path)
    p.add_argument("--cargo-lock", type=Path)
    p.add_argument("--out", type=Path)
    p.add_argument("--calls", type=int, default=50_000)
    p.add_argument("--blocks", type=int, default=8)
    p.add_argument("--mode", choices=("ab","ab_rev","aa","aa_rev","bb","bb_rev",
                                      "aa_shared","aa_shared_rev","bb_shared","bb_shared_rev"),
                   default="ab")
    p.add_argument("--self-test", action="store_true")
    a = p.parse_args()
    if a.self_test:
        self_test()
    else:
        if (a.source_root is None or a.raw is None
            or a.cargo_lock is None or a.out is None or a.out.exists()):
            p.error("source-root/raw/cargo-lock/new out required")
        result = audit(a.source_root.resolve(),a.raw.resolve(),
                       a.cargo_lock.resolve(),a.calls,a.blocks,a.mode)
        a.out.parent.mkdir(parents=True,exist_ok=True)
        a.out.write_text(json.dumps(result,indent=2,sort_keys=True)+"\n")
        print(json.dumps(result["timing"],sort_keys=True))
