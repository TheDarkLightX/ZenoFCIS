#!/usr/bin/env python3
"""Independent remeasurement and native finite oracle for actual LLVM palette.

Rejects missing or substituted compilers, source/Wasm/AOT/disassembly bytes,
misstated deltas, incomplete finite checks and forged deduplicated catalog.
No native machine all-state or runtime speed claim.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import subprocess
from pathlib import Path
from independent_recheck_scaling_28 import independent_elf_text_size
from independent_recheck_rust_idiom_28 import independent_result
from qualify_rust_signmask_palette_28 import SCHEMA
import qualify_rust_idiom_wasm_28 as rust
import qualify_wasmtime_frontend_28 as frontend
from install_rust_signmask_palette_28 import PALETTE, RULES, expected, sha
from install_isle_candidate import UPSTREAM, SOURCE, ORIGINAL_GIT_BLOB

AUDIT_SCHEMA = "zeno-fcis/tier1-rust-llvm-signmask-palette-independent/1"


def check(report_file: Path, upstream: Path, compilers: dict[str, Path],
          patches: dict[str, Path]) -> dict:
    raw = report_file.read_bytes()
    doc = json.loads(raw)
    root = report_file.parent
    here = Path(__file__).parent
    if (doc.get("schema") != SCHEMA
            or doc.get("status") != "finite-wasm-checked-real-palette-native-AOT-cost"
            or doc.get("source", {}).get("upstream_commit") != UPSTREAM
            or doc.get("source", {}).get("original_source_blob") != ORIGINAL_GIT_BLOB
            or doc.get("rust_target") != rust.RUST_TARGET
            or doc.get("native_target") != rust.TARGET
            or doc.get("rust_source_sha256") != sha(rust.SOURCE.read_bytes())
            or doc.get("producer_script_sha256") != sha((here / "qualify_rust_signmask_palette_28.py").read_bytes())
            or doc.get("palette_install_source_sha256") != sha((here / "install_rust_signmask_palette_28.py").read_bytes())
            or doc.get("candidates") != list(PALETTE)
            or not doc.get("rustc_version", "").startswith("rustc 1.97.1 ")
            or len(doc.get("profiles", [])) != len(rust.PROFILES)
            or set(compilers) != set(PALETTE) or set(patches) != set(PALETTE)):
        raise ValueError("wrong signed contract, source or complete palette")
    if subprocess.check_output(["git", "-C", str(upstream), "rev-parse", "HEAD"], text=True).strip() != UPSTREAM:
        raise ValueError("wrong native compiler source")
    if (upstream / SOURCE).read_text() != expected(upstream, "sub"):
        raise ValueError("compiler patch scope no longer final pinned candidate")
    metadata = doc["source"].get("compilers", {})
    if set(metadata) != set(PALETTE):
        raise ValueError("missing compiler identity")
    for mode in PALETTE:
        binary = compilers[mode]
        identity = metadata[mode]
        patch = json.loads(patches[mode].read_bytes())
        if (not binary.is_file() or sha(binary.read_bytes()) != identity.get("sha256")
                or binary.stat().st_size != identity.get("bytes")
                or subprocess.check_output([str(binary), "--version"], text=True).strip() != identity.get("version")
                or identity.get("source") != patch
                or patch.get("rule_sha256") != sha(RULES[mode].encode())
                or patch.get("source_sha256") != sha(expected(upstream, mode).encode())):
            raise ValueError("compiler or source patch substitution: " + mode)
    rows, catalog, unique = [], [], set()
    finite_expected = {
        "sat_add": len(rust.VECTORS), "sat_add_xor": len(rust.VECTORS),
        "sat_add_mix": len(rust.VECTORS), "wrap_add": len(rust.VECTORS),
        "sign_clamp": len(set(x for x, _ in rust.VECTORS)),
    }
    for position, (profile, opt) in enumerate(rust.PROFILES.items()):
        row = doc["profiles"][position]
        if (row.get("profile") != profile or row.get("rust_opt_level") != opt
                or set(row.get("compiled", {})) != set(PALETTE)):
            raise ValueError("missing, reordered, or duplicated profile")
        folder = root / profile
        wasm = (folder / "idiom.wasm").read_bytes()
        ws = row["wasm_source"]
        if (not wasm or sha(wasm) != ws.get("sha256")
                or len(wasm) != ws.get("bytes")
                or ws.get("argv", [None])[0] != "rustc"):
            raise ValueError("Rust-generated Wasm source changed")
        data = {}
        independent_checks = {}
        for mode in PALETTE:
            entry = row["compiled"][mode]
            obj = folder / mode / "idiom.cwasm"
            original = obj.read_bytes()
            disassembly = (folder / mode / "disassembly.txt").read_bytes()
            if (not original or sha(original) != entry.get("sha256")
                    or len(original) != entry.get("aot_bytes")
                    or sha(disassembly) != entry.get("disassembly_sha256")):
                raise ValueError("changed AOT bytes or disassembly")
            actual_text = independent_elf_text_size(obj)
            if actual_text != entry.get("elf_text_bytes"):
                raise ValueError("recomputed ELF .text disagrees")
            finite = entry.get("finite", {})
            if (finite.get("passed") is not True
                    or finite.get("functions") != finite_expected
                    or finite.get("type") != "finite-Wasm-execution-only-not-proof"):
                raise ValueError("unsubstantiated producer finite coverage")
            witnesses = ((0, 0), (-(1 << 63), -1), ((1 << 63) - 1, 1))
            count = 0
            for name in ("sat_add", "sat_add_xor", "sat_add_mix", "wrap_add"):
                for x, y in witnesses:
                    got = frontend.invoke(compilers[mode], obj, name, x, y)
                    if got != independent_result(name, x, y):
                        raise ValueError(f"independent native AOT witness failed: {mode}/{profile}/{name}")
                    count += 1
            for x in (-1, 0, 1):
                if frontend.invoke(compilers[mode], obj, "sign_clamp", x) != independent_result("sign_clamp", x):
                    raise ValueError("independent sign clamp mismatch")
                count += 1
            independent_checks[mode] = count
            data[mode] = {
                "sha256": sha(original), "aot_bytes": len(original),
                "text_bytes": actual_text, "dis_sha256": sha(disassembly),
            }
        comparisons = {}
        for mode in PALETTE[1:]:
            a, b = data["original"], data[mode]
            verified = {
                "baseline_minus_candidate_elf_text_bytes": a["text_bytes"] - b["text_bytes"],
                "baseline_minus_candidate_whole_cwasm_bytes": a["aot_bytes"] - b["aot_bytes"],
                "actual_aot_identical": a["sha256"] == b["sha256"],
                "native_disassembly_identical": a["dis_sha256"] == b["dis_sha256"],
                "speed_or_throughput_result": "not-measured",
            }
            if verified != row.get("comparisons", {}).get(mode):
                raise ValueError("reported savings differ from actual compiled objects")
            comparisons[mode] = verified
        for mode in PALETTE:
            actual = data[mode]
            key = sha(json.dumps({
                "profile": profile, "compiler_target": rust.TARGET,
                "wasm_sha256": ws["sha256"],
                "compiled_aot_sha256": actual["sha256"],
            }, sort_keys=True).encode())
            if key not in unique:
                unique.add(key)
                catalog.append({
                    "content_key": key, "profile": profile, "mode": mode,
                    "wasm_sha256": ws["sha256"],
                    "aot_sha256": actual["sha256"], "aot_bytes": actual["aot_bytes"],
                    "elf_text_bytes": actual["text_bytes"],
                    "binary_same_optimization_modes": [
                        candidate for candidate in PALETTE
                        if data[candidate]["sha256"] == actual["sha256"]],
                    "source_receipt_sha256": sha(patches[mode].read_bytes()),
                    "semantic_contract": "signed-i64 Rust saturation and explicit total wrapping derivatives; finite tested",
                    "verification_receipt": "external QF_BV preflight in same workflow required",
                    "cost_evidence": "actual pinned Wasmtime CLI AOT + ELF .text",
                })
        rows.append({"profile": profile, "comparisons": comparisons,
                     "independent_executions_per_mode": independent_checks})
    if catalog != doc.get("deduplicated_catalog"):
        raise ValueError("misleading deduplicated native cost catalog")
    return {
        "schema": AUDIT_SCHEMA,
        "status": "real-native-ELF-AOT-and-separate-finite-witnesses-rechecked",
        "receipt_sha256": sha(raw), "upstream_commit": UPSTREAM,
        "profiles": rows, "deduplicated_entries": len(catalog),
        "limits": [
            "source is experiment-authored Rust, not a production workload",
            "Wasm execution is finite, not whole-machine proof or compile pass proof",
            "QF_BV pure-expression proof is separately checked in the same workflow",
            "no performance speedup or slowdown established by code size",
        ],
    }


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--source-receipt", type=Path, required=True)
    p.add_argument("--upstream", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    for mode in PALETTE:
        p.add_argument("--" + mode + "-bin", required=True, type=Path)
        p.add_argument("--" + mode + "-receipt", required=True, type=Path)
    args = p.parse_args()
    if args.out.exists():
        p.error("refusing to overwrite independent audit")
    result = check(args.source_receipt.resolve(), args.upstream.resolve(),
        {m: getattr(args, m + "_bin").resolve() for m in PALETTE},
        {m: getattr(args, m + "_receipt").resolve() for m in PALETTE})
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"status": result["status"],
                      "profiles": result["profiles"]}, sort_keys=True))


if __name__ == "__main__":
    main()
