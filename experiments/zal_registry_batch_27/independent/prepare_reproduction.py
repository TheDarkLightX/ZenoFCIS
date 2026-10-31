"""Prepare fresh external evidence paths and a local descriptor for this study."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys

HERE = Path(__file__).resolve().parent
SCRIPTS = ("container_measurements.py", "namespace_process.py", "catalog_sizes.py", "test_catalog_boundaries.py",
           "run_checked_command.py", "test_namespace_process.py", "check_artifact_identity.py",
           "run_independent_checks.py", "check_initial_state.py", "test_measurement_pins.py")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline-root", required=True, type=Path)
    parser.add_argument("--candidate-root", required=True, type=Path)
    parser.add_argument("--python", required=True, type=Path, help="reviewed local interpreter path")
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    roots = {"baseline": args.baseline_root.resolve(), "candidate": args.candidate_root.resolve()}
    pins = {"baseline": "8de5a6543f93353d2f4563b17badfde285ce46a2",
            "candidate": "48358c98fabbd8bc03edc972c077aef945cd2f16"}
    output = args.output.resolve()
    if any(output == root or root in output.parents for root in roots.values()):
        parser.error("evidence must stay outside both pinned source roots")
    for role, root in roots.items():
        actual = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
        if actual != pins[role] or subprocess.check_output(["git", "status", "--porcelain"], cwd=root):
            raise RuntimeError("expected clean pinned " + role + " root")
    output.mkdir(parents=True, exist_ok=False)
    recipe = output / "published-recipe"
    recipe.mkdir()
    source = roots["candidate"] / "experiments/zal_registry_batch_27"
    for name in ("worker.py", "run_measurements.py", "owned_process.py", "FROZEN_PLAN_02.json", "pins.template.json"):
        shutil.copyfile(source / name, recipe / name)
    shutil.copytree(source / "workloads", recipe / "workloads")
    for name in SCRIPTS:
        shutil.copyfile(HERE / name, output / name)
    descriptor = json.loads((recipe / "pins.template.json").read_text())
    for role, root in roots.items():
        descriptor[role]["root"] = str(root)
    python = args.python.resolve()
    descriptor["python"] = {"path": str(python), "sha256": hashlib.sha256(python.read_bytes()).hexdigest(),
        "version": subprocess.check_output([str(python), "-I", "-B", "-c", "import sys;print(sys.version)"], text=True).strip()}
    raw = (json.dumps(descriptor, indent=2, sort_keys=True) + "\n").encode()
    (recipe / "MEASURE_PINS_01.json").write_bytes(raw)
    print(json.dumps({"output": str(output), "pins_sha256": hashlib.sha256(raw).hexdigest()}))


if __name__ == "__main__":
    main()
