#!/usr/bin/env python3
"""Prepare C1 expectations and L207 intake metadata; perform no native replay."""
from __future__ import annotations

import dataclasses
import hashlib
import json
from pathlib import Path

from build_contracts import APP
from native_check import world_json
from spot_reference import Command, N, U, Pool, World, step
from run_native_checks import seed

DEX_PIN = "c86221afa07462955955ff05e2ce5f81e13e78b3"


def zero_payout_labels():
    labels = {"floor-(9999, 20003)-1", "floor-(9999, 9999)-1"}
    groups = [((1,1),1,(1,)), ((1,1),999,(1,999)), ((1,1),1000000,(1,999,1000000)),
              ((7,9999),1,(1,)), ((7,9999),999,(1,)), ((7,9999),1000000,(1,999)),
              ((10001,20003),1000000,(1,))]
    for reserves,shares,burns in groups:
        for extra in (0,50):
            for burn in burns: labels.add(f"grid-{reserves}-{shares}-{extra}-{burn}")
    if len(labels) != 24: raise ValueError("the owner-policy rebase must identify exactly 24 labels")
    return sorted(labels)


def expectation(decision):
    return {"reason":decision.reason,"post":world_json(decision.post),
            "effects":[dataclasses.asdict(e) for e in decision.effects],"outputs":list(decision.outputs)}


def main():
    base = seed()
    absent = World(None,(0,0,0,0,10**12,10**12,10**12,10**12))
    cases = [("retained-v8-1",base,Command(1,a=1000),None)]
    cases += [
        ("zero-net",base,Command(1,a=1),"ZERO_NET"),
        ("last-zero-protocol",base,Command(1,a=1333),None),
        ("first-protocol",base,Command(1,a=1334),None),
        ("six-protocol",base,Command(1,a=10000),None),
        ("other-direction",base,Command(1,direction=1,a=10000),None),
        ("inverse-protocol",base,Command(2,a=10000,limit0=N),None),
        ("two-atom-inverse",seed(1,2,1001),Command(2,a=1,limit0=N),None),
        ("drain",base,Command(2,a=1000000,limit0=N),"DRAIN"),
        ("gross-cap-conservative",seed(N-10000),Command(1,a=10006),"POST_DOMAIN"),
        ("gross-cap-last-fit",seed(N-10000),Command(1,a=10000),None),
        ("ingress-last-fit",seed(ingress=U-6),Command(1,a=10000),None),
        ("ingress-first-overflow",seed(ingress=U-5),Command(1,a=10000),"INGRESS_OVERFLOW"),
        ("zero-protocol-full-ingress",seed(ingress=U),Command(1,a=1333),None),
        ("reserved-actor",base,Command(1,a=10000,actor=4),"RESERVED_ROLE"),
        ("reserved-other-ingress-recipient",base,Command(1,a=10000,recipient=5),"RESERVED_ROLE"),
        ("reserved-lock-first",base,Command(4,a=1,actor=3,recipient=5),"RESERVED_LP_LOCK"),
        ("create-at-lock",absent,Command(0,a=1000,b=1000,sqrt=1000),"INITIAL_TOO_SMALL"),
        ("create-above-lock",absent,Command(0,a=1001,b=1001,sqrt=1001),None),
        ("bad-sqrt-before-size",absent,Command(0,a=1001,b=1001,sqrt=1000),"BAD_SQRT"),
        ("pair-unique",base,Command(0,a=1001,b=1001,sqrt=1001),"POOL_EXISTS"),
        ("existing-empty-add",seed(0,0,0),Command(3,a=1,b=1),"EMPTY_POOL"),
        ("frozen",dataclasses.replace(base,pool=Pool(1000000,1000000,1000000,1)),Command(1,a=10000),"POOL_INACTIVE"),
        ("disabled",dataclasses.replace(base,pool=Pool(1000000,1000000,1000000,2)),Command(1,a=10000),"POOL_INACTIVE"),
        ("zero-payout",seed(1,1,1001),Command(4,a=1),"ZERO_PAYOUT"),
        ("zero-payout-before-minima",seed(1,1,1001),Command(4,a=1,limit0=1,limit1=1),"ZERO_PAYOUT"),
        ("insufficient-lp-first",seed(1001,1001,1001),Command(4,a=2),"INSUFFICIENT_LP"),
        ("positive-burn",seed(1001,1001,1001),Command(4,a=1),None),
        ("add-minimum",seed(1000,2000,1414),Command(3,a=100,b=100,limit0=51),"SLIPPAGE"),
        ("zero-used",seed(1,1000000,1001),Command(3,a=1,b=1),"ZERO_USED"),
        ("bad-mirror-before-role",dataclasses.replace(base,balances=(1000001,*base.balances[1:])),Command(1,a=1,actor=4),"DOMAIN"),
    ]
    if len(cases) != 31: raise ValueError("C1 must contain one retained record and 30 boundaries")
    rows=[]
    for index,(label,world,command,reason) in enumerate(cases):
        decision=step(world,command)
        if decision.reason != reason: raise ValueError(f"{label}: expected {reason}, got {decision.reason}")
        if index == 0 and decision.outputs != (1000,996,3,0,3,997,0,0): raise ValueError("retained C1 arithmetic changed")
        rows.append({"schema":"spot-corpus-row/1","id":f"C1/{label}","group":"retained" if index==0 else "boundary",
                     "disposition":"replay","source_pin":DEX_PIN if index==0 else "SPOT_CORE_SPEC.md + OWNER_ANSWERS.md",
                     "expectation_origin":"fixed-policy big-integer oracle; not a historical byte/root golden",
                     "pre":world_json(world),"command":dataclasses.asdict(command),"expect":expectation(decision)})
    directory=APP/"corpus"
    directory.mkdir(exist_ok=True)
    c1=directory/"c1.jsonl"
    c1.write_text("".join(json.dumps(row,sort_keys=True,separators=(',',':'))+"\n" for row in rows))
    plan={"schema":"spot-corpus-plan/1","authority":"none","native_replay":"not run",
          "C1":{"rows":31,"retained":1,"boundaries":30,"sha256":hashlib.sha256(c1.read_bytes()).hexdigest()},
          "L207":{"status":"awaiting normalized source export; no DEX repository accessed",
                  "source_pin":DEX_PIN,"source_path":"tests/core/test_spot_lp_burn_global_rust_v2.py",
                  "rows":209,"burns":207,"swaps":2,
                  "groups":{"floor":88,"guard":103,"history":8,"capacity":8,"byte-capacity":2},
                  "zero_payout_rebases":zero_payout_labels(),
                  "out_of_scope":["nonce","time","encoding","resource","capacity","context-only"],
                  "old_roots":"never compare to this profile's new bytes"}}
    (directory/"plan.json").write_text(json.dumps(plan,indent=2)+"\n")
    print(json.dumps({"prepared":31,"l207_input":"pending","native_replay":"not run","authority":"none"}))


if __name__ == "__main__": main()
