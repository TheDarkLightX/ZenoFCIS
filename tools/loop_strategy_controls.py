#!/usr/bin/env python3
"""Deferred native strategy controls; no product or publication authority.

Uses fixed finite fixtures, existing CLI judges and independent Python tables.
No research study or generated held-out generalization claim is made.
"""
import argparse
from copy import deepcopy
from contextlib import nullcontext
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import loop_strategy_reference as R


def classifies(code, report, status):
    wanted = 2 if status == "inconclusive" else 1 if status in ("refused","refuted") else 0
    return code == wanted and report.get("status") == status


def self_test(stub=False):
    classify = (lambda *args: True) if stub else classifies
    failures = []
    for name, code, status, expected in [("qualified-positive",0,"qualified",True),
        ("refutation-positive",1,"refuted",True),("refusal-not-refutation",1,"refused",False),
        ("stop-not-refutation",2,"inconclusive",False),("success-not-refutation",0,"qualified",False)]:
        target = status if expected else "refuted"
        if classify(code,{"status":status},target) != expected: failures.append(name)
    old = R.compare
    try:
        if stub: R.compare = lambda a,b: True
        failures += R.self_test()
    finally:
        R.compare = old
    return failures


def campaign(cli, work):
    work.mkdir(parents=True,exist_ok=False)
    def write(name, value):
        path = work / (name+".json")
        with path.open("x",encoding="ascii") as stream: json.dump(value,stream,sort_keys=True)
        return path
    def run(*arguments):
        done = subprocess.run([str(cli),*map(str,arguments)],capture_output=True,timeout=120)
        if len(done.stdout) > 8 << 20: raise ValueError("control-output-bound")
        return done.returncode,json.loads(done.stdout)
    domain={"kind":"Int","min":"0","max":"3"};out={"kind":"Int","min":str(R.C.R.I64_MIN),"max":str(R.C.R.I64_MAX)}
    original={"inputs":[domain],"outputs":[out],"nodes":[["Input",0],["Int","2"],["Lt",0,1],["Int","7"],["Select",2,3,0]],"roots":[4]}
    constant={**original,"nodes":[["Int","7"]],"roots":[0]};identity={**original,"nodes":[["Input",0]],"roots":[0]}
    pred={**original,"outputs":[{"kind":"Bool"}],"nodes":original["nodes"][:3],"roots":[2]}
    original_bytes=R.C.encoding(R.C.program(original));subject=work/"reference.zcve";subject.write_bytes(original_bytes)
    cases={"schema":"zeno-fcis/loop-strategy-problem/1","learn":{"schema":"zeno-fcis/learn-terms/1","original_sha256":hashlib.sha256(original_bytes).hexdigest(),"status":"complete","integers":[constant,identity],"predicates":[pred]},"config":{"initial":2,"spot_checks":1}}
    diagnostic={"schema":"zeno-fcis/loop-strategy-problem/1","circuit":{"inputs":1,"gates":[{"kind":"not","wires":[0]},{"kind":"not","wires":[1]}],"outputs":[2]},"vector":[0],"observations":[[2,1]],"max_faults":2}
    yes={"inputs":[{"kind":"Bool"}],"outputs":[{"kind":"Bool"}],"nodes":[["Bool",True]],"roots":[0]}
    x={**yes,"nodes":[["Input",0]]};equal={**yes,"nodes":[["Input",0],["Eq",0,0]],"roots":[1]};notx={**yes,"nodes":[["Input",0],["Not",0]],"roots":[1]}
    readings={"schema":"zeno-fcis/loop-strategy-problem/1","background":yes,"query":"count","readings":[{"id":"s1","alternatives":[yes,equal,x]}]}
    rows=[]
    def trial(name, kind, problem, status, extra=(), reference=True, reference_path=subject):
        path=write(name,problem);argv=["loop","strategy","--kind",kind,"--problem",path]
        if kind=="split-by-cases":argv += ["--original",reference_path]
        output=work/(name+"-export");argv += ["--out",output,*extra]
        code,report=run(*argv);detail=report.get("detail",{})
        failures=R.validate(problem,detail) if reference and "plan" in detail else []
        passed=classifies(code,report,status) and not failures
        if status=="inconclusive":passed=passed and detail.get("attempts_spent")==0 and not (output/"candidate.zcve").exists()
        if status=="refuted":passed=passed and detail.get("best_check",{}).get("verdict")=="counterexample" and not (output/"candidate.zcve").exists()
        if status=="qualified":
            receipt=output/"independent-receipt.json"
            check_code,check=run("transform","check","--original",reference_path,"--candidate",output/"candidate.zcve","--receipt",receipt)
            replay_code,replay=run("transform","replay","--original",reference_path,"--candidate",output/"candidate.zcve","--receipt",receipt)
            passed=passed and check_code==replay_code==0 and hashlib.sha256(receipt.read_bytes()).hexdigest()==detail["best_check"]["receipt_sha256"]
        rows.append({"name":name,"applied":True,"passed":bool(passed),"exit":code,"problem":problem,"report":report,"reference":failures})
        return detail
    case_result=trial("cases-checked","split-by-cases",cases,"qualified")
    rare_program=deepcopy(original);rare_program["inputs"][0]["max"]="7"
    rare_program["nodes"]=[["Input",0],["Int","4"],["Lt",0,1],["Int","2"],["Eq",0,3],["Int","9"],["Int","5"],["Select",4,5,6],["Int","7"],["Select",2,7,8]];rare_program["roots"]=[9]
    rare_bytes=R.C.encoding(R.C.program(rare_program));rare_path=work/"rare-reference.zcve";rare_path.write_bytes(rare_bytes)
    rare=deepcopy(cases);rare["learn"]["original_sha256"]=hashlib.sha256(rare_bytes).hexdigest()
    rare["learn"]["integers"]=[{**rare_program,"nodes":[["Int",str(v)]],"roots":[0]} for v in (5,7,9)]
    rare["learn"]["predicates"]=[{**rare_program,"outputs":[{"kind":"Bool"}],"nodes":[["Input",0],["Int",str(v)],[op,0,1]],"roots":[2]} for v,op in ((4,"Lt"),(2,"Eq"))]
    trial("cases-region-exception","split-by-cases",rare,"qualified",reference_path=rare_path)
    sampled=deepcopy(cases);sampled["config"]["initial"]=1
    trial("sample-fit-refuted","split-by-cases",sampled,"refuted")
    incomplete=deepcopy(cases);incomplete["learn"]={"schema":"zeno-fcis/learn-terms/1","original_sha256":cases["learn"]["original_sha256"],"status":"inconclusive"}
    trial("learn-stop","split-by-cases",incomplete,"inconclusive")
    query_stop=deepcopy(cases);query_stop["config"]["queries"]=0
    trial("query-stop","split-by-cases",query_stop,"inconclusive")
    stale=deepcopy(cases);stale["learn"]["original_sha256"]="0"*64
    trial("learn-binding-refused","split-by-cases",stale,"refused")
    diagnosis_result=trial("diagnosis-probe","diagnose",diagnostic,"question")
    measured=deepcopy(diagnostic);measured["observations"].append([1,0])
    trial("diagnosis-measured","diagnose",measured,"proposed-diagnosis")
    reading_result=trial("readings-question","ask-when-it-matters",readings,"question")
    stable=deepcopy(readings);stable["readings"][0]["alternatives"]=[x,notx]
    trial("readings-stable","ask-when-it-matters",stable,"proposed-answer")
    owner=deepcopy(readings);owner["owner_answers"]=[]
    trial("owner-field-refused","ask-when-it-matters",owner,"refused")
    trial("reading-stop","ask-when-it-matters",readings,"inconclusive",("--max-search-work","0"))
    for name,problem,report,field,value in [
        ("trace-fault",cases,case_result,"trace",["observe"]),
        ("diagnosis-fault",diagnostic,diagnosis_result,"diagnoses",[]),
        ("readings-fault",readings,reading_result,"answers",[])]:
        faulty=deepcopy(report);faulty[field]=value;failures=R.validate(problem,faulty)
        rows.append({"name":name,"applied":True,"passed":bool(failures),"reference":failures})
    return {"schema":"zeno-fcis/strategy-controls/1","authority":"none","rows":rows,"passed":len(rows)==15 and all(row["passed"] for row in rows)}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test",action="store_true");parser.add_argument("--stub-comparison",action="store_true")
    parser.add_argument("--cli",type=Path);parser.add_argument("--work",type=Path);parser.add_argument("--out",type=Path)
    parser.add_argument("--scratch",action="store_true")
    args=parser.parse_args()
    if args.self_test:
        failures=self_test(args.stub_comparison);print(json.dumps({"failures":failures}));return int(bool(failures))
    if not args.cli or args.stub_comparison or (args.scratch and (args.work or args.out)) or (not args.scratch and not all((args.work,args.out))):parser.error("--cli and either --scratch or --work --out required; stubs are self-test only")
    with (tempfile.TemporaryDirectory(prefix="fcis-strategy-controls-") if args.scratch else nullcontext(None)) as scratch:
        work=Path(scratch)/"controls" if scratch else args.work
        output=Path(scratch)/"results.json" if scratch else args.out
        report=campaign(args.cli.resolve(),work)
        with output.open("x",encoding="ascii") as stream:json.dump(report,stream,indent=2,sort_keys=True)
    print(json.dumps({"passed":report["passed"],"rows":len(report["rows"])}));return int(not report["passed"])

if __name__=="__main__":raise SystemExit(main())
