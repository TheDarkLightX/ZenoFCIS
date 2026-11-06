#!/usr/bin/env python3
"""Independent ELF/AOT and finite-result recheck of unsound matcher canaries.

Archives cannot be used for a compiler optimization claim: canaries are
intentionally semantically wrong. Recompute every claimed code-size delta and
reexecute with a separately written signed-integer reference.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import subprocess
from pathlib import Path

from install_isle_candidate import UPSTREAM, SOURCE
from install_overflow_match_canary_28 import MODES, BROAD_RULE, EXACT_RULE, sha
from qualify_overflow_match_canary_28 import (
    SCHEMA, WITNESSES, FUNCTIONS, classify,
)
import qualify_rust_idiom_wasm_28 as rust
import qualify_wasmtime_frontend_28 as frontend


def independent_value(fn: str, x: int, y: int) -> int:
    low, high, mod = -(1 << 63), (1 << 63) - 1, (1 << 64)
    total = x + y
    total = min(high, max(low, total))
    if fn == "sat_add":
        return total
    if fn == "sat_add_xor":
        n = (total % mod) ^ 1
    elif fn == "sat_add_mix":
        n = ((total % mod) + ((x % mod) // 128)) % mod
    elif fn == "wrap_add":
        n = (x + y) % mod
    else:
        raise ValueError("unexpected function")
    return n - mod if n >= (1 << 63) else n


def recheck(receipt_path: Path, binaries: dict[str, Path]) -> dict:
    raw = receipt_path.read_bytes()
    report = json.loads(raw)
    if (report.get("schema") != SCHEMA or
        report.get("status") != "INTENTIONALLY-UNSOUND-RULE-REACHABILITY-DIAGNOSTIC" or
        report.get("modes") != list(MODES) or
        report.get("source", {}).get("upstream_sha") != UPSTREAM or
        report.get("exact_canary_sha256") != sha(EXACT_RULE.encode()) or
        report.get("broad_canary_sha256") != sha(BROAD_RULE.encode()) or
        report.get("rust_source_sha256") != sha(rust.SOURCE.read_bytes()) or
        report.get("producer_script_sha256") !=
            sha((Path(__file__).parent / "qualify_overflow_match_canary_28.py").read_bytes()) or
        len(report.get("profiles", [])) != len(rust.PROFILES) or
        set(binaries) != set(MODES)):
        raise ValueError("source, mode or canary identity drift")
    compiler_ids = report.get("source", {}).get("compiler_identities", {})
    if set(compiler_ids) != set(MODES):
        raise ValueError("unrecognized compiler identities")
    for mode in MODES:
        binary = binaries[mode]
        identity = compiler_ids[mode]
        if (not binary.is_file() or
            len(binary.read_bytes()) != identity.get("file_bytes") or
            sha(binary.read_bytes()) != identity.get("sha256") or
            subprocess.check_output([str(binary), "--version"], text=True).strip()
               != identity.get("version")):
            raise ValueError("substituted compiler binary " + mode)
    root = receipt_path.parent
    profiles = []
    for index, (profile, optimization) in enumerate(rust.PROFILES.items()):
        item = report["profiles"][index]
        if item.get("profile") != profile or item.get("rust_optimization") != optimization:
            raise ValueError("missing or reordered Rust optimization profile")
        folder = root / profile
        wasm_data = (folder / "idiom.wasm").read_bytes()
        if (not wasm_data or sha(wasm_data) != item["source_wasm"].get("sha256")
                or len(wasm_data) != item["source_wasm"].get("bytes")):
            raise ValueError("source Wasm substituted")
        if set(item.get("compiled", {})) != set(MODES):
            raise ValueError("missing canary result")
        actual = {}
        replay = {}
        for mode in MODES:
            row = item["compiled"][mode]
            native = folder / mode / "idiom.cwasm"
            content = native.read_bytes()
            if not content or sha(content) != row.get("sha256") or len(content) != row.get("aot_bytes"):
                raise ValueError("wrong original emitted AOT bytes")
            disassembly = frontend.run(
                [str(binaries[mode]), "objdump", "--funcs", "wasm", "--bytes",
                 str(native)], timeout=180).stdout
            if sha(disassembly.encode()) != row.get("disassembly_sha256"):
                raise ValueError("raw native disassembly differs")
            text = frontend.text_size(native)
            if text != row.get("elf_text_bytes"):
                raise ValueError("wrong real ELF section size")
            observations = []
            for fn in FUNCTIONS:
                for x, y in WITNESSES:
                    got = frontend.invoke(binaries[mode], native, fn, x, y)
                    expected = independent_value(fn, x, y)
                    observations.append({
                        "function": fn, "x": x, "y": y,
                        "expected": expected, "observed": got,
                        "mismatch": expected != got,
                    })
            stored = row.get("witness", {})
            if (observations != stored.get("samples") or
                stored.get("sample_count") != len(observations) or
                stored.get("mismatch_count") != sum(x["mismatch"] for x in observations)):
                raise ValueError("finite output evidence does not replay")
            if mode == "baseline" and any(x["mismatch"] for x in observations):
                raise ValueError("baseline is incorrect")
            actual[mode] = {"sha256": sha(content), "aot_bytes": len(content),
                            "elf_text_bytes": text,
                            "disassembly_sha256": sha(disassembly.encode()),
                            "witness": {"mismatch_count": stored["mismatch_count"]}}
            replay[mode] = {"samples": len(observations),
                            "wrong_outputs": stored["mismatch_count"]}
        diagnosis = classify(*(actual[m] for m in MODES))
        if diagnosis != item.get("diagnostic"):
            raise ValueError("fabricated canary reachability decision or byte delta")
        profiles.append({"profile": profile, "diagnostic": diagnosis,
                         "finite_replayed": replay})
    return {
        "schema": "zeno-fcis/tier1-overflow-match-canary-independent/1",
        "status": "real-emitted-objects-and-finite-canary-witnesses-replayed",
        "producer_receipt_sha256": sha(raw), "upstream": UPSTREAM,
        "profiles": profiles,
        "limitations": [
            "Both canary compilers are deliberately unsound and forbidden for production",
            "A changed emitted native program is not a formal exact ISLE match counter",
            "No changed code cannot establish failure to match",
            "This is diagnostic finite testing only, not a new optimization or semantic proof",
        ],
    }


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--source-receipt", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    for mode in MODES:
        p.add_argument("--" + mode.replace("_", "-") + "-bin",
                       dest=mode + "_bin", type=Path, required=True)
    args = p.parse_args()
    if args.out.exists():
        p.error("will not overwrite independent audit receipt")
    audit = recheck(args.source_receipt.resolve(),
                    {m: getattr(args, m + "_bin").resolve() for m in MODES})
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(audit, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"status": audit["status"], "profiles":
                      [(p["profile"], p["diagnostic"]["verdict"])
                       for p in audit["profiles"]]}, sort_keys=True))


if __name__ == "__main__":
    main()
