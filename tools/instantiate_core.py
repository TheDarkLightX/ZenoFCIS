#!/usr/bin/env python3
"""Instantiate one bounded declarative seed as ordinary, untrusted F1 source.

This substitutes closed integer parameters in data. Only the existing F1
compiler and library Authority make/evaluate decision programs.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import re
from pathlib import Path
from string import Template

ROOT = Path(__file__).resolve().parents[1]
CATALOG = ROOT / "core-components"
FAMILIES = ("reservation-pool", "rate-limiter", "approval-queue")


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def strict_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def read_json(text: str):
    return json.loads(text, object_pairs_hook=strict_object)


def definition(family: str):
    if family not in FAMILIES:
        raise ValueError(f"unsupported family: {family}")
    return read_json((CATALOG / family / "family.json").read_text())


def instance_id(parameters: dict[str, int]) -> str:
    return "-".join(f"{key.lower()}{value}" for key, value in sorted(parameters.items()))


def source(family: str, parameters: dict[str, int], *, include_proof: bool = True) -> dict[str, bytes]:
    spec = definition(family)
    if (any(type(value) is not int for value in parameters.values())
            or parameters not in spec["instances"]):
        raise ValueError(f"unsupported parameters for {family}: {parameters}")
    directory = CATALOG / family
    identity = instance_id(parameters)
    replacements = {**parameters, "INSTANCE": identity}
    inputs = {
        "family.json": (directory / "family.json").read_bytes(),
        "project.zeno.in": (directory / "project.zeno.in").read_bytes(),
        "policy.json.in": (directory / "policy.json.in").read_bytes(),
        f"examples/{identity}.txt": (directory / "examples" / f"{identity}.txt").read_bytes(),
    }
    hashes = {name: digest(data) for name, data in inputs.items()}
    package_hash = digest(json.dumps(hashes, sort_keys=True, separators=(",", ":")).encode())
    project = Template(inputs["project.zeno.in"].decode()).substitute(replacements)
    policy = Template(inputs["policy.json.in"].decode()).substitute(replacements)
    read_json(policy)
    domains = [list(range(int(Template(str(lo)).substitute(replacements)),
                          int(Template(str(hi)).substitute(replacements)) + 1))
               for lo, hi in spec["input_intervals"]]
    proof = None
    if include_proof:
        from prove_core_families import validate_reference
        proof = validate_reference(family)
    proof_note = (f"Identified finite certificate in bound source tree {proof['path']} SHA256 {proof['sha256']}; replay required"
                  if proof else "certificate construction; no stored proof reference")
    # This provenance comment survives ordinary `new --contract` scaffolding.
    project += (f"// Core seed {family}/1; parameters {json.dumps(parameters, sort_keys=True)}.\n"
                f"// Instance source-set SHA256 {package_hash}.\n"
                f"// Family proof reference: {proof_note}; no KernelChecked claim.\n"
                "// Qualification procedure: tools/check_core_components.py in the source tree.\n")
    manifest = {
        "schema": "zeno-fcis/core-seed-instance/1", "family": family, "version": 1,
        "parameters": parameters, "source_sha256": hashes,
        "source_set_sha256": package_hash, "input_domains": domains,
        "input_tuples": math.prod(map(len, domains)),
        "state_tuples": math.prod(map(len, domains[:spec["state_width"]])),
        "parameter_range_evidence": "Identified" if proof else "pending", "family_theorem": None,
        "finite_family_certificate": proof,
        "authority": "none", "owner_adoption": False,
        "examples_provenance": "independent Root reference from public semantics; not owner labels",
    }
    return {"project.zeno": project.encode(), "v2/policy.json": policy.encode(),
            "tests/decision-examples.txt": inputs[f"examples/{identity}.txt"],
            "core-instance.json": (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode()}


def instantiate(family: str, parameters: dict[str, int], output: Path, *, include_proof: bool = True):
    files = source(family, parameters, include_proof=include_proof)
    # Never replace an existing contract or partially overwrite a live app.
    output.mkdir(parents=False, exist_ok=False)
    for relative, contents in files.items():
        target = output / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open("xb") as stream:
            stream.write(contents)
    return read_json(files["core-instance.json"].decode())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("family", choices=FAMILIES)
    parser.add_argument("output", type=Path)
    parser.add_argument("--param", action="append", default=[], metavar="NAME=INTEGER")
    args = parser.parse_args()
    parameters = {}
    try:
        for item in args.param:
            if not re.fullmatch(r"[A-Z]+=[0-9]+", item):
                raise ValueError(f"expected NAME=INTEGER, got {item!r}")
            key, value = item.split("=")
            if key in parameters:
                raise ValueError(f"duplicate parameter: {key}")
            parameters[key] = int(value)
        print(json.dumps(instantiate(args.family, parameters, args.output), sort_keys=True))
    except (ValueError, OSError, KeyError) as error:
        parser.exit(1, f"core instantiation refused: {error}\n")


if __name__ == "__main__":
    main()
