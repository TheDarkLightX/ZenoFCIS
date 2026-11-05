#!/usr/bin/env python3
"""Independent audit of real, unmodified upstream WAT AOT fixture corpus.

This verifier does not trust the producer's reported lengths, source hashes,
native disassembly hashes, binary hashes, finite tests, or deltas. It does
rely on the manifest of exact upstream Git blob IDs and a correctly pinned
upstream checkout; neither is evidence of external workload representativeness.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path

from install_isle_candidate import RULE, UPSTREAM
import qualify_wasmtime_frontend_28 as frontend
from qualify_external_wat_corpus_28 import (
    MANIFEST, SCHEMA, SMOKES, TARGET, git, sha, validate_fixture,
)


def oracle_bswap(value: int, width: int) -> int:
    original = value % (1 << width)
    swapped = int.from_bytes(
        original.to_bytes(width // 8, "little"), "big", signed=False)
    return swapped - (1 << width) if swapped & (1 << (width - 1)) else swapped


def independent_smoke(bin_path: Path, obj: Path, path: str) -> list[dict]:
    cases = {
        "tests/disas/byteswap.wat": [
            ("bswap32", 0x01020304, oracle_bswap(0x01020304, 32)),
            ("bswap32", -1, -1),
            ("bswap64", 0x0102030405060708,
             oracle_bswap(0x0102030405060708, 64)),
            ("bswap64", -1, -1),
        ],
        "tests/all/cli_tests/simple.wat": [
            ("simple", -1234567, -1234567),
            ("simple", 0, 0),
        ],
    }
    rows = []
    for name, arg, expected in cases.get(path, []):
        value = frontend.invoke(bin_path, obj, name, arg)
        if value != expected:
            raise ValueError(f"independent native run mismatch {path}:{name}({arg})")
        rows.append({"function": name, "arg": arg,
                     "expected": expected, "observed": value})
    return rows


def validate(upstream: Path, compilers: dict[str, Path], receipt: Path) -> dict:
    raw_receipt = receipt.read_bytes()
    report = json.loads(raw_receipt)
    source = Path(__file__).parent / "qualify_external_wat_corpus_28.py"
    if (report.get("schema") != SCHEMA
            or report.get("status") != "external-unmodified-upstream-compiler-fixture-aot-screen"
            or report.get("upstream_commit") != UPSTREAM
            or report.get("target") != TARGET
            or report.get("rule_sha256") != sha(RULE.encode())
            or report.get("producer_script_sha256") != sha(source.read_bytes())
            or report.get("source", {}).get("upstream_commit") != UPSTREAM
            or report.get("manifest") != [
                {"path": p, "git_blob": b} for p, b in MANIFEST]
            or len(report.get("cases", [])) != len(MANIFEST)
            or set(compilers) != {"baseline", "patched"}
            or git(upstream, "rev-parse", "HEAD") != UPSTREAM):
        raise ValueError("incomplete or forged source/receipt identity")
    versions = report.get("source", {}).get("compiler_binaries", {})
    if set(versions) != {"baseline", "patched"}:
        raise ValueError("compiler binary identity missing")
    for name, path in compilers.items():
        expected = versions[name]
        if (not path.is_file()
                or path.stat().st_size != expected.get("bytes")
                or sha(path.read_bytes()) != expected.get("sha256")
                or frontend.run([str(path), "--version"]).stdout.strip() != expected.get("version")):
            raise ValueError("source compiler binary changed or missing")

    audits = []
    for i, (path, original_blob) in enumerate(MANIFEST):
        row = report["cases"][i]
        original = validate_fixture(upstream, path, original_blob)
        case = receipt.parent / f"case-{i:02d}"
        copied = (case / "source.wat").read_bytes()
        if (copied != original or row.get("path") != path
                or row.get("upstream_git_blob") != original_blob
                or row.get("source_sha256") != sha(original)
                or row.get("bytes") != len(original)
                or set(row.get("results", {})) != {"baseline", "patched"}):
            raise ValueError(f"external source drift or omitted fixture: {path}")
        variants = {}
        independent_executions = {}
        for name in ("baseline", "patched"):
            saved = row["results"][name]
            directory = case / name
            object_file = directory / "module.cwasm"
            data = object_file.read_bytes()
            disassembly = (directory / "disassembly.txt").read_bytes()
            if (not data or sha(data) != saved.get("object_sha256")
                    or len(data) != saved.get("object_bytes")
                    or sha(disassembly) != saved.get("disassembly_sha256")):
                raise ValueError("incorrect native AOT or disassembly archive")
            text_bytes = frontend.text_size(object_file)
            if text_bytes != saved.get("elf_text_bytes"):
                raise ValueError("ELF .text length mismatch")
            finite = saved.get("finite_smoke", {})
            planned = SMOKES.get(path, ())
            receipts = finite.get("checks", [])
            if (len(receipts) != len(planned)
                    or finite.get("status") != (
                        "finite-execution-passed" if planned else "compile-only")):
                raise ValueError("missing or incorrectly labeled finite test")
            for measured, (func, arg, expected) in zip(receipts, planned):
                if measured != {
                        "function": func, "arg": arg,
                        "expected": expected, "actual": expected}:
                    raise ValueError("false or inconsistent archived finite result")
            variants[name] = {
                "actual_aot_bytes": len(data),
                "actual_text_bytes": text_bytes,
                "actual_aot_sha256": sha(data),
                "actual_disassembly_sha256": sha(disassembly),
            }
            independent_executions[name] = independent_smoke(
                compilers[name], object_file, path)
        a, b = variants["baseline"], variants["patched"]
        delta = {
            "actual_elf_text_bytes":
                a["actual_text_bytes"] - b["actual_text_bytes"],
            "whole_aot_bytes":
                a["actual_aot_bytes"] - b["actual_aot_bytes"],
        }
        if (delta != row.get("baseline_minus_patched")
                or row.get("complete_aot_identical") is not (
                    a["actual_aot_sha256"] == b["actual_aot_sha256"])
                or row.get("native_disassembly_identical") is not (
                    a["actual_disassembly_sha256"]
                    == b["actual_disassembly_sha256"])):
            raise ValueError("claimed cost/code delta contradicted by native artifacts")
        audits.append({
            "path": path, "upstream_git_blob": original_blob,
            "actual_deltas": delta, "actual_binary_identical":
                a["actual_aot_sha256"] == b["actual_aot_sha256"],
            "actual_disassembly_identical":
                a["actual_disassembly_sha256"] == b["actual_disassembly_sha256"],
            "variants": variants,
            "independent_finite_executions": independent_executions,
        })
    return {
        "schema": "zeno-fcis/tier1-external-upstream-wat-independent/1",
        "status": "rechecked-external-fixture-actual-ELF-and-AOT-cost",
        "upstream_commit": UPSTREAM,
        "source_receipt_sha256": sha(raw_receipt),
        "audited_count": len(audits),
        "changed_native_disassembly_count": sum(
            not x["actual_disassembly_identical"] for x in audits),
        "actual_elf_text_total_baseline_minus_patched": sum(
            x["actual_deltas"]["actual_elf_text_bytes"] for x in audits),
        "actual_aot_total_baseline_minus_patched": sum(
            x["actual_deltas"]["whole_aot_bytes"] for x in audits),
        "cases": audits,
        "nonclaims": [
            "upstream regression fixtures are not production application samples",
            "actual unmodified upstream modules; no generated saturation targets",
            "the total size delta sums independent AOT files, not one linked application",
            "compile-only fixtures have no execution equivalence evidence",
            "independent finite smokes do not prove all outputs or memory/trap semantics",
            "a zero delta cannot prove rule correctness or lack of effects elsewhere",
            "VeriISLE verification of the named rule still depends on a provisional select spec",
            "no runtime speed or target-wide regression claim",
        ],
    }


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--upstream", type=Path, required=True)
    p.add_argument("--baseline-bin", type=Path, required=True)
    p.add_argument("--patched-bin", type=Path, required=True)
    p.add_argument("--source-receipt", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    args = p.parse_args()
    if args.out.exists():
        p.error("refusing to overwrite independent receipt")
    result = validate(
        args.upstream.resolve(),
        {"baseline": args.baseline_bin.resolve(),
         "patched": args.patched_bin.resolve()},
        args.source_receipt.resolve())
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, sort_keys=True, indent=2) + "\n")
    print(json.dumps({
        "status": result["status"], "count": result["audited_count"],
        "changed_disassembly": result["changed_native_disassembly_count"],
        "sum_aot_delta": result["actual_aot_total_baseline_minus_patched"]},
        sort_keys=True))


if __name__ == "__main__":
    main()
