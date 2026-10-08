#!/usr/bin/env python3
"""Check the six-step spend-approval journey through real generated binaries.

No decision or delivery is simulated by this gate. Three contract versions
share one live SQLite store and its submission journal. The local HTTP receiver
deduplicates retries by delivery ID; transport remains at least once.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

import check_contract_migrate as migrations
import check_contract_upgrade as upgrades
import check_generated_application as applications
import relay_receiver

ROOT = applications.ROOT
SPEND = ROOT / "crates/zeno-fcis-cli/tests/fixtures/spend-approval"
STEPS = ("create", "submit", "deliver", "change", "migrate", "replay")


def expect(actual: dict, expected: dict, label: str) -> None:
    upgrades.expect(actual, expected, label)


def command(arguments: list[str], *, code: int = 0) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(arguments, cwd=ROOT, check=False, text=True,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=600)
    if completed.returncode != code:
        raise RuntimeError(f"{arguments[:2]}: expected exit {code}, got {completed.returncode}: "
                           f"{completed.stdout}\n{completed.stderr}")
    return completed


def zeno(*arguments: str) -> dict:
    return json.loads(command([upgrades.cli(), *arguments, "--format", "json"]).stdout)


def operate(binary: Path, *arguments: str, code: int = 0) -> dict:
    formatting = [] if arguments and arguments[0].startswith("--") else ["--format", "json"]
    return json.loads(command([str(binary), *arguments, *formatting], code=code).stdout)


def submit(binary: Path, store: Path, action: str, role: str, tier: int = 0,
           *, dry: bool = False) -> dict:
    result = operate(binary, "decide" if dry else "submit", str(store),
                     f"action={action}", f"tier_requested={tier}", f"role={role}",
                     "board=false", "within_limit=true")
    expect(result, {"status": "decided" if dry else "committed", "class": "Accept",
                    "reason": None}, f"{action} by {role}")
    return result


def store_bytes(store: Path) -> tuple[bytes, bytes]:
    return store.read_bytes(), Path(str(store) + ".submissions").read_bytes()


def stored_deliveries(store: Path) -> list[tuple[str, bool]]:
    with sqlite3.connect(f"file:{store}?mode=ro", uri=True) as connection:
        rows = connection.execute("SELECT delivery_id,acknowledged FROM v2_deliveries "
                                  "ORDER BY sequence,lane,ordinal").fetchall()
    return [(bytes(key).hex(), bool(acknowledged)) for key, acknowledged in rows]


def expected_bodies(store: Path) -> dict[str, dict]:
    """Freeze exact canonical delivery bytes, independently of relay serialization."""
    with sqlite3.connect(f"file:{store}?mode=ro", uri=True) as connection:
        rows = connection.execute("SELECT sequence,lane,ordinal,delivery_id,channel,destination_root,"
                                  "payload_root,destination,payload,entry_hash FROM v2_deliveries "
                                  "ORDER BY sequence,lane,ordinal").fetchall()
    return {bytes(row[3]).hex(): {
        "schema": "zeno-fcis/relay-delivery/1", "commit": row[0], "lane": row[1],
        "ordinal": row[2], "delivery_id": bytes(row[3]).hex(), "channel": row[4],
        "destination_root": row[5], "payload_root": row[6], "destination": bytes(row[7]).hex(),
        "payload": bytes(row[8]).hex(), "payload_sha256": hashlib.sha256(bytes(row[8])).hexdigest(),
        "entry_hash": bytes(row[9]).hex(),
    } for row in rows}


def observed_decision(report: dict, *, mapped: bool = False) -> dict:
    observation = {key: copy.deepcopy(report[key]) for key in ("class", "reason", "state", "deliveries")}
    if mapped:
        if observation["state"].pop("urgent", None) is not False:
            raise RuntimeError("the migration did not add urgent=false")
    return observation


def check_observations(old: dict, new: dict) -> None:
    if old != new:
        raise RuntimeError("migration changed a decision observation")


def check_effects(record: dict, ids: list[str], bodies: dict[str, dict] | None = None) -> None:
    if [entry["key"] for entry in record["effects"]] != ids or record["conflicts"] != 0:
        raise RuntimeError("receiver lost or duplicated an effect, or changed its identity")
    if bodies is not None and any(json.loads(effect["body"]) != bodies[effect["key"]]
                                  for effect in record["effects"]):
        raise RuntimeError("receiver observed a changed destination or payload")


def check_steps(completed: list[str]) -> None:
    if tuple(completed) != STEPS:
        raise RuntimeError("the operational journey skipped or reordered a step")


def finish_step(completed: list[str], name: str, evidence: str) -> None:
    if name != STEPS[len(completed)]:
        raise RuntimeError("the operational journey skipped or reordered a step")
    completed.append(name)
    print(f"operational-journey: PASS {name}: {evidence}", flush=True)


def built(app: Path, packages: dict[str, Path], version: str) -> Path:
    # Evolving this owned fixture changes its package name. Start each version
    # from the reviewed lock before admitting its newly resolved local graph.
    shutil.copyfile(ROOT / "Cargo.lock", app / "Cargo.lock")
    upgrades.prepare(app, packages, version)
    compiled = upgrades.build_app(app)
    contract_version = operate(compiled, "version")["contract_version"]
    frozen = compiled.with_name(f"{compiled.stem}-contract-v{contract_version}{compiled.suffix}")
    shutil.copy2(compiled, frozen)
    return frozen


def refused_evolution(app: Path, target: Path, *options: str) -> None:
    before = migrations.snapshot(app)
    refused = command([upgrades.cli(), "contract", "evolve", str(app), "--to", str(target),
                       *options, "--format", "json"], code=1)
    if "error" not in json.loads(refused.stdout) or migrations.snapshot(app) != before:
        raise RuntimeError("a refused evolution wrote to the application")


def contract(directory: Path, source: Path) -> Path:
    return migrations.contract(directory, source)


def check(directory: Path) -> dict:
    applications.run(["cargo", "+1.97.1", "build", "-p", "zeno-fcis-cli", "--locked", "--offline"], ROOT)
    packages = {tomllib.loads(path.read_text())["package"]["name"]: path.parent
                for path in sorted((ROOT / "crates").glob("*/Cargo.toml"))}
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    completed: list[str] = []
    app = directory / "app"
    command([upgrades.cli(), "new", str(app), "--contract", str(SPEND), "--source", str(ROOT)])
    first = built(app, packages, version)
    first_version = operate(first, "version")
    expect(first_version, {"contract_version": 1}, "version 1")
    store = directory / "spend.sqlite"
    initialized = operate(first, "init", str(store))
    expect(initialized, {"commits": 0, "pending": 0}, "init")
    finish_step(completed, "create", "generated README binding, version 1, real genesis")

    before = store_bytes(store)
    preview = submit(first, store, "Create", "Clerk", 2, dry=True)
    if store_bytes(store) != before:
        raise RuntimeError("decide wrote to the store or submission journal")
    created = submit(first, store, "Create", "Clerk", 2)
    check_observations(observed_decision(preview), observed_decision(created))
    submit(first, store, "Approve", "Cfo")
    submit(first, store, "Approve", "Ceo")
    executed = submit(first, store, "Execute", "Clerk")
    expect(executed["state"], {"status": "Executed", "tier": 2, "cfo_ok": True, "ceo_ok": True}, "execution")
    expect(operate(first, "state", str(store)), {"commits": 4, "pending": 1}, "state")
    history = operate(first, "history", str(store))
    if len(history["entries"]) != 4 or any(e["contract_version"] != 1 for e in history["entries"]):
        raise RuntimeError("history did not replay all four version 1 submissions")
    waiting = operate(first, "pending", str(store))
    ids = [waiting["next"]["delivery_id"]]
    bodies = expected_bodies(store)
    finish_step(completed, "submit", "CFO and CEO approval, 4 commits, dry run unchanged, payment pending")

    receiver_record = directory / "receiver.json"
    config_path = directory / "relay.json"
    with relay_receiver.Running(receiver_record) as url:
        config = {"schema": "zeno-fcis/relay-destination/1", "http": url,
                  "attempts": 2, "backoff": 0, "max_backoff": 0, "timeout": 3,
                  "crash_at": "after-send"}
        migrations.write_json(config_path, config)
        operate(first, "deliver", str(store), "--relay", str(config_path), code=3)
        expect(operate(first, "pending", str(store)), {"pending": 1}, "after crash")
        check_effects(relay_receiver.ledger(receiver_record), ids, bodies)
        del config["crash_at"]
        migrations.write_json(config_path, config)
        delivered = operate(first, "deliver", str(store), "--relay", str(config_path))
        expect(delivered, {"deliveries": ids, "pending": 0, "destination": "relay"}, "relay restart")
        again = operate(first, "deliver", str(store), "--relay", str(config_path))
        expect(again, {"deliveries": [], "pending": 0}, "drained relay")
        ledger = relay_receiver.ledger(receiver_record)
        check_effects(ledger, ids, bodies)
        if ledger["requests"].get(ids[0]) != 2:
            raise RuntimeError("the crash/restart did not exercise a duplicate transport attempt")
    finish_step(completed, "deliver", "2 transport attempts, 1 receiver effect, original delivery ID")

    # An actual program successor cannot be smuggled through the rule-change path.
    adopted = contract(directory / "program-successor", SPEND)
    program, candidate, receipt = (directory / name for name in ("original.zcve", "candidate.zcve", "receipt.json"))
    zeno("contract", "export-program", str(adopted), "--out", str(program))
    optimized = json.loads(command([upgrades.cli(), "optimize", "--program", str(program),
                                    "--candidate-out", str(candidate), "--receipt", str(receipt)]).stdout)
    expect(optimized, {"status": "improved"}, "control optimization")
    zeno("contract", "adopt", str(adopted), "--candidate", str(candidate), "--receipt", str(receipt), "--usage", "new-version")
    expect(zeno("contract", "diff", str(app), str(adopted)), {"kind": "program-successor"}, "control kind")
    refused_evolution(app, adopted)

    changed = contract(directory / "rule-change", SPEND)
    rules = json.loads((changed / "v2/policy.json").read_text())
    rules["cases"][1]["when"] = "action == 170 && status != 150 && status != 152"
    migrations.write_json(changed / "v2/policy.json", rules)
    difference = command([upgrades.cli(), "contract", "diff", str(app), str(changed)])
    if "rule" not in difference.stdout.lower():
        raise RuntimeError("plain-language diff did not identify the rule change")
    expect(zeno("contract", "diff", str(app), str(changed)), {"kind": "rule-change"}, "rule kind")
    zeno("contract", "evolve", str(app), "--to", str(changed))
    second = built(app, packages, version)
    unchanged = store_bytes(store)
    expect(operate(second, "--audit", str(store)), {"contract_version": 1, "commits": 4}, "old-segment audit")
    if store_bytes(store) != unchanged:
        raise RuntimeError("audit-before-upgrade wrote to the store or journal")
    expect(operate(second, "--upgrade", str(store)), {"kind": "behaviour-change", "contract_version": 2}, "rule upgrade")
    submit(second, store, "Create", "Clerk", 1)
    submit(second, store, "Approve", "Cfo")
    submit(second, store, "Execute", "Clerk")
    waiting = operate(second, "pending", str(store))
    ids.append(waiting["next"]["delivery_id"])
    bodies = expected_bodies(store)
    expect(waiting, {"commits": 7, "pending": 1}, "version 2 payment")
    finish_step(completed, "change", "reviewed rule diff, refused wrong route, live upgrade, 3 new commits")

    layout = contract(directory / "layout-change", changed)
    project = layout / "project.zeno"
    project.write_text(project.read_text().replace("field 123 100 ceo_ok 108;", "field 123 100 ceo_ok 108;\nfield 124 100 urgent 108;"))
    rules = json.loads((layout / "v2/policy.json").read_text())
    rules["variables"]["urgent"] = "pre.100.124"
    rules["genesis"]["124"] = False
    for case in rules["cases"]:
        if case["post"]:
            case["post"]["124"] = "urgent"
    migrations.write_json(layout / "v2/policy.json", rules)
    state_mapping = {str(field): {"from": field} for field in (120, 121, 122, 123)}
    state_mapping["124"] = {"default": False}
    bad_mapping = copy.deepcopy(state_mapping)
    bad_mapping["121"] = {"default": 0}
    bad_file = migrations.write_json(directory / "wrong-migration.json", migrations.migration_file(bad_mapping))
    refused_evolution(app, layout, "--migration", str(bad_file))
    file = migrations.write_json(directory / "migration.json", migrations.migration_file(state_mapping))
    before_decision = submit(second, store, "Create", "Clerk", 3, dry=True)
    evolved = zeno("contract", "evolve", str(app), "--to", str(layout), "--migration", str(file))
    if evolved["evolution"]["migration"]["tuples_compared"] <= 0:
        raise RuntimeError("migration admission did not perform a complete finite comparison")
    third = built(app, packages, version)
    before_ids = stored_deliveries(store)
    expect(operate(third, "--upgrade", str(store)), {"kind": "migration", "contract_version": 3}, "layout upgrade")
    if stored_deliveries(store) != before_ids:
        raise RuntimeError("migration changed an old delivery ID or acknowledgment")
    unchanged = store_bytes(store)
    at_upgrade = operate(third, "history", str(store))
    expect(at_upgrade, {"contract_version": 3, "commits": 7, "upgrades": 2}, "history after last-head migration")
    if [e["contract_version"] for e in at_upgrade["entries"]] != [1] * 4 + [2] * 3:
        raise RuntimeError("last-head migration lost the original committing versions")
    if "urgent" in at_upgrade["genesis"] or any("urgent" in e["state"] for e in at_upgrade["entries"]):
        raise RuntimeError("history used the newest schema to label older states")
    if store_bytes(store) != unchanged:
        raise RuntimeError("historical replay changed the store or journal")
    after_decision = submit(third, store, "Create", "Clerk", 3, dry=True)
    check_observations(observed_decision(before_decision), observed_decision(after_decision, mapped=True))
    # A real commit in the third segment makes the final replay exercise all three.
    submit(third, store, "Create", "Clerk", 0)
    finish_step(completed, "migrate", "exhaustive admission, wrong mapping refused, pending ID and decisions preserved")

    with relay_receiver.Running(receiver_record) as url:
        config["http"] = url
        migrations.write_json(config_path, config)
        final_delivery = operate(third, "deliver", str(store), "--relay", str(config_path))
        expect(final_delivery, {"deliveries": ids[1:], "pending": 0}, "old payment under version 3")
    check_effects(relay_receiver.ledger(receiver_record), ids, bodies)
    audit = operate(third, "--audit", str(store))
    expect(audit, {"contract_version": 3, "commits": 8, "upgrades": 2, "pending": 0}, "full audit")
    history = operate(third, "history", str(store))
    if [e["contract_version"] for e in history["entries"]] != [1] * 4 + [2] * 3 + [3]:
        raise RuntimeError("history did not replay every submission under its original contract")
    # A different context on the old Create can leave the same successor.
    # The journal must still match the actual stored inputs, not just state.
    journal = Path(str(store) + ".submissions")
    original_journal = journal.read_bytes()
    lines = original_journal.decode().splitlines(keepends=True)
    sections = lines[0].split("|")
    context = sections[2].split()
    context[-1] = "0" if context[-1] == "1" else "1"
    lines[0] = "|".join(sections[:2]) + "| " + " ".join(context) + "\n"
    journal.write_text("".join(lines))
    corrupted = store_bytes(store)
    refused_history = operate(third, "history", str(store), code=1)
    if "differs from the stored contract or inputs" not in refused_history["error"]["message"]:
        raise RuntimeError("different journal inputs did not refuse for the input mismatch")
    if store_bytes(store) != corrupted:
        raise RuntimeError("refused historical replay changed the store or journal")
    journal.write_bytes(original_journal)
    if stored_deliveries(store) != [(key, True) for key in ids]:
        raise RuntimeError("final audit lost or changed a delivered entry")
    # A store may start at a later lineage version, or pass two upgrades at
    # sequence zero. Neither has a commit from which to infer its genesis.
    fresh = directory / "fresh-v3.sqlite"
    operate(third, "init", str(fresh))
    fresh_history = operate(third, "history", str(fresh))
    expect(fresh_history, {"contract_version": 3, "commits": 0, "upgrades": 0, "entries": []}, "later-version genesis")
    if fresh_history["genesis"].get("urgent") is not False:
        raise RuntimeError("history did not use the actual later-version genesis")
    zero = directory / "zero-commits.sqlite"
    expect(operate(first, "version"), {"contract_version": 1}, "retained version 1 binary")
    operate(first, "init", str(zero))
    operate(third, "--upgrade", str(zero))
    zero_before = store_bytes(zero)
    zero_history = operate(third, "history", str(zero))
    expect(zero_history, {"contract_version": 3, "commits": 0, "upgrades": 2, "entries": []}, "zero-commit upgrades")
    if "urgent" in zero_history["genesis"] or store_bytes(zero) != zero_before:
        raise RuntimeError("zero-commit history rewrote or mislabeled original genesis")
    final_version = operate(third, "version")
    expect(final_version, {"contract_version": 3}, "final version")
    if final_version["identity"] == first_version["identity"]:
        raise RuntimeError("changed contracts reused the original identity")
    finish_step(completed, "replay", "8 commits over 3 segments, 2 deliveries with original IDs, final version identity")
    check_steps(completed)
    return {"steps": completed, "audit": audit, "version": final_version, "delivery_ids": ids}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work-dir", type=Path, help="new evidence directory; keep test artifacts")
    args = parser.parse_args()
    if args.work_dir:
        args.work_dir.mkdir(parents=True, exist_ok=False)
        check(args.work_dir)
    else:
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-operational-") as directory:
            check(Path(directory))
    print("operational-journey: complete PASS (6 steps)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
