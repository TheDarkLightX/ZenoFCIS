#!/usr/bin/env python3
"""Qualify actual fixed-width numeric bodies and semantic defect controls.

The reviewed profile binds the integer specification, executable declarations,
refinements and complete proof closure. Compiler/ghost erasure is assumed.
"""
from __future__ import annotations

import argparse
from collections import Counter
import copy
import json
import os
from pathlib import Path
import re
import shutil

import check_verus as verifier
from check_ordering_types256 import method_region, read, require
from verus_coverage import inventory, parse_vir

ROOT = Path(__file__).resolve().parents[1]
BASE = Path("crates/zeno-fcis-value/src/profile2")
SUBJECT = BASE / "arithmetic256.rs"
MODEL = BASE / "numeric256_model.rs"
HARNESS = Path("verification/verus/numeric256_harness.rs")
PROFILE = Path("verification/verus/numeric256_profile.json")
NAMESPACE = "numeric256_harness::"
UNIT = (SUBJECT, MODEL, BASE / "number256.rs", BASE / "ordering_types_model.rs",
        BASE / "ordering256.rs", BASE / "ordering_model.rs",
        BASE / "reference/legacy_wide.rs", BASE / "reference/legacy_wide_model.rs", HARNESS)
SOURCES = (*UNIT, PROFILE, verifier.PIN, Path("tools/check_numeric256.py"),
           Path("tools/test_check_numeric256.py"), Path("tools/record_numeric256_profile.py"),
           Path("tools/check_ordering_types256.py"), Path("tools/check_verus.py"),
           Path("tools/verus_coverage.py"), *verifier.QUALIFICATION_SOURCES)
CONSTANTS = {"U256::ZERO", "U256::ONE", "U256::MAX", "I256::ZERO", "I256::MIN",
             "I256::MAX", "U512::ZERO", "U768::ZERO"}
CONVERSIONS = {"U256::from_u128", "U256::try_u128", "I256::from_i128", "I256::magnitude",
               "I256::from_magnitude", "I256::try_unsigned", "I256::difference_unsigned",
               "U512::from_u256", "U512::narrow", "U768::from_u256", "U768::narrow"}
CHECKED = {"U256::checked_add", "U256::checked_sub", "I256::checked_add", "I256::checked_sub", "I256::checked_neg"}
DIVISION = {"U256::div_rem", "U256::checked_div"}
MULTIPLICATION = {"U256::full_product", "U256::checked_mul"}
# Private module-level helpers whose actual bodies a control may mutate.
HELPERS = {"divide", "add_word", "multiply"}


def expected_type(name):
    if name.startswith("Result<") and name.endswith(",Failure>"):
        value = name[len("Result<"):-len(",Failure>")]
        return ["Typ", "Datatype", ["Dt", "Path", "core::result::Result"],
                [expected_type(value), expected_type("Failure")], []]
    if name.startswith("(") and name.endswith(")"):
        items = name[1:-1].split(",")
        return ["Typ", "Datatype", ["Dt", "Tuple", str(len(items))], [expected_type(i) for i in items], []]
    if name in ("U256", "I256", "U512", "U768", "Failure", "Rounding"):
        path = NAMESPACE + "arithmetic256::" + ("number256::" if name in ("U256", "I256") else "") + name
        return ["Typ", "Datatype", ["Dt", "Path", path], [], []]
    if name in ("u128", "i128"):
        return ["Typ", "Int", ["IntRange", "U" if name == "u128" else "I", "128"]]
    if name == "bool":
        return ["Typ", "Bool"]
    raise ValueError("unknown full-width numeric type")


def parameter_types(node):
    fields = dict(zip(node[2::2], node[3::2]))
    def typ(param):
        require(param[0] == "Param", "unknown numeric typed parameter")
        info = dict(zip(param[1::2], param[2::2]))
        require(info[":mode"] == "Exec" and info[":user_mut"] == "false",
                "numeric boundary parameter is ghost or mutable")
        return info[":typ"]
    return [typ(p) for p in fields[":params"]], typ(fields[":ret"])


def report_function(obligation):
    if obligation["member"] in CONSTANTS:
        return obligation["refinement"]
    kind, method = obligation["member"].split("::")
    return NAMESPACE + "arithmetic256::" + ("number256::" if kind in ("U256", "I256") else "") + kind + "::" + method


def snapshot(root):
    result = {}
    for path in SOURCES:
        read(root, path)
        result[str(path)] = verifier.digest(root / path)
    return result


def configuration(root, profile, expected=None):
    require(profile.get("schema") == "zeno-fcis/numeric256-profile/1"
            and profile.get("namespace") == NAMESPACE, "unexpected numeric profile")
    require(set(profile["source_hashes"]) == {str(p) for p in UNIT}, "incomplete source profile")
    hashes = profile["source_hashes"] if expected is None else expected
    require(set(hashes) == set(profile["source_hashes"]), "incomplete specimen binding")
    require(set(profile["native_configuration_exceptions"]).issubset({str(p) for p in UNIT}),
            "foreign configuration exception")
    admitted = {}
    for path in UNIT:
        source = read(root, path)
        require(verifier.digest(root / path) == hashes[str(path)], f"source changed: {path}")
        admitted[str(path)] = verifier.check_native_configuration(
            source, path, profile["native_configuration_exceptions"].get(str(path), ()))
        words = {t[0] for t in verifier._rust_tokens(source)}
        require(not words.intersection({"assume", "assume_specification", "admit", "axiom",
                "external_body", "external_fn_specification", "unsafe"}),
                f"untrusted numeric proof escape: {path}")
    require({o["member"] for o in profile["obligations"] if o["stage"] == "constants"}
            == CONSTANTS, "constant obligations incomplete")
    stages = {o["stage"] for o in profile["obligations"]}
    require(stages.issubset({"constants", "conversions", "checked", "division", "multiplication"}),
            "unreviewed numeric stage")
    if "division" in stages:
        require("checked" in stages and
                {o["member"] for o in profile["obligations"] if o["stage"] == "division"} == DIVISION,
                "division obligations incomplete")
    if "multiplication" in stages:
        require("conversions" in stages and
                {o["member"] for o in profile["obligations"] if o["stage"] == "multiplication"}
                == MULTIPLICATION, "multiplication obligations incomplete")
    if "checked" in stages:
        require("conversions" in stages and
                {o["member"] for o in profile["obligations"] if o["stage"] == "checked"} == CHECKED,
                "checked arithmetic obligations incomplete")
    if "conversions" in stages:
        require({o["member"] for o in profile["obligations"] if o["stage"] == "conversions"}
                == CONVERSIONS, "conversion obligations incomplete")
        expected_support = {"I256::negative_bits"} | (set() if "checked" in stages else {"U256::checked_sub"})
        require({o["member"] for o in profile.get("supporting_obligations", [])} == expected_support,
                "conversion dependencies incomplete")
    obligations = profile["obligations"] + profile.get("supporting_obligations", [])
    require(len({o["member"] for o in obligations}) == len(obligations),
            "duplicate numeric obligation")
    return admitted


def helper_region(source, name):
    """Resolve a private module-level helper, from its contract to its closing brace."""
    tokens = verifier._rust_tokens(source)
    values = [t[0] for t in tokens]
    starts = [i for i in range(len(values) - 1) if values[i] == "fn" and values[i + 1] == name]
    require(len(starts) == 1 and "{" in values[starts[0]:], f"missing or ambiguous helper: {name}")
    end, depth = values.index("{", starts[0]), 0
    while True:
        require(end < len(values), f"unclosed helper: {name}")
        depth += (values[end] == "{") - (values[end] == "}")
        if not depth:
            break
        end += 1
    first, _ = verifier.function_lines(source, name)
    return first, source.count("\n", 0, tokens[end][1]) + 1


def control_sources(root, profile):
    controls = profile["controls"]
    require(len({c["name"] for c in controls}) == len(controls), "duplicate numeric control")
    for stage in {o["stage"] for o in profile["obligations"]}:
        require(sum(c["stage"] == stage for c in controls) >= 3, "stage controls incomplete")
    obligations = {o["member"]: o for o in profile["obligations"]}
    source = read(root, SUBJECT)
    result = []
    for control in controls:
        require(control["member"] in obligations
                and control["stage"] == obligations[control["member"]]["stage"],
                "control has no corresponding obligation")
        if "helper" in control:
            # A defect in a private helper must fail inside that helper's own proof.
            require(control["helper"] in HELPERS
                    and control["error_function"] == NAMESPACE + "arithmetic256::" + control["helper"]
                    and control["error_function"] in profile["body_covered_functions"],
                    "helper control targets an unproved or different helper")
            first, last = helper_region(source, control["helper"])
        else:
            require(control["error_function"] == obligations[control["member"]]["report_function"],
                    "control targets a different refinement")
            first, last = (method_region(source, *control["member"].split("::"))
                           if control["stage"] != "constants" else (1, len(source.splitlines())))
        lines = source.splitlines(keepends=True)
        selected = "".join(lines[first - 1:last])
        require(control["old"] and selected.count(control["old"]) == 1
                and control["old"] != control["new"], f"ambiguous numeric mutation: {control['name']}")
        result.append((control, "".join(lines[:first - 1])
                       + selected.replace(control["old"], control["new"])
                       + "".join(lines[last:])))
    return result


def check_inventory(vir, profile):
    actual = inventory(vir, NAMESPACE, tuple(profile["body_covered_functions"]))
    require(actual == profile["functions"], "translated numeric contracts, bodies or inventory changed")
    concrete = {n for n, r in actual.items() if r["mode"] in ("Exec", "Proof")}
    require(concrete == set(profile["body_covered_functions"])
            and all("body_sha256" in r for r in actual.values()), "incomplete numeric body coverage")
    nodes = {n[1][2]: n for n in parse_vir(vir)
             if n and n[0] == "Function" and n[1][2] in actual}
    for obligation in profile["obligations"] + profile.get("supporting_obligations", []):
        name = obligation["refinement"]
        require(name in concrete and actual[name]["requires"] == 0
                and actual[name]["ensures"] > 0, "conditional or missing numeric refinement")
        require(name.endswith("::" + obligation["member"].split("::")[-1])
                and obligation["report_function"] == report_function(obligation)
                and obligation["report_function"] in profile["report_functions"],
                "numeric refinement and diagnostic identity do not agree")
        require(parameter_types(nodes[name]) ==
                ([expected_type(t) for t in obligation["parameters"]], expected_type(obligation["result"])),
                "refinement does not bind the declared actual full-width types")
    return actual


def positive_accepted(proc, report, pin, profile):
    if proc.returncode != 0 or not verifier.proof_succeeded(report, pin):
        return False
    expected = set(profile["report_functions"])
    actual = {n for n in report["func-details"] if n.startswith(NAMESPACE)}
    return report["verification-results"]["verified"] == profile["expected_verified"] and actual == expected


def solver_sections(directory, root):
    sections, queries = [], 0
    for path in sorted(directory.glob("*.smt2")):
        text = path.read_text()
        queries += len(re.findall(r"^\s*\(check-sat\)\s*$", text, re.M))
        # Bodies and the termination checks of recursive specification functions.
        headers = re.findall(r"^;; (Function-Def|Spec-Termination) (\S+)\n;; (.+):(\d+):(\d+):[^\n]*$",
                             text, re.M)
        require(text.count(";; Function-Def ") + text.count(";; Spec-Termination ") == len(headers),
                "unknown numeric body header")
        for kind, name, source, line, _ in headers:
            resolved = Path(source) if Path(source).is_absolute() else root / source
            require(resolved.resolve().is_relative_to(root.resolve()), "body outside qualified source")
            sections.append({"function": name, "source": str(resolved.resolve().relative_to(root.resolve())),
                             "line": int(line), **({"kind": "termination"} if kind == "Spec-Termination" else {})})
    return sections, queries


def smt_coverage(directory, root, profile):
    sections, queries = solver_sections(directory, root)
    key = lambda s: (s.get("kind", "body"), s["function"], s["source"], s["line"])
    require(queries == profile["expected_verified"] == len(profile["smt_sections"])
            and Counter(map(key, sections)) == Counter(map(key, profile["smt_sections"])),
            "missing, duplicate or foreign numeric verification condition")
    return {"conditions": queries, "sections": sections}


def prove(tools, pin, directory, root, harness=HARNESS):
    directory.mkdir(parents=True, exist_ok=False)
    env = {k: v for k, v in os.environ.items() if not k.startswith(("VERUS_", "VARGO_"))
           and k not in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")}
    env.update(RUSTUP_TOOLCHAIN=pin["rust_toolchain"], VERUS_Z3_PATH=str(tools / "z3"), RUST_BACKTRACE="0")
    command = [str(tools / "verus"), "--crate-type=lib", "--edition=2024", "--no-cheating",
               "--no-external-by-default", "--output-json", "--triggers-mode", "silent",
               "--multiple-errors", "0", "--num-threads", "1", "--log", "vir",
               "--log", "vir-option=no_span+no_type+no_fn_details", "--log", "smt",
               "--log-dir", str(directory / "logs"), str(root / harness)]
    proc = verifier.run(command, root, env, timeout=240)
    (directory / "stdout.json").write_text(proc.stdout)
    (directory / "stderr.txt").write_text(proc.stderr)
    (directory / "command.json").write_text(json.dumps(command, indent=2) + "\n")
    return proc, json.loads(proc.stdout)


def check(cache, install, directory, root=ROOT):
    before = snapshot(root)
    profile = json.loads(read(root, PROFILE))
    admitted = configuration(root, profile)
    controls = control_sources(root, profile)
    pin = json.loads(read(root, verifier.PIN))
    tools = verifier.prepare_tools(cache, pin, install)
    proc, report = prove(tools, pin, directory / "positive", root)
    require(positive_accepted(proc, report, pin, profile), "whole numeric closure did not verify")
    check_inventory((directory / "positive/logs/crate.vir").read_text(), profile)
    coverage = smt_coverage(directory / "positive/logs", root, profile)
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
        specimen_profile = copy.deepcopy(profile)
        for entry in specimen_profile["native_configuration_exceptions"].get(str(SUBJECT), ()):
            entry["following"] = entry["following"].replace(control["old"], control["new"])
        configuration(specimen, specimen_profile, expected)
        failed_proc, failed = prove(tools, pin, directory / name, specimen)
        require(verifier.semantic_refusal(failed_proc, failed, pin, Path(control["error_source"]), specimen,
                    region=tuple(control["error_region"]), target_functions=[control["error_function"]]),
                f"defect did not reach intended semantic failure: {name}")
        outcomes.append({"name": name, "member": control["member"], "stage": control["stage"],
                         **({"helper": control["helper"]} if "helper" in control else {}),
                         "exit": failed_proc.returncode, "source_sha256": expected[str(SUBJECT)],
                         "semantic_refusal": True})
    require(snapshot(root) == before, "numeric qualification inputs changed")
    verifier.verify_tool_files(tools, pin)
    return {"schema": "zeno-fcis/numeric256-qualification/1", "source_bindings": before,
            "native_configuration": admitted, "verification": report["verification-results"],
            "coverage": coverage, "defects": outcomes, "obligations": profile["obligations"],
            "scope": profile["scope"], "assumptions": profile["assumptions"],
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
    print(f"numeric256: actual bodies proved; all {len(result['defects'])} semantic controls rejected")


if __name__ == "__main__":
    main()
