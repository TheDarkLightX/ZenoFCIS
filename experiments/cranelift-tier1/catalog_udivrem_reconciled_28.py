#!/usr/bin/env python3
"""Fail-closed, content-key catalog of *previously measured* Cranelift udivrem.

Not a new executable benchmark, compiler-pass proof, or original discovery.
All numeric costs are decoded native x86-64 guest-function bytes, not CLIF cost.
"""
import argparse
import copy
import hashlib
import json
import subprocess
from pathlib import Path

HERE = Path(__file__).resolve().parent
WASMTIME = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
RECEIPTS = {
 "udivrem_context_stress_20261114.json": "c6eff621888a765b20fd10b0ae5fa25b000811d1",
 "flag_predicate_contexts_20261116.json": "fb229775c2944fbeecbb4091bf02c8b0c02e5019",
 "native_trap_table_qualified_20261115.json": "f75308a130b5aa4918ebf07817bb1cab9160e692",
 "structural_verifier_two_site_20261115.json": "3b97f5ceff36a2a570f9b67d6d77b61f09c6d4bf",
 "two_site_symbolic64_qualified_20261115.json": "f9e1efc38fac4561a3bd538fda3459db598daa39",
}
SOURCES = {
 "install_udivrem_combiner_28.py": "1e5ad0cc46b8ce791b14cad6998df4f32ea474ec",
 "install_udivrem_multivalue_28.py": "01b92461269b5ee354287a48a6750143e7564dc9",
 "qualify_udivrem_contexts_28.py": "e2d6417b1e5cd4c327da9adc79684c2124de5171",
}
WATS = {
 "two_live_pairs": "763d99565f7ec0530db081209bd6cea968a2bdcf58deb84b690209394cba634d",
 "long_lived_results": "6bac213185462eb1bae7b47a6982948ba86827d5a38c41474fed57d9464ccecf",
 "exclusive_branches": "297c7480253be378f8ee05f2c7436c840c8eec9d5de59b3a85d6de6661bafcad",
 "predicate_carried_across_pair": "54c105607917237698c273befa020fb27d4193b10336fdb2c7c5b152446713ea",
 "quotient_remainder_branch": "c4b1e62c836eefcbf2e9f5d843b05b026443bb485b459dc2b3b0e79062ff1674",
}
BASE = ["udiv.i64(x,y) -> q", "urem.i64(x,y) -> r"]
NEW = ["udivrem.i64(x,y) -> (q,r)"]
CONTRACT = {
 "domain": "CLIF unsigned i64 x,y; y!=0 returns q,r; y==0 traps",
 "matching": "same block, directly adjacent udiv and urem, exact same SSA operands and i64 types",
 "signedness": "unsigned; signed division excluded",
 "overflow": "u64 modular addition for derived second-pair operands",
 "trap": "first division original source location for each identical operand pair",
 "flags": "not fully proved for emitted x86 machine states",
 "poison_undef": "no LLVM poison admitted by selected typed WebAssembly integers; general poison theorem not shown",
 "target": "x86_64-unknown-linux, pinned Wasmtime, speed_and_size",
}
def encoding(obj):
    return json.dumps(obj,sort_keys=True,separators=(",",":"),ensure_ascii=True).encode()
def digest(data):
    return hashlib.sha256(data).hexdigest()
def blob(data):
    return hashlib.sha1(b"blob "+str(len(data)).encode()+b"\x00"+data).hexdigest()
def need(predicate,reason):
    if not predicate: raise ValueError(reason)
def pinned(root,subdir,name,expected):
    f=root/subdir/name
    need(f.is_file() and not f.is_symlink(),name+" absent/symlinked")
    data=f.read_bytes()
    need(blob(data)==expected,name+" source blob drift")
    return data
def load(root):
    from qualify_udivrem_contexts_28 import CASES
    need(set(WATS).issubset(CASES),"missing WAT contract")
    for name,h in WATS.items():
        need(digest(CASES[name].encode())==h,name+" WAT mismatch")
    checksums={}
    for name,h in SOURCES.items():
        checksums[name]=digest(pinned(root,Path("."),name,h))
    values={}
    for name,h in RECEIPTS.items():
        raw=pinned(root,Path("evidence"),name,h)
        checksums[name]=digest(raw)
        values[name]=json.loads(raw)
    return values,checksums
def entry(name,row,source):
    if source=="historical":
        result={
            "wat_sha256":row["wat_sha256"],
            "native_instruction_bytes":row["decoded_native_bytes"],
            "instruction_count":row["native_instructions"],
            "native_divisions":row["static_unsigned_divisions"],
            "whole_aot_bytes":row["whole_aot_bytes"],
            "aot_sha256":[row["baseline_aot_sha256"],row["patched_aot_sha256"]],
            "receipt":"udivrem_context_stress_20261114.json",
        }
    else:
        a,b=row["baseline"],row["patched"]
        result={
            "wat_sha256":row["wat_sha256"],
            "native_instruction_bytes":[a["decoded_x86_64_native_instruction_bytes"],b["decoded_x86_64_native_instruction_bytes"]],
            "instruction_count":[a["instructions"],b["instructions"]],
            "native_divisions":[a["div_instructions"],b["div_instructions"]],
            "whole_aot_bytes":[a["aot_bytes"],b["aot_bytes"]],
            "aot_sha256":None,
            "receipt":"flag_predicate_contexts_20261116.json",
        }
    result["name"]=name
    need(result["wat_sha256"]==WATS[name],name+" wrong WAT")
    for metric in ("native_instruction_bytes","instruction_count","native_divisions","whole_aot_bytes"):
        values=result[metric]
        need(len(values)==2 and all(type(v)==int and 0<=v<100000 for v in values),name+" invalid metric "+metric)
    result["decoded_native_bytes_saved"]=result["native_instruction_bytes"][0]-result["native_instruction_bytes"][1]
    result["static_divisions_removed"]=result["native_divisions"][0]-result["native_divisions"][1]
    result["whole_aot_bytes_saved"]=result["whole_aot_bytes"][0]-result["whole_aot_bytes"][1]
    need(result["decoded_native_bytes_saved"]>=0 and result["static_divisions_removed"]>=0,name+" regression: do not hide it")
    need(result["whole_aot_bytes_saved"]==0,name+" unsupported AOT saving")
    return result
def build(root,revision):
    vals,sha=load(root)
    old=vals["udivrem_context_stress_20261114.json"]
    recent=vals["flag_predicate_contexts_20261116.json"]
    trap=vals["native_trap_table_qualified_20261115.json"]
    structural=vals["structural_verifier_two_site_20261115.json"]
    symbolic=vals["two_site_symbolic64_qualified_20261115.json"]
    need(old["source"]["wasmtime_compiler_source"]==WASMTIME,"old compiler pin")
    need(recent["wasmtime_source_commit"]==WASMTIME,"new compiler pin")
    need(trap["wasmtime_source_commit"]==WASMTIME,"trap source pin")
    need(structural["isolation"]["wasmtime_source"]==WASMTIME,"structural source pin")
    need(symbolic["source_pins"]["wasmtime"]==WASMTIME,"symbolic source pin")
    need(old["ci"]["source_controls"]=="passed" and old["ci"]["independent_context_archive_auditor"]=="passed","old audit incomplete")
    need(recent["workflow"]["source_controls"]=="success" and recent["workflow"]["identical_wasm_two_compilers"]=="success","new audit incomplete")
    need(trap["workflow"]["source_controls"]=="success" and trap["workflow"]["identical_wasm_two_compilers"]=="success","native trap receipt incomplete")
    need(len(symbolic["all_positive_counterexample_queries_unsat"])==7 and len(symbolic["negative_mutants_sat_witness_found"])==3,"conditional theorem incomplete")
    need(all(v["result"]=="success" for v in structural["structural_verification"]["real_build_test_results"]),"structural verifier incomplete")
    rows=[entry(name,row,"historical") for name,row in old["measured_cases"].items()]
    rows += [entry(name,row,"predicate") for name,row in recent["cases"].items()]
    need(len(rows)==len(WATS) and {r["name"] for r in rows}==set(WATS),"missing or duplicate fixture")
    rows.sort(key=lambda x:x["name"])
    need(len(revision)==40 and all(c in "0123456789abcdef" for c in revision),"bad revision")
    payload={
      "schema":"zenofcis/tier1-udivrem-content-key-catalog/1",
      "classification":"pinned reconciliation of prior native measurements; no new compiler run",
      "source_revision":revision,"wasmtime_revision":WASMTIME,
      "candidate_key":digest(encoding({"base":BASE,"new":NEW,"contract":CONTRACT,"upstream":WASMTIME})),
      "baseline_sequence":BASE,"candidate_sequence":NEW,"contract":CONTRACT,
      "contexts":rows,"pinned_input_sha256":sha,
      "receipt_classes":{
        "finite-native-and-guest-traps":["43609235482","43799948356"],
        "exhaustive-finite-8bit":["43699318875"],
        "SMT-conditional-u64-contract":["43732389013"],
        "CLIF-structural-verifier":["43698912588","43698912579"],
        "encoded-native-trap-PC-check":["43764894051"],
      },
      "limitations":[
       "No original compiler or AOT execution in this catalog replay; only pinned receipt/source integrity and arithmetic recomputation",
       "Conditional SMT and finite tests do not prove all 64-bit compiler states or Rust pass soundness",
       "Machine flags, register allocation, poison/undef, cross-target behavior and throughput remain unqualified",
       "Native decoded instruction bytes are not CLIF cost or serialized AOT size",
       "One human-authored prototype; five contexts are not five independently discovered algorithms",
      ],
    }
    return {"payload":payload,"digest_sha256":digest(encoding(payload))}
def checkout_revision(root):
    """Bind replay to the *actual* checkout, not a caller-supplied 40-hex label."""
    head=subprocess.check_output(["git","rev-parse","HEAD"],cwd=root,text=True).strip()
    need(len(head)==40 and all(c in "0123456789abcdef" for c in head),"invalid checkout revision")
    return head
def replay(root,doc):
    need(set(doc)=={"payload","digest_sha256"},"bad envelope")
    need(isinstance(doc["payload"],dict),"bad catalog payload")
    head=checkout_revision(root)
    need(doc["payload"].get("source_revision")==head,"catalog revision is not checkout HEAD")
    need(doc==build(root,head),"forged/stale catalog")
def test(root=None):
    assert blob(b"test\n")=="9daeafb9864cf43055ae93beb0afd6c7d144bfa4"
    assert digest(encoding({"a":1,"b":2}))==digest(encoding({"b":2,"a":1}))
    if root is not None:
        head=checkout_revision(root)
        ref=build(root,head)
        replay(root,ref)
        defects=[]
        x=copy.deepcopy(ref);x["payload"]["contexts"][0]["decoded_native_bytes_saved"]+=1;defects.append(x)
        x=copy.deepcopy(ref);x["payload"]["contexts"].pop();defects.append(x)
        x=copy.deepcopy(ref);x["payload"]["candidate_sequence"].append("fabricated");defects.append(x)
        x=copy.deepcopy(ref);x["payload"]["pinned_input_sha256"].clear();defects.append(x)
        x=copy.deepcopy(ref);x["payload"]["receipt_classes"]["SMT-conditional-u64-contract"]=["unconditional-machine-proof"];defects.append(x)
        # Crucial negative: a *self-consistent*, freshly rehashed catalog
        # claiming a different source revision must still be rejected.
        other="a"*40 if head!="a"*40 else "b"*40
        defects.append(build(root,other))
        for x in defects:
            try: replay(root,x)
            except ValueError: pass
            else: raise AssertionError("catalog mutation accepted")
        print("pinned source/receipts and six negative catalog mutations (including rehashed fake revision) passed")
    else:
        print("canonicalization and Git blob self-test passed")
def main():
    p=argparse.ArgumentParser()
    p.add_argument("--out",type=Path)
    p.add_argument("--replay",type=Path)
    p.add_argument("--source",type=Path,default=HERE)
    p.add_argument("--revision")
    p.add_argument("--self-test",action="store_true")
    a=p.parse_args()
    if a.self_test: test(a.source);return
    if a.replay:
        replay(a.source,json.loads(a.replay.read_text()))
        print("pinned catalog replay passed");return
    if not a.out or a.out.exists():p.error("new --out required")
    revision=checkout_revision(a.source)
    need(a.revision is None or a.revision==revision,"--revision does not match checkout HEAD")
    doc=build(a.source,revision)
    replay(a.source,doc)
    a.out.parent.mkdir(parents=True,exist_ok=True)
    a.out.write_text(json.dumps(doc,sort_keys=True,indent=2)+"\n")
    print(json.dumps({"cases":len(doc["payload"]["contexts"]),"catalog_key":doc["payload"]["candidate_key"],"receipt_digest":doc["digest_sha256"]},sort_keys=True))
if __name__=="__main__":main()
