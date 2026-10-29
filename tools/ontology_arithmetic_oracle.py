#!/usr/bin/env python3
"""Independent integer oracle: Python integers/bytes, no production limb math.

The fixed seed and boundary set are part of this corpus version. Comparisons
and inclusive ranges use Python's numeric operators and signed byte decoding.
The requirements projection is separately obtained from the actual ZAL bridge.
"""
from __future__ import annotations

import argparse
from hashlib import sha256
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
DATA = ROOT / "test-data/ontology-arithmetic"


def values():
    boundary = {0, 1, 2, (1 << 256) - 1}
    for bit in (127, 128, 129, 254, 255):
        boundary.update(((1 << bit) - 1, 1 << bit, (1 << bit) + 1))
    boundary.update(int.from_bytes(sha256(f"ontology-arithmetic-oracle/1/{i}".encode()).digest(), "big")
                    for i in range(21))
    return sorted(boundary)


def signed(n):
    return int.from_bytes(n.to_bytes(32, "big"), "big", signed=True)


def compare(a, b):
    return (a > b) - (a < b)


def vectors():
    numbers = values()
    order = ["# a_bits b_bits unsigned_order signed_order"]
    for a in numbers:
        for b in numbers:
            order.append(f"{a:064x} {b:064x} {compare(a,b)} {compare(signed(a),signed(b))}")
    ranges = ["# kind min_bits max_bits value_bits expected_in_range"]
    for kind, intervals in (
        ("U256", [(0, (1 << 256)-1), (1 << 128, (1 << 255)+1), (0, 0), ((1 << 256)-2, (1 << 256)-1)]),
        ("I256", [(-(1 << 255), (1 << 255)-1), (-(1 << 255), -1), (-1, 1), (0, 0)])):
        for lower, upper in intervals:
            lo = lower.to_bytes(32, "big", signed=kind == "I256").hex()
            hi = upper.to_bytes(32, "big", signed=kind == "I256").hex()
            for bits in numbers:
                value = bits if kind == "U256" else signed(bits)
                ranges.append(f"{kind} {lo} {hi} {bits:064x} {int(lower <= value <= upper)}")
    return {"order.tsv": "\n".join(order) + "\n", "ranges.tsv": "\n".join(ranges) + "\n"}


def projection():
    sys.path.insert(0, str(ROOT / "integrations/zal"))
    from arithmetic_handoff_tools import handoff
    from zal_notation.ontology_cnl import parse_ontology_cnl
    directory = ROOT / "integrations/zal/examples/ontology"
    ontology = parse_ontology_cnl((directory / "defi_arithmetic.english.txt").read_text())
    bindings = json.loads((directory / "defi_arithmetic.bindings.json").read_text())["bindings"]
    packet = handoff(ontology, bindings)
    if packet["status"] != "ready":
        raise ValueError("example handoff is blocked")
    rows = ["# individual type_id nominal_type requirement_id"]
    for row in packet["requirements"]:
        requirement = row["type_requirement"]
        rows.append(f'{row["binding"]["individual"]} {requirement["schema_type_id"]} {requirement["nominal_type"]} {row["requirement_id"]}')
    return "\n".join(rows) + "\n"


def artifacts():
    return {**vectors(), "requirements.tsv": projection()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    DATA.mkdir(exist_ok=True, parents=True)
    for name, text in artifacts().items():
        path = DATA / name
        if args.check:
            if path.read_bytes() != text.encode():
                raise ValueError("stale oracle or requirement projection: " + name)
        else:
            path.write_text(text)
    print("oracle: 40 bit patterns, 1600 comparisons, 320 ranges; exact requirements projection")


if __name__ == "__main__":
    main()
