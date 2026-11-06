#!/usr/bin/env python3
"""Independent full-file/native ELF cost and second-oracle finite witness replay.

Do not trust producer-reported deltas or equivalence attestations. The named
VeriISLE proof is separate. No measured improvement is assumed.
"""
from __future__ import annotations
import argparse
import json
from pathlib import Path

from install_isle_candidate import UPSTREAM
from install_overflow_forced_28 import MODES, digest, FORCED_RULE
from qualify_overflow_forced_28 import SCHEMA
from independent_recheck_scaling_28 import independent_elf_text_size
from independent_recheck_overflow_match_canary_28 import independent_value
import qualify_rust_idiom_wasm_28 as rust
import qualify_wasmtime_frontend_28 as frontend

WITNESSES = ((0, 0), (1, 2), (-1, -2), (frontend.MAX, 1),
             (frontend.MIN, -1), (frontend.MAX, frontend.MAX),
             (frontend.MIN, frontend.MIN), (frontend.MAX, frontend.MIN))
FUNCTIONS = ("sat_add", "sat_add_xor", "sat_add_mix", "wrap_add")
SCHEMA_OUT = "zeno-fcis/tier1-overflow-forced-independent-recheck/1"


def validate(path: Path, bins: dict[str, Path]) -> dict:
    raw = path.read_bytes()
    report = json.loads(raw)
    if (report.get("schema") != SCHEMA or
            report.get("status") != "real-Wasmtime-AOT-three-way-finite-checked"
            or report.get("source", {}).get("upstream_commit") != UPSTREAM
            or report.get("forced_rule_sha256") != digest(FORCED_RULE.encode())
            or report.get("rust_source_sha256") != digest(rust.SOURCE.read_bytes())
            or report.get("producer_script_sha256") != digest(
               (Path(__file__).parent / "qualify_overflow_forced_28.py").read_bytes())
            or report.get("modes") != list(MODES)
            or report.get("target") != rust.TARGET
            or set(bins) != set(MODES)
            or len(report.get("profiles", [])) != len(rust.PROFILES)):
        raise ValueError("experiment identity/source or coverage mismatch")
    identity = report["source"].get("compiler_identities", {})
    if set(identity) != set(MODES):
        raise ValueError("missing compiler identities")
    for mode in MODES:
        exe, rec = bins[mode], identity[mode]
        if (not exe.is_file() or
                digest(exe.read_bytes()) != rec.get("sha256") or
                exe.stat().st_size != rec.get("bytes") or
                frontend.run([str(exe), "--version"]).stdout.strip() != rec.get("version")):
            raise ValueError("compiler binary provenance failure")
    rows, seen = [], {}
    for position, (profile, opt) in enumerate(rust.PROFILES.items()):
        row = report["profiles"][position]
        if (row.get("profile") != profile or row.get("rust_opt_level") != opt
                or set(row.get("variants", {})) != set(MODES)):
            raise ValueError("missing/altered profile or variants")
        folder = path.parent / profile
        wasm = (folder / "idiom.wasm").read_bytes()
        if (digest(wasm) != row.get("source_wasm", {}).get("sha256")
                or len(wasm) != row["source_wasm"].get("bytes")):
            raise ValueError("source Wasm identity mismatch")
        results, witnesses = {}, {}
        for mode in MODES:
            meta = row["variants"][mode]
            sub = folder / mode
            aot = sub / "idiom.cwasm"
            data = aot.read_bytes()
            if (not data or digest(data) != meta.get("sha256")
                    or len(data) != meta.get("aot_bytes")):
                raise ValueError("emitted binary absent, changed or wrongly sized")
            dis = (sub / "disassembly.txt").read_bytes()
            if digest(dis) != meta.get("disassembly_sha256"):
                raise ValueError("disassembly identity mismatch")
            # Independent GNU ELF parser; do not extrapolate instruction bytes.
            text = independent_elf_text_size(aot)
            if text != meta.get("elf_text_bytes"):
                raise ValueError("independent ELF .text cost mismatch")
            if meta.get("finite", {}).get("passed") is not True:
                raise ValueError("original finite test attestation absent")
            checked = 0
            for fn in FUNCTIONS:
                for x, y in WITNESSES:
                    actual = frontend.invoke(bins[mode], aot, fn, x, y)
                    if actual != independent_value(fn, x, y):
                        raise ValueError(f"second-oracle mismatch {mode}/{profile}/{fn}")
                    checked += 1
            for x in (frontend.MIN, -1, 0, frontend.MAX):
                actual = frontend.invoke(bins[mode], aot, "sign_clamp", x)
                expected = frontend.MIN if x < 0 else frontend.MAX
                if actual != expected:
                    raise ValueError("second-oracle sign clamp mismatch")
                checked += 1
            results[mode] = {"aot_sha256": digest(data), "aot_bytes": len(data),
                             "elf_text_bytes": text,
                             "disassembly_sha256": digest(dis)}
            witnesses[mode] = {"passed": True, "checks": checked,
                               "scope": "additional finite native-Wasm replay, not all-input proof"}
            seen[(profile, mode)] = (digest(data), len(data), text)
        b = results["baseline"]
        comparisons = {}
        for mode in MODES[1:]:
            p = results[mode]
            comparisons[mode] = {
                "whole_cwasm_bytes": b["aot_bytes"] - p["aot_bytes"],
                "elf_text_bytes": b["elf_text_bytes"] - p["elf_text_bytes"],
                "aot_binary_identical": b["aot_sha256"] == p["aot_sha256"],
                "disassembly_identical":
                    b["disassembly_sha256"] == p["disassembly_sha256"],
            }
        if comparisons != row.get("baseline_minus_candidates"):
            raise ValueError("tampered or false cost deltas")
        rows.append({"profile": profile, "verified": results,
                     "independent_finite_replay": witnesses,
                     "verified_cost": comparisons})
    catalog = report.get("deduplicated_catalog", [])
    expected_keys = set()
    for (profile, mode), (hashval, count, text) in seen.items():
        source_row = next(r for r in report["profiles"] if r["profile"] == profile)
        key = digest(json.dumps({
            "source": source_row["source_wasm"]["sha256"],
            "native": hashval, "target": rust.TARGET,
        }, sort_keys=True).encode())
        expected_keys.add(key)
        candidates = [item for item in catalog if item.get("content_key") == key]
        if len(candidates) != 1 or candidates[0].get("aot_bytes") != count or \
                candidates[0].get("text_bytes") != text or \
                candidates[0].get("aot_sha256") != hashval:
            raise ValueError("deduplicated cost catalog incomplete or forged")
        expected_modes = [m for m in MODES if seen[(profile, m)][0] == hashval]
        if candidates[0].get("equivalent_byte_modes") != expected_modes:
            raise ValueError("catalog aliases do not match actual binary hashes")
    if len(catalog) != len(expected_keys):
        raise ValueError("catalog contains duplicates/uncovered elements")
    return {"schema": SCHEMA_OUT, "status": "independent-ELF-full-AOT-and-execution-recheck",
            "original_source_receipt_sha256": digest(raw),
            "upstream_commit": UPSTREAM, "profiles": rows,
            "limits": ["finite execution plus separate QF_BV proof of abstract predicate",
                       "named forced-rule VeriISLE result independently required",
                       "no instruction-byte claims, throughput, ABI or target-wide proof"]}


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--source-receipt", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    for mode in MODES:
        p.add_argument("--" + mode + "-bin", type=Path, required=True)
    args = p.parse_args()
    if args.out.exists():
        p.error("refuse overwrite of independent audit")
    result = validate(args.source_receipt.resolve(),
       {m: getattr(args, m+"_bin").resolve() for m in MODES})
    args.out.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": result["status"],
                      "profiles": [(r["profile"], r["verified_cost"])
                                   for r in result["profiles"]]}, sort_keys=True))


if __name__ == "__main__":
    main()
