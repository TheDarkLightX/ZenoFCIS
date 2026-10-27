#!/usr/bin/env python3
"""Replay native synthesis conformance and controlled refusal cases on Linux or macOS.

Uses installed Rust 1.97.1, Python 3 and Node.js 22; installs nothing. All
stand-in tools are controlled fixtures. This checks finite conformance and
bounded process supervision, not a security sandbox or production authority.
The output directory must be new and retains every command and receipt.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parent.parent


def require(condition: bool, message: str) -> None:
    if not condition:
        raise RuntimeError(message)


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check(cli: Path, out: Path) -> dict:
    out.mkdir(parents=True, exist_ok=False)
    commands = out / "commands"
    commands.mkdir()
    problem = ROOT / "test-data/synthesis/saturating-add.json"
    rows = []

    def call(name, args, exit=0, status=None, code=None, environment=None):
        command = [str(cli), *map(str, args), "--no-kb"]
        start = time.monotonic()
        result = subprocess.run(command, cwd=out, env=environment,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=45)
        record = {"command": command, "exit": result.returncode,
                  "seconds": round(time.monotonic() - start, 3),
                  "stdout": result.stdout.decode(), "stderr": result.stderr.decode()}
        (commands / f"{name}.json").write_text(json.dumps(record, indent=2) + "\n")
        report = json.loads(record["stdout"])
        require(result.returncode == exit, f"{name}: unexpected exit {record}")
        if status is not None:
            require(report.get("status") == status, f"{name}: unexpected status {report}")
        if code is not None:
            require(report.get("detail", {}).get("code") == code, f"{name}: unexpected reason {report}")
        rows.append({"case": name, "exit": result.returncode, "status": report.get("status"),
                     "code": report.get("detail", {}).get("code"), "seconds": record["seconds"]})
        return report

    rustc = subprocess.check_output(["rustup", "which", "--toolchain", "1.97.1", "rustc"], text=True).strip()
    tools = {"rust": Path(rustc), "python": Path(shutil.which("python3") or "/missing-python"),
             "javascript": Path(shutil.which("node") or "/missing-node")}
    extensions = {"rust": "rs", "python": "py", "javascript": "mjs"}
    artifacts = {}
    expected = b"".join(f"{min(a + b, 3)}\n".encode() for a in range(4) for b in range(4))
    expected_digest = hashlib.sha256(expected).hexdigest()
    discovery = call("discover", ["synth", "discover"])
    require(discovery["runtime_platforms"] == ["linux", "macos"], "missing host discovery")

    for language, tool in tools.items():
        artifact, receipt = out / language, out / f"{language}-conformance.json"
        artifacts[language] = artifact
        args = ["synth", "run", problem, "--target", language, "--out", artifact]
        report = call(f"{language}-select", args, status="selected")
        require(report["runtime_conformance"] == "not-run", "synthesis claimed target execution")
        call(f"{language}-current", [*args, "--check"], status="current")
        verify = ["synth", "verify", problem, "--target", language, "--out", artifact, "--tool", tool]
        report = call(f"{language}-conformance", [*verify, "--receipt", receipt], status="passed")
        require(report == json.loads(receipt.read_text()), "receipt differs from observed report")
        require(report["authority"] == "none" and report["inputs_checked"] == 16, "wrong authority or coverage")
        require(report["stdout_sha256"] == expected_digest, "independent saturating-add table differs")
        require(report["source_sha256"] == digest(artifact / f"transition.{extensions[language]}"), "source binding differs")
        require(report["program_sha256"] == digest(artifact / "program.zcve"), "program binding differs")
        require(report["manifest_sha256"] == digest(artifact / "manifest.json"), "manifest binding differs")
        require(report["tool"]["binary_sha256"] == digest(tool.resolve()), "tool binding differs")
        require("not a security sandbox" in report["runtime_supervision"], "missing scope")
        kept = receipt.read_bytes()
        call(f"{language}-existing-receipt", [*verify, "--receipt", receipt], exit=1, status="invalid-receipt-path")
        require(receipt.read_bytes() == kept, "existing receipt was changed")
        call(f"{language}-receipt-inside-artifact", [*verify, "--receipt", artifact / "receipt.json"],
             exit=1, status="invalid-receipt-path")
        require(not (artifact / "receipt.json").exists(), "refusal wrote inside immutable artifact")
        for filename in [f"transition.{extensions[language]}", "manifest.json", "program.zcve", "vectors.json"]:
            path = artifact / filename
            original = path.read_bytes()
            path.write_bytes(original + b" ")
            failed_receipt = out / f"{language}-{filename}-drift-receipt.json"
            try:
                call(f"{language}-{filename}-drift", [*verify, "--receipt", failed_receipt], exit=1, status="artifact-drift")
                require(not failed_receipt.exists(), "artifact drift produced a receipt")
            finally:
                path.write_bytes(original)

    # Exercise the default rustup discovery path as well as explicit --tool.
    # Canonicalizing a rustup symlink must not invoke it as rustup-init.
    default_receipt = out / "rust-default-conformance.json"
    default = call(
        "rust-default-conformance",
        ["synth", "verify", problem, "--target", "rust", "--out", artifacts["rust"],
         "--receipt", default_receipt],
        status="passed",
    )
    require(default == json.loads(default_receipt.read_text()), "default Rust receipt differs")
    require(default["authority"] == "none" and default["inputs_checked"] == 16, "wrong default Rust coverage")
    require(default["stdout_sha256"] == expected_digest, "default Rust output differs")
    require(default["tool"]["binary_sha256"] == digest(tools["rust"].resolve()), "default Rust tool differs")

    def stand_in(name, body, version="Python 3.9.6"):
        path = out / name
        path.write_text(f'#!/bin/sh\nif [ "$1" = "--version" ]; then printf "%s\\n" "{version}"; exit 0; fi\n{body}\n')
        path.chmod(0o700)
        return path

    def refused_tool(name, tool, status, code, language="python", exit=2):
        receipt = out / f"{name}-receipt.json"
        call(name, ["synth", "verify", problem, "--target", language, "--out", artifacts[language],
                    "--tool", tool, "--receipt", receipt], exit=exit, status=status, code=code)
        require(not receipt.exists(), f"{name}: failed execution wrote a receipt")

    refused_tool("missing-tool", out / "absent", "conformance-unknown", "tool-missing")
    refused_tool("wrong-version", stand_in("old-node", "exit 0", "v18.20.8"),
                 "conformance-unknown", "tool-version", language="javascript")
    refused_tool("bad-result", stand_in("bad-python", "i=0; while [ $i -lt 16 ]; do echo 0; i=$((i+1)); done"),
                 "conformance-failed", "counterexample", exit=1)
    refused_tool("bad-protocol", stand_in("malformed-python", "printf malformed"),
                 "conformance-failed", "output-protocol", exit=1)
    refused_tool("nonzero-exit", stand_in("crashing-python", "exit 23"), "conformance-unknown", "tool-exit")
    python = str(tools["python"].resolve())
    # The fixture writes just above the real output cap, with a bounded allocation.
    refused_tool("output-overflow", stand_in("noisy-python",
                 f'''exec '{python}' -I -B -c 'import os; os.write(1, b"x" * (32*1024*1024+1))' '''),
                 "conformance-unknown", "output-limit")
    refused_tool("operation-timeout", stand_in("slow-python", "exec /bin/sleep 60"),
                 "conformance-unknown", "timeout")

    malformed = json.loads(problem.read_text())
    malformed["sketch"]["nodes"][0] = ["read_file", "unavailable"]
    invalid = out / "invalid-problem.json"
    invalid.write_text(json.dumps(malformed))
    rejected = out / "invalid-artifact"
    call("invalid-program", ["synth", "run", invalid, "--target", "python", "--out", rejected],
         exit=1, status="invalid-problem")
    require(not rejected.exists(), "invalid program created an artifact")

    hostile = dict(os.environ)
    hostile.update({"NODE_OPTIONS": "--require=/zeno-fcis-missing-preload.cjs", "NODE_PATH": "/zeno-fcis-missing-modules"})
    call("inherited-node-switches", ["synth", "verify", problem, "--target", "javascript", "--out", artifacts["javascript"],
                                    "--tool", tools["javascript"]], status="passed", environment=hostile)
    return {"status": "passed", "cli_sha256": digest(cli), "cases": rows,
            "scope": "exact finite output conformance and process supervision; no sandbox or production authority"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    report = check(args.cli.resolve(), args.out.resolve())
    (args.out / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
