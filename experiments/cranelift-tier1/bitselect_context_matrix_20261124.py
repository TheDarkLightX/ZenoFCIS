#!/usr/bin/env python3
"""Current-source, generic-XOR-only bitselect A/B context qualification.

Run with a pristine checkout at UP. No upstream commits; generated evidence
records code size and finite execution, not a claim of full compiler proof.
"""
import ctypes as C
import hashlib, itertools, json, os, random, re, subprocess, sys
from pathlib import Path

UP = "d49635a71351df82f3320095801848d8f128b938"
BLOB = "545c79528cbb10fb1280623f23f53880b9762e49"
SOURCE = "cranelift/codegen/src/isa/x64/lower.isle"
RULE = "tier1_i64_bitselect_xor"
NEEDLE = "(rule 4 (lower (bitselect (ty_int_ref_scalar_64_extract ty) c t f))"
PATCH = """;; Experimental i64 only: compare XOR with baseline, including BMI1 hosts.
(rule tier1_i64_bitselect_xor 6 (lower (bitselect $I64 c t f))
      (let ((d Gpr (x64_xor $I64 t f))
            (a Gpr (x64_and $I64 c d)))
        (x64_xor $I64 a f)))

"""
M = (1 << 64) - 1
def select(c,t,f):
    return (c&t)|((M^c)&f)
def sha(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def save(p,obj):
    Path(p).write_text(json.dumps(obj,sort_keys=True,indent=2)+"\n")
def run(*args, cwd=None, env=None):
    return subprocess.check_output(list(map(str,args)),cwd=cwd,env=env,text=True,stderr=subprocess.STDOUT)
def logged(path,args,cwd=None,env=None):
    with Path(path).open("w") as f:
        subprocess.run(list(map(str,args)),cwd=cwd,env=env,stdout=f,stderr=subprocess.STDOUT,check=True)

# Case = arity, optional single load argument, instructions, independent oracle.
# Pointer inputs are valid, aligned, readable uint64_t slots, no volatile,
# atomic, concurrency or invalid-address behavior is claimed.
def make_cases():
    cases={}
    def add(name, arity, ptr, ops, ref):
        cases[name]=(arity,ptr,ops,ref)
    bit="    v3 = bitselect v0, v1, v2\n"
    add("plain",3,None,bit+"    return v3",lambda x:select(*x[:3]))
    for i,n in enumerate(("mask","true","false")):
        add(n+"_live",3,None,bit+f"    v4 = iadd v3, v{i}\n    return v4",
            lambda x,i=i:(select(*x[:3])+x[i])&M)
    add("all_live",3,None,bit+"    v4 = iadd v0, v1\n    v5 = ixor v4, v2\n    v6 = iadd v3, v5\n    return v6",
        lambda x:(select(*x[:3])+(((x[0]+x[1])&M)^x[2]))&M)
    for i,n in enumerate(("mask","true","false")):
        args=[f"v{j}" for j in range(3)]
        args[i]="v3"
        ops=f"    v3 = load.i64 v{i}\n    v4 = bitselect "+", ".join(args)+"\n    return v4"
        add("load_"+n,3,i,ops,lambda x:select(*x[:3]))
    add("pressure6",6,None,"    v6 = bitselect v0, v1, v2\n    v7 = iadd v6, v3\n    v8 = ixor v7, v4\n    v9 = iadd v8, v5\n    v10 = ixor v9, v0\n    v11 = iadd v10, v1\n    v12 = ixor v11, v2\n    return v12",
        lambda x:(((((((select(*x[:3])+x[3])&M)^x[4])+x[5])&M)^x[0])+x[1]&M)^x[2])
    add("comparison_live",3,None,bit+"    v4 = icmp ult v0, v1\n    v5 = select v4, v3, v2\n    return v5",
        lambda x:select(*x[:3]) if x[0]<x[1] else x[2])
    add("constant_mask",3,None,"    v3 = iconst.i64 6148914691236517205\n    v4 = bitselect v3, v1, v2\n    return v4",
        lambda x:select(0x5555555555555555,x[1],x[2]))
    return cases
CASES=make_cases()

def prepare(out):
    out.mkdir(parents=True,exist_ok=True)
    for name,(arity,ptr,ops,_) in CASES.items():
        types=", ".join(["i64"]*arity)
        params=", ".join(f"v{i}: i64" for i in range(arity))
        (out/(name+".clif")).write_text(
            "test verifier\nset opt_level=speed_and_size\ntarget x86_64\n"+
            f"function %tier1_{name}({types}) -> i64 {{\nblock0({params}):\n{ops}\n}}\n")
    save(out/"contracts.json",{"word":"unsigned interpretation only for icmp ult; otherwise modular 64-bit bitwise/addition", "overflow":"wrap in iadd, no trap/poison from bitselect", "flags":"not CLIF values, live comparison fixture observes unsigned predicate", "loads":"valid readable 8-byte address; no invalid/trapping or concurrent memory claim", "features":"x86_64 SysV, BMI1 explicitly toggled; candidate uses only generic operations", "cases":{n:{"arity":a,"load_arg":p,"clif_sha256":sha(out/(n+".clif"))} for n,(a,p,_,_) in CASES.items()}})
def patch(out,up):
    assert run("git","rev-parse","HEAD",cwd=up).strip()==UP
    assert run("git","hash-object",SOURCE,cwd=up).strip()==BLOB
    assert not run("git","diff","--name-only",cwd=up).strip()
    p=up/SOURCE
    s=p.read_text()
    assert s.count(NEEDLE)==1 and RULE not in s
    p.write_text(s.replace(NEEDLE,PATCH+NEEDLE))
    assert run("git","diff","--name-only",cwd=up).strip()==SOURCE
    (out/"experimental.patch").write_text(run("git","diff","--",SOURCE,cwd=up))
def measure(obj,name,case):
    arity,ptr,_,ref=case
    dump=run("objdump","-dw","-Mintel",obj)
    obj.with_suffix(".disasm").write_text(dump)
    obj.with_suffix(".relocs").write_text(run("objdump","-r",obj))
    syms=[line.split() for line in run("nm","-S","--defined-only","--format=posix",obj).splitlines()]
    syms=[s for s in syms if len(s)==4 and s[0]=="%tier1_"+name and s[1] in ("T","t")]
    assert len(syms)==1,syms
    start,size=int(syms[0][2],16),int(syms[0][3],16)
    ins=[]
    for line in dump.splitlines():
        m=re.match(r"^\s*([0-9a-f]+):\s+((?:[0-9a-f]{2}\s+)+)\s*(\S.*)$",line)
        if m and start<=int(m[1],16)<start+size:
            ins.append((int(m[1],16),bytes.fromhex(m[2]),m[3]))
    assert ins and ins[0][0]==start
    assert all(a[0]+len(a[1])==b[0] for a,b in zip(ins,ins[1:]))
    code=b"".join(v[1] for v in ins)
    assert len(code)==size,(name,size,len(code))
    so=obj.with_suffix(".so")
    logged(obj.with_suffix(".link.log"),["gcc","-shared","-o",so,obj])
    lib=C.CDLL(str(so))
    fn=getattr(lib,"%tier1_"+name)
    fn.argtypes=[C.c_uint64]*arity
    fn.restype=C.c_uint64
    rng=random.Random(20261124)
    edges=[0,1,2,255,256,1<<31,1<<32,1<<63,M,M-1,0x5555555555555555,0xaaaaaaaaaaaaaaaa]
    tests=[tuple(t)+tuple(rng.getrandbits(64) for _ in range(arity-3)) for t in itertools.product(edges,repeat=3)]
    tests += [tuple(rng.getrandbits(64) for _ in range(arity)) for _ in range(4096)]
    for values in tests:
        actual=list(values)
        if ptr is not None:
            slot=C.c_uint64(values[ptr])
            actual[ptr]=C.addressof(slot)
        got=fn(*actual)
        expect=ref(values)
        assert got==expect,(name,values,got,expect)
    return {"symbol_bytes":size,"instructions":len(ins),"native_checks":len(tests),"object_sha256":sha(obj),"body_sha256":hashlib.sha256(code).hexdigest(),"dedup_key":hashlib.sha256(("i64-bitselect:"+name+":").encode()+code).hexdigest(),"relocs_sha256":sha(obj.with_suffix(".relocs"))}

def matrix(out,up):
    prepare(out)
    assert run("git","rev-parse","HEAD",cwd=up).strip()==UP
    assert run("git","hash-object",SOURCE,cwd=up).strip()==BLOB
    assert "bmi1" in Path("/proc/cpuinfo").read_text().split("flags")[1].splitlines()[0].split()
    env=dict(os.environ,CARGO_TARGET_DIR=str(out/"target"))
    rows=[]
    for stage in ("baseline","xor"):
        if stage=="xor":patch(out,up)
        logged(out/(stage+"-build.log"),["cargo","build","--locked","--manifest-path",up/"cranelift/Cargo.toml","--bin","clif-util"],env=env)
        clif=out/"target/debug/clif-util"
        for name,case in CASES.items():
            fixture=out/(name+".clif")
            logged(out/(stage+"-"+name+"-verify.log"),[clif,"test",fixture])
            for opt in ("speed","speed_and_size"):
                for bmi in ("false","true"):
                    stem=f"{stage}-{name}-{opt}-bmi1_{bmi}"
                    obj=out/(stem+".o")
                    logged(out/(stem+".compile.log"),[clif,"compile","--target","x86_64","--set","opt_level="+opt,"--set","has_bmi1="+bmi,"-D","-o",obj,fixture])
                    row=measure(obj,name,case)
                    row.update(stage=stage,case=name,opt=opt,bmi1=bmi,fixture_sha256=sha(fixture))
                    rows.append(row)
                    save(out/"partial-rows.json",rows)
    by={(r["case"],r["opt"],r["bmi1"],r["stage"]):r for r in rows}
    comparisons=[]
    for (n,opt,bmi,stage),a in by.items():
        if stage!="baseline":continue
        b=by[(n,opt,bmi,"xor")]
        comparisons.append({"case":n,"opt":opt,"bmi1":bmi,"bytes_baseline":a["symbol_bytes"],"bytes_xor":b["symbol_bytes"],"delta_bytes":b["symbol_bytes"]-a["symbol_bytes"],"delta_instructions":b["instructions"]-a["instructions"]})
    save(out/"context-catalog.json",{"upstream":UP,"lower_blob":BLOB,"zenofcis":run("git","rev-parse","HEAD",cwd=Path(__file__).resolve().parents[2]).strip(),"patch_sha256":sha(out/"experimental.patch"),"contracts_sha256":sha(out/"contracts.json"),"rows":rows,"comparisons":comparisons,"limits":"exact ELF symbol bytes/instruction counts, finite native oracle tests, structural CLIF verifier; no throughput or full compiler/VeriISLE claim"})
if __name__=="__main__":
    phase=sys.argv[1]
    out=Path(sys.argv[2]).resolve()
    if phase=="prepare":prepare(out)
    elif phase=="patch":
        out.mkdir(parents=True,exist_ok=True);patch(out,Path(sys.argv[3]).resolve())
    elif phase=="matrix":matrix(out,Path(sys.argv[3]).resolve())
    else:raise ValueError(phase)
