#!/usr/bin/env python3
"""True same-source AOT cost for original, unforced and *correct forced* rule."""
from __future__ import annotations
import argparse
import hashlib
import json
import platform
import subprocess
from pathlib import Path

from install_isle_candidate import UPSTREAM, SOURCE, ORIGINAL_GIT_BLOB
from install_overflow_forced_28 import MODES, FORCED_RULE, digest, source
import qualify_rust_idiom_wasm_28 as rust

SCHEMA = "zeno-fcis/tier1-overflow-correct-forced-AOT/1"
HERE = Path(__file__).resolve().parent


def pinned(upstream: Path, bins: dict[str, Path], receipts: dict[str, Path]) -> dict:
    if set(bins) != set(MODES) or set(receipts) != set(MODES):
        raise ValueError("missing or extra compiler")
    if platform.machine() not in ("x86_64", "AMD64"):
        raise ValueError("native experiment is x86-64 only")
    def git(*args: str) -> str:
        return subprocess.check_output(
            ["git", "-C", str(upstream), *args], text=True).strip()
    if git("rev-parse", "HEAD") != UPSTREAM:
        raise ValueError("upstream commit moved")
    if git("rev-parse", "HEAD:" + SOURCE) != ORIGINAL_GIT_BLOB:
        raise ValueError("original upstream blob changed")
    if (upstream / SOURCE).read_text() != source(upstream, "forced"):
        raise ValueError("not exact forced-only final ISLE rule")
    if git("diff", "--name-only").splitlines() != [SOURCE]:
        raise ValueError("unreviewed upstream source modifications")
    identities = {}
    for mode in MODES:
        receipt_bytes = receipts[mode].read_bytes()
        receipt = json.loads(receipt_bytes)
        exe = bins[mode]
        if (not exe.is_file() or exe.stat().st_size <= 0
                or receipt.get("schema") != "zeno-fcis/tier1-overflow-forced-correct-rule/1"
                or receipt.get("mode") != mode
                or receipt.get("upstream_commit") != UPSTREAM
                or receipt.get("source_sha256") != digest(source(upstream, mode).encode())):
            raise ValueError("missing compiler or invalid compiler/patch source receipt")
        identities[mode] = {
            "sha256": digest(exe.read_bytes()),
            "bytes": exe.stat().st_size,
            "version": subprocess.check_output(
                [str(exe), "--version"], text=True).strip(),
            "installer_receipt_sha256": digest(receipt_bytes),
        }
    if len(set(v["version"] for v in identities.values())) != 1:
        raise ValueError("compiler versions are inconsistent")
    return {"upstream_commit": UPSTREAM, "original_blob": ORIGINAL_GIT_BLOB,
            "compiler_identities": identities}


def qualify(upstream: Path, bins: dict[str, Path], receipts: dict[str, Path],
            output: Path) -> dict:
    if output.exists():
        raise ValueError("will not overwrite experiment data")
    version = subprocess.check_output(["rustc", "--version"], text=True).strip()
    if not version.startswith("rustc 1.97.1 "):
        raise ValueError("wrong source-to-Wasm compiler")
    pins = pinned(upstream, bins, receipts)
    output.mkdir(parents=True)
    rows, seen, catalog = [], set(), []
    for profile, opt in rust.PROFILES.items():
        folder = output / profile
        folder.mkdir()
        source_wasm = rust.compile_rust(folder, opt)
        wasm = folder / "idiom.wasm"
        results = {}
        for mode in MODES:
            sub = folder / mode
            sub.mkdir()
            receipt = rust.compile_aot(bins[mode], wasm, sub)
            receipt["finite"] = rust.finite(bins[mode], sub / "idiom.cwasm")
            results[mode] = receipt
            key = digest(json.dumps({
                "source": source_wasm["sha256"], "native": receipt["sha256"],
                "target": rust.TARGET,
            }, sort_keys=True).encode())
            if key not in seen:
                seen.add(key)
                catalog.append({
                    "content_key": key, "profile": profile, "mode_first_seen": mode,
                    "wasm_sha256": source_wasm["sha256"],
                    "aot_sha256": receipt["sha256"],
                    "aot_bytes": receipt["aot_bytes"],
                    "text_bytes": receipt["elf_text_bytes"],
                    "equivalent_byte_modes": [m for m in MODES
                        if m in results and results[m]["sha256"] == receipt["sha256"]],
                    "semantics": "signed-i64 saturation and modular downstream Rust ops",
                    "verification": "separate unchanged-spec named VeriISLE required for forced rule; finite code execution only",
                })
        base = results["baseline"]
        deltas = {}
        for mode in MODES[1:]:
            result = results[mode]
            deltas[mode] = {
                "whole_cwasm_bytes": base["aot_bytes"] - result["aot_bytes"],
                "elf_text_bytes": base["elf_text_bytes"] - result["elf_text_bytes"],
                "aot_binary_identical": base["sha256"] == result["sha256"],
                "disassembly_identical":
                    base["disassembly_sha256"] == result["disassembly_sha256"],
            }
        rows.append({"profile": profile, "rust_opt_level": opt,
                     "source_wasm": source_wasm, "variants": results,
                     "baseline_minus_candidates": deltas})
    # Recompute complete deduplicated mode lists, rather than accepting
    # a partial list created while previous candidate variants were still pending.
    for row in rows:
        for entry in catalog:
            if entry["profile"] == row["profile"]:
                entry["equivalent_byte_modes"] = [
                    m for m in MODES if row["variants"][m]["sha256"] == entry["aot_sha256"]]
    return {
        "schema": SCHEMA, "status": "real-Wasmtime-AOT-three-way-finite-checked",
        "source": pins, "rust_version": version,
        "rust_source_sha256": digest(rust.SOURCE.read_bytes()),
        "producer_script_sha256": digest(Path(__file__).read_bytes()),
        "forced_rule_sha256": digest(FORCED_RULE.encode()),
        "target": rust.TARGET, "wasm_target": rust.RUST_TARGET,
        "modes": list(MODES), "profiles": rows,
        "deduplicated_catalog": catalog,
        "nonclaims": [
            "no premise that subsuming an equivalent expression improves real target code",
            "no certified VeriISLE soundness for forced rule before independent named-rule checker",
            "tests do not prove whole backend x86 ABI, flag, poison or optimization-chain semantics",
            "finite Rust source program, not application prevalence, throughput or CPU speed",
            "compared exact same .wasm within profile, not across Rust opt levels",
        ],
    }


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--upstream", required=True, type=Path)
    p.add_argument("--out", required=True, type=Path)
    for mode in MODES:
        p.add_argument("--" + mode.replace("_", "-") + "-bin",
                       dest=mode + "_bin", type=Path, required=True)
        p.add_argument("--" + mode.replace("_", "-") + "-receipt",
                       dest=mode + "_receipt", type=Path, required=True)
    a = p.parse_args()
    report = qualify(a.upstream.resolve(),
        {m: getattr(a, m+"_bin").resolve() for m in MODES},
        {m: getattr(a, m+"_receipt").resolve() for m in MODES},
        a.out.resolve())
    (a.out / "qualification.json").write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": report["status"],
        "outcomes": [{"profile": r["profile"],
                      "costs": r["baseline_minus_candidates"]}
                     for r in report["profiles"]]}, sort_keys=True))


if __name__ == "__main__":
    main()
