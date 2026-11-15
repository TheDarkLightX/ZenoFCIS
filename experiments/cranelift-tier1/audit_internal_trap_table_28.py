#!/usr/bin/env python3
"""Independent decoder of pinned Wasmtime .wasmtime.traps, cross-checked with x64 DIV PCs.

Tests only the two-live-pairs fixture and identical-WAT baseline/experimental AOTs.
No general pass, target-state, or performance proof is claimed.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import tempfile
from pathlib import Path

from qualify_udivrem_contexts_28 import CASES

PIN = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
SECTION = ".wasmtime.traps"
BLOCK_SIZE = 128
# Pinned crates/environ/src/trap_encoding.rs: 7th variant (zero-based).
INTEGER_DIVISION_BY_ZERO = 7
HERE = Path(__file__).resolve().parent


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def uleb(data: bytes, pos: int, limit: int) -> tuple[int, int]:
    value = 0
    for i in range(10):
        if pos >= limit:
            raise ValueError("truncated trap-table ULEB")
        byte = data[pos]
        pos += 1
        value |= (byte & 127) << (7 * i)
        if byte < 128:
            if value > (1 << 64) - 1:
                raise ValueError("overwide trap-table ULEB")
            return value, pos
    raise ValueError("nonterminating trap-table ULEB")


def decode(data: bytes) -> list[tuple[int, int]]:
    if len(data) < 8:
        raise ValueError("short .wasmtime.traps section")
    count = int.from_bytes(data[:4], "little")
    blocks = int.from_bytes(data[4:8], "little")
    if blocks != (count + BLOCK_SIZE - 1) // BLOCK_SIZE or len(data) < 8 + blocks * 8:
        raise ValueError("incorrect trap-table count or index")
    index = [
        (int.from_bytes(data[8 + i * 8:12 + i * 8], "little"),
         int.from_bytes(data[12 + i * 8:16 + i * 8], "little"))
        for i in range(blocks)
    ]
    body = data[8 + blocks * 8:]
    if (blocks == 0 and body) or (blocks and index[0][1] != 0):
        raise ValueError("noncanonical trap-table body")
    entries = []
    for i, (first, start) in enumerate(index):
        end = index[i + 1][1] if i + 1 < blocks else len(body)
        if start >= end or end > len(body):
            raise ValueError("invalid trap-table block boundary")
        if i and first <= index[i - 1][0]:
            raise ValueError("trap-table block order invalid")
        default = body[start]
        pos = start + 1
        pc = first
        for j in range(min(BLOCK_SIZE, count - i * BLOCK_SIZE)):
            token, pos = uleb(body, pos, end)
            delta = token >> 1
            if (j == 0 and delta != 0) or (j and delta == 0):
                raise ValueError("unsorted or duplicate trap PC")
            pc += delta
            if pc > 0xffffffff or (entries and pc <= entries[-1][0]):
                raise ValueError("trap PC overflow or reordering")
            if token & 1:
                if pos >= end:
                    raise ValueError("missing overridden trap code")
                code = body[pos]
                pos += 1
            else:
                code = default
            entries.append((pc, code))
        if pos != end:
            raise ValueError("trailing or overlapping encoded trap data")
    if len(entries) != count:
        raise ValueError("trap entry-count mismatch")
    return entries


ADDR = re.compile(r"^\s*([0-9a-fA-F]{1,16}):\s")
DIV = re.compile(r"\bdiv(?:q|l)?\b", re.I)
TRAP = "trap: Normal(IntegerDivisionByZero)"


def parse_disasm(raw: str) -> tuple[set[int], set[int], set[int]]:
    instructions, divisions, annotations = set(), set(), set()
    active = False
    previous = None
    for line in raw.splitlines():
        if line.rstrip().endswith(":") and "]::function[" in line:
            active = True
            previous = None
            continue
        # Only wasm functions are requested, but do not attribute decorations
        # outside a function to a previous instruction.
        if line.rstrip().endswith(":") and not ADDR.match(line):
            active = False
            previous = None
            continue
        if not active:
            continue
        m = ADDR.match(line)
        if m:
            pc = int(m.group(1), 16)
            if pc in instructions:
                raise ValueError("duplicate decoded native instruction address")
            instructions.add(pc)
            previous = pc
            if DIV.search(line):
                divisions.add(pc)
        if "trap:" in line:
            if TRAP in line:
                if previous is None or previous in annotations:
                    raise ValueError("unattached or repeated division trap annotation")
                annotations.add(previous)
            elif "IntegerDivisionByZero" in line:
                raise ValueError("unexpected division trap representation")
    if not instructions or not divisions or not annotations:
        raise ValueError("missing decoded function, DIV, or trap annotations")
    return instructions, divisions, annotations


def check_aot(exe: Path, aot: Path, out: Path, expected_divs: int) -> dict:
    raw = out / (aot.stem + ".traps.bin")
    if raw.exists():
        raise ValueError("refusing to overwrite trap-table bytes")
    subprocess.run(["objcopy", "--dump-section", SECTION + "=" + str(raw), str(aot)],
                   check=True, capture_output=True, text=True, timeout=30)
    entries = decode(raw.read_bytes())
    cmd = [str(exe), "objdump", "--funcs", "wasm", "--addresses", "--traps",
           "--color", "never", str(aot)]
    output = subprocess.run(cmd, check=True, capture_output=True, text=True, timeout=60).stdout
    text = out / (aot.stem + ".native-traps.txt")
    text.write_text(output)
    inst, divs, annotated = parse_disasm(output)
    encoded = {pc for pc, code in entries if pc in inst and code == INTEGER_DIVISION_BY_ZERO}
    if not (len(divs) == expected_divs and divs == annotated == encoded):
        raise AssertionError(
            f"native DIV/trap-table divergence: div={sorted(divs)}, "
            f"objdump={sorted(annotated)}, independently_decoded={sorted(encoded)}"
        )
    if any(pc not in inst for pc in divs):
        raise AssertionError("division outside decoded function")
    return {
        "aot_sha256": digest(aot), "trap_section_sha256": digest(raw),
        "annotated_disassembly_sha256": digest(text),
        "all_encoded_trap_entries": len(entries),
        "division_trap_pcs": sorted(divs),
        "division_trap_codes": [INTEGER_DIVISION_BY_ZERO] * len(divs),
    }


def audit(root: Path) -> dict:
    out = root / "native-trap-table"
    if out.exists():
        raise ValueError("refusing to overwrite trap-table audit")
    if (root / "wasmtime-head.txt").read_text().strip() != PIN:
        raise ValueError("wrong upstream source pin")
    source = root / "two-live-pairs.wat"
    if source.read_text() != CASES["two_live_pairs"]:
        raise ValueError("two-live-pairs source changed")
    contexts = json.loads((root / "context-controls/qualification.json").read_text())
    if contexts.get("upstream_source_pin") != PIN:
        raise ValueError("context receipt source mismatch")
    previous = json.loads((root / "second-trap-audit.json").read_text())
    if previous.get("status") != "passed-finite-identical-WAT-two-distinct-original-Wasm-trap-sites":
        raise ValueError("guest-visible trap-site checkpoint missing")
    out.mkdir()
    results = {}
    for mode, expected in (("baseline", 4), ("patched", 2)):
        exe = root / mode / "wasmtime"
        aot = root / "context-controls/two_live_pairs" / (mode + ".cwasm")
        prior = contexts["cases"]["two_live_pairs"]["results"][mode]
        if digest(exe) != contexts["compiler_pins"][mode]["sha256"]:
            raise ValueError("compiler hash changed")
        if digest(aot) != prior["aot_sha256"]:
            raise ValueError("AOT hash changed")
        results[mode] = check_aot(exe.resolve(), aot.resolve(), out, expected)
    receipt = {
        "schema": "zenofcis/tier1-wasmtime-encoded-native-trap-table/1",
        "status": "passed-exact-pc-division-trap-encoding-crosscheck",
        "zenofcis_head": (root / "zenofcis-head.txt").read_text().strip(),
        "wasmtime_source_pin": PIN,
        "two_live_pairs_wat_sha256": digest(source),
        "context_receipt_sha256": digest(root / "context-controls/qualification.json"),
        "guest_trap_receipt_sha256": digest(root / "second-trap-audit.json"),
        "checker_sha256": digest(Path(__file__)),
        "format_source": "crates/environ/src/compile/trap_encoding.rs at pinned Wasmtime",
        "results": results,
        "scope": [
            "Independently decoded .wasmtime.traps section, not merely matched guest signal",
            "Checks native x86-64 DIV instruction PCs and emitted trap annotations",
            "Two live pairs, identical WAT, source-pinned compilers; not all compiler contexts",
            "Does not prove all machine flags, registers, target ABIs, runtime speed, or pass correctness",
        ],
    }
    (out / "qualification.json").write_text(json.dumps(receipt, sort_keys=True, indent=2) + "\n")
    return receipt


def self_test() -> None:
    # One block with two entries at PCs 0x10 and 0x14, normal divide traps.
    good = (2).to_bytes(4, "little") + (1).to_bytes(4, "little")
    good += (16).to_bytes(4, "little") + (0).to_bytes(4, "little")
    good += bytes([INTEGER_DIVISION_BY_ZERO, 0, 8])
    assert decode(good) == [(16, 7), (20, 7)]
    bad = [good[:-1], good + b"\0", b"", good[:4] + b"\0\0\0\0" + good[8:],
           good[:12] + (1).to_bytes(4, "little") + good[16:],
           good[:-2] + bytes([2, 8]), good[:-1] + b"\\0"]
    for candidate in bad:
        try:
            decode(candidate)
        except ValueError:
            continue
        raise AssertionError("malformed trap table accepted")
    listing = ("wasm[0]::function[0]:\n"
               "0000000010: divq %rcx\n"
               "            ╰─╼ trap: Normal(IntegerDivisionByZero)\n"
               "0000000014: retq\n")
    inst, div, trap = parse_disasm(listing)
    assert div == trap == {16} and inst == {16, 20}
    try:
        parse_disasm(listing.replace("divq", "addq"))
    except ValueError:
        pass
    else:
        raise AssertionError("missing DIV native evidence accepted")
    print("independent Wasmtime trap-table decoder: positive and seven malformed-table controls passed")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("--out", type=Path)
    args = ap.parse_args()
    if args.self_test:
        self_test()
    elif args.out:
        print(json.dumps({"status": audit(args.out)["status"]}, sort_keys=True))
    else:
        ap.error("pass --self-test or --out")
