#!/usr/bin/env python3
"""Translate the pinned retained source export into scoped M1 economic rows.

Original full records stay private in the evidence archive. No historical root
is a new-policy golden. Unaffected external accounts and metadata are explicitly
projected away; a third participating principal cannot be silently aliased.
"""
import argparse
import collections
import dataclasses
import hashlib
import json
from pathlib import Path

from build_contracts import APP
from native_check import strict_json, world_json
from prepare_corpus import DEX_PIN, expectation, zero_payout_labels
from spot_reference import Command, Decision, Effect, Pool, World, mc, mf, step

GROUPS = {"floor": 88, "guard": 103, "history": 8, "capacity": 8, "byte-capacity": 2}
LOCK = "0x" + "0" * 96
ALICE, BOB = "0x" + "a" * 96, "0x" + "b" * 96


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def digest(value):
    return hashlib.sha256(canonical(value).encode()).hexdigest()


def projection(spot, state, owners):
    pool = spot["pool"]
    balances = {(row["owner"], row["asset"], row["custody_domain"]): row["amount_atoms"] for row in state["balances"]}
    custody = {(row["owner"], row["asset"], row["custody_domain"]): row["amount_atoms"] for row in state["custody"]}
    lp = {row["owner"]: row["shares"] for row in spot["lp_positions"]}
    if any(value and owner not in (*owners, LOCK) for owner, value in lp.items()):
        raise ValueError("more than two participating ordinary LP principals")
    status = {"ACTIVE": 0, "FROZEN": 1, "DISABLED": 2}[pool["status"]]
    amounts = tuple(custody.get((pool["pool_id"], asset, "spot_pool"), 0) for asset in ("A", "B"))
    amounts += (0, 0) + tuple(balances.get((owner, asset, "accounts"), 0) for owner in owners for asset in ("A", "B"))
    return World(Pool(pool["reserve0"], pool["reserve1"], pool["lp_supply"], status), amounts,
                 tuple(lp.get(owner, 0) for owner in owners) + (lp.get(LOCK, 0),))


def scope(row):
    label, group, code = row["id"], row["group"], row["expect"].get("code")
    if group in ("capacity", "byte-capacity"):
        return "capacity", "retained row tests nonce/row/byte capacity outside M1"
    if code in ("INVALID_NONCE", "REPLAY_ALREADY_CONSUMED"):
        return "nonce", f"historical {code}; M1 has no nonce/replay command state"
    if code in ("INVALID_PARAMS", "UNSUPPORTED_FIELDS", "INVALID_RECIPIENT", "INVALID_SENDER"):
        return "encoding", f"historical {code}; raw signed shape/identity has no scalar mounting decoder"
    if code == "EXPIRED" or label in ("boundary-time", "u64-deadline-and-time"):
        return "time", "signed deadline/timestamp outside M1"
    if label in ("unicode-salt-at-character-limit", "salt-json-escaping"):
        return "encoding", "signed salt/byte encoding outside M1"
    if code in ("OCCURRENCE_CONTEXT_MISMATCH", "OCCURRENCE_COMMAND_MISMATCH", "SUBJECT_MISMATCH",
                "POOL_MISMATCH", "UNSUPPORTED_ASSET_POLICY", "PROJECTION_MISMATCH", "ASSET_ORIGIN_MISMATCH") or label == "all-occurrence-fields":
        return "context-only", f"historical {code or label}; authenticated global framing/policy outside closed M1"
    if label == "status-before-sender-alias":
        return "encoding", "invalid sender spelling cannot be represented by a canonical scalar account slot"
    return None


def normalize(original):
    rows, inventory = [], []
    previous_history = None
    history_index = 0
    for source in original:
        pre, old = source["pre"], source["expect"]
        label, group = source["id"], source["group"]
        if old.get("status") not in ("accepted", "rejected"):
            raise ValueError("malformed original golden status")
        if old["status"] == "rejected" and (type(old.get("code")) is not str or "effects" not in old):
            raise ValueError("malformed original reject golden")
        post_source = {"assets": old["post_assets"], "spot": old["post_spot"], "state": old["post_state"]} if old["status"] == "accepted" else {key: pre[key] for key in ("assets", "spot", "state")}
        base = {"schema": "spot-corpus-row/1", "id": label, "group": group, "source_pin": DEX_PIN,
                "source_command": "SwapExactIn" if source["kind"] == "swap" else "RemoveLiquidity",
                "source_record_sha256": digest(source)}
        if group == "history":
            pre_digest = digest({key: pre[key] for key in ("assets", "spot", "state")})
            if previous_history is not None and previous_history != pre_digest:
                raise ValueError("original source history is discontinuous")
            previous_history = digest(post_source)
            base.update(history_index=history_index, history_pre=pre_digest, history_post=previous_history)
            history_index += 1
        exclusion = scope(source)
        fields = pre["intent"]["fields"]
        actor = pre["intent"]["sender_pubkey"]
        recipient = fields.get("recipient", actor)
        owners = [ALICE, BOB] if group == "history" else sorted({row["owner"] for row in pre["spot"]["lp_positions"] if row["owner"] != LOCK and row["shares"]})
        if exclusion is None:
            for owner in (actor, recipient):
                if owner != LOCK and owner not in owners: owners.append(owner)
            while len(owners) < 2: owners.append("unused:ordinary")
            if len(owners) > 2:
                exclusion = "context-only", "three participating principals cannot fit two ordinary slots without aliasing"
        if exclusion is not None:
            base.update(disposition="inapplicable", scope_reason=exclusion[0],
                        scope_evidence=f"{group}/{label}: {exclusion[1]}; source record {digest(source)}")
        else:
            before = projection(pre["spot"], pre["state"], owners)
            slot = lambda owner: 3 if owner == LOCK else owners.index(owner)
            swap = source["kind"] == "swap"
            command = Command(1 if swap else 4, direction=0 if not swap or fields["asset_in"] == "A" else 1,
                              a=fields["amount_in" if swap else "lp_amount"],
                              limit0=fields.get("min_amount_out" if swap else "amount0_min", 0),
                              limit1=0 if swap else fields.get("amount1_min", 0), actor=slot(actor), recipient=slot(recipient))
            if label in zero_payout_labels():
                if old["status"] != "accepted": raise ValueError("ZERO_PAYOUT rebase must start at a historical accept")
                wanted = Decision("ZERO_PAYOUT", before)
                rebase = "O1: either zero floor payout now refuses before minima; no state/effects"
            elif old["status"] == "rejected":
                mapping = {"SLIPPAGE_LIMIT": "SLIPPAGE", "POOL_INACTIVE": "POOL_INACTIVE",
                           "INSUFFICIENT_LP": "INSUFFICIENT_LP", "RESERVED_LP_LOCK": "RESERVED_LP_LOCK"}
                if old["code"] not in mapping: raise ValueError(f"unexplained exclusion/reject mapping: {label}")
                wanted = Decision(mapping[old["code"]], before)
                rebase = f"V4 diagnostic mapping {old['code']} -> {wanted.reason}"
            else:
                post = projection(old["post_spot"], old["post_state"], owners)
                for key in ("balances",):
                    outside = lambda state: [r for r in state[key] if r["owner"] not in owners]
                    if outside(pre["state"]) != outside(old["post_state"]):
                        raise ValueError("projection would discard an affected account")
                if swap:
                    direction = command.direction
                    paid = (before.pool.r1 - post.pool.r1) if direction == 0 else (before.pool.r0 - post.pool.r0)
                    fee = mc(command.a, 30, 10000); protocol = mf(fee, 2000, 10000)
                    if protocol != 0: raise ValueError("retained history unexpectedly needs a nonzero protocol rebase")
                    effects = (Effect(300 + direction, command.actor, 2, command.a),
                               Effect(301 - direction, 2, command.recipient, paid))
                    outputs = (command.a, paid, fee, protocol, fee - protocol, command.a - fee, 0, 0)
                else:
                    o0, o1 = before.pool.r0 - post.pool.r0, before.pool.r1 - post.pool.r1
                    effects = (Effect(302, command.actor, command.actor, -command.a),)
                    effects += tuple(Effect(300 + i, 2, command.recipient, paid) for i, paid in enumerate((o0, o1)) if paid)
                    outputs = (o0, o1, 0, 0, 0, 0, 0, 0)
                wanted = Decision(None, post, effects, outputs)
                rebase = "fixed f=30,p=2000; retained economic floors; metadata/old roots excluded"
            if step(before, command) != wanted:
                raise ValueError(f"{label}: independent Spot oracle differs from retained/rebased golden")
            base.update(disposition="replay", pre=world_json(before), command=dataclasses.asdict(command),
                        expect=expectation(wanted), policy_rebase=rebase,
                        projection={"ordinary_owners": owners, "excluded": "unchanged nonparticipating balances; global/LP metadata and old roots"})
        rows.append(base)
        inventory.append({key: base[key] for key in ("id", "group", "source_command", "source_record_sha256", "disposition")})
        inventory[-1]["normalized_sha256"] = digest(base)
    return rows, inventory


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--inventory", type=Path, required=True)
    parser.add_argument("--out", type=Path, default=APP / "corpus")
    args = parser.parse_args()
    source_inventory = strict_json(args.inventory.read_text())
    if source_inventory["source_pin"] != DEX_PIN or hashlib.sha256(args.source.read_bytes()).hexdigest() != source_inventory["original_sha256"]:
        raise ValueError("original export/source pin differs")
    original = [strict_json(line) for line in args.source.read_text().splitlines()]
    if len(original) != 209 or len({row["id"] for row in original}) != 209 or collections.Counter(row["group"] for row in original) != GROUPS:
        raise ValueError("original source census differs")
    if [(row["id"], row["group"], digest(row)) for row in original] != [(row["id"], row["group"], row["sha256"]) for row in source_inventory["rows"]]:
        raise ValueError("original label/group/golden differs")
    rows, inventory = normalize(original)
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "l207.jsonl").write_text("".join(canonical(row) + "\n" for row in rows))
    (args.out / "l207-inventory.json").write_text(json.dumps({"source_pin": DEX_PIN, "source_export_sha256": source_inventory["original_sha256"], "rows": inventory}, indent=2) + "\n")
    print(json.dumps({"rows": len(rows), "groups": dict(collections.Counter(row["group"] for row in rows)),
                      "dispositions": dict(collections.Counter(row["disposition"] for row in rows)), "ZERO_PAYOUT": len(zero_payout_labels())}))


if __name__ == "__main__":
    main()
