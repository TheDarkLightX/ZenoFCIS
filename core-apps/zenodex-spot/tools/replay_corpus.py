#!/usr/bin/env python3
"""Validate corpus intake, or explicitly run a source-bound native replay later.

The default plan command only inspects files. It never starts the CLI. A replay
requires --allow-replay and a complete normalized L207 source export, with every
row's applicability and policy rebase explicit. No toolchain or DEX access.
"""
from __future__ import annotations

import argparse
import collections
import dataclasses
import hashlib
import json
from pathlib import Path

from build_contracts import APP, REASONS, source_hash
from native_check import decode_command, decode_world, expected_observation, probe, strict_json
from prepare_corpus import DEX_PIN, zero_payout_labels
from spot_reference import Decision, Effect, step
from check_bundle import check_sources
from prepare_l207 import digest

INTEGRATION_CLI = Path.home()/".cache/zf-targets/v250/debug/zeno-fcis"


def require_cli_identity(cli, wanted):
    actual = hashlib.sha256(cli.read_bytes()).hexdigest()
    if actual != wanted:
        raise ValueError("CLI identity changed; run is UNKNOWN and cannot count as a source-bound replay")
    return actual


def expected(row):
    value=row["expect"]
    if type(value) is not dict or set(value) != {"reason","post","effects","outputs"}: raise ValueError("closed expected observation required")
    if value["reason"] is not None and type(value["reason"]) is not str: raise ValueError("reject reason must be text")
    if value["reason"] is not None and value["reason"] not in REASONS: raise ValueError("unknown reject reason")
    if type(value["effects"]) is not list or type(value["outputs"]) is not list: raise ValueError("effect/output lists required")
    effects=[]
    for e in value["effects"]:
        if type(e) is not dict or set(e) != set(Effect.__dataclass_fields__) or any(type(v) is not int for v in e.values()):
            raise ValueError("closed scalar effect required")
        effects.append(Effect(**e))
    if any(type(v) is not int for v in value["outputs"]): raise ValueError("outputs must be exact integers")
    if value["reason"] is None and len(value["outputs"]) != 8: raise ValueError("accepted output has eight positions")
    if value["reason"] is not None and (effects or value["outputs"]): raise ValueError("reject effects and outputs must be empty")
    if any(e.channel not in (300,301,302) or e.amount==0 for e in effects): raise ValueError("zero or unknown economic effect")
    return Decision(value["reason"],decode_world(value["post"]),tuple(effects),tuple(value["outputs"]))


def load_rows(path, family):
    rows=[strict_json(line) for line in path.read_text().splitlines() if line.strip()]
    return validate_rows(rows, family)


def validate_rows(rows, family):
    if family not in ("C1", "L207"): raise ValueError("unknown corpus family")
    if any(type(row) is not dict for row in rows): raise ValueError("corpus row must be an object")
    ids=[row["id"] for row in rows]
    if any(type(value) is not str or not value for value in ids): raise ValueError("corpus labels must be nonempty text")
    if len(ids) != len(set(ids)): raise ValueError(f"{family}: duplicate corpus ID")
    if len(rows) != (31 if family=="C1" else 209): raise ValueError(f"{family}: incomplete corpus row count")
    for row in rows:
        if row.get("schema") != "spot-corpus-row/1": raise ValueError("unknown corpus schema")
        if row.get("disposition") == "inapplicable":
            if family == "C1": raise ValueError("C1 rows cannot be skipped")
            if row.get("scope_reason") not in ("nonce","time","encoding","resource","capacity","context-only") or not row.get("scope_evidence"):
                raise ValueError("inapplicable row needs a named exclusion and source evidence")
            if any(key in row for key in ("expect", "pre", "command")):
                raise ValueError("inapplicable row cannot carry a replay golden or input")
            continue
        if row.get("disposition") != "replay": raise ValueError("every row needs an explicit disposition")
        before=decode_world(row["pre"]); command=decode_command(row["command"]); wanted=expected(row)
        if wanted.reason is not None and wanted.post != before: raise ValueError("a reject must retain the complete pre-state")
        if expected_observation(step(before, command)) != expected_observation(wanted):
            raise ValueError("malformed or false golden: independent oracle differs")
    if family=="C1":
        if collections.Counter(row["group"] for row in rows) != {"retained":1,"boundary":30}: raise ValueError("C1 group counts differ")
    else:
        if any(row.get("source_pin") != DEX_PIN for row in rows): raise ValueError("L207 source pin differs")
        groups=collections.Counter(row["group"] for row in rows)
        if groups != {"floor":88,"guard":103,"history":8,"capacity":8,"byte-capacity":2}: raise ValueError("L207 group counts differ")
        actions=collections.Counter(row.get("source_command") for row in rows)
        if actions != {"RemoveLiquidity":207,"SwapExactIn":2}: raise ValueError("L207 source command census differs")
        inventory=strict_json((APP/"corpus/l207-inventory.json").read_text())
        if inventory["source_pin"] != DEX_PIN: raise ValueError("L207 inventory source pin differs")
        retained={row["id"]:row for row in inventory["rows"]}
        if len(retained) != 209 or set(ids) != set(retained): raise ValueError("L207 original label census differs")
        for row in rows:
            original=retained[row["id"]]
            if any(row.get(key) != original[key] for key in ("group","source_command","source_record_sha256","disposition")):
                raise ValueError("L207 original row or disposition differs")
            if digest(row) != original["normalized_sha256"]: raise ValueError("L207 normalized row/golden differs from reviewed intake")
        by_id={row["id"]:row for row in rows}
        for label in zero_payout_labels():
            row=by_id.get(label)
            if row is None or row.get("disposition") != "replay" or row.get("expect",{}).get("reason") != "ZERO_PAYOUT" or not row.get("policy_rebase"):
                raise ValueError(f"missing explicit owner-policy ZERO_PAYOUT rebase: {label}")
        history=sorted((row for row in rows if row["group"]=="history"),key=lambda row:row["history_index"])
        if [row["history_index"] for row in history] != list(range(8)): raise ValueError("history order must account for all eight source steps")
        for previous,current in zip(history,history[1:]):
            if previous.get("history_post") != current.get("history_pre") or not previous.get("history_post"):
                raise ValueError("L207 source history is discontinuous across an excluded step")
            if previous["disposition"]==current["disposition"]=="replay" and expected(previous).post != decode_world(current["pre"]):
                raise ValueError("L207 history is discontinuous after policy rebase")
    return rows


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode",choices=("plan","replay"))
    parser.add_argument("--c1",type=Path,default=APP/"corpus/c1.jsonl")
    parser.add_argument("--l207",type=Path)
    parser.add_argument("--out",type=Path,required=True)
    parser.add_argument("--allow-replay",action="store_true")
    parser.add_argument("--cli",type=Path,default=INTEGRATION_CLI)
    args=parser.parse_args()
    c1=load_rows(args.c1,"C1")
    l207=[] if args.l207 is None else load_rows(args.l207,"L207")
    summary={"authority":"none","C1_rows":len(c1),"L207_rows":len(l207),"L207_status":"pending export" if not l207 else "accounted",
             "native_replay":"not run","acceptance":"pending", "application_sources":source_hash(APP),
             "c1_sha256":hashlib.sha256(args.c1.read_bytes()).hexdigest()}
    args.out.mkdir(parents=True,exist_ok=True)
    if args.mode == "replay":
        if not args.allow_replay: raise SystemExit("replay is disabled; the lead must grant runs and select --allow-replay")
        if not l207: raise SystemExit("complete L207 export is required; missing rows cannot count as passes")
        pins=check_sources()
        cli_identity = require_cli_identity(args.cli, pins["cli"]["sha256"])
        matched=0; skipped=[]
        for index,row in enumerate(c1+l207):
            if row["disposition"] == "inapplicable":
                skipped.append({"id":row["id"],"reason":row["scope_reason"],"evidence":row["scope_evidence"]}); continue
            world,command,wanted=decode_world(row["pre"]),decode_command(row["command"]),expected(row)
            reference=step(world,command)
            if expected_observation(reference) != expected_observation(wanted): raise ValueError(f"{row['id']}: independent oracle differs from policy expectation")
            require_cli_identity(args.cli, cli_identity)
            probe(world,command,wanted,args.out/f"row-{index:03}","native",cli=args.cli)
            require_cli_identity(args.cli, cli_identity)
            matched+=1
        summary.update({"native_replay":"completed","matched":matched,"mismatches":0,"inapplicable":skipped,
                        "acceptance":"scoped economic replay only; no mount/recovery/resource claim",
                        "cli_path":str(args.cli),"cli_sha256":require_cli_identity(args.cli, cli_identity),
                        "l207_sha256":hashlib.sha256(args.l207.read_bytes()).hexdigest()})
    (args.out/"summary.json").write_text(json.dumps(summary,indent=2)+"\n")
    print(json.dumps(summary))


if __name__ == "__main__": main()
