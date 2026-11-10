#!/usr/bin/env python3
"""Fail-closed independent audit of same-WAT baseline/patched call-live timing."""
import argparse, hashlib, json, statistics
from pathlib import Path
UP="b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
HEAD="7967adce41243c2c024c025e674f0b14461a6da2"
SCRIPT_SHA="60076e6e7068e2a22bffc83a53839b0b4f365e04926b723ff3f668fc8660d20e"
SCHEMA="zenofcis/udivrem-live-inprocess/1"
CASE={"live_0":(0,"forward"),"live_4":(4,"forward"),"live_12":(12,"forward"),
      "live_20":(20,"forward"),"live_12_reverse":(12,"reverse"),
      "live_12_quotient_only":(12,"quotient")}
PAIRS=[(0,1),(7,3),(-(1<<63),-1),((1<<63)-1,1),(-1,-1),
       ((1<<63)-1,-(1<<63)),(-(1<<63),(1<<63)-1),(123456789,-7),
       (0,-1),(-1,1),(-(1<<63),3),((1<<63)-1,7),
       (7,2),(-(1<<63),2),(123456789,98765),(-987654321,31337)]
MASK=(1<<64)-1
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def oracle(x,y,n,mode):
    a,b=x&MASK,y&MASK
    q,r=divmod(a,b)
    v=q if mode=="quotient" else q^(((r<<17)|(r>>47))&MASK)
    for i in range(n):v^=(a^(3+2*i))^17
    return v&MASK
def check(calls,n,mode):
    return sum(oracle(*PAIRS[i%16],n,mode) for i in range(calls))&MASK
def audit(src,raw,lock,calls,blocks):
    if not 10_000<=calls<=1_000_000 or not 4<=blocks<=32 or blocks%2:
        raise ValueError("bad timing budget")
    root=src/"results";r=json.loads((root/"qualification.json").read_text())
    if ((src/"zenofcis-head.txt").read_text().strip()!=HEAD or
        r.get("upstream")!=UP or r.get("target")!="x86_64-unknown-linux" or
        r.get("source_script_sha256")!=SCRIPT_SHA or set(r.get("cases",{}))!=set(CASE) or
        r.get("total_finite_values")!=1044 or r.get("total_zero_traps")!=108 or
        b'name = "wasmtime"' not in lock.read_bytes()):
        raise ValueError("wrong producer, source, checks or runtime dependency")
    out={"schema":"zenofcis/udivrem-live-inprocess-audit/1",
         "status":"exploratory-paired-one-host-timing-NOT-verified-speedup",
         "upstream":UP,"producer_run":42708736506,"producer_artifact":13715505924,
         "producer_head":HEAD,"producer_receipt_sha256":sha(root/"qualification.json"),
         "cargo_lock_sha256":sha(lock),"calls":calls,"blocks":blocks,"cases":{},
         "limitations":["virtualized CPU and Wasmtime guest dispatch overhead",
                        "synthetic live-call fixture, not representative app workloads",
                        "finite checks, not soundness of arbitrary Cranelift pass",
                        "native function bytes not CLIF cost or whole-AOT savings"]}
    for name,(n,mode) in CASE.items():
        rec=r["cases"][name]
        if rec["call_live_values"]!=n or rec["mode"]!=mode or \
            sha(root/name/"input.wat")!=rec["wat_sha256"]:
            raise ValueError("wrong WAT or context")
        for v in ("baseline","patched"):
            x=rec["compilers"][v]
            if sha(root/name/(v+".cwasm"))!=x["aot_sha256"] or \
                (x["finite_values"],x["zero_traps"])!=(87,9):
                raise ValueError("invalid compiler AOT or finite checks")
        b=rec["compilers"]["baseline"]["native_cost"]["static_divq"]
        p=rec["compilers"]["patched"]["native_cost"]["static_divq"]
        if (b,p)!=((2,1) if mode=="forward" else (b,b)):
            raise ValueError("unexpected fusion or negative control")
        lines=(raw/(name+".csv")).read_text().strip().splitlines()
        if len(lines)!=3+4*blocks or lines[:3]!=[
          f"META,{SCHEMA},{name},{n},{mode},{calls},{blocks},5623",
          "CHECK,baseline,16","CHECK,patched,16"]:
            raise ValueError("wrong timing transcript")
        wanted=check(calls,n,mode);ratios=[]
        for i in range(blocks):
            order=(("baseline","patched","patched","baseline") if i%2==0 else
                   ("patched","baseline","baseline","patched"))
            data={"baseline":[],"patched":[]}
            for pos,v in enumerate(order):
                parts=lines[3+4*i+pos].split(",")
                if len(parts)!=6 or parts[:4]!=["SAMPLE",str(i),str(pos),v] or \
                    not parts[4].isdigit() or not parts[5].isdigit() or \
                    not 0<int(parts[4])<2**63 or int(parts[5])!=wanted:
                    raise ValueError("sample order, timing or checksum invalid")
                data[v].append(int(parts[4]))
            ratios.append(statistics.median(data["patched"])/statistics.median(data["baseline"]))
        out["cases"][name]={"live":n,"mode":mode,"source_wat_sha256":rec["wat_sha256"],
          "raw_sha256":sha(raw/(name+".csv")),"paired_ratios":ratios,
          "median_patched_over_baseline":statistics.median(ratios),
          "ratio_range":[min(ratios),max(ratios)],
          "static_native_cost":{v:rec["compilers"][v]["native_cost"] for v in ("baseline","patched")},
          "complete_aot_bytes":{v:rec["compilers"][v]["whole_aot_bytes"] for v in ("baseline","patched")}}
    return out
def self_test():
    assert len(PAIRS)==16 and check(1,0,"forward")==0
    assert oracle(7,3,0,"forward")==2^(1<<17)
    assert oracle(7,3,4,"forward")!=oracle(7,3,0,"forward")
    print("call-live independent arithmetic self-test passed")
if __name__=="__main__":
    ap=argparse.ArgumentParser()
    for key in ("source","raw","lock","out"):ap.add_argument("--"+key,type=Path)
    ap.add_argument("--calls",type=int,default=100000)
    ap.add_argument("--blocks",type=int,default=8)
    ap.add_argument("--self-test",action="store_true")
    a=ap.parse_args()
    if a.self_test:self_test()
    else:
        if not all((a.source,a.raw,a.lock,a.out)) or a.out.exists():
            ap.error("missing inputs or output already exists")
        result=audit(a.source,a.raw,a.lock,a.calls,a.blocks)
        a.out.write_text(json.dumps(result,indent=2,sort_keys=True)+"\n")
        print(json.dumps({k:v["median_patched_over_baseline"] for k,v in result["cases"].items()}))
