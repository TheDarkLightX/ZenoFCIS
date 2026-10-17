"""LLM-generated learn workspace shell and two ZAL surfaces.

Rust enumerates the declared vocabulary and its existing evaluator checks
lowered examples. This module parses, renders, stores and calls that command.
It computes no meanings or verdicts. Initial declarations belong to the owner;
models can ask for analysis and suggest answers. Only the existing terminal
owner channel confirms an output tuple. Channel separation is cooperative,
not authentication. There is no MCP answer route or vocabulary mutation act.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import re
import tempfile

from behavior import Refusal
import loop_doors as doors
import loop_language as Z

SCHEMA = "zeno-fcis/loop-learn/1"
JUDGE_KINDS = ("learn-question", "learn-determined", "learn-outside", "learn-ambiguous", "learn-inconclusive")
OWNER_KINDS = ("learn-answer",)
MODEL_KINDS = ("learn", "learn-suggest")
KINDS = MODEL_KINDS + OWNER_KINDS + JUDGE_KINDS
FIELDS = {"learn": (), "learn-suggest": ("output",), "learn-answer": ("output",),
          "learn-question": ("counts", "input"), "learn-determined": ("examples", "sha256"),
          "learn-outside": ("examples",), "learn-ambiguous": ("counts",), "learn-inconclusive": ()}
LIMIT_FLAGS = {"max_work": 2_000_000, "max_tuples": 4096, "max_meanings": 4096,
               "max_cells": 1_000_000, "max_conflict_subsets": 10_000}


def strict_json(text):
    def unique(pairs):
        out = {}
        for key, val in pairs:
            if key in out:
                raise Refusal("learn-duplicate-json-field")
            out[key] = val
        return out

    def constant(_):
        raise Refusal("learn-nonfinite-json-number")

    try:
        return json.loads(text, object_pairs_hook=unique, parse_constant=constant)
    except (ValueError, TypeError, RecursionError) as e:
        raise Refusal("learn-json") from e


def tuple_text(values):
    return "[" + ",".join(str(v) for v in values) + "]"


def tuple_values(text):
    values = strict_json(text)
    if type(values) is not list or len(values) > 8 or any(type(v) is not int or not Z.I64[0] <= v <= Z.I64[1] for v in values):
        raise Refusal("learn-tuple")
    return tuple(values)


def validate_tuple(values, ctx, output):
    if type(values) is not tuple or any(type(v) is not int for v in values):
        raise Refusal("learn-tuple-types")
    if output:
        domains = ctx.subject.get("output_domains", [])
        spans = [(0, 1) if d["kind"] == "Bool" else (int(d["min"]), int(d["max"])) for d in domains]
    else:
        spans = ctx.subject["ranges"]
    if len(values) != len(spans) or any(not int(span[0]) <= v <= int(span[1]) for v, span in zip(values, spans)):
        raise Refusal("learn-tuple-domain")
    return values


def naturals(text, maximum=4096, length=8):
    values = strict_json(text)
    if type(values) is not list or len(values) > length or any(type(v) is not int or not 0 <= v <= maximum for v in values):
        raise Refusal("learn-natural-list")
    return tuple(values)


def _count_tuple(values, ctx):
    if len(values) != len(ctx.outputs) or not values or any(n == 0 for n in values):
        raise Refusal("learn-counts")
    return values


def parse_statement(line, ctx, english):
    """Return a learn act, or None to leave the existing profile untouched."""
    patterns = [
        ("learn", rf"Learn ({Z.ID})\." if english else rf"learn ({Z.ID});"),
        ("learn-answer", rf"Learn answer ({Z.ID}): (\[[^\]]*\])\." if english else rf"learn-answer ({Z.ID}): (\[[^\]]*\]);"),
        ("learn-suggest", rf"Learn suggestion ({Z.ID}): (\[[^\]]*\]) \(advisory\)\." if english else rf"learn-suggest ({Z.ID}): (\[[^\]]*\]);"),
        ("learn-question", rf"Learn question ({Z.ID}): when (\[[^\]]*\]), (\[[^\]]*\]) meanings remain \(proposal\)\." if english else rf"learn-question ({Z.ID}): (\[[^\]]*\]) remaining (\[[^\]]*\]);"),
        ("learn-determined", rf"Learn determined ({Z.ID}): program ([0-9a-f]{{64}}) reproduces (\d+) examples; uniqueness is a vocabulary proposal\." if english else rf"learn-determined ({Z.ID}): ([0-9a-f]{{64}}) on (\d+) examples;"),
        ("learn-outside", rf"Learn outside ({Z.ID}): examples (\[[^\]]*\]) exclude this bounded vocabulary \(proposal\)\." if english else rf"learn-outside ({Z.ID}): (\[[^\]]*\]);"),
        ("learn-ambiguous", rf"Learn ambiguous ({Z.ID}): (\[[^\]]*\]) meanings remain \(proposal\)\." if english else rf"learn-ambiguous ({Z.ID}): (\[[^\]]*\]);"),
        ("learn-inconclusive", rf"Learn inconclusive ({Z.ID}): no completed enumeration\." if english else rf"learn-inconclusive ({Z.ID});"),
    ]
    for kind, pattern in patterns:
        m = re.fullmatch(pattern, line)
        if m is None:
            continue
        fields = {}
        if kind in ("learn-answer", "learn-suggest"):
            fields["output"] = validate_tuple(tuple_values(m[2]), ctx, True)
        elif kind == "learn-question":
            fields["input"] = validate_tuple(tuple_values(m[2]), ctx, False)
            fields["counts"] = _count_tuple(naturals(m[3]), ctx)
        elif kind == "learn-determined":
            if len(m[3]) > 2 or not 0 <= int(m[3]) <= 64:
                raise Refusal("learn-example-count")
            fields.update(sha256=m[2], examples=int(m[3]))
        elif kind == "learn-outside":
            ids = naturals(m[2], 63, 64)
            if len(set(ids)) != len(ids):
                raise Refusal("learn-conflict-examples")
            fields["examples"] = ids
        elif kind == "learn-ambiguous":
            fields["counts"] = _count_tuple(naturals(m[2]), ctx)
        return Z.act(kind, m[1], **fields)
    return None


def render_act(a, ctx, english):
    k, i, g = a.kind, a.id, a.get
    if k not in KINDS or not re.fullmatch(Z.ID, i) or tuple(name for name, _ in a.fields) != FIELDS[k]:
        raise Refusal("learn-act-shape")
    if k == "learn":
        text = f"Learn {i}." if english else f"learn {i};"
    elif k in ("learn-answer", "learn-suggest"):
        out = tuple_text(validate_tuple(g("output"), ctx, True))
        text = (f"Learn answer {i}: {out}." if k == "learn-answer" else f"Learn suggestion {i}: {out} (advisory).") if english else f"{k} {i}: {out};"
    elif k == "learn-question":
        point, counts = tuple_text(validate_tuple(g("input"), ctx, False)), tuple_text(g("counts"))
        text = f"Learn question {i}: when {point}, {counts} meanings remain (proposal)." if english else f"learn-question {i}: {point} remaining {counts};"
    elif k == "learn-determined":
        text = f"Learn determined {i}: program {g('sha256')} reproduces {g('examples')} examples; uniqueness is a vocabulary proposal." if english else f"learn-determined {i}: {g('sha256')} on {g('examples')} examples;"
    elif k == "learn-outside":
        text = f"Learn outside {i}: examples {tuple_text(g('examples'))} exclude this bounded vocabulary (proposal)." if english else f"learn-outside {i}: {tuple_text(g('examples'))};"
    elif k == "learn-ambiguous":
        text = f"Learn ambiguous {i}: {tuple_text(g('counts'))} meanings remain (proposal)." if english else f"learn-ambiguous {i}: {tuple_text(g('counts'))};"
    else:
        text = f"Learn inconclusive {i}: no completed enumeration." if english else f"learn-inconclusive {i};"
    if parse_statement(text, ctx, english) != a:
        raise Refusal("learn-act-fields")
    return text


def subject(spec):
    inputs = spec["inputs"]
    names = [i["name"] for i in inputs]
    ranges = [[0, 1] if i["domain"]["kind"] == "Bool" else [i["domain"]["min"], i["domain"]["max"]] for i in inputs]
    return {"tag": "learn", "revision": Z.digest(spec), "names": names,
            "kinds": [i["domain"]["kind"] for i in inputs], "ranges": ranges,
            "outputs": [o["kind"] for o in spec["outputs"]], "output_domains": [dict(d) for d in spec["outputs"]],
            "state": [], "variants": {}, "reasons": [], "roots": {n: "command" for n in names},
            "parameters": {}, "cases": [], "laws": [], "paths": {}, "reason_ids": {}, "family_subject": None}


def specification(state):
    learn = state["learn"]
    spec = json.loads(json.dumps(learn["initial"]))
    spec["examples"] = spec.get("examples", []) + json.loads(json.dumps(learn["confirmed"]))
    return spec


def validate_snapshot(state):
    learn = state["learn"]
    if (type(learn) is not dict or type(learn.get("initial")) is not dict
            or state["subject"] != subject(learn["initial"])
            or type(learn.get("confirmed")) is not list or len(specification(state)["examples"]) > 64
            or learn.get("limits") != LIMIT_FLAGS):
        raise Refusal("learn-workspace-drift")


def run(cli, path):
    code, report = doors.run(cli, ["loop", "learn", "--spec", path])
    status, detail = report.get("status"), report.get("detail", {})
    expected_exit = {"determined": 0, "question": 3, "outside-vocabulary": 1, "ambiguous": 1, "inconclusive": 2}
    if status not in expected_exit or code != expected_exit[status] or report.get("authority") != "none":
        raise Refusal("learn-command-refused:" + str(detail.get("reason", status)))
    if status == "inconclusive":
        if any(detail.get(k) is not None for k in ("counts", "question", "candidate", "core")):
            raise Refusal("learn-budget-prefix-result")
        # A supervisor stop has no door detail or measured invocation usage.
        # Own a normalized envelope without inventing any proposal or counters.
        detail = dict(detail)
        if "reason" not in detail and "reason" in report:
            detail["reason"] = report["reason"]
        report = {**report, "detail": detail}
    elif (detail.get("enumeration") != "proposal" or detail.get("complete") is not True
            or detail.get("specification_sha256") != hashlib.sha256(path.read_bytes()).hexdigest()):
        raise Refusal("learn-report-subject")
    return report


def initialize(workspace, path):
    """Owner initialization; use temporary owned input BEFORE creating persistent files."""
    try:
        if path.stat().st_size > 65536:
            raise Refusal("learn-document-too-large")
        with path.open(encoding="ascii") as stream:
            text = stream.read(65537)
        if len(text) > 65536:
            raise Refusal("learn-document-too-large")
        initial = strict_json(text)
        if type(initial) is not dict or initial.get("schema") != SCHEMA:
            raise Refusal("learn-schema")
        cli = workspace.cli()
        with tempfile.TemporaryDirectory(prefix=".zal-learn-", dir=workspace.path.parent) as temporary:
            staged = doors.write_new(Path(temporary) / "declaration.json", initial)
            report = run(cli, staged)
        state = {"profile": Z.PROFILE, "subject": subject(initial), "cli": str(cli),
                 "learn": {"initial": initial, "confirmed": [], "suggestions": [], "limits": dict(LIMIT_FLAGS)},
                 "session": None, "family": None, "incumbent": None, "original": None,
                 "questions": {}, "refuted": {}, "meanings": {}, "next_question": 1, "next_document": 1,
                 "events": [], "authority": "none"}
        ctx = Z.Context(state["subject"])
        seq = workspace._record(state, ctx, "owner", Z.act("learn", "start"))["seq"]
        post(workspace, state, ctx, "start", seq, report)
        workspace.data.mkdir()
        workspace.save(state)
        return workspace.view(state)
    except (OSError, UnicodeError, KeyError, TypeError, RecursionError) as e:
        raise Refusal("learn-initialization") from e


def analyze(workspace, state):
    validate_snapshot(state)
    path = workspace._document(state, "learn", specification(state))
    return run(workspace.cli(state), path)


def post(workspace, state, ctx, id_, seq, report):
    status, detail = report["status"], report["detail"]
    # Only the current completed proposal can supply a pending question.
    for q in state["questions"].values():
        if q.get("kind") == "learn" and q["status"] == "open":
            q["status"] = "superseded"
    if status == "inconclusive":
        workspace._reply(state, ctx, "learn-inconclusive", id_, seq, report)
    elif status == "question":
        q = detail["question"]
        qid = workspace._fresh_question_id(state)
        state["questions"][qid] = {"kind": "learn", "status": "open", "asked_by": "judge",
                                    "input": list(q["input"]), "binding": Z.digest(specification(state)),
                                    "proposal": q, "counts": list(detail["counts"])}
        workspace._reply(state, ctx, "learn-question", qid, seq, report,
                         input=tuple(q["input"]), counts=tuple(detail["counts"]))
    elif status == "determined":
        candidate = detail.get("candidate") or {}
        checked = candidate.get("examples_check") or {}
        if (candidate.get("status") != "examples-reproduced" or checked.get("decided_by") != "library-evaluator"
                or checked.get("verdict") != "reproduced" or type(checked.get("checked")) is not int
                or checked["checked"] != len(specification(state)["examples"])):
            raise Refusal("learn-determined-not-example-checked")
        workspace._reply(state, ctx, "learn-determined", id_, seq, report,
                         sha256=candidate["sha256"], examples=checked["checked"])
    elif status == "outside-vocabulary":
        workspace._reply(state, ctx, "learn-outside", id_, seq, report, examples=tuple(detail["core"]))
    elif status == "ambiguous":
        workspace._reply(state, ctx, "learn-ambiguous", id_, seq, report, counts=tuple(detail["counts"]))


def handle(workspace, state, ctx, a, seq):
    if not state.get("learn"):
        raise Refusal("learn-act-needs-learn-workspace")
    validate_snapshot(state)
    if a.kind == "learn":
        post(workspace, state, ctx, a.id, seq, analyze(workspace, state))
        return
    if a.kind not in ("learn-suggest", "learn-answer"):
        raise Refusal("learn-workspace-only-learn-acts")
    q = state["questions"].get(a.id)
    if (not q or q["status"] != "open" or q.get("kind") != "learn"
            or q.get("binding") != Z.digest(specification(state))):
        raise Refusal("learn-stale-or-absent-question")
    output = validate_tuple(a.get("output"), ctx, True)
    if a.kind == "learn-suggest":
        if len(state["learn"]["suggestions"]) >= 64:
            raise Refusal("learn-suggestion-limit")
        state["learn"]["suggestions"].append({"question": a.id, "output": list(output),
                                               "label": "advisory model suggestion; not an example"})
        workspace._fact(state, ctx, a.id, "advisory suggestion recorded; only the owner can confirm an example", seq)
        return
    spec = specification(state)
    if len(spec["examples"]) >= 64:
        raise Refusal("learn-example-limit")
    if any(e["input"] == q["input"] for e in spec["examples"]):
        raise Refusal("learn-input-already-confirmed")
    # Preserve every in-domain owner answer, including one outside the buckets.
    # Later enumeration failure cannot erase an answer or spend an attempt.
    state["learn"]["confirmed"].append({"input": list(q["input"]), "output": list(output)})
    q["status"], q["answer"] = "answered", list(output)
    workspace._fact(state, ctx, a.id, "owner example confirmed; every final program must reproduce it", seq)
    workspace.save(state)
    post(workspace, state, ctx, a.id, seq, analyze(workspace, state))


def answer(workspace, state, ctx, question, option):
    output = validate_tuple(tuple_values(option), ctx, True)
    statement = f"learn-answer {question}: {tuple_text(output)};"
    doc = Z.parse(workspace._owner_document(state, ctx, statement, False), ctx)
    return workspace._submit(state, ctx, doc)


def view(workspace, state):
    questions = []
    for id_, q in state["questions"].items():
        if q["status"] != "open" or q.get("kind") != "learn":
            continue
        a = Z.act("learn-question", id_, input=tuple(q["input"]), counts=tuple(q["counts"]))
        ctx = Z.Context(state["subject"])
        questions.append({"id": id_, "asked_by": "judge", "kind": "learn", "options": [],
                          "english": Z.render_act(a, ctx, True), "symbolic": Z.render_act(a, ctx, False),
                          "input": list(q["input"]), "buckets": json.loads(json.dumps(q["proposal"]["buckets"])),
                          "workflow": "owner: loop answer " + id_ + " '[output,tuple]'; model suggestions are advisory"})
    return {"schema": "zeno-fcis/zal-loop-view/1", "workspace": str(workspace.path), "zal_profile": Z.PROFILE,
            "mode": "learn", "subject": list(Z.Context(state["subject"]).subject_ref), "incumbent": None,
            "original_nodes": None, "events": [{k: e[k] for k in ("seq", "author", "english", "symbolic")}
                                                  for e in state["events"][-40:]],
            "event_count": len(state["events"]), "open_questions": questions, "authority": "none"}


def packet(state, english):
    ctx = Z.Context(state["subject"])
    spec = specification(state)
    lines = [Z.HEADER["english" if english else "symbolic"], Z.subject_line(ctx.subject_ref, english),
             "The author is the model." if english else "author model;", "",
             "# fixed owner-declared vocabulary; uniqueness and partitions are proposals",
             "# " + json.dumps(spec.get("vocabulary", {}), ensure_ascii=True, sort_keys=True),
             f"# maximum vocabulary expression size {spec.get('max_size', 7)}"]
    for inp in spec["inputs"]:
        lines.append("# input " + inp["name"] + ": " + json.dumps(inp["domain"], sort_keys=True))
    for j, output in enumerate(spec["outputs"]):
        lines.append(f"# output {j}: " + json.dumps(output, sort_keys=True))
    for example in spec["examples"]:
        lines.append("# confirmed example " + tuple_text(example["input"]) + " -> " + tuple_text(example["output"]))
    lines.append(Z.render_act(Z.act("learn", "next"), ctx, english))
    return "\n".join(lines) + "\n"
