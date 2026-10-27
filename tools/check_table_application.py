#!/usr/bin/env python3
"""Compare the synthesized fixed-table application with registered scenarios.

Uses the actual scaffold, operational CLI and SQLite session without changing
emitted artifacts. This is bounded regression evidence, not release authority.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tomllib

from cargo_source_scope import source_environment

ROOT = Path(__file__).resolve().parents[1]
WRAP = ["nice", "-n", "19", "ionice", "-c3"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    args.work.mkdir(parents=True, exist_ok=False)
    example = ROOT / "examples/contract-tables/library-loans"
    app = args.work / "library-loans"
    env = dict(os.environ, CARGO_BUILD_JOBS="4", CARGO_PROFILE_DEV_DEBUG="0",
               CARGO_PROFILE_TEST_DEBUG="0", CARGO_NET_OFFLINE="true", CARGO_INCREMENTAL="0")
    rows = []

    def run(label, command, expected=0):
        environment = source_environment(app, env) if "--manifest-path" in command else env
        p = subprocess.run(WRAP + list(map(str, command)), cwd=ROOT, env=environment,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)
        (args.out / (label + ".stdout")).write_bytes(p.stdout)
        (args.out / (label + ".stderr")).write_bytes(p.stderr)
        rows.append(dict(check=label, command=list(map(str, command)), exit_code=p.returncode,
                         stdout_sha256=hashlib.sha256(p.stdout).hexdigest()))
        (args.out / "checks.json").write_text(json.dumps(rows, indent=2) + "\n")
        assert p.returncode == expected, f"{label}: exit {p.returncode}, expected {expected}"
        print(f"{label}: exit {p.returncode}", flush=True)
        return p.stdout

    packet = json.loads(run("registered-scenarios", [args.cli.resolve(), "contract", "scenario", example]))
    measured = packet
    assert measured["failed_on_contract"] == 0, measured
    run("scaffold", [args.cli.resolve(), "new", app, "--contract", example, "--source", ROOT])
    cargo = ["cargo", "+1.97.1"]
    run("consumer-tests", cargo + ["test", "--locked", "--offline", "--manifest-path", app / "Cargo.toml"])
    run("consumer-build", cargo + ["build", "--locked", "--offline", "--manifest-path", app / "Cargo.toml"])
    name = tomllib.loads((app / "Cargo.toml").read_text())["package"]["name"]
    binary = Path(env["CARGO_TARGET_DIR"]) / "debug" / name
    scenario = None
    results = []
    for number, line in enumerate((example / "tests/decision-scenarios.txt").read_text().splitlines(), 1):
        if line.startswith("# scenario "):
            scenario = line.removeprefix("# scenario ")
            store = args.work / (scenario + ".sqlite")
            run(f"{number}-init", [binary, "init", store, "--format", "json"])
        elif line.strip() and not line.lstrip().startswith("#"):
            assert scenario, f"unregistered step {number}"
            pre, command, context, expected, deliveries = (part.strip() for part in line.split("|"))
            assert deliveries == "-", "this corpus has no deliveries"
            state = json.loads(run(f"{number}-pre", [binary, "state", store, "--format", "json"]))["state"]
            before = [int(v) for v in pre.split()]
            assert [state[f"copies_{i}"] for i in range(6)] == before, (number, state, before)
            action, copy, member = command.split()
            words = [f"action={action}", f"copy={copy}", f"member={member}", f"context.context={context}"]
            fields = expected.split()
            wanted_class, reason = fields[:2]
            wanted = dict(class_=wanted_class.capitalize(), reason=None if reason == "-" else int(reason),
                          state=[int(v) for v in fields[2:]])
            code = 2 if wanted_class == "reject" else 0
            decision = json.loads(run(f"{number}-submit", [binary, "submit", store, *words, "--format", "json"], code))
            assert decision["class"] == wanted["class_"], (number, decision, wanted)
            assert decision["reason"] == wanted["reason"], (number, decision, wanted)
            assert [decision["state"][f"copies_{i}"] for i in range(6)] == wanted["state"], (number, decision, wanted)
            assert decision["deliveries"] == [], (number, decision)
            current = json.loads(run(f"{number}-post", [binary, "state", store, "--format", "json"]))
            assert [current["state"][f"copies_{i}"] for i in range(6)] == wanted["state"], (number, current)
            results.append(dict(scenario=scenario, line=number, decision=decision))
    assert len(results) == sum(s["steps"] for s in measured["scenarios"]), "missing registered steps"
    assert len({r["scenario"] for r in results}) == measured["count"], "missing registered scenarios"
    (args.out / "decisions.json").write_text(json.dumps(results, indent=2) + "\n")
    print(f"table application: passed ({measured['count']} scenarios, {len(results)} persistent decisions)")


if __name__ == "__main__":
    main()
