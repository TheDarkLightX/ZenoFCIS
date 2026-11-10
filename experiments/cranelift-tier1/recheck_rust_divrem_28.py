#!/usr/bin/env python3
"""Independent original AOT/CLIF/disassembly hash and fresh-input rechecker."""
import argparse
import json
import random
import re
import subprocess
from pathlib import Path

from qualify_rust_divrem_28 import (
    SCHEMA, PROFILES, EXPORTS, UPSTREAM, TARGET, sha, signed, MASK, text_size
)

def independent_oracle(fn,x,y):
    if y == 0 or (fn == "divrem_swapped" and x == 0):
        return None
    a,b=x%(1<<64),y%(1<<64)
    if fn=="signed_divrem":
        if x==-(1<<63) and y==-1:return None
        q=(abs(x)//abs(y))*(-1 if ((x<0)!=(y<0)) else 1)
        rem=x-q*y
    else:
        q,rem=divmod(a,b)
        if fn=="divrem_swapped":rem=b%a
    if fn=="div_only":return signed(q)
    if fn=="divrem_offset":q=(q+3)%(1<<64)
    rem%=1<<64
    rot=((rem * (1<<17))%(1<<64)) | (rem>>47)
    return signed(q ^ rot)

def self_test():
    from qualify_rust_divrem_28 import oracle
    rng=random.Random(0x41234)
    for fn in EXPORTS:
        for _ in range(1000):
            x,y=signed(rng.getrandbits(64)),signed(rng.getrandbits(64))
            if independent_oracle(fn,x,y)!=oracle(fn,x,y):
                raise AssertionError("independent arithmetic oracles differ")
    assert independent_oracle("signed_divrem",-(1<<63),-1) is None
    assert independent_oracle("divrem_pair",7,3)==131074
    print("two independently formulated oracles agree for 6000 random cases")

def recheck(archive:Path,root:Path)->dict:
    r=json.loads((root/"qualification.json").read_text())
    if r.get("schema")!=SCHEMA or r.get("source_pin")!=UPSTREAM or r.get("native_target")!=TARGET:
        raise ValueError("source pin/target changed")
    if sha(archive/"qualification.json")!=r["archive_receipt_sha256"]:
        raise ValueError("archived compiler receipt changed")
    if set(r.get("profiles",{}))!=set(PROFILES):
        raise ValueError("profile set altered")
    rng=random.Random(0x156232026)
    pairs=[(signed(rng.getrandbits(64)),signed(rng.getrandbits(64))) for _ in range(36)]
    pairs += [(0,1),(1,0),(0,0),(-1,-1),(-(1<<63),-1)]
    checks=0; costs={}
    for profile,opt in PROFILES.items():
        row=r["profiles"][profile]
        wasm=root/profile/"idiom.wasm"
        if row["rust_opt"]!=opt or sha(wasm)!=row["wasm_sha256"] or wasm.stat().st_size!=row["wasm_bytes"]:
            raise ValueError("altered exact Rust-produced Wasm")
        for name in ("baseline","patched"):
            exe=archive/name/"wasmtime"
            target=root/profile/name
            aot=target/"native.cwasm"
            dis=target/"disassembly.txt"
            item=row["results"][name]
            if sha(exe)!=item["compiler_sha256"] or sha(aot)!=item["aot_sha256"] or sha(dis)!=item["disassembly_sha256"]:
                raise ValueError("original native source hash mismatch")
            if aot.stat().st_size!=item["whole_aot_bytes"] or text_size(aot)!=item["elf_text_bytes"]:
                raise ValueError("independently measured ELF/AOT length differs")
            if len(re.findall(r"\bdivq\b",dis.read_text()))!=item["module_static_divq"]:
                raise ValueError("static module-wide native opcode count differs")
            clif=target/"clif"
            if not item["clif"]:
                raise ValueError("missing independent CLIF capture")
            for source in item["clif"]:
                p=clif/source["path"]
                if sha(p)!=source["sha256"] or p.stat().st_size!=source["bytes"]:
                    raise ValueError("changed exact emitted CLIF")
            if not item["finite"]["value"] or not item["finite"]["trap_or_panic"]:
                raise ValueError("producer omitted finite/negative coverage")
            for fn in EXPORTS:
                for x,y in pairs:
                    p=subprocess.run([str(exe),"run","--allow-precompiled","--invoke",fn,
                        str(aot),str(x),str(y)],capture_output=True,text=True,timeout=45)
                    expected=independent_oracle(fn,x,y)
                    if expected is None:
                        if p.returncode==0:raise AssertionError("trap/panic refusal missing")
                    elif p.returncode or p.stdout.strip()!=str(expected):
                        raise AssertionError(f"incorrect fresh-input native value: {profile}/{name}/{fn}")
                    checks+=1
        b,p=row["results"]["baseline"],row["results"]["patched"]
        if row["baseline_minus_patched"]!={"elf_text_bytes":b["elf_text_bytes"]-p["elf_text_bytes"],
            "whole_aot_bytes":b["whole_aot_bytes"]-p["whole_aot_bytes"],
            "module_static_divq":b["module_static_divq"]-p["module_static_divq"]}:
            raise ValueError("source cost receipt mismatch")
        if row["aot_byte_identical"]!=(b["aot_sha256"]==p["aot_sha256"]):
            raise ValueError("incorrect AOT byte identity claim")
        costs[profile]=row["baseline_minus_patched"]
    return {"status":"independently-verified-finite-Wasm-and-native-cost",
            "fresh_executions":checks,"baseline_minus_patched":costs,
            "limitation":"not an exhaustive proof, code-reachability frequency, or runtime throughput result"}

if __name__=="__main__":
    ap=argparse.ArgumentParser()
    ap.add_argument("--archive",type=Path)
    ap.add_argument("--results",type=Path)
    ap.add_argument("--self-test",action="store_true")
    a=ap.parse_args()
    if a.self_test:self_test()
    else:
        if a.archive is None or a.results is None:ap.error("archive/results required")
        print(json.dumps(recheck(a.archive.resolve(),a.results.resolve()),sort_keys=True))
