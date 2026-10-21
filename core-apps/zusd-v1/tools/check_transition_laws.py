#!/usr/bin/env python3
"""Check shared accounting primitives and the separate aggregate induction proof."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "tools"))
import check_verus as qualified

HARNESS = Path("verification/verus/zusd_transition_laws.rs")
SUBJECT = Path("core-apps/zusd-v1/src/accounting.rs")
PIN = Path("verification/verus/toolchain.json")
TARGETS = ("row_sum_bound", "ledger_sum_fits_wide", "row_replacement", "transfer_conserves", "issuance_conserves",
           "funding_is_exact", "redistribution_conserves", "supply_debt_step",
           "checked_debt_admission", "checked_balance_step", "one_step_conserves",
           "finite_trace_conserves")
PREFIX = ["nice", "-n", "19", "ionice", "-c3"]


def source_hashes():
    paths = set((ROOT / "core-apps/zusd-v1/src").rglob("*.rs"))
    paths.update(ROOT / p for p in (HARNESS, PIN, Path(__file__).resolve().relative_to(ROOT),
        Path("tools/check_verus.py"), Path("core-apps/zusd-v1/tests/conservation.rs")))
    return {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted(paths)}


def run(command, directory, environment):
    return qualified.run(PREFIX + list(map(str, command)), directory, environment)


def check(cache: Path, output: Path, git_command: str):
    before = source_hashes()
    pin = json.loads((ROOT / PIN).read_text())
    tools = qualified.prepare_tools(cache, pin, False)
    env = {k: v for k, v in os.environ.items()
           if not k.startswith(("VERUS_", "VARGO_"))
           and k not in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")}
    env.update(RUSTUP_TOOLCHAIN=pin["rust_toolchain"], VERUS_Z3_PATH=str(tools / "z3"))
    command = [tools / "verus", "--crate-type=lib", "--edition=2024", "--no-cheating",
               "--output-json", HARNESS]
    result = run(command, ROOT, env)
    (output / "positive.stdout.json").write_text(result.stdout)
    (output / "positive.stderr.txt").write_text(result.stderr)
    qualified.require_success(result)
    report = json.loads(result.stdout)
    names = {"zusd_transition_laws::" + name for name in TARGETS}
    names.update("zusd_transition_laws::accounting::" + name for name in
                 ("debt_matches", "movement_allowed", "balance_change"))
    if (not qualified.proof_succeeded(report, pin)
            or report["verification-results"]["verified"] != 16
            or not names.issubset(report["func-details"])):
        raise RuntimeError("pinned positive proof inventory did not pass")
    text = (ROOT / SUBJECT).read_text()
    if re.search(r"\b(assume|admit|axiom|external_body|sorry|unimplemented)\b", text
                 + (ROOT / HARNESS).read_text()):
        raise RuntimeError("proof placeholder or trusted-body escape")
    mutations = {
        "omit_default_debt": ("active.checked_add(default) == Some(supply)",
                              "active == supply", "debt_matches"),
        "permit_external_issuance": ("!((!has_from || !has_to) && !is_zusd)",
                                    "true", "movement_allowed"),
        "reverse_credit": ("old.checked_add(amount)", "old.checked_sub(amount)",
                           "balance_change"),
    }
    controls = []
    with tempfile.TemporaryDirectory(prefix="zusd-accounting-") as scratch:
        scratch = Path(scratch)
        for name, (old, replacement, target) in mutations.items():
            if text.count(old) != 1:
                raise RuntimeError("control source anchor changed: " + name)
            changed = text.replace(old, replacement)
            specimen = scratch / name
            (specimen / SUBJECT).parent.mkdir(parents=True)
            (specimen / HARNESS).parent.mkdir(parents=True)
            (specimen / SUBJECT).write_text(changed)
            (specimen / HARNESS).write_bytes((ROOT / HARNESS).read_bytes())
            build = run(["rustc", "+1.97.1", "--edition=2024", "--crate-type=lib",
                         "--crate-name", "accounting_control", SUBJECT,
                         "-o", specimen / "control.rlib"], specimen, env)
            qualified.require_success(build)
            mutant = run(command, specimen, env)
            mutant_report = json.loads(mutant.stdout)
            targets = ["zusd_transition_laws::accounting::" + target]
            killed = qualified.semantic_refusal(mutant, mutant_report, pin, SUBJECT, specimen,
                region=qualified.function_lines(changed, target), target_functions=targets)
            (output / (name + ".stdout.json")).write_text(mutant.stdout)
            (output / (name + ".stderr.txt")).write_text(mutant.stderr)
            controls.append({"name": name, "built": build.returncode == 0,
                             "killed": killed, "exit_code": mutant.returncode,
                             "target": targets[0]})
            if not killed:
                raise RuntimeError("control survived or failed outside its contract: " + name)
    versions = {}
    for name, cmd in {"verus": [tools / "verus", "--version"],
                      "bundled_z3": [tools / "z3", "--version"],
                      "verifier_rust": ["rustc", "+" + pin["rust_toolchain"], "--version"],
                      "runtime_rust": ["rustc", "+1.97.1", "--version"]}.items():
        r = run(cmd, ROOT, env)
        qualified.require_success(r)
        versions[name] = r.stdout.strip()
    qualified.verify_tool_files(tools, pin)
    if before != source_hashes():
        raise RuntimeError("source changed during proof check")
    rev = run([git_command, "rev-parse", "HEAD"], ROOT, env)
    qualified.require_success(rev)
    return {"schema": "zusd-accounting-proof/1", "status": "passed",
            "commit": rev.stdout.strip(), "source_sha256": before, "versions": versions,
            "toolchain_pin": pin, "command": list(map(str, command)),
            "verified": 16, "theorems": sorted(names), "controls": controls,
            "placeholder_scan": "passed", "proof_report": report,
            "scope": ["source-shared u128 debt admission, balance arithmetic and movement shape",
                      "exact integer row sums, transfer, issuance, funding, debt redistribution",
                      "one-step aggregate transition and arbitrary finite trace induction"],
            "premises": ["canonical ledger rows represent each wallet/custody asset once",
                         "recorded active/default debt includes reserves and undistributed debt",
                         "accepted arithmetic fits persisted u128 fields",
                         "external funding is authenticated by the bound receipt verifier"],
            "open_refinement": ["Vec insertion and traversal implement exact abstract row sums",
                                "Wide aggregate arithmetic implements mathematical integers",
                                "every application command refines the aggregate successor",
                                "claim liabilities never exceed custody for every accepted state",
                                "receipt backend correctness and atomic external custody publication"],
            "nonclaim": "The full application conservation proof remains incomplete."}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--cache", type=Path, default=qualified.cache_directory())
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--git-command", default="git")
    a = p.parse_args()
    a.out.mkdir(parents=True, exist_ok=True)
    try:
        receipt = check(a.cache, a.out, a.git_command)
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        receipt = {"schema": "zusd-accounting-proof/1", "status": "failed", "error": str(error)}
    (a.out / "receipt.json").write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    print("Accounting proof:", receipt["status"])
    if receipt["status"] != "passed":
        print(receipt["error"])
        return 1
    print("16 obligations; 3 semantic controls; full application refinement remains open")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
