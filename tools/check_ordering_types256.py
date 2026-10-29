#!/usr/bin/env python3
"""Qualify the actual 256-bit type/order methods and seven semantic defects.

Use the ordinary process/resource containment for Verus gates. This checks a
reviewed source dialect and complete proof closure, not compiler/ghost erasure.
"""
from __future__ import annotations

import argparse
from collections import Counter
import json
import os
from pathlib import Path
import re
import shutil

import check_verus as verifier
from verus_coverage import inventory, parse_vir

ROOT = Path(__file__).resolve().parents[1]
BASE = Path("crates/zeno-fcis-value/src/profile2")
SUBJECT = BASE / "number256.rs"
MODEL = BASE / "ordering_types_model.rs"
HARNESS = Path("verification/verus/ordering256_types_harness.rs")
PROFILE = Path("verification/verus/ordering_types256_profile.json")
NAMESPACE = "ordering256_types_harness::"
UNIT = (SUBJECT, MODEL, BASE / "ordering256.rs", BASE / "ordering_model.rs",
        BASE / "arithmetic256.rs", BASE / "reference/legacy_wide.rs",
        BASE / "reference/legacy_wide_model.rs", HARNESS)
METHODS = {("U256", name) for name in ("from_words_le", "words_le", "cmp")} | {
    ("I256", name) for name in ("from_bits", "bits", "is_negative", "cmp")}
SOURCES = (*UNIT, PROFILE, verifier.PIN, Path("tools/check_ordering_types256.py"),
           Path("tools/test_check_ordering_types256.py"), Path("tools/check_verus.py"),
           Path("tools/verus_coverage.py"), *verifier.QUALIFICATION_SOURCES)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def read(root, path):
    full = root / path
    require(full.is_file() and not any(p.is_symlink() for p in (full, *full.parents)),
            f"missing or symbolic-link input: {path}")
    return full.read_text()


def snapshot(root):
    result = {}
    for path in SOURCES:
        read(root, path)
        result[str(path)] = verifier.digest(root / path)
    return result


def configuration(root, profile, expected=None):
    require(profile.get("schema") == "zeno-fcis/ordering-types256-profile/1"
            and profile.get("namespace") == NAMESPACE, "unexpected type-order profile")
    require(set(profile["source_hashes"]) == {str(p) for p in UNIT},
            "incomplete type-order source profile")
    hashes = profile["source_hashes"] if expected is None else expected
    require(set(hashes) == set(profile["source_hashes"]), "incomplete specimen binding")
    exceptions = profile["native_configuration_exceptions"]
    require(set(exceptions) == {str(p) for p in (SUBJECT, BASE / "arithmetic256.rs",
            BASE / "reference/legacy_wide.rs", HARNESS)}, "unexpected configuration exception paths")
    admitted = {}
    for path in UNIT:
        source = read(root, path)
        require(verifier.digest(root / path) == hashes[str(path)], f"source changed: {path}")
        admitted[str(path)] = verifier.check_native_configuration(
            source, path, exceptions.get(str(path), ()))
        words = {t[0] for t in verifier._rust_tokens(source)}
        require(not words.intersection({"assume", "assume_specification", "admit", "axiom",
                "external_body", "external_fn_specification", "unsafe"}),
                f"untrusted type-order proof escape: {path}")
    require(profile["expected_verified"] == 21, "type-order condition count changed")
    return admitted


def method_region(source, typename, method):
    """Resolve an actual method within its impl, including its contract lines."""
    tokens = verifier._rust_tokens(source)
    values = [t[0] for t in tokens]
    candidates = []
    for index in range(len(tokens) - 2):
        if values[index:index + 3] != ["impl", typename, "{"]:
            continue
        depth, end = 1, index + 3
        while depth and end < len(tokens):
            depth += (values[end] == "{") - (values[end] == "}")
            end += 1
        require(not depth, "unclosed method implementation")
        start = tokens[index][1]
        text = source[start:tokens[end - 1][2]]
        if re.search(r"\bpub\s+(?:const\s+)?fn\s+" + re.escape(method) + r"\s*\(", text):
            first, last = verifier.function_lines(text, method)
            offset = source.count("\n", 0, start)
            candidates.append((offset + first, offset + last))
    require(len(candidates) == 1, f"missing or ambiguous method: {typename}::{method}")
    return candidates[0]


def control_sources(root, profile):
    controls = profile["controls"]
    require(len(controls) == 7 and {(c["type"], c["method"]) for c in controls} == METHODS
            and len({c["name"] for c in controls}) == 7, "seven method controls incomplete")
    source = read(root, SUBJECT)
    result = []
    for control in controls:
        first, last = method_region(source, control["type"], control["method"])
        lines = source.splitlines(keepends=True)
        selected = "".join(lines[first - 1:last])
        require(control["old"] and selected.count(control["old"]) == 1
                and control["old"] != control["new"], "ambiguous native method mutation")
        changed = "".join(lines[:first - 1]) + selected.replace(
            control["old"], control["new"]) + "".join(lines[last:])
        result.append((control, changed))
    return result


def expected_type(name):
    datatype = lambda path: ["Typ", "Datatype", ["Dt", "Path", path], [], []]
    if name in ("U256", "I256"):
        return datatype(NAMESPACE + "number256::" + name)
    if name in ("&U256", "&I256"):
        return ["Typ", "Decorate", ["TypDecoration", "Ref"], "None", expected_type(name[1:])]
    if name == "words":
        return ["Typ", "Primitive", ["Primitive", "Array"],
                [["Typ", "Int", ["IntRange", "U", "128"]], ["Typ", "ConstInt", "2"]]]
    if name == "Ordering":
        return datatype("core::cmp::Ordering")
    require(name == "bool", "unknown method type")
    return ["Typ", "Bool"]


def check_inventory(vir, profile):
    actual = inventory(vir, NAMESPACE, tuple(profile["body_covered_functions"]))
    require(actual == profile["functions"], "translated contracts, bodies or inventory changed")
    executable = {n for n, r in actual.items() if r["mode"] in ("Exec", "Proof")}
    require(executable == set(profile["body_covered_functions"])
            and len(executable) == 18 and len(actual) == 36
            and all("body_sha256" in r for r in actual.values()), "incomplete type-order body coverage")
    nodes = {n[1][2]: dict(zip(n[2::2], n[3::2])) for n in parse_vir(vir)
             if n and n[0] == "Function" and n[1][2] in actual}
    def parameter(node):
        require(node[0] == "Param", "unknown typed parameter record")
        return dict(zip(node[1::2], node[2::2]))
    for method in profile["methods"]:
        name = method["vir"]
        require(name in executable and actual[name]["requires"] == 0
                and actual[name]["ensures"] > 0, "conditional or missing actual method contract")
        params = [parameter(p) for p in nodes[name][":params"]]
        require([p[":typ"] for p in params] == [expected_type(t) for t in method["parameters"]]
                and parameter(nodes[name][":ret"])[":typ"] == expected_type(method["result"])
                and all(p[":mode"] == "Exec" and p[":user_mut"] == "false" for p in params),
                "method does not use the declared actual full-width types")
    require(len(profile["methods"]) == 9, "actual methods/derived clones incomplete")
    displays = {m["display"] for m in profile["methods"]}
    required = {NAMESPACE + "number256::" + kind + "::" + name for kind, name in METHODS}
    required |= {NAMESPACE + "number256::" + kind + "::clone" for kind in ("U256", "I256")}
    require(displays == required and len({m["vir"] for m in profile["methods"]}) == 9,
            "duplicate or missing typed method binding")
    return actual


def positive_accepted(proc, report, pin, profile):
    if not verifier.proof_succeeded(report, pin) or proc.returncode != 0:
        return False
    aliases = {m["vir"]: m["display"] for m in (*profile["methods"], *profile["view_aliases"])}
    expected = {aliases.get(n, n) for n in profile["functions"]}
    actual = {n for n in report["func-details"] if n.startswith(NAMESPACE)}
    return report["verification-results"]["verified"] == 21 and actual == expected


def smt_coverage(directory, root, profile):
    actual, queries = [], 0
    for path in sorted(directory.glob("*.smt2")):
        text = path.read_text()
        queries += len(re.findall(r"^\s*\(check-sat\)\s*$", text, re.M))
        headers = re.findall(r"^;; Function-Def (\S+)\n;; (.+):(\d+):(\d+):[^\n]*$", text, re.M)
        require(text.count(";; Function-Def ") == len(headers), "unknown type-order body header")
        for name, source, line, _ in headers:
            path = Path(source) if Path(source).is_absolute() else root / source
            require(path.resolve().is_relative_to(root.resolve()), "body outside qualified source")
            actual.append((name, path.resolve().relative_to(root.resolve()).as_posix(), int(line)))
    expected = [(x["function"], x["source"], x["line"]) for x in profile["smt_sections"]]
    require(queries == 21 and len(expected) == 21 and Counter(actual) == Counter(expected),
            "missing, duplicate or foreign type-order verification condition")
    return {"conditions": queries, "sections": profile["smt_sections"]}


def check(cache, install, directory, root=ROOT):
    before = snapshot(root)
    profile = json.loads(read(root, PROFILE))
    admitted = configuration(root, profile)
    controls = control_sources(root, profile)
    pin = json.loads(read(root, verifier.PIN))
    tools = verifier.prepare_tools(cache, pin, install)
    env = {k: v for k, v in os.environ.items() if not k.startswith(("VERUS_", "VARGO_"))
           and k not in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")}
    env.update(RUSTUP_TOOLCHAIN=pin["rust_toolchain"], VERUS_Z3_PATH=str(tools / "z3"), RUST_BACKTRACE="0")
    def prove(source_root, label):
        output = directory / label
        output.mkdir()
        command = [str(tools / "verus"), "--crate-type=lib", "--edition=2024", "--no-cheating",
                   "--no-external-by-default", "--output-json", "--triggers-mode", "silent",
                   "--multiple-errors", "0", "--num-threads", "1", "--log", "vir",
                   "--log", "vir-option=no_span+no_type+no_fn_details", "--log", "smt",
                   "--log-dir", str(output / "logs"), str(source_root / HARNESS)]
        proc = verifier.run(command, source_root, env, timeout=120)
        (output / "stdout.json").write_text(proc.stdout)
        (output / "stderr.txt").write_text(proc.stderr)
        (output / "command.json").write_text(json.dumps(command, indent=2) + "\n")
        return proc, json.loads(proc.stdout), output
    positive, report, output = prove(root, "positive")
    require(positive_accepted(positive, report, pin, profile), "whole type-order closure did not verify")
    check_inventory((output / "logs/crate.vir").read_text(), profile)
    coverage = smt_coverage(output / "logs", root, profile)
    outcomes = []
    for control, changed in controls:
        name = control["name"]
        specimen = directory / (name + "-source")
        for path in UNIT:
            target = specimen / path
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(root / path, target)
        (specimen / SUBJECT).write_text(changed)
        expected = {**profile["source_hashes"], str(SUBJECT): verifier.digest(specimen / SUBJECT)}
        configuration(specimen, profile, expected)
        proc, failed, _ = prove(specimen, name)
        target = NAMESPACE + "number256::" + control["type"] + "::" + control["method"]
        require(verifier.semantic_refusal(proc, failed, pin, SUBJECT, specimen,
                    region=method_region(changed, control["type"], control["method"]),
                    target_functions=[target]), f"method defect did not reach intended semantic failure: {name}")
        outcomes.append({"name": name, "method": target, "exit": proc.returncode,
                         "source_sha256": expected[str(SUBJECT)], "semantic_refusal": True})
    require(snapshot(root) == before, "type-order qualification inputs changed")
    verifier.verify_tool_files(tools, pin)
    return {"schema": "zeno-fcis/ordering-types256-qualification/1", "source_bindings": before,
            "native_configuration": admitted, "verification": report["verification-results"],
            "coverage": coverage, "method_defects": outcomes, "scope": profile["scope"],
            "native_execution_qualified": False, "complete_arithmetic_proved": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, default=verifier.cache_directory())
    parser.add_argument("--install", action="store_true")
    parser.add_argument("--out", type=Path, required=True, help="Fresh directory for logs and receipt")
    args = parser.parse_args()
    directory = args.out.absolute()
    directory.mkdir(parents=True, exist_ok=False)
    result = check(args.cache, args.install, directory)
    (directory / "receipt.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print("ordering-types256: actual type/order closure proved; all seven method defects rejected")


if __name__ == "__main__":
    main()
