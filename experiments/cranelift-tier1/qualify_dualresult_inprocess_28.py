#!/usr/bin/env python3
"""Fail-closed audit of paired, same-WAT, baseline-vs-patched compiler call timings.
Exploratory virtualized-host in-process time, not isolated division latency or speed claim.
Only consumes exact-head, successful dual-result compiler artifacts from CI 42596373104.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import math
import statistics
from pathlib import Path

UPSTREAM = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
PRODUCER_HEAD = "5e2d277f956af5fa7e5037d5316032370f59d513"
PRODUCER_RUN = 42596373104
PRODUCER_ARTIFACT = 13664510417
SCHEMA = "zeno-fcis/udivrem-same-wasm-compiler-call-timing/1"
PRODUCER_SCHEMA = "zeno-fcis/tier1-udivrem-same-wasm-compiler-ab/1"
MASK = (1 << 64) - 1

def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()

def signed(x: int) -> int:
    x &= MASK
    return x if x < (1 << 63) else x - (1 << 64)

def inputs() -> list[tuple[int,int]]:
    pairs = [(0,1),(7,3),(-(1<<63),-1),((1<<63)-1,1),(-1,-1),
             ((1<<63)-1,-(1<<63)),(-(1<<63),(1<<63)-1),(123456789,-7),
             (0,-1),(-1,1),(-(1<<63),3),((1<<63)-1,7)]
    state = 0x9e3779b97f4a7c15
    for _ in range(256):
        state ^= (state << 13) & MASK
        state ^= state >> 7
        state ^= (state << 17) & MASK
        state &= MASK
        x = signed(state)
        state ^= (state << 13) & MASK
        state ^= state >> 7
        state ^= (state << 17) & MASK
        state &= MASK
        pairs.append((x,signed(state if state else 1)))
    return pairs

def checksum(calls: int) -> int:
    data = inputs()
    result = 0
    for k in range(calls):
        x,y=data[k % len(data)]
        a,b=x&MASK,y&MASK
        q,r=divmod(a,b)
        result=(result+(q ^ (((r << 17) | (r >> 47)) & MASK)))&MASK
    return result

def validate_producer(root: Path) -> dict:
    receipt = json.loads((root / "qualification.json").read_text())
    if (receipt.get("schema")!=PRODUCER_SCHEMA or receipt.get("upstream")!=UPSTREAM
        or receipt.get("same_wat_source") is not True
        or set(receipt.get("results",{}))!={"baseline","patched"}
        or (root/"zenofcis-head.txt").read_text().strip()!=PRODUCER_HEAD
        or (root/"wasmtime-head.txt").read_text().strip()!=UPSTREAM
        or sha(root/"input.wat")!=receipt.get("source_sha256")):
        raise ValueError("wrong pinned producer or not identical input WAT")
    independent = json.loads((root / "independent_audit.json").read_text())
    if independent.get("status") != "independent-audit-passed":
        raise ValueError("independent native cost audit not passed")
    contexts = json.loads((root / "context-controls" / "qualification.json").read_text())
    if (contexts.get("status") != "finite-context-controls-passed"
        or contexts.get("upstream_source_pin") != UPSTREAM
        or contexts.get("tests_per_mode_per_case") != 38
        or len(contexts.get("cases", {})) != 11
        or contexts["cases"]["adjacent"]["static_div_savings"] != 1
        or any(contexts["cases"][name]["static_div_savings"] != 0 for name in
               ("reverse_order", "swapped_rem_inputs", "quotient_only",
                "remainder_only", "signed_64", "unsigned_32", "signed_32"))):
        raise ValueError("missing or contradictory eleven-context qualification")
    files={}
    for mode in ("baseline","patched"):
        record=receipt["results"][mode]
        for name,key in (("wasmtime","compiler_sha256"),("input.cwasm","aot_sha256")):
            file=root/mode/name
            if sha(file)!=record[key]:
                raise ValueError("compiler or AOT hash mismatch: "+str(file))
        files[mode]={"compiler_sha256":record["compiler_sha256"],
                     "aot_sha256":record["aot_sha256"],
                     "aot_bytes":record["aot_bytes"],
                     "static_cost":record["static_cost"]}
    cost=receipt.get("baseline_minus_patched",{})
    if (cost.get("static_div")!=1 or cost.get("through_ret_bytes")!=18
        or cost.get("instructions")!=6 or cost.get("aot_bytes")!=0
        or cost.get("elf_text_bytes")!=0):
        raise ValueError("source cost scope changed")
    return {"receipt_sha256":sha(root/"qualification.json"),
            "independent_audit_sha256":sha(root/"independent_audit.json"),
            "context_controls_sha256":sha(root/"context-controls"/"qualification.json"),
            "input_wat_sha256":sha(root/"input.wat"),
            "cost_baseline_minus_patched":cost,"artifacts":files}

def audit(root: Path, raw: Path, lock: Path, calls: int, blocks: int) -> dict:
    if not (10_000 <= calls <= 2_000_000 and 4 <= blocks <= 64 and blocks % 2 == 0):
        raise ValueError("invalid bounded timing budget")
    producer=validate_producer(root)
    if b'name = "wasmtime"' not in lock.read_bytes():
        raise ValueError("unrecognised Wasmtime runtime lock")
    lines=[x.strip() for x in raw.read_text().splitlines() if x.strip()]
    if len(lines)!=3+blocks*4 or lines[0]!=f"META,{SCHEMA},{calls},{blocks},5623":
        raise ValueError("wrong schema or incomplete paired transcript")
    if lines[1:3]!=[f"CHECK,baseline,{len(inputs())}",f"CHECK,patched,{len(inputs())}"]:
        raise ValueError("finite checks missing")
    want=checksum(calls)
    ratios=[]
    all_samples=[]
    for i in range(blocks):
        order=(["baseline","patched","patched","baseline"] if i%2==0
               else ["patched","baseline","baseline","patched"])
        by_variant={"baseline":[],"patched":[]}
        for pos,variant in enumerate(order):
            parts=lines[3+4*i+pos].split(",")
            if (len(parts)!=6 or parts[:4]!=["SAMPLE",str(i),str(pos),variant]
                or not parts[4].isdigit() or not parts[5].isdigit()):
                raise ValueError("sample order, duration, or checksum format mismatch")
            ns,chk=int(parts[4]),int(parts[5])
            if not 0<ns<2**63 or chk!=want:
                raise ValueError("invalid duration or guest checksum")
            by_variant[variant].append(ns)
            all_samples.append({"block":i,"position":pos,"variant":variant,
                                "elapsed_ns":ns,"checksum":chk})
        ratios.append(statistics.median(by_variant["patched"])/statistics.median(by_variant["baseline"]))
    source=Path(__file__).resolve().parent
    scripts=[source/"inprocess_same_wasm_28"/"src"/"main.rs",
             source/"inprocess_same_wasm_28"/"Cargo.toml",
             source/"qualify_dualresult_inprocess_28.py"]
    return {
      "schema":"zeno-fcis/tier1-dualresult-same-wasm-balanced-call-timing-audit/1",
      "status":"exploratory-dualresult-same-wasm-paired-timing-not-qualified-speedup",
      "upstream":UPSTREAM,"producer_head":PRODUCER_HEAD,
      "producer_run":PRODUCER_RUN,"producer_artifact":PRODUCER_ARTIFACT,
      "producer":producer,"cargo_lock_sha256":sha(lock),
      "source_sha256":{str(x.relative_to(source)):sha(x) for x in scripts},
      "raw_sha256":sha(raw),"calls_per_sample":calls,"balanced_blocks":blocks,
      "finite_pairs_per_variant":len(inputs()),"timed_checksum":want,
      "timing":{"patched_over_baseline_median_paired_ratio":statistics.median(ratios),
                "per_block_ratios":ratios,"min":min(ratios),"max":max(ratios),
                "sample_count":len(all_samples)},
      "samples":all_samples,
      "nonclaims":["virtualized one-host result, no reliable speed advantage",
                   "call, store, loop and dispatch overhead included",
                   "no isolated x86 div latency or throughput measurement",
                   "no universal compiler-pass or machine-code proof",
                   "native size delta is only static instruction prefix; whole AOT unchanged"]
    }

def self_test() -> None:
    assert len(inputs())==268
    assert checksum(1)==0
    assert checksum(2)==(2 ^ (1<<17))  # first (0,1), second (7,3) q=2, r=1
    assert signed(MASK)==-1
    print("same-Wasm timing reference self-tests passed")

if __name__=="__main__":
    ap=argparse.ArgumentParser()
    ap.add_argument("--self-test",action="store_true")
    ap.add_argument("--source-root",type=Path)
    ap.add_argument("--raw",type=Path)
    ap.add_argument("--cargo-lock",type=Path)
    ap.add_argument("--calls",type=int,default=100_000)
    ap.add_argument("--blocks",type=int,default=16)
    ap.add_argument("--out",type=Path)
    args=ap.parse_args()
    if args.self_test:
        self_test()
    else:
        if not all((args.source_root,args.raw,args.cargo_lock,args.out)):
            ap.error("source-root, raw, cargo-lock, out required")
        if args.out.exists():
            ap.error("refusing to overwrite existing evidence")
        result=audit(args.source_root.resolve(),args.raw.resolve(),
                     args.cargo_lock.resolve(),args.calls,args.blocks)
        args.out.write_text(json.dumps(result,sort_keys=True,indent=2)+"\n")
        print(json.dumps({"status":result["status"],"paired_ratio":result["timing"]["patched_over_baseline_median_paired_ratio"]},sort_keys=True))
