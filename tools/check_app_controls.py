#!/usr/bin/env python3
"""Plant each generated-application defect, check its named test fails, then restore.

Run it on the escrow example built with `zeno-fcis new ... --contract
examples/apps/escrow-deal --app`. The application is copied first, so the
built application is never changed. Default execution uses real loopback;
--memory selects the socket-free harness.
"""
from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

APP = "src/app.rs"
CONTROLS = [
    ("stale-preview-head", "preview_uses_the_latest_committed_head", "preview read a stale head after commit",
     "let head = self.durable.verified_snapshot()?;\n        if request.expected() != head.version {",
     "let mut head = self.durable.verified_snapshot()?;\n        head.version = 0;\n        head.state = crate::genesis().map_err(failed)?;\n        if request.expected() != head.version {"),
    ("clock-read-at-replay", "server_clock_journey", "clock read again at replay",
     "&|configured, stored| same_party(authority, clock, configured, stored),",
     "&|configured, stored| admit(authority, clock, last, configured).is_ok_and(|now| now == stored),"),
    ("clock-backward-after-restart", "server_clock_journey", "clock went backward after a restart",
     "for commit in self.durable.commits() {", "for commit in self.durable.commits().take(0) {"),
    ("clock-clamped", "server_clock_journey", "out-of-range clock clamped",
     "if time < self.range.0 || time > self.range.1 {",
     "let time = time.clamp(self.range.0, self.range.1);\n        if false {"),
    ("view-hidden-in-state", "parties_journey", "a hidden field appears in /state",
     "hide(self.view(principal), &mut values);", ""),
    ("view-from-request", "parties_journey", "the view is chosen from the request",
     "let view = self.view(request.principal());",
     "let shown: Vec<bool> = serde_json::from_slice::<Value>(request.bytes()).ok()"
     ".and_then(|v| v.as_array().map(|a| a.iter().map(|x| !x.is_null()).collect())).unwrap_or_default();"
     "\n        let view = Some(shown.as_slice());"),
    ("journal-drops-request", "parties_journey", "an admitted request is missing from the journal",
     "self.file.write_all(line.as_bytes())?;\n            self.file.sync_all()?;\n            self.position = position;",
     "self.file.sync_all()?;\n            let _ = position;\n            self.position = kept;"),
    ("connector-acknowledges-failure", "program_connector_acknowledges_only_success", "a non-zero exit is acknowledged",
     "return Ok(status.success());", "return Ok(true);"),
    ("connector-drops-delivery-id", "program_connector_acknowledges_only_success", "the delivery ID is missing from the envelope",
     'value["payload"] = Value::from(hex(&delivery.payload));',
     'value["payload"] = Value::from(hex(&delivery.payload));\n        value.as_object_mut().map(|object| object.remove("delivery"));'),
    # Mutated requests: a decode panic, or a refusal shown as a decision.
    ("decode-panics", "mutated_requests_refuse_or_decide", "ParseIntError",
     "let n = text.parse::<i128>().map_err(|_| Refusal::Malformed)?;", "let n = text.parse::<i128>().unwrap();"),
    ("refusal-shown-as-accept", "mutated_requests_refuse_or_decide", "an input outside its declared domain was decided",
     '"class":"Refused","reason":null,"post":[]', '"class":"Accept","reason":null,"post":[]'),
    # Operator configuration is parsed strictly.
    ("context-domain-unchecked", "party_contexts_refuse_values_outside_their_domains",
     "a context outside its declared domain started",
     "if !declared(schema, &context) {", "if false && !declared(schema, &context) {"),
    ("clock-file-loose", "clock_file_is_read_strictly", "read loosely",
     ".and_then(|text| text.parse::<i128>().ok().filter(|n| n.to_string() == text))",
     ".and_then(|text| text.trim().parse::<i128>().ok())"),
    ("connector-input-blocks", "program_connector_acknowledges_only_success", "time limit with unread input",
     "std::thread::spawn(move || stdin.write_all(&input));", "let _ = stdin.write_all(&input);"),
    ("unknown-setting-ignored", "unknown_settings_refuse_start_up", "an unknown or conflicting setting started the server",
     'if name.starts_with("ZENO_APP_") && !SETTINGS.contains(&name.as_ref()) {',
     'if false && name.starts_with("ZENO_APP_") && !SETTINGS.contains(&name.as_ref()) {'),
    ("connector-and-receiver", "unknown_settings_refuse_start_up", "an unknown or conflicting setting started the server",
     '&& std::env::var_os("ZENO_APP_RECEIVER").is_some()', "&& false"),
]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--memory", action="store_true")
    parser.add_argument("--only", action="append", default=[])
    args = parser.parse_args()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, CARGO_NET_OFFLINE="true")
    if args.memory:
        env.pop("APP_JOURNEY_TCP", None)
    else:
        env["APP_JOURNEY_TCP"] = "1"
    cargo = ["cargo", "+1.97.1", "test", "--offline", "--locked", "--all-features", "--test", "app_journey"]
    rows = []

    def run(root, command, label):
        result = subprocess.run(command, cwd=root, env=env, capture_output=True, text=True, timeout=900)
        log = result.stdout + result.stderr
        args.out.with_name(label + ".log").write_text(log)
        return result.returncode, log

    with tempfile.TemporaryDirectory(prefix="app-controls-") as directory:
        app = Path(directory) / "app"
        shutil.copytree(args.app, app, ignore=shutil.ignore_patterns("target"))
        for name, test, marker, before, after in CONTROLS:
            if args.only and name not in args.only:
                continue
            check = cargo + [test, "--", "--exact", "--nocapture"]
            code, log = run(app, check, name + "-baseline-before")
            if code or "1 passed" not in log:
                raise RuntimeError(f"baseline failed: {name}")
            target = app / APP
            original = target.read_text()
            if original.count(before) != 1:
                raise RuntimeError(f"nonunique mutation anchor: {name}")
            target.write_text(original.replace(before, after))
            build, _ = run(app, cargo + ["--no-run"], name + "-build")
            code, log = run(app, check, name + "-mutant")
            target.write_text(original)
            restored, baseline = run(app, check, name + "-baseline-after")
            killed = code == 101 and marker in log and "test result: FAILED" in log
            row = dict(control=name, check=test, marker=marker, build=build, mutant=code, baseline=restored,
                       killed=killed, transport="memory" if args.memory else "loopback")
            rows.append(row)
            args.out.write_text(json.dumps(rows, indent=2) + "\n")
            print(json.dumps(row), flush=True)
            if build or not killed or restored or "1 passed" not in baseline:
                raise RuntimeError(f"control did not meet build/fail/baseline obligation: {name}")
    print(f"app controls: PASS ({len(rows)} compiled mutants; named failures; restored baselines)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
