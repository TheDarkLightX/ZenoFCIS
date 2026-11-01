"""Measure two separate pinned-Cranelift CLIF compilations as a qualified A/B.

The result distinguishes emitted *whole-function* bytes from fixed-register
x86 Zeno kernel bytes. It records an observation, not a verified ISLE patch.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path


def capture(*args: str) -> str:
    return subprocess.run(args, capture_output=True, text=True, check=True).stdout


def profile(path: Path) -> dict:
    contents = path.read_bytes()
    header = capture("readelf", "-h", str(path))
    if "ELF64" not in header or "X86-64" not in header:
        raise ValueError(f"refusing non-x86_64 ELF: {path}")
    symbols = []
    for line in capture("nm", "-S", "--defined-only", "--format=posix", str(path)).splitlines():
        fields = line.split()
        if len(fields) == 4 and fields[1] in ("T", "t"):
            try:
                symbols.append({"name": fields[0], "offset": int(fields[2], 16),
                                "bytes": int(fields[3], 16)})
            except ValueError as error:
                raise ValueError("invalid symbol size format") from error
    if len(symbols) != 1 or symbols[0]["bytes"] == 0:
        raise ValueError(f"expected exactly one sized text symbol, found: {symbols}")
    disassembly = capture("objdump", "-d", "-Mintel", str(path))
    instruction_lines = re.findall(r"^\s*[0-9a-f]+:\s+(?:[0-9a-f]{2}\s+)+\s+\S+", disassembly, re.M)
    if not instruction_lines:
        raise ValueError("no decoded x86-64 instructions observed")
    return {
        "object_sha256": hashlib.sha256(contents).hexdigest(),
        "file_bytes": len(contents),
        "function": symbols[0],
        "decoded_disassembly_lines": len(instruction_lines),
        "object_path": str(path),
        "metric": "ELF text symbol size including ABI/prologue/epilogue (not raw Zeno body)",
        "disassembly": disassembly,
    }


def run(baseline: Path, candidate: Path, receipt: Path, out: Path) -> dict:
    if out.exists():
        raise ValueError("fresh output required")
    if baseline.resolve() == candidate.resolve():
        raise ValueError("A/B must be distinct object files")
    proof = json.loads(receipt.read_text())
    if proof.get("status") != "unsat-counterexample-query":
        raise ValueError("independent CLIF expression proof not successful")
    verified_hashes = proof["source_files_sha256"]
    here = Path(__file__).resolve().parent
    for name in ("satadd.clif", "satadd_shift.clif"):
        if verified_hashes[name] != hashlib.sha256((here / name).read_bytes()).hexdigest():
            raise ValueError(f"CLIF fixture {name} has changed since proof")
    old = profile(baseline)
    new = profile(candidate)
    delta = old["function"]["bytes"] - new["function"]["bytes"]
    result = {
        "schema": "zeno-fcis/cranelift-tier1-clif-measurement/1",
        "status": "measured-a-b-only",
        "comparison": "same pinned upstream, target and optimization settings, separate CLIF functions",
        "source_pins": proof["source_pins"],
        "clif_proof_receipt_sha256": hashlib.sha256(receipt.read_bytes()).hexdigest(),
        "baseline": {key: value for key, value in old.items() if key != "disassembly"},
        "candidate": {key: value for key, value in new.items() if key != "disassembly"},
        "native_function_byte_difference_old_minus_new": delta,
        "candidate_has_smaller_emitted_function": delta > 0,
        "nonclaims": [
            "This is an IR-level A/B, not an upstream lowering patch",
            "Native instruction byte counts differ from abstract CLIF or Zeno kernel costs",
            "No clock-based throughput or general-target performance conclusion",
            "Independently proven pure result semantics do not prove compiler or ABI correctness",
        ],
    }
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(result, sort_keys=True, indent=2) + "\n")
    (out.parent / "baseline-disassembly.txt").write_text(old["disassembly"])
    (out.parent / "candidate-disassembly.txt").write_text(new["disassembly"])
    return result


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("--baseline", type=Path, required=True)
    p.add_argument("--candidate", type=Path, required=True)
    p.add_argument("--proof-receipt", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    args = p.parse_args()
    try:
        result = run(args.baseline, args.candidate, args.proof_receipt, args.out)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"A/B measurement refused: {error}") from error
    print(json.dumps({"status": result["status"],
                      "old_whole_function_bytes": result["baseline"]["function"]["bytes"],
                      "new_whole_function_bytes": result["candidate"]["function"]["bytes"],
                      "difference_bytes": result["native_function_byte_difference_old_minus_new"],
                      "upstream_patch": False}, sort_keys=True))


if __name__ == "__main__":
    main()
