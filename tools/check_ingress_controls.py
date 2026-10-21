#!/usr/bin/env python3
"""Build each ingress mutant, check its named failure, then rerun the baseline.

Default execution uses real loopback. --memory selects the explicit byte-stream
harness; that evidence does not qualify TCP or loopback operation.
"""
from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
HTTP = "crates/zeno-fcis-ingress/src/http.rs"
DURABLE = "crates/zeno-fcis-ingress/src/durable.rs"
CONTROLS = [
    ("global-outbox-blocking", "journey", "failing_destination_does_not_block_replies_or_other_destinations", "unrelated destination was blocked", DURABLE,
     "let delivery = DeliveryProgram::start_per_destination(", "let delivery = DeliveryProgram::start("),
    ("destination-order", "journey", "failing_destination_does_not_block_replies_or_other_destinations", "destination order broken", "crates/zeno-fcis-shell/src/effects/program.rs",
     "continuation.blocked.insert(destination);", "let _ = destination;"),
    ("delivery-replaces-commit", "journey", "failing_destination_does_not_block_replies_or_other_destinations", "delivery failure changed committed reply", DURABLE,
     "let _ = run::<DeliveryProgram>(&mut self.terminal, delivery, trace, sink);",
     "if !matches!(run::<DeliveryProgram>(&mut self.terminal, delivery, trace, sink)?, Outcome::Ok(_)) { return Err(Refusal::Unavailable); }"),
    ("external-tail-served", "journey", "external_tail_is_never_served", "external tail was served", DURABLE,
     "self.load(trace, sink)?;\n        let actual = match self.call(Operation::ReadState(self.store), trace, sink)? {",
     "self.load(trace, sink)?;\n        if let Some(head) = &self.head { return Ok(head.clone()); }\n        let actual = match self.call(Operation::ReadState(self.store), trace, sink)? {"),
    ("wrong-replay-index", "journey", "replay_index_keeps_distinct_keys", "replay index returned another key", DURABLE,
     "match self.commits.get(&key) {", "match self.commits.values().next() {"),
    ("body-actor", "http", "actor_from_credential", "actor came from body", HTTP,
     "principal: authenticated,", "principal: Hash32::new(body[73..105].try_into().unwrap_or([0; 32])),"),
    ("missing-size-limit", "http", "oversized_requests", "body size limit missing", HTTP,
     "Limited::new(request.into_body(), MAX_BODY)", "Limited::new(request.into_body(), usize::MAX)"),
    ("duplicate-header", "http", "duplicate_headers", "duplicate header accepted", HTTP,
     "if headers.get_all(name).iter().count() != 1 {", "if false && headers.get_all(name).iter().count() != 1 {"),
    ("cross-origin", "http", "cross_origin", "cross-origin accepted", HTTP,
     "origin.as_bytes() != config.origin.as_bytes()", "false"),
    ("changed-command", "journey", "durable_journey_and_changed_key", "changed command accepted under old key", DURABLE,
     "entry.publication.command != command.bytes", "false"),
    ("trace-secret", "http", "trace_redaction", "secret in trace", HTTP,
     "let trace = Trace::request(&command);", "let trace = Trace { request: Some(Hash32::new(config.parties[0].0)), ..Trace::request(&command) };"),
    ("credential-compare", None, "credential-constant-time", "credential-constant-time", HTTP,
     "difference |= std::hint::black_box(candidate[index] ^ configured[index]);", "if candidate[index] != configured[index] { return false; } difference |= 0;"),
    # Several parties per server: each control builds and fails its named test.
    ("party-first-context", "journey", "parties_use_their_own_context", "context came from another party", DURABLE,
     ".find(|(owner, _)| *owner == principal)", ".find(|_| true)"),
    ("credential-scan-early-exit", "lib", "http::tests::credential_scan_visits_every_entry", "credential scan stopped early", HTTP,
     "*byte |= owned & mask;\n        }\n", "*byte |= owned & mask;\n        }\n        if found != 0 {\n            break;\n        }\n"),
    ("party-replay-key", "journey", "parties_cannot_replay_each_other", "another party replayed a request", DURABLE,
     "let key = command.replay_key();",
     "let key = crate::Command { principal: self.parties[0].0, contract: command.contract, key: command.key, expected: command.expected, bytes: Vec::new() }.replay_key();"),
    # Only chunked framing; `gzip, chunked` would hand over still-coded bytes.
    ("transfer-coding", "http", "transfer_codings_other_than_chunked", "transfer coding other than chunked accepted", HTTP,
     '.is_some_and(|value| !value.as_bytes().eq_ignore_ascii_case(b"chunked"))', ".is_some_and(|_| false)"),
    # A public origin from ZENO_APP_ORIGIN must match exactly.
    ("origin-prefix", "http", "configured_https_origin_is_exact", "configured origin not exact", HTTP,
     "origin.as_bytes() != config.origin.as_bytes()", "!origin.as_bytes().starts_with(config.origin.as_bytes())"),
    ("origin-ignore-case", "http", "configured_https_origin_is_exact", "configured origin not exact", HTTP,
     "origin.as_bytes() != config.origin.as_bytes()", "!origin.as_bytes().eq_ignore_ascii_case(config.origin.as_bytes())"),
    # A second party's credential in a trace passes every single-secret trace test.
    ("party-secret-in-trace", "journey", "party_traces_hold_no_credential", "party credential in trace", HTTP,
     "let trace = Trace::request(&command);",
     "let trace = match config.parties.get(1) { Some((leak, _)) => Trace { request: Some(Hash32::new(*leak)), ..Trace::request(&command) }, None => Trace::request(&command) };"),
]

def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--memory", action="store_true")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, CARGO_NET_OFFLINE="true")
    if args.memory: env["INGRESS_TEST_MEMORY"] = "1"
    else: env.pop("INGRESS_TEST_MEMORY", None)
    cargo = ["cargo", "+1.97.1", "test", "--offline", "--locked", "-p", "zeno-fcis-ingress"]
    rows = []
    def run(root, command, label):
        result = subprocess.run(command, cwd=root, env=env, capture_output=True, text=True, timeout=600)
        log = result.stdout+result.stderr
        args.out.with_name(label+".log").write_text(log)
        return result.returncode, log
    def check(root, suite, name, label):
        selector = ["--lib"] if suite == "lib" else ["--test", suite]
        command = cargo+selector+[name, "--", "--exact", "--nocapture"] if suite else ["python3", "tools/check_ingress.py"]
        return run(root, command, label)
    with tempfile.TemporaryDirectory(prefix="ingress-controls-") as directory:
        source = Path(directory)/"source"
        shutil.copytree(ROOT, source, ignore=shutil.ignore_patterns(".git", "target", "node_modules", "__pycache__"))
        temporary = Path(directory)/"test-tmp"
        temporary.mkdir()
        env["TMPDIR"] = str(temporary)
        for name, suite, test, marker, path, before, after in CONTROLS:
            code, log = check(source, suite, test, name+"-baseline-before")
            if code or (suite and "1 passed" not in log): raise RuntimeError(f"baseline failed: {name}")
            target = source/path
            original = target.read_text()
            if original.count(before) != 1: raise RuntimeError(f"nonunique mutation anchor: {name}")
            target.write_text(original.replace(before, after))
            build, _ = run(source, cargo+["--no-run"], name+"-build")
            code, log = check(source, suite, test, name+"-mutant")
            target.write_text(original)
            restored, baseline_log = check(source, suite, test, name+"-baseline-after")
            killed = code == (101 if suite else 1) and marker in log and (not suite or "test result: FAILED" in log)
            row = dict(control=name, check=test, build=build, mutant=code, baseline=restored, killed=killed, transport="memory" if args.memory else "loopback")
            rows.append(row)
            args.out.write_text(json.dumps(rows, indent=2)+"\n")
            print(json.dumps(row), flush=True)
            if build or not killed or restored or (suite and "1 passed" not in baseline_log): raise RuntimeError(f"control did not meet build/fail/baseline obligation: {name}")
    print(f"ingress controls: PASS ({len(CONTROLS)} compiled mutants; named failures; restored baselines)")
    return 0
if __name__ == "__main__":
    raise SystemExit(main())
