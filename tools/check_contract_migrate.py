#!/usr/bin/env python3
"""Migrate the data of live spend-approval stores through the command line alone.

Version 1 is the spend-approval fixture's contract. Version 2,
`tests/fixtures/spend-approval-priority`, adds a state field, the urgent
flag, which every case carries over: the classifier calls the change a
`layout-change`. `zeno-fcis contract evolve --migration` takes it with the
fixture's `migration.json`, which carries the four old fields and gives the
new one the default `false`, and admits it only because forward simulation
over version 1's whole declared input domain finds every observation kept.

Two version 1 stores are made, because an executed spend-approval request is
final and commits nothing more:
- store A, from a session that creates, approves and executes a tier 1
  request: three commits and one payment pending;
- store B, from a session that only creates the request: one commit, the
  request pending approval.

The version 2 build audits both without changing them, then upgrades each
with `--upgrade`, recorded as a `migration` that names what the shell's own
simulation compared. Store A's pending payment is then delivered with the
delivery ID its version 1 commit bound, and store B keeps committing under
version 2: the CFO approves, the clerk executes, and the new payment is
delivered. Both stores then audit, every segment under its own contract.

Version 3 renames the urgent flag, a `rename`, which needs no migration and
is admitted at any state: store A upgrades again and its three segments
replay.

These are refused by `contract evolve`, each without changing a byte of the
application:
- a migration under which the new contract pays a different tier (a broken
  delivery);
- a migration to a contract that refuses a request created by anyone but
  the clerk with another reason (a changed decision);
- a migration that sets the new flag to true, so the old genesis state does
  not map to the new one;
- a migration to a contract with a new state law, that a pending tier 0
  request carries no CEO approval, which a state version 1's laws allow
  breaks once migrated, although no version 1 commit reaches it;
- a shortcut from version 1 that disagrees with the composed route of two
  consecutive migrations (an agreeing one is admitted);
- a migration of an escrow application, whose input domain is above the
  simulation's cap of 2^20 tuples;
- a layout change without a migration.

The version 1 build refuses the migrated store. Dependencies resolve exactly
as in check_generated_application.py, and each application runs its own
binary copy: they share a package name.
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

import check_contract_upgrade as upgrades
import check_generated_application as applications

ROOT = applications.ROOT
FIXTURES = ROOT / "crates/zeno-fcis-cli/tests/fixtures"
SPEND = FIXTURES / "spend-approval"
PRIORITY = FIXTURES / "spend-approval-priority"
ESCROW = FIXTURES / "escrow"
ESCROW_30 = FIXTURES / "escrow-dispute-30"

# Store A: a tier 1 request created, approved by the CFO and executed; the
# payment stays pending.
EXECUTED = """\
# Create, approve and execute a tier 1 request.
150 0 0 0 170 1 161 0 1 | accept - 151 1 0 0 | -
151 1 0 0 171 0 162 0 1 | accept - 151 1 1 0 | -
151 1 1 0 172 0 161 0 1 | accept - 152 1 1 0 | 300 1
"""

# Store B: a tier 1 request created and left pending.
CREATED = """\
# Create a tier 1 request.
150 0 0 0 170 1 161 0 1 | accept - 151 1 0 0 | -
"""

# Version 2 continues store B: the CFO approves and the clerk executes.
CONTINUED = """\
# The CFO approves and the clerk executes, after the migration.
151 1 0 0 0 171 0 162 0 1 | accept - 151 1 1 0 0 | -
151 1 1 0 0 172 0 161 0 1 | accept - 152 1 1 0 0 | 300 1
152 1 1 0 0 173 0 163 0 1 | reject 201 152 1 1 0 0 | -
"""

STATE = (120, 121, 122, 123)


def cli() -> str:
    return upgrades.cli()


def zeno(arguments: list[str]) -> dict:
    """One `zeno-fcis` command that must succeed and print a JSON report."""
    completed = subprocess.run([cli(), *arguments], check=False, text=True,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    return upgrades.report(completed, " ".join(arguments[:2]))


def snapshot(directory: Path) -> dict[str, bytes]:
    return {str(path.relative_to(directory)): path.read_bytes()
            for path in sorted(directory.rglob("*")) if path.is_file()}


def migration_file(state: dict[str, dict], from_version: int | None = None) -> dict:
    document = {"schema": "zeno-fcis/migration/1"}
    if from_version is not None:
        document["from_version"] = from_version
    document["state"] = state
    return document


def write_json(path: Path, value: dict) -> Path:
    path.write_text(json.dumps(value, indent=2) + "\n")
    return path


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


def edited(directory: Path, base: Path, project=None, rules=None) -> Path:
    """`base` with its declarations or rules edited."""
    contract(directory, base)
    if project is not None:
        path = directory / "project.zeno"
        path.write_text(project(path.read_text()))
    if rules is not None:
        path = directory / "v2/policy.json"
        path.write_text(json.dumps(rules(json.loads(path.read_text())), indent=2) + "\n")
    return directory


def evolve(app: Path, to: Path, *extra: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run([cli(), "contract", "evolve", str(app), "--to", str(to), *extra,
                           "--format", "json"], check=False, text=True,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def refused_evolution(app: Path, to: Path, extra: list[str], words: list[str],
                      label: str) -> str:
    """`contract evolve` refuses, naming every word, and writes nothing."""
    before = snapshot(app)
    completed = evolve(app, to, *extra)
    if completed.returncode == 0:
        raise RuntimeError(f"{label}: evolve was admitted: {completed.stdout}")
    message = json.loads(completed.stdout)["error"]["message"]
    for word in words:
        if word not in message:
            raise RuntimeError(f"{label}: the refusal does not name {word!r}: {message}")
    if snapshot(app) != before:
        raise RuntimeError(f"{label}: the refusal changed the application")
    return message


def refused_without_write(binary: Path, arguments: list[str], store: Path, word: str,
                          label: str) -> None:
    before = store.read_bytes()
    upgrades.expect_refusal(upgrades.run_app(binary, arguments), word, label)
    if store.read_bytes() != before:
        raise RuntimeError(f"{label}: the refusal changed the store")


def built(root: Path, name: str, version_1: Path, packages, version: str,
          steps: list[tuple[Path, list[str]]]) -> tuple[Path, list[dict]]:
    """An application created from `version_1`, evolved through `steps`,
    checked and built."""
    app = root / name
    applications.run([cli(), "new", str(app), "--contract", str(version_1)], ROOT)
    reports = []
    for to, extra in steps:
        completed = evolve(app, to, *extra)
        reports.append(upgrades.report(completed, f"evolve {name}"))
    zeno(["generate", "contract", str(app), "--check", "--format", "json"])
    upgrades.prepare(app, packages, version)
    return upgrades.build_app(app), reports


def deliveries(store: Path) -> list[str]:
    with sqlite3.connect(f"file:{store}?mode=ro", uri=True) as connection:
        rows = connection.execute(
            "SELECT delivery_id FROM v2_deliveries WHERE acknowledged=0 "
            "ORDER BY sequence,lane,ordinal").fetchall()
    return [bytes(row[0]).hex() for row in rows]


def refusals(directory: Path, version_1: Path) -> dict:
    """Every invalid migration is refused by `contract evolve`, writing nothing."""
    app = directory / "refusals-app"
    applications.run([cli(), "new", str(app), "--contract", str(version_1)], ROOT)
    carry = {str(field): {"from": field} for field in STATE}
    good = write_json(directory / "migration.json", migration_file(
        {**carry, "124": {"default": False}}))
    found = {}

    # The new contract pays the tier a new, carried field holds; the
    # migration gives that field 0, so a tier 1 payment becomes tier 0.
    def paid_tier(project: str) -> str:
        return project.replace("field 124 100 urgent 108;", "field 124 100 paid_tier 105;")

    def pays_paid_tier(rules: dict) -> dict:
        rules["variables"]["paid_tier"] = rules["variables"].pop("urgent")
        for case in rules["cases"]:
            if "124" in case["post"]:
                case["post"]["124"] = "paid_tier"
            for delivery in case["outbox"]:
                if delivery["channel"] == 300:
                    delivery["payload"] = {"145": "paid_tier"}
        rules["genesis"]["124"] = 0
        return rules

    broken = edited(directory / "breaks-delivery", PRIORITY, paid_tier, pays_paid_tier)
    zero = write_json(directory / "zero.json", migration_file({**carry, "124": {"default": 0}}))
    found["delivery"] = refused_evolution(
        app, broken, ["--migration", str(zero)], ["`deliveries` differs", "tier 1"],
        "a migration that breaks a delivery")

    # A request created by anyone but the clerk is refused with another
    # reason.
    def other_reason(rules: dict) -> dict:
        for case in rules["cases"]:
            if case["rule"] == "create: only the clerk":
                case["reason"] = 204
        return rules

    changed = edited(directory / "changes-decision", PRIORITY, rules=other_reason)
    found["decision"] = refused_evolution(
        app, changed, ["--migration", str(good)], ["`decision-class-and-reason` differs"],
        "a migration with a changed decision")

    urgent = write_json(directory / "urgent.json", migration_file(
        {**carry, "124": {"default": True}}))
    found["genesis"] = refused_evolution(
        app, PRIORITY, ["--migration", str(urgent)], ["does not map the old genesis state"],
        "a migration whose genesis does not map")

    # A pending tier 0 request with the CEO's approval satisfies version 1's
    # laws; a new state law forbids it, so the migration must be refused
    # even though no version 1 commit leads there.
    def no_tier0_ceo(project: str) -> str:
        return project.replace(
            "law 501 roles_are_respected",
            "law 505 pending_tier0_has_no_ceo on commit, genesis = "
            "(post.100.120 == 151 && post.100.121 == 0) -> post.100.123 == 0;\n"
            "law 501 roles_are_respected")

    def state_invariant(rules: dict) -> dict:
        rules["law_kinds"]["505"] = "StateInvariant"
        return rules

    law = edited(directory / "new-state-law", PRIORITY, no_tier0_ceo, state_invariant)
    found["new_state_law"] = refused_evolution(
        app, law, ["--migration", str(good)],
        ["breaks a state law of the new contract", "status 151, tier 0", "ceo_ok 1"],
        "a migration into a state a new state law forbids")

    found["layout_without_migration"] = refused_evolution(
        app, PRIORITY, [], ["classified `layout-change`", "--migration"],
        "a layout change without a migration")

    escrow = directory / "escrow-app"
    applications.run([cli(), "new", str(escrow), "--contract", str(ESCROW)], ROOT)
    identity = write_json(directory / "escrow-identity.json", migration_file(
        {str(field): {"from": field} for field in range(110, 117)}))
    found["cap"] = refused_evolution(
        escrow, ESCROW_30, ["--migration", str(identity)],
        ["more than the cap of 1048576", "solver evidence is not accepted"],
        "a domain above the cap")

    # Two consecutive migrations, and shortcuts from version 1 against them.
    route = directory / "route-app"
    applications.run([cli(), "new", str(route), "--contract", str(version_1)], ROOT)
    upgrades.report(evolve(route, PRIORITY, "--migration", str(good)), "evolve route 2")

    def escalated(project: str) -> str:
        return project.replace("field 124 100 urgent 108;",
                               "field 124 100 urgent 108;\nfield 125 100 escalated 108;")

    def carries_escalated(rules: dict) -> dict:
        rules["variables"]["escalated"] = "pre.100.125"
        for case in rules["cases"]:
            if "124" in case["post"]:
                case["post"]["125"] = "escalated"
        rules["genesis"]["125"] = False
        return rules

    third = edited(directory / "escalated", PRIORITY, escalated, carries_escalated)
    second_step = write_json(directory / "escalated.json", migration_file(
        {**carry, "124": {"from": 124}, "125": {"default": False}}))
    wrong = write_json(directory / "shortcut-wrong.json", migration_file(
        {**carry, "124": {"default": False}, "125": {"default": True}}, from_version=1))
    found["shortcut"] = refused_evolution(
        route, third, ["--migration", str(second_step), "--shortcut", str(wrong)],
        ["the shortcut disagrees with the composed route"],
        "a shortcut that disagrees with the composed route")
    right = write_json(directory / "shortcut.json", migration_file(
        {**carry, "124": {"default": False}, "125": {"default": False}}, from_version=1))
    admitted = upgrades.report(evolve(route, third, "--migration", str(second_step),
                                      "--shortcut", str(right)), "evolve with a shortcut")
    upgrades.expect(admitted["evolution"], {"version": 3, "path": "migration"}, "shortcut")
    shortcuts = admitted["evolution"]["migration"]["shortcuts"]
    if [shortcut["from_version"] for shortcut in shortcuts] != [1]:
        raise RuntimeError(f"the admitted shortcut is not reported: {shortcuts}")
    zeno(["generate", "contract", str(route), "--check", "--format", "json"])
    return {"refused": sorted(found), "shortcut": shortcuts}


def check(directory: Path) -> dict:
    applications.run(["cargo", "+1.97.1", "build", "-p", "zeno-fcis-cli", "--locked", "--offline"], ROOT)
    packages = {tomllib.loads(manifest.read_text())["package"]["name"]: manifest.parent
                for manifest in sorted((ROOT / "crates").glob("*/Cargo.toml"))}
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]

    classified = zeno(["contract", "diff", str(SPEND), str(PRIORITY), "--format", "json"])
    upgrades.expect(classified, {"kind": "layout-change"}, "contract diff")

    refused = refusals(directory, contract(directory / "version-1-contract", SPEND, EXECUTED))

    # Version 1: two stores with committed history.
    stores = {}
    for name, session, expected in (
        ("a", EXECUTED, {"decisions": ["Accept"] * 3, "bundles": 3, "pending": 1}),
        ("b", CREATED, {"decisions": ["Accept"], "bundles": 1, "pending": 0}),
    ):
        source = contract(directory / f"version-1-{name}-contract", SPEND, session)
        binary, _ = built(directory, f"version-1-{name}", source, packages, version, [])
        store = directory / f"store-{name}.sqlite"
        decided = upgrades.report(upgrades.run_app(binary, ["--decide", str(store)]), "decide")
        upgrades.expect(decided, {"status": "passed", **expected}, f"decide store {name}")
        stores[name] = (binary, store)
    first, store_a = stores["a"]
    _, store_b = stores["b"]
    pending = deliveries(store_a)
    if len(pending) != 1:
        raise RuntimeError(f"store A holds {pending} pending deliveries, not one")

    # Version 2: the layout change with its migration, evolved and built.
    version_2 = contract(directory / "version-2-contract", PRIORITY, CONTINUED)
    second, (evolved,) = built(
        directory, "version-2", directory / "version-1-a-contract", packages, version,
        [(version_2, ["--migration", str(PRIORITY / "migration.json")])])
    upgrades.expect(evolved, {"status": "evolved", "kind": "layout-change"}, "evolve")
    upgrades.expect(evolved["evolution"], {"ordinal": 1, "from_version": 1, "version": 2,
                                           "path": "migration"}, "evolution")
    simulated = evolved["evolution"]["migration"]

    # The new build audits the old stores without writing to them.
    for name in ("a", "b"):
        store = stores[name][1]
        before = store.read_bytes()
        audit = upgrades.report(upgrades.run_app(second, ["--audit", str(store)]), "audit before")
        upgrades.expect(audit, {"status": "audited", "contract_version": 1, "upgrades": 0},
                        f"audit store {name} before the upgrade")
        if store.read_bytes() != before:
            raise RuntimeError(f"auditing store {name} changed it")
    refused_without_write(second, ["--deliver", str(store_a)], store_a, "upgrade it",
                          "delivering before the upgrade")

    upgraded = {}
    for name, commits, pending_count in (("a", 3, 1), ("b", 1, 0)):
        store = stores[name][1]
        report = upgrades.report(upgrades.run_app(second, ["--upgrade", str(store)]), "upgrade")
        upgrades.expect(report, {
            "status": "upgraded", "kind": "migration", "premises": None, "ordinal": 1,
            "at_commit": commits, "receipts": [], "contract_version": 2, "commits": commits,
            "pending": pending_count, "upgrades": 1,
        }, f"upgrade store {name}")
        migration = report["migration"]
        if migration["tuples_compared"] != simulated["tuples_compared"]:
            raise RuntimeError(f"the store's simulation compared {migration} tuples, generation "
                               f"{simulated}")
        upgraded[name] = report
    refused_without_write(first, ["--audit", str(store_a)], store_a, "Identity",
                          "version 1 over the migrated store")

    # Store A's pending payment keeps the ID its version 1 commit bound.
    delivered = upgrades.report(upgrades.run_app(second, ["--deliver", str(store_a)]), "deliver")
    upgrades.expect(delivered, {"status": "delivered", "contract_version": 2, "commits": 3,
                                "pending": 0, "upgrades": 1}, "deliver store A")
    if delivered["deliveries"] != pending:
        raise RuntimeError(f"delivered {delivered['deliveries']}, expected {pending}")

    # Store B keeps committing under version 2.
    continued = upgrades.report(upgrades.run_app(second, ["--decide", str(store_b)]), "decide")
    upgrades.expect(continued, {"status": "passed", "decisions": ["Accept", "Accept", "Reject"],
                                "bundles": 3, "pending": 1}, "decide store B under version 2")
    paid = upgrades.report(upgrades.run_app(second, ["--deliver", str(store_b)]), "deliver B")
    upgrades.expect(paid, {"status": "delivered", "commits": 3, "pending": 0}, "deliver store B")
    audits = {}
    for name in ("a", "b"):
        audits[name] = upgrades.report(
            upgrades.run_app(second, ["--audit", str(stores[name][1])]), "audit after")
        upgrades.expect(audits[name], {"status": "audited", "contract_version": 2,
                                       "pending": 0, "upgrades": 1},
                        f"audit store {name} after the migration")

    # A forged migrated state is damage.
    forged = directory / "forged.sqlite"
    shutil.copyfile(store_a, forged)
    with sqlite3.connect(forged) as connection:
        publication = bytearray(connection.execute(
            "SELECT publication FROM v2_upgrades WHERE ordinal=1").fetchone()[0])
        publication[-2] ^= 1
        connection.execute("UPDATE v2_upgrades SET publication=? WHERE ordinal=1",
                           (bytes(publication),))
    refused_without_write(second, ["--audit", str(forged)], forged, "History",
                          "a forged migrated state")

    # Version 3 renames the urgent flag: admitted at any state.
    version_3 = edited(directory / "version-3-contract", PRIORITY,
                       project=lambda text: text.replace("field 124 100 urgent 108;",
                                                         "field 124 100 rush 108;"))
    shutil.copyfile(version_2 / "tests/decision-examples.txt",
                    version_3 / "tests/decision-examples.txt")
    third, (_, renamed) = built(
        directory, "version-3", directory / "version-1-a-contract", packages, version,
        [(version_2, ["--migration", str(PRIORITY / "migration.json")]), (version_3, [])])
    upgrades.expect(renamed, {"status": "evolved", "kind": "rename"}, "rename")
    upgrades.expect(renamed["evolution"], {"version": 3, "path": "rename"}, "rename evolution")
    rename = upgrades.report(upgrades.run_app(third, ["--upgrade", str(store_a)]), "rename upgrade")
    upgrades.expect(rename, {"status": "upgraded", "kind": "rename", "ordinal": 2,
                             "at_commit": 3, "contract_version": 3, "upgrades": 2},
                    "rename upgrade")
    replayed = upgrades.report(upgrades.run_app(third, ["--audit", str(store_a)]), "audit three")
    upgrades.expect(replayed, {"status": "audited", "contract_version": 3, "commits": 3,
                               "pending": 0, "upgrades": 2}, "audit three segments")
    return {"schema": "zeno-fcis/contract-migrate-check/1", "status": "passed",
            "authority": "none", "classified": classified["kind"],
            "evolution": evolved["evolution"], "upgrades": upgraded, "delivered": delivered,
            "continued": continued, "audits": audits, "rename": rename, "replayed": replayed,
            "refusals": refused}


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
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-contract-migrate-") as directory:
            result = check(Path(directory))
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, RuntimeError, subprocess.CalledProcessError, KeyError, ValueError) as error:
        print(f"contract migrate: FAIL: {error}", file=sys.stderr)
        sys.exit(1)
