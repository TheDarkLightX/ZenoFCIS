#!/usr/bin/env python3
"""Named VeriISLE check of exact LLVM overflow predicate against *unchanged*
upstream verifier specifications. Strictly distinguish pass, missing
specification, unknown, and refutation. Never claim full target-ISA proof.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import re
import shutil
import subprocess
from pathlib import Path
from install_isle_candidate import UPSTREAM, SOURCE, ORIGINAL_GIT_BLOB
from install_rust_overflow_predicate_28 import RULE, RULE_NAME, source, sha

COUNTERS = ("Total expansions", "In scope expansions", "Expansion errors",
            "Type instantiations", "Applicable", "Verification passed",
            "Verification failed", "Verification unknown")


def cmd(*args: str) -> str:
    return subprocess.check_output(args, text=True).strip()


def validate(upstream: Path) -> dict:
    if cmd("git","-C",str(upstream),"rev-parse","HEAD") != UPSTREAM:
        raise ValueError("upstream commit differs")
    if cmd("git","-C",str(upstream),"rev-parse","HEAD:" + SOURCE) != ORIGINAL_GIT_BLOB:
        raise ValueError("upstream original ISLE file blob differs")
    changed = cmd("git","-C",str(upstream),"diff","--name-only").splitlines()
    if changed != [SOURCE] or (upstream / SOURCE).read_text() != source(upstream,"xorand"):
        raise ValueError("expected exactly the experimental rule with no extra tracked modifications")
    identity = {}
    for exe in ("cargo","rustc","z3","cvc5"):
        path = shutil.which(exe)
        if path is None:
            raise ValueError("missing external verifier tool: " + exe)
        identity[exe] = {"path": path,
                         "version": subprocess.run([path,"--version"],
                          capture_output=True,text=True,check=True).stdout.strip()[:300]}
    return {"upstream_commit": UPSTREAM,"source_path": SOURCE,
            "original_file_blob": ORIGINAL_GIT_BLOB,
            "patch_sha256": sha((upstream / SOURCE).read_bytes()),
            "rule_sha256": sha(RULE.encode()),
            "tools": identity}


def classify(stdout: str, exitcode: int) -> tuple[str,dict]:
    if len(re.findall(r"(?m)^only rule:\s*" + re.escape(RULE_NAME) + r"\s*$",stdout)) != 1:
        raise ValueError("targeted rule name absent or duplicated in VeriISLE output")
    if stdout.count("Verification summary") != 1:
        raise ValueError("missing/duplicate verifier summary")
    counts = {}
    for title in COUNTERS:
        values = re.findall(r"(?m)^" + re.escape(title) + r":\s*(\d+)\s*$", stdout)
        if len(values) != 1:
            raise ValueError("ambiguous verifier counter: " + title)
        counts[title.lower().replace(" ","_")] = int(values[0])
    if counts["total_expansions"] < 1 or counts["in_scope_expansions"] < 1:
        raise ValueError("no meaningful rule analysis")
    if counts["verification_failed"] > 0:
        return "counterexample-or-verifier-failure-not-qualified", counts
    if counts["verification_unknown"] > 0:
        return "unknown-no-proof", counts
    if counts["expansion_errors"] > 0 or counts["applicable"] == 0:
        return "unsupported-or-zero-applicable-no-proof", counts
    if (exitcode == 0 and counts["applicable"] >= 1 and
        counts["verification_passed"] == counts["applicable"] and
        counts["type_instantiations"] >= 1):
        return "proved-relative-to-unchanged-upstream-veriisle-specifications", counts
    return "incomplete-or-nonzero-verifier-no-proof", counts


def qualify(upstream: Path, output: Path) -> dict:
    if output.exists():
        raise ValueError("cannot overwrite previous receipt")
    pins = validate(upstream)
    command = ["cargo", "run", "--locked", "--manifest-path",
               str(upstream / "Cargo.toml"),
               "-p", "cranelift-isle-veri", "--bin", "veri", "--",
               "--name", "opt", "--rule", RULE_NAME, "--num-threads", "2",
               "--timeout", "60"]
    run = subprocess.run(command,cwd=upstream,capture_output=True,text=True,timeout=1200)
    output.parent.mkdir(parents=True, exist_ok=True)
    stdout_path = output.with_suffix(".stdout.txt")
    stderr_path = output.with_suffix(".stderr.txt")
    if stdout_path.exists() or stderr_path.exists():
        raise ValueError("refusing to overwrite existing raw verifier output")
    stdout_path.write_text(run.stdout)
    stderr_path.write_text(run.stderr)
    try:
        verdict, counts = classify(run.stdout, run.returncode)
    except ValueError as exc:
        verdict, counts = "unparseable-or-incomplete-verifier-no-proof", {}
        parser_error = str(exc)
    else:
        parser_error = None
    result = {
        "schema": "zeno-fcis/tier1-llvm-overflow-unmodified-upstream-veriisle/1",
        "status": verdict, "counts": counts, "exit_code": run.returncode,
        "parse_error": parser_error,
        "source": pins, "command": command,
        "stdout_sha256": sha(run.stdout.encode()),
        "stderr_sha256": sha(run.stderr.encode()),
        "additional_specification_changes": "none",
        "nonclaims": ["not universal x86/ARM ISA flags/ABI proof",
                      "not a correctness proof if unsupported, unknown or failed",
                      "not a native cost or runtime speed result",
                      "named-rule model proof covers only admitted total IR cases"],
    }
    output.write_text(json.dumps(result,sort_keys=True,indent=2)+"\n")
    return result


def main():
    p=argparse.ArgumentParser()
    p.add_argument("--upstream",type=Path,required=True)
    p.add_argument("--out",type=Path,required=True)
    a=p.parse_args()
    result=qualify(a.upstream.resolve(),a.out.resolve())
    print(json.dumps({"status":result["status"],"counts":result["counts"],
                      "exit_code":result["exit_code"]},sort_keys=True))
    if result["status"] not in (
        "proved-relative-to-unchanged-upstream-veriisle-specifications",
        "unsupported-or-zero-applicable-no-proof"):
        raise SystemExit("VeriISLE not qualified: " + result["status"])


if __name__=="__main__":
    main()
