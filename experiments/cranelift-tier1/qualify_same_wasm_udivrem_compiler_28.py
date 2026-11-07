#!/usr/bin/env python3
"""Same-WAT two-compiler native-cost and finite-semantics qualification.

All saved deltas are static machine measurements, NOT latency or throughput.
"""
import argparse
import hashlib
import json
import subprocess
from pathlib import Path
import qualify_wasmtime_frontend_28 as frontend
from qualify_udivrem_source_palette_28 import WATS, TARGET, finite, disasm_cost, sha
from independent_recheck_udivrem_source_palette_28 import independent_static_cost, second_oracle
from independent_recheck_scaling_28 import independent_elf_text_size

SCHEMA="zeno-fcis/tier1-udivrem-same-wasm-compiler-ab/1"
UPSTREAM="b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
MODES=("baseline","patched")
def measurements(out, source):
    if (out/"qualification.json").exists():
        raise ValueError("refusing to overwrite evidence")
    if source.read_bytes()!=WATS["direct"].encode():
        raise ValueError("same WAT source identity violated")
    rows={}
    for mode in MODES:
        folder=out/mode
        exe=(folder/"wasmtime").resolve()
        aot=(folder/"input.cwasm").resolve()
        command=[str(exe),"compile","--target",TARGET,"-o",str(aot),str(source.resolve())]
        result=frontend.run(command,timeout=300)
        (folder/"compile.stdout").write_text(result.stdout)
        (folder/"compile.stderr").write_text(result.stderr)
        disasm=frontend.run([str(exe),"objdump","--funcs","wasm","--bytes",str(aot)]).stdout
        (folder/"disassembly.txt").write_text(disasm)
        cost=disasm_cost(disasm)
        if cost!=independent_static_cost(disasm):
            raise ValueError("two static-cost decoders disagree")
        rows[mode]={
            "compiler_sha256":sha(exe.read_bytes()),"compiler_bytes":exe.stat().st_size,
            "aot_sha256":sha(aot.read_bytes()),"aot_bytes":aot.stat().st_size,
            "elf_text_bytes":independent_elf_text_size(aot),
            "disassembly_sha256":sha(disasm.encode()),"static_cost":cost,
            "finite":finite(exe,aot)}
    b,p=rows["baseline"],rows["patched"]
    def delta(row):
        return {"static_div":row["static_cost"]["static_div_instructions"],
                "through_ret_bytes":row["static_cost"]["through_ret_bytes"],
                "instructions":row["static_cost"]["decoded_instruction_lines"],
                "elf_text_bytes":row["elf_text_bytes"],"aot_bytes":row["aot_bytes"]}
    left,right=delta(b),delta(p)
    d={k:left[k]-right[k] for k in left}
    catalog=[]
    for mode,row in rows.items():
        identity={"target":TARGET,"wat_sha256":sha(source.read_bytes()),
                  "compiler_sha256":row["compiler_sha256"],"aot_sha256":row["aot_sha256"],
                  "contract":"u64 quotient xor rol64(u64 remainder,17); zero divisor traps"}
        catalog.append({"key":sha(json.dumps(identity,sort_keys=True).encode()),
                        "mode":mode,"cost":row["static_cost"],"finite":row["finite"]})
    obj={"schema":SCHEMA,"status":"producer-collected-await-independent-audit",
         "upstream":UPSTREAM,"target":TARGET,"source_sha256":sha(source.read_bytes()),
         "same_wat_source":True,"results":rows,"baseline_minus_patched":d,
         "catalog":sorted(catalog,key=lambda c:c["key"]),
         "semantics":{"domain":"all u64 bit patterns; nonzero unsigned divisor for values",
                      "zero_divisor":"Wasm must trap, tested separately",
                      "overflow":"mod 2^64 multiplication and subtraction",
                      "flags_poison":"not directly ISA/whole compiler proven"},
         "nonclaims":["finite tests are not compiler correctness proof",
                      "conditional Euclidean identity is not a proof of native lowering",
                      "static instructions or code bytes are not speed or throughput"]}
    (out/"qualification.json").write_text(json.dumps(obj,indent=2,sort_keys=True)+"\n")
    return d
def audit(out,source):
    o=json.loads((out/"qualification.json").read_text())
    if (o.get("schema")!=SCHEMA or o.get("upstream")!=UPSTREAM
        or o.get("target")!=TARGET or o.get("same_wat_source") is not True
        or source.read_bytes()!=WATS["direct"].encode()
        or sha(source.read_bytes())!=o.get("source_sha256")):
        raise ValueError("invalid source/pin/envelope")
    measured={}
    for mode in MODES:
        row=o["results"][mode]; folder=out/mode
        exe=(folder/"wasmtime").resolve(); aot=(folder/"input.cwasm").resolve()
        if (sha(exe.read_bytes())!=row["compiler_sha256"]
            or exe.stat().st_size!=row["compiler_bytes"]
            or sha(aot.read_bytes())!=row["aot_sha256"]
            or aot.stat().st_size!=row["aot_bytes"]):
            raise ValueError("archived compiler/object identity mismatch")
        disasm=frontend.run([str(exe),"objdump","--funcs","wasm","--bytes",str(aot)]).stdout
        if (disasm!=(folder/"disassembly.txt").read_text()
            or sha(disasm.encode())!=row["disassembly_sha256"]):
            raise ValueError("archived disassembly mismatch")
        cost=independent_static_cost(disasm)
        if cost!=row["static_cost"] or independent_elf_text_size(aot)!=row["elf_text_bytes"]:
            raise ValueError("independent native-cost audit failed")
        for x,y in [(0,1),(1,1),(7,3),(-1,-1),(-(1<<63),3),((1<<63)-1,-7)]:
            if frontend.invoke(exe,aot,"work",x,y)!=second_oracle(x,y):
                raise ValueError("independent arithmetic witness failed")
        trap=subprocess.run([str(exe),"run","--allow-precompiled","--invoke","work",
                             str(aot),"1","0"],text=True,capture_output=True,timeout=40)
        if trap.returncode==0 or "trap" not in trap.stderr.lower():
            raise ValueError("zero divisor did not trap")
        if row["finite"].get("passed") is not True or row["finite"].get("zero_divisor_trap_checked") is not True:
            raise ValueError("producer finite receipt incomplete")
        measured[mode]={
            "static_div":cost["static_div_instructions"],
            "through_ret_bytes":cost["through_ret_bytes"],
            "instructions":cost["decoded_instruction_lines"],
            "elf_text_bytes":row["elf_text_bytes"],"aot_bytes":row["aot_bytes"]}
    b,p=measured["baseline"],measured["patched"]
    delta={k:b[k]-p[k] for k in b}
    if delta!=o["baseline_minus_patched"]:
        raise ValueError("falsified cost deltas")
    for row in o["catalog"]:
        mode=row["mode"]; m=o["results"][mode]
        identity={"target":TARGET,"wat_sha256":sha(source.read_bytes()),
                  "compiler_sha256":m["compiler_sha256"],"aot_sha256":m["aot_sha256"],
                  "contract":"u64 quotient xor rol64(u64 remainder,17); zero divisor traps"}
        if row["key"]!=sha(json.dumps(identity,sort_keys=True).encode()):
            raise ValueError("catalog identity mismatch")
    (out/"independent_audit.json").write_text(json.dumps(
        {"status":"independent-audit-passed","delta":delta},indent=2,sort_keys=True)+"\n")
    return delta
def main():
    p=argparse.ArgumentParser()
    p.add_argument("--mode",choices=["collect","audit"],required=True)
    p.add_argument("--out",type=Path,required=True)
    p.add_argument("--source",type=Path,required=True)
    a=p.parse_args()
    print(json.dumps(measurements(a.out,a.source) if a.mode=="collect" else audit(a.out,a.source),sort_keys=True))
if __name__=="__main__":
    main()
