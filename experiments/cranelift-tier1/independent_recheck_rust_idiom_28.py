#!/usr/bin/env python3
"""Read-only audit of the exact Rust-generated Wasm and Wasmtime AOT bytes.

Second independent arithmetic oracle and additional on-host execution; not a
proof of all observable state. Recomputes each ELF/AOT cost from actual object.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path

from install_isle_candidate import UPSTREAM, RULE
import qualify_wasmtime_frontend_28 as frontend
from qualify_rust_idiom_wasm_28 import (
    PROFILES, SCHEMA, SOURCE, TARGET, RUST_TARGET, VECTORS, digest,
)


def i64(value: int) -> int:
    return ((value + (1 << 63)) % (1 << 64)) - (1 << 63)


def independent_result(name: str, x: int, y: int | None = None) -> int:
    lo, hi = -(1 << 63), (1 << 63) - 1
    if not lo <= x <= hi:
        raise ValueError("x not signed i64")
    if name == "sign_clamp":
        return lo if x < 0 else hi
    if y is None or not lo <= y <= hi:
        raise ValueError("y not signed i64")
    raw = x + y
    saturation = lo if raw < lo else hi if raw > hi else raw
    if name == "sat_add":
        return saturation
    if name == "sat_add_xor":
        return i64(saturation ^ 1)
    if name == "sat_add_mix":
        return i64(saturation + ((x % (1 << 64)) // 128))
    if name == "wrap_add":
        return i64(raw)
    raise ValueError("unadmitted export")


def replay(upstream: Path, bins: dict, receipt: Path) -> dict:
    raw = receipt.read_bytes()
    report = json.loads(raw)
    script = Path(__file__).parent / "qualify_rust_idiom_wasm_28.py"
    if (report.get("schema") != SCHEMA
            or report.get("status") != "idiomatic-Rust-to-Wasm-compile-and-finite-check"
            or report.get("upstream_commit") != UPSTREAM
            or report.get("compiler_target") != TARGET
            or report.get("rust_target") != RUST_TARGET
            or report.get("rule_sha256") != digest(RULE.encode())
            or report.get("producer_script_sha256") != digest(script.read_bytes())
            or report.get("rust_source_sha256") != digest(SOURCE.read_bytes())
            or report.get("rust_source_bytes") != len(SOURCE.read_bytes())
            or not report.get("rustc_version", "").startswith("rustc 1.97.1 ")
            or len(report.get("cases", [])) != len(PROFILES)
            or set(bins) != {"baseline", "patched"}):
        raise ValueError("bad Rust source/proof/cost envelope")
    pins = frontend.source_pins(upstream, bins)
    if pins != report.get("wasmtime_source_pins"):
        raise ValueError("source or compiler binary pin mismatch")

    result = []
    for i, (profile, opt) in enumerate(PROFILES.items()):
        row = report["cases"][i]
        if (row.get("profile") != profile or row.get("opt_level") != opt
                or set(row.get("results", {})) != {"baseline", "patched"}):
            raise ValueError("missing, shuffled, or mislabeled opt profile")
        directory = receipt.parent / profile
        wasm = (directory / "idiom.wasm").read_bytes()
        rust = row.get("rust_wasm", {})
        if (not wasm or digest(wasm) != rust.get("sha256")
                or len(wasm) != rust.get("bytes")
                or rust.get("argv", [None])[0] != "rustc"):
            raise ValueError("Wasm producer artifact changed or missing")
        measured = {}
        independent_checks = {}
        for name in ("baseline", "patched"):
            entry = row["results"][name]
            folder = directory / name
            obj = folder / "idiom.cwasm"
            data = obj.read_bytes()
            disassembly = (folder / "disassembly.txt").read_bytes()
            actual_text = frontend.text_size(obj)
            if (not data or digest(data) != entry.get("sha256")
                    or len(data) != entry.get("aot_bytes")
                    or actual_text != entry.get("elf_text_bytes")
                    or digest(disassembly) != entry.get("disassembly_sha256")):
                raise ValueError(f"changed or fabricated actual AOT bytes in {profile}/{name}")
            source_check = entry.get("finite", {})
            counts = source_check.get("functions", {})
            expected_counts = {
                "sat_add": len(VECTORS),
                "sat_add_xor": len(VECTORS),
                "sat_add_mix": len(VECTORS),
                "wrap_add": len(VECTORS),
                "sign_clamp": len(set(x for x, _ in VECTORS)),
            }
            if (source_check.get("passed") is not True
                    or counts != expected_counts
                    or source_check.get("type") != "finite-Wasm-execution-only-not-proof"):
                raise ValueError("missing or forged source finite check counts")
            # Separate independent oracle and additional finite witness inputs.
            witnesses = ((0, 0), (-(1 << 63), -1), ((1 << 63) - 1, 1))
            observed = 0
            for function in ("sat_add", "sat_add_xor", "sat_add_mix", "wrap_add"):
                for x, y in witnesses:
                    actual = frontend.invoke(bins[name], obj, function, x, y)
                    if actual != independent_result(function, x, y):
                        raise ValueError(f"independent runtime mismatch: {profile}/{name}/{function}")
                    observed += 1
            for x in (-1, 0, 1):
                actual = frontend.invoke(bins[name], obj, "sign_clamp", x)
                if actual != independent_result("sign_clamp", x):
                    raise ValueError("independent sign clamp runtime mismatch")
                observed += 1
            independent_checks[name] = observed
            measured[name] = {
                "full_aot_sha256": digest(data),
                "native_disassembly_sha256": digest(disassembly),
                "complete_aot_bytes": len(data), "elf_text_bytes": actual_text,
            }
        a, b = measured["baseline"], measured["patched"]
        delta = {
            "elf_text_bytes": a["elf_text_bytes"] - b["elf_text_bytes"],
            "whole_cwasm_bytes": a["complete_aot_bytes"] - b["complete_aot_bytes"],
        }
        if (delta != row.get("baseline_minus_patched")
                or row.get("binary_identical") is not (
                    a["full_aot_sha256"] == b["full_aot_sha256"])
                or row.get("disassembly_identical") is not (
                    a["native_disassembly_sha256"]
                    == b["native_disassembly_sha256"])):
            raise ValueError("producer's reported delta contradicts actual objects")
        result.append({
            "profile": profile, "rust_wasm_sha256": digest(wasm),
            "recomputed_deltas": delta,
            "actual_binary_identical":
                a["full_aot_sha256"] == b["full_aot_sha256"],
            "actual_disassembly_identical":
                a["native_disassembly_sha256"]
                == b["native_disassembly_sha256"],
            "independent_executions_per_compiler": independent_checks,
            "native_objects": measured,
        })
    return {
        "schema": "zeno-fcis/tier1-rust-idiom-wasm-independent-replay/1",
        "status": "independent-Wasm-and-native-AOT-correctness-and-cost-check",
        "source_receipt_sha256": digest(raw), "upstream_commit": UPSTREAM,
        "cases": result,
        "limits": [
            "Rust idiom is experiment-authored, not an external application",
            "Rust LLVM output is measured, not assumed equivalent to handwritten WAT shape",
            "finite native output checks do not prove arbitrary state/flags/poison behavior",
            "named VeriISLE still conditional on unreviewed scalar select spec",
            "no CPU runtime throughput claim",
        ],
    }


def main() -> None:
    p = argparse.ArgumentParser()
    for x in ("upstream", "baseline-bin", "patched-bin",
              "source-receipt", "out"):
        p.add_argument("--" + x, type=Path, required=True)
    a = p.parse_args()
    if a.out.exists():
        p.error("will not overwrite audit")
    result = replay(a.upstream.resolve(), {
        "baseline": a.baseline_bin.resolve(),
        "patched": a.patched_bin.resolve()},
        a.source_receipt.resolve())
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps({
        "status": result["status"],
        "cases": [{
            "profile": r["profile"], "actual_deltas": r["recomputed_deltas"],
            "binary_identical": r["actual_binary_identical"],
        } for r in result["cases"]]}, sort_keys=True))


if __name__ == "__main__":
    main()
