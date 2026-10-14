#!/usr/bin/env python3
"""Check lossless state/payload splits through actual generated apps and relay.

The complete old tier domain is 0..3. A new state field observes its high bit,
and future payment payloads represent the tier by two Boolean bits. Old pending
payments retain their old integer payload, Authority, entry hash and delivery
ID. A real packaged relay sends and acknowledges those historical bytes after
upgrade; a send-before-ack crash keeps the payment pending for identical retry.
No handwritten policy evaluator or decision callback participates.
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

import check_contract_migrate as migrations
import check_contract_upgrade as upgrades
import check_generated_application as applications

ROOT = applications.ROOT
SPEND = migrations.SPEND
CONTINUED = migrations.CONTINUED.replace("| 300 1\n", "| 300 1 0\n")


def mapping() -> dict:
    return {"schema": "zeno-fcis/migration/2",
            "state": {**{str(field): {"from": field} for field in migrations.STATE},
                      "124": {"expr": "old.121 >= 2"}},
            "payloads": {"300": {
                "145": {"expr": "old.145 == 1 || old.145 == 3"},
                "147": {"expr": "old.145 >= 2"}}}}


def split_contract(directory: Path) -> Path:
    def project(text: str) -> str:
        return text.replace("field 123 100 ceo_ok 108;",
                            "field 123 100 ceo_ok 108;\nfield 124 100 high_tier 108;") \
            .replace("field 145 104 payment_tier 105;",
                     "field 145 104 tier_low 108;\nfield 147 104 tier_high 108;")

    def rules(document: dict) -> dict:
        document["genesis"]["124"] = False
        for case in document["cases"]:
            if "121" in case["post"]:
                case["post"]["124"] = f"{case['post']['121']} >= 2"
            for delivery in case["outbox"]:
                if delivery["channel"] == 300:
                    delivery["payload"] = {"145": "tier == 1 || tier == 3", "147": "tier >= 2"}
        return document

    migrations.edited(directory, SPEND, project, rules)
    (directory / "tests/decision-examples.txt").write_text(CONTINUED)
    return directory


def retained_history(store: Path) -> tuple:
    """Every original genesis, commit and delivery token, excluding only ack."""
    with sqlite3.connect(f"file:{store}?mode=ro", uri=True) as connection:
        return tuple(connection.execute(query).fetchall() for query in (
            "SELECT * FROM v2_genesis",
            "SELECT * FROM v2_commits ORDER BY sequence",
            "SELECT sequence,lane,ordinal,channel,destination_root,payload_root,destination,"
            "payload,marker,delivery_id,entry_hash FROM v2_deliveries ORDER BY sequence,lane,ordinal"))


def outgoing(store: Path) -> dict:
    with sqlite3.connect(f"file:{store}?mode=ro", uri=True) as connection:
        row = connection.execute(
            "SELECT sequence,lane,ordinal,channel,destination_root,payload_root,destination,"
            "payload,delivery_id,entry_hash FROM v2_deliveries WHERE acknowledged=0 "
            "ORDER BY sequence,lane,ordinal LIMIT 1").fetchone()
    if row is None:
        raise RuntimeError("expected one genuine pending delivery")
    commit, lane, ordinal, channel, destination_root, payload_root, destination, payload, identity, entry = row
    return {"schema": "zeno-fcis/relay-export/1", "commit": commit, "lane": lane,
            "ordinal": ordinal, "channel": channel, "destination_root": destination_root,
            "payload_root": payload_root, "destination": bytes(destination).hex(),
            "payload": bytes(payload).hex(), "delivery_id": bytes(identity).hex(),
            "entry_hash": bytes(entry).hex(), "payload_sha256": hashlib.sha256(payload).hexdigest()}


def report(binary: Path, arguments: list[str], label: str) -> dict:
    return upgrades.report(upgrades.run_app(binary, arguments), label)


def check(directory: Path, packages: dict, version: str,
          executed_binary: Path, created_binary: Path) -> dict:
    directory.mkdir()
    target = split_contract(directory / "split")
    valid = mapping()
    migration = migrations.write_json(directory / "mapping.json", valid)
    refusal_app = directory / "refusal-app"
    applications.run([migrations.cli(), "new", str(refusal_app), "--contract", str(SPEND)], ROOT)
    refused = {}
    for label, replacement, words in (
        ("missing", {}, ["payload"]),
        ("partial", {"145": {"from": 145, "map": {"0": False, "1": True, "2": False}},
                     "147": valid["payloads"]["300"]["147"]}, ["map gives no value"]),
        ("lossy", {"145": valid["payloads"]["300"]["145"],
                   "147": {"expr": "old.145 < 0"}}, ["payload"]),
        ("trap", {"145": valid["payloads"]["300"]["145"],
                  "147": {"expr": "old.145 + 9223372036854775807 > 0"}}, ["payload"]),
        ("wrong-injective", {"145": {"expr": "old.145 == 0 || old.145 == 2"},
                             "147": valid["payloads"]["300"]["147"]}, ["`deliveries` differs"]),
    ):
        invalid = json.loads(json.dumps(valid))
        invalid["payloads"] = {} if label == "missing" else {"300": replacement}
        path = migrations.write_json(directory / f"{label}.json", invalid)
        refused[label] = migrations.refused_evolution(refusal_app, target, ["--migration", str(path)],
                                                     words, label)

    stores = {}
    for name, binary in (("pending", executed_binary), ("continuation", created_binary)):
        store = directory / f"{name}.sqlite"
        report(binary, ["--decide", str(store)], f"create {name} store")
        stores[name] = store
    pending = stores["pending"]
    original = outgoing(pending)
    history = retained_history(pending)
    binary, (evolved,) = migrations.built(directory, "payload-app", SPEND, packages, version,
                                         [(target, ["--migration", str(migration)])])
    upgrades.expect(evolved, {"status": "evolved", "kind": "layout-change"}, "payload evolution")
    if evolved["evolution"]["migration"]["tuples_compared"] != 14_592:
        raise RuntimeError("payload evolution did not compare the entire old domain")
    for name, store in stores.items():
        before = store.read_bytes()
        audit = report(binary, ["--audit", str(store)], f"audit {name} before upgrade")
        upgrades.expect(audit, {"contract_version": 1, "upgrades": 0}, name)
        if store.read_bytes() != before:
            raise RuntimeError("pre-upgrade read-only audit wrote to the store")
        upgraded = report(binary, ["--upgrade", str(store)], f"upgrade {name}")
        upgrades.expect(upgraded, {"kind": "migration", "contract_version": 2, "upgrades": 1}, name)
    if outgoing(pending) != original or retained_history(pending) != history:
        raise RuntimeError("payload upgrade rewrote original history or a physical delivery token")

    # Corrupt the stored full migration version or a payload program byte.
    # Neither forged description is allowed to open or write on fresh replay.
    for label in ("unknown-version", "tampered-payload"):
        forged = directory / f"{label}.sqlite"
        shutil.copyfile(pending, forged)
        with sqlite3.connect(forged) as connection:
            encoded = bytearray(connection.execute("SELECT publication FROM v2_upgrades WHERE ordinal=1").fetchone()[0])
            description_size = int.from_bytes(encoded[:8], "big")
            offset = 8 + len(b"ZFCIS-MIGRATION\0") if label == "unknown-version" else 8 + description_size - 1
            encoded[offset] ^= 1
            connection.execute("UPDATE v2_upgrades SET publication=? WHERE ordinal=1", (bytes(encoded),))
        migrations.refused_without_write(binary, ["--audit", str(forged)], forged, "History", label)

    # The real packaged relay has already sent, but must report failure before
    # acknowledgment at the configured crash point. Retry sends identical bytes.
    queue = directory / "relay.jsonl"
    config = directory / "relay.json"
    migrations.write_json(config, {"schema": "zeno-fcis/relay-destination/1",
                                   "queue": queue.name, "crash_at": "after-send"})
    before = pending.read_bytes()
    crashed = upgrades.run_app(binary, ["deliver", str(pending), "--relay", str(config),
                                       "--format", "json"])
    if crashed.returncode != 3 or "stays pending" not in crashed.stderr:
        raise RuntimeError(f"send before ack: expected transport failure 3 naming stays pending: "
                           f"exit {crashed.returncode}, stderr {crashed.stderr.strip()!r}")
    if pending.read_bytes() != before or outgoing(pending) != original:
        raise RuntimeError("relay send failure acknowledged or rewrote the historical delivery")
    migrations.write_json(config, {"schema": "zeno-fcis/relay-destination/1", "queue": queue.name})
    delivered = report(binary, ["deliver", str(pending), "--relay", str(config), "--format", "json"],
                       "relay the historical payment")
    upgrades.expect(delivered, {"status": "delivered", "pending": 0,
                                "deliveries": [original["delivery_id"]]}, "historical delivery")
    sent = [json.loads(line) for line in queue.read_text().splitlines()]
    original_body = {**original, "schema": "zeno-fcis/relay-delivery/1"}
    if sent != [original_body, original_body] or retained_history(pending) != history:
        raise RuntimeError("historical relay changed its interpretation, identity or replay bytes")

    continuation = stores["continuation"]
    continued = report(binary, ["--decide", str(continuation)], "continue after payload split")
    upgrades.expect(continued, {"decisions": ["Accept", "Accept", "Reject"], "pending": 1}, "continuation")
    future = outgoing(continuation)
    if future["payload"] == original["payload"]:
        raise RuntimeError("future payment did not use the declared split payload representation")
    report(binary, ["deliver", str(continuation), "--relay", str(config), "--format", "json"], "relay new payment")
    sent = [json.loads(line) for line in queue.read_text().splitlines()]
    future_body = {**future, "schema": "zeno-fcis/relay-delivery/1"}
    if sent != [original_body, original_body, future_body]:
        raise RuntimeError("relay did not preserve the old and new deliveries in order")
    for name, store in stores.items():
        audit = report(binary, ["--audit", str(store)], f"reopen {name}")
        upgrades.expect(audit, {"contract_version": 2, "commits": 3, "pending": 0, "upgrades": 1}, name)
    return {"status": "passed", "authority": "none", "map": "total-injective-bounded",
            "original": original, "future": future, "relay": delivered,
            "refused": sorted(refused), "tuples": 14_592}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--keep", type=Path, help="retain artifacts in a new directory")
    arguments = parser.parse_args()
    os.environ.setdefault("CARGO_BUILD_JOBS", "2")
    os.environ.setdefault("CARGO_INCREMENTAL", "0")
    os.environ.setdefault("CARGO_PROFILE_DEV_DEBUG", "0")
    os.environ.setdefault("CARGO_PROFILE_TEST_DEBUG", "0")
    os.environ.setdefault("CARGO_TARGET_DIR", str(ROOT / "target"))
    applications.run(["cargo", "+1.97.1", "build", "-p", "zeno-fcis-cli", "--locked", "--offline"], ROOT)
    packages = {tomllib.loads(path.read_text())["package"]["name"]: path.parent
                for path in sorted((ROOT / "crates").glob("*/Cargo.toml"))}
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]

    def run(directory: Path) -> dict:
        binaries = []
        for name, session in (("executed", migrations.EXECUTED), ("created", migrations.CREATED)):
            source = migrations.contract(directory / f"{name}-source", SPEND, session)
            binary, _ = migrations.built(directory, name, source, packages, version, [])
            binaries.append(binary)
        return check(directory / "payload-journey", packages, version, *binaries)

    if arguments.keep:
        directory = arguments.keep.resolve()
        directory.mkdir()
        result = run(directory)
    else:
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-payload-migrate-") as directory:
            result = run(Path(directory))
    print(json.dumps({"schema": "zeno-fcis/payload-migrate-check/1", **result}, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, RuntimeError, subprocess.CalledProcessError, KeyError, ValueError) as error:
        print(f"payload migrate: FAIL: {error}", file=sys.stderr)
        sys.exit(1)
