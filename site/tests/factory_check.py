#!/usr/bin/env python3
"""Build the native oracle and compare the published Wasm against it and the CLI."""
import json
import os
import re
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
SITE = ROOT / "site"
sys.path.insert(0, str(SITE))
import build


def native_results(environment):
    build.run(["cargo", "+1.97.1", "build", "--release", "--locked", "--offline", "-p", "zeno-fcis-site-factory", "--bin", "zeno-fcis-site-factory"], SITE, environment)
    ids = ["none", "m0002", "m0018", "m0040", "m0054", "m0061", "m0049"]
    requests = [{"act": "candidate", "number": n} for n in range(8)] + [{"act": "synthesis"}] + [{"act": "inventory", "mutant": id} for id in ids]
    requests.extend([{"act": "search"}, {"act": "money"}])
    executable = build.target_dir(environment) / "release" / "zeno-fcis-site-factory"
    reply = subprocess.run([str(executable)], input="\n".join(map(json.dumps, requests))+"\n", text=True, capture_output=True, check=True)
    results = [json.loads(line) for line in reply.stdout.splitlines()]
    expected = {"synthesis": {**results[8], "candidates": results[:8]}, **{f"inventory/{id}": result for id,result in zip(ids,results[9:9+len(ids)], strict=True)}, "search": results[-2], "money": results[-1]}
    row_requests = [{"act":"inventory_rows","mutant":id,"start":start,"count":48} for id in ids for start in range(0,864,48)]
    rows_reply = subprocess.run([str(executable)],input="\n".join(map(json.dumps,row_requests))+"\n",text=True,capture_output=True,check=True)
    chunks = [json.loads(line) for line in rows_reply.stdout.splitlines()]
    detail_requests = []
    for index,id in enumerate(ids):
        result = expected[f"inventory/{id}"]
        cells = [cell for chunk in chunks[index*18:(index+1)*18] for cell in chunk["cells"]]
        first = next((cell["ordinal"] for cell in cells if result["first_difference"] and cell["input"] == result["first_difference"]["input"]),None)
        result["grid"] = {"mutant":id,"cells":cells,"first_ordinal":first}
        assert len(cells) == 864 and sum(cell["different"] for cell in cells) == result["differences"]
        assert {name:sum(cell["class"]==name for cell in cells) for name in result["classes"]} == result["classes"]
        start_cell = cells[first or 0]["input"]
        available,reserved,action,quantity,authorized = start_cell
        column = min(47,(action-150)*12+authorized*6+available+1)
        next_ordinal = ((((column%6)*6+reserved)*4+column//12)*3+quantity-1)*2+(column%12)//6
        for ordinal in sorted({0,143,435,495,863,first or 0,next_ordinal}):
            detail_requests.append({"act":"inventory_detail","mutant":id,"ordinal":ordinal})
    detail_reply = subprocess.run([str(executable)],input="\n".join(map(json.dumps,detail_requests))+"\n",text=True,capture_output=True,check=True)
    details = {f"{request['mutant']}/{request['ordinal']}":json.loads(reply) for request,reply in zip(detail_requests,detail_reply.stdout.splitlines(),strict=True)}
    (SITE / "tests/harness/inventory-details-native.json").write_text(json.dumps(details,sort_keys=True)+"\n")
    return expected


def money_vectors(environment):
    pilot = (ROOT / "verification/arithmetic/graphs.rs").read_text()
    browser = (SITE / "factory/src/money.rs").read_text()
    graph = lambda source, name: re.sub(r"\s+", "", re.search(rf"const {name}:.*?= (\[.*?\]);", source, re.S).group(1))
    assert graph(pilot,"SWAP_NODES") == graph(browser,"NODES"), "money graph differs from pilot"
    assert graph(pilot,"SWAP_ROOTS") == graph(browser,"ROOTS"), "money roots differ from pilot"
    requests = [{"act":"money", **dict(zip(("amount","fee_bps","reserve_in","reserve_out"),map(str,values), strict=True))}
                for values in [(0,0,1,1),(1,1,1,3),(1000,30,10000,20000),(3000000000,10000,3000000000,3000000000),(3000000000,9999,1,3000000000),(1,10000,1,1)]]
    executable = build.target_dir(environment) / "release/zeno-fcis-site-factory"
    result = subprocess.run([str(executable)], input="\n".join(map(json.dumps,requests))+"\n",text=True,capture_output=True,check=True)
    vectors=[]
    for request, reply in zip(requests,result.stdout.splitlines(),strict=True):
        actual=json.loads(reply)
        amount,rate,x,y = [int(request[name]) for name in ("amount","fee_bps","reserve_in","reserve_out")]
        fee=(amount*rate+9999)//10000
        net=amount-fee
        out=y*net//(x+net)
        expected=[str(v) for v in (fee,net,out,x+net,y-out)] + [(x+net)*(y-out)>=x*y]
        assert actual["outputs"] == expected and actual["steps"] == 12, "independent big integer swap reference"
        assert actual["overflow"]["steps"] == 9, "overflow refusal preserves attempted work"
        vectors.append({"request":request,"result":actual})
    (SITE / "tests/money-native.json").write_text(json.dumps(vectors,sort_keys=True)+"\n")


def check_cli(environment, results):
    executable = build.target_dir(environment) / "debug" / "zeno-fcis"
    with tempfile.TemporaryDirectory(prefix="factory-native-") as directory:
        output = Path(directory) / "synthesis"
        subprocess.run([str(executable), "synth", "run", str(SITE / "apps/agent-treasury-guard/synthesis.json"), "--out", str(output)], cwd=ROOT, check=True, capture_output=True)
        manifest = json.loads((output / "manifest.json").read_text())
    with tempfile.TemporaryDirectory(prefix="factory-review-") as directory:
        packet = Path(directory) / "review.json"
        subprocess.run([str(executable), "contract", "review", str(SITE / "apps/inventory-reservation"), "--no-kb", "--out", str(packet)], cwd=ROOT, check=True, capture_output=True)
        review = json.loads(packet.read_text())
    catalog = {m["id"]: m for m in review["mutants"]["list"]}
    for key,result in results.items():
        if not key.startswith("inventory/") or result["mutant"] == "none":
            continue
        native = catalog[result["mutant"]]
        witness = result["first_difference"]
        if native["classification"] == "equivalent-over-full-domain":
            assert witness is None and result["differences"] == 0, "native_mutant_first_difference"
        else:
            assert witness["input"] == list(map(int,native["witness"]["input"])), "native_mutant_first_difference"
            for side in ("original", "mutant"):
                actual, expected = witness[side], native["witness"][side]
                if "refused" in expected:
                    assert actual["refusal"] == expected["refused"], "native mutant refusal"
                    continue
                for field in ("class", "reason", "post", "outbox"):
                    if field in expected:
                        assert actual.get(field) == expected[field], f"native mutant decision: {field}"
    synthesis = results["synthesis"]
    assert synthesis["first_counterexample"] == manifest["first_counterexample"], "native_synthesis_counterexample"
    assert synthesis["assignments"] == manifest["assignments_evaluated"] == 8
    assert synthesis["passed"] == manifest["input_coverage"] == 6144
    for result in results.values():
        if result.get("act") == "inventory":
            assert result["names"]["fields"]["110"] == "available" and result["names"]["variants"]["152"] == "Ship"


def main():
    environment = dict(os.environ)
    (SITE / "tests/gate-native.json").write_text(json.dumps(build.gate.EXAMPLE_TEMPLATES, sort_keys=True)+"\n")
    results = native_results(environment)
    check_cli(environment, results)
    money_vectors(environment)
    (SITE / "tests/harness/factory-native.json").write_text(json.dumps(results, sort_keys=True)+"\n")
    build.run(["node", str(SITE / "tests/factory.mjs")], ROOT, environment)
    build.run(["node", str(SITE / "tests/factory-worker.mjs")], ROOT, environment)
    build.run(["node", str(SITE / "tests/factory-presentation.mjs")], ROOT, environment)
    print("factory native: PASS: complete results match Wasm and pinned CLI synthesis witness")


if __name__ == "__main__":
    main()
