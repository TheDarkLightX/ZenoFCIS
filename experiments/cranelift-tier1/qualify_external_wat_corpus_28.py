#!/usr/bin/env python3
"""External, unchanged Wasmtime test-fixture AOT regression screen.

This is NOT an application benchmark. The source files are wholly external
to ZenoFCIS and taken unchanged from the pinned upstream Git tree.
Compare exactly the same WAT in unmodified vs one-rule-patched Wasmtime,
not raw machine instruction bytes against CLIF or a guessed cost model.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import platform
import subprocess
from pathlib import Path

from install_isle_candidate import UPSTREAM, RULE
import qualify_wasmtime_frontend_28 as frontend
import qualify_wasmtime_scaling_28 as scaling

SCHEMA = "zeno-fcis/tier1-external-upstream-wat-screen/1"
TARGET = "x86_64-unknown-linux"
# Independently inspected via pinned upstream GitHub contents API; no
# generated source, no modified fixture, and no dependency on latest main.
MANIFEST = (
    ("tests/disas/arith.wat", "8b8c5dd1c610f24ff24408e759350894469a60e7"),
    ("tests/disas/byteswap.wat", "307d4d75cb88a5d24b258d50dea4f0fbd660f184"),
    ("tests/disas/fibonacci.wat", "bce01a34e7a431549e44b8165b7ad40ff188ca84"),
    ("tests/disas/br_table.wat", "7e90405cf5fe67a0073376aa26258c2f974b49d4"),
    ("tests/disas/call.wat", "33a9335e61630b726f5cd8fe79b3ea5250faa960"),
    ("tests/disas/conditional-traps.wat", "441345a049c7d85d724c3bf8b1eb8622505e5836"),
    ("tests/disas/ctz-clz-bool-condition.wat", "67caeff226a1ce2f7c313472070004e83f55264c"),
    ("tests/disas/dead-code.wat", "1ddc3cfa6240664cd86204f968ba7f815337c71f"),
    ("tests/disas/globals.wat", "40cc8e6974993de399428ba2bec17d00b1733760"),
    ("tests/disas/i64-load.wat", "a3466302512bf781b701892e2c130681d2b652d0"),
    ("tests/all/cli_tests/fib.wat", "3e9b9f0e5fba05a723ae7918b7a1133ff659b8ad"),
    ("tests/all/cli_tests/simple.wat", "a62319c5e179f188e661e539507fea5698ec2fba"),
)
SMOKES = {
    "tests/disas/byteswap.wat": (
        ("bswap32", 305419896, 2018915346),
        ("bswap32", 0, 0),
        ("bswap64", 72623859790382856, 578437695752307201),
    ),
    "tests/all/cli_tests/simple.wat": (
        ("simple", -7, -7),
        ("simple", 43, 43),
    ),
}


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def git(upstream: Path, *args: str) -> str:
    return subprocess.check_output(
        ["git", "-C", str(upstream), *args], text=True).strip()


def validate_fixture(upstream: Path, path: str, blob: str) -> bytes:
    if not path.startswith(("tests/disas/", "tests/all/cli_tests/")):
        raise ValueError("unadmitted fixture path")
    if ".." in Path(path).parts or not path.endswith(".wat"):
        raise ValueError("unexpected fixture path")
    if git(upstream, "rev-parse", f"HEAD:{path}") != blob:
        raise ValueError("external source git blob changed: " + path)
    data = (upstream / path).read_bytes()
    if git(upstream, "hash-object", path) != blob:
        raise ValueError("source worktree differs from pinned blob: " + path)
    if not data or b"(module" not in data:
        raise ValueError("not an admitted standalone WAT module: " + path)
    return data


def compile_one(bin_path: Path, wat: Path, dest: Path) -> dict:
    object_file = dest / "module.cwasm"
    command = [str(bin_path), "compile", "--target", TARGET,
               "-o", str(object_file), str(wat)]
    output = frontend.run(command, timeout=300)
    if not object_file.is_file() or not object_file.stat().st_size:
        raise ValueError("no native AOT output for " + str(wat))
    (dest / "compile.stdout").write_text(output.stdout)
    (dest / "compile.stderr").write_text(output.stderr)
    disassembly = frontend.run(
        [str(bin_path), "objdump", "--funcs", "wasm", "--bytes",
         str(object_file)], timeout=180).stdout
    if not disassembly.strip():
        raise ValueError("empty native code disassembly")
    (dest / "disassembly.txt").write_text(disassembly)
    raw = object_file.read_bytes()
    return {
        "object_sha256": sha(raw), "object_bytes": len(raw),
        "elf_text_bytes": frontend.text_size(object_file),
        "disassembly_sha256": sha(disassembly.encode()),
        "compile_argv": command,
    }


def smoke(bin_path: Path, obj: Path, path: str) -> dict:
    checks = []
    for func, arg, expected in SMOKES.get(path, ()):
        got = frontend.invoke(bin_path, obj, func, arg)
        if got != expected:
            raise ValueError(f"external Wasm smoke failed: {path} {func}({arg})")
        checks.append({"function": func, "arg": arg,
                       "expected": expected, "actual": got})
    return {
        "status": "finite-execution-passed" if checks else "compile-only",
        "checks": checks,
        "important": "tests check selected exports; not all fixture semantics",
    }


def qualify(upstream: Path, bins: dict, dest: Path) -> dict:
    if dest.exists():
        raise ValueError("refusing to overwrite evidence")
    if platform.machine() not in ("x86_64", "AMD64"):
        raise ValueError("only x86-64 native AOT screening admitted")
    if set(bins) != {"baseline", "patched"}:
        raise ValueError("missing or surplus compiler variant")
    if git(upstream, "rev-parse", "HEAD") != UPSTREAM:
        raise ValueError("unexpected upstream revision")
    pins = frontend.source_pins(upstream, bins)
    dest.mkdir(parents=True)
    results = []
    seen = set()
    for i, (path, blob) in enumerate(MANIFEST):
        if path in seen:
            raise ValueError("duplicate manifest entry")
        seen.add(path)
        directory = dest / f"case-{i:02d}"
        directory.mkdir()
        data = validate_fixture(upstream, path, blob)
        source_file = directory / "source.wat"
        source_file.write_bytes(data)
        measured = {}
        for label in ("baseline", "patched"):
            target = directory / label
            target.mkdir()
            measured[label] = compile_one(bins[label], source_file, target)
            measured[label]["finite_smoke"] = smoke(
                bins[label], target / "module.cwasm", path)
        base, candidate = measured["baseline"], measured["patched"]
        results.append({
            "path": path, "upstream_git_blob": blob,
            "source_sha256": sha(data),
            "bytes": len(data), "results": measured,
            "baseline_minus_patched": {
                "actual_elf_text_bytes":
                    base["elf_text_bytes"] - candidate["elf_text_bytes"],
                "whole_aot_bytes":
                    base["object_bytes"] - candidate["object_bytes"],
            },
            "complete_aot_identical":
                base["object_sha256"] == candidate["object_sha256"],
            "native_disassembly_identical":
                base["disassembly_sha256"] == candidate["disassembly_sha256"],
        })
    return {
        "schema": SCHEMA,
        "status": "external-unmodified-upstream-compiler-fixture-aot-screen",
        "upstream_commit": UPSTREAM, "target": TARGET, "source": pins,
        "producer_script_sha256": sha(Path(__file__).read_bytes()),
        "rule_sha256": sha(RULE.encode()),
        "manifest": [{"path": p, "git_blob": h} for p, h in MANIFEST],
        "cases": results,
        "scope": "pinned upstream Wasmtime compiler test modules, not application benchmarks",
        "nonclaims": [
            "no source module was modified or generated by ZenoFCIS",
            "a fixture is not a representative production application",
            "unmodified vs patched same-WAT actual AOT comparison only",
            "compile-only fixtures are not executed semantic checks",
            "finite smoke tests for selected exports are not whole-module proof",
            "unmodified/patched binaries may differ due to alignment or metadata",
            "a no-change case means this rewrite did not affect that exact test, not global correctness",
            "existing VeriISLE named-rule proof remains conditional on local select spec",
            "no runtime performance, flags, ABI, all-target or power claim",
        ],
    }


def main() -> None:
    p = argparse.ArgumentParser()
    for arg in ("upstream", "baseline-bin", "patched-bin", "out"):
        p.add_argument("--" + arg, type=Path, required=True)
    a = p.parse_args()
    result = qualify(a.upstream.resolve(), {
        "baseline": a.baseline_bin.resolve(),
        "patched": a.patched_bin.resolve()}, a.out.resolve())
    (a.out / "qualification.json").write_text(
        json.dumps(result, sort_keys=True, indent=2) + "\n")
    print(json.dumps({
        "status": result["status"],
        "cases": [{"path": r["path"],
                   "deltas": r["baseline_minus_patched"],
                   "binary_identical": r["complete_aot_identical"]}
                  for r in result["cases"]]}, sort_keys=True))


if __name__ == "__main__":
    main()
