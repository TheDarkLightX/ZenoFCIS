#!/usr/bin/env python3
"""Independent finite strategy reference and structural comparison controls.

Standard library only. This is test evidence, never imported by product code.
"""
import argparse
from collections import Counter
from itertools import combinations, product
import json
from pathlib import Path
import loop_composable_reference as C

SIGNATURES = {
    "check-program": (("Original", "Proposal"), ("Qualified",)),
    **{name: (("Original", "Proposal"), ("Proposal",)) for name in ("complete", "repair", "canonical", "resubstitute")},
    "sample": (("Original",), ("Examples",)), "learn-terms": (("Examples", "Vocabulary"), ("Matches",)),
    "cover": (("Matches",), ("Cover",)), "learn-split": (("Cover", "Examples", "Vocabulary"), ("Tree",)),
    "assemble": (("Tree",), ("Proposal", "Committee")), "committee-question": (("Committee", "Original"), ("Examples",)),
    "spot-check": (("Proposal", "Original"), ("Examples", "Agreement")), "observe": (("Model",), ("Observations",)),
    "explain": (("Model", "Observations"), ("Diagnoses",)), "choose-probe": (("Model", "Diagnoses"), ("Question",)),
    "measure": (("Question", "Model"), ("Observations",)), "isolate": (("Diagnoses",), ("Answer",)),
    "formalize-alternatives": (("Proposal",), ("Readings",)), "distinct-readings": (("Readings", "Model"), ("Readings",)),
    "solve-every-reading": (("Readings", "Model"), ("Answers",)), "confirm-reading": (("Answers", "Readings", "OwnerGate"), ("Readings",)),
    "decide": (("Answers",), ("Answer",)),
    "capture": (("Original", "Proposal"), ("Proposal",)),
}


def compare(actual, expected):
    """Shared seam; every checker has a planted-negative stub control."""
    return actual == expected


def check_plan(plan):
    problems = []
    def walk(node, have):
        if "step" in node:
            need, out = SIGNATURES[node["step"]]
            if not compare(set(need) <= have, True):
                problems.append("step-input-missing")
                return [have]
            return [have | set(out)]
        if "seq" in node:
            states = [have]
            for step in node["seq"]:
                states = [after for state in states for after in walk(step, state)]
            return states
        if "branch" in node:
            branch = node["branch"]
            if not compare(branch["on"] in have, True): problems.append("branch-input-missing")
            if not branch["cases"]: problems.append("empty-branch")
            return [after for case in branch["cases"] for after in walk(case, have)]
        repeat = node["repeat"]
        if not 1 <= repeat["bound"] <= 64: raise ValueError("repeat-bound")
        states = walk(repeat["body"], have)
        if not compare(any(repeat["until"] in state for state in states), True): problems.append("repeat-goal-missing")
        return states
    ends = walk(plan["node"], set(plan["needs"]))
    if not compare(any(set(plan["goal"]) <= state for state in ends), True): problems.append("goal-unreachable")
    return problems


def conforms(plan, trace):
    def match(node, index):
        if index == len(trace): return {index}
        if "step" in node: return {index + 1} if compare(trace[index], node["step"]) else set()
        if "seq" in node:
            positions = {index}
            for step in node["seq"]: positions = {end for pos in positions for end in match(step, pos)}
            return positions
        if "branch" in node: return {end for case in node["branch"]["cases"] for end in match(case, index)}
        repeat = node["repeat"]
        positions, seen = {index}, {index}
        for _ in range(repeat["bound"]):
            positions = {end for pos in positions for end in match(repeat["body"], pos)} - seen
            seen |= positions
            if not positions: break
        return seen
    return len(trace) in match(plan["node"], 0)


def values(program):
    parsed = C.program(program)
    table = C.R.table(parsed)
    if any(row[0] != "ok" for row in table): raise ValueError("formula-fails")
    return [bool(row[1][0]) for row in table]


def reading_answers(problem):
    background = values(problem["background"])
    distinct, tables = [], []
    for group in problem["readings"]:
        seen, kept = [], []
        for index, formula in enumerate(group["alternatives"]):
            mask = tuple(a and b for a, b in zip(values(formula), background))
            if mask not in seen: seen.append(mask); kept.append(index)
        distinct.append(kept); tables.append(seen)
    query, answers = problem["query"], []
    for combo in product(*(range(len(row)) for row in tables)):
        statements = len(tables) if query == "count" else len(tables) - 1
        models = [background[j] and all(tables[i][combo[i]][j] for i in range(statements)) for j in range(len(background))]
        if query == "count": answer = sum(models)
        elif query == "must": answer = int(any(models) and all(not m or tables[-1][combo[-1]][j] for j, m in enumerate(models)))
        else: answer = int(any(m and tables[-1][combo[-1]][j] for j, m in enumerate(models)))
        answers.append([list(combo), answer])
    question = None
    if len({answer for _, answer in answers}) > 1:
        dependence = []
        for i, row in enumerate(tables):
            spread = len({frozenset(answer for combo, answer in answers if combo[i] == k) for k in range(len(row))})
            if spread > 1: dependence.append((-spread, i))
        if dependence:
            _, index = min(dependence)
            a, b = next((a,b) for a,b in combinations(range(len(tables[index])),2) if tables[index][a] != tables[index][b])
            ordinal = next(j for j,(a,b) in enumerate(zip(tables[index][a],tables[index][b])) if a != b)
            worlds = list(C.R.tuples(C.program(problem["background"])))
            question = {"statement": index, "id": problem["readings"][index]["id"], "text": problem["readings"][index].get("text"),
                        "ordinal": ordinal, "world": list(worlds[ordinal]),
                        "holds": [distinct[index][k] for k,row in enumerate(tables[index]) if row[ordinal]],
                        "does_not_hold": [distinct[index][k] for k,row in enumerate(tables[index]) if not row[ordinal]]}
    return distinct, answers, question


def diagnostic(problem):
    circuit, vector = problem["circuit"], problem["vector"]
    observed = dict(problem["observations"])
    def run(forced):
        row = list(vector)
        for index, gate in enumerate(circuit["gates"]):
            args = [row[wire] for wire in gate["wires"]]
            a, b = args[0], args[-1]
            value = {"not": int(not a), "and": a & b, "or": a | b, "xor": a ^ b,
                     "nand": int(not (a & b)), "nor": int(not (a | b))}[gate["kind"]]
            row.append(forced.get(index, value))
        return row
    diagnoses = []
    for size in range(problem.get("max_faults", 2) + 1):
        for faults in combinations(range(len(circuit["gates"])), size):
            if any(set(d["gates"]) <= set(faults) for d in diagnoses): continue
            worlds = [run(dict(zip(faults, bits))) for bits in product((0,1),repeat=size)]
            worlds = [row for row in worlds if all(row[wire] == value for wire,value in observed.items())]
            if worlds: diagnoses.append({"gates": list(faults), "worlds": worlds})
    best = None
    if len(diagnoses) > 1:
        for wire in range(circuit["inputs"],circuit["inputs"]+len(circuit["gates"])):
            if wire in circuit["outputs"] or wire in observed: continue
            zero = sum(any(row[wire] == 0 for row in d["worlds"]) for d in diagnoses)
            one = sum(any(row[wire] == 1 for row in d["worlds"]) for d in diagnoses)
            key = max(zero,one), zero*zero+one*one, wire
            if best is None or key < best[0]: best = key, {"wire":wire,"zero":zero,"one":one}
    return diagnoses, best[1] if best and best[0][0] < len(diagnoses) else None


def greedy_cover(rows, examples):
    unseen, out = set(examples), []
    while unseen:
        score = [(sum(row[j] for j in unseen), -i) for i,row in enumerate(rows)]
        if not score or max(score)[0] == 0: return None
        _, index = max(score); index = -index
        out.append(index); unseen -= {j for j in unseen if rows[index][j]}
    return out


def minority_cells(regions, predicates):
    cells = []
    for region in regions:
        for pred in predicates:
            yes = [a and b for a,b in zip(region,pred)]
            no = [a and not b for a,b in zip(region,pred)]
            side = yes if sum(yes) <= sum(no) else no
            if any(side): cells.append(side)
    return cells


def case_search(problem, original):
    """Independent deterministic cover/tree/committee/region replay.

    Only the queried columns affect learning. Full tables are test oracles for
    checking reported proposals, not a product qualification route.
    """
    config = {"initial":8,"depth":2,"committee":16,"alternatives":6,
              "leaf_alternatives":8,"spot_checks":8,"queries":64, **problem.get("config",{})}
    def table(p):
        rows = C.R.table(C.program(p))
        if any(row[0] != "ok" for row in rows): raise ValueError("term-fails")
        return tuple(row[1][0] for row in rows)
    truth = table(original)
    terms = []
    for p in sorted(problem["learn"]["integers"],key=lambda p:len(p["nodes"])):
        row = table(p)
        if row not in terms: terms.append(row)
    predicates, sizes = [], []
    for p in problem["learn"]["predicates"]:
        row = tuple(bool(v) for v in table(p))
        if any(row) and not all(row) and row not in predicates:
            predicates.append(row);sizes.append(len(p["nodes"]))
    count = len(truth)
    queried = {i*count//min(config["initial"],count) for i in range(min(config["initial"],count))}
    committee_queries, spot_queries, trace = [], [], ["sample"]
    streak = 0
    def trees(matches, columns, depth):
        explains = [i for i,row in enumerate(matches) if all(row[j] for j in columns)]
        if explains or depth == 0: return explains[:config["committee"]]
        chosen = greedy_cover(matches,columns)
        if chosen is None: return []
        order = []
        for i,pred in enumerate(predicates):
            left = [j for j in columns if pred[j]];right = [j for j in columns if not pred[j]]
            if not left or not right: continue
            score = sum(max(sum(matches[t][j] for j in side) for t in chosen) for side in (left,right))
            order.append((-score,sizes[i],i,left,right))
        out = []
        for _,_,i,left,right in sorted(order)[:config["alternatives"]]:
            ls,rs = trees(matches,left,depth-1),trees(matches,right,depth-1)
            if ls and rs:
                out.extend((i,l,rs[0]) for l in ls[:2])
                out.extend((i,ls[0],r) for r in rs[1:2])
            if len(out) >= config["committee"]: break
        return out[:config["committee"]]
    def variants(tree,matches,columns):
        if type(tree) is int:
            explains = [i for i,row in enumerate(matches) if all(row[j] for j in columns)]
            return [i for i in explains[:config["leaf_alternatives"]+1] if i != tree]
        i,left,right = tree
        a = [j for j in columns if predicates[i][j]];b = [j for j in columns if not predicates[i][j]]
        return [(i,v,right) for v in variants(left,matches,a)] + [(i,left,v) for v in variants(right,matches,b)]
    def meaning(tree):
        if type(tree) is int: return terms[tree]
        i,left,right = tree;a,b = meaning(left),meaning(right)
        return tuple(a[j] if predicates[i][j] else b[j] for j in range(count))
    def regions(tree,mask):
        if type(tree) is int: return [mask]
        i,left,right = tree
        return regions(left,[m and predicates[i][j] for j,m in enumerate(mask)]) + regions(right,[m and not predicates[i][j] for j,m in enumerate(mask)])
    for _ in range(config["queries"]+1):
        if len(queried) > config["queries"]: raise ValueError("query-budget")
        columns = sorted(queried)
        matches = [[value == truth[j] for j,value in enumerate(row)] for row in terms]
        trace.extend(("learn-terms","cover"))
        if greedy_cover(matches,columns) is None: raise ValueError("no-cover-proposed")
        candidates = trees(matches,columns,config["depth"]);trace.append("learn-split")
        if not candidates: raise ValueError("no-case-tree-proposed")
        tree = candidates[0];candidates += variants(tree,matches,columns);trace.append("assemble")
        meanings = set(map(meaning,candidates))
        free = [j for j in range(count) if j not in queried]
        keys = []
        for j in free:
            buckets = Counter(row[j] for row in meanings)
            keys.append((max(buckets.values()),sum(n*n for n in buckets.values()),j))
        best = min(keys) if keys else None
        if best and best[0] < len(meanings):
            if len(queried) >= config["queries"]: raise ValueError("query-budget")
            j = best[2];queried.add(j);committee_queries.append(j);trace.append("committee-question");streak=0
            continue
        cells = minority_cells(regions(tree,[True]*count),predicates)
        untested = [cell for cell in cells if not any(cell[j] for j in queried)]
        j = max(free,key=lambda j:(sum(cell[j] for cell in untested),-j)) if free else None
        if j is not None:
            if len(queried) >= config["queries"]: raise ValueError("query-budget")
            queried.add(j);spot_queries.append(j);trace.append("spot-check")
            if meaning(tree)[j] != truth[j]: streak=0;continue
            streak += 1
        covered = all(any(cell[j] for j in queried) for cell in cells)
        if j is None or streak >= config["spot_checks"] and covered:
            if j is None: trace.append("spot-check")
            return {"queried":sorted(queried),"committee_queries":committee_queries,
                    "spot_queries":spot_queries,"region_cells":len(cells),"trace":trace,
                    "meaning":meaning(tree)}
    raise ValueError("query-budget")


def validate(problem, report):
    failures = []
    if not compare(check_plan(report["plan"]), []): failures.append("plan-types")
    if not compare(conforms(report["plan"], report["trace"]), True): failures.append("trace-order")
    if report.get("status") in ("inconclusive","refused"): return failures
    if report["kind"] == "ask-when-it-matters":
        distinct, answers, question = reading_answers(problem)
        for name, actual in [("distinct", distinct), ("answers", answers), ("question", question)]:
            if not compare(report.get(name), actual): failures.append("reading-" + name)
    elif report["kind"] == "diagnose":
        diagnoses, probe = diagnostic(problem)
        if not compare(sorted(report["diagnoses"],key=lambda d:d["gates"]), sorted(diagnoses,key=lambda d:d["gates"])): failures.append("diagnoses")
        if not compare(report.get("question"), probe): failures.append("probe")
    elif report["kind"] == "split-by-cases" and report["status"] in ("qualified","refuted"):
        expected = case_search(problem,report["original"]["program"])
        for name in ("queried","committee_queries","spot_queries","region_cells"):
            if not compare(report.get(name),expected[name]): failures.append("case-"+name)
        final = ["capture", "check-program"] if report["status"] == "qualified" else ["capture"]
        if not compare(report["trace"],expected["trace"]+final): failures.append("case-trace")
        actual = tuple(row[1][0] for row in C.R.table(C.program(report["best"]["program"])))
        if not compare(actual,expected["meaning"]): failures.append("case-table")
        failures += C.check_program_report(report["original"]["program"], report)
        if report["status"] == "refuted":
            expected_check = C.comparison(C.program(report["original"]["program"]),C.program(report["best"]["program"]))
            check = report.get("best_check",{})
            if not compare((check.get("verdict"),check.get("ordinal")),(expected_check["status"],expected_check.get("ordinal"))): failures.append("case-counterexample")
    return failures


def self_test():
    plan = {"needs": ["Original", "Proposal"], "goal": ["Qualified"], "node": {"step": "check-program"}}
    bad = {**plan, "needs": []}
    cases = [("types-positive", check_plan(plan), []), ("types-negative", check_plan(bad), ["step-input-missing", "goal-unreachable"]),
             ("trace-positive", conforms(plan,["check-program"]), True), ("trace-negative", conforms(plan,["observe"]),False)]
    failures = [name for name,actual,expected in cases if actual != expected]
    problem={"circuit":{"inputs":1,"gates":[{"kind":"not","wires":[0]},{"kind":"not","wires":[1]}],"outputs":[2]},"vector":[0],"observations":[[2,1]]}
    dplan={"needs":["Model"],"goal":["Answer"],"node":{"seq":[{"step":"observe"},{"step":"explain"},{"step":"isolate"}]}}
    faulty={"plan":dplan,"trace":["observe","explain"],"kind":"diagnose","diagnoses":[],"question":None}
    if validate(problem,faulty) != ["diagnoses","probe"]: failures.append("diagnosis-probe-negative")
    yes={"inputs":[{"kind":"Bool"}],"outputs":[{"kind":"Bool"}],"nodes":[["Bool",True]],"roots":[0]}
    x={**yes,"nodes":[["Input",0]]}
    problem={"background":yes,"readings":[{"id":"s1","alternatives":[yes,x]}],"query":"count"}
    rplan={"needs":["Proposal","Model"],"goal":["Answers"],"node":{"seq":[{"step":"formalize-alternatives"},{"step":"distinct-readings"},{"step":"solve-every-reading"}]}}
    faulty={"plan":rplan,"trace":[],"kind":"ask-when-it-matters","distinct":[],"answers":[],"question":None}
    if validate(problem,faulty) != ["reading-distinct","reading-answers","reading-question"]: failures.append("readings-negative")
    if compare(greedy_cover([[True,False],[False,True]],[0,1]),[0]): failures.append("cover-negative")
    if compare(minority_cells([[True,True]],[[True,False]]),[]): failures.append("regions-negative")
    domain={"kind":"Int","min":"0","max":"3"};out={"kind":"Int","min":str(C.R.I128_MIN),"max":str(C.R.I128_MAX)}
    original={"inputs":[domain],"outputs":[out],"nodes":[["Input",0],["Int","2"],["Lt",0,1],["Int","7"],["Select",2,3,0]],"roots":[4]}
    constant={**original,"nodes":[["Int","7"]],"roots":[0]};identity={**original,"nodes":[["Input",0]],"roots":[0]}
    pred={**original,"outputs":[{"kind":"Bool"}],"nodes":original["nodes"][:3],"roots":[2]}
    problem={"learn":{"integers":[constant,identity],"predicates":[pred]},"config":{"initial":2,"spot_checks":1}}
    faulty={"plan":plan,"trace":[],"kind":"split-by-cases","status":"refuted","original":{"program":original},"best":{"program":constant,**C.metadata(constant)},"best_check":{"verdict":"counterexample","ordinal":2},"queried":[],"committee_queries":[0],"spot_queries":[],"region_cells":-1}
    if validate(problem,faulty) != ["case-queried","case-committee_queries","case-spot_queries","case-region_cells","case-trace","case-table"]: failures.append("cases-negative")
    return failures


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test",action="store_true")
    parser.add_argument("--stub-comparison",action="store_true")
    parser.add_argument("--problem",type=Path)
    parser.add_argument("--report",type=Path)
    args = parser.parse_args()
    if args.stub_comparison:
        global compare
        compare = lambda a,b: True
    if args.self_test:
        failures = self_test()
    else:
        if args.problem is None or args.report is None: parser.error("--problem and --report required")
        report = json.loads(args.report.read_text())
        failures = validate(json.loads(args.problem.read_text()), report.get("detail", report))
    print(json.dumps({"authority":"none","failures":failures}))
    return int(bool(failures))

if __name__ == "__main__": raise SystemExit(main())
