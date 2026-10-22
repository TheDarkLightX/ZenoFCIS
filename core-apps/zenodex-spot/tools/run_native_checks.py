#!/usr/bin/env python3
"""Bounded authoring checks against actual Authority probes, not corpus replay."""
from __future__ import annotations

import argparse
import dataclasses
import hashlib
import json
import subprocess
from pathlib import Path

from build_contracts import APP, REASONS, source_hash
from native_check import parse_observation, probe
from run_judges import PINNED_CLI
from spot_reference import Command, K, N, U, Pool, World, balance_step, limbs, step


def seed(r0=1000000, r1=1000000, supply=1000000, ingress=0):
    return World(Pool(r0,r1,supply), (r0,r1,ingress,0,10**12,10**12,10**12,10**12),
                 (max(supply-K,0),0,min(supply,K)))


def journey(out):
    world = World(None,(0,0,0,0,10**12,10**12,10**12,10**12))
    commands = (Command(0,a=100000,b=100000,sqrt=100000), Command(1,a=10000,recipient=1),
                Command(2,direction=1,a=1000,limit0=N,actor=1,recipient=0), Command(3,a=10000,b=10000), Command(4,a=500))
    for i, command in enumerate(commands):
        result = step(world,command)
        if result.reason is not None: raise ValueError(f"journey reference step {i}: {result.reason}")
        probe(world,command,result,out,f"journey-{i}")
        world = result.post
    return len(commands)


def boundaries(out):
    base = seed()
    rows = [
        ("fee-first-extraction",base,Command(1,direction=1,a=1334),None),
        ("ingress-last-fit",seed(ingress=U-6),Command(1,a=10000),None),
        ("ingress-overflow",seed(ingress=U-5),Command(1,a=10000),"INGRESS_OVERFLOW"),
        ("zero-protocol-at-u128-max",seed(ingress=U),Command(1,a=1333),None),
        ("zero-net",base,Command(1,a=1),"ZERO_NET"),
        ("zero-output",seed(1000000,1),Command(1,a=2),"ZERO_OUTPUT"),
        ("conservative-gross-cap",seed(N-10000,1000000),Command(1,a=10006),"POST_DOMAIN"),
        ("lock-before-recipient-role",base,Command(4,a=1,actor=3,recipient=5),"RESERVED_LP_LOCK"),
        ("burn-zero-before-minimum",seed(1,1,1001),Command(4,a=1,limit0=1,limit1=1),"ZERO_PAYOUT"),
        ("quote-cap",seed(1,10000,1001),Command(2,a=100,limit0=N),"QUOTE_GAP"),
        ("slippage-before-quote-cap",seed(1,10000,1001),Command(2,a=100,limit0=1),"SLIPPAGE"),
        ("zero-used",seed(1,1000000,1001),Command(3,a=1,b=1),"ZERO_USED"),
        ("zero-mint",seed(1000000,1000000,1000),Command(3,a=1,b=1),"ZERO_MINT"),
    ]
    # Canonical key order is pool, ingress, alice, bob. Earlier ingress overflow
    # wins over a later unfunded actor; no partial Pool/Balance state escapes.
    bad = dataclasses.replace(seed(ingress=U-5), balances=(1000000,1000000,U-5,0,0,10**12,10**12,10**12))
    rows.append(("canonical-key-precedence",bad,Command(1,a=10000),"INGRESS_OVERFLOW"))
    for label,world,command,reason in rows:
        result = step(world,command)
        if result.reason != reason: raise ValueError(f"{label}: reference reason {result.reason}, expected {reason}")
        probe(world,command,result,out,label)
    return len(rows)


def balance_boundaries(out):
    rows = [(v,0,1,False,0) for v in ((1<<32)-1,(1<<64)-1,(1<<96)-1)]
    rows += [((1<<96),1,0,False,0), (1,2,100,False,0), (U,0,1,False,0), (U,1,1,True,1), (U-6,0,6,True,6)]
    for i, (balance,debit,credit,ingress,expected) in enumerate(rows):
        values = limbs(balance) + limbs(debit) + limbs(credit) + (int(ingress),expected)
        label = f"balance-{i}"
        target = out / f"{label}.json"
        argv = [str(PINNED_CLI),"--no-kb","contract","probe","explain",str(APP/"components/balance"),
                "--input"," ".join(map(str,values)),"--out",str(target)]
        run = subprocess.run(argv,capture_output=True,text=True,timeout=20,check=False)
        (out/f"{label}.command.json").write_text(json.dumps({"argv":argv,"exit_code":run.returncode,"stdout":run.stdout,"stderr":run.stderr},indent=2)+"\n")
        if run.returncode: raise ValueError(f"{label}: {run.stderr}")
        authority = json.loads(target.read_text())["result"]["authority"]
        if "example" not in authority: raise ValueError(f"{label}: library refused {authority}")
        reason, post = balance_step(balance,debit,credit,ingress,expected)
        wanted = ("accept" if reason is None else "reject", None if reason is None else REASONS[reason], limbs(post), ())
        if parse_observation(authority["example"]) != wanted: raise ValueError(f"{label}: mismatched u128 observation")
    return len(rows)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out",type=Path,required=True)
    parser.add_argument("--group",choices=("journey","boundaries","balance"),required=True)
    args=parser.parse_args(); args.out.mkdir(parents=True,exist_ok=True)
    count={"journey":journey,"boundaries":boundaries,"balance":balance_boundaries}[args.group](args.out)
    result={"group":args.group,"matched":count,"mismatches":0,"authority":"none",
            "cli_sha256":hashlib.sha256(PINNED_CLI.read_bytes()).hexdigest(),"application_sources":source_hash(APP),
            "balance_sources":source_hash(APP/"components/balance"),"oracle_sha256":hashlib.sha256((APP/"tools/spot_reference.py").read_bytes()).hexdigest()}
    (args.out/"result.json").write_text(json.dumps(result,indent=2)+"\n")
    print(json.dumps(result))


if __name__ == "__main__": main()
