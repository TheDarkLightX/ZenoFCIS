#!/usr/bin/env python3
"""Replay the specification-parser proof kernels on their actual runtime bodies.

The receipt records scoped Verus evidence, native tests and semantic negative
controls. It does not certify the complete AST lowerer or parser completeness.
Requires the repository's pinned verifier and Rust toolchains; no Python package
or new runtime Cargo dependency is needed.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time
import tomllib

from check_verus import prepare_tools, proof_succeeded, semantic_refusal

ROOT = Path(__file__).resolve().parents[1]
PROFILE = ROOT / "verification/verus/spec_parser_profile.json"
PIN = ROOT / "verification/verus/toolchain.json"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def execute(command, cwd, environment, log):
    started = time.monotonic()
    proc = subprocess.run(command, cwd=cwd, env=environment, text=True,
                          capture_output=True, timeout=90, check=False)
    write_json(log, {"argv": command, "cwd": str(cwd), "exit_code": proc.returncode,
                     "seconds": time.monotonic() - started,
                     "stdout": proc.stdout, "stderr": proc.stderr})
    return proc


def require(ok, message):
    if not ok:
        raise RuntimeError(message)


def accepted_kernel(proc, report, pin, kernel):
    """Refuse partial runs, stale tools, and missing proof obligations."""
    return (proc.returncode == 0 and proof_succeeded(report, pin)
            and report["verification-results"]["verified"] == kernel["expected_verified"]
            and set(kernel["target_functions"]).issubset(report["func-details"]))


def accepted_native(proc, expected):
    summaries = re.findall(
        r"(?m)^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;",
        proc.stdout,
    )
    return proc.returncode == 0 and summaries == [(str(expected), "0", "0", "0", "0")]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=ROOT / "target/spec-parser-proof")
    parser.add_argument("--install", action="store_true")
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    # A failed replay must never leave a prior success receipt at this path.
    (out / "receipt.json").unlink(missing_ok=True)
    pin = json.loads(PIN.read_text())
    profile = json.loads(PROFILE.read_text())
    cache = Path(os.environ.get("ZENO_FCIS_VERUS_CACHE", str(Path.home() / ".cache/zeno-fcis/verus")))
    verifier = prepare_tools(cache, pin, args.install)
    env = {k: v for k, v in os.environ.items()
           if not k.startswith(("VERUS_", "VARGO_"))
           and k not in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS")}
    env.update(RUSTUP_TOOLCHAIN=pin["rust_toolchain"], VERUS_Z3_PATH=str(verifier / "z3"))
    rust = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
    source_paths = set(profile["bindings"])
    for kernel in profile["kernels"]:
        source_paths.update(kernel["files"])
    bound = {path: sha(ROOT / path) for path in sorted(source_paths)}
    results = []
    grammar = execute(["python3", "tools/generate_spec_grammar.py", "--check"],
                      ROOT, env, out / "grammar.json")
    require(grammar.returncode == 0, "generated grammar is stale")
    for kernel in profile["kernels"]:
        name = kernel["name"]
        prefix = out / name
        command = [str(verifier / "verus"), "--crate-type=lib", "--edition=2024",
                   "--no-cheating", "--no-external-by-default", "--output-json",
                   "--triggers-mode", "silent", kernel["harness"]]
        positive = execute(command, ROOT, env, prefix / "positive.json")
        report = json.loads(positive.stdout)
        require(accepted_kernel(positive, report, pin, kernel),
                f"{name}: pinned proof or obligation inventory failed")
        binary = prefix / "native-tests"
        compile_test = execute(["rustc", "+" + rust, "--edition=2024", "--test",
                                "--check-cfg", "cfg(verus_keep_ghost)",
                                "--check-cfg", "cfg(test)", kernel["harness"],
                                "-o", str(binary)], ROOT, env, prefix / "native-compile.json")
        require(compile_test.returncode == 0, f"{name}: native compilation failed")
        native = execute([str(binary)], ROOT, env, prefix / "native.json")
        require(accepted_native(native, kernel["expected_native_tests"]),
                f"{name}: native tests failed or inventory changed")
        controls = []
        for mutant in kernel["mutations"]:
            subject = Path(mutant["subject"])
            original = (ROOT / subject).read_text()
            start = original.index("fn " + mutant["function"] + "(")
            following = re.search(r"(?m)^(?:pub(?:\([^)]*\))?\s+)?fn\s+", original[start + 1:])
            end = start + 1 + following.start() if following else len(original)
            body = original[start:end]
            require(body.count(mutant["old"]) == 1,
                    f'{name}/{mutant["name"]}: mutation anchor is not unique')
            changed = original[:start] + body.replace(mutant["old"], mutant["new"]) + original[end:]
            with tempfile.TemporaryDirectory(prefix="spec-parser-control-") as temporary:
                specimen = Path(temporary)
                for path in kernel["files"]:
                    dest = specimen / path
                    dest.parent.mkdir(parents=True, exist_ok=True)
                    if Path(path) == subject:
                        dest.write_text(changed)
                    else:
                        shutil.copyfile(ROOT / path, dest)
                outcome = execute(command, specimen, env, prefix / (mutant["name"] + ".json"))
                parsed = json.loads(outcome.stdout)
                require(semantic_refusal(outcome, parsed, pin, subject, specimen,
                                         target_functions=mutant["target_functions"]),
                        f'{name}/{mutant["name"]}: not an intended semantic proof failure')
                controls.append({"name": mutant["name"], "status": "semantic-refusal",
                                 "mutated_source_sha256": hashlib.sha256(changed.encode()).hexdigest(),
                                 "verification_results": parsed["verification-results"]})
        results.append({"kernel": name, "verification_results": report["verification-results"],
                        "verus": report["verus"], "native_tests": kernel["expected_native_tests"],
                        "negative_controls": controls})
        print(f'{name}: {kernel["expected_verified"]} verified; '
              f'{kernel["expected_native_tests"]} native tests; {len(controls)} semantic controls', flush=True)
    require(bound == {path: sha(ROOT / path) for path in sorted(source_paths)},
            "bound source changed during this run")
    logs = {str(path.relative_to(out)): sha(path) for path in sorted(out.rglob("*.json"))
            if path != out / "receipt.json"}
    write_json(out / "receipt.json", {
        "schema": "zeno-fcis/spec-parser-proof/1", "status": "passed",
        "source_sha256": bound, "kernels": results, "log_sha256": logs,
        "scope": profile["scope"], "unproved": profile["unproved"],
    })
    print(out / "receipt.json")


if __name__ == "__main__":
    main()
