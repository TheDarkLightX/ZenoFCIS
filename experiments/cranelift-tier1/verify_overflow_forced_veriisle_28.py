#!/usr/bin/env python3
"""Source-pinned named VeriISLE for *forced* correct overflow rewrite.

Unchanged upstream specification set; no locally authored select model.
A clean named-rule result does not itself establish target performance.
"""
from __future__ import annotations
import argparse
import json
import re
import shutil
import subprocess
from pathlib import Path

from install_isle_candidate import UPSTREAM, SOURCE, ORIGINAL_GIT_BLOB
from install_overflow_forced_28 import FORCED_RULE, NEW_NAME, digest, source

FIELDS = ("Total expansions", "In scope expansions", "Expansion errors",
          "Type instantiations", "Applicable", "Verification passed",
          "Verification failed", "Verification unknown")


def qualify(upstream: Path, output: Path) -> dict:
    if output.exists():
        raise ValueError("will not overwrite verifier receipt")
    def git(*args):
        return subprocess.check_output(["git", "-C", str(upstream), *args],
                                       text=True).strip()
    if (git("rev-parse", "HEAD") != UPSTREAM
            or git("rev-parse", "HEAD:" + SOURCE) != ORIGINAL_GIT_BLOB
            or git("diff", "--name-only").splitlines() != [SOURCE]
            or (upstream / SOURCE).read_text() != source(upstream, "forced")):
        raise ValueError("upstream checker source/ref deviates from one exact rule")
    tools = {}
    for program in ("cargo", "rustc", "cvc5", "z3"):
        executable = shutil.which(program)
        if executable is None:
            raise ValueError("missing verifier: " + program)
        tools[program] = subprocess.check_output(
            [executable, "--version"], text=True).strip()[:200]
    command = ["cargo", "run", "--locked", "--manifest-path",
               str(upstream / "Cargo.toml"), "-p", "cranelift-isle-veri",
               "--bin", "veri", "--", "--name", "opt", "--rule", NEW_NAME,
               "--num-threads", "2", "--timeout", "60"]
    run = subprocess.run(command, cwd=upstream, text=True,
                         capture_output=True, timeout=1200)
    output.parent.mkdir(parents=True, exist_ok=True)
    stdout = output.with_suffix(".stdout.txt")
    stderr = output.with_suffix(".stderr.txt")
    if stdout.exists() or stderr.exists():
        raise ValueError("raw verifier log paths already exist")
    stdout.write_text(run.stdout)
    stderr.write_text(run.stderr)
    result = {}
    if len(re.findall(r"(?m)^only rule:\s*" + re.escape(NEW_NAME) + r"\s*$",
                      run.stdout)) != 1 or run.stdout.count("Verification summary") != 1:
        status = "no-complete-verifier-summary"
    else:
        for label in FIELDS:
            hits = re.findall(r"(?m)^" + re.escape(label) + r":\s*(\d+)\s*$",
                              run.stdout)
            if len(hits) != 1:
                raise ValueError("ambiguous verifier count " + label)
            result[label.lower().replace(" ", "_")] = int(hits[0])
        if result["verification_failed"] > 0:
            status = "counterexample-not-qualified"
        elif result["verification_unknown"] > 0:
            status = "unknown-not-qualified"
        elif result["expansion_errors"] or not result["applicable"]:
            status = "unsupported-or-zero-applicable"
        elif (run.returncode == 0 and result["verification_passed"] ==
              result["applicable"] and result["type_instantiations"] >= 1):
            status = "unchanged-upstream-spec-named-rule-passed"
        else:
            status = "not-qualified"
    receipt = {
        "schema": "zeno-fcis/tier1-overflow-forced-veriisle/1", "status": status,
        "counts": result, "returncode": run.returncode, "command": command,
        "source": {"upstream_commit": UPSTREAM, "original_git_blob": ORIGINAL_GIT_BLOB,
                   "only_modified_file": SOURCE,
                   "source_sha256": digest((upstream / SOURCE).read_bytes()),
                   "forced_rule_sha256": digest(FORCED_RULE.encode()),
                   "tool_versions": tools},
        "stdout_sha256": digest(run.stdout.encode()),
        "stderr_sha256": digest(run.stderr.encode()),
        "no_local_specification_addenda": True,
        "limits": ["named-rule admitted IR proof, not general x86 flags/ABI proof",
                   "no target native-cost or throughput result"],
    }
    output.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    return receipt


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--upstream", required=True, type=Path)
    p.add_argument("--out", required=True, type=Path)
    args = p.parse_args()
    report = qualify(args.upstream.resolve(), args.out.resolve())
    print(json.dumps({"status": report["status"], "counts": report["counts"]}))
    if report["status"] != "unchanged-upstream-spec-named-rule-passed":
        raise SystemExit("forced named rule not proved: " + report["status"])


if __name__ == "__main__":
    main()
