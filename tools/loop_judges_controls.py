#!/usr/bin/env python3
"""Fault controls for the loop's question, teach-back and decompose doors and the routed scenario act.

Instances are generated afresh with ``tools/instantiate_core.py`` in a scratch directory, every parameter set
the core-component catalogue declares. Per instance:

* **teach-back, one reason changed**: a rule restating one reject case of the core (its first-match region,
  written from the policy) must be exact; the same rule with another declared reason must be wrong with
  exactly that region reported as exceptions;
* **an answer contradicting the core**: the first question's options include one the core does not make;
  answering it must be reported as a disagreement, with its laws listed, and the contract's files must not
  change;
* **a decomposition with one read removed**: one component owning every state field and reading every request
  field must be realizable; removing one read must give an interface counterexample whose suggested or
  separating reads name the removed field (the first read whose removal breaks it);
* **a decomposition with one wrong case**: that component's projected case table with one reject case given
  another reason must be refuted and blamed on that component, with a region;
* **the composed check** (stage 3): the one-component decomposition, its case table projected, composed with
  ``loop decompose --compose`` must equal the core on every input with its obligations proved; the same with
  one reject case given another reason must differ from the core;
* **the reject-everything core**: for each family's first instance, a loop workspace receives one scenario of a
  single accepted step from genesis, routed to ``contract scenario``; the scenario must pass on the core and
  the reject-everything core must fail it.

Every verdict comes from the CLI; this tool counts. A row is ``passed`` only when the door gave its normal
verdict and the control's comparison holds. A door that refuses, is inconclusive or answers with an
unexpected status fails the row; only a structural reason (an instance with no reject case, a single reason,
no open question) makes a row not applicable, and each control needs at least half of its rows, and at least
one, applicable.

**Size envelope.** The teach-back and disagreement controls (the stages the admissibility table and the gaps
judge back) apply to declared domains of at most ``SIZE_ENVELOPE`` state x input tuples. On the doors' fixed
stage deadlines a larger domain can stop inconclusive; such a row is reported ``outside-size-envelope`` with
its domain and timing, is not counted as a pass and does not fail the run. A wrong verdict on it still fails
the run. ``--out DIR`` receives ``judges-controls.json`` and ``judges-controls.md`` with every row.

``--self-test`` runs no CLI: it feeds each control's comparison right and planted-wrong door results, checks
that the planted ones fail and the right ones pass, and that with the comparison stubbed the planted ones
pass (so the comparison is what decides), and that a run whose doors refuse everywhere fails.

This tool is LLM-generated.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "integrations" / "zal"))
sys.path.insert(0, str(ROOT / "tools"))

import instantiate_core  # noqa: E402
import loop_doors  # noqa: E402
import loop_language as Z  # noqa: E402
import loop_workspace as LW  # noqa: E402

FAMILIES = ("reservation-pool", "rate-limiter", "approval-queue", "bounded-counter", "consumable-budget",
            "versioned-register", "idempotency-slot", "retry-budget", "finite-phase-machine", "logical-deadline")


# Largest declared domain (state x input tuples) the teach-back and disagreement controls apply to.
SIZE_ENVELOPE = 8192
ENVELOPED = ("teach_back", "disagreement")


def domain_size(fields: dict) -> int:
    """Declared state x input tuples: each field's declared values (a sum type's variants, else its range)."""
    total = 1
    for name, (low, high) in zip(fields["names"], fields["ranges"]):
        table = fields["variants"].get(name)
        total *= len(table) if table else high - low + 1
    return total


def enveloped(key: str, row: dict, size: int) -> dict:
    """Outside the size envelope a row that passed, or whose door did not answer, is reported
    ``outside-size-envelope`` (not a pass, not a failure); a wrong verdict stays a failure."""
    row["domain"] = size
    if key not in ENVELOPED or size <= SIZE_ENVELOPE or row.get("passed") is None:
        return row
    wrong_verdict = row.get("passed") is False and "result" not in row
    if wrong_verdict:
        return row
    row["outcome"] = "passed" if row.get("passed") is True else row.get("result", "not applicable")
    row["result"] = f"outside-size-envelope ({size} tuples > {SIZE_ENVELOPE})"
    row["passed"] = None
    row["outside_size_envelope"] = True
    return row


# --- the comparisons (stubbed in the self-test) ---------------------------------------------------------------

def teach_back_passes(row: dict) -> bool:
    return row["own"] == "exact" and row["changed"] == "wrong" and row["exceptions"] == row["region"]


def disagreement_passes(row: dict) -> bool:
    return row["status"] == "disagreement" and row["files_unchanged"] is True


def read_removed_passes(row: dict) -> bool:
    return row["removed"] in row["named"]


def wrong_case_passes(row: dict) -> bool:
    return row["status"] == "refuted" and row["blamed"] == "all" and bool(row["wrong"])


def composed_passes(row: dict) -> bool:
    return (row["status"] == "accepted" and row["differ"] == 0 and row["obligations"] == "proved"
            and ("planted_status" not in row or (row["planted_status"] == "refuted" and bool(row["planted_differ"]))))


def reject_everything_passes(row: dict) -> bool:
    return row["passed_on_core"] is True and row["reject_everything_killed"] is True


# Owner decision 2026-10-16: the original <1.0 s target was never met on a contained run; the measured time is
# 1.73-1.79 s (about 5 CPU-seconds under the 400% checker quota). This guards against regression, not a speed goal.
QUESTION_TIMING_LIMIT_SECONDS = 2.5


def question_timing_passes(row: dict) -> bool:
    return (row["status"] == "determined" and row["answers"] == 8 and row["tuples"] == 432
            and row["determined_with_reasons"] == 432 and row["inconsistent"] == 0
            and row["answers_recorded"] == row["answers"]
            and row["gaps"] == "measured" and row["identical_repeat"] is True
            and row["seconds"] < QUESTION_TIMING_LIMIT_SECONDS)


def question_timing_control(cli: Path, contract: Path, scratch: Path) -> dict:
    """Time the full eight-answer AQ K1 question, then compare its complete detail on replay."""
    fixture = ROOT / "crates/zeno-fcis-loop-doors/tests/data/aq-k1-explicit-answers.json"
    document = write(scratch / "question-answers.json", json.loads(fixture.read_text()))
    start = time.monotonic()
    code, report = run(cli, "loop", "question", "--contract", contract, "--answers", document)
    elapsed = time.monotonic() - start
    repeat_code, repeat = run(cli, "loop", "question", "--contract", contract, "--answers", document)
    detail = report.get("detail") or {}
    stats = detail.get("stats") or {}
    row = {"instance": "approval-queue-k1", "seconds": elapsed, "limit_seconds": QUESTION_TIMING_LIMIT_SECONDS,
           "status": report.get("status"), "answers": len(detail.get("answers") or []),
           "answers_recorded": sum(answer.get("status") == "recorded" for answer in detail.get("answers") or []),
           "tuples": stats.get("tuples"), "determined_with_reasons": stats.get("determined_with_reasons"),
           "inconsistent": stats.get("inconsistent"), "gaps": (detail.get("gaps") or {}).get("status"),
           "identical_repeat": code == repeat_code == 0 and detail == (repeat.get("detail") or {})}
    row["passed"] = question_timing_passes(row)
    return row


COMPARE = {"teach_back": teach_back_passes, "disagreement": disagreement_passes,
           "read_removed": read_removed_passes, "wrong_case": wrong_case_passes, "composed": composed_passes,
           "reject_everything": reject_everything_passes}


def judged(key: str, row: dict) -> dict:
    """The row with its verdict from the control's comparison."""
    row["passed"] = bool(COMPARE[key](row))
    return row


def failed(row: dict, report: dict, what: str) -> dict:
    """A row the door did not answer normally: a failure, never 'not applicable'."""
    reason = (report.get("detail") or {}).get("reason") if isinstance(report.get("detail"), dict) else None
    row.update({"result": f"{what}: {report.get('status')}" + (f" ({str(reason)[:160]})" if reason else ""),
                "passed": False})
    return row


def not_applicable(row: dict, why: str) -> dict:
    row.update({"result": why, "passed": None})
    return row


SECONDS: dict = {}
# Finite-data commands stay on POSIX even when the environment selects systemd.
FINITE_DATA = ("--containment-backend", "auto")


def run(cli: Path, *arguments) -> tuple[int, dict]:
    started = time.monotonic()
    done = subprocess.run([str(cli), *FINITE_DATA, *map(str, arguments)], capture_output=True, timeout=600)
    SECONDS["last"] = round(time.monotonic() - started, 2)
    try:
        return done.returncode, json.loads(done.stdout or b"{}")
    except ValueError:
        return done.returncode, {"unreadable": done.stdout[-400:].decode(errors="replace")}


def write(path: Path, value: dict) -> Path:
    path.write_text(json.dumps(value))
    return path


def case_where(fields: dict, k: int) -> str | None:
    """Case k's first-match region over the field names, from the policy."""
    parts = []
    for j, case in enumerate(fields["cases"][:k + 1]):
        when = case.get("when", "true")
        node = LW._policy_expression(fields, when if isinstance(when, str) else ("true" if when else "false"))
        if node is None:
            return None
        parts.append(node if j == k else ("not", node))
    text = " && ".join(f"({Z.print_symbolic(p)})" for p in parts)
    return text


def teach_back_control(cli: Path, name: str, contract: Path, fields: dict, scratch: Path) -> dict:
    reasons = {v: k for k, v in fields["reason_ids"].items()}
    for k, case in enumerate(fields["cases"]):
        if case.get("class") != "Reject":
            continue
        where = case_where(fields, k)
        if where is None:
            continue
        reason = reasons[case["reason"]]
        other = next((r for r in sorted(fields["reason_ids"]) if r != reason), None)
        if other is None:
            break
        rules = {"schema": "zeno-fcis/loop-rules/1", "rules": [
            {"id": "own", "where": where, "class": "Reject", "reason": reason},
            {"id": "changed", "where": where, "class": "Reject", "reason": other}]}
        code, report = run(cli, "loop", "teach-back", "--contract", contract, "--rule",
                           write(scratch / f"{name}-tb.json", rules))
        rows = (report.get("detail") or {}).get("rules") or []
        row = {"instance": name, "case": k, "reason": reason, "changed_to": other, "seconds": SECONDS["last"]}
        if report.get("status") != "judged" or len(rows) != 2 or any(r.get("verdict") == "refused" for r in rows):
            return failed(row, report, "teach-back did not judge both rules")
        own, changed = rows
        if own.get("applies", 0) == 0:
            continue
        row.update({"own": own["verdict"], "changed": changed["verdict"], "region": own["applies"],
                    "exceptions": changed["exceptions"], "missing": changed["missing"]})
        return judged("teach_back", row)
    return not_applicable({"instance": name}, "no reject case with a readable region and a second reason")


def disagreement_control(cli: Path, name: str, contract: Path, scratch: Path) -> dict:
    before = {p: p.read_bytes() for p in (contract / "project.zeno", contract / "v2" / "policy.json")}
    code, report = run(cli, "loop", "question", "--contract", contract)
    asked = SECONDS["last"]
    question = (report.get("detail") or {}).get("question")
    if report.get("status") == "determined":
        return not_applicable({"instance": name}, "the declared laws determine every decision")
    if report.get("status") != "open" or not question:
        return failed({"instance": name, "seconds": asked}, report, "the question door asked nothing")
    # Another class when one is offered (the gaps judge then sees the pinned laws fail on the core), else
    # another reason.
    current = next((o for o in question["options"] if o["current"]), None)
    others = [o for o in question["options"] if not o["current"]]
    other = next((o for o in others if current is None or o["class"] != current["class"]), None) or \
        next(iter(others), None)
    if other is None:
        return not_applicable({"instance": name}, "only the core's option")
    answer = {"where": question["where"], "class": other["class"], "reason": other.get("reason"),
              "case": question["case"]}
    code, report = run(cli, "loop", "question", "--contract", contract, "--answers",
                       write(scratch / f"{name}-dis.json", {"schema": "zeno-fcis/loop-answers/1",
                                                             "answers": [answer]}))
    detail = report.get("detail") or {}
    record = (detail.get("answers") or [{}])[0]
    unchanged = all(p.read_bytes() == b for p, b in before.items())
    row = {"instance": name, "answer": f"{other['class']} {other.get('reason') or ''}".strip(),
           "status": record.get("status") or report.get("status"), "laws": len(record.get("laws") or []),
           "false_alarm": "false_alarm" in record, "files_unchanged": unchanged,
           "seconds": [asked, SECONDS["last"]]}
    if report.get("status") not in ("open", "determined"):
        return failed(row, report, "the answer was not checked")
    return judged("disagreement", row)


def one_component(fields: dict) -> dict:
    state = fields["state"]
    reads = [n for n in fields["names"] if n not in state]
    return {"name": "all", "owns": state, "reads": reads, "decides": sorted(fields["reason_ids"])}


def decompose(cli: Path, contract: Path, path: Path, components: list) -> dict:
    code, report = run(cli, "loop", "decompose", "--contract", contract, "--architecture",
                       write(path, {"schema": "zeno-fcis/loop-architecture/1", "components": components}))
    return report


def read_removed_control(cli: Path, name: str, contract: Path, fields: dict, scratch: Path) -> tuple[dict, dict]:
    whole = one_component(fields)
    report = decompose(cli, contract, scratch / f"{name}-all.json", [whole])
    if report.get("status") != "completed":
        return failed({"instance": name}, report, "the one-component decomposition did not complete"), report
    for field in whole["reads"]:
        part = dict(whole, reads=[f for f in whole["reads"] if f != field])
        removed = decompose(cli, contract, scratch / f"{name}-no-{field}.json", [part])
        if removed.get("status") not in ("completed", "unrealizable", "meets-requirements", "refuted"):
            return failed({"instance": name, "removed": field}, removed, "decompose did not judge"), report
        clashes = (removed.get("detail") or {}).get("interface") or []
        if not clashes:
            continue
        named = sorted(set(clashes[0].get("suggested_reads") or []) | set(clashes[0].get("separating_fields") or []))
        return judged("read_removed", {"instance": name, "removed": field, "status": removed.get("status"),
                                       "suggested_reads": clashes[0].get("suggested_reads"), "named": named,
                                       "names_it": field in named}), report
    return not_applicable({"instance": name}, "no single read is needed"), report


def wrong_case_control(cli: Path, name: str, contract: Path, fields: dict, scratch: Path, whole: dict) -> dict:
    table = (((whole.get("detail") or {}).get("projection") or {}).get("all") or {}).get("table")
    if not table:
        return failed({"instance": name}, whole, "no projected table")
    cases = [dict(c) for c in table]
    index = next((k for k, c in enumerate(cases) if c["class"] == "Reject"), None)
    if index is None or len(fields["reason_ids"]) < 2:
        return not_applicable({"instance": name}, "no reject case or a single reason")
    other = next(r for r in sorted(fields["reason_ids"]) if r != cases[index]["reason"])
    cases[index]["reason"] = other
    for case in cases:
        if not case.get("sets"):
            case.pop("sets", None)
        if case["class"] == "Accept":
            case.pop("reason", None)
    part = dict(one_component(fields), cases=cases)
    report = decompose(cli, contract, scratch / f"{name}-wrong.json", [part])
    blame = ((report.get("detail") or {}).get("blame") or [{}])[0]
    row = {"instance": name, "case": index, "changed_to": other, "status": report.get("status"),
           "blamed": blame.get("component"), "wrong": blame.get("wrong"), "region": blame.get("where")}
    if report.get("status") not in ("refuted", "meets-requirements", "completed", "unrealizable"):
        return failed(row, report, "decompose did not judge")
    return judged("wrong_case", row)


def composed_control(cli: Path, name: str, contract: Path, fields: dict, scratch: Path, whole: dict) -> dict:
    table = (((whole.get("detail") or {}).get("projection") or {}).get("all") or {}).get("table")
    if not table:
        return failed({"instance": name}, whole, "no projected table")

    def composed(label: str, cases: list) -> dict:
        part = dict(one_component(fields), cases=cases)
        code, report = run(cli, "loop", "decompose", "--contract", contract, "--architecture",
                           write(scratch / f"{name}-{label}.json", {"schema": "zeno-fcis/loop-architecture/1",
                                                                     "components": [part]}), "--compose")
        return report

    cases = []
    for case in table:
        case = dict(case)
        if not case.get("sets"):
            case.pop("sets", None)
        if case["class"] == "Accept":
            case.pop("reason", None)
        cases.append(case)
    right = composed("compose", cases)
    detail = (right.get("detail") or {}).get("compose") or {}
    row = {"instance": name, "status": right.get("status"), "inputs": detail.get("inputs"),
           "differ": detail.get("differ"), "obligations": (detail.get("obligations") or {}).get("verdict")}
    index = next((k for k, c in enumerate(cases) if c["class"] == "Reject"), None)
    if index is not None and len(fields["reason_ids"]) > 1:
        wrong = [dict(c) for c in cases]
        wrong[index]["reason"] = next(r for r in sorted(fields["reason_ids"]) if r != cases[index]["reason"])
        planted = composed("compose-wrong", wrong)
        row["planted_status"] = planted.get("status")
        row["planted_differ"] = ((planted.get("detail") or {}).get("compose") or {}).get("differ")
    if right.get("status") not in ("accepted", "refuted", "obligations-not-proved"):
        return failed(row, right, "the composed check did not judge")
    if "planted_status" in row and row["planted_status"] not in ("accepted", "refuted", "obligations-not-proved"):
        return failed(row, planted, "the planted composition was not judged")
    return judged("composed", row)


def reject_everything_control(cli: Path, family: str, contract: Path, work: Path) -> dict:
    sys.path.insert(0, str(ROOT / "tools"))
    import contract_judges_reference as cj
    sys.path.pop(0)
    c = cj.Contract((contract / "project.zeno").read_text(), json.loads((contract / "v2" / "policy.json").read_text()))
    m = cj.Machine(c)
    step = next((x for x in c.inputs if (m.table[(c.genesis, x)] or ("",))[0] == "Accept"), None)
    if step is None:
        return not_applicable({"family": family}, "no accept from genesis")
    path = work / f"{family}.loop.json"
    workspace = LW.LoopWorkspace(path, cli)
    workspace.init(contract=contract)
    state = workspace.load()
    subject = state["subject"]
    ctx = Z.Context(subject)
    decision = m.table[(c.genesis, step)]
    request = [n for n in subject["names"] if n not in subject["state"]]
    shown = []
    for name, v in zip(request, step):
        kind = dict(zip(subject["names"], subject["kinds"]))[name]
        table = {value: key for key, value in subject["variants"].get(name, {}).items()}
        shown.append(f"{name} = {('true' if v else 'false') if kind == 'Bool' else table.get(v, v)}")
    sets = ", ".join(f"{f} = {v}" for f, v in zip(subject["state"], decision[2]))
    body = (Z.HEADER["symbolic"] + "\n" + Z.subject_line(ctx.subject_ref, False) + "\nauthor model;\n"
            f"scenario one from genesis;\nstep one 1: {', '.join(shown)} -> accept {sets};\n")
    events = workspace.submit(body, "model")
    fact = events[-1]["symbolic"]
    detail = (events[-1].get("detail") or {})
    killed = ((detail.get("reject_everything") or {}).get("killed"))
    return judged("reject_everything", {
        "family": family, "scenario": events[1]["symbolic"] if len(events) > 1 else None, "fact": fact,
        "passed_on_core": "passed: all 1 steps" in fact, "reject_everything_killed": bool(killed)})


def markdown(report: dict) -> str:
    lines = ["# Loop judge fault controls", "", f"CLI `{report['cli']}`.", ""]
    for title, key, columns in (
            ("Teach-back: one reason changed", "teach_back",
             ["instance", "domain", "seconds", "case", "reason", "changed_to", "own", "changed", "region",
              "exceptions", "missing", "passed"]),
            ("An answer contradicting the core", "disagreement",
             ["instance", "domain", "seconds", "answer", "status", "laws", "false_alarm", "files_unchanged",
              "passed"]),
            ("A decomposition with one read removed", "read_removed",
             ["instance", "removed", "status", "suggested_reads", "named", "names_it", "passed"]),
            ("A decomposition with one wrong case", "wrong_case",
             ["instance", "case", "changed_to", "status", "blamed", "wrong", "region", "passed"]),
            ("The composed check", "composed",
             ["instance", "status", "inputs", "differ", "obligations", "planted_status", "planted_differ",
              "passed"]),
            ("The reject-everything core and one success scenario", "reject_everything",
             ["family", "scenario", "passed_on_core", "reject_everything_killed", "passed"])):
        lines += [f"## {title}", "", "| " + " | ".join(columns) + " | result |",
                  "|" + "---|" * (len(columns) + 1)]
        for row in report[key]:
            lines.append("| " + " | ".join(str(row.get(c, "")) for c in columns) + f" | {row.get('result', '')} |")
        lines.append("")
    lines += ["## Summary", "", f"Size envelope for teach-back and disagreement: {SIZE_ENVELOPE} tuples.", "",
              "| Control | Rows | Passed | Not applicable | Outside size envelope | Failed | Enough applicable |",
              "|---|---:|---:|---:|---:|---:|---|"]
    for key, (rows, passed, na, outside, bad, enough) in report["summary"].items():
        lines.append(f"| {key} | {rows} | {passed} | {na} | {outside} | {bad} | {enough} |")
    return "\n".join(lines) + "\n"


CONTROLS = ("teach_back", "disagreement", "read_removed", "wrong_case", "composed", "reject_everything")


def summarize(report: dict) -> bool:
    """Counts per control into ``report["summary"]``; whether every control passed: no failed row, and at
    least half of its rows (and at least one) applicable."""
    report["summary"], ok = {}, True
    for key in CONTROLS:
        rows = report[key]
        passed = sum(1 for r in rows if r.get("passed") is True)
        outside = sum(1 for r in rows if r.get("outside_size_envelope"))
        na = sum(1 for r in rows if r.get("passed") is None) - outside
        bad = sum(1 for r in rows if r.get("passed") is False)
        applicable = len(rows) - na - outside
        enough = applicable >= max(1, (len(rows) + 1) // 2)
        report["summary"][key] = (len(rows), passed, na, outside, bad, enough)
        ok = ok and bad == 0 and enough
    return ok


def self_test() -> list:
    """No CLI: each comparison on right and planted-wrong door results, stubbed and not; and a run whose doors
    refuse everywhere."""
    right = {
        "teach_back": {"own": "exact", "changed": "wrong", "exceptions": 4, "region": 4},
        "disagreement": {"status": "disagreement", "files_unchanged": True},
        "read_removed": {"removed": "action", "named": ["action"]},
        "wrong_case": {"status": "refuted", "blamed": "all", "wrong": 2},
        "composed": {"status": "accepted", "differ": 0, "obligations": "proved", "planted_status": "refuted",
                     "planted_differ": 3},
        "reject_everything": {"passed_on_core": True, "reject_everything_killed": True},
    }
    planted = {
        "teach_back": {"own": "exact", "changed": "exact", "exceptions": 0, "region": 4},
        "disagreement": {"status": "recorded", "files_unchanged": True},
        "read_removed": {"removed": "action", "named": ["authorized"]},
        "wrong_case": {"status": "meets-requirements", "blamed": None, "wrong": None},
        "composed": {"status": "accepted", "differ": 0, "obligations": "proved", "planted_status": "accepted",
                     "planted_differ": 0},
        "reject_everything": {"passed_on_core": True, "reject_everything_killed": False},
    }
    failures = []
    for key in CONTROLS:
        if judged(key, dict(right[key]))["passed"] is not True:
            failures.append(f"{key}: a right result failed")
        if judged(key, dict(planted[key]))["passed"] is not False:
            failures.append(f"{key}: a planted-wrong result passed")
    real = dict(COMPARE)
    try:
        for key in CONTROLS:
            COMPARE[key] = lambda row: True  # every door result looks right
        report = {key: [judged(key, dict(planted[key]))] for key in CONTROLS}
        if not summarize(report):
            failures.append("with the comparisons stubbed the planted results still fail: the comparison is not "
                            "what decides")
    finally:
        COMPARE.update(real)
    refusing = {key: [failed({"instance": "x"}, {"status": "refused", "detail": {"reason": "planted"}}, "planted")]
                for key in CONTROLS}
    if summarize(refusing):
        failures.append("a run whose doors refuse everywhere passed")
    vacuous = {key: [not_applicable({"instance": "x"}, "planted"), not_applicable({"instance": "y"}, "planted")]
               for key in CONTROLS}
    if summarize(vacuous):
        failures.append("a run with no applicable row passed")
    # the size envelope: outside it an inconclusive row neither passes nor fails, a pass is not counted, and a
    # wrong verdict still fails the run
    big = SIZE_ENVELOPE + 1
    for key in ENVELOPED:
        base = {k: [judged(k, dict(right[k])) for _ in range(3)] for k in CONTROLS}
        stopped = dict(base, **{key: base[key] + [enveloped(key, failed(
            {"instance": "big"}, {"status": "inconclusive"}, "planted"), big)]})
        if not summarize(stopped):
            failures.append(f"{key}: an inconclusive row outside the size envelope failed the run")
        counted = summarize(dict(base, **{key: base[key] + [enveloped(key, judged(key, dict(right[key])), big)]}))
        row = enveloped(key, judged(key, dict(right[key])), big)
        if not counted or row["passed"] is not None:
            failures.append(f"{key}: a pass outside the size envelope was counted as a pass")
        wrong = dict(base, **{key: base[key] + [enveloped(key, judged(key, dict(planted[key])), big)]})
        if summarize(wrong):
            failures.append(f"{key}: a wrong verdict outside the size envelope did not fail the run")
        inside = dict(base, **{key: base[key] + [enveloped(key, failed(
            {"instance": "small"}, {"status": "inconclusive"}, "planted"), SIZE_ENVELOPE)]})
        if summarize(inside):
            failures.append(f"{key}: an inconclusive row inside the size envelope did not fail the run")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--cli", type=Path)
    parser.add_argument("--work", type=Path, help="new scratch directory outside the source tree")
    parser.add_argument("--out", type=Path)
    parser.add_argument("--family", action="append", choices=FAMILIES)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--question-timing", action="store_true", help="run only the AQ K1 eight-answer timing control")
    options = parser.parse_args()
    if options.self_test:
        failures = self_test()
        print(json.dumps({"self_test": "passed" if not failures else "failed", "failures": failures}, indent=1))
        return 1 if failures else 0
    if options.work is None or options.out is None:
        parser.error("--work and --out are required without --self-test")
    cli = loop_doors.resolve_cli(options.cli)
    os.environ["ZENO_FCIS_CLI"] = str(cli)
    options.work.mkdir(parents=True, exist_ok=False)
    if options.question_timing:
        contract = options.work / "contract"
        instantiate_core.instantiate("approval-queue", {"K": 1}, contract, include_proof=False)
        row = question_timing_control(cli, contract, options.work)
        options.out.mkdir(parents=True, exist_ok=True)
        report = {"cli": str(cli), "question_timing": row, "authority": "none"}
        (options.out / "question-timing.json").write_text(json.dumps(report, indent=1, sort_keys=True) + "\n")
        print(json.dumps(report, indent=1))
        return 0 if row["passed"] else 1
    report = {"cli": str(cli), "teach_back": [], "disagreement": [], "read_removed": [], "wrong_case": [],
              "composed": [], "reject_everything": [], "authority": "none"}
    for family in options.family or FAMILIES:
        first = None
        for parameters in loop_doors.family_instances(family):
            name = loop_doors.instance_name(family, parameters)
            contract = options.work / "contracts" / name
            contract.parent.mkdir(parents=True, exist_ok=True)
            instantiate_core.instantiate(family, parameters, contract, include_proof=False)
            scratch = options.work / "scratch" / name
            scratch.mkdir(parents=True)
            fields = LW.fields_of(contract)
            size = domain_size(fields)
            report["teach_back"].append(enveloped("teach_back", teach_back_control(cli, name, contract, fields,
                                                                                    scratch), size))
            report["disagreement"].append(enveloped("disagreement", disagreement_control(cli, name, contract,
                                                                                         scratch), size))
            row, whole = read_removed_control(cli, name, contract, fields, scratch)
            report["read_removed"].append(row)
            report["wrong_case"].append(wrong_case_control(cli, name, contract, fields, scratch, whole))
            report["composed"].append(composed_control(cli, name, contract, fields, scratch, whole))
            first = first or contract
        workspaces = options.work / "workspaces"
        workspaces.mkdir(exist_ok=True)
        try:
            report["reject_everything"].append(reject_everything_control(cli, family, first, workspaces))
        except Exception as error:  # noqa: BLE001 - a row, never a crash
            report["reject_everything"].append({"family": family, "result": str(error)[:200], "passed": False})
        print(f"{family}: done", file=sys.stderr, flush=True)
    ok = summarize(report)
    options.out.mkdir(parents=True, exist_ok=True)
    (options.out / "judges-controls.json").write_text(json.dumps(report, indent=1, sort_keys=True) + "\n")
    (options.out / "judges-controls.md").write_text(markdown(report))
    print(json.dumps(report["summary"], indent=1))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
