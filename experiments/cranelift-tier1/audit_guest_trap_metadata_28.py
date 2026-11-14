#!/usr/bin/env python3
"""Audit finite Wasmtime *guest* trap classification and original-Wasm offsets.

The pinned baseline/patched compiler experiments use identical WAT, and the
Rust probe queries Wasmtime's Trap and WasmBacktrace API. Fail closed on a
missing recorded optimization, empty maps, wrong trap codes or changed source
offsets. Does not establish a general compiler-pass proof or native speedup.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import re
from pathlib import Path

BASE = Path(__file__).resolve().parent
WASM = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
V = re.compile(r"VALUE x=(-?\d+) y=(-?\d+) result=(-?\d+)")
T = re.compile(r"TRAP x=(-?\d+) y=0 code=IntegerDivisionByZero func_index=0 func_offset=(\d+) module_offset=(\d+)")
EXPECTED_VALID = ((0,1),(1,1),(7,3),((1<<63)-1,1),(-(1<<63),1),
                  (-1,-1),(-(1<<63),-1),((1<<63)-1,9),(-7,3),(1,-1))
EXPECTED_TRAPS = ((0,0),(1,0),(-1,0),(-(1<<63),0),((1<<63)-1,0))

def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()

def decode(text: str) -> dict:
    values, traps = {}, {}
    for line in text.strip().splitlines():
        if match := V.fullmatch(line):
            x,y,result = map(int,match.groups())
            if (x,y) in values: raise ValueError("duplicate finite value row")
            if not y: raise ValueError("zero-divisor value unexpectedly returned")
            ux,uy=x & ((1<<64)-1),y & ((1<<64)-1)
            rem=ux%uy
            expected=(ux//uy)^(((rem<<17)|(rem>>47))&((1<<64)-1))
            if expected & (1<<63): expected -= 1<<64
            if result != expected: raise ValueError("wrong finite arithmetic result")
            values[(x,y)]=result
        elif match := T.fullmatch(line):
            x,fo,mo=map(int,match.groups())
            if (x,0) in traps: raise ValueError("duplicate guest trap row")
            if mo <= 0 or mo < fo: raise ValueError("invalid guest source offset")
            traps[(x,0)]={"func_index":0,"func_offset":fo,"module_offset":mo,
                           "trap":"IntegerDivisionByZero"}
        else: raise ValueError("malformed or unqualified Wasmtime API row: "+line[:120])
    if set(values) != set(EXPECTED_VALID) or set(traps) != set(EXPECTED_TRAPS):
        raise ValueError("missing or extra exact test vector")
    if len({(v["func_offset"],v["module_offset"]) for v in traps.values()})!=1:
        raise ValueError("zero-divisor traps map to different guest instructions")
    return {"values":values,"traps":traps}

def audit(out: Path) -> dict:
    if (out/"guest-trap-metadata.json").exists():
        raise ValueError("refusing to overwrite receipt")
    source=out/"input.wat"
    if not source.exists(): raise ValueError("missing identical WAT source")
    head=(out/"wasmtime-head.txt").read_text().strip()
    if head != WASM: raise ValueError("upstream source moved")
    source_pin=json.loads((out/"qualification.json").read_text())
    if (source_pin.get("upstream")!=WASM or not source_pin.get("same_wat_source")
        or source_pin.get("source_sha256") != sha(source)):
        raise ValueError("WAT qualification receipt does not match")
    results=source_pin["results"]
    if (source_pin["baseline_minus_patched"].get("static_div") != 1
        or results["baseline"]["static_cost"]["static_div_instructions"] != 2
        or results["patched"]["static_cost"]["static_div_instructions"] != 1):
        raise ValueError("no reproducibly measured positive compiler rewrite")
    rows={}
    for mode in ("baseline","patched"):
        exe=out/mode/"wasmtime"
        if sha(exe)!=results[mode]["compiler_sha256"]:
            raise ValueError("compiler identity changed since A/B test")
        transcript=out/(mode+"-guest-trap-api.txt")
        decoded=decode(transcript.read_text())
        rows[mode]={"values":{f"{x},{y}":v for (x,y),v in decoded["values"].items()},
                    "traps":{f"{x},{y}":v for (x,y),v in decoded["traps"].items()},
                    "transcript_sha256":sha(transcript),
                    "compiler_sha256":sha(exe)}
    for key in ("values","traps"):
        if rows["baseline"][key]!=rows["patched"][key]:
            raise ValueError("guest-visible A/B mismatch: "+key)
    evidence={
        "schema":"zenofcis/tier1-wasmtime-guest-trap-metadata/1",
        "status":"finite-guest-code-and-wasm-source-offset-comparison-passed",
        "upstream":WASM,
        "zenofcis_source_commit":(out/"zenofcis-head.txt").read_text().strip(),
        "source_wat_sha256":sha(source),
        "probe_sha256":sha(BASE/"guest_trap_metadata_28.rs"),
        "auditor_sha256":sha(Path(__file__)),
        "target":"x86-64 Linux, Wasmtime API, opt_level=speed_and_size, generate_address_map=true",
        "qualification":"same WAT; 2 baseline vs 1 patched native div, separately checked in CLI A/B",
        "records":rows,
        "counts":{"finite_values_per_mode":len(EXPECTED_VALID),
                  "traps_per_mode":len(EXPECTED_TRAPS)},
        "limitations":[
            "No direct proof of internal trap-table entries or native fault-PC equivalence",
            "Matching original Wasm offsets does not prove identical machine PC or trap-table entries",
            "Finite Wasmtime API execution does not prove compiler-pass SSA, liveness or flags",
            "Only one direct WAT shape and the x86-64 target; no representative runtime profitability",
            "This prototype is human authored; not a new ZenoFCIS-generated sequence",
        ],
    }
    (out/"guest-trap-metadata.json").write_text(json.dumps(evidence,indent=2,sort_keys=True)+"\n")
    return evidence

def self_test():
    good="\n".join([f"VALUE x={x} y={y} result={signed_result(x,y)}" for x,y in EXPECTED_VALID]+[
        f"TRAP x={x} y=0 code=IntegerDivisionByZero func_index=0 func_offset=6 module_offset=42"
        for x,_ in EXPECTED_TRAPS])+"\n"
    decode(good)
    for bad in (good.replace("IntegerDivisionByZero","IntegerOverflow"),
                good.replace("func_offset=6","func_offset=7",1),
                good.replace("module_offset=42","module_offset=0",1),
                good.replace("VALUE x=0 y=1","VALUE x=0 y=0",1),
                good+"INVALID\n"):
        try: decode(bad)
        except ValueError: pass
        else: raise AssertionError("mutant accepted")
    print("guest metadata parser passes positive and five corrupt-data controls")

def signed_result(x,y):
    mask=(1<<64)-1
    a,b=x&mask,y&mask
    r=a%b
    v=(a//b)^(((r<<17)|(r>>47))&mask)
    return v-(1<<64) if v&(1<<63) else v

if __name__=="__main__":
    p=argparse.ArgumentParser()
    p.add_argument("--out",type=Path)
    p.add_argument("--self-test",action="store_true")
    a=p.parse_args()
    if a.self_test: self_test()
    elif a.out: print(json.dumps({"status":audit(a.out)["status"]}))
    else: p.error("choose --self-test or --out")
