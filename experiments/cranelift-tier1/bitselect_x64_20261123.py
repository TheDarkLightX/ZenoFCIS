#!/usr/bin/env python3
"""Isolated i64 bitselect experiment. No upstream submission or profitability claim."""
import ctypes, hashlib, itertools, json, os, random, subprocess, sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT))
from tools.machine_code import session, verify2, targets
UP="cef75a9fea591317bb076ec04d1f1a99ccfedb63"
BLOB="545c79528cbb10fb1280623f23f53880b9762e49"
SOURCE="cranelift/codegen/src/isa/x64/lower.isle"
TARGET="x86_64-sysv-register/2"
def cmd(*args):
    return subprocess.check_output(list(map(str,args)),text=True).strip()
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def save(p,x): p.write_text(json.dumps(x,indent=2,sort_keys=True)+"\n")
out=Path(sys.argv[2]).resolve()
phase=sys.argv[1]
if phase=="search":
    out.mkdir(parents=True,exist_ok=True)
    c,t,f=[["input",v] for v in ["c","t","f"]]
    grammar=[["mov","rax","rsi"],["mov","rax","rdx"],["xor","rax","rdx"],["xor","rax","rsi"],["and","rax","rdi"],["or","rax","rdi"]]
    problem={"schema":"zeno-fcis/machine-problem/2","target":TARGET,"inputs":["c","t","f"],"outputs":[["or",["and",c,t],["and",["not",c],f]]],"grammar":grammar,"max_instructions":4,"objective":"bytes"}
    p=out/"problem.json"; save(p,problem)
    session.open_session(p,out/"session")
    assert not (out/"session/events.jsonl").read_bytes()
    res=session.search(out/"session",max_candidates=5000,max_checks=12,deadline_ms=30000,timeout_ms=3000,prune=True)
    save(out/"search.json",res)
    assert res["status"]=="checked-candidate",res
    assert session.resume(out/"session",timeout_ms=3000)["status"]=="replayed"
    assert session.emit(out/"session",out/"artifact",timeout_ms=3000)["status"]=="emitted"
    assert session.replay_artifact(out/"artifact",timeout_ms=3000)["status"]=="replayed"
    code=(out/"artifact/kernel.bin").read_bytes()
    check=verify2.check(problem,code,timeout_ms=3000)
    assert check["status"]=="equivalent",check
    wrong=targets.encode_program(TARGET,[["mov","rax","rsi"]])
    neg=verify2.check(problem,wrong,timeout_ms=3000)
    assert neg["status"]=="counterexample",neg
    import z3
    c,t,f=z3.BitVecs("c t f",64)
    reference=(c&t)|(~c&f)
    proof={}
    for name,expr in {"xor":((t^f)&c)^f,"andn":(c&t)|(~c&f),"mutant":(t^f)&c}.items():
        s=z3.SolverFor("QF_BV");s.set(timeout=10000);s.add(reference!=expr)
        (out/(name+".smt2")).write_text(s.to_smt2())
        result=s.check(); proof[name]=str(result)
        assert result==(z3.sat if name=="mutant" else z3.unsat)
    save(out/"proof.json",{"scope":"pure 64-bit returned word, all 2^192 inputs; no trap/poison admitted; flags dead at return, no whole-machine refinement","z3":z3.get_full_version(),"conditional_smt":proof,"zeno_check":check,"negative":neg,"zeno_source":cmd("git","-C",ROOT,"rev-parse","HEAD"),"candidate_hex":code.hex(),"problem_sha256":sha(p)})
    (out/"bitselect.clif").write_text("""test verifier
set opt_level=speed_and_size
target x86_64
function %tier1_bitselect(i64, i64, i64) -> i64 {
block0(v0: i64, v1: i64, v2: i64):
    v3 = bitselect v0, v1, v2
    return v3
}
""")
elif phase=="patch":
    upstream=Path(sys.argv[3])
    assert cmd("git","-C",upstream,"rev-parse","HEAD")==UP
    assert cmd("git","-C",upstream,"hash-object",SOURCE)==BLOB
    assert not cmd("git","-C",upstream,"diff","--name-only")
    path=upstream/SOURCE
    old=path.read_text()
    needle="(rule 4 (lower (bitselect (ty_int_ref_scalar_64_extract ty) c t f))"
    assert old.count(needle)==1
    # ISLE conservatively treats ty_scalar_float extractor as overlapping $I64.
    # Keep priorities distinct from the existing float rule (5); no change to its semantics.
    # BMI1 ANDN is already used by adjacent band-not lowering.
    rule=""";; Experimental Tier-1 bitselect: keep generic rule for other types.
(rule 7 (lower (bitselect $I64 c t f))
      (if-let true (has_bmi1))
      (let ((a Gpr (x64_and $I64 c t))
            (b Gpr (x64_andn $I64 c f)))
        (x64_or $I64 a b)))
(rule 6 (lower (bitselect $I64 c t f))
      (let ((d Gpr (x64_xor $I64 t f))
            (a Gpr (x64_and $I64 c d)))
        (x64_xor $I64 a f)))

"""
    path.write_text(old.replace(needle,rule+needle))
    assert cmd("git","-C",upstream,"diff","--name-only")==SOURCE
    (out/"experimental.patch").write_text(cmd("git","-C",upstream,"diff","--",SOURCE)+"\n")
elif phase=="measure":
    import re
    assert (out/"proof.json").exists()
    rng=random.Random(20261123); M=(1<<64)-1
    edge=[0,1,2,255,256,1<<31,1<<32,1<<63,M,M-1,0x5555555555555555,0xaaaaaaaaaaaaaaaa]
    inputs=list(itertools.product(edge,repeat=3))+[tuple(rng.getrandbits(64) for _ in range(3)) for i in range(20000)]
    rows=[]
    for cfg in ("generic","bmi1"):
        for stage in ("baseline","candidate"):
            obj=out/(cfg+"-"+stage+".o")
            so=obj.with_suffix(".so")
            subprocess.run(["gcc","-shared","-o",str(so),str(obj)],check=True)
            lib=ctypes.CDLL(str(so)); fn=getattr(lib,"%tier1_bitselect"); fn.argtypes=[ctypes.c_uint64]*3;fn.restype=ctypes.c_uint64
            for c,t,f in inputs: assert fn(c,t,f)==((c&t)|((M^c)&f))
            dump=cmd("objdump","-dw","-Mintel",obj);obj.with_suffix(".disasm").write_text(dump+"\n")
            syms=[l.split() for l in cmd("nm","-S","--defined-only","--format=posix",obj).splitlines()]
            sz=[int(v[3],16) for v in syms if len(v)==4 and v[1] in ("T","t")]
            assert len(sz)==1
            instructions=[]
            for l in dump.splitlines():
                m=re.match(r"^\s*([0-9a-f]+):\s+((?:[0-9a-f]{2}\s+)+)\s*(\S.*)$",l)
                if m:
                    a=int(m[1],16); b=bytes.fromhex(m[2])
                    assert not instructions or a==instructions[-1][0]+len(instructions[-1][1])
                    instructions.append((a,b,m[3]))
                    if re.match(r"^ret(?:q)?(?:\s|$)",m[3]):break
            assert instructions and instructions[-1][2].startswith("ret")
            prefix=sum(len(v[1]) for v in instructions);assert prefix<=sz[0]
            rows.append({"config":cfg,"stage":stage,"symbol_bytes":sz[0],"prefix_bytes":prefix,"instructions":len(instructions),"native_checks":len(inputs),"sha256":sha(obj),"dedup_key":hashlib.sha256(b"i64-bitselect:"+b"".join(v[1] for v in instructions)).hexdigest()})
    save(out/"catalog.json",{"upstream":UP,"source_blob":BLOB,"zenofcis":cmd("git","-C",ROOT,"rev-parse","HEAD"),"contract":"all i64 mask,true,false inputs; per-bit selection; no arithmetic overflow; flags not observable at return; BMI1 only on supported host","proof_sha256":sha(out/"proof.json"),"rows":rows,"limits":"finite native executions and conditional expression/registered machine model proof; not whole compiler proof or throughput"})
else:raise ValueError(phase)
