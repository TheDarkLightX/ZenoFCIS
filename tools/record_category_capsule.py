#!/usr/bin/env python3
"""Produce the pins of the ZAL category capsule, in dependency order.

The chain, each step the consumer's own raw-byte SHA-256:
1. provider/SOURCE_MANIFEST.json pins the eight provider sources;
2. provider/main.py MANIFEST_SHA256 and adapter.py PROVIDER_MANIFEST pin
   that manifest;
3. DEPENDENCIES.json pins the captured library, including the manifest and
   provider/main.py;
4. adapter.py DEPENDENCIES_SHA256 pins DEPENDENCIES.json;
5. host.py ADAPTER_SHA256 pins adapter.py.

The declared closure: both manifests at the base revision (their path sets,
schema and scope), exactly one assignment line per constant, and a provider
directory holding exactly the ten captured files that prepare_runtime copies.
This never prepares or runs the provider.
"""
from __future__ import annotations

import hashlib
from pathlib import Path
import re
import sys

import pin_producer as pins

ZAL = "integrations/zal/"
PROFILE = ZAL + "category_profile/"
PROVIDER = PROFILE + "provider/"
MANIFEST = PROVIDER + "SOURCE_MANIFEST.json"
DEPENDENCIES = PROFILE + "DEPENDENCIES.json"
MAIN = PROVIDER + "main.py"
ADAPTER = PROFILE + "adapter.py"
HOST = PROFILE + "host.py"


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


class Plan:
    """Planned bytes overlay the tree, so later pins bind earlier rewrites."""

    def __init__(self, root: Path):
        self.root, self.files, self.changes = root, {}, []

    def bytes(self, name: str) -> bytes:
        return self.files[name] if name in self.files else pins.read(self.root, name)

    def manifest(self, name: str, prefix: str, base: str) -> None:
        value = pins.load(self.root, name)
        declared = pins.load_at_base(self.root, base, name)
        pins.require_same(name, {**value, "files": list(value["files"])},
                          {**declared, "files": list(declared["files"])}, "path set, schema or scope")
        fresh = {}
        for bound, old in value["files"].items():
            fresh[bound] = sha(self.bytes(prefix + bound))
            if fresh[bound] != old:
                self.changes.append(pins.Change(name, prefix + bound, old, fresh[bound]))
        if fresh != value["files"]:
            self.files[name] = pins.render({**value, "files": fresh})

    def constant(self, name: str, variable: str, bound: str) -> None:
        text = self.bytes(name).decode()
        pattern = re.compile(rf"^{variable} = '([0-9a-f]{{64}})'$", re.M)
        found = pattern.findall(text)
        if len(found) != 1:
            raise pins.Refusal(f"{name}: expected exactly one {variable} assignment")
        fresh = sha(self.bytes(bound))
        if fresh != found[0]:
            self.changes.append(pins.Change(f"{name} {variable}", bound, found[0], fresh))
            self.files[name] = pattern.sub(f"{variable} = '{fresh}'", text).encode()


def provider_files(root: Path) -> set[str]:
    directory = root / PROVIDER
    return {str(p.relative_to(directory)) for p in directory.rglob("*")
            if p.is_file() and "__pycache__" not in p.parts and p.suffix != ".pyc"}


def plan(root: Path, base: str):
    manifest = pins.load(root, MANIFEST)
    dependencies = pins.load(root, DEPENDENCIES)
    captured = set(manifest["files"]) | {"SOURCE_MANIFEST.json", "main.py"}
    if provider_files(root) != captured:
        raise pins.Refusal(f"{PROVIDER}: change outside the declared closure (files "
                           f"{sorted(provider_files(root) ^ captured)})")
    prefix = PROVIDER[len(ZAL):]
    if {n[len(prefix):] for n in dependencies["files"] if n.startswith(prefix)} != captured:
        raise pins.Refusal(DEPENDENCIES + ": provider entries differ from the captured provider files")
    steps = Plan(root)
    steps.manifest(MANIFEST, PROVIDER, base)
    steps.constant(MAIN, "MANIFEST_SHA256", MANIFEST)
    steps.constant(ADAPTER, "PROVIDER_MANIFEST", MANIFEST)
    steps.manifest(DEPENDENCIES, ZAL, base)
    steps.constant(ADAPTER, "DEPENDENCIES_SHA256", DEPENDENCIES)
    steps.constant(HOST, "ADAPTER_SHA256", ADAPTER)
    return steps.changes, steps.files


if __name__ == "__main__":
    sys.exit(pins.main(plan, __doc__))
