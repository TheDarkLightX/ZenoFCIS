#!/usr/bin/env python3
"""Replay the explicit ontology-to-arithmetic handoff and its evidence lanes.

Only fixed local commands run. No command, evidence status or path from an
ontology/handoff is executed. --native and --formal explicitly select lanes;
omitted or unsuccessful lanes leave the corresponding obligations OPEN.
"""
from __future__ import annotations

import argparse
from hashlib import sha256
import json
import os
from pathlib import Path
import re
import resource
import shutil
import signal
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "integrations/zal"))
import arithmetic_handoff_tools as adapter
from test_ontology_arithmetic_handoff import decision_cases, finite_oracle, example, bindings
from arithmetic_handoff import select
import ontology_arithmetic_oracle as oracle

NATIVE_ARGV = ["cargo", "+1.97.1", "test", "--locked", "--offline", "--color", "never",
               "--jobs", "1", "-p", "zeno-fcis-schema", "--features", "schema-profile-2",
               "--test", "ontology_arithmetic_handoff", "--", "--test-threads=1"]
TESTS = {"independent_vectors", "schema_ranges_and_nominal_refusals", "requirements_select_actual_schema_types"}


def digest(path):
    return sha256(path.read_bytes()).hexdigest()


def snapshot():
    paths = set(adapter.source_binding()["contracts"]["files"])
    paths.update(str(p.relative_to(ROOT)) for p in (ROOT / "integrations/zal").rglob("*.py"))
    paths.update(str(p.relative_to(ROOT)) for p in (ROOT / "test-data/ontology-arithmetic").glob("*.tsv"))
    paths.update(("integrations/zal/arithmetic_handoff_sources.json",
                  "integrations/zal/examples/ontology/defi_arithmetic.bindings.json",
                  "crates/zeno-fcis-schema/tests/ontology_arithmetic_handoff.rs",
                  "tools/check_arithmetic_handoff.py", "tools/test_check_arithmetic_handoff.py",
                  "tools/ontology_arithmetic_oracle.py"))
    return {p: digest(ROOT / p) for p in sorted(paths)}


def contained_run(argv, cwd, directory, label, *, env=None, seconds=180):
    """Linux process group, CPU/address-space/file caps and bounded output files.

    These are qualification resource limits, not a security sandbox. All source
    is trusted repository code. A killed or incomplete process is never proof.
    """
    def caps():
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
        resource.setrlimit(resource.RLIMIT_AS, (4 << 30, 4 << 30))
        resource.setrlimit(resource.RLIMIT_CPU, (seconds, seconds))
        resource.setrlimit(resource.RLIMIT_FSIZE, (32 << 20, 32 << 20))
        os.nice(10)
    stdout, stderr = directory / (label + ".stdout"), directory / (label + ".stderr")
    timed_out, startup_error, code = False, None, None
    with stdout.open("xb") as out, stderr.open("xb") as err:
        try:
            process = subprocess.Popen(argv, cwd=cwd, env=env, stdout=out, stderr=err,
                                       start_new_session=True, preexec_fn=caps)
        except OSError as error:
            startup_error = type(error).__name__
            err.write((startup_error + ": process startup failed\n").encode())
        else:
            try:
                code = process.wait(timeout=seconds + 15)
            except subprocess.TimeoutExpired:
                timed_out = True
                os.killpg(process.pid, signal.SIGKILL)
                code = process.wait()
            finally:
                # All children belong to this one invocation, including on timeout.
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
    record = {"argv": argv, "cwd": ".", "exit": code, "timed_out": timed_out,
              "startup_error": startup_error,
              "stdout_sha256": digest(stdout), "stderr_sha256": digest(stderr)}
    return record, stdout.read_text(), stderr.read_text()


def native_outcome(record, stdout, stderr, *, failed_test=None):
    diagnostics = re.findall(r"^error(?:\[[^\]]+\])?: (.*)$", stderr, re.M)
    if record["timed_out"] or "could not compile" in stderr or any(
            not d.startswith("test failed, to rerun pass ") for d in diagnostics):
        return False
    events = re.findall(r"^test (\w+) \.\.\. (ok|FAILED)$", stdout, re.M)
    observed = dict(events)
    summaries = re.findall(r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; 0 ignored; 0 measured; 0 filtered out; finished in [0-9.]+s$", stdout, re.M)
    if len(events) != 3 or set(observed) != TESTS or len(summaries) != 1:
        return False
    if failed_test is None:
        return record["exit"] == 0 and set(observed.values()) == {"ok"} and summaries == [("ok", "3", "0")]
    expected = {failed_test} if isinstance(failed_test, str) else set(failed_test)
    return (record["exit"] == 101 and {k for k, v in observed.items() if v == "FAILED"} == expected
            and "assertion" in stdout and summaries == [("FAILED", str(3-len(expected)), str(len(expected)))])


def assess(packet, native, controls, formal, prerequisites):
    """Promote only completed lanes whose full source/oracle prerequisites hold."""
    native_complete = (native["status"] == "satisfied" and len(controls) == 4
                       and len({c["name"] for c in controls}) == 4
                       and all(c["status"] == "refuted" for c in controls))
    assessments = []
    for row in packet["requirements"]:
        for obligation in row["obligations"]:
            lane = "formal" if obligation["kind"] in ("representation", "numeric-order") else "native" if obligation["kind"] == "native-corpus" else None
            assessment = {"obligation_id": obligation["id"], "kind": obligation["kind"],
                          "status": obligation["status"] if prerequisites else "open", "evidence": [],
                          "source_binding_digest": obligation["source_binding_digest"]}
            if prerequisites and ((lane == "formal" and formal["status"] == "satisfied") or
                                  (lane == "native" and native_complete)):
                assessment["status"] = "satisfied"
                assessment["evidence"] = ["qualification/" + lane]
            assessments.append(assessment)
    return assessments


def native_lane(directory):
    env = {k: v for k, v in os.environ.items() if not k.startswith(("RUST", "CARGO_"))
           or k in ("RUSTUP_HOME", "CARGO_HOME")}
    env.update(CARGO_PROFILE_TEST_DEBUG="0", CARGO_PROFILE_DEV_DEBUG="0", CARGO_INCREMENTAL="0",
               CARGO_TARGET_DIR=str(directory / "native-target"), CARGO_NET_OFFLINE="true")
    def tool_identity():
        identities = {}
        for name in ("cargo", "rustc"):
            located = subprocess.run(["rustup", "which", "--toolchain", "1.97.1", name], env=env,
                                     capture_output=True, text=True, timeout=15, check=True)
            path = Path(located.stdout.strip())
            version = subprocess.run([str(path), "--version", "--verbose"], env=env,
                                     capture_output=True, text=True, timeout=15, check=True)
            if not version.stdout.startswith(name + " 1.97.1 "):
                raise ValueError("unexpected native tool version")
            identities[name] = {"sha256": digest(path), "version": version.stdout.strip()}
        launcher = shutil.which("cargo", path=env.get("PATH"))
        if launcher is None:
            raise FileNotFoundError("cargo launcher unavailable")
        identities["cargo_launcher_sha256"] = digest(Path(launcher).resolve())
        return identities
    try:
        identities = tool_identity()
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        return {"status": "open", "reason": "native tool preflight failed", "error_kind": type(error).__name__}, []
    record, stdout, stderr = contained_run(NATIVE_ARGV, ROOT, directory, "native-positive", env=env)
    record["status"] = "satisfied" if native_outcome(record, stdout, stderr) else "open"
    record["tools"] = identities
    record["scope"] = "1600 comparison pairs, 320 scalar intervals, nominal/identity refusals and two real requirement consumers. Not universal refinement."
    controls = []
    if record["status"] != "satisfied":
        return record, controls
    validate = "crates/zeno-fcis-schema/src/profile2/validate.rs"
    projection = "test-data/ontology-arithmetic/requirements.tsv"
    mutations = [
        ("unsigned_accepts_signed", validate,
         "match (definition.kind(), value.view()) {",
         "match (definition.kind(), value.view()) {\n        (TypeKind::U256 { .. }, ValueRef::I256(_)) => Ok(()),",
         "schema_ranges_and_nominal_refusals"),
        ("signed_accepts_unsigned", validate,
         "match (definition.kind(), value.view()) {",
         "match (definition.kind(), value.view()) {\n        (TypeKind::I256 { .. }, ValueRef::U256(_)) => Ok(()),",
         "schema_ranges_and_nominal_refusals"),
        ("signed_range_uses_bit_order", validate,
         "(TypeKind::I256 { min, max }, ValueRef::I256(integer)) => {\n            if !(*min).cmp(integer).is_gt() && !integer.cmp(*max).is_gt() {",
         "(TypeKind::I256 { min, max }, ValueRef::I256(integer)) => {\n            if !min.bits().cmp(integer.bits()).is_gt() && !integer.bits().cmp(max.bits()).is_gt() {",
         ("schema_ranges_and_nominal_refusals", "requirements_select_actual_schema_types")),
        ("swapped_requirement", projection, "signedPnl 2 I256", "signedPnl 2 U256",
         "requirements_select_actual_schema_types"),
    ]
    for name, path, old, new, failed in mutations:
        source = (ROOT / path).read_text()
        if source.count(old) != 1:
            raise ValueError("mutation anchor changed: " + name)
        changed = source.replace(old, new)
        with tempfile.TemporaryDirectory(prefix="handoff-mutant-") as temporary:
            specimen = Path(temporary) / "source"
            shutil.copytree(ROOT, specimen, ignore=shutil.ignore_patterns(".git", "target", "__pycache__"))
            (specimen / path).write_text(changed)
            # Each specimen gets a fresh compilation cache. Copy-preserved mtimes
            # and the same package ID must not reuse a previous mutant's library.
            mutant_env = {**env, "CARGO_TARGET_DIR": str(directory / (name + "-target"))}
            outcome, out, err = contained_run(NATIVE_ARGV, specimen, directory, name, env=mutant_env)
        outcome.update(name=name, path=path, original_sha256=digest(ROOT / path),
                       mutant_sha256=sha256(changed.encode()).hexdigest(),
                       expected_failed_tests=[failed] if isinstance(failed, str) else list(failed),
                       status="refuted" if native_outcome(outcome, out, err, failed_test=failed) else "open")
        controls.append(outcome)
    record["tool_postcheck"] = tool_identity() == identities
    if not record["tool_postcheck"]:
        record["status"] = "open"
    return record, controls


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--native", action="store_true")
    parser.add_argument("--formal", action="store_true")
    parser.add_argument("--verus-cache", type=Path)
    args = parser.parse_args()
    directory = args.out.absolute()
    if directory.is_relative_to(ROOT):
        raise ValueError("qualification output must be outside source")
    directory.mkdir(parents=True, exist_ok=False)
    before = snapshot()
    failures = []
    for name, expected in oracle.artifacts().items():
        if (oracle.DATA / name).read_bytes() != expected.encode():
            failures.append("stale corpus/projection: " + name)
    cases = list(decision_cases())
    mismatches = [list(c) for c in cases if select(*c) != finite_oracle(*c)]
    if len(cases) != 2304 or mismatches:
        failures.append("finite decision table mismatch")
    packet = adapter.handoff(example(), bindings())
    result = {"schema": "zeno-fcis/arithmetic-handoff-qualification/1", "source_bindings": before,
              "handoff": packet, "selector": {"status": "refuted" if mismatches else "satisfied",
                "domain_size": len(cases), "counterexamples": mismatches,
                "scope": "Exhaustive actual select function against an independent finite decision table. Not a proof of the ontology reasoner or the entire Python adapter."},
              "native": {"status": "open", "reason": "not requested"}, "negative_controls": [],
              "formal": {"status": "open", "reason": "not requested"}, "authority": "none"}
    if args.native:
        result["native"], result["negative_controls"] = native_lane(directory)
        if result["native"]["status"] != "satisfied" or any(c["status"] != "refuted" for c in result["negative_controls"]):
            failures.append("native evidence or semantic negative controls incomplete")
    if args.formal:
        command = [sys.executable, "tools/check_ordering_types256.py", "--out", str(directory / "formal")]
        if args.verus_cache:
            command.extend(["--cache", str(args.verus_cache.absolute())])
        record, out, err = contained_run(command, ROOT, directory, "formal-gate", seconds=240)
        if record["exit"] == 0 and not record["timed_out"] and (directory / "formal/receipt.json").is_file():
            proof = json.loads((directory / "formal/receipt.json").read_text())
            record.update(status="satisfied", receipt=proof)
        else:
            record.update(status="open", reason="complete pinned proof gate did not succeed")
            failures.append("formal gate incomplete")
        # Portable replay argv; installation paths are environment configuration.
        record["argv"] = ["python3", "tools/check_ordering_types256.py", "--out", "<fresh-output>"]
        result["formal"] = record
    try:
        result["source_postcheck"] = snapshot() == before
    except (OSError, ValueError):
        result["source_postcheck"] = False
    if not result["source_postcheck"]:
        failures.append("source changed during qualification")
    result["obligation_assessments"] = assess(packet, result["native"], result["negative_controls"],
                                              result["formal"], prerequisites=not failures)
    result["failures"] = failures
    result["status"] = "incomplete" if failures else "checked-requested-lanes"
    result["universal_schema_refinement"] = "open"
    (directory / "qualification.json").write_text(json.dumps(result, sort_keys=True, indent=2) + "\n")
    print(json.dumps({"status": result["status"], "selector_cases": len(cases), "native": result["native"]["status"],
                      "formal": result["formal"]["status"], "negative_controls": [c["status"] for c in result["negative_controls"]],
                      "failures": failures, "schema_refinement": "open"}))
    return 2 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
