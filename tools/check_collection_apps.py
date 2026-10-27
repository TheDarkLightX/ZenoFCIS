#!/usr/bin/env python3
"""Exercise emitted collection applications with locked offline consumers.

HTTP checks use the actual ingress parser on caller-owned local byte streams.
This is scoped application evidence, not release or deployment authority.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

from cargo_source_scope import source_environment

ROOT = Path(__file__).resolve().parents[1]
WRAP = ["nice", "-n", "19", "ionice", "-c3"]


def capacity_project(destination, capacity):
    """Copy the public token input with a small declared capacity for regression."""
    shutil.copytree(ROOT / "examples/collections/token-balances", destination)
    path = destination / "project.zeno"
    text = path.read_text()
    assert text.count("capacity 1048576;") == 1
    path.write_text(text.replace("capacity 1048576;", f"capacity {capacity};"))
    path = destination / "tests/collection-journey.json"
    data = json.loads(path.read_text())
    data["capacity_regression"] = capacity
    path.write_text(json.dumps(data, indent=2) + "\n")
    return destination


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--strict", action="store_true")
    parser.add_argument("--only-capacity", action="store_true")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    args.work.mkdir(parents=True, exist_ok=False)
    env = dict(os.environ, CARGO_BUILD_JOBS=os.environ.get("CARGO_BUILD_JOBS", "4"), CARGO_PROFILE_DEV_DEBUG="0",
               CARGO_PROFILE_TEST_DEBUG="0", CARGO_NET_OFFLINE="true", CARGO_INCREMENTAL="0")
    rows = []
    costs = []

    def run(label, command):
        target = Path(env["CARGO_TARGET_DIR"])
        size = sum(p.stat().st_size for p in target.rglob("*") if p.is_file())
        if size >= 5_000_000_000:
            raise RuntimeError("collection target reached 5 GB")
        start = time.monotonic()
        log = args.out / (label + ".log")
        environment = env
        if "--manifest-path" in command:
            app = Path(command[command.index("--manifest-path") + 1]).resolve().parent
            environment = source_environment(app, env)
        with log.open("w") as output:
            result = subprocess.run(WRAP + list(map(str, command)), cwd=ROOT, env=environment,
                                    stdout=output, stderr=subprocess.STDOUT, check=False)
        rows.append({"check": label, "command": list(map(str, command)),
                     "exit_code": result.returncode, "seconds": time.monotonic() - start,
                     "log_sha256": hashlib.sha256(log.read_bytes()).hexdigest()})
        (args.out / "checks.json").write_text(json.dumps(rows, indent=2) + "\n")
        print(f"{label}: exit {result.returncode}", flush=True)
        if result.returncode:
            raise RuntimeError(f"{label}: inspect {log}")
        for line in log.read_text().splitlines():
            if "collection_request_cost=" in line:
                costs.append({"application": label.removesuffix("-tests"),
                              **json.loads(line.split("collection_request_cost=", 1)[1])})
        (args.out / "costs.json").write_text(json.dumps(costs, indent=2) + "\n")

    for example, app_flag in [("token-balances", False), ("library", True)]:
        if args.only_capacity:
            break
        app = args.work / example
        run(example + "-scaffold", [args.cli.resolve(), "new", app, "--contract",
            ROOT / "examples/collections" / example, "--source", ROOT, *(["--app"] if app_flag else [])])
        run(example + "-tests", ["cargo", "+1.97.1", "test", "--locked", "--offline",
            "--manifest-path", app / "Cargo.toml", "--", "--nocapture"])
        if args.strict:
            run(example + "-clippy", ["cargo", "+1.97.1", "clippy", "--locked", "--offline",
                "--manifest-path", app / "Cargo.toml", "--all-targets", "--all-features", "--", "-D", "warnings"])
    for capacity in [1, 2]:
        project = capacity_project(args.work / "test-projects" / f"token-capacity-{capacity}", capacity)
        label = f"capacity-{capacity}"
        run(label + "-checker", [args.cli.resolve(), "contract", "collections", project])
        packet, _ = json.JSONDecoder().raw_decode((args.out / (label + "-checker.log")).read_text())
        assert packet["status"] == "checked" and packet["maps"][0]["capacity"] == str(capacity)
        assert "declared capacities" in packet["premises"][3]
        app = args.work / label
        run(label + "-scaffold", [args.cli.resolve(), "new", app, "--contract", project,
                                  "--source", ROOT])
        run(label + "-tests", ["cargo", "+1.97.1", "test", "--locked", "--offline",
            "--manifest-path", app / "Cargo.toml", "--test", "collection_journey", "--",
            "--exact", "full_map_capacity_refuses_without_publication", "--nocapture"])
    deletion = args.work / "canonical-deletion"
    run("deletion-scaffold", [args.cli.resolve(), "new", deletion, "--contract",
        ROOT / "test-data/collections/deletion", "--source", ROOT])
    run("deletion-test", ["cargo", "+1.97.1", "test", "--locked", "--offline",
        "--manifest-path", deletion / "Cargo.toml", "--test", "collection_journey",
        "canonical_deletion_is_refused_without_publication", "--", "--exact", "--nocapture"])
    lawful_project = args.work / "test-projects" / "lawful-deletion"
    shutil.copytree(ROOT / "test-data/collections/deletion", lawful_project)
    path = lawful_project / "project.zeno"
    path.write_text("\n".join(line for line in path.read_text().splitlines()
        if not line.startswith("law 500")) + "\n")
    path = lawful_project / "v2/policy.json"
    rules = json.loads(path.read_text())
    del rules["law_kinds"]["500"]
    path.write_text(json.dumps(rules, indent=2) + "\n")
    path = lawful_project / "tests/collection-journey.json"
    path.write_text(json.dumps({"lawful_deletion_regression": True, "contexts": [["180"]]}))
    lawful = args.work / "lawful-deletion"
    run("lawful-deletion-scaffold", [args.cli.resolve(), "new", lawful, "--contract",
        lawful_project, "--source", ROOT])
    run("lawful-deletion-test", ["cargo", "+1.97.1", "test", "--locked", "--offline",
        "--manifest-path", lawful / "Cargo.toml", "--test", "collection_journey",
        "lawful_canonical_deletion_publishes_and_replays", "--", "--exact", "--nocapture"])
    print("collection applications: passed (HTTP byte streams; no live socket claim)")


if __name__ == "__main__":
    main()
