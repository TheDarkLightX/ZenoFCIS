#!/usr/bin/env python3
"""Migrate the shipped order-fulfillment contract and a live unused-field drop.

All decisions use actual generated applications and their library Authority.
The split fixture mechanically maps the shipped labels; no new owner labels
or policy evaluator are introduced. Historical mapped and unchanged channel
deliveries retain their original bytes, IDs and version, through real relay.
The existing locked generated-app build helpers remain the admission gate.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
import sqlite3
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

import check_contract_migrate as migrations
import check_contract_upgrade as upgrades
import check_generated_application as applications
import check_payload_migrate as payloads

ROOT = applications.ROOT
TEMPLATE = ROOT / "crates/zeno-fcis-cli/templates/order-fulfillment"
SPLIT = migrations.FIXTURES / "order-fulfillment-split"
ORDER_COUNTS = (24, 20, 1, 1_440)
DROP_COUNTS = (160, 114, 1, 29_184)
COUNT_KEYS = ("states", "states_satisfying_state_laws", "genesis_states", "tuples_compared")
STATUS = {160: (False, 0), 161: (False, 1), 165: (False, 2),
          162: (True, 0), 163: (True, 1), 164: (True, 2)}


def mapped_label(line: str) -> str:
    """Only the declared state/payload encoding changes a retained label."""
    before, after, outbox = [part.strip().split() for part in line.split("|")]
    paid, progress = STATUS[int(before[0])]
    before = [before[1], str(int(paid)), str(progress), *before[2:]]
    paid, progress = STATUS[int(after[2])]
    after = [*after[:2], after[3], str(int(paid)), str(progress)]
    if outbox[0] == "300":
        outbox[2] = str(int(outbox[2] == "175"))
    return " ".join(before) + " | " + " ".join(after) + " | " + " ".join(outbox)


def labels(path: Path) -> list[str]:
    return [line.strip() for line in path.read_text().splitlines()
            if line.strip() and not line.lstrip().startswith("#")]


def fixture_provenance() -> dict:
    source = TEMPLATE / "tests/decision-examples.txt"
    digest = hashlib.sha256(source.read_bytes()).hexdigest()
    derived = SPLIT / "tests/decision-examples.txt"
    original = labels(source)
    if digest not in derived.read_text() or labels(derived) != [mapped_label(line) for line in original]:
        raise RuntimeError("split examples differ from the explicit mapping of retained shipped labels")
    old = json.loads((TEMPLATE / "v2/policy.json").read_text())
    new = json.loads((SPLIT / "v2/policy.json").read_text())
    if [(case["class"], case["reason"]) for case in old["cases"]] != [
            (case["class"], case["reason"]) for case in new["cases"]]:
        raise RuntimeError("split fixture changes a retained decision class/reason or branch order")
    return {"source": str(source.relative_to(ROOT)), "source_sha256": digest,
            "mapped_labels": len(original), "new_owner_labels": 0}


def counts(actual: dict, expected: tuple[int, int, int, int], label: str) -> None:
    upgrades.expect(actual, dict(zip(COUNT_KEYS, expected)), label)


def report(binary: Path, arguments: list[str], label: str) -> dict:
    return upgrades.report(upgrades.run_app(binary, arguments), label)


def operation(binary: Path, command: str, store: Path, *words: str) -> dict:
    return report(binary, [command, str(store), *words, "--format", "json"], command)


def order_commit(binary: Path, store: Path, action: str, callback: int, caller: str,
                 commit: int, decision: str = "Accept", reason: int | None = None) -> dict:
    result = operation(binary, "submit", store, f"action={action}",
                       f"callback_attempt={callback}", f"caller={caller}")
    upgrades.expect(result, {"status": "committed", "class": decision,
                            "reason": reason, "commit": commit}, action)
    return result


def pending_exports(store: Path) -> list[dict]:
    """The same physical relay export as check_payload_migrate, for all rows."""
    with sqlite3.connect(f"file:{store}?mode=ro", uri=True) as connection:
        rows = connection.execute(
            "SELECT sequence,lane,ordinal,channel,destination_root,payload_root,destination,"
            "payload,delivery_id,entry_hash FROM v2_deliveries WHERE acknowledged=0 "
            "ORDER BY sequence,lane,ordinal").fetchall()
    result = []
    for commit, lane, ordinal, channel, destination_root, payload_root, destination, payload, identity, entry in rows:
        result.append({"schema": "zeno-fcis/relay-export/1", "commit": commit, "lane": lane,
                       "ordinal": ordinal, "channel": channel, "destination_root": destination_root,
                       "payload_root": payload_root, "destination": bytes(destination).hex(),
                       "payload": bytes(payload).hex(), "delivery_id": bytes(identity).hex(),
                       "entry_hash": bytes(entry).hex(),
                       "payload_sha256": hashlib.sha256(payload).hexdigest()})
    if result and result[0] != payloads.outgoing(store):
        raise RuntimeError("pending exports disagree with the existing historical relay helper")
    return result


def relay(binary: Path, store: Path, directory: Path, name: str, expected: list[dict]) -> dict:
    queue = directory / f"{name}.jsonl"
    config = migrations.write_json(
        directory / f"{name}.json",
        {"schema": "zeno-fcis/relay-destination/1", "queue": queue.name})
    result = report(binary, ["deliver", str(store), "--relay", str(config), "--format", "json"],
                    f"relay {name}")
    upgrades.expect(result, {"status": "delivered", "pending": 0,
                            "deliveries": [entry["delivery_id"] for entry in expected]}, name)
    sent = [json.loads(line) for line in queue.read_text().splitlines()]
    wanted = [{**entry, "schema": "zeno-fcis/relay-delivery/1"} for entry in expected]
    if sent != wanted:
        raise RuntimeError(f"{name}: relay changed historical identity, destination or payload bytes")
    return result


def audit_read_only(binary: Path, store: Path, version: int, commits: int,
                    pending: int, upgrades_count: int) -> dict:
    before = store.read_bytes()
    result = report(binary, ["--audit", str(store)], "read-only audit")
    upgrades.expect(result, {"status": "audited", "contract_version": version, "commits": commits,
                            "pending": pending, "upgrades": upgrades_count}, "read-only audit")
    if store.read_bytes() != before:
        raise RuntimeError("read-only audit changed the store")
    return result


def history_read_only(binary: Path, store: Path, versions: list[int]) -> dict:
    before = store.read_bytes()
    result = operation(binary, "history", store)
    if [entry["contract_version"] for entry in result["entries"]] != versions:
        raise RuntimeError(f"history did not replay the expected old/new segments: {result}")
    if store.read_bytes() != before:
        raise RuntimeError("history replay changed the store")
    return result


def order_refusals(directory: Path) -> dict:
    app = directory / "order-refusals-app"
    applications.run([migrations.cli(), "new", str(app), "--contract", str(TEMPLATE)], ROOT)
    good = json.loads((SPLIT / "migration.json").read_text())
    found = {}
    for name, field, replacement, drop, words in (
        ("merged-shipped-delivered", "123", {"expr":
         "choose(old.120 == 164, 1, choose(old.120 == 160 || old.120 == 162, 0, "
         "choose(old.120 == 161 || old.120 == 163, 1, 2)))"},
         None, ["differs"]),
        ("negative-attempts", "121", {"expr": "old.121 - 1"}, None,
         ["expression of new field 121", "out-of-domain"]),
        ("undeclared-attempt-drop", "121", {"default": 0}, None,
         ["does not carry old state field 121"]),
        ("declared-attempt-drop", "121", {"default": 0}, [121],
         ["successor-state"]),
    ):
        bad = copy.deepcopy(good)
        bad["state"][field] = replacement
        if drop is not None:
            bad["drop"] = drop
        path = migrations.write_json(directory / f"{name}.json", bad)
        found[name] = migrations.refused_evolution(
            app, SPLIT, ["--migration", str(path)], words, name)
    bad = copy.deepcopy(good)
    bad["payloads"]["300"]["142"] = {"from": 141, "map": {"175": False, "176": False}}
    path = migrations.write_json(directory / "constant-capture.json", bad)
    found["constant-capture"] = migrations.refused_evolution(
        app, SPLIT, ["--migration", str(path)], ["payload"], "a lossy constant capture")
    return found


def order_journey(directory: Path, packages: dict, version: str) -> dict:
    classified = migrations.zeno(["contract", "diff", str(TEMPLATE), str(SPLIT),
                                 "--format", "json"])
    upgrades.expect(classified, {"kind": "layout-change"}, "shipped split classification")
    first, _ = migrations.built(directory, "order-v1", TEMPLATE, packages, version, [])
    second, (evolved,) = migrations.built(
        directory, "order-v2", TEMPLATE, packages, version,
        [(SPLIT, ["--migration", str(SPLIT / "migration.json")])])
    counts(evolved["evolution"]["migration"], ORDER_COUNTS, "order evolution")
    refused = order_refusals(directory)
    store_a = directory / "order-a.sqlite"
    store_b = directory / "order-b.sqlite"

    operation(first, "init", store_a)
    order_commit(first, store_a, "Checkout", 0, "Customer", 1)
    first_payment = pending_exports(store_a)
    if len(first_payment) != 1 or first_payment[0]["channel"] != 300:
        raise RuntimeError("checkout did not leave the original capture request")
    relay(first, store_a, directory, "first-capture", first_payment)
    decline = order_commit(first, store_a, "PaymentDeclined", 1, "PaymentProvider",
                           2, "CommittedFailure", 204)
    upgrades.expect(decline, {"state": {"status": "Placed", "payment_attempts": 1}},
                    "retained committed-failure label")
    order_commit(first, store_a, "Checkout", 0, "Customer", 3)
    order_commit(first, store_a, "PaymentCaptured", 2, "PaymentProvider", 4)
    original = pending_exports(store_a)
    if [entry["channel"] for entry in original] != [300, 301]:
        raise RuntimeError("store A does not have both mapped and unchanged channels pending")
    original_history = payloads.retained_history(store_a)
    before = audit_read_only(second, store_a, 1, 4, 2, 0)
    moved = report(second, ["--upgrade", str(store_a)], "upgrade shipped order")
    upgrades.expect(moved, {"kind": "migration", "contract_version": 2, "commits": 4,
                           "pending": 2, "upgrades": 1}, "order upgrade")
    counts(moved["migration"], ORDER_COUNTS, "order store simulation")
    if pending_exports(store_a) != original or payloads.retained_history(store_a) != original_history:
        raise RuntimeError("order migration rewrote old history or pending delivery bytes/IDs")
    state = operation(second, "state", store_a)
    upgrades.expect(state, {"state": {"payment_attempts": 2, "paid": True, "progress": 0}},
                    "mapped paid order")
    delivered = relay(second, store_a, directory, "old-mixed-channels", original)
    if payloads.retained_history(store_a) != original_history:
        raise RuntimeError("historical delivery changed its bound bytes")
    order_commit(second, store_a, "ParcelDispatched", 0, "Carrier", 5)
    order_commit(second, store_a, "ParcelDelivered", 0, "Carrier", 6)
    audited = audit_read_only(second, store_a, 2, 6, 0, 1)
    history = history_read_only(second, store_a, [1, 1, 1, 1, 2, 2])
    if history["entries"][1]["class"] != "CommittedFailure" or history["entries"][1]["reason"] != 204:
        raise RuntimeError("replay lost the retained committed-failure class/reason")
    migrations.refused_without_write(first, ["--audit", str(store_a)], store_a, "Identity",
                                      "version 1 over the split order")

    # A second retained source path: checkout, upgrade, then cancellation.
    # The pending Capture is old bytes; the new Void uses capture=false.
    operation(first, "init", store_b)
    order_commit(first, store_b, "Checkout", 0, "Customer", 1)
    capture = pending_exports(store_b)
    audit_read_only(second, store_b, 1, 1, 1, 0)
    report(second, ["--upgrade", str(store_b)], "upgrade awaiting payment")
    if pending_exports(store_b) != capture:
        raise RuntimeError("store B upgrade changed its old Capture")
    cancelled = order_commit(second, store_b, "CancelOrder", 0, "Customer", 2)
    upgrades.expect(cancelled, {"state": {"payment_attempts": 1, "paid": False, "progress": 2}},
                    "mapped cancel label")
    if cancelled["deliveries"] != [
            {"channel": 300, "payload": {"request_attempt": 1, "capture": False}}]:
        raise RuntimeError(f"cancel did not publish the mapped Void payload: {cancelled}")
    mixed = pending_exports(store_b)
    if len(mixed) != 2 or mixed[0] != capture[0] or mixed[1]["payload"] == mixed[0]["payload"]:
        raise RuntimeError("store B does not retain one old Capture and one new Void")
    cancelled_delivery = relay(second, store_b, directory, "old-capture-new-void", mixed)
    cancel_audit = audit_read_only(second, store_b, 2, 2, 0, 1)
    cancel_history = history_read_only(second, store_b, [1, 2])
    return {"status": "passed", "classification": classified["kind"],
            "evolution": evolved["evolution"], "refusals": refused, "audit_before": before,
            "upgrade": moved, "original_pending": original, "delivered": delivered,
            "audit": audited, "history": history, "cancel_delivered": cancelled_delivery,
            "cancel_audit": cancel_audit, "cancel_history": cancel_history}


def drop_journey(directory: Path, packages: dict, version: str) -> dict:
    directory.mkdir()
    mapping = {"schema": "zeno-fcis/migration/2", "drop": [124],
               "state": {str(field): {"from": field} for field in migrations.STATE}}
    path = migrations.write_json(directory / "drop.json", mapping)
    first, _ = migrations.built(directory, "priority-v1", migrations.PRIORITY,
                                packages, version, [])
    second, (evolved,) = migrations.built(
        directory, "base-v2", migrations.PRIORITY, packages, version,
        [(migrations.SPEND, ["--migration", str(path)])])
    counts(evolved["evolution"]["migration"], DROP_COUNTS, "unused-field evolution")
    upgrades.expect(evolved["evolution"]["migration"], {"dropped": [124]}, "declared field drop")
    store = directory / "priority.sqlite"
    operation(first, "init", store)
    for commit, action, tier, role, board in (
        (1, "Create", 3, "Clerk", "false"),
        (2, "Approve", 0, "Cfo", "false"),
        (3, "Approve", 0, "Ceo", "false"),
        (4, "Execute", 0, "Clerk", "true"),
    ):
        decided = operation(first, "submit", store, f"action={action}", f"tier_requested={tier}",
                            f"role={role}", f"board={board}", "within_limit=true")
        upgrades.expect(decided, {"class": "Accept", "reason": None, "commit": commit},
                        "retained priority journey")
    original = pending_exports(store)
    if len(original) != 1:
        raise RuntimeError("priority journey did not leave its tier 3 payment pending")
    old_history = payloads.retained_history(store)
    before = audit_read_only(second, store, 1, 4, 1, 0)
    moved = report(second, ["--upgrade", str(store)], "drop unused urgent")
    counts(moved["migration"], DROP_COUNTS, "unused-field store simulation")
    upgrades.expect(moved, {"kind": "migration", "contract_version": 2, "commits": 4,
                           "pending": 1, "upgrades": 1}, "unused-field upgrade")
    if pending_exports(store) != original or payloads.retained_history(store) != old_history:
        raise RuntimeError("field drop changed the existing history or pending payment")
    state = operation(second, "state", store)
    upgrades.expect(state, {"state": {"status": "Executed", "tier": 3,
                                    "cfo_ok": True, "ceo_ok": True}}, "base-framed state")
    sent = relay(second, store, directory, "historical-priority-payment", original)
    replayed = history_read_only(second, store, [1, 1, 1, 1])
    audited = audit_read_only(second, store, 2, 4, 0, 1)
    migrations.refused_without_write(first, ["--audit", str(store)], store, "Identity",
                                      "priority version 1 after the drop")

    # CLI admission refuses authoring a map that forgets the read CEO
    # approval. Native data_migration S2 also exercises the actual shell
    # upgrade with this harmful lineage and snapshots its database/history.
    refusal_app = directory / "harmful-drop-app"
    applications.run([migrations.cli(), "new", str(refusal_app),
                      "--contract", str(migrations.PRIORITY)], ROOT)
    harmful = copy.deepcopy(mapping)
    harmful["drop"] = [123, 124]
    harmful["state"]["123"] = {"default": False}
    bad = migrations.write_json(directory / "harmful-drop.json", harmful)
    refused = migrations.refused_evolution(
        refusal_app, migrations.SPEND, ["--migration", str(bad)],
        ["decision-class-and-reason"], "forgetting the read CEO approval")
    return {"status": "passed", "evolution": evolved["evolution"], "audit_before": before,
            "upgrade": moved, "pending": original, "delivered": sent, "history": replayed,
            "audit": audited, "harmful_cli_refusal": refused}


def check(directory: Path, packages: dict, version: str) -> dict:
    directory.mkdir()
    provenance = fixture_provenance()
    order = order_journey(directory, packages, version)
    dropped = drop_journey(directory / "drop-journey", packages, version)
    return {"schema": "zeno-fcis/template-migrate-check/1", "status": "passed",
            "authority": "none", "provenance": provenance,
            "order_fulfillment": order, "unused_field_drop": dropped}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--keep", type=Path, help="retain artifacts in a new directory")
    args = parser.parse_args()
    os.environ.setdefault("CARGO_BUILD_JOBS", "4")
    os.environ.setdefault("CARGO_INCREMENTAL", "0")
    os.environ.setdefault("CARGO_PROFILE_DEV_DEBUG", "0")
    os.environ.setdefault("CARGO_PROFILE_TEST_DEBUG", "0")
    os.environ.setdefault("CARGO_TARGET_DIR", str(ROOT / "target"))
    applications.run(["cargo", "+1.97.1", "build", "-p", "zeno-fcis-cli", "--locked", "--offline"], ROOT)
    packages = {tomllib.loads(path.read_text())["package"]["name"]: path.parent
                for path in sorted((ROOT / "crates").glob("*/Cargo.toml"))}
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    if args.keep:
        result = check(args.keep.resolve(), packages, version)
    else:
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-template-migrate-") as root:
            result = check(Path(root) / "journey", packages, version)
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, RuntimeError, subprocess.CalledProcessError, KeyError, ValueError) as error:
        print(f"template migrate: FAIL: {error}", file=sys.stderr)
        sys.exit(1)
