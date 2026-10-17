"""One strategy surface: fixed modes and owner-only reading confirmation.

All computation/verdicts come from Rust. Persisted problems are owned copies.
This extends the existing locked owner answer gate, never the MCP tool list.
"""
from copy import deepcopy
import json
from behavior import Refusal
import loop_doors as D
import loop_language as Z

KINDS = ("split-by-cases", "diagnose", "ask-when-it-matters")


def strict_problem(text):
    def fields(pairs):
        out = {}
        for key, value in pairs:
            if key in out:
                raise Refusal("strategy-duplicate-field")
            out[key] = value
        return out
    if type(text) is not str or len(text.encode("utf-8")) > 65536 or not text.isascii():
        raise Refusal("strategy-problem-bound")
    try:
        value = json.loads(text, object_pairs_hook=fields,
                           parse_float=lambda _: (_ for _ in ()).throw(Refusal("strategy-fractional-number")),
                           parse_constant=lambda _: (_ for _ in ()).throw(Refusal("strategy-nonfinite-number")))
    except (ValueError, RecursionError) as error:
        raise Refusal("strategy-problem-json") from error
    if type(value) is not dict:
        raise Refusal("strategy-problem-object")
    if any(key in value for key in ("answers", "owner_answers", "owner_confirmations", "primitives")):
        raise Refusal("strategy-problem-no-owner-or-primitive-input")
    return value


def run(workspace, state, kind, problem):
    if kind not in KINDS:
        raise Refusal("strategy-kind")
    workspace._contract(state)  # Reuse the bound revision gate.
    path = workspace._document(state, "strategy-mode", deepcopy(problem))
    arguments = ["loop", "strategy", "--kind", kind, "--problem", path]
    if kind == "split-by-cases":
        from loop_composable import _original
        arguments += ["--original", _original(workspace, state)]
    return D.run(workspace.cli(state), arguments)


def record(workspace, state, ctx, id_, seq, kind, problem, code, report):
    detail = report.get("detail", {})
    if code == 0 and report.get("status") == "qualified":
        if kind != "split-by-cases":
            raise Refusal("strategy-mode-cannot-qualify-program")
        check = detail.get("best_check", {})
        if check.get("verdict") != "equivalent" or check.get("replay") != "matched":
            raise Refusal("strategy-qualification-without-replay")
        if detail.get("binding_original_sha256") != state["subject"]["original_sha256"]:
            raise Refusal("strategy-result-subject-changed")
        workspace._reply(state, ctx, "accepted", id_, seq, {"report": report},
                         tuples=check["domain_tuples"], nodes=detail["best"]["nodes"])
        return
    workspace._fact(state, ctx, id_, f"{report.get('status', 'refused')}: {kind}; conditional proposal only", seq,
                    {"report": report})
    question = detail.get("question")
    if kind == "ask-when-it-matters" and code == 0 and report.get("status") == "question":
        if type(question) is not dict or not question.get("holds") or not question.get("does_not_hold"):
            raise Refusal("strategy-reading-question-shape")
        qid = workspace._fresh_question_id(state)
        state["questions"][qid] = {"status": "open", "asked_by": "proposer", "options": [],
            "strategy_reading": {"run": id_, "kind": kind, "problem": deepcopy(problem),
                                 "question": deepcopy(question), "binding": deepcopy(detail.get("binding"))}}
        workspace._fact(state, ctx, qid,
                        f"owner reading confirmation: {question['id']} {(question.get('text') or '')} at {question['world']}; answer any if it holds, none if it does not hold", seq)


def route(workspace, state, ctx, acts, seq):
    if len(acts) != 1:
        raise Refusal("strategy-mode-one-act")
    a = acts[0]
    kind, problem = a.get("mode"), strict_problem(a.get("problem"))
    code, report = run(workspace, state, kind, problem)
    record(workspace, state, ctx, a.id, seq, kind, problem, code, report)


def confirm(workspace, state, ctx, a, seq):
    """Called only after the existing owner channel and open-question checks.

    Fresh problem/question/implementation bindings are checked before the owner
    verdict filters an owned list. Failed confirmation changes no readings.
    """
    question = state["questions"][a.id]
    choice = a.get("choice")
    if choice not in ("any", "none"):
        raise Refusal("strategy-reading-answer-use-any-or-none")
    pending = deepcopy(question["strategy_reading"])
    code, fresh = run(workspace, state, pending["kind"], pending["problem"])
    detail = fresh.get("detail", {})
    if (code != 0 or fresh.get("status") != "question" or detail.get("question") != pending["question"]
            or detail.get("binding") != pending["binding"]):
        raise Refusal("strategy-reading-question-changed")
    q = pending["question"]
    index = q.get("statement")
    groups = pending["problem"].get("readings")
    if type(index) is not int or type(groups) is not list or not 0 <= index < len(groups):
        raise Refusal("strategy-reading-question-index")
    alternatives = groups[index].get("alternatives")
    kept = q["holds" if choice == "any" else "does_not_hold"]
    if (type(alternatives) is not list or type(kept) is not list or not kept
            or any(type(k) is not int or not 0 <= k < len(alternatives) for k in kept)):
        raise Refusal("strategy-reading-question-choices")
    groups[index]["alternatives"] = [deepcopy(alternatives[k]) for k in kept]
    # The fresh question was acquired above. A later search stop cannot invent
    # an answer, but it does not erase the owner's actual confirmation.
    question["status"] = "answered"
    question["answer"] = choice
    question["owner_reading_confirmation"] = {"holds": choice == "any", "question": deepcopy(q),
        "binding": deepcopy(pending["binding"]), "channel": "owner"}
    question["confirmed_readings_problem"] = deepcopy(pending["problem"])
    workspace._fact(state, ctx, a.id, "owner reading verdict confirmed; conditional readings filtered", seq)
    workspace.save(state)
    next_code, next_report = run(workspace, state, pending["kind"], pending["problem"])
    record(workspace, state, ctx, pending["run"], seq, pending["kind"], pending["problem"], next_code, next_report)
