#!/usr/bin/env python3
"""Exercise shared synthesis, real language replay, and hostile artifact cases."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGETS = {"rust": "rs", "python": "py", "javascript": "mjs"}


def invoke(cli: list[str], arguments: list[str], cwd: Path, environment: dict[str, str],
           *, expected: int = 0) -> dict:
    result = subprocess.run(cli + arguments, cwd=cwd, env=environment, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=120)
    if result.returncode != expected:
        raise RuntimeError(f"synthesis exit {result.returncode}, wanted {expected}: {result.stdout} {result.stderr}")
    report = json.loads(result.stdout)
    if report.get("authority") != "none":
        raise RuntimeError("synthesis unexpectedly claims authority")
    return report


def check_vectors(directory: Path, domain: str) -> None:
    """Independent mathematical oracle, without IR/interpreter helpers."""
    vectors = json.loads((directory / "vectors.json").read_text())["cases"]
    actual = {}
    for case in vectors:
        if any(type(value) is not int for value in case["input"] + case["output"]):
            raise RuntimeError("replay scalars must be exact integers")
        inputs = tuple(case["input"])
        if inputs in actual:
            raise RuntimeError("duplicate replay input")
        actual[inputs] = case["output"]
    if domain == "counter":
        expected = {}
        for count in range(4):
            for failures in range(4):
                for command in range(2):
                    for allowed in range(2):
                        selected = failures if command else count
                        if not allowed:
                            out = [0, count, failures, 0, 0, 0]
                        elif selected == 3:
                            out = [1, count, failures, 0, 0, 0]
                        elif command:
                            out = [3, count, failures + 1, 1, count, failures + 1]
                        else:
                            out = [2, count + 1, failures, 1, count + 1, failures]
                        expected[count, failures, command, allowed] = out
    else:
        expected = {(a, b): [min(a + b, 3)] for a in range(4) for b in range(4)}
    if actual != expected:
        raise RuntimeError(f"{domain} differs from independent complete decision table")


def exercise_counter(cli: list[str], app: Path, directory: Path,
                     environment: dict[str, str]) -> dict:
    """Used by both source-consumer and actual packaged-consumer qualification."""
    spec = str(app / "synthesis.json")
    rust = app / "synthesized"
    current = invoke(cli, ["synth", "run", spec, "--out", str(rust), "--check"], directory, environment)
    if current["status"] != "current":
        raise RuntimeError("mounted Rust artifact drifted")
    check_vectors(rust, "counter")
    records = [invoke(cli, ["synth", "verify", spec, "--out", str(rust)], directory, environment)]
    for language in TARGETS:
        if language == "rust":
            continue
        out = directory / f"counter-{language}"
        invoke(cli, ["synth", "run", spec, "--target", language, "--out", str(out)], directory, environment)
        check_vectors(out, "counter")
        records.append(invoke(cli, ["synth", "verify", spec, "--target", language, "--out", str(out)], directory, environment))
    if any(record["status"] != "passed" or record["inputs_checked"] != 64 for record in records):
        raise RuntimeError("target did not pass complete finite conformance")
    if len({record["certificate"] for record in records}) != 1 or len({record["stdout_sha256"] for record in records}) != 1:
        raise RuntimeError("language targets disagree on semantics or output")
    return {"status": "passed", "targets": records, "claim": "finite pure-step conformance; application authority is checked separately"}


def check_integer_boundaries(cli: list[str], directory: Path, environment: dict[str, str]) -> list[dict]:
    """The expected relation is an independent exact-integer identity/constant."""
    records = []
    for name, low, high, constant in [
        ("above-number-precision", 2**53 - 1, 2**53 + 1, False),
        ("i64-min", -(2**63), -(2**63) + 2, False),
        ("i64-max", 2**63 - 3, 2**63 - 1, False),
        ("zero-input", -(2**63), -(2**63), True),
    ]:
        field = {"name": "value", "type": {"kind": "int", "min": low, "max": high}}
        problem = {"schema": "zeno-fcis/synthesis-problem/1", "profile": "zeno-fcis/finite-i64/1",
                   "inputs": [] if constant else [field], "outputs": [field],
                   "contract": {"nodes": [["input", 0], ["int", low] if constant else ["input", 1], ["eq", 0, 1]], "roots": [2]},
                   "sketch": {"nodes": [{"hole": 1, "alternatives": [["int", 0 if constant else low],
                                                                                 ["int", low] if constant else ["input", 0]]}], "roots": [0]}}
        spec = directory / f"{name}.json"
        spec.write_text(json.dumps(problem) + "\n")
        expected = [{"input": [] if constant else [value], "output": [value]} for value in range(low, high + 1)]
        current = []
        for language in TARGETS:
            out = directory / f"{name}-{language}"
            arguments = [str(spec), "--target", language, "--out", str(out)]
            invoke(cli, ["synth", "run"] + arguments, directory, environment)
            cases = json.loads((out / "vectors.json").read_text())["cases"]
            if cases != expected:
                raise RuntimeError(f"{language} {name} lost integer precision or corpus coverage")
            record = invoke(cli, ["synth", "verify"] + arguments, directory, environment)
            if record["status"] != "passed" or record["inputs_checked"] != len(expected):
                raise RuntimeError(f"{language} {name} conformance failed")
            current.append(record)
        if len({record["certificate"] for record in current}) != 1 or len({record["stdout_sha256"] for record in current}) != 1:
            raise RuntimeError(f"{name} target results disagree")
        records.append({"case": name, "targets": current})
    return records


def check_node_environment(cli: list[str], app: Path, directory: Path, environment: dict[str, str]) -> None:
    marker = directory / "node-preload-executed"
    preload = directory / "node-preload.cjs"
    preload.write_text(f"require('node:fs').writeFileSync({json.dumps(str(marker))}, 'executed'); process.exit(94);\n")
    injected = {**environment, "NODE_OPTIONS": "--require=" + json.dumps(str(preload)),
                "NODE_PATH": str(directory / "untrusted-node-modules"),
                "NODE_V8_COVERAGE": str(directory / "unwanted-node-coverage"),
                "NODE_COMPILE_CACHE": str(directory / "unwanted-node-cache")}
    report = invoke(cli, ["synth", "verify", str(app / "synthesis.json"), "--target", "javascript",
                          "--out", str(directory / "counter-javascript")], directory, injected)
    if report["status"] != "passed" or any(path.exists() for path in [marker, directory / "unwanted-node-coverage", directory / "unwanted-node-cache"]):
        raise RuntimeError("inherited Node environment changed target execution or wrote external artifacts")
    # Prove the payload actually reaches Node without the runner's mediation.
    control = {key: value for key, value in environment.items() if not key.startswith("NODE_")}
    control["NODE_OPTIONS"] = injected["NODE_OPTIONS"]
    probe = subprocess.run([report["tool"]["path"], "--eval", "process.exit(95)"],
                           cwd=directory, env=control, capture_output=True, timeout=30)
    if probe.returncode != 94 or not marker.is_file() or marker.read_text() != "executed":
        raise RuntimeError("Node preload control did not exercise the intended failure")
    marker.unlink()


def check(cli: list[str], directory: Path, environment: dict[str, str]) -> dict:
    app = directory / "counter"
    # 'new' reports a human message; subsequent synthesis commands are structured.
    subprocess.run(cli + ["new", str(app), "--template", "durable-counter"], cwd=ROOT,
                   env=environment, check=True, stdout=subprocess.PIPE, timeout=120)
    counter = exercise_counter(cli, app, directory, environment)
    generic = []
    for language, extension in TARGETS.items():
        out = directory / f"generic-{language}"
        spec = str(ROOT / "test-data/synthesis/saturating-add.json")
        invoke(cli, ["synth", "run", spec, "--target", language, "--out", str(out)], directory, environment)
        check_vectors(out, "generic")
        generic.append(invoke(cli, ["synth", "verify", spec, "--target", language, "--out", str(out)], directory, environment))
        # A changed source cannot be blessed by editing its public hash record.
        source = out / f"transition.{extension}"
        source.write_bytes(source.read_bytes() + b"\n")
        manifest = json.loads((out / "manifest.json").read_text())
        for entry in manifest["files"]:
            if entry["path"] == source.name:
                entry["bytes"] = source.stat().st_size
                entry["sha256"] = hashlib.sha256(source.read_bytes()).hexdigest()
        (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
        rejected = invoke(cli, ["synth", "verify", spec, "--target", language, "--out", str(out)], directory, environment, expected=1)
        if rejected["status"] != "artifact-drift":
            raise RuntimeError("hostile source mutation escaped regeneration binding")
    if len({record["certificate"] for record in generic}) != 1 or len({record["stdout_sha256"] for record in generic}) != 1 or any(record["status"] != "passed" or record["inputs_checked"] != 16 for record in generic):
        raise RuntimeError("generic target evidence is inconsistent")
    integer_boundaries = check_integer_boundaries(cli, directory, environment)
    check_node_environment(cli, app, directory, environment)
    spec = str(app / "synthesis.json")
    for args, status, code in [
        (["--max-assignments", "1"], "incomplete", 2),
        (["--max-steps", "1"], "incomplete", 2),
        (["--target", "unregistered"], "unsupported-target", 2),
    ]:
        report = invoke(cli, ["synth", "run", spec, "--out", str(directory / "unwritten")] + args, directory, environment, expected=code)
        if report["status"] != status or (directory / "unwritten").exists():
            raise RuntimeError("incomplete or unsupported synthesis wrote a success artifact")
    misplaced = invoke(cli, ["synth", "verify", spec, "--out", str(app / "synthesized"),
                             "--receipt", str(app / "synthesized/conformance.json")], directory, environment, expected=1)
    if misplaced["status"] != "invalid-receipt-path" or (app / "synthesized/conformance.json").exists():
        raise RuntimeError("receipt polluted immutable artifacts")
    missing_problem = invoke(cli, ["synth", "run", str(directory / "missing.json"), "--out", str(directory / "unwritten")], directory, environment, expected=3)
    if missing_problem["status"] != "io-error":
        raise RuntimeError("filesystem failure was misclassified")
    missing = invoke(cli, ["synth", "verify", spec, "--out", str(app / "synthesized"),
                           "--tool", str(directory / "absent-compiler")], directory, environment, expected=2)
    if missing["status"] != "conformance-unknown":
        raise RuntimeError("missing compiler did not remain unknown")
    return {"schema": "zeno-fcis/synthesis-validation/1", "status": "passed",
            "counter": counter, "generic": generic, "integer_boundaries": integer_boundaries,
            "negative_cases": ["source-and-manifest-tampering", "assignment-budget", "work-budget", "unsupported-target", "missing-tool", "receipt-location", "missing-problem", "inherited-node-environment"]}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path)
    parser.add_argument("--receipt", type=Path)
    args = parser.parse_args()
    env = dict(os.environ)
    if args.cli:
        cli = [str(args.cli.resolve())]
    else:
        subprocess.run(["cargo", "+1.97.1", "build", "-p", "zeno-fcis-cli", "--locked", "--offline"], cwd=ROOT, env=env, check=True)
        cli = [str((ROOT / Path(env.get("CARGO_TARGET_DIR", ROOT / "target")) / "debug/zeno-fcis").resolve())]
    with tempfile.TemporaryDirectory(prefix="zeno-synthesis-check-") as temporary:
        receipt = check(cli, Path(temporary), env)
    if args.receipt:
        with args.receipt.open("x") as output:
            json.dump(receipt, output, indent=2)
            output.write("\n")
    print("finite synthesis: Rust, Python and JavaScript passed counter, generic and exact-integer boundary cases; hostile cases rejected")


if __name__ == "__main__":
    main()
