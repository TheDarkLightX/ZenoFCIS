#!/usr/bin/env python3
"""Independent native AOT ELF/archive recheck and additional real Wasm witnesses."""
from __future__ import annotations
import argparse
import json
import subprocess
from pathlib import Path

from independent_recheck_scaling_28 import independent_elf_text_size
from independent_recheck_rust_idiom_28 import independent_result
from install_isle_candidate import UPSTREAM, SOURCE, ORIGINAL_GIT_BLOB
from install_rust_overflow_predicate_28 import MODES, RULE, sha, source
from qualify_rust_overflow_predicate_28 import SCHEMA
import qualify_rust_idiom_wasm_28 as rust
import qualify_wasmtime_frontend_28 as frontend

CHECK_SCHEMA = "zeno-fcis/rust-llvm-overflow-predicate-native-independent/1"
HERE = Path(__file__).parent


def audit(report_file: Path, upstream: Path, bins: dict, receipts: dict) -> dict:
    raw = report_file.read_bytes()
    report = json.loads(raw)
    if (report.get("schema") != SCHEMA or
        report.get("status") != "native-wasmtime-AOT-finite-checked-no-speed-claim" or
        report.get("sources", {}).get("upstream_commit") != UPSTREAM or
        report.get("sources", {}).get("upstream_original_blob") != ORIGINAL_GIT_BLOB or
        report.get("rust_source_sha256") != sha(rust.SOURCE.read_bytes()) or
        report.get("producer_script_sha256") != sha((HERE / "qualify_rust_overflow_predicate_28.py").read_bytes()) or
        report.get("installer_script_sha256") != sha((HERE / "install_rust_overflow_predicate_28.py").read_bytes()) or
        report.get("rule_sha256") != sha(RULE.encode()) or
        report.get("target") != rust.TARGET or
        report.get("wasm_target") != rust.RUST_TARGET or
        report.get("modes") != list(MODES) or
        len(report.get("profiles", [])) != len(rust.PROFILES) or
        set(bins) != set(MODES) or set(receipts) != set(MODES)):
        raise ValueError("incomplete or altered experiment identity")
    if subprocess.check_output(["git","-C",str(upstream),"rev-parse","HEAD"],
                                text=True).strip() != UPSTREAM:
        raise ValueError("upstream commit drift")
    if (upstream / SOURCE).read_text() != source(upstream, "xorand"):
        raise ValueError("unrecognized patched compiler tree")

    identities = report["sources"].get("compilers", {})
    if set(identities) != set(MODES):
        raise ValueError("missing binary identities")
    for mode in MODES:
        binary, info = bins[mode], identities[mode]
        receipt = json.loads(receipts[mode].read_bytes())
        if (not binary.is_file() or
            sha(binary.read_bytes()) != info.get("sha256") or
            binary.stat().st_size != info.get("bytes") or
            subprocess.check_output([str(binary),"--version"],text=True).strip() != info.get("version") or
            receipt != info.get("source_receipt") or
            receipt.get("source_sha256") != sha(source(upstream, mode).encode())):
            raise ValueError("compiler/source substitution in " + mode)

    rows, dedup, seen = [], [], set()
    root = report_file.parent
    expected_finite = {"sat_add": len(rust.VECTORS),
                       "sat_add_xor": len(rust.VECTORS),
                       "sat_add_mix": len(rust.VECTORS),
                       "wrap_add": len(rust.VECTORS),
                       "sign_clamp": len(set(x for x, _ in rust.VECTORS))}
    for index,(profile,opt) in enumerate(rust.PROFILES.items()):
        row = report["profiles"][index]
        if (row.get("profile") != profile or row.get("rust_opt_level") != opt or
            set(row.get("compiled", {})) != set(MODES)):
            raise ValueError("profile omitted, reordered or duplicated")
        directory = root / profile
        wat = (directory / "idiom.wasm").read_bytes()
        identity = row["wasm_source"]
        if (not wat or sha(wat) != identity.get("sha256") or
            len(wat) != identity.get("bytes") or
            identity.get("argv", [None])[0] != "rustc"):
            raise ValueError("generated Rust Wasm mismatch")
        actuals, additional = {}, {}
        for mode in MODES:
            item = row["compiled"][mode]
            folder = directory / mode
            obj = folder / "idiom.cwasm"
            binary = obj.read_bytes()
            dis = (folder / "disassembly.txt").read_bytes()
            if (not binary or sha(binary) != item.get("sha256") or
                len(binary) != item.get("aot_bytes") or
                sha(dis) != item.get("disassembly_sha256") or
                independent_elf_text_size(obj) != item.get("elf_text_bytes")):
                raise ValueError("actual compiled native ELF/archive cost differs")
            finite = item.get("finite", {})
            if (finite.get("passed") is not True or
                finite.get("type") != "finite-Wasm-execution-only-not-proof" or
                finite.get("functions") != expected_finite):
                raise ValueError("producer's finite checks absent")
            count = 0
            for name in ("sat_add", "sat_add_xor", "sat_add_mix", "wrap_add"):
                for x,y in ((0,0), (-(1 << 63),-1), ((1 << 63)-1,1)):
                    if frontend.invoke(bins[mode],obj,name,x,y) != independent_result(name,x,y):
                        raise ValueError("independent witness failed " + name)
                    count += 1
            for x in (-1,0,1):
                if frontend.invoke(bins[mode],obj,"sign_clamp",x) != independent_result("sign_clamp",x):
                    raise ValueError("independent clamp witness failed")
                count += 1
            additional[mode] = count
            actuals[mode] = {
                "sha256": sha(binary), "aot_bytes": len(binary),
                "elf_text_bytes": independent_elf_text_size(obj),
                "disassembly_sha256": sha(dis),
            }
        b, c = actuals["baseline"], actuals["xorand"]
        comparison = {
            "elf_text_baseline_minus_candidate": b["elf_text_bytes"] - c["elf_text_bytes"],
            "complete_cwasm_baseline_minus_candidate": b["aot_bytes"] - c["aot_bytes"],
            "aot_binary_identical": b["sha256"] == c["sha256"],
            "native_disassembly_identical": b["disassembly_sha256"] == c["disassembly_sha256"],
            "runtime_performance": "not-measured",
        }
        if comparison != row.get("comparison"):
            raise ValueError("falsified compiler savings")
        for mode in MODES:
            item = actuals[mode]
            key = sha(json.dumps({
                "wasm_sha256": identity["sha256"], "target": rust.TARGET,
                "aot_sha256": item["sha256"],
            },sort_keys=True).encode())
            if key not in seen:
                seen.add(key)
                dedup.append({
                    "content_key": key, "profile": profile, "mode": mode,
                    "wasm_sha256": identity["sha256"],
                    "aot_sha256": item["sha256"], "aot_bytes": item["aot_bytes"],
                    "elf_text_bytes": item["elf_text_bytes"],
                    "same_native_bytes_modes": [m for m in MODES if actuals[m]["sha256"] == item["sha256"]],
                    "semantic_contract": "signed i64 saturation + downstream Rust wrapping operations, finite checked",
                    "source_receipt_sha256": sha(receipts[mode].read_bytes()),
                    "verification_receipt": "separate same-workflow independent QF_BV proof",
                    "cost_evidence": "real same-Wasm pinned Wasmtime AOT and ELF section",
                })
        rows.append({"profile": profile, "measured_deltas": comparison,
                     "independent_witnesses_per_compiler": additional})
    if dedup != report.get("deduplicated_catalog"):
        raise ValueError("deduplicated cost ledger was forged or omitted")
    return {"schema": CHECK_SCHEMA,
            "status": "independent-real-AOT-ELF-and-Wasm-witnesses-rechecked",
            "upstream_commit": UPSTREAM, "source_receipt_sha256": sha(raw),
            "catalog_entries": len(dedup), "profiles": rows,
            "limits": ["only finite executable witnesses; independent QF_BV proof separately",
                       "ISLE rule applicability or flags/ABI/poison not thereby proven",
                       "no power/latency/throughput data", "not representative production code"]}


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--source-receipt", type=Path, required=True)
    p.add_argument("--upstream", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    for mode in MODES:
        p.add_argument("--" + mode + "-bin", type=Path, required=True)
        p.add_argument("--" + mode + "-receipt", type=Path, required=True)
    a = p.parse_args()
    if a.out.exists():
        p.error("refusing to overwrite independent receipt")
    result = audit(a.source_receipt.resolve(), a.upstream.resolve(),
        {m:getattr(a,m+"_bin").resolve() for m in MODES},
        {m:getattr(a,m+"_receipt").resolve() for m in MODES})
    a.out.parent.mkdir(parents=True,exist_ok=True)
    a.out.write_text(json.dumps(result,sort_keys=True,indent=2)+"\n")
    print(json.dumps({"status":result["status"],"profiles":result["profiles"]},sort_keys=True))


if __name__=="__main__":
    main()
