#!/usr/bin/env python3
"""Qualify source-emitted scalar arithmetic against an independent integer oracle.

This is a heavy command: it runs Cargo, compiles emitted Rust, and executes all
three adapters. It must be scheduled in an authorized, owned compute lane.
This bridge check is separate from Verus and complete application enumeration.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
from scalar_arithmetic_reference import Case, boundary_cases, result
from scalar_arithmetic_controls import replace_once

ROOT=Path(__file__).resolve().parents[1]
SOURCES=(".node-version","tools/check_scalar_arithmetic.py","tools/test_check_scalar_arithmetic.py",
    "tools/scalar_arithmetic_reference.py","tools/scalar_arithmetic_controls.py",
    "tools/test-data/scalar-arithmetic/vectors.tsv",
    "crates/zeno-fcis-synthesis/tests/scalar_arithmetic.rs",
    "crates/zeno-fcis-synthesis/src/finite/emit.rs",
    "crates/zeno-fcis-synthesis/src/finite/value/scalar.rs",
    "crates/zeno-fcis-synthesis/src/finite/value/wide.rs",
    "crates/zeno-fcis-synthesis/src/finite/evaluation/mod.rs",
    "crates/zeno-fcis-synthesis/src/finite/evaluation/typed.rs",
    "crates/zeno-fcis-synthesis/src/finite/ir.rs",
    "crates/zeno-fcis-synthesis/src/finite_runtime.rs")


def snapshot()->dict:
    return {p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in SOURCES}


def compare(actual:list,wanted:list,enabled:bool=True)->bool:
    return actual==wanted if enabled else True


def grade(positive:list,wanted:list,mutants:dict,enabled:bool=True)->dict:
    if not compare(positive,wanted,enabled):
        raise ValueError("positive emitted evaluator differs from the independent oracle")
    killed={name:not compare(observed,expected,enabled) for name,(observed,expected) in mutants.items()}
    if not killed or not all(killed.values()):
        raise ValueError("semantic emitted-code mutant survived the comparison")
    return killed


def run(command:list[str],directory:Path,label:str,input_text:str|None=None,env:dict|None=None)->str:
    process=subprocess.run(command,cwd=ROOT,input=input_text,text=True,capture_output=True,
        timeout=600,env=env)
    (directory/(label+".stdout")).write_text(process.stdout)
    (directory/(label+".stderr")).write_text(process.stderr)
    (directory/(label+".process.json")).write_text(json.dumps({"command":command,"exit":process.returncode},indent=2)+"\n")
    if process.returncode:
        raise RuntimeError("tool failure, not a semantic arithmetic control: "+label)
    return process.stdout


# The emitted subjects are on the finite-i128/3 main route: signed i128
# operands and results, so the oracle, the corpus and the Rust driver use 128 bits.
BITS=128
RUST_DRIVER='\nfn main() {\n    use std::io::Read;\n    let mut input=String::new();std::io::stdin().read_to_string(&mut input).unwrap();\n    for line in input.lines() {\n        let fields:Vec<i128>=line.split(\' \').map(|v|v.parse().unwrap()).collect();\n        match transition(&fields) {\n            Some(output)=>println!("{}",output[0]),None=>println!("null"),\n        }\n    }\n}\n'


def execute(path:Path,cases:tuple[Case,...],rustc:str,node:str,directory:Path,label:str)->list:
    text="".join(" ".join(str(x) for x in case.operands)+"\n" for case in cases)
    if path.suffix==".rs":
        driver=path.with_name(path.stem+"-driver.rs")
        driver.write_text(path.read_text()+RUST_DRIVER)
        executable=path.with_name(path.stem+"-driver")
        run([rustc,"--edition=2024","--check-cfg","cfg(verus_keep_ghost)","--crate-name","scalar_arithmetic_subject",str(driver),"-o",str(executable)],directory,label+"-build")
        output=run([str(executable)],directory,label,input_text=text)
    elif path.suffix==".py":
        driver="import sys, json; namespace={}; exec(open(sys.argv[1]).read(),namespace); transition=namespace['transition'];\nfor line in sys.stdin:\n r=transition(tuple(int(v) for v in line.strip().split(' '))); print(json.dumps(None if r is None else r[0]))"
        output=run([sys.executable,"-c",driver,str(path)],directory,label,input_text=text)
    else:
        driver=path.with_name(path.stem+"-driver.mjs")
        driver.write_text("import {transition} from "+json.dumps(path.as_uri())+";\nimport fs from 'node:fs';\nfor (const line of fs.readFileSync(0,'utf8').trimEnd().split('\\n')) {const r=transition(line); console.log(r===null?'null':r); }\n")
        output=run([node,str(driver)],directory,label,input_text=text)
    lines=output.splitlines()
    if len(lines)!=len(cases):
        raise RuntimeError("missing output rows, not a semantic mutant kill: "+label)
    try:
        values=[None if line=="null" else int(line) for line in lines]
    except ValueError as error:
        raise RuntimeError("malformed output, not a semantic mutant kill: "+label) from error
    return values


def check(out:Path,node:str,stub:bool)->dict:
    before=snapshot()
    out.mkdir(parents=True,exist_ok=False)
    sources=out/"emitted"
    rustc=run(["rustup","which","--toolchain","1.97.1","rustc"],out,"rustc-path").strip()
    version=run([rustc,"--version"],out,"rustc-version").strip()
    if not version.startswith("rustc 1.97.1 "):
        raise RuntimeError("wrong compiler pin")
    node_version=run([node,"--version"],out,"node-version").strip()
    if node_version != "v"+(ROOT/".node-version").read_text().strip():
        raise RuntimeError("wrong Node pin")
    environment=dict(os.environ,ZENO_FCIS_ARITHMETIC_EMIT_DIR=str(sources))
    if not environment.get("CARGO_TARGET_DIR"):
        raise RuntimeError("an owned CARGO_TARGET_DIR is required")
    jobs=int(environment.get("CARGO_BUILD_JOBS","1"))
    if not 1<=jobs<=6:
        raise RuntimeError("Cargo jobs must be within the assigned cap, at most six")
    environment["RUST_TEST_THREADS"]="1"
    run(["cargo","+1.97.1","test","-p","zeno-fcis-synthesis","--test","scalar_arithmetic","--locked","--offline","-j",str(jobs),"--","emit_qualification_subjects","--exact"],out,"emit",env=environment)
    vectors=boundary_cases(BITS)
    all_actual=[];all_expected=[];subjects=[]
    for operation,arity in [("mul",2),("div_floor",2),("div_ceil",2),("mul_div_floor",3),("mul_div_ceil",3),("products_ge",4),("products_ge",6)]:
        cases=tuple(c for c in vectors if c.operation==operation and len(c.operands)==arity)
        expected=[result(c,BITS) for c in cases]
        for extension in ("rs","py","mjs"):
            key=f"{operation}-{arity}.{extension}";path=sources/key
            actual=execute(path,cases,rustc,node,out,key)
            if not compare(actual,expected,not stub):
                mismatch=next(i for i,(a,b) in enumerate(zip(actual,expected)) if a!=b)
                raise ValueError(f"{key} differs at {cases[mismatch]}: {actual[mismatch]} != {expected[mismatch]}")
            all_actual+=actual;all_expected+=expected
            subjects.append({"file":key,"sha256":hashlib.sha256(path.read_bytes()).hexdigest(),"rows":len(cases)})
    # The false choice arm still executes its division. These two inputs have
    # independently fixed observations: division-by-zero refusal, then zero.
    eager_cases=(Case("div_floor",(1,0)),Case("div_floor",(1,2)))
    for extension in ("rs","py","mjs"):
        key=f"eager-choose.{extension}";path=sources/key
        actual=execute(path,eager_cases,rustc,node,out,key)
        all_actual+=actual;all_expected+=[None,0]
        subjects.append({"file":key,"sha256":hashlib.sha256(path.read_bytes()).hexdigest(),"rows":2})
    # Faults run successfully through genuine emitted code and are graded by
    # observed results. Compilation/process failures never enter this map.
    mutants={}
    faults=[
        ("truncation","div_floor-2.py","v0 // v1","int(abs(v0)//abs(v1)) * (-1 if (v0<0)!=(v1<0) else 1)",Case("div_floor",(-1,2))),
        ("rounding_swap","div_ceil-2.py","-((-v0) // v1)","v0 // v1",Case("div_ceil",(1,2))),
        ("dropped_product_word","mul_div_floor-3.py","(v0 * v1) // v2","((v0 * v1) & ((1<<128)-1)) // v2",Case("mul_div_floor",((1<<127)-1,(1<<127)-1,(1<<127)-1))),
        ("hardcoded_fee","mul_div_ceil-3.py","(v0 * v1 + v2 - 1) // v2","1",Case("mul_div_ceil",(20,3,10))),
        ("all_zero","mul_div_ceil-3.py","(v0 * v1 + v2 - 1) // v2","0",Case("mul_div_ceil",(11,1,10))),
        ("always_refuse","mul_div_ceil-3.py","return (v3,)","return None",Case("mul_div_ceil",(11,1,10))),
        ("division_zero_to_zero","div_floor-2.rs","if b == 0 { return None; }","if b == 0 { return Some(0); }",Case("div_floor",(1,0))),
        ("wrapping_multiply","mul-2.rs","a.checked_mul(b)","Some(a.wrapping_mul(b))",Case("mul",((1<<127)-1,2))),
    ]
    for name,base,anchor,replacement,witness in faults:
        original=sources/base
        changed=sources/(name+original.suffix)
        changed.write_text(replace_once(original.read_text(),anchor,replacement))
        actual=execute(changed,(witness,),rustc,node,out,"mutant-"+name)
        mutants[name]=(actual,[result(witness,BITS)])
    lazy=sources/"lazy-choose.py"
    source=replace_once((sources/"eager-choose.py").read_text(),
        "if v1 == 0:","if False:")
    lazy.write_text(replace_once(source,"v2 = v0 // v1","v2 = 0"))
    actual=execute(lazy,(eager_cases[0],),rustc,node,out,"mutant-lazy-choose")
    mutants["lazy_choose"]=(actual,[None])
    killed=grade(all_actual,all_expected,mutants,not stub)
    if before!=snapshot():
        raise RuntimeError("source drift during conformance")
    return {"schema":"zeno-fcis/scalar-arithmetic-conformance/1","status":"passed",
        "authority":"none","source_sha256":before,"subjects":subjects,"rows":len(all_actual),
        "mutants":killed,"tools":{"rustc":version,"node":node_version,"python":sys.version},
        "scope":"source-emitted arithmetic on the pinned boundary/sample corpus",
        "nonclaims":["Verus proof","complete application-domain enumeration","compiled-app Authority or store replay","compiler proof","deployment approval"]}


def main()->int:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out",type=Path,required=True)
    parser.add_argument("--node",default="node")
    parser.add_argument("--stub-comparison",action="store_true")
    args=parser.parse_args()
    folder=args.out.resolve()
    if folder.exists():
        parser.error("--out must name a new evidence directory")
    try:
        report=check(folder,args.node,args.stub_comparison)
    except (OSError,ValueError,RuntimeError,subprocess.SubprocessError) as error:
        folder.mkdir(parents=True,exist_ok=True)
        report={"schema":"zeno-fcis/scalar-arithmetic-conformance/1","status":"failed","authority":"none","error":str(error)}
    (folder/"receipt.json").write_text(json.dumps(report,indent=2,sort_keys=True)+"\n")
    print(json.dumps({k:report[k] for k in ("status","authority")}))
    return 0 if report["status"]=="passed" else 1

if __name__ == "__main__":
    from telemetry import run_cli
    def _entrypoint():
        raise SystemExit(main())
    run_cli(_entrypoint)
