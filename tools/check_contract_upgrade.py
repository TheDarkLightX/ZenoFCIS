#!/usr/bin/env python3
"""Upgrade withdrawal-queue stores through the application command line.

Two stores run contract version 1, the withdrawal-queue template's contract:
- the template application's journey leaves one at the genesis state with
  twelve commits and both payouts delivered;
- an application built from the template's contract runs three decisions
  with `--decide`: a deposit, a request and a paying tick. That leaves its
  store away from the genesis state, with priority on lane B and lane A's
  payout still pending.

An application built from the adopted withdrawal-queue contract (version 2,
the 100-node candidate) then audits each store at version 1 and upgrades it
with `--upgrade`. The store's shell establishes every premise of a program
succession itself, comparing the two decision programs on every input tuple,
so the store away from genesis upgrades too; the number of tuples it reports
must be the adoption receipt's own count. It delivers the pending payout with `--deliver` under
the ID that version 1's commit bound, exactly once, and refuses a second
upgrade and a migration. The version 1 build then refuses the upgraded store.
Dependencies resolve exactly as in check_generated_application.py: only the
new consumer locks change, against the reviewed external graph.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

import check_generated_application as applications

ROOT = applications.ROOT
TEMPLATE = ROOT / "crates/zeno-fcis-cli/templates/withdrawal-queue"
FIXTURE = ROOT / "crates/zeno-fcis-cli/tests/fixtures/withdrawal-queue-adopted"
EXPECTED_JOURNEY = {"status": "passed", "bundles": 12, "pending": 0, "deliveries": 2, "balance": 0}


def premises(tuples: int) -> dict:
    """The premises a program-successor upgrade reports: all five, with the
    number of input tuples on which the shell compared the two programs."""
    return {"policy_differs_only_in_program_and_step_limit": True,
            "decision_law_991_required": True, "programs_equal_on_input_tuples": tuples,
            "step_limits_never_bind": True, "step_usage_unobserved": True}


def receipt_tuples(receipt: Path) -> int:
    """The input tuples an adoption's `transform` receipt says F3 checked."""
    return json.loads(receipt.read_text())["inputs_checked"]
# A deposit of 2, lane A's request for 2, and the tick that pays it: the state
# ends with an empty vault and priority on lane B, away from genesis.
SESSION = """\
# Three decisions that leave lane A's payout pending away from genesis.
0 180 0 180 0 0 0 170 160 170 2 190 0 | accept - 2 180 0 180 0 0 0 170 | -
2 180 0 180 0 0 0 170 161 170 2 191 0 | accept - 2 181 2 180 0 0 0 170 | -
2 181 2 180 0 0 0 170 162 170 1 193 0 | accept - 0 180 0 180 0 0 0 171 | 300 170 2
"""


def cli() -> str:
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    return str((ROOT / target / "debug/zeno-fcis").resolve())


def prepare(app: Path, packages: dict[str, Path], version: str) -> dict:
    """Check the binding to this checkout that `zeno-fcis new` wrote and admit
    the resolved graph."""
    applications.check_generated_binding(app, packages, version)
    allowed = {(name, version): path / "Cargo.toml" for name, path in packages.items()}
    consumer = tomllib.loads((app / "Cargo.toml").read_text())["package"]
    allowed[(consumer["name"], consumer["version"])] = app / "Cargo.toml"
    return applications.resolve_reviewed_graph(app, allowed, dict(os.environ))


def build_app(app: Path) -> Path:
    """Build an application once and keep its binary under a name of its own.

    The three applications share the target directory and the package name
    `withdrawal-queue`, so the binary cargo leaves in the target directory is
    whichever one was linked last; `cargo run` could execute another
    application's build. Each application's first build here is its only one.
    """
    applications.run(["cargo", "+1.97.1", "build", "--locked", "--offline"], app)
    package = tomllib.loads((app / "Cargo.toml").read_text())["package"]["name"]
    # Cargo reads a relative target directory from where it runs.
    target = Path(os.environ["CARGO_TARGET_DIR"])
    target = target if target.is_absolute() else app / target
    binary = app / f"{package}.bin"
    shutil.copy2(target / "debug" / package, binary)
    return binary


def run_app(binary: Path, arguments: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run([str(binary), *arguments], check=False, text=True,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def report(completed: subprocess.CompletedProcess[str], label: str) -> dict:
    if completed.returncode != 0:
        raise RuntimeError(f"{label} failed: {completed.stderr}")
    return json.loads(completed.stdout)


def expect(actual: dict, expected: dict, label: str) -> None:
    if any(actual.get(key) != value for key, value in expected.items()):
        raise RuntimeError(f"{label} differs: {actual}, expected {expected}")


def expect_refusal(completed: subprocess.CompletedProcess[str], word: str, label: str) -> None:
    if completed.returncode != 1 or word not in completed.stderr:
        raise RuntimeError(f"{label}: expected a refusal naming {word}: "
                           f"exit {completed.returncode}, stderr {completed.stderr.strip()!r}")


def pending_ids(store: Path) -> list[str]:
    """The unacknowledged delivery IDs, in delivery order, read without writing."""
    with sqlite3.connect(f"file:{store}?mode=ro", uri=True) as connection:
        rows = connection.execute(
            "SELECT delivery_id FROM v2_deliveries WHERE acknowledged=0 "
            "ORDER BY sequence,lane,ordinal").fetchall()
    return [bytes(row[0]).hex() for row in rows]


def check(directory: Path) -> dict:
    applications.run(["cargo", "+1.97.1", "build", "-p", "zeno-fcis-cli", "--locked", "--offline"], ROOT)
    packages = {tomllib.loads(manifest.read_text())["package"]["name"]: manifest.parent
                for manifest in sorted((ROOT / "crates").glob("*/Cargo.toml"))}
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    receipts = [adoption["receipt_sha256"] for adoption in
                json.loads((FIXTURE / "v2/policy.json").read_text())["adoptions"]]
    held = premises(receipt_tuples(FIXTURE / "v2/adoptions/1/receipt.json"))

    # Version 1 through the template application: a store at genesis.
    template = directory / "withdrawal-queue"
    applications.run([cli(), "new", str(template), "--template", "withdrawal-queue"], ROOT)
    template_graph = prepare(template, packages, version)
    template = build_app(template)
    at_genesis = directory / "vault.sqlite"
    journey = report(run_app(template, [str(at_genesis)]), "journey")
    expect(journey, EXPECTED_JOURNEY, "journey")

    # Version 1 through an application built from the template's contract: a
    # store away from genesis with a pending payout.
    contract = directory / "version-1-contract"
    for name in ("project.zeno", "v2/policy.json", "v2/schema-origin.json"):
        (contract / name).parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(TEMPLATE / name, contract / name)
    (contract / "tests").mkdir()
    (contract / "tests/decision-examples.txt").write_text(SESSION)
    first = directory / "version-1"
    applications.run([cli(), "new", str(first), "--contract", str(contract)], ROOT)
    applications.run([cli(), "generate", "contract", str(first), "--check"], ROOT)
    first_graph = prepare(first, packages, version)
    first = build_app(first)
    away = directory / "paid.sqlite"
    decided = report(run_app(first, ["--decide", str(away)]), "decide")
    expect(decided, {"status": "passed", "decisions": ["Accept"] * 3, "bundles": 3,
                     "pending": 1, "deliveries": 0}, "decide")
    pending = pending_ids(away)
    if len(pending) != 1:
        raise RuntimeError(f"expected one pending payout, found {pending}")

    adopted = directory / "adopted"
    applications.run([cli(), "new", str(adopted), "--contract", str(FIXTURE)], ROOT)
    applications.run([cli(), "generate", "contract", str(adopted), "--check"], ROOT)
    adopted_graph = prepare(adopted, packages, version)
    adopted = build_app(adopted)

    # The adopted build audits a store at version 1 and reports that version.
    before = report(run_app(adopted, ["--audit", str(away)]), "audit before upgrade")
    expect(before, {"status": "audited", "contract_version": 1, "commits": 3, "pending": 1,
                    "upgrades": 0}, "audit before upgrade")
    # A store at version 1 delivers nothing through this build until it is upgraded.
    expect_refusal(run_app(adopted, ["--deliver", str(away)]), "upgrade it", "deliver before upgrade")
    upgraded = report(run_app(adopted, ["--upgrade", str(away)]), "upgrade away from genesis")
    expect(upgraded, {"status": "upgraded", "kind": "program-successor", "premises": held,
                      "ordinal": 1, "at_commit": 3, "receipts": receipts, "contract_version": 2,
                      "commits": 3, "pending": 1, "upgrades": 1}, "upgrade away from genesis")
    audited = report(run_app(adopted, ["--audit", str(away)]), "audit after upgrade")
    expect(audited, {"status": "audited", "contract_version": 2, "commits": 3, "pending": 1,
                     "upgrades": 1}, "audit after upgrade")
    if pending_ids(away) != pending:
        raise RuntimeError("the upgrade changed the pending delivery")
    delivered = report(run_app(adopted, ["--deliver", str(away)]), "deliver")
    expect(delivered, {"status": "delivered", "deliveries": pending, "contract_version": 2,
                       "commits": 3, "pending": 0, "upgrades": 1}, "deliver")
    again = report(run_app(adopted, ["--deliver", str(away)]), "deliver again")
    expect(again, {"status": "delivered", "deliveries": [], "pending": 0}, "deliver again")
    expect_refusal(run_app(adopted, ["--upgrade", str(away)]), "SameContract", "second upgrade")
    expect_refusal(run_app(adopted, ["--migrate", str(away)]), "Schema(10)", "migrating a v10 store")
    # The version 1 build cannot replay the version 2 segment.
    expect_refusal(run_app(first, ["--audit", str(away)]), "Identity", "version 1 over the upgraded store")
    expect_refusal(run_app(first, ["--decide", str(away)]), "Identity", "a session over the upgraded store")

    # The template's store at genesis upgrades the same way.
    genesis_audit = report(run_app(adopted, ["--audit", str(at_genesis)]), "audit at genesis")
    expect(genesis_audit, {"contract_version": 1, "commits": 12, "pending": 0, "upgrades": 0},
           "audit at genesis")
    genesis_upgrade = report(run_app(adopted, ["--upgrade", str(at_genesis)]), "upgrade at genesis")
    expect(genesis_upgrade, {"kind": "program-successor", "premises": held, "at_commit": 12,
                             "contract_version": 2, "pending": 0, "upgrades": 1}, "upgrade at genesis")
    expect_refusal(run_app(template, [str(at_genesis)]), "Schema", "the template over the upgraded store")
    return {"schema": "zeno-fcis/contract-upgrade-check/2", "status": "passed", "authority": "none",
            "journey": journey, "decided": decided, "audit_before": before, "upgrade": upgraded,
            "audit": audited, "delivered": delivered, "genesis_upgrade": genesis_upgrade,
            "template_graph_lock_sha256": template_graph["lock_sha256"],
            "version_1_graph_lock_sha256": first_graph["lock_sha256"],
            "adopted_graph_lock_sha256": adopted_graph["lock_sha256"]}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--keep", type=Path, help="retain artifacts in a new directory")
    args = parser.parse_args()
    os.environ.setdefault("CARGO_BUILD_JOBS", "2")
    os.environ.setdefault("CARGO_INCREMENTAL", "0")
    os.environ.setdefault("CARGO_PROFILE_DEV_DEBUG", "0")
    os.environ.setdefault("CARGO_PROFILE_TEST_DEBUG", "0")
    os.environ.setdefault("CARGO_TARGET_DIR", str(ROOT / "target"))
    if args.keep:
        directory = args.keep.resolve()
        directory.mkdir()
        result = check(directory)
    else:
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-contract-upgrade-") as directory:
            result = check(Path(directory))
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
