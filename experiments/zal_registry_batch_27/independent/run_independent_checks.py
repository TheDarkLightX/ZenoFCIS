"""Replay the focused checks without changing the pinned source roots."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

HERE = Path(__file__).resolve().parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline-root", required=True, type=Path)
    parser.add_argument("--candidate-root", required=True, type=Path)
    parser.add_argument("--recipe", required=True, type=Path)
    args = parser.parse_args()
    roots = {"baseline": args.baseline_root.resolve(), "candidate": args.candidate_root.resolve()}
    recipe = args.recipe.resolve()
    result_path = HERE / "checks-summary.json"
    if result_path.exists():
        raise RuntimeError("refusing to overwrite prior check summary")
    jobs = []
    bootstrap = ("import runpy,sys;sys.path.insert(0,sys.argv.pop(1));"
                 "sys.argv[0]='unittest';runpy.run_module('unittest',run_name='__main__')")
    for optimize in (False, True):
        mode = "optimized" if optimize else "normal"
        flags = ["-O"] if optimize else []
        for name in ("test_lens_library", "test_lens_integration", "test_library_lens_browser"):
            jobs.append(("existing-" + name + "-" + mode, optimize, 0,
                [sys.executable, *flags, "-I", "-B", "-c", bootstrap,
                 str(roots["candidate"] / "integrations/zal"), "zal_notation.tests." + name, "-v"], None))
        for role in roots:
            label = "boundaries-" + role + "-" + mode
            jobs.append((label, optimize, 0, [sys.executable, *flags, "-I", "-B", str(HERE / "test_catalog_boundaries.py"),
                "--root", str(roots[role]), "--record", str(HERE / (label + ".json"))], None))
        jobs.append(("namespace-ownership-" + mode, optimize, 0,
                     [sys.executable, *flags, "-I", "-B", str(HERE / "test_namespace_process.py")], None))
    for role, own, other in (("baseline", "baseline-01", "candidate-02"),
                             ("candidate", "candidate-02", "baseline-01")):
        label = "artifact-identity-" + role
        jobs.append((label, False, 0, [sys.executable, "-I", "-B", str(HERE / "check_artifact_identity.py"),
            "--root", str(roots[role]), "--own-packet", str(recipe / "measures" / own / "heldout-latch"),
            "--other-packet", str(recipe / "measures" / other / "heldout-latch"),
            "--output", str(HERE / (label + ".json"))], None))
        # These are diagnostic checks. An observed failure stays an unresolved
        # qualification failure, even when the expected exit reproduces it.
        jobs.append(("evaluator-identity-" + role, False, 1,
                     [sys.executable, "-B", str(roots[role] / "tools/check_authority_v2.py"), "--check-sources"], roots[role]))
    result = {"schema": "zal/independent-registry-checks/1", "checks": [], "pass": False,
              "qualification_failures": []}
    for label, optimize, expected, command, cwd in jobs:
        output = HERE / "checks" / label
        bounded = [sys.executable, "-I", "-B", str(HERE / "run_checked_command.py"), "--output", str(output)]
        if cwd is not None:
            bounded.extend(["--cwd", str(cwd)])
        env = os.environ.copy()
        env["PYTHONDONTWRITEBYTECODE"] = "1"
        env.pop("PYTHONPATH", None)
        if optimize:
            env["PYTHONOPTIMIZE"] = "1"
        else:
            env.pop("PYTHONOPTIMIZE", None)
        completed = subprocess.run([*bounded, "--", *command], env=env, capture_output=True, text=True)
        record = json.loads((output / "RESULT.json").read_text())
        match = record.get("exit") == expected and not record.get("cleanup", {}).get("survivors", ["unknown"])
        result["checks"].append({"label": label, "expected_exit": expected,
            "exit": record.get("exit"), "expected_outcome_reproduced": match,
            "bounded_runner_exit": completed.returncode,
            "log_sha256": hashlib.sha256((output / "output.log").read_bytes()).hexdigest()})
        if expected or not match:
            result["qualification_failures"].append(label)
        print(json.dumps(result["checks"][-1]), flush=True)
        result_path.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    result["pass"] = all(x["expected_outcome_reproduced"] for x in result["checks"])
    result["scope"] = "pass means expected diagnostic outcomes reproduced; qualification_failures remain failures"
    result_path.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    return 0 if result["pass"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
