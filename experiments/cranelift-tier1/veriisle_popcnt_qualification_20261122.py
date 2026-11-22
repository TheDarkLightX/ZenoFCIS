#!/usr/bin/env python3
"""A/B verify fresh popcnt SMT names against exact pinned Wasmtime source.

Research only: uses a disposable source checkout; never changes Wasmtime upstream.
A timeout, a solver unknown, or a test failure is not a correctness proof.
"""
from pathlib import Path
import hashlib, json, os, subprocess, sys, time

PIN="fb792bf7a143801c22c0ba5b16fe2c6085a8598b"
SOLVER="cranelift/isle/veri/veri/src/solver.rs"
POPCNT="cranelift/isle/veri/veri/src/encoded/popcnt.rs"
BLOBS={SOLVER:"8d3b5eb63482b9b6f438a517fa15e22d9c0accd9", POPCNT:"418c77c4b7c9b3bd477d31b0757de2488faacf1a"}
OLD="""                let xe = self.expr_atom(x);
                let id = x.index();
                match width {
                    8 | 16 | 32 | 64 => Ok(popcnt(&mut self.smt, width, xe, id)),
"""
NEW="""                let xe = self.expr_atom(x);
                // Lane-map reuses an ExprId for different concrete lane values.
                // Keep named popcount definitions, but make each invocation fresh.
                let id = self.tmp_idx;
                self.tmp_idx += 1;
                match width {
                    8 | 16 | 32 | 64 => Ok(popcnt(&mut self.smt, width, xe, id)),
"""
ROOT=Path("upstream")
OUT=Path(os.environ.get("RUNNER_TEMP", "."))/"tier1-popcnt-veri"
OUT.mkdir(parents=True,exist_ok=True)
state={"status":"unqualified","wasmtime_pin":PIN,"blobs":BLOBS,"tests":{},"semantics":{
    "lanes":"bitvector modular arithmetic, fixed bv8x2 and bv32x2 witnesses",
    "scalar":"arbitrary 64-bit bitvector with equality of independently encoded popcnt instances",
    "signedness":"same population count for signed/unsigned interpretations of a fixed bit pattern",
    "overflow":"SMT bitvector wrap; count zero-extended to lane width",
    "flags":"no native flags or registers modeled",
    "poison_undefined":"no native/CLIF/poison model; only SMT expression semantics",
    "cost":"none; verifier correctness experiment, not a compiler optimization",
    "proof":"solver-based only if actual verifier succeeds; timeout/failure unknown",
}}
def git(*args):
    return subprocess.check_output(["git","-C",str(ROOT),*args],text=True).strip()
def step(name,cmd,seconds):
    start=time.monotonic()
    try:
        p=subprocess.run(cmd,cwd=ROOT,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=seconds)
        rc,timeout,out=p.returncode,False,p.stdout
    except subprocess.TimeoutExpired as e:
        rc,timeout,out=None,True,e.stdout or b""
        if isinstance(out,bytes):out=out.decode(errors="replace")
    log=OUT/(name+".log")
    log.write_text(out)
    record={"argv":cmd,"exit":rc,"timeout":timeout,"elapsed_s":round(time.monotonic()-start,3),"log":log.name,"sha256":hashlib.sha256(log.read_bytes()).hexdigest()}
    state["tests"][name]=record
    print(json.dumps({"step":name,"exit":rc,"timeout":timeout,"seconds":record["elapsed_s"]}),flush=True)
    return record
def fixture(name,width,lanes,input_bits,output_bits):
    d=ROOT/"cranelift/isle/veri/veri/filetests/pass"
    p=d/(name+".isle")
    if p.exists():raise ValueError("Test fixture exists: "+str(p))
    total=width*lanes
    data=f"""(type Value (primitive Value))
(model Value (type (bv {total})))
(decl test (Value) Value)
(spec (test x) (provide (= result (lanemap ((x x)) (popcnt x)))))
(instantiate test ((args (bv {width} x {lanes})) (ret (bv {width} x {lanes}))))
(decl input () Value)
(extern extractor input input)
(spec (input) (provide (= result #x{input_bits})))
(decl counts () Value)
(extern constructor counts counts)
(spec (counts) (provide (= result #x{output_bits})))
(rule test (test (input)) (counts))
"""
    p.write_text(data)
def scalar():
    p=ROOT/"cranelift/isle/veri/veri/filetests/pass/scalar_popcnt64.isle"
    if p.exists():raise ValueError("Scalar fixture already present")
    p.write_text("""(type Value (primitive Value))
(model Value (type (bv 64)))
(decl test (Value) Value)
(spec (test x) (provide (= result (popcnt x))))
(instantiate test ((args (bv 64)) (ret (bv 64))))
(decl pc (Value) Value)
(extern constructor pc pc)
(spec (pc x) (provide (= result (popcnt x))))
(rule test (test x) (pc x))
""")
def test(name,seconds=480):
    return step(name,["cargo","test","--locked","-p","cranelift-isle-veri","--test","filetests",name.replace("patched_","").replace("baseline_",""),"--","--nocapture"],seconds)
try:
    assert git("rev-parse","HEAD")==PIN
    for path,sha in BLOBS.items():
        assert git("hash-object",path)==sha,(path,"incorrect source blob")
    original=(ROOT/SOLVER).read_text()
    assert original.count(OLD)==1
    for cmd in [["rustc","-Vv"],["cargo","--version"],["z3","--version"],["cvc5","--version"]]:
        r=step("tool_"+cmd[0],cmd,20)
        if r["exit"]!=0:raise RuntimeError("Unavailable tool: "+cmd[0])
    fixture("lanes_popcnt8",8,2,"03f0","0204")
    fixture("lanes_popcnt32",32,2,"0000000100000003","0000000100000002")
    scalar()
    before=test("baseline_lanes_popcnt8")
    if before["timeout"] or before["exit"] in (None,0):
        raise RuntimeError("Baseline did not fail promptly; cannot confirm regression")
    changed=original.replace(OLD,NEW)
    (ROOT/SOLVER).write_text(changed)
    diff=subprocess.check_output(["git","-C",str(ROOT),"diff","--",SOLVER],text=True)
    (OUT/"candidate.patch").write_text(diff)
    state["patched_solver_blob"]=git("hash-object",SOLVER)
    checked=[]
    for name,limit in [("patched_lanes_popcnt8",480),("patched_lanes_popcnt32",480),("patched_scalar_popcnt64",300),("patched_lanes_double_negation",480)]:
        r=test(name,limit)
        checked.append((name,r))
    if all(r["exit"]==0 and not r["timeout"] for _,r in checked):
        state["status"]="targeted_checks_passed"
    else:
        state["status"]="failed_or_unknown"
except Exception as e:
    state["error"]=str(e)
    print("UNQUALIFIED:",e,file=sys.stderr)
finally:
    (OUT/"receipt.json").write_text(json.dumps(state,indent=2,sort_keys=True)+"\n")
    print("Receipt:",OUT/"receipt.json","status:",state["status"])
if state["status"]!="targeted_checks_passed":sys.exit(1)
