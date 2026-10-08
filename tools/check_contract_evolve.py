#!/usr/bin/env python3
"""Change the rules of a live escrow store through the command line alone.

The study's rule change moves the escrow's dispute window from 14 to 30 days.
Version 1 is the escrow fixture's contract. An application built from it runs
a session with `--decide`: the buyer funds, the seller ships, and a dispute
14 days and one second after shipping is refused as too late. The store then
holds two commits, at the shipped state, away from genesis.

`zeno-fcis contract evolve` then replaces a new application's contract with
`tests/fixtures/escrow-dispute-30`, which the classifier calls a rule change,
and records its plain-language diff as the owner's review. That build:
- audits the version 1 store without changing a byte of it;
- upgrades it with `--upgrade`: a behaviour change, admitted because every
  state law of the new contract holds on the store's state; genesis
  exactness, law 990, is reported as not evaluated, and the record binds the
  review's SHA-256;
- keeps committing: the same dispute is now accepted, the arbiter splits the
  escrow, and both payouts are delivered;
- audits both segments, each under its own contract.

A second lineage checks inductive claims: a withdrawal-queue store with three
decisions, away from genesis, evolves to the same contract with another
genesis state, a rule change whose law 990 the store's state does not
satisfy: its pause counter is 0, and the new genesis state's is 1. The
upgrade is admitted because law 990 is not evaluated, and it reports both of
the contract's inductive claims as holding on the store's state.

These are refused, each without changing the store's bytes:
- an upgrade to a rule change whose new state law fails on the store's state
  (a law that no shipped escrow may exist), naming the law;
- the upgraded store opened by the version 1 build;
- a copy of the upgraded store whose stored review text was altered.

Dependencies resolve exactly as in check_generated_application.py, and each
application runs its own binary copy: they share a package name.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

import check_contract_upgrade as upgrades
import check_generated_application as applications

ROOT = applications.ROOT
ESCROW = ROOT / "crates/zeno-fcis-cli/tests/fixtures/escrow"
QUEUE = ROOT / "crates/zeno-fcis-cli/templates/withdrawal-queue"
THIRTY_DAYS = ROOT / "crates/zeno-fcis-cli/tests/fixtures/escrow-dispute-30"

# Fund, ship, and a dispute 14 days and one second after shipping, which the
# 14-day window refuses: two commits, at the shipped state.
SESSION = """\
# Fund and ship; a dispute 14 days and one second after shipping is late.
150 0 0 0 0 0 0 170 5000 0 161 1000000 | accept - 151 5000 5000 0 0 1000000 0 | -
151 5000 5000 0 0 1000000 0 171 0 0 162 1100000 | accept - 152 5000 5000 0 0 1000000 1100000 | -
152 5000 5000 0 0 1000000 1100000 173 0 0 161 2309601 | reject 205 152 5000 5000 0 0 1000000 1100000 | -
"""

# A state law that no shipped escrow satisfies, for the refused upgrade.
BAD_LAW = """
// No escrow may wait in the shipped state.
law 505 no_escrow_waits_shipped on commit, genesis = post.100.110 != 152;
"""


def cli() -> str:
    return upgrades.cli()


def zeno(arguments: list[str]) -> dict:
    """One `zeno-fcis` command that must succeed and print a JSON report."""
    completed = subprocess.run([cli(), *arguments], check=False, text=True,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    return upgrades.report(completed, " ".join(arguments[:2]))


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def refused_without_write(binary: Path, arguments: list[str], store: Path, word: str,
                          label: str) -> None:
    """The command refuses, naming `word`, and leaves the store's bytes as they were."""
    before = store.read_bytes()
    upgrades.expect_refusal(upgrades.run_app(binary, arguments), word, label)
    if store.read_bytes() != before:
        raise RuntimeError(f"{label}: the refusal changed the store")


def contract(directory: Path, base: Path, examples: str | None = None) -> Path:
    """A contract directory: `base`'s declarations and rules, and examples."""
    for name in ("project.zeno", "v2/policy.json"):
        (directory / name).parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(base / name, directory / name)
    (directory / "tests").mkdir(exist_ok=True)
    if examples is None:
        shutil.copyfile(base / "tests/decision-examples.txt",
                        directory / "tests/decision-examples.txt")
    else:
        (directory / "tests/decision-examples.txt").write_text(examples)
    return directory


def evolved_application(root: Path, name: str, version_1: Path, to: Path,
                        packages: dict[str, Path], version: str) -> tuple[Path, dict]:
    """An application created from version 1, evolved to `to`, checked and
    built at once; see `check_contract_upgrade.build_app`."""
    app = root / name
    applications.run([cli(), "new", str(app), "--contract", str(version_1)], ROOT)
    evolved = zeno(["contract", "evolve", str(app), "--to", str(to), "--format", "json"])
    upgrades.expect(evolved, {"status": "evolved", "kind": "rule-change"}, f"evolve {name}")
    zeno(["generate", "contract", str(app), "--check", "--format", "json"])
    upgrades.prepare(app, packages, version)
    return upgrades.build_app(app), evolved


def check(directory: Path) -> dict:
    applications.run(["cargo", "+1.97.1", "build", "-p", "zeno-fcis-cli", "--locked", "--offline"], ROOT)
    packages = {tomllib.loads(manifest.read_text())["package"]["name"]: manifest.parent
                for manifest in sorted((ROOT / "crates").glob("*/Cargo.toml"))}
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]

    # The classifier calls the study's change a rule change.
    classified = zeno(["contract", "diff", str(ESCROW), str(THIRTY_DAYS), "--format", "json"])
    upgrades.expect(classified, {"kind": "rule-change"}, "contract diff")

    # Version 1: a store with committed history at the shipped state.
    version_1 = contract(directory / "version-1-contract", ESCROW, SESSION)
    first = directory / "version-1"
    applications.run([cli(), "new", str(first), "--contract", str(version_1)], ROOT)
    upgrades.prepare(first, packages, version)
    first = upgrades.build_app(first)
    store = directory / "escrow.sqlite"
    decided = upgrades.report(upgrades.run_app(first, ["--decide", str(store)]), "decide")
    upgrades.expect(decided, {"status": "passed", "decisions": ["Accept", "Accept", "Reject"],
                              "bundles": 2, "pending": 0}, "decide under version 1")

    # Version 2: the 30-day window, evolved and built.
    second, evolved = evolved_application(directory, "version-2", version_1, THIRTY_DAYS,
                                          packages, version)
    upgrades.expect(evolved["evolution"], {"ordinal": 1, "from_version": 1, "version": 2},
                    "evolution")
    review = directory / "version-2/v2/evolutions/1/review.txt"
    if sha256(review) != evolved["evolution"]["review_sha256"]:
        raise RuntimeError("the review's digest differs from the evolution's")

    # A rule change whose new state law fails on the store's state.
    bad = contract(directory / "bad-contract", THIRTY_DAYS)
    project = (bad / "project.zeno").read_text()
    (bad / "project.zeno").write_text(project + BAD_LAW)
    rules = json.loads((bad / "v2/policy.json").read_text())
    rules["law_kinds"]["505"] = "StateInvariant"
    (bad / "v2/policy.json").write_text(json.dumps(rules, indent=2) + "\n")
    third, _ = evolved_application(directory, "bad-law", version_1, bad, packages, version)
    refused_without_write(third, ["--upgrade", str(store)], store, "state law 505",
                          "upgrade to a law the store breaks")

    # The new build audits the old store without writing to it.
    before = sha256(store)
    audit = upgrades.report(upgrades.run_app(second, ["--audit", str(store)]), "audit before upgrade")
    upgrades.expect(audit, {"status": "audited", "contract_version": 1, "commits": 2,
                            "pending": 0, "upgrades": 0}, "audit before upgrade")
    if sha256(store) != before:
        raise RuntimeError("auditing the old store changed it")
    refused_without_write(second, ["--decide", str(store)], store, "upgrade it",
                          "deciding before the upgrade")

    upgraded = upgrades.report(upgrades.run_app(second, ["--upgrade", str(store)]), "upgrade")
    upgrades.expect(upgraded, {
        "status": "upgraded", "kind": "behaviour-change", "premises": None, "ordinal": 1,
        "at_commit": 2, "receipts": [], "contract_version": 2, "commits": 2, "upgrades": 1,
        "behaviour": {"state_laws_held": [500, 501], "claims_held": [], "not_evaluated": [990],
                      "reviews": [evolved["evolution"]["review_sha256"]]},
    }, "upgrade")
    with sqlite3.connect(f"file:{store}?mode=ro", uri=True) as connection:
        stored = connection.execute("SELECT publication FROM v2_upgrades WHERE ordinal=1").fetchone()[0]
    text = review.read_bytes()
    if bytes(stored) != len(text).to_bytes(8, "big") + text:
        raise RuntimeError("the upgrade row does not store the review text")

    # The store keeps committing: the late dispute is now in the window.
    continued = upgrades.report(upgrades.run_app(second, ["--decide", str(store)]), "decide after")
    upgrades.expect(continued, {
        "status": "passed",
        "decisions": ["Reject", "Reject", "Accept", "Reject", "Reject", "Accept", "Reject"],
        "bundles": 4, "pending": 2}, "decide under version 2")
    delivered = upgrades.report(upgrades.run_app(second, ["--deliver", str(store)]), "deliver")
    upgrades.expect(delivered, {"status": "delivered", "contract_version": 2, "commits": 4,
                                "pending": 0, "upgrades": 1}, "deliver")
    if len(delivered["deliveries"]) != 2:
        raise RuntimeError(f"expected the split's two payouts: {delivered}")
    audited = upgrades.report(upgrades.run_app(second, ["--audit", str(store)]), "audit both")
    upgrades.expect(audited, {"status": "audited", "contract_version": 2, "commits": 4,
                              "pending": 0, "upgrades": 1}, "audit both segments")
    refused_without_write(first, ["--audit", str(store)], store, "Identity",
                          "version 1 over the upgraded store")

    # A stored review that differs from the one the record binds is damage.
    forged = directory / "forged.sqlite"
    shutil.copyfile(store, forged)
    with sqlite3.connect(forged) as connection:
        publication = bytearray(connection.execute(
            "SELECT publication FROM v2_upgrades WHERE ordinal=1").fetchone()[0])
        publication[-2] ^= 1
        connection.execute("UPDATE v2_upgrades SET publication=? WHERE ordinal=1",
                           (bytes(publication),))
    refused_without_write(second, ["--audit", str(forged)], forged, "History",
                          "a forged review")
    claims = claims_journey(directory, packages, version)
    return {"schema": "zeno-fcis/contract-evolve-check/1", "status": "passed", "authority": "none",
            "classified": classified["kind"], "decided": decided,
            "evolution": evolved["evolution"], "audit_before": audit, "upgrade": upgraded,
            "continued": continued, "delivered": delivered, "audit": audited,
            "claims": claims}


def claims_journey(directory: Path, packages: dict[str, Path], version: str) -> dict:
    """A withdrawal-queue store upgrades across a genesis change, its claims checked."""
    root = directory / "withdrawal-queue"
    root.mkdir()
    version_1 = contract(root / "version-1-contract", QUEUE, upgrades.SESSION)
    shutil.copyfile(QUEUE / "v2/schema-origin.json", version_1 / "v2/schema-origin.json")
    first = root / "version-1"
    applications.run([cli(), "new", str(first), "--contract", str(version_1)], ROOT)
    upgrades.prepare(first, packages, version)
    first = upgrades.build_app(first)
    store = root / "queue.sqlite"
    decided = upgrades.report(upgrades.run_app(first, ["--decide", str(store)]), "decide")
    upgrades.expect(decided, {"decisions": ["Accept"] * 3, "bundles": 3, "pending": 1},
                    "decide under version 1")
    # Another genesis: the pause counter starts at 1. Law 990 changes with
    # it, and the store's state, whose counter is 0, does not satisfy it.
    changed = contract(root / "genesis-changed", QUEUE, upgrades.SESSION)
    rules = json.loads((changed / "v2/policy.json").read_text())
    rules["genesis"]["125"] = 1
    (changed / "v2/policy.json").write_text(json.dumps(rules, indent=2) + "\n")
    second, evolved = evolved_application(root, "version-2", version_1, changed, packages, version)
    upgrades.expect(evolved["evolution"], {"version": 2, "claims": [600, 601]}, "evolution")
    upgraded = upgrades.report(upgrades.run_app(second, ["--upgrade", str(store)]), "upgrade")
    upgrades.expect(upgraded, {
        "kind": "behaviour-change", "at_commit": 3, "contract_version": 2, "pending": 1,
        "behaviour": {"state_laws_held": [500], "claims_held": [600, 601],
                      "not_evaluated": [990],
                      "reviews": [evolved["evolution"]["review_sha256"]]},
    }, "upgrade with claims")
    audited = upgrades.report(upgrades.run_app(second, ["--audit", str(store)]), "audit")
    upgrades.expect(audited, {"contract_version": 2, "commits": 3, "pending": 1, "upgrades": 1},
                    "audit with claims")
    return {"decided": decided, "evolution": evolved["evolution"], "upgrade": upgraded,
            "audit": audited}


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
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-contract-evolve-") as directory:
            result = check(Path(directory))
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"contract evolve: FAIL: {error}", file=sys.stderr)
        sys.exit(1)
