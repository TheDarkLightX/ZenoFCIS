#!/usr/bin/env python3
"""Finite x64 Wasmtime dual-result division screen with call-carried live values.

Independent same-WAT comparison of archived baseline/patched compiler binaries.
The callee calls precede DIV and their results are consumed afterward, so the
pressure cannot disappear by simply sinking pure x/y arithmetic past DIV.
Not an exhaustive proof or an inference of register-allocation profitability.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import random
import re
import subprocess
from pathlib import Path

UPSTREAM = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
TARGET = "x86_64-unknown-linux"
MASK = (1 << 64)-1
SCHEMA = "zenofcis/tier1-udivrem-call-live-pressure/1"
CASES = {"live_0":(0,"forward"),"live_4":(4,"forward"),
         "live_12":(12,"forward"),"live_20":(20,"forward"),
         "live_12_reverse":(12,"reverse"),
         "live_12_quotient_only":(12,"quotient")}

def sha(path:Path)->str:
    return hashlib.sha256(path.read_bytes()).hexdigest()

def signed(x:int)->int:
    x &= MASK
    return x if x < (1<<63) else x-(1<<64)

def source(n:int,mode:str)->str:
    if n not in (0,4,12,20) or mode not in ("forward","reverse","quotient"):
        raise ValueError("out-of-scope experiment")
    a=["(module","  (func $mix (param $a i64) (result i64)",
       "    local.get $a","    i64.const 17","    i64.xor","  )",
       '  (func (export "work") (param $x i64) (param $y i64) (result i64)']
    a += [f"    (local $v{i} i64)" for i in range(n)]
    for i in range(n):
        a += ["    local.get $x",f"    i64.const {3+2*i}","    i64.xor",
              "    call $mix",f"    local.set $v{i}"]
    div=["    local.get $x","    local.get $y","    i64.div_u"]
    rem=["    local.get $x","    local.get $y","    i64.rem_u",
         "    i64.const 17","    i64.rotl"]
    a += div + rem + ["    i64.xor"] if mode=="forward" else (
         rem + div + ["    i64.xor"] if mode=="reverse" else div)
    for i in range(n):a += [f"    local.get $v{i}","    i64.xor"]
    return "\n".join(a+["  )",")",""])

def oracle(n:int,mode:str,x:int,y:int)->int|None:
    a,b=x&MASK,y&MASK
    if b==0:return None
    q,r=divmod(a,b)
    value = q if mode=="quotient" else q ^ (((r<<17)|(r>>47))&MASK)
    for i in range(n):value ^= ((a^(3+2*i))^17)&MASK
    return signed(value)

def vectors():
    edges=[0,1,2,7,-1,-3,-(1<<63),(1<<63)-1,1<<32]
    divisors=[0,1,2,3,-1,-7,-(1<<63),(1<<63)-1]
    pairs=[(x,y) for x in edges for y in divisors]
    r=random.Random(0x56232026)
    pairs += [(signed(r.getrandbits(64)),signed(r.getrandbits(64))) for _ in range(24)]
    return list(dict.fromkeys(pairs))

def exec_cmd(*args, timeout:int=180):
    return subprocess.run([str(x) for x in args],capture_output=True,text=True,timeout=timeout)

def native(disassembly:str)->dict:
    m=re.search(r"(?m)^wasm\[0\]::function\[1\]:\s*\n",disassembly)
    if not m:raise ValueError("missing exported work symbol")
    work=disassembly[m.end():].split("wasm[")[0]
    lines=work.splitlines()
    ins=[r for r in lines if re.match(r"^ {12}[0-9a-f]{2}(?:\s|$)",r)]
    if not ins:raise ValueError("no decoded work instructions")
    div=[i for i,r in enumerate(ins) if re.search(r"\bdivq\b",r)]
    if not div:raise ValueError("no division lowering")
    calls=[i for i,r in enumerate(ins) if re.search(r"\bcallq\b",r)]
    length=0
    for row in ins:
        prefix=0
        for b in row.strip().split():
            if re.fullmatch(r"[0-9a-f]{2}",b):prefix+=1
            else:break
        if not prefix:raise ValueError("unparsed instruction bytes")
        length+=prefix
    return {"static_divq":len(div),"work_instructions":len(ins),
            "work_function_bytes":length,"calls_before_first_div":sum(i<div[0] for i in calls),
            "calls_after_last_div":sum(i>div[-1] for i in calls),
            "pre_div_rsp_value_stores":sum(bool(re.search(r"movq\s+%rax,\s*(?:0x[0-9a-f]+|\d+)\(%rsp\)",r)) for r in ins[:div[0]]),
            "stack_traffic_lines":sum("(%rsp)" in r for r in ins)}

def check_archive(archive:Path)->dict:
    r=json.loads((archive/"qualification.json").read_text())
    if r.get("upstream")!=UPSTREAM or r.get("target")!=TARGET:
        raise ValueError("producer pin mismatch")
    for kind in ("baseline","patched"):
        exe=archive/kind/"wasmtime"
        if not exe.is_file() or sha(exe)!=r["results"][kind]["compiler_sha256"]:
            raise ValueError("unverified compiler bytes: "+kind)
    return r

def qualify(archive:Path,out:Path):
    if out.exists():raise FileExistsError("never overwrite an evidence directory")
    producer=check_archive(archive)
    out.mkdir(parents=True)
    receipt={"schema":SCHEMA,"status":"finite-call-live-pressure-screen",
             "target":TARGET,"upstream":UPSTREAM,"producer_run":42596373104,
             "source_script_sha256":sha(Path(__file__).resolve()),
             "producer_compiler_sha256":{k:producer["results"][k]["compiler_sha256"] for k in ("baseline","patched")},
             "cases":{},"nonclaims":["finite executions, not exhaustive u64 or SMT proof",
                 "calls and live-range evidence are synthetic, not realistic benchmark throughput",
                 "bytes are native x86 work-function bytes, not CLIF units or whole-AOT savings",
                 "no proof of full compiler-pass correctness, flags, register allocator, or cross-target semantics"]}
    for name,(n,mode) in CASES.items():
        dir=out/name;dir.mkdir();wat=dir/"input.wat";wat.write_text(source(n,mode))
        row={"wat_sha256":sha(wat),"call_live_values":n,"mode":mode,
             "contract":"u64 div/rem; Wasm trap on zero divisor; i64 xor/rotl mod 2^64; pure i64 callee",
             "compilers":{}}
        for kind in ("baseline","patched"):
            exe=archive/kind/"wasmtime"
            aot=dir/(kind+".cwasm")
            c=exec_cmd(exe,"compile","--target",TARGET,"-o",aot,wat)
            if c.returncode:raise RuntimeError(f"{name}/{kind} compile: {c.stderr}")
            d=exec_cmd(exe,"objdump","--funcs","wasm","--bytes",aot)
            if d.returncode:raise RuntimeError(f"{name}/{kind} disasm: {d.stderr}")
            dst=dir/(kind+".disasm");dst.write_text(d.stdout)
            cost=native(d.stdout)
            if cost["calls_before_first_div"]!=n or cost["calls_after_last_div"]!=0:
                raise AssertionError(f"{name}/{kind}: callee calls not all before DIV")
            if n>=12 and cost["pre_div_rsp_value_stores"]<1:
                raise AssertionError(f"{name}/{kind}: no observed stack-stored call results")
            correct,traps=0,0
            for x,y in vectors():
                v=exec_cmd(exe,"run","--allow-precompiled","--invoke","work",aot,x,y,timeout=60)
                expected=oracle(n,mode,x,y)
                if expected is None:
                    if v.returncode==0 or "trap" not in v.stderr.lower():
                        raise AssertionError(f"{name}/{kind}: missing zero trap ({x},{y})")
                    traps+=1
                elif v.returncode or v.stdout.strip()!=str(expected):
                    raise AssertionError(f"{name}/{kind}: bad value ({x},{y}): {v.stdout} {v.stderr}")
                else:correct+=1
            row["compilers"][kind]={"compiler_sha256":sha(exe),"aot_sha256":sha(aot),
                 "disassembly_sha256":sha(dst),"whole_aot_bytes":aot.stat().st_size,
                 "native_cost":cost,"finite_values":correct,"zero_traps":traps}
        b=row["compilers"]["baseline"];p=row["compilers"]["patched"]
        q=b["native_cost"];t=p["native_cost"]
        if mode=="forward" and (q["static_divq"],t["static_divq"])!=(2,1):
            raise AssertionError(f"{name}: forward fusion not observed")
        if mode!="forward" and q["static_divq"]!=t["static_divq"]:
            raise AssertionError(f"{name}: out-of-scope fusion")
        row["baseline_minus_patched"]={
             "native_work_bytes":q["work_function_bytes"]-t["work_function_bytes"],
             "native_instructions":q["work_instructions"]-t["work_instructions"],
             "static_divq":q["static_divq"]-t["static_divq"],
             "whole_aot_bytes":b["whole_aot_bytes"]-p["whole_aot_bytes"]}
        receipt["cases"][name]=row
    receipt["total_finite_values"]=sum(x["finite_values"] for r in receipt["cases"].values() for x in r["compilers"].values())
    receipt["total_zero_traps"]=sum(x["zero_traps"] for r in receipt["cases"].values() for x in r["compilers"].values())
    (out/"qualification.json").write_text(json.dumps(receipt,indent=2,sort_keys=True)+"\n")
    return receipt

def self_test():
    for name,(n,mode) in CASES.items():
        s=source(n,mode)
        assert s.count("call $mix")==n
        assert s.count("i64.rem_u")==int(mode!="quotient")
        assert s.index("call $mix")<s.index("i64.div_u") if n else True
        assert oracle(n,mode,7,3) is not None
        assert oracle(n,mode,7,0) is None
    assert len(vectors())>=90
    print("live-call WAT/source, unsigned oracle, and trap tests pass")

if __name__=="__main__":
    p=argparse.ArgumentParser();p.add_argument("--self-test",action="store_true")
    p.add_argument("--archive",type=Path);p.add_argument("--out",type=Path)
    a=p.parse_args()
    if a.self_test:self_test()
    else:
        if not a.archive or not a.out:p.error("--archive and --out required")
        r=qualify(a.archive,a.out)
        print(json.dumps({"status":r["status"],"total_finite_values":r["total_finite_values"],
                          "total_zero_traps":r["total_zero_traps"],
                          "savings":{k:v["baseline_minus_patched"] for k,v in r["cases"].items()}},sort_keys=True))
