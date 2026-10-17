#!/usr/bin/env python3
"""Independent test reference for concrete tables and fixed strategy grammar.

Uses the public scalar encoding/semantics reference, never Rust proposer code.
Partitions and ranking are recomputed, not read back as truth. This is testing
evidence, carries no authority and is never imported by the product shell.
"""
from __future__ import annotations

import argparse
import hashlib
import itertools
import json
from pathlib import Path

import program_search_reference as R

OPERATORS = ("complete", "repair", "canonical", "resubstitute")
OPCODES = {"Input": 0, "Int": 1, "Bool": 2, "Add": 3, "Sub": 4,
           "Eq": 5, "Lt": 6, "And": 7, "Not": 8, "Select": 9}


def agree(left, right):
    """The comparison seam; its planted-negative tests must die when stubbed."""
    return left == right


def digest(value) -> str:
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"),
                                     ensure_ascii=True).encode("ascii")).hexdigest()


def selection_digest(value) -> str:
    """The strategy commitment uses transform::canonical_json, including its LF."""
    return hashlib.sha256((json.dumps(value, sort_keys=True, separators=(",", ":"),
                                      ensure_ascii=True) + "\n").encode("ascii")).hexdigest()


def _decimal(value):
    if type(value) is not str or str(int(value)) != value:
        raise ValueError("canonical-decimal-required")
    return int(value)


def program(value: dict) -> R.Program:
    if type(value) is not dict or set(value) != {"inputs", "outputs", "nodes", "roots"}:
        raise ValueError("program-shape")

    def domain(d):
        if d == {"kind": "Bool"}:
            return True, 0, 1
        if type(d) is not dict or set(d) != {"kind", "min", "max"} or d["kind"] != "Int":
            raise ValueError("domain-shape")
        lo, hi = _decimal(d["min"]), _decimal(d["max"])
        if not R.I64_MIN <= lo <= hi <= R.I64_MAX:
            raise ValueError("domain-bounds")
        return False, lo, hi

    nodes = []
    for row in value["nodes"]:
        if type(row) is not list or not row or row[0] not in OPCODES:
            raise ValueError("node-shape")
        op = OPCODES[row[0]]
        args = row[1:]
        if len(args) != R.ARITY[op]:
            raise ValueError("node-arity")
        if op == 1:
            args = [_decimal(args[0])]
        elif op == 2:
            if type(args[0]) is not bool:
                raise ValueError("Boolean-literal")
            args = [int(args[0])]
        elif any(type(arg) is not int or not 0 <= arg <= 65535 for arg in args):
            raise ValueError("node-reference")
        nodes.append((op, *args))
    if any(type(root) is not int for root in value["roots"]):
        raise ValueError("root-reference")
    result = R.Program(tuple(domain(d) for d in value["inputs"]), tuple(domain(d) for d in value["outputs"]),
                       tuple(nodes), tuple(value["roots"]))
    R._type_check(result)
    return result


def grammar(depth: int) -> list[tuple[str, ...]]:
    if type(depth) is not int or not 1 <= depth <= 3:
        raise ValueError("grammar-depth")
    return [()] + [word for n in range(1, depth + 1) for word in itertools.product(OPERATORS, repeat=n)
                   if all(a != b for a, b in zip(word, word[1:]))]


def encoding(p: R.Program) -> bytes:
    """ZCVE/1 encoding from its public tags, independent of Rust encoding."""
    def signed(n):
        return b"\x04" + n.to_bytes(16, "big", signed=True)
    def unsigned(n):
        return b"\x03" + n.to_bytes(16, "big")
    def sequence(parts):
        return b"\x08" + len(parts).to_bytes(4, "big") + b"".join(parts)
    def domain(d):
        return sequence([b"\x02" if d[0] else b"\x01", signed(d[1]), signed(d[2])])
    profile = R.PROFILE
    return sequence([b"\x06" + len(profile).to_bytes(4, "big") + profile,
                     sequence([sequence([domain(d) for d in p.inputs]), sequence([domain(d) for d in p.outputs])]),
                     sequence([sequence([signed(n) for n in node]) for node in p.nodes]),
                     sequence([unsigned(n) for n in p.roots])])


def metadata(value: dict) -> dict:
    p = program(value)
    body = encoding(p)
    return {"program": value, "nodes": len(p.nodes), "bytes": len(body),
            "sha256": hashlib.sha256(body).hexdigest()}


def comparison(left: R.Program, right: R.Program) -> dict:
    result = dict(R.compare(left, right))
    result["status"] = {"equal": "equivalent", "different": "counterexample",
                        "abi-differs": "refused"}[result["status"]]
    return result


def partitions(terms: list[dict]) -> tuple[list[list[int]], list[list[int]]]:
    groups, rejected = {}, []
    for index, value in enumerate(terms):
        p = program(value)
        if len(p.outputs) != 1 or p.outputs[0] not in ((True, 0, 1), (False, R.I64_MIN, R.I64_MAX)):
            raise ValueError("full-term-output")
        rows = R.table(p, max_tuples=1 << 17)
        errors = [i for i, row in enumerate(rows) if row[0] != "ok"]
        if errors:
            rejected.append([index, errors[0]])
        else:
            groups.setdefault((p.outputs[0][0], tuple(rows)), []).append(index)
    return sorted((sorted(group) for group in groups.values())), sorted(rejected)


def check_bank(bank: dict) -> list[str]:
    failures = []
    expected, rejected = partitions(bank["terms"])
    if not agree(sorted(sorted(group) for group in bank["classes"]), expected):
        failures.append("term-partition")
    if not agree(sorted(bank["rejected"]), rejected):
        failures.append("trapping-terms")
    for rule in bank["rules"]:
        lhs, rhs = program(rule["lhs"]), program(rule["rhs"])
        if not agree(comparison(lhs, rhs)["status"], "equivalent"):
            failures.append("rule-not-equivalent")
        if not agree((len(rhs.nodes), len(encoding(rhs))) < (len(lhs.nodes), len(encoding(lhs))), True):
            failures.append("rule-cost")
    return failures


def check_program_report(original: dict, report: dict) -> list[str]:
    failures = []
    original = program(original)
    for artifact in ([report["best"]] if report.get("best") else []) + [row["candidate"] for row in report.get("candidates", [])]:
        expected = metadata(artifact["program"])
        for field in ("nodes", "bytes", "sha256"):
            if not agree(artifact.get(field), expected[field]):
                failures.append("program-" + field)
    if report["status"] == "qualified":
        best = report.get("best")
        check = report.get("best_check", {})
        if not best:
            return ["qualified-without-program"]
        if not agree(comparison(original, program(best["program"]))["status"], "equivalent"):
            failures.append("qualified-wrong-program")
        if not agree((check.get("verdict"), check.get("replay")), ("equivalent", "matched")):
            failures.append("qualified-without-replay")
    for row in report.get("candidates", []):
        actual = comparison(original, program(row["candidate"]["program"]))
        check = row["check"]
        if check.get("verdict") in ("equivalent", "counterexample"):
            if not agree(actual["status"], check["verdict"]):
                failures.append("candidate-verdict")
            if check["verdict"] == "counterexample" and not agree(actual.get("ordinal"), check.get("ordinal")):
                failures.append("counterexample-ordinal")
    return failures


def check_strategy(report: dict) -> list[str]:
    failures, scores = [], []
    selection = report.get("selection")
    if not selection:
        return [] if report["status"] == "inconclusive" else ["missing-selection"]
    policies = grammar(selection["depth"])
    if not agree(selection["depth"], 1):
        failures.append("runtime-strategy-depth")
    if not agree(len(report["trials"]), len(policies)):
        failures.append("development-policy-coverage")
    if not agree(report["policies_generated"], len(policies)):
        failures.append("grammar-count")
    if not agree(report["selection_sha256"], selection_digest(selection)):
        failures.append("selection-digest")
    if not agree((report.get("selection_frozen_before_evaluation"), report.get("evaluation_influenced_selection")),
                 (True, False)):
        failures.append("selection-boundary")
    for index, trial in enumerate(report["trials"]):
        count = sum(run["status"] == "qualified" for run in trial["runs"])
        nodes = sum(len(program(run["best"]["program"]).nodes) if run["status"] == "qualified" else
                    2 * len(program(run["original"]["program"]).nodes) for run in trial["runs"])
        pairs = sum(row["check"].get("gate_pairs_reserved", 0) for run in trial["runs"]
                    for row in run.get("candidates", []))
        if not agree(len(trial["runs"]), 1):
            failures.append("runtime-development-task-count")
        for run in trial["runs"]:
            checks = [row["check"] for row in run.get("candidates", [])]
            stopped_check = any(check.get("verdict") == "inconclusive" or
                                (check.get("verdict") == "equivalent" and check.get("replay") != "matched")
                                for check in checks)
            empty_search = (run.get("reason") == "bounded-search-no-qualified-candidate" and not checks)
            stopped_run = run["status"] == "inconclusive" and not empty_search
            if not agree(stopped_check or stopped_run, False):
                failures.append("development-stop-selected")
            failures += check_program_report(run["original"]["program"], run)
        if not agree(trial["score"]["qualified"], count):
            failures.append("qualified-count")
        if not agree(trial["score"]["nodes_with_failure_penalty"], nodes):
            failures.append("node-score")
        if not agree(trial["score"]["gate_pairs_reserved"], pairs):
            failures.append("gate-work-score")
        if index >= len(policies) or not agree(tuple(trial["stages"]), policies[index]):
            failures.append("grammar-order")
        score = trial["score"]
        scores.append((-count, nodes, pairs,
                       len(trial["stages"]), index))
    if scores:
        winner = min(range(len(scores)), key=lambda index: scores[index])
        if not agree(selection["stages"], report["trials"][winner]["stages"]):
            failures.append("strategy-ranking")
    return failures


def check_rules(report: dict) -> list[str]:
    original = report["original"]["program"]
    failures = check_program_report(original, report)
    requalified = 0
    for row in report["checks"]:
        check = row.get("check")
        if check is None:
            continue
        if row["stage"] == "target-rule":
            pair = program(row["lhs"]["program"]), program(row["rhs"]["program"])
            requalified += check.get("verdict") == "equivalent" and check.get("replay") == "matched"
        else:
            pair = program(original), program(row["candidate"]["program"])
        actual = comparison(*pair)
        if check.get("verdict") in ("equivalent", "counterexample"):
            if not agree(check["verdict"], actual["status"]):
                failures.append("rule-or-whole-program-verdict")
            if check["verdict"] == "counterexample" and not agree(check.get("ordinal"), actual.get("ordinal")):
                failures.append("rule-counterexample-ordinal")
    if "improved" in report or "result_kind" in report:
        before = program(original)
        after = program(report["best"]["program"])
        improved = (len(after.nodes), len(encoding(after))) < (len(before.nodes), len(encoding(before)))
        if not agree(report.get("improved"), improved):
            failures.append("improvement-claim")
        expected = "improved-program" if improved else "unchanged-original"
        if not agree(report.get("result_kind"), expected):
            failures.append("rule-result-label")
    if not agree(report["rules_requalified"], requalified):
        failures.append("requalified-rule-count")
    current = program(original)
    for row in report["applied"]:
        before, after = program(row["before"]["program"]), program(row["after"]["program"])
        if not agree(current, before) or not agree(comparison(program(original), after)["status"], "equivalent"):
            failures.append("applied-program-chain")
        if not agree((len(after.nodes), len(encoding(after))) < (len(before.nodes), len(encoding(before))), True):
            failures.append("applied-cost")
        current = after
    if not agree(program(report["best"]["program"]), current):
        failures.append("final-program-chain")
    return failures


def self_test() -> list[str]:
    """A positive and planted-negative case for each reference comparison."""
    from copy import deepcopy
    integer = {"kind": "Int", "min": str(R.I64_MIN), "max": str(R.I64_MAX)}
    x = {"inputs": [{"kind": "Int", "min": "0", "max": "3"}], "outputs": [integer],
         "nodes": [["Input", 0]], "roots": [0]}
    long = {**x, "nodes": [["Input", 0], ["Add", 0, 0], ["Sub", 1, 0]], "roots": [2]}
    wrong = {**x, "nodes": [["Input", 0], ["Int", "1"], ["Add", 0, 1]], "roots": [2]}
    good_bank = {"terms": [x, long], "classes": [[0, 1]], "rejected": [], "rules": [{"lhs": long, "rhs": x}]}
    bad_partition = deepcopy(good_bank)
    bad_partition["classes"] = [[0], [1]]
    bad_rule = deepcopy(good_bank)
    bad_rule["rules"] = [{"lhs": wrong, "rhs": x}]
    good_program = {"status": "qualified", "best": metadata(x), "best_check": {"verdict": "equivalent", "replay": "matched"}}
    bad_program = deepcopy(good_program)
    bad_program["best"] = metadata(wrong)
    bad_replay = deepcopy(good_program)
    bad_replay["best_check"]["replay"] = "differs"
    cases = [("bank-positive", [], check_bank(good_bank)),
             ("partition-negative", ["term-partition"], check_bank(bad_partition)),
             ("rule-negative", ["rule-not-equivalent"], check_bank(bad_rule)),
             ("program-positive", [], check_program_report(long, good_program)),
             ("program-negative", ["qualified-wrong-program"], check_program_report(long, bad_program)),
             ("replay-negative", ["qualified-without-replay"], check_program_report(long, bad_replay))]
    trials = [{"stages": list(word), "score": {"qualified": 1, "nodes_with_failure_penalty": 1,
               "gate_pairs_reserved": 8, "stage_count": len(word), "grammar_index": i},
               "runs": [{"status": "qualified", "original": metadata(long), "best": metadata(x),
                         "best_check": {"verdict": "equivalent", "replay": "matched"},
                         "candidates": [{"candidate": metadata(x), "check": {"verdict": "equivalent",
                                         "replay": "matched", "gate_pairs_reserved": 8}}]}]}
              for i, word in enumerate(grammar(1))]
    selection = {"depth": 1, "stages": []}
    good_strategy = {"status": "evaluated", "policies_generated": 5, "trials": trials, "selection": selection,
                     "selection_sha256": selection_digest(selection), "selection_frozen_before_evaluation": True,
                     "evaluation_influenced_selection": False}
    bad_strategy = deepcopy(good_strategy)
    bad_strategy["policies_generated"] = 4
    stopped_strategy = deepcopy(good_strategy)
    stopped = stopped_strategy["trials"][-1]
    stopped["runs"][0].update({"status": "inconclusive", "reason": "bounded-search-no-qualified-candidate", "best": None})
    stopped["runs"][0]["candidates"][0]["check"] = {"verdict": "inconclusive", "reason": "gate-budget"}
    stopped["score"].update({"qualified": 0, "nodes_with_failure_penalty": 6, "gate_pairs_reserved": 0})
    cases += [("strategy-positive", [], check_strategy(good_strategy)),
              ("strategy-negative", ["grammar-count"], check_strategy(bad_strategy)),
              ("development-stop-negative", ["development-stop-selected"], check_strategy(stopped_strategy))]
    good_rules = {"status": "qualified", "original": metadata(long), "best": metadata(x),
                  "best_check": {"verdict": "equivalent", "replay": "matched"}, "rules_requalified": 1,
                  "checks": [{"stage": "target-rule", "check": {"verdict": "equivalent", "replay": "matched"},
                              "lhs": metadata(long), "rhs": metadata(x)}],
                  "applied": [{"before": metadata(long), "after": metadata(x)}]}
    bad_rules = deepcopy(good_rules)
    bad_rules["rules_requalified"] = 0
    cases += [("target-rules-positive", [], check_rules(good_rules)),
              ("target-count-negative", ["requalified-rule-count"], check_rules(bad_rules))]
    return [name for name, expected, observed in cases if observed != expected]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bank", type=Path)
    parser.add_argument("--strategy", type=Path)
    parser.add_argument("--rules", type=Path)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--stub-comparison", action="store_true")
    args = parser.parse_args()
    if args.stub_comparison:
        global agree
        agree = lambda left, right: True
    if args.self_test:
        failures = self_test()
        print(json.dumps({"authority": "none", "self_test": "failed" if failures else "passed", "failures": failures}))
        return int(bool(failures))
    if sum(bool(v) for v in (args.bank, args.strategy, args.rules)) != 1:
        parser.error("name exactly one --bank, --strategy or --rules")
    value = json.loads((args.bank or args.strategy or args.rules).read_text())
    value = value.get("detail", value)
    failures = check_bank(value.get("bank", value)) if args.bank else check_strategy(value) if args.strategy else check_rules(value)
    print(json.dumps({"authority": "none", "failures": failures}))
    return int(bool(failures))


if __name__ == "__main__":
    raise SystemExit(main())
