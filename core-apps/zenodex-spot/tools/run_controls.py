#!/usr/bin/env python3
"""Small falsification controls, isolated from the application sources."""
from __future__ import annotations

import argparse
import json
import re
import shutil
from pathlib import Path

from build_contracts import APP, source_hash
from native_check import command_values, state_values
from run_judges import PINNED_CLI, invoke
from run_native_checks import seed
from spot_reference import Command


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--lp-project",type=Path,required=True)
    parser.add_argument("--out",type=Path,required=True)
    args=parser.parse_args();args.out.mkdir(parents=True,exist_ok=True)
    baseline=args.out/"without-progress-law"
    baseline.mkdir(exist_ok=True);(baseline/"v2").mkdir(exist_ok=True)
    source=(args.lp_project/"project.zeno").read_text()
    match=re.search(r"^law (\d+) refuses_only_ineligible .*\n",source,re.M)
    if match is None: raise ValueError("progress law missing from final project")
    policy=json.loads((args.lp_project/"v2/policy.json").read_text())
    policy["law_kinds"].pop(match[1]);(baseline/"project.zeno").write_text(source[:match.start()]+source[match.end():])
    (baseline/"v2/policy.json").write_text(json.dumps(policy,indent=2)+"\n")
    record,packet=invoke(PINNED_CLI,["contract","review",baseline,"--gaps","--max-tuples","512","--max-work","2000000"],args.out,"without-progress")
    if record["exit_code"] != 0 or not packet["gaps"]["reject_everything"]["admitted_where_the_contract_decides"]:
        raise ValueError("removing progress did not expose the reject-everything gap")
    record,false_lemma=invoke(PINNED_CLI,["contract","lemma",args.lp_project,"--lemma","post.100.110 <= 1001","--max-tuples","512"],args.out,"false-lemma")
    if record["exit_code"] != 1 or false_lemma["lemmas"][0]["verdict"] != "false": raise ValueError("false-lemma control was not refuted")
    bad=args.out/"bad-quote";bad.mkdir(exist_ok=True);(bad/"v2").mkdir(exist_ok=True)
    shutil.copyfile(APP/"project.zeno",bad/"project.zeno")
    policy=json.loads((APP/"v2/policy.json").read_text())
    policy["variables"]["quote"]=f"({policy['variables']['quote']}) + 1"
    (bad/"v2/policy.json").write_text(json.dumps(policy,indent=2)+"\n")
    values=state_values(seed())+command_values(Command(1,a=10000))+(1,)
    record,bad_packet=invoke(PINNED_CLI,["contract","probe","explain",bad,"--input"," ".join(map(str,values))],args.out,"bad-quote")
    authority=bad_packet["result"]["authority"] if bad_packet is not None else {}
    if record["exit_code"] != 0 or authority.get("class") != "Refused" or authority.get("law") in (None,990,991,909):
        raise ValueError(f"bad quote was not caught by a declared arithmetic law: {authority}")
    result={"authority":"none","without_progress":packet["gaps"]["admissible"]["full"],
            "reject_everything_without_progress":True,"false_lemma":false_lemma["lemmas"][0]["verdict"],
            "bad_quote":authority,"application_sources":source_hash(APP)}
    (args.out/"result.json").write_text(json.dumps(result,indent=2)+"\n")
    print(json.dumps(result))


if __name__ == "__main__": main()
