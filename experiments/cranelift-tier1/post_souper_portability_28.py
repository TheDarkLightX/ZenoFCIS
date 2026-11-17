#!/usr/bin/env python3
"""Requalify the *existing* experimental udiv/urem combiner after upstream Souper merge.

This adapter deliberately leaves the old pinned experiment and its receipts
unchanged. Builds and compiler source modifications happen only in a disposable
GitHub Actions checkout. No Bytecode Alliance patch or PR is generated.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path

import install_udivrem_multivalue_28 as multi
import install_udivrem_combiner_28 as combiner
import qualify_udivrem_contexts_28 as matrix
import audit_udivrem_context_stress_28 as independent

UPSTREAM = "8b19f199f89da864c529b3fa3ca2f7e40d5de609"
PR = "https://github.com/bytecodealliance/wasmtime/pull/15258"
BLOBS = {
    "cranelift/codegen/src/context.rs": "472f3d9db48aad0742e056861ee42243d76e50f4",
    "cranelift/codegen/meta/src/shared/instructions.rs": "1fcac42ebfb4b5a7c9220cd71ddd7688776cd05d",
    "cranelift/codegen/src/isa/x64/lower.isle": "654189182a09c1e22478faa157cd421ff8ed8894",
    "cranelift/interpreter/src/step.rs": "0ed9ade45cd631793d6b660a2121fd54c46ddbb4",
}
CHANGED = set(BLOBS)

def git(root: Path, *args: str) -> str:
    return subprocess.check_output(["git", "-C", str(root), *args], text=True).strip()

def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()

def preflight(root: Path) -> None:
    if git(root, "rev-parse", "HEAD") != UPSTREAM:
        raise ValueError("not the exact upstream post-Souper merge commit")
    if git(root, "status", "--porcelain"):
        raise ValueError("upstream checkout must be pristine")
    for name, digest in BLOBS.items():
        file = root / name
        if not file.is_file() or file.is_symlink() or git(root, "hash-object", name) != digest:
            raise ValueError("upstream source drift: " + name)

def configure() -> None:
    # Scoped adaptation: original installer modules on disk are NEVER modified.
    multi.UPSTREAM = UPSTREAM
    multi.BLOBS = {key: val for key, val in BLOBS.items() if key != combiner.CONTEXT}
    combiner.UPSTREAM = UPSTREAM
    combiner.CONTEXT_BLOB = BLOBS[combiner.CONTEXT]
    matrix.PIN = UPSTREAM
    independent.PIN = UPSTREAM

def install(root: Path, receipt: Path) -> None:
    preflight(root)
    configure()
    combiner.run(root, receipt)
    data = json.loads(receipt.read_text())
    if data["source_commit"] != UPSTREAM or data["original_git_blobs"] != BLOBS:
        raise ValueError("patch receipt source pins disagree")
    if set(git(root, "diff", "--name-only").splitlines()) != CHANGED:
        raise ValueError("patch changed a file outside four permitted compiler sources")
    data["requalification"] = {
        "upstream_souper_merge": PR,
        "classification": "disposable experimental compiler patch; independent A/B still required",
        "previous_pin": "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49",
    }
    receipt.write_text(json.dumps(data, sort_keys=True, indent=2) + "\n")

def rendered_bytes(disassembly: str) -> int:
    rows = [line for line in disassembly.splitlines()
            if re.match(r"^ {12}[0-9a-fA-F]{2}(?:\s|$)", line)]
    if not rows:
        raise ValueError("no decoded native instructions")
    total = 0
    for line in rows:
        match = re.match(r"^ {12}([0-9a-fA-F]{2}(?: [0-9a-fA-F]{2})*)(?: {2,}|$)", line)
        if not match:
            raise ValueError("unparseable native byte row")
        total += len(match.group(1).split())
    return total

def qualify(root: Path) -> None:
    configure()
    if (root / "context-controls").exists():
        raise ValueError("refusing to overwrite context results")
    original = root / "baseline" / "wasmtime"
    candidate = root / "patched" / "wasmtime"
    matrix.qualify(original, candidate, root / "context-controls")

def audit(root: Path, upstream: Path) -> None:
    configure()
    if git(upstream, "rev-parse", "HEAD") != UPSTREAM:
        raise ValueError("source commit differs from qualification pin")
    if set(git(upstream, "diff", "--name-only").splitlines()) != CHANGED:
        raise ValueError("patched sources differ from expected four paths")
    receipt = json.loads((root / "install.json").read_text())
    if receipt.get("original_git_blobs") != BLOBS or receipt.get("source_commit") != UPSTREAM:
        raise ValueError("installer did not preserve source provenance")
    if (root / "post_souper_cost_audit.json").exists():
        raise ValueError("refusing to overwrite measured audit")
    independent_result = independent.audit(root)  # replays selected native cases
    measured = json.loads((root / "context-controls" / "qualification.json").read_text())
    if measured["upstream_source_pin"] != UPSTREAM or set(measured["cases"]) != set(matrix.CASES):
        raise ValueError("wrong source pin or incomplete context matrix")
    cases = {}
    for name, row in sorted(measured["cases"].items()):
        folder = root / "context-controls" / name
        outcomes = {}
        for mode in ("baseline", "patched"):
            item = row["results"][mode]
            disfile = folder / (mode + ".disassembly.txt")
            aot = folder / (mode + ".cwasm")
            wat = folder / "input.wat"
            if (sha(disfile) != item["disassembly_sha256"]
                    or sha(aot) != item["aot_sha256"]
                    or sha(wat) != row["wat_sha256"]):
                raise ValueError("archive content mismatch for " + name + "/" + mode)
            outcomes[mode] = {
                "decoded_native_instruction_bytes": rendered_bytes(disfile.read_text()),
                "static_div_instructions": item["static_cost"]["static_divisions"],
                "decoded_instructions": item["static_cost"]["decoded_instructions"],
                "whole_aot_bytes": item["aot_bytes"],
            }
        b, p = outcomes["baseline"], outcomes["patched"]
        cases[name] = {
            "baseline": b, "experimental": p,
            "baseline_minus_experimental_decoded_bytes":
                b["decoded_native_instruction_bytes"] - p["decoded_native_instruction_bytes"],
            "baseline_minus_experimental_divs":
                b["static_div_instructions"] - p["static_div_instructions"],
            "baseline_minus_experimental_aot_bytes":
                b["whole_aot_bytes"] - p["whole_aot_bytes"],
        }
        if cases[name]["baseline_minus_experimental_divs"] != row["static_div_savings"]:
            raise ValueError("native instruction count disagrees with producer")
    head = git(Path(__file__).resolve().parents[2], "rev-parse", "HEAD")
    result = {
        "schema": "zenofcis/tier1-post-souper-merged-upstream-ab/1",
        "classification": "same-WAT finite native correctness and static-cost experiment",
        "zenofcis_source_commit": head,
        "upstream_source_commit": UPSTREAM,
        "upstream_souper_pr": PR,
        "pinned_source_blobs": BLOBS,
        "installer_receipt_sha256": sha(root / "install.json"),
        "matrix_receipt_sha256": sha(root / "context-controls" / "qualification.json"),
        "independent_stress_receipt_sha256":
            sha(root / "context-controls" / "independent-stress-audit.json"),
        "contexts": cases,
        "total_contexts": len(cases),
        "finite_wasm_calls_in_matrix":
            len(cases) * len(matrix.vectors()) * 2,
        "independently_replayed_calls": independent_result["independent_novel_case_replays"],
        "limits": [
            "Post-merge upstream source is pinned; this is not a proof of generic compiler-pass semantics",
            "Unsigned 64-bit only for matched adjacent pair; signed and 32-bit controls must not fuse",
            "No assumption that the optimization is profitable in all contexts",
            "Decoded native instruction bytes are per-function, not CLIF cost or AOT/throughput gains",
            "No ISA-wide flags, register-allocation, poison, or all-target proof is claimed",
            "Upstream Souper PR is merged; this local prototype is not an upstream contribution",
        ],
    }
    (root / "post_souper_cost_audit.json").write_text(json.dumps(result, sort_keys=True, indent=2) + "\n")
    print(json.dumps({
        "status": "post-merge same-WAT A/B independently audited",
        "cases": len(cases), "finite_calls": result["finite_wasm_calls_in_matrix"],
        "independent_calls": result["independently_replayed_calls"],
        "native_byte_deltas": {n: row["baseline_minus_experimental_decoded_bytes"]
                               for n, row in cases.items()},
    }, sort_keys=True))

def self_test() -> None:
    configure()
    assert combiner.self_test()
    independent.self_test()
    assert set(BLOBS) == {
        combiner.CONTEXT, multi.META, multi.X64, multi.INTERP,
    }
    assert all(len(v) == 40 for v in BLOBS.values())
    assert rendered_bytes("            48 89 c8  movq %rcx, %rax\n            c3  retq\n") == 4
    for corrupted in ["", "            48 gg c8  invalid\n"]:
        try:
            rendered_bytes(corrupted)
        except ValueError:
            continue
        raise AssertionError("malformed native byte input accepted")
    print("post-Souper pin adaptation, fixture model and bad-decoder controls passed")

def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--mode", choices=("self-test", "preflight", "install", "qualify", "audit"),
                   required=True)
    p.add_argument("--upstream", type=Path)
    p.add_argument("--out", type=Path)
    args = p.parse_args()
    if args.mode == "self-test":
        self_test()
        return
    if args.mode in ("preflight", "install", "audit") and args.upstream is None:
        p.error("--upstream required")
    if args.mode in ("install", "qualify", "audit") and args.out is None:
        p.error("--out required")
    if args.mode == "preflight":
        preflight(args.upstream)
    elif args.mode == "install":
        install(args.upstream, args.out / "install.json")
    elif args.mode == "qualify":
        qualify(args.out)
    else:
        audit(args.out, args.upstream)

if __name__ == "__main__":
    main()
