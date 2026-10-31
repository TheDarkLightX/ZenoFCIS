"""Supplement the unchanged host-only supervisor on a Linux container.

This is a different supervisor, not a successful run of run_measurements.py.
It keeps the published worker, pins, schedule, caps and pidfd cleanup intact.
Run from outside both source roots, with a reviewed descriptor SHA-256.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def need(condition, message):
    if not condition:
        raise RuntimeError(message)


def stable(recipe, pins, expected_descriptor):
    need(sha(recipe / "MEASURE_PINS_01.json") == expected_descriptor, "descriptor changed")
    for role in ("baseline", "candidate"):
        root = Path(pins[role]["root"])
        for name, digest in pins[role]["files"].items():
            need(sha(root / name) == digest, role + " source changed: " + name)
        need(not subprocess.check_output(["git", "status", "--porcelain"], cwd=root),
             role + " source root is dirty")
    for name, digest in pins["helpers"].items():
        need(sha(recipe / name) == digest, "helper changed: " + name)
    for name, digest in pins["workloads"].items():
        need(sha(recipe / "workloads" / name) == digest, "workload changed: " + name)
    need(sha(pins["python"]["path"]) == pins["python"]["sha256"], "interpreter changed")


def compare(reference, worker, role):
    for key in ("catalog_sha256", "fallback_sha256", "public_results_sha256", "catalog_encoded_bytes"):
        need(reference[key] == worker[key], "differential mismatch: " + key)
    for name, data in worker["workloads"].items():
        old = reference["workloads"][name]
        for key in ("semantic_projection_sha256", "request_sha256", "files", "artifact_bytes", "view_statuses"):
            need(old[key] == data[key], "workload mismatch: " + name + " " + key)
        need(data["replay_status"] == "replayed", "fresh replay failed")
        oldpins, newpins = old["producer_source_pins"], data["producer_source_pins"]
        need(oldpins.keys() == newpins.keys(), "producer pin set differs")
        differences = [n for n in oldpins if oldpins[n] != newpins[n]]
        need(differences == ([] if role == "baseline" else ["zal_notation/lens_library.py"]),
             "unexpected producer identity difference")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--recipe", required=True, type=Path)
    parser.add_argument("--pins-sha256", required=True)
    args = parser.parse_args()
    recipe = args.recipe.resolve()
    need(sha(recipe / "MEASURE_PINS_01.json") == args.pins_sha256, "reviewed descriptor differs")
    pins = json.loads((recipe / "MEASURE_PINS_01.json").read_text())
    stable(recipe, pins, args.pins_sha256)
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from namespace_process import Owned, wait_owned

    result_path = recipe.parent / "container-measurements.json"
    need(not result_path.exists(), "refusing to overwrite results")
    result = {"schema": "zal/independent-container-measurements/1", "pass": False,
              "original_host_protocol": "blocked: PID 1 is not systemd",
              "pid1": Path("/proc/1/comm").read_text().strip(),
              "pins_sha256": args.pins_sha256, "runner_sha256": sha(__file__),
              "ownership_helper_sha256": sha(Path(__file__).with_name("namespace_process.py")),
              "source_commits": {role: subprocess.check_output(
                  ["git", "rev-parse", "HEAD"], cwd=pins[role]["root"], text=True).strip()
                  for role in ("baseline", "candidate")}, "workers": []}
    owner = None
    try:
        reference = None
        for index, role in enumerate(("baseline", "candidate", "candidate", "baseline", "baseline", "candidate"), 1):
            stable(recipe, pins, args.pins_sha256)
            case = role + "-" + str(index).zfill(2)
            command = [pins["python"]["path"], "-I", "-B", str(recipe / "worker.py"),
                       "--bounded", "--root", pins[role]["root"], "--case", case]
            start = time.monotonic()
            env = os.environ.copy()
            env["PYTHONDONTWRITEBYTECODE"] = "1"
            env.pop("PYTHONOPTIMIZE", None)
            row = {"case": case, "backend": role, "command": command}
            result["workers"].append(row)
            with (recipe.parent / (case + ".log")).open("xb") as log:
                process = subprocess.Popen(command, cwd=recipe, env=env, stdout=log,
                                           stderr=subprocess.STDOUT, start_new_session=True)
                owner = Owned(process)
                try:
                    row["exit"] = wait_owned(process, owner, 120)
                finally:
                    row["cleanup"] = owner.finish()
                    owner = None
                    row["wall_with_startup_seconds"] = time.monotonic() - start
            stable(recipe, pins, args.pins_sha256)
            worker_path = recipe / "measures" / case / "RESULT.json"
            worker = json.loads(worker_path.read_text())
            row["result_sha256"] = sha(worker_path)
            need(row["exit"] == 0 and worker["pass"] and not row["cleanup"]["survivors"],
                 "worker incomplete: " + case)
            if reference is None:
                reference = worker
                need(all(x["status"] == "refused" for x in json.loads(
                    (worker_path.parent / "public-results.json").read_text())["refusals"]),
                    "negative registry case unexpectedly accepted")
            else:
                compare(reference, worker, role)
            print(json.dumps({"completed": case, "seconds": row["wall_with_startup_seconds"]}), flush=True)
            result_path.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
        result["pass"] = True
        result["source_inputs_stable"] = True
    except BaseException as error:
        result["failure"] = {"type": type(error).__name__, "message": str(error)}
    finally:
        if owner is not None:
            result["interrupted_cleanup"] = owner.finish()
        result_path.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps({k: v for k, v in result.items() if k != "workers"}), flush=True)
    return 0 if result["pass"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
