#!/usr/bin/env python3
"""Produce the source_sha256 pins of the typed arithmetic and emission profiles.

Each pin is the gates' own digest (check_arithmetic_proof.digest) of a bound
file. The declared closure is each profile at the base revision: its bound
path set and every other field, such as the function inventory and the
expected count, must be unchanged. This never runs a Verus gate; review the
listed files, then replay check_arithmetic_typed_proof.py and
check_arithmetic_emission.py.
"""
from __future__ import annotations

from pathlib import Path
import sys

import check_arithmetic_proof as primitive
import pin_producer as pins

PROFILES = ("verification/verus/arithmetic-typed-proof.json",
            "verification/verus/arithmetic-emission.json")


def outside_pins(profile: dict) -> dict:
    return {**profile, "source_sha256": list(profile["source_sha256"])}


def plan(root: Path, base: str):
    changes, files = [], {}
    for name in PROFILES:
        profile = pins.load(root, name)
        pins.require_same(name, outside_pins(profile), outside_pins(pins.load_at_base(root, base, name)),
                          "bound path set or a non-pin field")
        fresh = {}
        for bound, old in profile["source_sha256"].items():
            fresh[bound] = primitive.digest(pins.bound_path(root, bound))
            if fresh[bound] != old:
                changes.append(pins.Change(name, bound, old, fresh[bound]))
        if fresh != profile["source_sha256"]:
            files[name] = pins.render({**profile, "source_sha256": fresh})
    return changes, files


if __name__ == "__main__":
    sys.exit(pins.main(plan, __doc__))
