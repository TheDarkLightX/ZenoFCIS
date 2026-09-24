#!/usr/bin/env python3
"""Exercise shared synthesis, real language replay, and hostile artifact cases."""
from __future__ import annotations

import argparse
import hashlib
import itertools
import json
import operator
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


def gateway_decision_table(rules: str) -> dict:
    """Evaluates a compliance-gateway rule base on every input.

    This is a separate evaluator of rules.txt: it shares nothing with the
    template's rules_to_synthesis.py, which writes the contract, or with its
    src/rules.rs, which the law checker and the tests use. The highest-priority
    matching rule decides, and a tie is a conflict, so the table is refused.
    """
    features, rules_read = [], []
    for line in rules.splitlines():
        words = line.split()
        if not words or words[0].startswith("#"):
            continue
        if words[0] == "feature":
            spec = words[2:]
            if len(spec) == 1 and ".." in spec[0]:
                low, high = (int(bound) for bound in spec[0].split(".."))
                features.append((words[1], list(range(low, high + 1)), None))
            else:
                features.append((words[1], list(range(len(spec))), spec))
        elif words[0] == "rule":
            then = words.index("then")
            conditions = []
            if words[5:then] != ["always"]:
                clause = words[5:then]
                for start in range(0, len(clause), 4):
                    feature, op, value = clause[start:start + 3]
                    position = [name for name, _, _ in features].index(feature)
                    names = features[position][2]
                    conditions.append((position, op, names.index(value) if names else int(value)))
            rules_read.append((int(words[3]), conditions, {"allow": 0, "hold": 1, "block": 2}[words[then + 1]]))
        else:
            raise RuntimeError(f"unknown rule base line: {line}")
    compare = {"==": operator.eq, "!=": operator.ne, "<": operator.lt, "<=": operator.le,
               ">": operator.gt, ">=": operator.ge}
    # A block adds one strike up to the top of the strikes range; the other
    # verdicts keep the strikes.
    strikes_at = [name for name, _, _ in features].index("strikes")
    cap = features[strikes_at][1][-1]
    expected = {}
    for inputs in itertools.product(*[values for _, values, _ in features]):
        matching = [(priority, index, verdict) for index, (priority, conditions, verdict) in enumerate(rules_read)
                    if all(compare[op](inputs[position], value) for position, op, value in conditions)]
        if not matching:
            raise RuntimeError(f"rule base leaves {inputs} uncovered")
        top = max(priority for priority, _, _ in matching)
        fired = [(index, verdict) for priority, index, verdict in matching if priority == top]
        if len(fired) != 1:
            raise RuntimeError(f"rule base conflict on {inputs}: rules {[index for index, _ in fired]}")
        (index, verdict), strikes = fired[0], inputs[strikes_at]
        expected[inputs] = [verdict, index, min(strikes + 1, cap) if verdict == 2 else strikes]
    return expected


def check_vectors(directory: Path, domain: str, expected: dict | None = None) -> None:
    """Independent mathematical oracle, without IR/interpreter helpers.

    `expected` is the complete decision table of a domain whose rules live in
    the application, such as the gateway's rule base; the other domains' tables
    are written here.
    """
    vectors = json.loads((directory / "vectors.json").read_text())["cases"]
    actual = {}
    for case in vectors:
        if any(type(value) is not int for value in case["input"] + case["output"]):
            raise RuntimeError("replay scalars must be exact integers")
        inputs = tuple(case["input"])
        if inputs in actual:
            raise RuntimeError("duplicate replay input")
        actual[inputs] = case["output"]
    if expected is None:
        expected = builtin_decision_table(domain)
    if actual != expected:
        raise RuntimeError(f"{domain} differs from independent complete decision table")


def builtin_decision_table(domain: str) -> dict:
    """The complete decision tables of the counter, inventory, and generic domains."""
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
        return expected
    if domain == "inventory":
        expected = {}
        for available in range(6):
            for reserved in range(6):
                for action in range(4):
                    for quantity in range(1, 4):
                        # Codes: 0 short available, 1 short reserved, 2 over
                        # capacity, 3 accept; then available, reserved, ship.
                        kept = [available, reserved, 0]
                        if action == 0:
                            out = ([0, *kept] if quantity > available else
                                   [2, *kept] if reserved + quantity > 5 else
                                   [3, available - quantity, reserved + quantity, 0])
                        elif action == 1:
                            out = ([1, *kept] if quantity > reserved else
                                   [2, *kept] if available + quantity > 5 else
                                   [3, available + quantity, reserved - quantity, 0])
                        elif action == 2:
                            out = ([1, *kept] if quantity > reserved else
                                   [3, available, reserved - quantity, 1])
                        else:
                            out = ([2, *kept] if available + quantity > 5 else
                                   [3, available + quantity, reserved, 0])
                        expected[available, reserved, action, quantity] = out
        return expected
    if domain == "withdrawal":
        expected = {}
        for pending_a, pending_b, pause, must_serve, priority_b, arriving_a, arriving_b, alarm in (
                (a, b, p, m, r, x, y, z) for a in range(2) for b in range(2) for p in range(3)
                for m in range(2) for r in range(2) for x in range(2) for y in range(2)
                for z in range(2)):
            # Rule 6 of the template's README, written from its words: codes
            # 0 wait, 1 pay lane A, 2 pay lane B; then the next pending flags,
            # pause, must-serve, and priority.
            due_a, due_b = pending_a or arriving_a, pending_b or arriving_b
            if pause > 0:
                # Paused: nothing is paid; the pause counts down, and ending
                # with a lane due starts must-serve.
                output, next_pause = 0, pause - 1
                next_serve = int(pause == 1 and (due_a or due_b))
            elif alarm and not must_serve:
                # An honored alarm pays nothing and pauses two more ticks.
                output, next_pause, next_serve = 0, 2, 0
            else:
                # The priority lane when both are due, the due lane when one
                # is, nothing when none is; must-serve ends.
                if due_a and due_b:
                    output = 2 if priority_b else 1
                else:
                    output = 1 if due_a else 2 if due_b else 0
                next_pause, next_serve = 0, 0
            # Priority moves to the other lane after a payout.
            next_priority = 1 if output == 1 else 0 if output == 2 else priority_b
            expected[pending_a, pending_b, pause, must_serve, priority_b,
                     arriving_a, arriving_b, alarm] = [
                output, int(due_a and output != 1), int(due_b and output != 2),
                next_pause, next_serve, next_priority]
        return expected
    return {(a, b): [min(a + b, 3)] for a in range(4) for b in range(4)}


def exercise_counter(cli: list[str], app: Path, directory: Path,
                     environment: dict[str, str]) -> dict:
    """Used by both source-consumer and actual packaged-consumer qualification."""
    return exercise_synthesized(cli, app, directory, environment, "counter", 64)


def exercise_inventory(cli: list[str], app: Path, directory: Path,
                       environment: dict[str, str]) -> dict:
    """The inventory-reservation example's synthesized stock step."""
    return exercise_synthesized(cli, app, directory, environment, "inventory", 432)


def exercise_gateway(cli: list[str], app: Path, directory: Path,
                     environment: dict[str, str]) -> dict:
    """The compliance-gateway example's synthesized screening step, against
    a separate evaluation of its rule base."""
    expected = gateway_decision_table((app / "rules.txt").read_text())
    if len(expected) != 720:
        raise RuntimeError("gateway rule base does not span its 720 inputs")
    return exercise_synthesized(cli, app, directory, environment, "gateway", 720, expected)


def exercise_withdrawal(cli: list[str], app: Path, directory: Path,
                        environment: dict[str, str]) -> dict:
    """The withdrawal-queue example's synthesized controller step."""
    return exercise_synthesized(cli, app, directory, environment, "withdrawal", 384)


def exercise_synthesized(cli: list[str], app: Path, directory: Path,
                         environment: dict[str, str], domain: str, inputs: int,
                         expected: dict | None = None) -> dict:
    """Checks an application's mounted synthesis and replays it in every target."""
    spec = str(app / "synthesis.json")
    rust = app / "synthesized"
    current = invoke(cli, ["synth", "run", spec, "--out", str(rust), "--check"], directory, environment)
    if current["status"] != "current":
        raise RuntimeError("mounted Rust artifact drifted")
    check_vectors(rust, domain, expected)
    records = [invoke(cli, ["synth", "verify", spec, "--out", str(rust)], directory, environment)]
    for language in TARGETS:
        if language == "rust":
            continue
        out = directory / f"{domain}-{language}"
        invoke(cli, ["synth", "run", spec, "--target", language, "--out", str(out)], directory, environment)
        check_vectors(out, domain, expected)
        records.append(invoke(cli, ["synth", "verify", spec, "--target", language, "--out", str(out)], directory, environment))
    if any(record["status"] != "passed" or record["inputs_checked"] != inputs for record in records):
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
