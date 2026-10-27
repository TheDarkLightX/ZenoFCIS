#!/usr/bin/env python3
"""Replay upgrades of contracts and stores created by the released 2.4.0 CLI.

The released executable is checked by version and SHA-256. Its own source
binding check admits the supplied historical SDK. This check creates the
adoption and both stores with that executable and SDK, then builds the
current application from the retained contract. No receipt is refreshed.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile
import tomllib

import check_contract_upgrade as upgrade
import check_generated_application as applications

RELEASED_SHA256 = "92123be362417d7dedbd28b60822679261a1837ae8171e97218059ccffc4667a"


def export_test_data(directory: Path, destination: Path) -> dict:
    """Export retained inputs only. Generated fixtures confer no upgrade authority."""
    recorded = json.loads((directory / "RELEASED_INPUTS.json").read_text())
    if recorded["released_cli_sha256"] != RELEASED_SHA256:
        raise RuntimeError("retained run does not bind the released executable")
    destination.mkdir(parents=True, exist_ok=True)
    files = {}

    def write(name: str, data: bytes) -> None:
        (destination / name).write_bytes(data)
        files[name] = hashlib.sha256(data).hexdigest()

    def expand(identity: bytes, compact: bytes) -> bytes:
        # Independent fixture authoring codec. Complete runtime replay validates it.
        if compact[0] != 0 or int.from_bytes(compact[1:17], "big") != 0x5A505532 or compact[51] != 1:
            raise RuntimeError("unrecognized released compact publication")
        size = int.from_bytes(compact[52:68], "big")
        inner, tail = compact[68:68 + size], compact[68 + size:]
        if len(inner) != size or inner[0] != 0 or int.from_bytes(inner[1:17], "big") != 0x5A525032:
            raise RuntimeError("unrecognized released audited subject")
        framed = b"\x01" + len(identity).to_bytes(16, "big") + identity
        inner = inner[:51] + framed + inner[51:]
        return compact[:51] + b"\x01" + len(inner).to_bytes(16, "big") + inner + tail

    for store in ("genesis", "pending"):
        with sqlite3.connect((directory / (store + ".sqlite")).resolve().as_uri() + "?mode=ro", uri=True) as connection:
            connection.row_factory = sqlite3.Row
            version = connection.execute("PRAGMA user_version").fetchone()[0]
            if version != 10:
                raise RuntimeError("retained released store is not schema version 10")
            sql = "PRAGMA user_version=10;\n" + "\n".join(connection.iterdump()) + "\n"
            write(store + ".sql", sql.encode())
            genesis = connection.execute("SELECT * FROM v2_genesis").fetchone()
            write(store + "-identity.bin", genesis["identity"])
            write(store + "-initial.bin", genesis["initial"])
            write(store + "-genesis.bin", expand(genesis["identity"], genesis["publication"]))
            pre = genesis["initial"]
            for row in connection.execute("SELECT * FROM v2_commits ORDER BY sequence"):
                prefix = f"{store}-{row['sequence']}"
                write(prefix + "-state.bin", pre)
                write(prefix + "-command.bin", row["command"])
                write(prefix + "-context.bin", row["context"])
                write(prefix + "-publication.bin", expand(genesis["identity"], row["publication"]))
                pre = row["post"]
    manifest = {"schema": "zeno-fcis/released-24-test-inputs/1", "release": "2.4.0",
                "released_cli_sha256": RELEASED_SHA256, "files": files,
                "pending_ids": recorded["pending_ids"], "authority": "none"}
    (destination / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    return {"status": "generated", "files": len(files), "authority": "none"}


def run(command: list[str | Path], cwd: Path) -> str:
    completed = subprocess.run([str(item) for item in command], cwd=cwd,
                               text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if completed.returncode:
        raise RuntimeError(f"command failed ({completed.returncode}): {command}\n"
                           f"{completed.stdout}\n{completed.stderr}")
    return completed.stdout


def prepare(app: Path, source: Path, version: str) -> dict:
    packages = {tomllib.loads(path.read_text())["package"]["name"]: path.parent
                for path in sorted((source / "crates").glob("*/Cargo.toml"))}
    binding = applications.check_generated_binding(app, packages, version)
    package = tomllib.loads((app / "Cargo.toml").read_text())["package"]
    allowed = {(name, version): path / "Cargo.toml" for name, path in packages.items()}
    allowed[(package["name"], package["version"])] = app / "Cargo.toml"
    metadata = json.loads(run(["cargo", "+1.97.1", "metadata", "--locked", "--offline",
                               "--format-version", "1"], app))
    applications.validate_resolved_graph(tomllib.loads((source / "Cargo.lock").read_text()),
                                        tomllib.loads((app / "Cargo.lock").read_text()),
                                        metadata, allowed)
    return binding


def check(directory: Path, released_cli: Path, released_source: Path, current_cli: Path,
          test_data: Path | None = None) -> dict:
    digest = hashlib.sha256(released_cli.read_bytes()).hexdigest()
    if digest != RELEASED_SHA256 or run([released_cli, "--version"], directory).strip() != "zeno-fcis 2.4.0":
        raise RuntimeError("the supplied executable is not the qualified 2.4.0 release")
    if tomllib.loads((released_source / "Cargo.toml").read_text())["workspace"]["package"]["version"] != "2.4.0":
        raise RuntimeError("historical SDK must be the released 2.4.0 source")
    vectors = applications.ROOT / "crates/zeno-fcis-cli/tests/fixtures/transform-check/3"
    tiny = directory / "released-small-receipt.json"
    run([released_cli, "transform", "check", "--original", vectors / "program-3.zcve",
         "--candidate", vectors / "program-4.zcve", "--receipt", tiny], directory)
    if tiny.read_bytes() != (vectors / "equivalent-usage-differs.receipt.json").read_bytes():
        raise RuntimeError("native receipt-binding fixture differs from the released executable")
    raw = directory / "original-contract"
    for name in ("project.zeno", "v2/policy.json", "v2/schema-origin.json"):
        destination = raw / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(released_source / "crates/zeno-fcis-cli/templates/withdrawal-queue" / name,
                        destination)
    (raw / "tests").mkdir()
    (raw / "tests/decision-examples.txt").write_text(upgrade.SESSION)
    original_app = directory / "released-version-1"
    run([released_cli, "new", original_app, "--contract", raw, "--source", released_source], directory)
    original_binding = prepare(original_app, released_source, "2.4.0")
    original_binary = upgrade.build_app(original_app)
    genesis = directory / "genesis.sqlite"
    away = directory / "pending.sqlite"
    initialized = upgrade.report(upgrade.run_app(original_binary, ["init", str(genesis), "--format", "json"]), "released genesis")
    genesis_before = upgrade.report(upgrade.run_app(original_binary, ["--audit", str(genesis)]), "released genesis audit")
    upgrade.expect(genesis_before, {"contract_version": 1, "commits": 0, "pending": 0, "upgrades": 0}, "released genesis")
    decided = upgrade.report(upgrade.run_app(original_binary, ["--decide", str(away)]), "released non-genesis")
    upgrade.expect(decided, {"decisions": ["Accept"] * 3, "bundles": 3, "pending": 1, "deliveries": 0}, "released non-genesis")
    pending = upgrade.pending_ids(away)
    if len(pending) != 1:
        raise RuntimeError(f"expected one released pending payout: {pending}")
    adopted_contract = directory / "adopted-contract"
    shutil.copytree(raw, adopted_contract)
    original = directory / "original.zcve"
    receipt = directory / "receipt.json"
    candidate = released_source / "crates/zeno-fcis-cli/tests/fixtures/withdrawal-queue-adopted/v2/adoptions/1/program.zcve"
    run([released_cli, "contract", "export-program", raw, "--out", original], directory)
    run([released_cli, "transform", "check", "--original", original, "--candidate", candidate,
         "--receipt", receipt], directory)
    run([released_cli, "contract", "adopt", adopted_contract, "--candidate", candidate,
         "--receipt", receipt, "--usage", "new-version"], directory)
    retained = {name: hashlib.sha256((adopted_contract / name).read_bytes()).hexdigest()
                for name in ("v2/policy_v1.zcve", "v2/adoptions/1/receipt.json", "v2/adoptions/1/program.zcve")}
    released_app = directory / "released-adopted"
    run([released_cli, "new", released_app, "--contract", adopted_contract, "--source", released_source], directory)
    released_binding = prepare(released_app, released_source, "2.4.0")
    created = {"released_cli_sha256": digest, "original_binding": original_binding,
               "adopted_binding": released_binding, "initialized": initialized,
               "genesis_before": genesis_before, "non_genesis": decided,
               "pending_ids": pending, "retained_sha256": retained}
    (directory / "RELEASED_INPUTS.json").write_text(json.dumps(created, indent=2, sort_keys=True) + "\n")
    if test_data is not None:
        export_test_data(directory, test_data)
    current_app = directory / "current-adopted"
    run([current_cli, "new", current_app, "--contract", adopted_contract,
         "--source", applications.ROOT], directory)
    version = tomllib.loads((applications.ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    current_binding = prepare(current_app, applications.ROOT, version)
    current_binary = upgrade.build_app(current_app)
    observed = []
    for store, commits, count in ((genesis, 0, 0), (away, 3, 1)):
        before = upgrade.report(upgrade.run_app(current_binary, ["--audit", str(store)]), "current historical audit")
        upgrade.expect(before, {"contract_version": 1, "commits": commits, "pending": count, "upgrades": 0}, "current historical audit")
        upgraded = upgrade.report(upgrade.run_app(current_binary, ["--upgrade", str(store)]), "current upgrade")
        upgrade.expect(upgraded, {"contract_version": 2, "commits": commits, "pending": count, "upgrades": 1}, "current upgrade")
        after = upgrade.report(upgrade.run_app(current_binary, ["--audit", str(store)]), "current audit")
        upgrade.expect(after, {"contract_version": 2, "commits": commits, "pending": count, "upgrades": 1}, "current audit")
        observed.append({"before": before, "upgrade": upgraded, "after": after})
    if upgrade.pending_ids(away) != pending:
        raise RuntimeError("upgrade dropped or changed the released pending delivery ID")
    continued = upgrade.report(upgrade.run_app(current_binary, ["--decide", str(genesis)]), "decisions after upgrade")
    upgrade.expect(continued, {"decisions": ["Accept"] * 3, "bundles": 3, "pending": 1}, "decisions after upgrade")
    continued_audit = upgrade.report(upgrade.run_app(current_binary, ["--audit", str(genesis)]), "audit after current decisions")
    upgrade.expect(continued_audit, {"contract_version": 2, "commits": 3, "pending": 1, "upgrades": 1}, "audit after current decisions")
    delivered = upgrade.report(upgrade.run_app(current_binary, ["--deliver", str(away)]), "current delivery")
    upgrade.expect(delivered, {"deliveries": pending, "pending": 0}, "current delivery")
    again = upgrade.report(upgrade.run_app(current_binary, ["--deliver", str(away)]), "repeat delivery")
    upgrade.expect(again, {"deliveries": [], "pending": 0}, "repeat delivery")
    fresh = upgrade.report(upgrade.run_app(current_binary, ["--decide", str(directory / "current.sqlite")]), "current decisions")
    upgrade.expect(fresh, {"decisions": ["Accept"] * 3, "bundles": 3, "pending": 1}, "current decisions")
    for name, expected in retained.items():
        if hashlib.sha256((adopted_contract / name).read_bytes()).hexdigest() != expected:
            raise RuntimeError(f"retained evidence was rewritten: {name}")
    return {"schema": "zeno-fcis/contract-upgrade-24-check/1", "status": "passed", "authority": "none",
            "released_inputs": created, "current_binding": current_binding, "stores": observed,
            "delivered": delivered, "again": again, "current_decisions": fresh,
            "post_upgrade_decisions": continued, "post_upgrade_audit": continued_audit,
            "released_small_receipt_sha256": hashlib.sha256(tiny.read_bytes()).hexdigest()}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--released-cli", type=Path, required=True)
    parser.add_argument("--released-source", type=Path, required=True)
    parser.add_argument("--current-cli", type=Path, default=Path(upgrade.cli()))
    parser.add_argument("--keep", type=Path)
    parser.add_argument("--write-test-data", type=Path, help="export exact released inputs for native regression tests")
    parser.add_argument("--fixtures-from", type=Path, help="export inputs from a retained released run; this runs no upgrade check")
    args = parser.parse_args()
    os.environ.setdefault("CARGO_BUILD_JOBS", "2")
    if args.fixtures_from:
        if args.write_test_data is None or args.keep is not None:
            parser.error("--fixtures-from requires --write-test-data and excludes --keep")
        if (hashlib.sha256(args.released_cli.read_bytes()).hexdigest() != RELEASED_SHA256
                or run([args.released_cli.resolve(), "--version"], args.fixtures_from).strip() != "zeno-fcis 2.4.0"):
            raise RuntimeError("fixture export requires the qualified released executable")
        print(json.dumps(export_test_data(args.fixtures_from.resolve(), args.write_test_data.resolve()), sort_keys=True))
        return 0
    if args.keep:
        args.keep.mkdir()
        result = check(args.keep.resolve(), args.released_cli.resolve(), args.released_source.resolve(), args.current_cli.resolve(), args.write_test_data)
    else:
        with tempfile.TemporaryDirectory(prefix="contract-upgrade-24-") as directory:
            result = check(Path(directory), args.released_cli.resolve(), args.released_source.resolve(), args.current_cli.resolve(), args.write_test_data)
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        from telemetry import run_cli
    except ImportError:
        # Reporting is optional, including for a standalone copy of this tool.
        run_cli = lambda function: function()
    def _entrypoint():
        raise SystemExit(main())
    run_cli(_entrypoint)
