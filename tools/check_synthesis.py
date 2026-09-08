#!/usr/bin/env python3
"""Exercise shared synthesis, real Rust/Python replay, and hostile artifact cases."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


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
    python = directory / "counter-python"
    invoke(cli, ["synth", "run", spec, "--target", "python", "--out", str(python)], directory, environment)
    check_vectors(python, "counter")
    records.append(invoke(cli, ["synth", "verify", spec, "--target", "python", "--out", str(python)], directory, environment))
    if any(record["status"] != "passed" or record["inputs_checked"] != 64 for record in records):
        raise RuntimeError("target did not pass complete finite conformance")
    if records[0]["certificate"] != records[1]["certificate"] or records[0]["stdout_sha256"] != records[1]["stdout_sha256"]:
        raise RuntimeError("language targets disagree on semantics or output")
    return {"status": "passed", "targets": records, "claim": "finite pure-step conformance; application authority is checked separately"}


def check(cli: list[str], directory: Path, environment: dict[str, str]) -> dict:
    app = directory / "counter"
    # 'new' reports a human message; subsequent synthesis commands are structured.
    subprocess.run(cli + ["new", str(app), "--template", "durable-counter"], cwd=ROOT,
                   env=environment, check=True, stdout=subprocess.PIPE, timeout=120)
    counter = exercise_counter(cli, app, directory, environment)
    generic = []
    for language in ("rust", "python"):
        out = directory / f"generic-{language}"
        spec = str(ROOT / "fixtures/synthesis/saturating-add.json")
        invoke(cli, ["synth", "run", spec, "--target", language, "--out", str(out)], directory, environment)
        check_vectors(out, "generic")
        generic.append(invoke(cli, ["synth", "verify", spec, "--target", language, "--out", str(out)], directory, environment))
        # A changed source cannot be blessed by editing its public hash record.
        source = out / ("transition.rs" if language == "rust" else "transition.py")
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
    if generic[0]["certificate"] != generic[1]["certificate"] or generic[0]["inputs_checked"] != 16:
        raise RuntimeError("generic target evidence is inconsistent")
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
            "counter": counter, "generic": generic,
            "negative_cases": ["source-and-manifest-tampering", "assignment-budget", "work-budget", "unsupported-target", "missing-tool", "receipt-location", "missing-problem"]}


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
    print("finite synthesis: Rust and Python passed 64 counter and 16 generic inputs; hostile cases rejected")


if __name__ == "__main__":
    main()
