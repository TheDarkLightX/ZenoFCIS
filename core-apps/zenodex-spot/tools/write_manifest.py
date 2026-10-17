#!/usr/bin/env python3
"""Freeze local source identities after authoring checks; publish nothing."""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path

from build_contracts import APP, source_hash
from build_composition import KEYS
from replay_corpus import INTEGRATION_CLI

SPEC=Path("/home/trevormoc/Downloads/ZenoFCIS-evidence-archive/v231-crucible-port/post-reset/spec")


def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--spec",type=Path,default=SPEC)
    parser.add_argument("--cli",type=Path,default=INTEGRATION_CLI)
    args=parser.parse_args()
    source=(APP/"project.zeno").read_text()
    state_fields=[int(n) for n in re.findall(r"^field (\d+) 100 ",source,re.M)]
    composition={"schema":"zeno-fcis/core-application-composition/1","application":"zenodex-spot",
        "profile":"fake-ledger-two-users-one-pair/1","authority":"none","owner_adoption":False,
        "contract_owner":"Spot owner-approved fixed policy","composition_owner":"ZenoFCIS acceptance-example lead",
        "evidence_status":"source authored; bounded native checks; mount and full D4/R6 acceptance pending",
        "state_fields":state_fields,"stateless_components":[],
        "physical_component":400,"complete_contract":source_hash(APP),
        "components":[
            {"name":"pool","family":"spot-pool/4","owns_fields":[110,111,112,113,114,150,151,152],
             "source_files":source_hash(APP/"components/pool"),"local_evidence":"bounded judges and complete-root native probes",
             "views":{"state":["reserves","supply","existence","status"],
                      "command":["action","direction","amounts","limits","sqrt"],
                      "context":["snapshot mirror equations","actor/recipient roles","actor LP balance","closed shape"]}},
            {"name":"balance","family":"spot-u128-balance/1","owns_fields":list(range(2000,2032)),
             "source_files":source_hash(APP/"components/balance"),"local_evidence":"bounded ingress judge and native u128 carry/borrow probes",
             "views":{"state":["one complete-key balance in four little-endian u32 limbs"],
                      "command":["aggregated command-derived debit","aggregated command-derived credit"],
                      "context":["reserved ingress role","exact expected protocol credit"]}}],
        "occurrences":[{"name":name,"ordered_steps":["Pool computes exact plan","Balance checks each canonical complete key",
                         "per-asset and LP conservation","pool/Balance and lock equations","one complete candidate"]}
                       for name in ("CreatePool","SwapExactIn","SwapExactOut","AddLiquidity","RemoveLiquidity")],
        "key_order":list(KEYS),"key_identity":"fixed ledger spot-test, ordered assets X<Y; account path before asset; full key includes ledger",
        "account_slots":{"0":"user:alice","1":"user:bob","2":"pool:X:Y","3":"lock:X:Y",
                         "4":"protocol:fee-ingress:X","5":"protocol:fee-ingress:Y"},
        "balance_roles":"Ingress is authoritative custody outside LP reserves; no ingress debit command exists.",
        "effects":{"channels":{"300":"X transfer","301":"Y transfer","302":"Pool-authorized signed LP delta",
                              "303":"read-only decision output, never a transfer"},
                   "economic_payload":["source slot","destination slot","quantity"],
                   "zero_transfers":"omitted","destination":"zenoledger; output observations use decision",
                   "max_economic_per_transition":4},
        "output_positions":{"CreatePool":["mint","locked",0,0,0,0,0,0],
                            "SwapExactIn":["A","o","F","P","retained","net",0,0],
                            "SwapExactOut":["A","w","F","P","retained","net","q","gap_bps"],
                            "AddLiquidity":["used0","used1","mint","unspent0","unspent1",0,0,0],
                            "RemoveLiquidity":["paid0","paid1",0,0,0,0,0,0]},
        "assumptions":["closed two-user one-pair fake-value profile; no multi-pool runtime claim",
                       "identity/authentication and raw-byte admission are supplied by a future mounting adapter",
                       "the integrated contract derives component views; callers cannot provide effect plans or role flags",
                       "contract compiler, CLI, library Authority, law evaluator and scalar schema are named trusted boundaries"],
        "publication":"one complete native Authority decision; ZenoLedger atomic commit/recovery/retry adapter not implemented",
        "activation":None,"rollback":"no deployment or activation occurred; future coordinated activation is a lead gate"}
    (APP/"composition.json").write_text(json.dumps(composition,indent=2)+"\n")
    files={str(p.relative_to(APP)):sha(p) for p in sorted(APP.rglob('*')) if p.is_file()
           and "__pycache__" not in p.parts and p.name not in ("SOURCE_PINS.json","REPORT.md","PROGRESS.md")}
    manifest={"schema":"spot-source-pins/1","authority":"none","starting_head":"fda32144231bd5f8adbe201eff3a958caa2caaf6",
              "starting_source_manifest_sha256":"bdca784516973287a59731112d2b70b0215e3884a06f1133c90d232432626b87",
              "cli":{"path":str(args.cli),"sha256":sha(args.cli),
                     "observed_version":subprocess.check_output([str(args.cli),"--version"],text=True).strip()},
              "specification":{name:sha(args.spec/name) for name in ("SPOT_CORE_SPEC.md","OWNER_ANSWERS.md","PRIOR_OWNER_ANSWERS.md")},
              "files":files,"qualification":"bounded authoring evidence; not full-range proof or release acceptance"}
    (APP/"SOURCE_PINS.json").write_text(json.dumps(manifest,indent=2)+"\n")
    print(json.dumps({"frozen_files":len(files),"manifest_sha256":sha(APP/"SOURCE_PINS.json")}))


if __name__ == "__main__": main()
