#!/usr/bin/env python3
"""Build six effect-policy mutants, kill each by its named check, restore and pass.

Uses an isolated source copy, the caller's bounded cargo target and local cache.
A compiler error, signal, unrelated failure or surviving mutant is not a kill.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from cargo_source_scope import source_environment

ROOT = Path(__file__).resolve().parents[1]
EFFECTS = "crates/zeno-fcis-shell/src/effects/"
POLICY = EFFECTS + "policy.rs"
HANDLER = EFFECTS + "handler.rs"
EXPLORER = EFFECTS + "explore.rs"
TESTS = "effects::policy_tests::"

# name, named test, required behavioural diagnostic, source edits
CONTROLS = [
    ("rewrite-recipient", "policy_preserves_request", "policy rewrote requested operation", [
        (HANDLER, "None => terminal.handle(binding, operation),", """None => {
                let mut rewritten = operation.clone();
                if let Operation::Send(_, delivery) = &mut rewritten {
                    delivery.destination = b"rerouted".to_vec();
                }
                terminal.handle(binding, &rewritten)
            },"""),
    ]),
    ("second-send", "policy_multiset", "second identical send allowed", [
        (POLICY, "if state.attempts.contains(&key) {", "if false && state.attempts.contains(&key) {"),
    ]),
    ("missing-allowlist", "policy_allowlist", "missing allowlist check", [
        (POLICY, "if !recipients.iter().any(|allowed| allowed == destination) {",
         "if false && !recipients.iter().any(|allowed| allowed == destination) {"),
    ]),
    ("silent-drop", "policy_refusal_pending", "rejection silently dropped record", [
        (HANDLER, "Some(code) => Handled::Answer(Response::Send(Sent::Refused(code))),", """Some(_code) => {
                if let Operation::Send(store, delivery) = operation {
                    let ack = Operation::Acknowledge(*store, delivery.id, delivery.payload_sha256);
                    let _ = terminal.handle(&Binding::of(binding.id, &ack), &ack);
                }
                Handled::Crash
            },"""),
    ]),
    ("undeclared-field", "policy_declared_fields", "undeclared field read reached policy", [
        (POLICY, "let Some(key) = fields.key else {",
         "let Some(key) = fields.key.filter(|_| fields.destination.is_some()) else {"),
    ]),
    ("skip-policy-on-retry", "policy_retry_product", "policy: second send of a key", [
        (EXPLORER, """let handled = self.actors[index]
            .stack
            .handle(&mut self.simulation, &binding, &operation);""", """let skip = match &operation {
            Operation::Send(store, delivery) => self.attempts.contains(&(store.identity, delivery.id)),
            _ => false,
        };
        let handled = if skip {
            Stack::default().handle(&mut self.simulation, &binding, &operation)
        } else {
            self.actors[index].stack.handle(&mut self.simulation, &binding, &operation)
        };"""),
    ]),
]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--release", action="store_true", help="use optimized finite checks")
    args = parser.parse_args()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, CARGO_NET_OFFLINE="true")
    env.setdefault("CARGO_TARGET_DIR", str(ROOT / "target" / "effect-policy-controls"))
    cargo = ["cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-shell", "--lib"] + (["--release"] if args.release else [])

    def run(root: Path, extra: list[str]) -> subprocess.CompletedProcess[str]:
        if shutil.disk_usage("/").free < 8 * 1024**3:
            raise RuntimeError("root free space below 8 GiB")
        return subprocess.run(cargo + extra, cwd=root, env=source_environment(root, env), text=True,
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=900)

    rows = []
    with tempfile.TemporaryDirectory(prefix="effect-policy-controls-", dir=os.environ.get("TMPDIR")) as temp:
        copy = Path(temp) / "source"
        shutil.copytree(ROOT, copy, ignore=shutil.ignore_patterns(".git", "target", "node_modules", "__pycache__"))
        for name, test, marker, edits in CONTROLS:
            named = [TESTS + test, "--", "--exact", "--nocapture"]
            baseline = run(copy, named)
            if baseline.returncode or "1 passed" not in baseline.stdout:
                raise SystemExit(f"BASELINE FAILED {name}:\n{baseline.stdout}")
            original = {path: (copy / path).read_text() for path, _, _ in edits}
            for path, before, after in edits:
                text = (copy / path).read_text()
                if text.count(before) != 1:
                    raise RuntimeError(f"mutation anchor count for {name}: {text.count(before)}")
                (copy / path).write_text(text.replace(before, after))
            build = run(copy, ["--no-run"])
            if build.returncode:
                raise SystemExit(f"BUILD FAILURE, not a kill, {name}:\n{build.stdout}")
            mutant = run(copy, named)
            args.out.with_name(f"{args.out.stem}-{name}.log").write_text(build.stdout + mutant.stdout)
            if mutant.returncode != 101 or "test result: FAILED." not in mutant.stdout or marker not in mutant.stdout:
                raise SystemExit(f"NOT KILLED for named reason {name}:\n{mutant.stdout}")
            for path, text in original.items():
                (copy / path).write_text(text)
            restored = run(copy, named)
            if restored.returncode or "1 passed" not in restored.stdout:
                raise SystemExit(f"RESTORED BASELINE FAILED {name}:\n{restored.stdout}")
            args.out.with_name(f"{args.out.stem}-{name}-restored.log").write_text(restored.stdout)
            row = dict(control=name, check=TESTS + test, build_exit=build.returncode,
                       check_exit=mutant.returncode, restored_exit=restored.returncode, result="killed")
            rows.append(row)
            print(json.dumps(row), flush=True)
    args.out.write_text(json.dumps(rows, indent=2) + "\n")
    print("PASS: six mutants built and failed their named checks; each restored baseline passed")


if __name__ == "__main__":
    try:
        from telemetry import run_cli
    except ImportError:
        # Reporting is optional, including for a standalone copy of this tool.
        run_cli = lambda function: function()
    def _entrypoint():
        main()
    run_cli(_entrypoint)
