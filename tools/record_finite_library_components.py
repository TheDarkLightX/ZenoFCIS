#!/usr/bin/env python3
"""Produce the SHA-256 pins of the ZAL finite-library component list.

integrations/zal/finite_library/UPSTREAM.json names each shipped component by a path relative to the
finite_library directory and pins its raw-byte SHA-256. The declared closure is that file at the base
revision: the component names and paths, the schema and the scope must be unchanged, and every path must stay
inside integrations/zal. The category capsule pins UPSTREAM.json itself, so run
tools/record_category_capsule.py after a write here.
"""
from __future__ import annotations

import hashlib
from pathlib import Path
import posixpath
import sys

import pin_producer as pins

UPSTREAM = "integrations/zal/finite_library/UPSTREAM.json"
DIRECTORY = "integrations/zal/finite_library/"
ENTRY = ("path", "sha256")


def outside_pins(value: dict) -> dict:
    return {**value, "components": [(name, list(entry), entry.get("path")) for name, entry in value["components"].items()]}


def bound(path: str) -> str:
    """The repository path of one component, which must stay inside integrations/zal."""
    name = posixpath.normpath(DIRECTORY + path)
    if posixpath.isabs(path) or not name.startswith("integrations/zal/") or ".." in name.split("/"):
        raise pins.Refusal(UPSTREAM + ": component path outside integrations/zal: " + path)
    return name


def plan(root: Path, base: str):
    value = pins.load(root, UPSTREAM)
    pins.require_same(UPSTREAM, outside_pins(value), outside_pins(pins.load_at_base(root, base, UPSTREAM)),
                      "component list or a non-pin field")
    changes, components = [], {}
    for name, entry in value["components"].items():
        if tuple(entry) != ENTRY or entry["path"] != name:
            raise pins.Refusal(UPSTREAM + ": each component has exactly path and sha256, keyed by its path")
        target = bound(entry["path"])
        fresh = hashlib.sha256(pins.read(root, target)).hexdigest()
        if fresh != entry["sha256"]:
            changes.append(pins.Change(UPSTREAM, target, entry["sha256"], fresh))
        components[name] = {"path": entry["path"], "sha256": fresh}
    files = {UPSTREAM: pins.render({**value, "components": components})} if changes else {}
    return changes, files


if __name__ == "__main__":
    sys.exit(pins.main(plan, __doc__))
