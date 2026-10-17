"""Shell routing for program loops. LLM-generated; all judgments are Rust CLI results.

No Python term evaluator, optimizer, strategy ranker, answer path or adoption.
The workspace stores evidence and typed facts without changing its incumbent.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path

from behavior import Refusal
import loop_doors as doors
import loop_language as Z

PIPELINE = "zeno-fcis/loop-pipeline/1"


def _original(workspace, state) -> Path:
    workspace._contract(state)  # Preserve the existing source-revision gate.
    path = workspace.data / state.get("original_program", "original.zcve")
    if not path.resolve().is_relative_to(workspace.data.resolve()) or path.is_symlink() or not path.is_file():
        raise Refusal("program-loop-original-outside-workspace")
    with path.open("rb") as stream:
        body = stream.read(65_537)
    if len(body) > 65_536 or hashlib.sha256(body).hexdigest() != state["subject"]["original_sha256"]:
        raise Refusal("program-loop-original-binding-changed")
    return path


def _packet(cli, original: Path, expected_original: str | None = None) -> dict:
    code, report = doors.run(cli, ["loop", "pipeline", "--original", original, "--packet"])
    if code != 0 or report.get("status") != "packet":
        raise Refusal("program-loop-packet-unavailable")
    detail = report.get("detail", {})
    if any(type(detail.get(field)) is not str or len(detail[field]) != 64
           for field in ("original_sha256", "implementation_sha256")):
        raise Refusal("program-loop-packet-binding")
    if expected_original is not None and detail["original_sha256"] != expected_original:
        raise Refusal("program-loop-original-binding-changed")
    return detail


def _proposal(packet: dict, vocabulary: dict, outputs: list[str], parameters: dict, stages=None) -> dict:
    value = {"schema": PIPELINE, "original_sha256": packet["original_sha256"],
             "implementation_sha256": packet["implementation_sha256"],
             "expressions": doors.document(vocabulary, outputs, parameters)}
    if stages is not None:
        value["stages"] = list(stages)
    return value


def _reply(workspace, state, ctx, id_, seq, code, report):
    detail = report.get("detail", {})
    status = report.get("status", "refused")
    if code == 0 and status == "qualified":
        if detail.get("binding", {}).get("original_sha256") != state["subject"]["original_sha256"]:
            raise Refusal("program-loop-result-subject-changed")
        check, best = detail.get("best_check", {}), detail.get("best", {})
        if check.get("verdict") != "equivalent" or check.get("replay") != "matched":
            raise Refusal("program-loop-qualification-without-replay")
        if report.get("door") == "rules" and detail.get("improved") is not True:
            workspace._fact(state, ctx, id_, "original checked and replayed; no improving rewrite checked; proposal only", seq,
                            {"report": report})
            return
        workspace._reply(state, ctx, "accepted", id_, seq, {"report": report},
                         tuples=check["domain_tuples"], nodes=best["nodes"])
        workspace._fact(state, ctx, id_, "program checked and replayed on the declared domain; proposal only", seq)
    else:
        workspace._fact(state, ctx, id_, f"{status}: program loop returned exit {code}; proposal only", seq,
                        {"report": report})


def route(workspace, state, ctx, kind: str, acts: list, seq: int):
    from loop_workspace import vocabulary
    id_ = acts[0].id
    original = _original(workspace, state)
    cli = workspace.cli(state)
    expressions = [a for a in acts if a.kind == kind]
    if not expressions:
        raise Refusal("program-loop-needs-expressions")
    if kind == "mine":
        terms = [{"kind": ctx.output_kind(a.get("output")), "expression": Z.print_symbolic(a.get("expr"))}
                 for a in expressions]
        path = workspace._document(state, f"terms-{id_}",
                                   {"schema": "zeno-fcis/loop-terms/1", "vocabulary": vocabulary(state["subject"]),
                                    "parameters": state["subject"]["parameters"], "terms": terms})
        code, report = doors.run(cli, ["loop", "rules", "--original", original, "--terms", path])
        return _reply(workspace, state, ctx, id_, seq, code, report)
    outputs = [Z.print_symbolic(n) for n in workspace._outputs(ctx, expressions)]
    if kind == "pipeline":
        configs = [a for a in acts if a.kind == "stages"]
        if len(configs) > 1:
            raise Refusal("program-loop-duplicate-stages")
        stages = configs[0].get("stages") if configs else ("complete", "repair", "canonical")
        path = workspace._document(state, f"pipeline-{id_}",
                                   _proposal(_packet(cli, original, state["subject"]["original_sha256"]), vocabulary(state["subject"]), outputs,
                                             state["subject"]["parameters"], stages))
        code, report = doors.run(cli, ["loop", "pipeline", "--original", original, "--proposal", path])
        return _reply(workspace, state, ctx, id_, seq, code, report)
    if not state.get("family"):
        raise Refusal("strategy-needs-a-family-workspace-with-distinct-instances")
    configs = [a for a in acts if a.kind == "explore"]
    if len(configs) > 1:
        raise Refusal("program-loop-duplicate-explore")
    depth = configs[0].get("depth") if configs else 1
    manifest_path = workspace.data / state["family"]["manifest"]
    if (not manifest_path.resolve().is_relative_to(workspace.data.resolve())
            or manifest_path.is_symlink() or not manifest_path.is_file()):
        raise Refusal("strategy-manifest-outside-workspace")
    with manifest_path.open("rb") as stream:
        body = stream.read(65_537)
    if len(body) > 65_536:
        raise Refusal("strategy-manifest-too-large")
    manifest = json.loads(body)
    instances = manifest.get("instances", [])
    if len(instances) != 2:
        raise Refusal("strategy-runtime-requires-two-instances")
    # Explicit unfitted entries are evaluation-only. With none, reserve the
    # final listed instance before any trial runs. This is selection holdout,
    # not an unseen-family or statistical generalization claim.
    explicit = any(entry.get("fit") is False for entry in instances)
    development, evaluation = [], []
    for index, entry in enumerate(instances):
        source = manifest_path.parent / entry["program"]
        if not source.resolve().is_relative_to(manifest_path.parent.resolve()) or source.is_symlink() or not source.is_file():
            raise Refusal("strategy-program-outside-family")
        packet = _packet(cli, source)
        with source.open("rb") as stream:
            program_bytes = stream.read(65_537)
        if len(program_bytes) > 65_536 or hashlib.sha256(program_bytes).hexdigest() != packet["original_sha256"]:
            raise Refusal("strategy-original-binding-changed")
        # The Rust task-manifest loader admits only relative paths below its
        # directory. Store a fresh bounded snapshot next to that manifest.
        number = state["next_document"]
        state["next_document"] += 1
        copy = workspace.data / "documents" / f"{number:04d}-strategy-original.zcve"
        copy.parent.mkdir(parents=True, exist_ok=True)
        with copy.open("xb") as stream:
            stream.write(program_bytes)
        task = {"original": copy.name,
                "proposal": _proposal(packet, manifest["vocabulary"], outputs, entry["parameters"])}
        heldout = entry.get("fit") is False if explicit else index == len(instances) - 1
        (evaluation if heldout else development).append(task)
    if not development or not evaluation:
        raise Refusal("strategy-needs-nonempty-development-and-evaluation")
    path = workspace._document(state, f"strategy-{id_}",
                               {"schema": "zeno-fcis/loop-strategy/1", "development": development,
                                "evaluation": evaluation})
    code, report = doors.run(cli, ["loop", "strategy", "--tasks", path, "--depth", depth])
    _reply(workspace, state, ctx, id_, seq, code, report)
