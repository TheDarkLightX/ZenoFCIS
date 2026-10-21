#!/usr/bin/env python3
"""Compare generated default bytes, or build and execute template controls.

All output is local evidence. New consumers use the documented first offline
build to prune their copied lock; subsequent builds and tests are locked.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import subprocess
import time
import tomllib

from cargo_source_scope import source_environment

ROOT = Path(__file__).resolve().parents[1]
CARGO = ["cargo", "+1.97.1"]
TEMPLATES = ["durable-counter", "prepared-counter", "account-lockout",
             "order-fulfillment", "inventory-reservation", "compliance-gateway",
             "withdrawal-queue", "agent-treasury-guard", "minimal", "mini-determinator"]


def run(command: list[str], cwd: Path, log: Path) -> subprocess.CompletedProcess:
    start = time.monotonic()
    environment = source_environment(cwd, dict(os.environ)) if command[0] == "cargo" else dict(os.environ)
    result = subprocess.run(command, cwd=cwd, env=environment, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, timeout=600)
    with log.open("ab") as output:
        output.write((json.dumps({"argv": command, "cwd": str(cwd),
                                  "exit": result.returncode,
                                  "seconds": time.monotonic() - start}) + "\n").encode())
        output.write(result.stdout)
    return result


def require(result: subprocess.CompletedProcess) -> bytes:
    if result.returncode:
        raise RuntimeError(f"command failed ({result.returncode}): {result.stdout[-2000:]!r}")
    return result.stdout


def cell(value):
    return {"bytes": value.hex()} if isinstance(value, bytes) else value


def database(path: Path) -> dict:
    with sqlite3.connect(f"file:{path}?mode=ro", uri=True) as connection:
        names = [r[0] for r in connection.execute(
            "SELECT name FROM sqlite_master WHERE type='table' AND name GLOB 'v2_*' ORDER BY name")]
        result = {}
        for name in names:
            if not name.replace("_", "").isalnum():
                raise ValueError(name)
            count = len(connection.execute(f"PRAGMA table_info({name})").fetchall())
            order = ",".join(str(i + 1) for i in range(count))
            result[name] = [[cell(c) for c in row] for row in connection.execute(
                f"SELECT * FROM {name} ORDER BY {order}")]
        return result


def capture(cli: Path, apps: Path, out: Path, feature: bool, only: str | None) -> None:
    apps.mkdir(parents=True, exist_ok=True)
    log = out.with_suffix(".log")
    observations = {}
    reference = tomllib.loads((ROOT / "Cargo.lock").read_text())
    def external(lock):
        return {(p["name"], p["version"], p.get("source"), p.get("checksum"))
                for p in lock["package"] if p.get("source")}
    for template in ([only] if only else TEMPLATES):
        app = apps / template
        if not app.exists():
            require(run([str(cli), "new", str(app), "--template", template], ROOT, log))
        vectors = {p.relative_to(app).as_posix(): p.read_bytes().hex()
                   for p in sorted(app.rglob("*")) if p.is_file()
                   and (p.suffix in (".zcve",) or p.name == "vectors.json")}
        if not (app / "Cargo.toml").exists():
            observations[template] = {"vectors": vectors}
            continue
        flags = ["--features", "shell-effects"] if feature and template == "withdrawal-queue" else []
        # A fresh consumer's copied workspace lock needs only pruning.
        lock = tomllib.loads((app / "Cargo.lock").read_text())
        resolved = any(p["name"] == template for p in lock["package"])
        require(run(CARGO + ["build", "--offline"] + (["--locked"] if resolved else []) + flags, app, log))
        lock = tomllib.loads((app / "Cargo.lock").read_text())
        if not external(lock) <= external(reference):
            raise RuntimeError("consumer resolved an unreviewed dependency")
        binary = Path(os.environ["CARGO_TARGET_DIR"]) / "debug" / template
        path = app / "observed.sqlite"
        if path.exists():
            raise RuntimeError(f"observation store already exists: {path}")
        result = run([str(binary), str(path)], app, log)
        require(result)
        observation = {"vectors": vectors, "stdout": result.stdout.hex(),
                       "tables": database(path)}
        if template == "withdrawal-queue":
            pending = app / "pending.sqlite"
            observation["payouts"] = require(run([str(binary), "--payouts", str(pending)], app, log)).hex()
            export = require(run([str(binary), "--relay-export", str(pending)], app, log))
            observation["export"] = export.hex()
            records = [json.loads(line) for line in export.splitlines()]
            first = records[0]
            # Malformed input, unknown ID, wrong hash, matching ack, repeated
            # ack with a wrong hash: retain both diagnostics and their order.
            attempts = [("bad", "bad"), ("ff" * 32, "ff" * 32),
                        (first["delivery_id"], "ff" * 32),
                        (first["delivery_id"], first["payload_sha256"]),
                        (first["delivery_id"], "ff" * 32)]
            observation["acknowledgements"] = []
            for identity, digest in attempts:
                r = run([str(binary), "--relay-acknowledge", str(pending), identity, digest], app, log)
                observation["acknowledgements"].append([r.returncode, r.stdout.hex()])
            observation["pending_tables"] = database(pending)
        observations[template] = observation
    out.write_text(json.dumps(observations, indent=2, sort_keys=True) + "\n")


def controls(app: Path, out: Path) -> None:
    log = out.with_suffix(".log")
    source = app / "src/shell_effects.rs"
    lib = app / "src/lib.rs"
    original = {source: source.read_bytes(), lib: lib.read_bytes()}
    flags = ["--locked", "--offline", "--features", "shell-effects"]
    build = CARGO + ["test", "--no-run"] + flags
    def check(name):
        return run(CARGO + ["test"] + flags + ["--test", "shell_effects", name, "--", "--exact", "--nocapture"], app, log)
    summary = []
    mutations = [
        ("default-byte-leak", lib, '"status\\\":\\\"decided', '"status\\\":\\\"effects',
         "default_bytes_unchanged", "default byte vector changed"),
        ("unbounded-retries", source, "for _ in 0..MAX_REQUESTS {", "for _ in 0..usize::MAX {",
         "retries_are_bounded", "retry budget exceeded"),
        ("transcript-published", source, "self.last_transcript.as_ref()", 'println!("{:?}", self.last_transcript); self.last_transcript.as_ref()',
         "private_recording_stays_private", "private transcript reached stdout"),
        ("new-recovery-key", source, "key: retained.key,",
         "key: if recover { zeno_fcis_codec::Hash32::ZERO } else { retained.key },",
         "recovery_uses_original_key", "original-key recovery failed"),
    ]
    try:
        for name, path, before, after, test, marker in mutations:
            for p, b in original.items(): p.write_bytes(b)
            require(check(test))
            text = path.read_text()
            if text.count(before) != 1:
                raise RuntimeError(f"mutation anchor {name}: {text.count(before)}")
            text = text.replace(before, after)
            if name == "unbounded-retries":
                anchor = "Handled::Answer(response) =>"
                if text.count(anchor) != 1: raise RuntimeError("retry anchor")
                text = text.replace(anchor, "Handled::Answer(Response::Send(zeno_fcis_shell::effects::port::Sent::Unknown(_))) => continue,\n                    " + anchor)
            path.write_text(text)
            built = run(build, app, log)
            require(built)
            killed = check(test)
            if killed.returncode != 101 or marker.encode() not in killed.stdout:
                raise RuntimeError(f"control did not fail its named check: {name}")
            for p, b in original.items(): p.write_bytes(b)
            restored = check(test)
            require(restored)
            summary.append({"control": name, "check": test, "build": built.returncode,
                            "mutant": killed.returncode, "restored": restored.returncode,
                            "marker": marker})
    finally:
        for p, b in original.items(): p.write_bytes(b)
        out.write_text(json.dumps(summary, indent=2) + "\n")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path)
    parser.add_argument("--apps", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--feature", action="store_true")
    parser.add_argument("--only", choices=TEMPLATES)
    parser.add_argument("--before", type=Path)
    parser.add_argument("--controls-app", type=Path)
    args = parser.parse_args()
    if args.controls_app:
        controls(args.controls_app.resolve(), args.out.resolve())
    else:
        if not args.cli or not args.apps: parser.error("capture needs --cli and --apps")
        capture(args.cli.resolve(), args.apps.resolve(), args.out.resolve(), args.feature, args.only)
        if args.before:
            before = json.loads(args.before.read_bytes())
            after = json.loads(args.out.read_bytes())
            assert {name: before[name] for name in after} == after, "default byte vectors changed"
        print(json.dumps({"bytes": args.out.stat().st_size, "sha256": hashlib.sha256(args.out.read_bytes()).hexdigest()}))


if __name__ == "__main__":
    try:
        from telemetry import run_cli
    except ImportError:
        # Reporting is optional, including for a standalone copy of this tool.
        run_cli = lambda function: function()
    def _entrypoint():
        main()
    run_cli(_entrypoint)
