"""Reproduce the bounded co-search qualification packet (research only).

Usage (from integrations/zal): python3 -B run_representation_cosearch.py
A completed finite replay is NOT native-ZAL integration or a general theorem.
"""
from __future__ import annotations

from hashlib import sha256
import json
from pathlib import Path

import representation_cosearch as research
from test_representation_cosearch import make_task

HERE = Path(__file__).resolve().parent
FIXTURE = HERE / "test-data" / "representation_cosearch_cases.json"


def artifact_hash(path: Path) -> str:
    return sha256(path.read_bytes()).hexdigest()


def run():
    manifest = json.loads(FIXTURE.read_text(encoding="utf-8"))
    if manifest.get("schema") != "zeno-fcis/representation-cosearch-cases/1":
        raise ValueError("fixture schema mismatch")
    sources = {name: artifact_hash(HERE / name) for name in
               ("discovery_loop.py", "representation_cosearch.py",
                "test_representation_cosearch.py", "test-data/representation_cosearch_cases.json")}
    training = {}
    result = {"schema": "zeno-fcis/representation-cosearch-experiment/1",
              "authority": "none", "sources_sha256": sources, "development": [],
              "evaluation": [], "negative": [], "claims": {
                  "verified": "exhaustive finite input rows under declared integer/Boolean semantics",
                  "unverified": "unbounded-domain correctness, ML predictive generalization, performance speedup, native integration"}}
    for case in manifest["development"]:
        task = make_task(case)
        report = research.search(task)
        if report.candidate is None or not research.replay(task, report.candidate):
            raise RuntimeError("development candidate refused: " + task.name)
        training[task.name] = report
        result["development"].append({
            "id": case["id"], "task_sha256": task.identity(), "rows": len(task.rows),
            "baseline": report.baseline.status, "feature": report.candidate.features[-1].pretty(),
            "feature_nodes": report.candidate.features[-1].size,
            "weights": report.candidate.separator.weights, "bias": report.candidate.separator.bias,
            "replayed": True, "generated_semantic_features": report.generated,
            "evaluated_feature_candidates": report.enumerated, "grammar_complete": report.complete,
            "cegis_refutations": len(report.candidate_trace.counterexample_indices),
            "bdd_nodes": report.bdd_nodes, "bdd_orders": report.bdd_orders})
    for case in manifest["evaluation"]:
        task = make_task(case)
        trace, receipt = research.reuse_feature(training[case["source"]], task)
        result["evaluation"].append({
            "id": task.name, "source_feature_from": case["source"], "rows": len(task.rows),
            "task_sha256": task.identity(), "refitted_procedure": trace.separator is not None,
            "frozen_feature": training[case["source"]].candidate.features[-1].pretty(),
            "replayed": receipt is not None and research.replay(task, receipt),
            "cegis_refutations": len(trace.counterexample_indices)})
    for case in manifest["negative"]:
        task = make_task(case)
        report = research.search(task)
        result["negative"].append({
            "id": task.name, "task_sha256": task.identity(),
            "baseline": report.baseline.status, "lift_found": report.candidate is not None,
            "grammar_complete": report.complete, "bdd_nodes": report.bdd_nodes,
            "bdd_orders": report.bdd_orders, "rows": len(task.rows)})
    if (not all(entry["replayed"] and entry["grammar_complete"] for entry in result["development"])
            or not all(entry["replayed"] for entry in result["evaluation"])
            or not all(entry["grammar_complete"] and not entry["lift_found"] and entry["bdd_nodes"] is not None
                       for entry in result["negative"])):
        raise RuntimeError("experiment qualification failed")
    return result


if __name__ == "__main__":
    print(json.dumps(run(), sort_keys=True, indent=2))
