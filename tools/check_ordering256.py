#!/usr/bin/env python3
"""Check the actual shared limb comparator and three semantic defects.

Run under the same process/resource containment as the other Verus gates.
This is a source/profile admission gate, not a Rust/Verus erasure theorem.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import shutil

import check_verus as verifier

ROOT = Path(__file__).resolve().parents[1]
PROFILE = Path("verification/verus/ordering256_profile.json")
SUBJECT = Path("crates/zeno-fcis-value/src/profile2/ordering256.rs")
MODEL = SUBJECT.with_name("ordering_model.rs")
ARITHMETIC = SUBJECT.with_name("arithmetic256.rs")
MODULE = SUBJECT.with_name("mod.rs")
HARNESS = Path("verification/verus/ordering256_harness.rs")
UNIT = (SUBJECT, MODEL, ARITHMETIC, MODULE, HARNESS)
TARGET = "ordering256_harness::arithmetic256::compare"
CONTROL_NAMES = {"always_equal", "reverse_pivot", "omit_lowest"}
SOURCES = (*UNIT, PROFILE, verifier.PIN, Path("tools/check_ordering256.py"),
           Path("tools/test_check_ordering256.py"), Path("tools/check_verus.py"),
           *verifier.QUALIFICATION_SOURCES)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def read_source(root, path):
    full = root / path
    require(full.is_file() and not any(p.is_symlink() for p in (full, *full.parents)),
            f"missing or symbolic-link input: {path}")
    return full.read_text()


def snapshot(root):
    result = {}
    for path in SOURCES:
        read_source(root, path)
        result[str(path)] = verifier.digest(root / path)
    return result


def configuration(root, profile, expected=None):
    require(profile.get("schema") == "zeno-fcis/ordering256-profile/1",
            "unexpected comparison profile")
    require(set(profile["source_hashes"]) == {str(p) for p in UNIT},
            "incomplete comparison source profile")
    hashes = profile["source_hashes"] if expected is None else expected
    require(set(hashes) == set(profile["source_hashes"]), "incomplete specimen binding")
    exceptions = profile["native_configuration_exceptions"]
    require(set(exceptions) == {str(p) for p in (ARITHMETIC, MODULE, HARNESS)},
            "unexpected conditional compilation exception paths")
    admitted = {}
    for path in UNIT:
        source = read_source(root, path)
        require(verifier.digest(root / path) == hashes[str(path)],
                f"comparison source changed: {path}")
        admitted[str(path)] = verifier.check_native_configuration(
            source, path, exceptions.get(str(path), ()))
        if path in (SUBJECT, MODEL, HARNESS):
            tokens = {t[0] for t in verifier._rust_tokens(source)}
            require(not tokens.intersection({"assume", "assume_specification", "admit",
                    "axiom", "external_body", "external_fn_specification", "unsafe", "requires"}),
                    f"unconditional comparison proof boundary changed: {path}")
    require(profile["target_function"] == TARGET and profile["expected_verified"] == 2,
            "comparison target/count changed")
    require(profile["body_lines"] == [13, 21], "comparison entry/loop map changed")
    return admitted


def positive_accepted(proc, report, pin, profile):
    local = {n for n in report.get("func-details", {})
             if n.startswith("ordering256_harness::")}
    return (proc.returncode == 0 and verifier.proof_succeeded(report, pin)
            and report["verification-results"]["verified"] == profile["expected_verified"]
            and local == set(profile["local_functions"]) and TARGET in local)


def smt_coverage(directory, root, profile):
    """Require both actual entry and loop queries, not an aggregate count alone."""
    bodies, queries = [], 0
    for path in sorted(directory.glob("*.smt2")):
        text = path.read_text()
        queries += len(re.findall(r"^\s*\(check-sat\)\s*$", text, re.M))
        headers = re.findall(r"^;; Function-Def (\S+)\n;; (.+):(\d+):(\d+):[^\n]*$",
                             text, re.M)
        require(text.count(";; Function-Def ") == len(headers),
                "unrecognized comparison body header")
        for function, source, line, _ in headers:
            resolved = Path(source) if Path(source).is_absolute() else root / source
            require(function == TARGET and resolved.resolve() == (root / SUBJECT).resolve(),
                    "comparison query is not bound to the actual shared body")
            bodies.append(int(line))
    require(queries == 2 and sorted(bodies) == profile["body_lines"],
            "missing, duplicate or additional comparison entry/loop queries")
    return {"function": TARGET, "body_lines": sorted(bodies), "solver_queries": queries}


def control_sources(root, profile):
    controls = profile["mutants"]
    require(len(controls) == 3 and {c["name"] for c in controls} == CONTROL_NAMES,
            "comparison semantic controls incomplete")
    source = read_source(root, SUBJECT)
    result = []
    for control in controls:
        require(control["old"] and source.count(control["old"]) == 1
                and control["new"] != control["old"], "ambiguous comparison mutant")
        result.append((control["name"], source.replace(control["old"], control["new"])))
    return result


def check(cache, install, directory, root=ROOT):
    before = snapshot(root)
    profile = json.loads(read_source(root, PROFILE))
    admitted = configuration(root, profile)
    controls = control_sources(root, profile)
    pin = json.loads(read_source(root, verifier.PIN))
    tools = verifier.prepare_tools(cache, pin, install)
    env = {k: v for k, v in os.environ.items() if not k.startswith(("VERUS_", "VARGO_"))
           and k not in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")}
    env.update(RUSTUP_TOOLCHAIN=pin["rust_toolchain"], VERUS_Z3_PATH=str(tools / "z3"),
               RUST_BACKTRACE="0")

    def prove(source_root, label):
        output = directory / label
        output.mkdir()
        command = [str(tools / "verus"), "--crate-type=lib", "--edition=2024",
                   "--no-cheating", "--no-external-by-default", "--output-json",
                   "--triggers-mode", "silent", "--multiple-errors", "0",
                   "--num-threads", "1", "--log", "smt", "--log-dir", str(output / "logs"),
                   str(source_root / HARNESS)]
        proc = verifier.run(command, source_root, env, timeout=120)
        (output / "stdout.json").write_text(proc.stdout)
        (output / "stderr.txt").write_text(proc.stderr)
        (output / "command.json").write_text(json.dumps(command, indent=2) + "\n")
        return proc, json.loads(proc.stdout), output

    positive, report, output = prove(root, "positive")
    require(positive_accepted(positive, report, pin, profile),
            "whole shared-comparison harness did not verify")
    coverage = smt_coverage(output / "logs", root, profile)
    outcomes = []
    for name, changed in controls:
        specimen = directory / (name + "-source")
        for source in UNIT:
            target = specimen / source
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(root / source, target)
        (specimen / SUBJECT).write_text(changed)
        expected = {**profile["source_hashes"], str(SUBJECT): verifier.digest(specimen / SUBJECT)}
        configuration(specimen, profile, expected)
        proc, mutant, _ = prove(specimen, name)
        require(verifier.semantic_refusal(proc, mutant, pin, SUBJECT, specimen,
                    region=verifier.function_lines(changed, "compare"), target_functions=[TARGET]),
                f"mutant did not reach an intended comparison failure: {name}")
        outcomes.append({"name": name, "exit": proc.returncode,
                         "source_sha256": expected[str(SUBJECT)], "semantic_refusal": True})
    require(snapshot(root) == before, "comparison qualification inputs changed during checking")
    verifier.verify_tool_files(tools, pin)
    return {"schema": "zeno-fcis/ordering256-qualification/1", "source_bindings": before,
            "native_configuration": admitted, "verification": report["verification-results"],
            "coverage": coverage, "controls": outcomes,
            "scope": profile["scope"], "native_execution_or_full_arithmetic_proof": False}


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
    print("ordering256: shared comparator proved; all three semantic defects rejected")


if __name__ == "__main__":
    main()
