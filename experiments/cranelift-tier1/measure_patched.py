"""Cost-only A/B of one identical CLIF body under pinned unpatched/patched codegen."""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
from measure_ab import profile
from install_isle_candidate import RULE, RULE_NAME, UPSTREAM

def run(base: Path, patched: Path, install_receipt: Path, proof_receipt: Path, out: Path) -> dict:
    if out.exists() or base.resolve()==patched.resolve():
        raise ValueError("fresh output and distinct object files required")
    patch=json.loads(install_receipt.read_text())
    proof=json.loads(proof_receipt.read_text())
    if patch.get("status")!="source-pinned-patch-applied" or proof.get("status")!="unsat-counterexample-query":
        raise ValueError("source-pinned patch and independent bitvector proof required")
    digest=hashlib.sha256(RULE.encode()).hexdigest()
    if (patch.get("rule_sha256")!=digest or proof.get("rule_sha256")!=digest or
            patch.get("rule_name")!=RULE_NAME or proof.get("rule")!=RULE_NAME or
            patch.get("upstream_commit")!=UPSTREAM or proof.get("upstream_commit")!=UPSTREAM):
        raise ValueError("rule, source revision or independent proof identity changed")
    old=profile(base)
    new=profile(patched)
    delta=old["function"]["bytes"]-new["function"]["bytes"]
    data={"schema":"zeno-fcis/cranelift-tier1-patched-compiler-cost/1",
          "status":"measured-patched-compiler-a-b",
          "scope":"same CLIF source, pinned Cranelift before/after experimental ISLE rule",
          "upstream_commit":UPSTREAM,"rule_name":RULE_NAME,
          "rule_sha256":digest,
          "patch_receipt_sha256":hashlib.sha256(install_receipt.read_bytes()).hexdigest(),
          "qf_bv_proof_sha256":hashlib.sha256(proof_receipt.read_bytes()).hexdigest(),
          "baseline":{k:v for k,v in old.items() if k!="disassembly"},
          "patched":{k:v for k,v in new.items() if k!="disassembly"},
          "whole_symbol_delta_old_minus_patched":delta,
          "candidate_smaller":delta>0,
          "nonclaims":["not a VeriISLE chain proof", "not an upstream PR",
                       "target-specific and may regress other architectures",
                       "not whole ISA, ABI or clock-based throughput proof"]}
    out.parent.mkdir(parents=True,exist_ok=True)
    out.write_text(json.dumps(data,sort_keys=True,indent=2)+"\n")
    (out.parent/"unpatched-objdump.txt").write_text(old["disassembly"])
    (out.parent/"patched-objdump.txt").write_text(new["disassembly"])
    return data

def main():
    p=argparse.ArgumentParser()
    for name in ("baseline","patched","patch-receipt","proof-receipt","out"):
        p.add_argument("--"+name,type=Path,required=True)
    a=p.parse_args()
    result=run(a.baseline,a.patched,a.patch_receipt,a.proof_receipt,a.out)
    print(json.dumps({"status":result["status"],"baseline":result["baseline"]["function"]["bytes"],
                      "patched":result["patched"]["function"]["bytes"],
                      "delta":result["whole_symbol_delta_old_minus_patched"]},sort_keys=True))
if __name__=="__main__":
    main()
