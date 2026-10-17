#!/usr/bin/env python3
"""Non-vacuous CLI controls for composable program loops; run after a build slot.

Each planted fault must be caught by the existing checker or binding gate.
Refusal, timeout and inconclusive never count as an expected counterexample.
LLM-generated harness, no product authority and no acceptance callback.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess

import loop_composable_reference as C


def qualifies(code: int, report: dict, expected: str) -> bool:
    return code == (0 if expected == "qualified" else 1) and report.get("status") == expected


def self_test(stub: bool) -> list[str]:
    classify = (lambda *args: True) if stub else qualifies
    cases = [(0, {"status": "qualified"}, "qualified", True),
             (1, {"status": "refuted"}, "refuted", True),
             (1, {"status": "refused"}, "refuted", False),
             (2, {"status": "inconclusive"}, "refuted", False),
             (0, {"status": "qualified"}, "refuted", False)]
    return [str(i) for i, (code, report, expected, wanted) in enumerate(cases)
            if classify(code, report, expected) != wanted]


def campaign(cli: Path, work: Path) -> dict:
    from test_loop_composable_reference import LONG, WRONG, X
    work.mkdir(parents=True, exist_ok=False)

    def write(name, value):
        path = work / name
        with path.open("x", encoding="ascii") as stream:
            json.dump(value, stream, ensure_ascii=True, sort_keys=True)
        return path

    def run(*arguments):
        done = subprocess.run([str(cli), *map(str, arguments)], capture_output=True, timeout=120)
        if len(done.stdout) > 8 << 20:
            raise RuntimeError("control-output-bound")
        return done.returncode, json.loads(done.stdout)

    original = work / "original.zcve"
    original.write_bytes(C.encoding(C.program(LONG)))
    code, packet = run("loop", "pipeline", "--original", original, "--packet")
    if code != 0 or packet.get("status") != "packet":
        return {"rows": [{"name": "packet", "passed": False, "report": packet}], "passed": False}

    def proposal(candidate, stages):
        return {"schema": "zeno-fcis/loop-pipeline/1", "original_sha256": packet["detail"]["original_sha256"],
                "implementation_sha256": packet["detail"]["implementation_sha256"],
                "program": candidate, "stages": stages}

    rows = []
    for name, candidate, expected in [("positive", X, "qualified"), ("wrong-output", WRONG, "refuted")]:
        path = write(name + ".json", proposal(candidate, []))
        code, report = run("loop", "pipeline", "--original", original, "--proposal", path)
        problems = C.check_program_report(LONG, report.get("detail", {}))
        rows.append({"name": name, "passed": qualifies(code, report, expected) and not problems,
                     "exit": code, "report": report, "reference": problems})
    stale = proposal(X, [])
    stale["implementation_sha256"] = "0" * 64
    path = write("stale.json", stale)
    code, report = run("loop", "pipeline", "--original", original, "--proposal", path)
    rows.append({"name": "stale-code", "passed": code == 1 and report.get("status") == "refused",
                 "exit": code, "report": report})
    path = write("late.json", proposal(LONG, ["canonical"]))
    out = work / "late-export"
    code, report = run("loop", "pipeline", "--original", original, "--proposal", path,
                       "--max-gate-pairs", "8", "--out", out)
    rows.append({"name": "late-budget", "passed": code == 2 and report.get("status") == "inconclusive"
                 and not (out / "candidate.zcve").exists() and report.get("detail", {}).get("attempts_spent") == 0,
                 "exit": code, "report": report})
    out = work / "rules"
    code, report = run("loop", "rules", "--original", original, "--out", out)
    detail = report.get("detail", {})
    problems = C.check_bank(detail["bank"]) if "bank" in detail else ["missing-bank"]
    problems += C.check_rules(detail) if "original" in detail else ["missing-result"]
    rows.append({"name": "concrete-rules", "passed": qualifies(code, report, "qualified") and not problems
                 and detail.get("best", {}).get("nodes") == 1, "exit": code, "report": report, "reference": problems})
    forged = write("forged-bank.json", {"schema": "zeno-fcis/loop-rule-bank/1", "authority": "forged",
                                        "rules": [{"lhs": WRONG, "rhs": X}]})
    code, report = run("loop", "rules", "--original", original, "--bank", forged)
    detail = report.get("detail", {})
    rejected = any(row.get("stage") == "target-rule" and row.get("check", {}).get("verdict") == "counterexample"
                   for row in detail.get("checks", []))
    rows.append({"name": "forged-rule", "passed": qualifies(code, report, "qualified") and rejected
                 and detail.get("rules_requalified") == 0 and not detail.get("applied"),
                 "exit": code, "report": report})
    evaluation_program = json.loads(json.dumps(LONG))
    evaluation_program["inputs"][0]["max"] = "2"
    evaluation = work / "evaluation.zcve"
    evaluation.write_bytes(C.encoding(C.program(evaluation_program)))
    packet_code, eval_packet = run("loop", "pipeline", "--original", evaluation, "--packet")
    evaluation_proposal = {"schema": "zeno-fcis/loop-pipeline/1",
        "original_sha256": eval_packet.get("detail", {}).get("original_sha256"),
        "implementation_sha256": eval_packet.get("detail", {}).get("implementation_sha256"),
        "program": evaluation_program, "stages": []}
    task_data = {"schema": "zeno-fcis/loop-strategy/1",
        "development": [{"original": original.name, "proposal": proposal(LONG, [])}],
        "evaluation": [{"original": evaluation.name, "proposal": evaluation_proposal}]}
    tasks = write("tasks.json", task_data)
    code, strategy = run("loop", "strategy", "--tasks", tasks)
    strategy_problems = C.check_strategy(strategy.get("detail", {}))
    # Change only the held-out proposal; no development binding or setting changes.
    task_data["evaluation"][0]["proposal"]["program"]["nodes"] = [["Input", 0]]
    task_data["evaluation"][0]["proposal"]["program"]["roots"] = [0]
    changed = write("tasks-changed-evaluation.json", task_data)
    other_code, other = run("loop", "strategy", "--tasks", changed)
    strategy_problems += C.check_strategy(other.get("detail", {}))
    frozen = strategy.get("detail", {}).get("selection_sha256")
    rows.append({"name": "strategy-freeze", "passed": packet_code == code == other_code == 0
                 and strategy.get("status") == other.get("status") == "evaluated"
                 and bool(frozen) and frozen == other.get("detail", {}).get("selection_sha256")
                 and not strategy_problems, "exit": code, "report": strategy,
                 "changed_evaluation_report": other, "reference": strategy_problems})
    stopped_out = work / "development-stop"
    stop_code, stopped = run("loop", "strategy", "--tasks", tasks,
                             "--max-gate-pairs", "44", "--out", stopped_out)
    stop_detail = stopped.get("detail", {})
    rows.append({"name": "development-check-budget-stop", "passed": stop_code == 2
                 and stopped.get("status") == "inconclusive" and stop_detail.get("selection") is None
                 and stop_detail.get("evaluation") == [] and stop_detail.get("attempts_spent") == 0
                 and not (stopped_out / "selection.json").exists()
                 and not (stopped_out / "candidate.zcve").exists(), "exit": stop_code, "report": stopped})
    # No result is silently dropped. All eight mandatory controls must apply.
    return {"schema": "zeno-fcis/composable-controls/1", "authority": "none", "rows": rows,
            "passed": len(rows) == 8 and all(row["passed"] is True for row in rows)}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--stub-comparison", action="store_true")
    parser.add_argument("--cli", type=Path)
    parser.add_argument("--work", type=Path)
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    if args.self_test:
        failures = self_test(args.stub_comparison)
        print(json.dumps({"self_test": "failed" if failures else "passed", "failures": failures}))
        return int(bool(failures))
    if not all((args.cli, args.work, args.out)) or args.stub_comparison:
        parser.error("--cli --work --out are required; stubs are self-test only")
    result = campaign(args.cli.resolve(), args.work)
    with args.out.open("x", encoding="ascii") as stream:
        json.dump(result, stream, sort_keys=True, indent=2)
    print(json.dumps({"passed": result["passed"], "rows": len(result["rows"])}))
    return int(not result["passed"])


if __name__ == "__main__":
    raise SystemExit(main())
