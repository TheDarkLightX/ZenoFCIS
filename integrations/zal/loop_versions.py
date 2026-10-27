"""LLM-generated requirements workspace shell. Rust computes and judges; this
module parses, persists and displays. Owner answers use LoopWorkspace's existing
terminal channel. No new MCP answer or stop path exists.
"""
from __future__ import annotations

import json
import re
import tempfile
from pathlib import Path

from behavior import Refusal
import report_numbers as N
import loop_doors as doors
import loop_language as Z

SCHEMA = "zeno-fcis/loop-version-space/1"
JUDGE_KINDS = ("version-count", "version-question", "version-determined", "version-conflict", "version-stopped")
MODEL_KINDS = ("versions", "version-suggest")
OWNER_KINDS = ("version-answer", "version-stop")
ACT_KINDS = MODEL_KINDS + OWNER_KINDS + JUDGE_KINDS


def is_statement(line: str, english: bool) -> bool:
    """Claim only this family's complete statement prefixes."""
    words = line.split()
    if not words:
        return False
    if not english:
        return words[0] in ACT_KINDS
    return words[0] == "Versions" or (len(words) > 1 and words[0] == "Version" and
        words[1] in ("count", "question", "suggestion", "answer", "determined", "conflict", "stop", "stopped"))


def strict_json(text: str):
    def pairs(items):
        obj = {}
        for key, value in items:
            if key in obj:
                raise Refusal("duplicate JSON property")
            obj[key] = value
        return obj

    def noninteger(_):
        raise Refusal("integral JSON required")
    try:
        return json.loads(text, object_pairs_hook=pairs, parse_float=noninteger, parse_constant=noninteger)
    except (ValueError, RecursionError) as exc:
        raise Refusal("invalid bounded JSON") from exc


def tuple_text(values) -> str:
    return "[" + ",".join(str(x) for x in values) + "]"


def tuple_values(text: str) -> tuple:
    if not re.fullmatch(r"\[(?:-?\d+(?:,-?\d+)*)?\]", text):
        raise Refusal("version-space-complete-tuple-required")
    parts = text[1:-1].split(",") if text != "[]" else []
    if len(parts) > 32 or any(len(x.lstrip("-")) > 19 for x in parts):
        raise Refusal("version-space-tuple-bound")
    values = tuple(int(x) for x in parts)
    if len(values) > 32 or any(not Z.I64[0] <= x <= Z.I64[1] for x in values):
        raise Refusal("version-space-tuple-bound")
    return values


def _natural(text, cap):
    if len(text) > len(str(cap)):
        raise Refusal("version-space-count-bound")
    value = int(text)
    if not 0 <= value <= cap:
        raise Refusal("version-space-count-bound")
    return value


def _tuple_kind(values: tuple, ctx: Z.Context, output: bool):
    kinds = ctx.output_kinds if output else [ctx.kinds[n] for n in ctx.names]
    if len(values) != len(kinds) or any(k == "Bool" and v not in (0, 1) for k, v in zip(kinds, values)):
        raise Refusal("version-space-tuple-kind")
    domains = ctx.subject.get("output_domains") if output else None
    ranges = [(0, 1) if d["kind"] == "Bool" else (int(d["min"]), int(d["max"])) for d in domains] if output else ctx.subject["ranges"]
    if any(not lo <= v <= hi for v, (lo, hi) in zip(values, ranges)):
        raise Refusal("version-space-value-outside-domain")
    return values


def render_act(a, ctx, english):
    k, i, g = a.kind, a.id, a.get
    if k == "versions":
        return f"Versions {i}: ask about the remaining meanings." if english else f"versions {i};"
    if k == "version-count":
        return f"Version count {i}: {g('count')} meanings (proposal)." if english else f"version-count {i}: {g('count')};"
    if k == "version-question":
        answers = g("answers")
        shown = " or ".join(f"answer {tuple_text(v)} leaves {n} meanings" for v, n in answers) if english else \
            " | ".join(f"{tuple_text(v)} -> {n}" for v, n in answers)
        return (f"Version question {i} at {tuple_text(g('input'))}: {shown}." if english else
                f"version-question {i} at {tuple_text(g('input'))}: {shown};")
    if k in ("version-answer", "version-suggest"):
        if english:
            word = "answer" if k == "version-answer" else "suggestion"
            return f"Version {word} {i}: {tuple_text(g('output'))}" + (" (advisory)." if k == "version-suggest" else ".")
        return f"{k} {i}: {tuple_text(g('output'))};"
    if k == "version-determined":
        return (f"Version determined {i}: program {g('sha256')} checked on {g('domain')} inputs." if english else
                f"version-determined {i}: {g('sha256')} on {g('domain')} inputs;")
    if k == "version-conflict":
        ids = "[" + ",".join(g("requirements")) + "]"
        return (f"Version conflict {i}: requirements {ids} exclude every meaning in this sketch (proposal)." if english else
                f"version-conflict {i}: {ids};")
    if k in ("version-stop", "version-stopped"):
        return (f"Version {'stop' if k == 'version-stop' else 'stopped'} {i}: keep the confirmed requirements."
                if english else f"{k} {i};")
    return None


def parse_act(line, ctx, english):
    if not is_statement(line, english):
        return None
    if ctx.tag != "requirements":
        raise Refusal("version-space-act-needs-requirements-workspace")
    # New acts retain the specification's 64-character identifier bound.
    id_ = r"[A-Za-z_][A-Za-z0-9_]{0,63}"
    patterns = [
        ("versions", rf"Versions ({id_}): ask about the remaining meanings\." if english else rf"versions ({id_});"),
        ("version-count", rf"Version count ({id_}): (\d+) meanings \(proposal\)\." if english else rf"version-count ({id_}): (\d+);"),
        ("version-question", rf"Version question ({id_}) at (\[[^\]]*\]): (.+)\." if english else rf"version-question ({id_}) at (\[[^\]]*\]): (.+);"),
        ("version-answer", rf"Version answer ({id_}): (\[[^\]]*\])\." if english else rf"version-answer ({id_}): (\[[^\]]*\]);"),
        ("version-suggest", rf"Version suggestion ({id_}): (\[[^\]]*\]) \(advisory\)\." if english else rf"version-suggest ({id_}): (\[[^\]]*\]);"),
        ("version-determined", rf"Version determined ({id_}): program ([0-9a-f]{{64}}) checked on (\d+) inputs\." if english else rf"version-determined ({id_}): ([0-9a-f]{{64}}) on (\d+) inputs;"),
        ("version-conflict", rf"Version conflict ({id_}): requirements (\[[^\]]*\]) exclude every meaning in this sketch \(proposal\)\." if english else rf"version-conflict ({id_}): (\[[^\]]*\]);"),
        ("version-stop", rf"Version stop ({id_}): keep the confirmed requirements\." if english else rf"version-stop ({id_});"),
        ("version-stopped", rf"Version stopped ({id_}): keep the confirmed requirements\." if english else rf"version-stopped ({id_});"),
    ]
    for kind, pattern in patterns:
        m = re.fullmatch(pattern, line)
        if not m:
            continue
        args = m.groups()[1:]
        fields = {}
        if kind == "version-count":
            fields["count"] = _natural(args[0], 16384)
            if fields["count"] > 16384:
                raise Refusal("version-space-count-bound")
        elif kind == "version-question":
            fields["input"] = _tuple_kind(tuple_values(args[0]), ctx, False)
            answers = []
            for part in args[1].split(" or " if english else " | "):
                p = re.fullmatch(r"answer (\[[^\]]*\]) leaves (\d+) meanings" if english else r"(\[[^\]]*\]) -> (\d+)", part)
                if not p or not 1 <= _natural(p[2], 16384) <= 16384:
                    raise Refusal("version-space-answer-bucket")
                answers.append((_tuple_kind(tuple_values(p[1]), ctx, True), _natural(p[2], 16384)))
            if len(answers) < 2 or len({v for v, _ in answers}) != len(answers) or sum(n for _, n in answers) > 16384:
                raise Refusal("version-space-partition")
            fields["answers"] = tuple(answers)
        elif kind in ("version-answer", "version-suggest"):
            fields["output"] = _tuple_kind(tuple_values(args[0]), ctx, True)
        elif kind == "version-determined":
            fields.update(sha256=args[0], domain=_natural(args[1], 65536))
            if not 1 <= fields["domain"] <= 65536:
                raise Refusal("version-space-domain-bound")
        elif kind == "version-conflict":
            text = args[0][1:-1]
            ids = tuple(text.split(",")) if text else ()
            if len(ids) > 64 or len(set(ids)) != len(ids) or any(not re.fullmatch(id_, x) for x in ids):
                raise Refusal("version-space-requirement-ids")
            fields["requirements"] = ids
        return Z.act(kind, m[1], **fields)
    return None


def _subject(spec, tag="requirements"):
    names = [f["name"] for f in spec["inputs"]]
    kinds = [f["domain"]["kind"] for f in spec["inputs"]]
    ranges = [[0, 1] if f["domain"]["kind"] == "Bool" else [f["domain"]["min"], f["domain"]["max"]] for f in spec["inputs"]]
    return {"tag": tag, "revision": Z.digest(spec), "names": names, "kinds": kinds, "ranges": ranges,
            "outputs": [d["kind"] for d in spec["outputs"]], "output_domains": spec["outputs"], "state": [],
            "variants": {}, "reasons": [], "roots": {n: "command" for n in names}, "parameters": {},
            "cases": [], "laws": [], "paths": {}, "reason_ids": {}, "family_subject": None}


def _run_question(cli, path, limits):
    arguments = ["loop", "question", "--specification", path]
    for flag, value in sorted(limits.items()):
        arguments.extend(["--" + flag.replace("_", "-"), str(value)])
    code, report = doors.run(cli, arguments)
    if report.get("status") == "inconclusive" or (code == 2 and report.get("status") != "question"):
        return {"status": "inconclusive", "authority": "none", "detail": report.get("detail", {})}
    expected_exit = {"determined": 0, "question": 2, "no-completion": 1}
    if report.get("status") not in expected_exit or code != expected_exit[report["status"]]:
        raise Refusal("requirements-only question refused: " + str((report.get("detail") or {}).get("reason", report.get("status"))))
    return report


def _analyze(workspace, state):
    version = state["version_space"]
    if version["stopped"]:
        return {"status": "stopped", "authority": "none", "detail": {"count": None, "question": None, "candidate": None}}
    # Recompute from the requirements on every operation. Stored results never
    # supply count, decision or acceptance authority.
    path = workspace._document(state, "version-space", version["specification"])
    return _run_question(workspace.cli(state), path, version["limits"])


def _post(workspace, state, ctx, id_, seq, report):
    status, detail = report["status"], report.get("detail", {})
    if status == "inconclusive":
        workspace._fact(state, ctx, id_, "inconclusive: no count, question or candidate; no attempt spent", seq, report)
        return
    if status == "stopped":
        workspace._reply(state, ctx, "version-stopped", id_, seq, report)
        return
    workspace._reply(state, ctx, "version-count", id_, seq, report, count=detail["count"])
    if status == "question":
        q = detail["question"]
        old = state["questions"].get(q["id"])
        if old is not None and (old.get("status") != "open" or old.get("version_question") != q):
            raise Refusal("version-space-question-id-collision")
        state["questions"][q["id"]] = {"status": "open", "asked_by": "judge", "kind": "version-space", "version_question": q}
        workspace._reply(state, ctx, "version-question", q["id"], seq, report, input=N.integers(q["input"]),
                         answers=tuple((N.integers(a["output"]), a["remaining"]) for a in q["answers"]))
    elif status == "determined":
        candidate = detail["candidate"]
        if candidate.get("status") != "checked" or not candidate.get("checks") or any(
                c.get("verdict") != "equivalent" or c.get("replay") != "matched" for c in candidate["checks"]):
            raise Refusal("version-space-singleton-not-checked")
        workspace._reply(state, ctx, "version-determined", id_, seq, report, sha256=candidate["sha256"], domain=candidate["domain"])
    elif status == "no-completion":
        workspace._reply(state, ctx, "version-conflict", id_, seq, report, requirements=tuple(detail["conflict"]["requirements"]))


def validate_snapshot(state):
    version = state["version_space"]
    if (type(version.get("stopped")) is not bool or not isinstance(version.get("limits"), dict)
            or set(version["limits"]) - {"max_work", "max_domain", "max_meanings", "max_cells", "max_conflict_subsets"}
            or any(type(v) is not int or v < 0 for v in version["limits"].values())
            or len(version.get("suggestions", [])) > 64
            or state["subject"] != _subject(version["specification"])):
        raise Refusal("version-space-workspace-drift")


def initialize(workspace, specification: Path, limits: dict):
    # Admission uses exclusively owned temporary input. A malformed declaration
    # or Rust refusal must not create persistent workspace state or block retry.
    try:
        if specification.stat().st_size > 65536:
            raise Refusal("version-space-document-too-large")
        with specification.open(encoding="ascii") as stream:
            text = stream.read(65537)
        if len(text) > 65536:
            raise Refusal("version-space-document-too-large")
        spec = strict_json(text)
        if not isinstance(spec, dict) or spec.get("schema") != SCHEMA:
            raise Refusal("version-space-schema")
        state = {"profile": Z.PROFILE, "subject": _subject(spec), "cli": str(doors.resolve_cli(workspace.cli_given)),
                 "questions": {}, "events": [], "next_document": 1, "next_question": 1, "meanings": {}, "refuted": {},
                 "version_space": {"specification": spec, "limits": dict(limits), "stopped": False, "suggestions": []},
                 "authority": "none"}
        ctx = Z.Context(state["subject"])
        validate_snapshot(state)
        with tempfile.TemporaryDirectory(prefix=".zal-versions-", dir=workspace.path.parent) as temporary:
            path = doors.write_new(Path(temporary) / "specification.json", spec)
            report = _run_question(workspace.cli(state), path, state["version_space"]["limits"])
        _post(workspace, state, ctx, "initial", 0, report)
        view = workspace.view(state)
    except Refusal:
        raise
    except (OSError, UnicodeError, KeyError, TypeError, ValueError, RecursionError) as exc:
        raise Refusal("version-space-invalid-initialization") from exc
    workspace.data.mkdir(parents=True)
    workspace.save(state)
    return view


def handle(workspace, state, ctx, a, seq):
    if not state.get("version_space"):
        raise Refusal("version-space-act-needs-requirements-workspace")
    version = state["version_space"]
    if a.kind == "versions":
        _post(workspace, state, ctx, a.id, seq, _analyze(workspace, state))
    elif a.kind == "version-stop":
        version["stopped"] = True
        for q in state["questions"].values():
            if q.get("kind") == "version-space" and q["status"] == "open":
                q["status"] = "stopped"
        workspace._reply(state, ctx, "version-stopped", a.id, seq)
    elif a.kind in ("version-suggest", "version-answer"):
        report = _analyze(workspace, state)
        q = (report.get("detail") or {}).get("question")
        if report["status"] == "inconclusive":
            _post(workspace, state, ctx, a.id, seq, report)
            return
        if not q or q["id"] != a.id:
            raise Refusal("version-space-stale-or-absent-question")
        values = _tuple_kind(a.get("output"), ctx, True)
        if a.kind == "version-suggest":
            if len(version["suggestions"]) >= 64:
                raise Refusal("version-space-suggestion-limit")
            version["suggestions"].append({"question": a.id, "output": list(values), "label": "advisory model suggestion; not a requirement"})
            workspace._fact(state, ctx, a.id, "advisory suggestion; no requirement added", seq)
            return
        if len(version["specification"]["requirements"]) >= 64:
            raise Refusal("version-space-requirement-limit")
        # Own a fresh next specification. Retain an in-domain answer even if
        # it excludes this grammar; a later budget stop never erases the answer.
        proposal = json.loads(json.dumps(version["specification"]))
        proposal["requirements"].append({"id": "answer_" + a.id, "input": list(N.integers(q["input"])), "output": list(values)})
        version["specification"] = proposal
        state["subject"] = _subject(proposal)
        state["questions"][a.id]["status"] = "answered"
        state["questions"][a.id]["answer"] = list(values)
        _post(workspace, state, Z.Context(state["subject"]), a.id, seq, _analyze(workspace, state))
    else:
        raise Refusal("requirements-workspace-only-version-acts")


def answer(workspace, state, ctx, question, option):
    q = state["questions"].get(question)
    if not q or q.get("kind") != "version-space" or q["status"] != "open":
        raise Refusal("no open version-space question")
    if option.startswith("["):
        output = tuple_values(option)
    elif re.fullmatch(r"[1-9][0-9]{0,4}", option):
        answers = q["version_question"]["answers"]
        index = int(option) - 1
        if not 0 <= index < len(answers):
            raise Refusal("version-space-answer-option")
        output = N.integers(answers[index]["output"])
    else:
        raise Refusal("answer with an option number or a complete [output,tuple]; use loop stop to end")
    _tuple_kind(output, ctx, True)
    statement = f"version-answer {question}: {tuple_text(output)};"
    source = workspace._owner_document(state, ctx, statement, False)
    return workspace._submit(state, ctx, Z.parse(source, ctx))


def view(workspace, state):
    questions = []
    for id_, saved in state["questions"].items():
        if saved["status"] != "open" or saved.get("kind") != "version-space":
            continue
        q = saved["version_question"]
        questions.append({"id": id_, "asked_by": "judge", "input": q["input"],
                          "options": [{"n": k + 1, "output": a["output"], "remaining": a["remaining"],
                                       "english": f"answer {tuple_text(a['output'])} leaves {a['remaining']} meanings (proposal)",
                                       "symbolic": tuple_text(a["output"])} for k, a in enumerate(q["answers"])]})
    return {"schema": "zeno-fcis/zal-loop-view/2", "workspace": str(workspace.path), "zal_profile": Z.PROFILE,
            "subject": list(Z.Context(state["subject"]).subject_ref), "mode": "version-space", "incumbent": None,
            "event_count": len(state["events"]), "open_questions": questions, "authority": "none",
            "events": [{k: e[k] for k in ("seq", "author", "english", "symbolic")} for e in state["events"][-40:]]}


def packet(state, english):
    ctx = Z.Context(state["subject"])
    lines = [Z.HEADER["english" if english else "symbolic"], Z.subject_line(ctx.subject_ref, english),
             "The author is the model." if english else "author model;", "",
             "# requirements-only finite sketch; no existing core", "# counts and conflicts are proposals"]
    for item in state["version_space"]["specification"]["inputs"]:
        lines.append("# input " + item["name"] + ": " + json.dumps(item["domain"], sort_keys=True))
    for k, sketch in enumerate(state["version_space"]["specification"]["sketch"]):
        lines.append(f"# out{k} = {sketch}")
    for req in state["version_space"]["specification"]["requirements"]:
        lines.append("# requirement " + json.dumps(req, sort_keys=True))
    lines.append(Z.render_act(Z.act("versions", "next"), ctx, english))
    return "\n".join(lines) + "\n"
