#!/usr/bin/env python3
"""Independent archive audit of the five-case reconstruction SSA qualifier.

The input/result schema is checked, every hashed WAT/AOT/CLIF/disassembly file
is re-opened, native guest costs are re-parsed, and fresh native Wasm outputs
are checked with an independently written oracle. Does not rely on or import
the producer's oracle or native-cost parser.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import random
import re
import subprocess
from pathlib import Path

MASK = (1 << 64) - 1
UPSTREAM = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
TARGET = "x86_64-unknown-linux"
CASES = ("live_q_m_r", "reversed_mul", "wrong_multiplier",
         "stateful_between", "double_reconstruction")
SCHEMA = "zenofcis/tier1-reconstruction-ssa-contexts/1"


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def to_cli_signed(v: int) -> int:
    v %= 2**64
    return v if v < 2**63 else v - 2**64


def rotate(v: int, n: int) -> int:
    return ((v >> (64 - n)) | (v << n)) % (2**64)


def reference(name: str, x: int, y: int) -> int | None:
    if name not in CASES:
        raise KeyError(name)
    xx, yy = x % (2**64), y % (2**64)
    if not yy:
        return None
    quot, _ = divmod(xx, yy)
    product = (quot * (xx if name == "wrong_multiplier" else yy)) % (2**64)
    residual = (xx - product) % (2**64)
    v = quot ^ rotate(product, 7) ^ rotate(residual, 17)
    if name == "stateful_between":
        v ^= rotate(quot, 31)
    if name == "double_reconstruction":
        xx2 = (xx + 17) % (2**64)
        quot2, rem2 = divmod(xx2, yy)
        v ^= rotate(quot2, 19) ^ rotate(quot2 * yy, 23) ^ rotate(rem2, 29)
    return to_cli_signed(v)


def fresh_vectors() -> tuple[tuple[int, int], ...]:
    edges = (-(2**63), -(2**63) + 1, -65537, -7, -1, 0, 1, 2, 3,
             17, 2**32 + 1, 2**63 - 1)
    divisors = (0, 1, 3, -1)
    v = [(x, y) for x in edges for y in divisors]
    rng = random.Random(0x202611128)
    v += [(to_cli_signed(rng.getrandbits(64)),
           to_cli_signed(rng.getrandbits(64))) for _ in range(24)]
    return tuple(dict.fromkeys(v))


def reparse_cost(path: Path) -> dict:
    content = path.read_text()
    label = "wasm[0]::function[0]:"
    if content.count(label) != 1:
        raise ValueError("cannot uniquely isolate exported native function")
    block = content.split(label, 1)[1].split("wasm[", 1)[0]
    count = 0
    total = 0
    div = 0
    for line in block.splitlines():
        if not re.match(r"^\s{12}[a-f0-9]{2}(?:\s|$)", line):
            continue
        tokens = line.strip().split()
        prefix = 0
        for t in tokens:
            if re.fullmatch("[0-9a-f]{2}", t):
                prefix += 1
            else:
                break
        if prefix == 0:
            raise ValueError("invalid native byte column")
        count += 1
        total += prefix
        div += int(bool(re.search(r"\bdivq\b", line)))
    if count < 1:
        raise ValueError("no native guest instructions")
    return {"decoded_guest_instruction_bytes": total,
            "decoded_guest_instruction_count": count,
            "static_divq": div}


def validate(result: Path, archive: Path) -> dict:
    data = json.loads((result / "qualification.json").read_text())
    prod = json.loads((archive / "qualification.json").read_text())
    if data["schema"] != SCHEMA or data["wasmtime_source_commit"] != UPSTREAM or data["target"] != TARGET:
        raise ValueError("wrong output semantic/source schema")
    if data["source_producer_run"] != 42951667233 or prod["upstream"] != UPSTREAM or prod["target"] != TARGET:
        raise ValueError("wrong pinned producer")
    if data["producer_receipt_sha256"] != sha(archive / "qualification.json"):
        raise ValueError("producer receipt mutated")
    if set(data["cases"]) != set(CASES):
        raise ValueError("missing/extra context cases")
    if len(fresh_vectors()) < 70:
        raise ValueError("fresh witness set too small")
    checks = 0
    traps = 0
    checked_files = 0
    deltas = {}
    for name in CASES:
        row = data["cases"][name]
        src = result / name / "input.wat"
        if sha(src) != row["wat_sha256"]:
            raise ValueError(f"{name} WAT changed")
        checked_files += 1
        if set(row["results"]) != {"baseline", "patched"}:
            raise ValueError("missing compiler variant")
        for mode in ("baseline", "patched"):
            v = row["results"][mode]
            compiler = archive / mode / "wasmtime"
            aot = result / name / mode / "work.cwasm"
            dis = result / name / mode / "native.txt"
            if sha(compiler) != data["compiler_sha256"][mode] or sha(compiler) != prod["results"][mode]["compiler_sha256"]:
                raise ValueError("compiler binary changed")
            if v["compiler_sha256"] != sha(compiler) or v["aot_sha256"] != sha(aot):
                raise ValueError(f"{name}/{mode}: compiler or AOT hash mismatch")
            if v["native_disassembly_sha256"] != sha(dis) or v["native_cost"] != reparse_cost(dis):
                raise ValueError(f"{name}/{mode}: native disassembly/hash/cost mismatch")
            checked_files += 3
            for rel, expected in v["clif_hashes"].items():
                p = (result / name / mode / "clif" / rel).resolve()
                if not p.is_relative_to((result / name / mode / "clif").resolve()) or sha(p) != expected:
                    raise ValueError(f"{name}/{mode}: CLIF file changed")
                checked_files += 1
            for x, y in fresh_vectors():
                p = subprocess.run(
                    [str(compiler), "run", "--allow-precompiled", "--invoke", "work",
                     str(aot), str(x), str(y)],
                    capture_output=True, text=True, timeout=40)
                expected = reference(name, x, y)
                if expected is None:
                    if p.returncode == 0 or "trap" not in p.stderr.lower():
                        raise AssertionError(f"{name}/{mode} lost a fresh zero-divisor trap")
                    traps += 1
                elif p.returncode or p.stdout.strip() != str(expected):
                    raise AssertionError(f"{name}/{mode} fresh mismatch at {x},{y}")
                else:
                    checks += 1
        b, p = (row["results"][m] for m in ("baseline", "patched"))
        calc = {metric: b["native_cost"][metric] - p["native_cost"][metric]
                for metric in ("decoded_guest_instruction_bytes",
                               "decoded_guest_instruction_count", "static_divq")}
        calc["whole_aot_bytes"] = b["whole_aot_bytes"] - p["whole_aot_bytes"]
        if calc != row["baseline_minus_patched"]:
            raise ValueError("forged cost delta")
        if row["aot_byte_identical"] != (b["aot_sha256"] == p["aot_sha256"]):
            raise ValueError("forged byte-identical flag")
        if name in ("wrong_multiplier", "stateful_between") and not row["aot_byte_identical"]:
            raise ValueError("unexpected out-of-scope compiler effect")
        deltas[name] = calc
    if (sum(v["finite"]["values"] for row in data["cases"].values()
            for v in row["results"].values()) != data["total_finite_values"]
        or sum(v["finite"]["zero_divisor_traps"] for row in data["cases"].values()
               for v in row["results"].values()) != data["total_zero_divisor_traps"]):
        raise ValueError("producer witness tally inconsistent")
    return {
        "schema": "zenofcis/tier1-reconstruction-ssa-independent-recheck/1",
        "status": "fresh-finite-value-and-trap-replay-passed",
        "wasmtime": UPSTREAM, "producer_run": data["source_producer_run"],
        "producer_result_sha256": sha(result / "qualification.json"),
        "producer_compiler_receipt_sha256": sha(archive / "qualification.json"),
        "rechecker_source_sha256": sha(Path(__file__)),
        "archive_files_hashed": checked_files,
        "fresh_values": checks, "fresh_zero_divisor_traps": traps,
        "cost_recalculated": deltas,
        "limits": [
            "finite independent checker and re-parsed target cost, not whole pass or ISA proof",
            "none of these standalone WAT cases changed guest machine code under the pinned compiler pair",
            "no runtime performance inference; no upstream patch, ISLE rule proof or synthesized discovery",
        ],
    }


def self_test() -> None:
    assert len(fresh_vectors()) >= 70
    for case in CASES:
        assert reference(case, 7, 0) is None
        assert reference(case, 7, 3) is not None
    assert reference("live_q_m_r", 7, 3) != reference("wrong_multiplier", 7, 3)
    assert reference("live_q_m_r", 7, 3) == reference("reversed_mul", 7, 3)
    p = "wasm[0]::function[0]:\n            48 89    movq %rax,%rcx\n"
    import tempfile
    with tempfile.TemporaryDirectory() as td:
        tmp = Path(td) / "native.txt"
        tmp.write_text(p)
        assert reparse_cost(tmp)["decoded_guest_instruction_bytes"] == 2
        tmp.write_text("")
        try:
            reparse_cost(tmp)
        except ValueError:
            pass
        else:
            raise AssertionError("missing symbol accepted")
    print("independent fresh-value reference and cost parser self-tests passed")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("--results", type=Path)
    ap.add_argument("--archive", type=Path)
    ap.add_argument("--out", type=Path)
    a = ap.parse_args()
    if a.self_test:
        self_test()
    else:
        if not a.results or not a.archive or not a.out or a.out.exists():
            ap.error("requires results/archive/out; no evidence overwrite")
        r = validate(a.results.resolve(), a.archive.resolve())
        a.out.write_text(json.dumps(r, sort_keys=True, indent=2) + "\n")
        print(json.dumps({k: r[k] for k in ("status", "archive_files_hashed",
                                           "fresh_values", "fresh_zero_divisor_traps",
                                           "cost_recalculated")}, sort_keys=True))
