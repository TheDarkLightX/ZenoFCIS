#!/usr/bin/env python3
"""Exercise delivery accounting in emitted Rust applications and durable stores.

The ordinary generator builds the retained escrow with its payout law, then a
second application with a planted double payout. The latter must refuse the
split with law 510 and leave both the store and submission journal unchanged.
This checks the installed application path, not a second policy evaluator.
The retained examples are AI-authored fixtures, not independent owner labels.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import tomllib

import check_app_journey as journeys
import check_contract_upgrade as upgrades
import check_generated_application as applications

ROOT = applications.ROOT
FIXTURES = ROOT / "crates/zeno-fcis-cli/tests/fixtures"
OVERLAY = FIXTURES / "delivery-laws"


def fixture(directory: Path, *, double_payout: bool) -> Path:
    """Keep the original schema/examples; change only the declared overlay/case."""
    shutil.copytree(FIXTURES / "escrow", directory)
    project = directory / "project.zeno"
    project.write_text(project.read_text() + "\n" + (OVERLAY / "escrow-law.zeno").read_text())
    policy = directory / "v2/policy.json"
    rules = json.loads(policy.read_text())
    rules["delivery_laws"] = json.loads((OVERLAY / "escrow-law.json").read_text())
    rules["law_kinds"]["510"] = "AssetConservation"
    if double_payout:
        split = [case for case in rules["cases"] if case["rule"] == "resolve: split"]
        if len(split) != 1 or len(split[0]["outbox"]) != 2:
            raise RuntimeError("escrow fixture lost its single two-payment split")
        for delivery in split[0]["outbox"]:
            delivery["payload"]["141"] = "held"
    policy.write_text(json.dumps(rules, indent=2) + "\n")
    return directory


def app_report(binary: Path, arguments: list[str], label: str) -> dict:
    return upgrades.report(upgrades.run_app(binary, arguments), label)


def submit(binary: Path, store: Path, action: int, caller: int, now: int,
           *, amount: int = 0, refund: int = 0) -> dict:
    report = app_report(binary, [
        "submit", str(store), f"command.action={action}",
        f"command.amount={amount}", f"command.refund={refund}",
        f"context.caller={caller}", f"context.now={now}", "--format", "json",
    ], "escrow submit")
    upgrades.expect(report, {"status": "committed", "class": "Accept"}, "escrow submit")
    return report


def prefix(binary: Path, store: Path) -> None:
    app_report(binary, ["init", str(store), "--format", "json"], "escrow init")
    for action, caller, now, amount in ((170, 161, 1000000, 5000),
                                        (171, 162, 1100000, 0),
                                        (173, 161, 2309600, 0)):
        submit(binary, store, action, caller, now, amount=amount)
    upgrades.expect(app_report(binary, ["--audit", str(store)], "prefix audit"),
                    {"commits": 3, "pending": 0}, "prefix audit")


def custody(store: Path) -> dict[str, bytes]:
    """Include the durable journal and SQLite sidecars, including absence."""
    return {path.name: path.read_bytes() for path in sorted(store.parent.iterdir())
            if path.name.startswith(store.name) and path.is_file()}


def check(directory: Path) -> dict:
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    os.environ["CARGO_TARGET_DIR"] = str((ROOT / target).resolve())
    applications.run(["cargo", "+1.97.1", "build", "-p", "zeno-fcis-cli",
                      "--locked", "--offline"], ROOT)
    packages = {tomllib.loads(path.read_text())["package"]["name"]: path.parent
                for path in sorted((ROOT / "crates").glob("*/Cargo.toml"))}
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    contracts = [fixture(directory / name, double_payout=bad)
                 for name, bad in (("correct-contract", False), ("double-payout-contract", True))]
    apps = []
    bindings = []
    graphs = []
    for contract, name in zip(contracts, ("correct-app", "double-payout-app")):
        app = directory / name
        applications.run([upgrades.cli(), "new", str(app), "--contract", str(contract)], ROOT)
        bindings.append(applications.check_generated_binding(app, packages, version))
        graphs.append(upgrades.prepare(app, packages, version))
        apps.append(app)

    # Follow the correct app's actual README commands, not a handwritten core.
    good = apps[0]
    commands = journeys.readme_commands(good)
    if [command[:2] for command in commands] != [["cargo", "test"], ["cargo", "run"]]:
        raise RuntimeError("escrow README no longer supplies its test/run journey")
    readme_store = directory / "readme.sqlite"
    environment = {key: value for key, value in os.environ.items() if key != "RUSTUP_TOOLCHAIN"}
    # Newly generated applications with the same package name need distinct caches.
    environment["CARGO_TARGET_DIR"] = str(good / "target")
    outputs = [applications.run(journeys.as_written(command, readme_store), good,
                                capture=True, environment=environment) for command in commands]
    print(outputs[0], end="")
    if "test every_example_is_the_authority_decision ... ok" not in outputs[0]:
        raise RuntimeError("the emitted application did not run its decision examples")
    summary = json.loads(outputs[1].strip().splitlines()[-1])
    upgrades.expect(summary, journeys.ESCROW_SUMMARY, "delivery-law README journey")
    original_target = os.environ["CARGO_TARGET_DIR"]
    try:
        os.environ["CARGO_TARGET_DIR"] = str(good / "target")
        good_binary = upgrades.build_app(good)
    finally:
        os.environ["CARGO_TARGET_DIR"] = original_target
    audited = app_report(good_binary, ["--audit", str(readme_store)], "README audit")
    upgrades.expect(audited, {"status": "audited", "commits": 4, "pending": 0}, "README audit")

    correct_store = directory / "correct.sqlite"
    prefix(good_binary, correct_store)
    split = submit(good_binary, correct_store, 174, 163, 2400000, refund=2000)
    expected_deliveries = [
        {"channel": 300, "payload": {"payee": "Buyer", "payout_amount": 2000}},
        {"channel": 300, "payload": {"payee": "Seller", "payout_amount": 3000}},
    ]
    if split["deliveries"] != expected_deliveries:
        raise RuntimeError(f"actual split deliveries differ: {split['deliveries']}")
    upgrades.expect(split["state"], {"held": 0, "to_seller": 3000, "to_buyer": 2000},
                    "actual payout counters")
    correct_audit = app_report(good_binary, ["--audit", str(correct_store)], "correct audit")
    upgrades.expect(correct_audit, {"commits": 4, "pending": 2}, "correct audit")

    try:
        os.environ["CARGO_TARGET_DIR"] = str(apps[1] / "target")
        bad_binary = upgrades.build_app(apps[1])
    finally:
        os.environ["CARGO_TARGET_DIR"] = original_target
    binary_digests = {"correct": hashlib.sha256(good_binary.read_bytes()).hexdigest(),
                      "double_payout": hashlib.sha256(bad_binary.read_bytes()).hexdigest()}
    if binary_digests["correct"] == binary_digests["double_payout"]:
        raise RuntimeError("the double-payout control reused the correct binary")
    bad_store = directory / "double-payout.sqlite"
    prefix(bad_binary, bad_store)
    before = custody(bad_store)
    if bad_store.name + ".submissions" not in before:
        raise RuntimeError("missing durable journal before the refusal control")
    arguments = [str(bad_store), "command.action=174", "command.amount=0",
                 "command.refund=2000", "context.caller=163", "context.now=2400000",
                 "--format", "json"]
    refusals = []
    for operation in ("decide", "submit"):
        failed = upgrades.run_app(bad_binary, [operation, *arguments])
        refusal = json.loads(failed.stdout)
        error = refusal.get("error", {})
        message = error.get("message", "")
        if (failed.returncode == 0 or refusal.get("status") != "error"
                or error.get("code") != "refused" or "510" not in message
                or "Violated" not in message):
            raise RuntimeError(f"{operation}: expected law510 Violation; got "
                               f"exit{failed.returncode}, {refusal}, {failed.stderr}")
        if custody(bad_store) != before:
            raise RuntimeError(f"{operation} changed the refused store/journal/sidecars")
        refusals.append({"operation": operation, "exit": failed.returncode, "report": refusal})
    bad_audit = app_report(bad_binary, ["--audit", str(bad_store)], "refused-store audit")
    upgrades.expect(bad_audit, {"status": "audited", "commits": 3, "pending": 0},
                    "refused-store audit")
    if custody(bad_store) != before:
        raise RuntimeError("audit changed the refused store or submission journal")
    return {"status": "passed", "scope": "generated Rust and durable delivery-law journey",
            "bindings": bindings, "graphs": graphs, "readme": summary, "readme_audit": audited,
            "correct_split": split, "correct_audit": correct_audit, "refusals": refusals,
            "refused_audit": bad_audit,
            "refused_files_sha256": {name: hashlib.sha256(data).hexdigest()
                                      for name, data in before.items()},
            "binary_sha256": binary_digests,
            "independent_owner_labels": False, "new_formal_proof": False}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work-dir", type=Path, help="retain artifacts in a new external directory")
    args = parser.parse_args()
    try:
        if args.work_dir is not None:
            directory = args.work_dir.resolve()
            if directory.is_relative_to(ROOT) or args.work_dir.is_symlink():
                raise RuntimeError("work directory must be a new external directory")
            directory.mkdir(parents=True, exist_ok=False)
            report = check(directory)
        else:
            with tempfile.TemporaryDirectory(prefix="zeno-delivery-laws-") as temporary:
                report = check(Path(temporary))
        print(json.dumps(report, sort_keys=True))
        return 0
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"delivery laws: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
