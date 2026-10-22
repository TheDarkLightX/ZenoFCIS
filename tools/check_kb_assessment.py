#!/usr/bin/env python3
"""Replay the public frozen example and check private native assessment evidence."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
import shutil
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
SUITE = ROOT / "crates/zeno-fcis-cli/tests/data/knowledge-suite/suite.json"


def run(cli, root, name, arguments, expected=0):
    result = subprocess.run(["nice", "-n", "19", "ionice", "-c3", str(cli), "kb", *arguments],
                            stdin=subprocess.DEVNULL, capture_output=True, timeout=600, check=False,
                            env={k: v for k, v in os.environ.items() if k not in {"ZENO_FCIS_KB", "ZENO_FCIS_CHECKER_CONTROL"}})
    (root / (name + ".json")).write_bytes(result.stdout)
    (root / (name + ".stderr")).write_bytes(result.stderr)
    value = json.loads(result.stdout)
    accepted_exits = (expected,) if type(expected) is int else expected
    if result.returncode not in accepted_exits:
        raise AssertionError(f"{name}: exit {result.returncode}, expected {expected}; {value.get('reason')}")
    print(json.dumps({"check": name, "exit": result.returncode, "status": value.get("status")}), flush=True)
    return value


def development(cli, root, suite):
    episodes = []
    tasks = []
    for cap in (1, 2):
        source = ROOT / f"crates/zeno-fcis-cli/tests/fixtures/i128-door-programs/bounded-counter-c{cap}.original.zcve"
        data = source.read_bytes()
        task = json.loads(json.dumps(suite["tasks"][0]))
        task["name"] = f"development-c{cap}"
        task["subject"] = {"path": source.name, "sha256": hashlib.sha256(data).hexdigest()}
        task["sha256"] = hashlib.sha256(json.dumps([task["name"], task["subject"]["sha256"], task["scope"], task["seed"], task["budget"], task["proposal"]], separators=(",", ":")).encode()).hexdigest()
        tasks.append(task["sha256"])
        pair = []
        for on in (False, True):
            name = f"development-{cap}-{'on' if on else 'off'}"
            request = root / (name + "-request.json")
            output = root / (name + "-output.json")
            request.write_text(json.dumps({"task": task, "original": list(data), "actions": [{"skip": ["canonical"], "order": []}] if on else []}))
            run(cli, root, name, ["assessment-task", "--request", str(request), "--out", str(output)])
            detail = json.loads(output.read_text())["detail"]
            pair.append(detail)
            work = dict(sorted((k, v) for k, v in detail["work"].items() if type(v) is int and k != "knowledge_consultation_work"))
            family = {"subject": task["subject"]["sha256"], "session": name, "actor": "controlled-native-run", "tool": "v250", "environment": "sandbox"}
            episode = {"schema": "zeno-fcis/kb-episode/1", "task": task["sha256"], "subject": task["subject"]["sha256"],
                       "scope": task["scope"], "door": "pipeline", "steps": detail["execution_trace"], "advice": [], "counterexamples": [],
                       "stopping_bound": detail.get("reason"), "verdict": "accepted", "receipt_sha256": detail["best_check"]["receipt_sha256"],
                       "work": work, "checker_sha256": hashlib.sha256(cli.read_bytes()).hexdigest(),
                       "engine_sha256": detail["binding"]["implementation_sha256"], "family": family,
                       "result_cost": [detail["best"]["nodes"], detail["best"]["bytes"]], "model_calls": 0, "tokens": 0, "money_reserved": 0, "owner_questions": 0}
            episodes.append(episode)
        assert pair[0]["status"] == pair[1]["status"] == "qualified"
        assert [pair[0]["best"][k] for k in ("nodes", "bytes")] == [pair[1]["best"][k] for k in ("nodes", "bytes")]
        assert sum(v for v in pair[1]["work"].values() if type(v) is int) < sum(v for v in pair[0]["work"].values() if type(v) is int)
    return episodes, tasks


def examples(cli, root, resume_from=None, attempts=1):
    if root.is_relative_to(ROOT):
        raise ValueError("private output must be outside checkout")
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    suite = json.loads(SUITE.read_text())
    layer = root / "layer.json"
    if resume_from is not None:
        if resume_from.is_symlink() or not resume_from.is_file() or resume_from.stat().st_size > 65536 or resume_from.parent.stat().st_mode & 0o077:
            raise ValueError("private resume layer required")
        descriptor_bytes = resume_from.read_bytes()
        descriptor = json.loads(descriptor_bytes)
        state_hash = descriptor["state"]["sha256"]
        if type(state_hash) is not str or len(state_hash) != 64 or any(c not in "0123456789abcdef" for c in state_hash):
            raise ValueError("resume state digest")
        state_name = "state-" + state_hash + ".json"
        state_path = resume_from.parent / state_name
        if descriptor["schema"] != "zeno-fcis/kb-layer/2" or descriptor["state"]["path"] != state_name or state_path.is_symlink() or not state_path.is_file() or state_path.stat().st_size > 16*1024*1024:
            raise ValueError("resume descriptor binding")
        state_bytes = state_path.read_bytes()
        if hashlib.sha256(state_bytes).hexdigest() != state_hash:
            raise ValueError("resume state hash")
        (root / state_name).write_bytes(state_bytes)
        layer.write_bytes(descriptor_bytes)
        state = json.loads(state_bytes)
        rejected = {"assessment_sha256": next(c["assessment"] for c in state["candidates"] if c["id"] == "broad-shortcut")}
        (root / "resume.json").write_text(json.dumps({"source_descriptor_sha256":hashlib.sha256(descriptor_bytes).hexdigest(),"state_sha256":state_hash,"history_preserved":True}) + "\n")
        run(cli, root, "resume-check", ["check", "--kb", str(layer)])
    else:
        initial_path = SUITE.with_name("initial-resource-suite.json")
        initial = json.loads(initial_path.read_text())
        earlier_initial = json.loads(SUITE.with_name("initial-suite.json").read_text())
        episodes, development_tasks = development(cli, root, suite)
        run(cli, root, "init", ["init", "--out", str(layer)])
        episode_file = root / "development-episodes.json"
        episode_file.write_text(json.dumps(episodes))
        run(cli, root, "development-episodes", ["ingest-episodes", "--source", str(episode_file), "--kb", str(layer)])

        def proposal(name, stage, scope, subjects, tasks):
            families = [{"subject": subject, "session": "proposing-subject-" + str(i), "actor": "controlled-native-run",
                         "tool": "v250", "environment": "sandbox"} for i, subject in enumerate(subjects)]
            return {"id": name, "statement_sha256": "", "subject": "pipeline", "claim": "", "scope": scope, "effect": "helps",
                    "action": {"skip": [stage], "order": []}, "proposing_tasks": tasks, "families": families,
                    "episodes": [], "parents": [], "dependencies": {}, "state": "proposed", "assessment": None,
                    "observed_work": 0, "origin": "development-proposal"}

        def curate(name, candidates):
            path = root / (name + "-candidates.json")
            path.write_text(json.dumps({"schema": "zeno-fcis/kb-candidates/1", "candidates": candidates, "isa": []}))
            return run(cli, root, name, ["curate", "--source", str(path), "--kb", str(layer)])

        broad = proposal("broad-shortcut", "resubstitute", {"capability": ["immutable-proposal"]}, initial["proposing_subjects"], development_tasks)
        curate("curate-initial", [broad])
        rejected = run(cli, root, "initial-negative", ["assess", "--suite", str(initial_path), "--candidate", "broad-shortcut", "--kb", str(layer)])
        assert rejected["assessment"]["decision"] == "reject"
        parent = next(iter(rejected["assessment"]["candidates"]))
        candidates = []
        for name, stage, scope in (("helpful", "canonical", {"capability": ["immutable-proposal"], "family": ["bounded-counter"]}),
                                    ("harmful", "resubstitute", {"capability": ["immutable-proposal"], "family": ["bounded-counter"]}),
                                    ("inapplicable", "canonical", {"family": ["absent-class"]})):
            row = proposal(name, stage, scope, suite["proposing_subjects"], development_tasks + [t["sha256"] for t in initial["tasks"] + earlier_initial["tasks"]])
            if name == "helpful":
                row["scope"]["family"] = [" bounded-counter "]
            row["parents"] = [parent]
            candidates.append(row)
        curate("curate", candidates)
    def assess(name, candidate=None):
        args = ["assess", "--suite", str(SUITE), "--kb", str(layer)]
        if candidate is not None:
            args += ["--candidate", candidate]
        for attempt in range(1, attempts+1):
            report = run(cli, root, f"{name}-attempt-{attempt}", args, (0, 2))
            stop = report["assessment"]["resource_stop"]
            if stop is None:
                shutil.copyfile(root / f"{name}-attempt-{attempt}.json", root / f"{name}.json")
                return report
            if stop not in {"kb-task-refused", "kb-task-timeout"}:
                raise AssertionError(f"{name}: {stop}")
        raise AssertionError(f"{name}: operational attempts exhausted; every stop remains inconclusive")

    results = {}
    for name, decision in (("helpful", "promote"), ("harmful", "reject"), ("inapplicable", "inconclusive")):
        report = assess(name, name)
        assert report["assessment"]["decision"] == decision, (name, report["assessment"]["reason"])
        results[name] = report
    aggregate = assess("aggregate")
    assert aggregate["aggregate"]["on"]["solved"] == aggregate["aggregate"]["off"]["solved"]
    assert aggregate["aggregate"]["on"]["work"] < aggregate["aggregate"]["off"]["work"]
    assert aggregate["aggregate"]["prior_assessment_work"] > 0
    before_replay = layer.read_bytes()
    run(cli, root, "assessment-replay", ["replay-assessment", "--assessment", aggregate["assessment_sha256"], "--suite", str(SUITE), "--kb", str(layer)])
    assert layer.read_bytes() == before_replay
    run(cli, root, "check", ["check", "--kb", str(layer)])
    explanation = run(cli, root, "explain", ["explain", "helpful", "--kb", str(layer)])
    assert explanation["episodes"] and explanation["justification_chain"]
    advice = run(cli, root, "deduplicated-advice", ["advise", "--task", 'task(subject,pipeline).task(family,bounded-counter).task(capability,immutable-proposal).', "--kb", str(layer), "--kb", str(layer)])
    assert len(advice["assessed_advice"]) == 1
    board = run(cli, root, "leaderboard", ["leaderboard", "--kb", str(layer)])
    assert board["points_never_promote_veto_or_exclude"] is True and board["leaderboards"]
    run(cli, root, "telemetry", ["telemetry", "--kb", str(layer)])
    stop = run(cli, root, "total-budget", ["assess", "--suite", str(SUITE), "--candidate", "inapplicable", "--total-budget", "1", "--kb", str(layer)], 2)
    assert stop["assessment"]["decision"] == "inconclusive"
    original = SUITE.parent / suite["tasks"][0]["subject"]["path"]
    certified = run(cli, root, "certify", ["certify", "--original", str(original), "--candidate", str(original), "--scope", '{"family":["bounded-counter"]}', "--kb", str(layer)])
    fact = certified["fact_sha256"]
    run(cli, root, "reuse", ["reuse", "--fact", fact, "--subject", str(original), "--kb", str(layer)])
    other = SUITE.parent / suite["tasks"][1]["subject"]["path"]
    refusal = run(cli, root, "wrong-subject", ["reuse", "--fact", fact, "--subject", str(other), "--kb", str(layer)], 1)
    assert refusal["reason"] == "kb-fact-subject-or-checker"
    legacy = root / "legacy.kb"
    legacy.write_text('edge(a,b). path(X,Y):-edge(X,Y).\n')
    old_descriptor = root / "legacy.layer.json"
    old_descriptor.write_text(json.dumps({"schema":"zeno-fcis/kb-layer/1","knowledge":{"path":legacy.name,"sha256":hashlib.sha256(legacy.read_bytes()).hexdigest()},"validations":[]}))
    migrated = root / "migrated.layer.json"
    run(cli, root, "migration", ["migrate", "--source", str(old_descriptor), "--out", str(migrated)])
    legacy_query = run(cli, root, "migrated-query", ["query", "path(X,Y)", "--kb", str(migrated)])
    assert legacy_query["answers"] == [{"X":"a","Y":"b"}] and legacy_query["pending_legacy_layers"] == 1
    summary = {"suite_file_sha256": hashlib.sha256(SUITE.read_bytes()).hexdigest(),
               "examples": {k: {"decision": v["assessment"]["decision"], "assessment_sha256": v["assessment_sha256"]} for k, v in results.items()},
               "aggregate": aggregate["aggregate"], "assessment_sha256": aggregate["assessment_sha256"],
               "initial_negative": rejected["assessment_sha256"], "provenance_limit": "controlled contributor labels do not authenticate independent actors"}
    (root / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--resume-from", type=Path, help="Copy this example's private layer, retaining prior work and inconclusive records")
    parser.add_argument("--attempts", type=int, choices=range(1, 4), default=1, help="Bounded whole-assessment retries for operational refusals; every attempt is recorded")
    args = parser.parse_args()
    examples(args.cli.resolve(), args.out.resolve(), args.resume_from, args.attempts)


if __name__ == "__main__":
    main()
