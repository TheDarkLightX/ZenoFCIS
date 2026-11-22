#!/usr/bin/env python3
"""Pinned, disposable wide-width VeriISLE popcnt experiment.

Fresh solver-local names versus expression inlining, cold build separated from
warm verifier calls. These are SMT checks of declared BV semantics only.
"""
from pathlib import Path
import hashlib, json, os, re, subprocess, sys, time

PIN = "fb792bf7a143801c22c0ba5b16fe2c6085a8598b"
SOLVER = "cranelift/isle/veri/veri/src/solver.rs"
POPCNT = "cranelift/isle/veri/veri/src/encoded/popcnt.rs"
BLOBS = {SOLVER: "8d3b5eb63482b9b6f438a517fa15e22d9c0accd9",
         POPCNT: "418c77c4b7c9b3bd477d31b0757de2488faacf1a"}
OLD = """                let id = x.index();
                match width {
                    8 | 16 | 32 | 64 => Ok(popcnt(&mut self.smt, width, xe, id)),
"""
FRESH = """                let id = self.tmp_idx;
                self.tmp_idx += 1;
                match width {
                    8 | 16 | 32 | 64 => Ok(popcnt(&mut self.smt, width, xe, id)),
"""
INLINE_OLD = """    let id = format!("{ty}_{id}");
    let result = declare(
        smt,
        format!("popcnt_{id}"),
        smt.list(vec![
            smt.atoms().und,
            smt.atom("BitVec"),
            smt.numeral(bits_for_result),
        ]),
    );
    smt.assert(smt.eq(result, r)).unwrap();
    zero_extend(smt, ty - bits_for_result, result)
"""
INLINE_NEW = """    // Experimental comparison: retain the exact bit sum, but inline it.
    zero_extend(smt, ty - bits_for_result, r)
"""
ROOT=Path("upstream")
OUT=Path(os.environ.get("RUNNER_TEMP","."))/"tier1-popcnt-wide-matrix"
OUT.mkdir(parents=True,exist_ok=True)
state={
  "schema":"zenofcis/tier1-veriisle-popcnt-wide-matrix/v1",
  "wasmtime_commit":PIN,"original_blobs":BLOBS,"variants":{},"cases":{},
  "contract":{
    "fixed":"64x2 and 16x8 lane-map bitvectors (128 total bits), fixed independent Python bit_count oracle",
    "symbolic":"all x in BV32/BV64: popcnt(x) + popcnt(bvnot(x)) = width",
    "signedness":"fixed-width bit population count independent of signed interpretation",
    "overflow":"SMT modular bitvectors; sum of complementary popcounts fits width",
    "flags":"no physical flags modeled","poison_undefined":"not modeled, no CLIF or native claims",
    "verification":"VeriISLE solver passes vs failures vs unknown vs timeout separately",
    "cost":"warm-process wall time for individual filetests (screening only; NOT emitted bytes or compiler speed)"
  }
}

def git(*a):
    return subprocess.check_output(["git","-C",str(ROOT),*a],text=True).strip()

def cmd(argv,timeout=120):
    begin=time.monotonic()
    try:
        r=subprocess.run(argv,cwd=ROOT,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=timeout)
        return r.returncode,r.stdout,False,round(time.monotonic()-begin,4)
    except subprocess.TimeoutExpired as e:
        x=e.stdout or ""
        if isinstance(x,bytes): x=x.decode(errors="replace")
        return None,x,True,round(time.monotonic()-begin,4)

def lane_case(name,width,values,mutant=False):
    assert width*len(values)==128 and width in (16,64)
    mask=(1<<width)-1
    val=sum((v&mask)<<(width*i) for i,v in enumerate(values))
    expected=sum(v.bit_count()<<(width*i) for i,v in enumerate(values))
    if mutant: expected ^= 1
    body=f"""(type Value (primitive Value))
(model Value (type (bv 128)))
(decl test (Value) Value)
(spec (test x) (provide (= result (lanemap ((x x)) (popcnt x)))))
(instantiate test ((args (bv {width} x {len(values)})) (ret (bv {width} x {len(values)}))))
(decl input () Value)
(extern extractor input input)
(spec (input) (provide (= result #x{val:032x})))
(decl counts () Value)
(extern constructor counts counts)
(spec (counts) (provide (= result #x{expected:032x})))
(rule test (test (input)) (counts))
"""
    return name,body,{"oracle":"Python int.bit_count","values":list(map(hex,values)),"counts":[v.bit_count() for v in values],"mutant":mutant}

def complement_case(width):
    name=f"symbolic_popcnt_complement{width}"
    body=f"""(type Value (primitive Value))
(model Value (type (bv {width})))
(decl test (Value) Value)
(spec (test x) (provide (= result #x{width:0{width//4}x})))
(instantiate test ((args (bv {width})) (ret (bv {width}))))
(decl complementary_counts (Value) Value)
(extern constructor complementary_counts complementary_counts)
(spec (complementary_counts x) (provide (= result (bvadd (popcnt x) (popcnt (bvnot x))))))
(rule test (test x) (complementary_counts x))
"""
    return name,body,{"oracle":"symbolic popcnt complement identity", "width":width}

def test(variant,name,rep,seconds=120):
    argv=["cargo","test","--locked","-p","cranelift-isle-veri","--test","filetests",
          "pass_"+name,"--","--nocapture"]
    rc,out,timeout,elapsed=cmd(argv,seconds)
    log=OUT/f"{variant}_{name}_{rep}.log";log.write_text(out)
    keys=["Applicable:","Verification passed:","Verification failed:","Verification unknown:","Expansion errors:"]
    metrics={}
    for key in keys:
        m=re.search(re.escape(key)+r"\s*(\d+)",out)
        metrics[key]=int(m.group(1)) if m else None
    if timeout: verdict="timeout"
    elif (rc==0 and metrics=={"Applicable:":1,"Verification passed:":1,
                              "Verification failed:":0,"Verification unknown:":0,"Expansion errors:":0}):
        verdict="verified_under_model"
    elif metrics["Verification failed:"] and metrics["Verification failed:"]>0:
        verdict="counterexample"
    elif metrics["Verification unknown:"] and metrics["Verification unknown:"]>0: verdict="unknown"
    else: verdict="failed_or_not_applicable"
    print(json.dumps({"variant":variant,"case":name,"rep":rep,"elapsed":elapsed,"verdict":verdict}),flush=True)
    return {"argv":argv,"exit":rc,"timeout":timeout,"elapsed_s":elapsed,"log":log.name,
            "log_sha256":hashlib.sha256(log.read_bytes()).hexdigest(),"metrics":metrics,"verdict":verdict}

try:
    assert git("rev-parse","HEAD")==PIN
    pristine={}
    for path,blob in BLOBS.items():
        pristine[path]=subprocess.check_output(["git","-C",str(ROOT),"show",PIN+":"+path],text=True)
        assert git("rev-parse",PIN+":"+path)==blob
    assert pristine[SOLVER].count(OLD)==1
    assert pristine[POPCNT].count(INLINE_OLD)==1
    cases=[
      lane_case("wide_popcnt16x8",16,[0,1,2,3,0xffff,0x8000,0x5555,0xaaaa]),
      lane_case("wide_popcnt64x2",64,[0x8000000000000001,0xffffffffffffffff]),
      complement_case(32),complement_case(64)
    ]
    mutant=lane_case("wide_popcnt64x2_mutant",64,[0x8000000000000001,0xffffffffffffffff],True)
    directory=ROOT/"cranelift/isle/veri/veri/filetests/pass"
    for name,source,contract in cases+[mutant]:
        p=directory/(name+".isle")
        assert not p.exists()
        p.write_text(source)
        state["cases"][name]={"contract":contract,"fixture_sha256":hashlib.sha256(source.encode()).hexdigest(),"runs":{}}
    for variant in ("fresh_named","inlined"):
        (ROOT/SOLVER).write_text(pristine[SOLVER].replace(OLD,FRESH) if variant=="fresh_named" else pristine[SOLVER])
        (ROOT/POPCNT).write_text(pristine[POPCNT] if variant=="fresh_named" else pristine[POPCNT].replace(INLINE_OLD,INLINE_NEW))
        state["variants"][variant]={"source_blobs":{p:git("hash-object",p) for p in BLOBS}}
        state["variants"][variant]["build_warmup"]=test(variant,"wide_popcnt16x8","build_warmup",seconds=480)
        for name,_,_ in cases:
            state["cases"][name]["runs"][variant]=[test(variant,name,k) for k in range(2)]
        state["cases"][mutant[0]]["runs"][variant]=[test(variant,mutant[0],"mutant")]
    fixed_ok=all(r["verdict"]=="verified_under_model"
                 for name in ("wide_popcnt16x8","wide_popcnt64x2")
                 for r in state["cases"][name]["runs"]["fresh_named"])
    mutant_red=state["cases"][mutant[0]]["runs"]["fresh_named"][0]["verdict"]=="counterexample"
    state["status"]="qualified_fixed_lanes_and_negative_control" if fixed_ok and mutant_red else "failed_or_unknown_fixed_lanes"
except Exception as e:
    state["status"]="setup_error";state["error"]=repr(e)
    print("UNQUALIFIED:",repr(e),file=sys.stderr)
finally:
    (OUT/"matrix_receipt.json").write_text(json.dumps(state,indent=2,sort_keys=True)+"\n")
print("WIDE MATRIX:",state["status"])
# Symbolic timeouts/unknowns remain explicit and do not become purported proofs.
sys.exit(0 if state["status"]=="qualified_fixed_lanes_and_negative_control" else 1)
