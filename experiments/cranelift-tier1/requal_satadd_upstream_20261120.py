#!/usr/bin/env python3
"""Requalify the existing human-authored signed-i64 saturation-clamp ISLE rule
against a newer, exact Wasmtime source pin. This is NOT a Zeno synthesis claim.

All compiler edits occur only in the disposable CI checkout. Outputs distinguish
full ELF symbol size, decoded executable prefix through the first RET, native
finite testing and conditional QF_BV proof. No throughput or full-state claims.
"""
from __future__ import annotations

import argparse
import ctypes
import hashlib
import json
import platform
import random
import re
import subprocess
import tempfile
from pathlib import Path

from install_isle_candidate import RULE, RULE_NAME, SOURCE
from measure_ab import profile

UPSTREAM = "fb792bf7a143801c22c0ba5b16fe2c6085a8598b"
SOURCE_BLOB = "5ec428ceefedfc89c49392875a3fad7299a5f884"
CLIF = Path(__file__).with_name("satadd.clif")
SYMBOL = "%tier1_signed_satadd"
SMIN, SMAX = -(1 << 63), (1 << 63) - 1
EDGE = [SMIN, SMIN + 1, SMIN + 2, -(1 << 62), -65536, -256, -2, -1,
        0, 1, 2, 256, 65536, (1 << 62), SMAX - 2, SMAX - 1, SMAX]
SEED = 20261120


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def git(root: Path, *args: str) -> str:
    return subprocess.check_output(["git", "-C", str(root), *args], text=True).strip()


def ensure_pin(root: Path) -> Path:
    if git(root, "rev-parse", "HEAD") != UPSTREAM:
        raise ValueError("not the reviewed upstream revision")
    if git(root, "rev-parse", "HEAD:" + SOURCE) != SOURCE_BLOB:
        raise ValueError("original upstream ISLE file blob drift")
    path = root / SOURCE
    if git(root, "hash-object", SOURCE) != SOURCE_BLOB:
        raise ValueError("upstream working file was already edited")
    if RULE_NAME in path.read_text():
        raise ValueError("experimental rule already present")
    return path


def write_new(path: Path, data: dict) -> None:
    if path.exists():
        raise ValueError("refusing to overwrite a qualification receipt: " + str(path))
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")


def prove(out: Path) -> None:
    import z3
    x = z3.BitVec("x", 64)
    hi = z3.BitVecVal(SMAX, 64)
    lo = z3.BitVecVal(1 << 63, 64)
    original = z3.If(x < z3.BitVecVal(0, 64), lo, hi)
    replacement = hi + z3.LShR(x, 63)
    solver = z3.SolverFor("QF_BV")
    solver.set(timeout=30000)
    solver.add(original != replacement)
    verdict = solver.check()
    if verdict != z3.unsat:
        raise ValueError("QF_BV unproven: " + str(verdict) + " " + solver.reason_unknown())
    write_new(out, {
        "schema": "zenofcis/tier1-satadd-new-pin-proof/1",
        "status": "unsat",
        "scope": "pure signed-i64 sign clamp bits, full 2^64 input domain",
        "solver": z3.get_full_version(), "logic": "QF_BV", "timeout_ms": 30000,
        "smt2_sha256": digest(solver.to_smt2().encode()),
        "isle_rule": RULE_NAME, "rule_sha256": digest(RULE.encode()),
        "clif_sha256": digest(CLIF.read_bytes()), "upstream_commit": UPSTREAM,
        "upstream_source_blob": SOURCE_BLOB,
        "excluded": ["full ISLE rule chains", "poison", "undefined flags",
                     "target machine state", "ABI", "register allocation",
                     "unwind", "throughput", "unrestricted optimality"],
    })


def patch(root: Path, out: Path) -> None:
    path = ensure_pin(root)
    old = path.read_text()
    if not old.startswith(";; `select`/`bitselect`-related rewrites"):
        raise ValueError("unrecognized upstream ISLE source opening")
    if git(root, "diff", "--name-only"):
        raise ValueError("refusing dirty upstream checkout")
    patched = old.rstrip("\n") + "\n" + RULE
    path.write_text(patched)
    if set(git(root, "diff", "--name-only").splitlines()) != {SOURCE}:
        raise ValueError("unexpected upstream changes after patch")
    write_new(out, {
        "schema": "zenofcis/tier1-satadd-new-pin-patch/1",
        "status": "patched-disposable-checkout",
        "upstream_commit": UPSTREAM, "original_blob": SOURCE_BLOB,
        "source_path": SOURCE, "rule_name": RULE_NAME,
        "rule_sha256": digest(RULE.encode()),
        "original_sha256": digest(old.encode()),
        "patched_sha256": digest(patched.encode()),
        "nonclaim": "isolated human-authored ISLE experiment, not upstream",
    })


def prefix(path: Path) -> dict:
    # -w prevents objdump wrapping 10-byte movabs and memory operands into
    # continuation byte rows, which otherwise appear as false instruction gaps.
    # Keep the existing strict contiguous-offset and first-RET checks.
    dump = subprocess.check_output(["objdump", "-d", "-w", "-Mintel", str(path)], text=True)
    lines = []
    pat = re.compile(r"^\s*([0-9a-f]+):\s+((?:[0-9a-f]{2}\s+)+)\s*(\S.*)$", re.I)
    for line in dump.splitlines():
        found = pat.match(line)
        if not found:
            continue
        offset = int(found.group(1), 16)
        code = bytes.fromhex(found.group(2))
        inst = found.group(3).strip()
        if lines and offset != lines[-1][0] + len(lines[-1][1]):
            raise ValueError("disassembly has a gap before RET; no reliable byte attribution")
        lines.append((offset, code, inst))
        if re.match(r"^ret(?:q)?(?:\s|$)", inst, re.I):
            break
    if not lines or not re.match(r"^ret(?:q)?(?:\s|$)", lines[-1][2], re.I):
        raise ValueError("could not decode complete function prefix to RET")
    return {"instruction_bytes_through_ret": sum(len(c) for _, c, _ in lines),
            "instructions_through_ret": len(lines),
            "first_ret_end_offset": lines[-1][0] + len(lines[-1][1]),
            "disassembly_sha256": digest(dump.encode()),
            "scope": "decoded prefix through first RET, includes function prologue/epilogue"}


def finite_native(base: Path, candidate: Path) -> dict:
    if platform.system() != "Linux" or platform.machine().lower() not in ("x86_64", "amd64"):
        raise ValueError("native reference requires Linux x86-64 SysV")
    with tempfile.TemporaryDirectory(prefix="tier1-satadd-newpin-") as d:
        fns = []
        for obj in (base, candidate):
            so = Path(d) / (obj.stem + ".so")
            subprocess.run(["gcc", "-shared", "-o", str(so), str(obj)],
                           check=True, capture_output=True, text=True)
            lib = ctypes.CDLL(str(so))
            fn = getattr(lib, SYMBOL)
            fn.argtypes = (ctypes.c_int64, ctypes.c_int64)
            fn.restype = ctypes.c_int64
            fns.append((lib, fn))
        rng = random.Random(SEED)
        vectors = [(x, y) for x in EDGE for y in EDGE]
        vectors.extend((rng.randrange(SMIN, SMAX + 1), rng.randrange(SMIN, SMAX + 1))
                       for _ in range(20000))
        for idx, (x, y) in enumerate(vectors):
            expected = max(SMIN, min(SMAX, x + y))
            for _, fn in fns:
                if fn(x, y) != expected:
                    raise ValueError("native mismatch at vector " + str(idx))
    return {"classification": "finite native executions, NOT exhaustive",
            "pairs_per_object": len(vectors), "objects": 2,
            "calls": 2 * len(vectors), "boundary_grid_pairs": len(EDGE) ** 2,
            "seed": SEED, "failures": 0,
            "reference": "Python arbitrary-precision signed saturation"}


def measure(root: Path, base: Path, candidate: Path, proof_file: Path,
            patch_file: Path, out: Path) -> None:
    if git(root, "rev-parse", "HEAD") != UPSTREAM:
        raise ValueError("upstream revision drift")
    if git(root, "rev-parse", "HEAD:" + SOURCE) != SOURCE_BLOB:
        raise ValueError("original ISLE blob drift")
    if set(git(root, "diff", "--name-only").splitlines()) != {SOURCE}:
        raise ValueError("unrecognized compiler edits")
    proof, patch_receipt = json.loads(proof_file.read_text()), json.loads(patch_file.read_text())
    if (proof.get("status") != "unsat" or patch_receipt.get("status") != "patched-disposable-checkout"
            or proof.get("clif_sha256") != digest(CLIF.read_bytes())
            or proof.get("rule_sha256") != digest(RULE.encode())
            or patch_receipt.get("rule_sha256") != digest(RULE.encode())
            or patch_receipt.get("patched_sha256") != digest((root / SOURCE).read_bytes())
            or proof.get("upstream_commit") != UPSTREAM
            or patch_receipt.get("upstream_commit") != UPSTREAM):
        raise ValueError("proof, source, or patch receipts do not match")
    old, new = profile(base), profile(candidate)
    old_prefix, new_prefix = prefix(base), prefix(candidate)
    for summary, body in ((old, old_prefix), (new, new_prefix)):
        if body["instruction_bytes_through_ret"] > summary["function"]["bytes"]:
            raise ValueError("decoded executable bytes exceed symbol size")
    native = finite_native(base, candidate)
    result = {
        "schema": "zenofcis/tier1-satadd-current-upstream-a-b/1",
        "upstream_commit": UPSTREAM, "upstream_source_blob": SOURCE_BLOB,
        "source_fixture_sha256": digest(CLIF.read_bytes()),
        "rule_name": RULE_NAME, "rule_sha256": digest(RULE.encode()),
        "proof_sha256": digest(proof_file.read_bytes()),
        "patch_receipt_sha256": digest(patch_file.read_bytes()),
        "baseline": {**{k:v for k,v in old.items() if k != "disassembly"}, **old_prefix},
        "experimental": {**{k:v for k,v in new.items() if k != "disassembly"}, **new_prefix},
        "full_symbol_byte_delta": old["function"]["bytes"] - new["function"]["bytes"],
        "executable_prefix_byte_delta": (old_prefix["instruction_bytes_through_ret"] -
                                           new_prefix["instruction_bytes_through_ret"]),
        "native": native,
        "compiler_settings": {"target": "x86_64", "opt_level": "speed_and_size"},
        "comparison": "identical source CLIF, pristine vs one-rule patched same upstream checkout",
        "discovery_class": "human-authored historical ISLE rule; not newly synthesized",
        "not_established": ["upstream-worthy size win until data analyzed", "runtime speedup",
                            "full compiler-pass/VeriISLE/poison/ABI/flags correctness",
                            "whole-AOT savings", "general-target profitability"],
    }
    write_new(out, result)
    (out.parent / "baseline-objdump.txt").write_text(old["disassembly"])
    (out.parent / "experimental-objdump.txt").write_text(new["disassembly"])
    print(json.dumps({"full_symbol_delta": result["full_symbol_byte_delta"],
                      "decoded_prefix_delta": result["executable_prefix_byte_delta"],
                      "finite_native_calls": native["calls"]}))


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--phase", required=True, choices=("prove", "patch", "measure"))
    p.add_argument("--upstream", type=Path)
    p.add_argument("--baseline", type=Path)
    p.add_argument("--candidate", type=Path)
    p.add_argument("--proof", type=Path)
    p.add_argument("--patch", type=Path)
    p.add_argument("--out", type=Path, required=True)
    a = p.parse_args()
    if a.phase == "prove":
        prove(a.out)
    elif a.phase == "patch":
        if a.upstream is None:
            p.error("--upstream required")
        patch(a.upstream, a.out)
    else:
        if any(x is None for x in (a.upstream, a.baseline, a.candidate, a.proof, a.patch)):
            p.error("--upstream --baseline --candidate --proof --patch required")
        measure(a.upstream, a.baseline, a.candidate, a.proof, a.patch, a.out)


if __name__ == "__main__":
    main()
