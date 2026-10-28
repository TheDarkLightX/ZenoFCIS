#!/usr/bin/env python3
"""Check feasibility-record structure/bindings, never algorithm truth or authority."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re

AXES = ("correctness", "asymptotic_complexity", "concrete_resource_model",
        "empirical_crossover", "assurance_preservation")
KINDS = {"proof", "replay", "measurement", "analysis", "review", "qualification", "artifact", "workload", "toolchain"}
DIGEST = re.compile(r"[0-9a-f]{64}\Z")
REVISION = re.compile(r"(?:[0-9a-f]{40}|[0-9a-f]{64})\Z")
MAX_RECORD = 2 * 1024 * 1024
MAX_EVIDENCE = 64 * 1024 * 1024


def need(condition, message):
    if not condition:
        raise ValueError(message)


def fields(value, names, label):
    need(type(value) is dict and set(value) == set(names), label + ": unexpected/missing fields")


def text(value, label):
    need(type(value) is str and bool(value.strip()) and len(value) <= 16000, label + ": expected bounded text")


def strings(value, label):
    need(type(value) is list and len(value) <= 256, label + ": expected bounded list")
    for item in value:
        text(item, label)


def check(record, evidence_root=None):
    fields(record, ("schema", "claim", "candidate", "baseline", "workloads", "components", "negative_results", "disposition"), "record")
    need(record["schema"] == "zeno-fcis/computational-feasibility/2", "unsupported assessment version")
    text(record["claim"], "claim")
    references = []

    def evidence(rows):
        need(type(rows) is list and len(rows) <= 256, "expected bounded evidence list")
        for ref in rows:
            fields(ref, ("path", "sha256", "kind"), "evidence")
            text(ref["path"], "evidence path")
            parts = ref["path"].split("/")
            need(all(p not in ("", ".", "..") for p in parts) and
                 not any(c in ref["path"] for c in "\\:\x00?#") and
                 not any(ord(c) < 32 for c in ref["path"]), "unsafe evidence path")
            need(type(ref["sha256"]) is str and DIGEST.fullmatch(ref["sha256"]), "invalid evidence SHA-256")
            need(type(ref["kind"]) is str and ref["kind"] in KINDS, "unknown evidence kind")
            references.append(ref)
        return {ref["kind"] for ref in rows}

    def identity(value, names):
        fields(value, names, "backend identity")
        for key in ("name", "revision", "artifact_sha256"):
            if value[key] is not None:
                text(value[key], key)
        need(value["revision"] is None or REVISION.fullmatch(value["revision"]), "invalid full source revision")
        need(value["artifact_sha256"] is None or DIGEST.fullmatch(value["artifact_sha256"]), "invalid artifact SHA-256")

        if value["artifact"] is not None:
            evidence([value["artifact"]])
            need(value["artifact"]["kind"] == "artifact" and value["artifact"]["sha256"] == value["artifact_sha256"],
                 "backend artifact identity differs from retained byte reference")
        toolchain = value["toolchain"]
        if toolchain is not None:
            fields(toolchain, ("description", "path", "sha256"), "toolchain descriptor")
            text(toolchain["description"], "toolchain description")
            evidence([{"path": toolchain["path"], "sha256": toolchain["sha256"], "kind": "toolchain"}])

    candidate = record["candidate"]
    identity(candidate, ("name", "revision", "artifact_sha256", "artifact", "toolchain"))
    baseline = record["baseline"]
    identity(baseline, ("name", "revision", "artifact_sha256", "artifact", "toolchain", "qualification_scope", "qualification_evidence",
                        "selection_rationale", "selection_evidence", "inventory_as_of"))
    for key in ("qualification_scope", "selection_rationale", "inventory_as_of"):
        if baseline[key] is not None:
            text(baseline[key], key)
    qualification_kinds = evidence(baseline["qualification_evidence"])
    selection_kinds = evidence(baseline["selection_evidence"])
    need(not qualification_kinds or bool(qualification_kinds & {"qualification", "proof", "replay"}),
         "baseline qualification needs qualification/proof/replay evidence")
    need(not selection_kinds or bool(selection_kinds & {"review", "analysis"}),
         "baseline selection needs inventory review/analysis")
    workloads = record["workloads"]
    fields(workloads, ("representative", "held_out", "used_for_selection", "frozen_plan_evidence"), "workloads")
    sets, stable_ids = {}, {}
    unbound_workloads = 0
    for role in ("representative", "held_out", "used_for_selection"):
        rows = workloads[role]
        need(type(rows) is list and len(rows) <= 4096, "invalid workload list")
        ids, digests = set(), set()
        for row in rows:
            fields(row, ("id", "sha256", "path"), "workload")
            text(row["id"], "workload identity")
            need(type(row["sha256"]) is str and DIGEST.fullmatch(row["sha256"]), "invalid workload digest")
            need(row["id"] not in ids and row["sha256"] not in digests, "duplicate workload identity/content")
            need(row["id"] not in stable_ids or stable_ids[row["id"]] == row["sha256"],
                 "conflicting content for stable workload identity")
            stable_ids[row["id"]] = row["sha256"]
            if row["path"] is None:
                unbound_workloads += 1
            else:
                evidence([{"path": row["path"], "sha256": row["sha256"], "kind": "workload"}])
            ids.add(row["id"]); digests.add(row["sha256"])
        sets[role] = ids, digests
    for role in ("representative", "used_for_selection"):
        need(not any(a & b for a, b in zip(sets[role], sets["held_out"])), "held-out workload was used in development/representative selection")
    evidence(workloads["frozen_plan_evidence"])
    fields(record["components"], AXES, "five independent components")
    for axis in AXES:
        assessment = record["components"][axis]
        fields(assessment, ("status", "analysis", "evidence", "open_questions"), axis)
        need(assessment["status"] in ("supported", "refuted", "unknown"), "invalid axis status")
        text(assessment["analysis"], axis + " analysis")
        strings(assessment["open_questions"], axis + " questions")
        kinds = evidence(assessment["evidence"])
        if assessment["status"] != "unknown":
            need(bool(kinds), axis + ": assessed status requires evidence")
            if axis == "empirical_crossover":
                need("measurement" in kinds, "empirical crossover needs measured evidence")
        else:
            need(bool(assessment["open_questions"]), axis + ": unknown requires an open question")
        if assessment["status"] == "supported":
            required = {"correctness": {"proof", "replay"}, "asymptotic_complexity": {"proof", "analysis"},
                        "concrete_resource_model": {"measurement", "analysis"}, "empirical_crossover": {"measurement"},
                        "assurance_preservation": {"proof", "replay"}}[axis]
            need(bool(kinds & required), axis + ": missing applicable evidence kind")
            if axis == "assurance_preservation":
                need("review" in kinds, "assurance preservation requires explicit trust-boundary review")
    negatives = record["negative_results"]
    need(type(negatives) is list and len(negatives) <= 256, "invalid negative-result list")
    need(not any(record["components"][axis]["status"] == "refuted" for axis in AXES) or bool(negatives),
         "refuted component requires a retained negative result")
    for item in negatives:
        fields(item, ("observation", "scope", "classification", "cause_status", "explanation", "evidence", "next_test"), "negative result")
        for key in ("observation", "scope", "explanation", "next_test"):
            text(item[key], key)
        need(item["classification"] in ("fundamental", "implementation-specific", "workload-specific", "mixed", "unresolved"), "unknown limitation class")
        need(item["cause_status"] in ("supported", "provisional", "unknown"), "invalid diagnosis status")
        kinds = evidence(item["evidence"])
        need(bool(kinds), "negative observation needs retained evidence")
        if item["classification"] == "fundamental":
            need(item["cause_status"] == "supported" and "proof" in kinds, "timing alone does not establish a fundamental limitation")
    need(record["disposition"] in ("research-only", "negative-result", "request-independent-review"), "unknown research disposition")
    need(len(references) <= 4096, "too many evidence references")
    if evidence_root is not None:
        root = Path(evidence_root)
        need(not root.is_symlink() and root.is_dir(), "invalid evidence root")
        root = root.resolve()
        checked = {}
        for ref in references:
            if ref["path"] in checked:
                need(checked[ref["path"]] == ref["sha256"], "conflicting evidence identities")
                continue
            path = root
            for part in ref["path"].split("/"):
                path /= part
                need(not path.is_symlink(), "evidence symlink refused")
            need(path.is_file() and path.resolve().is_relative_to(root), "evidence absent/outside root")
            need(path.stat().st_size <= MAX_EVIDENCE, "evidence file exceeds 64 MiB record-check limit")
            with path.open("rb") as stream:
                actual = hashlib.file_digest(stream, "sha256").hexdigest()
            need(actual == ref["sha256"], "evidence bytes changed: " + ref["path"])
            checked[ref["path"]] = actual
    pending = [axis for axis in AXES if record["components"][axis]["status"] != "supported"]
    if any(candidate[k] is None for k in ("name", "revision", "artifact_sha256", "artifact", "toolchain")):
        pending.append("candidate identity")
    if any(not baseline[k] for k in baseline):
        pending.append("strongest qualified applicable baseline and selection evidence")
    if not all(workloads[k] for k in ("representative", "held_out", "frozen_plan_evidence")):
        pending.append("frozen representative and held-out comparison")
    if unbound_workloads:
        pending.append("retained workload byte locations")
    if evidence_root is None:
        pending.append("referenced file hashes not checked")
    return {"schema": "zeno-fcis/computational-feasibility-record-check/2", "record_valid": True,
            "declared_axes": {axis: record["components"][axis]["status"] for axis in AXES},
            "ready_for_independent_review": not pending, "missing_review_prerequisites": pending,
            "negative_results_retained": len(negatives), "evidence_references": len(references),
            "file_hashes_checked": evidence_root is not None, "algorithm_claims_independently_evaluated": False,
            "backend_artifacts_bound": {"candidate": candidate["artifact"] is not None, "baseline": baseline["artifact"] is not None},
            "unbound_workload_rows": unbound_workloads,
            "qualification_or_promotion_authority": "none"}


def unique_pairs(pairs):
    result = {}
    for key, value in pairs:
        need(key not in result, "duplicate JSON field")
        result[key] = value
    return result


def load_record(path):
    with Path(path).open("rb") as stream:
        raw = stream.read(MAX_RECORD + 1)
    need(len(raw) <= MAX_RECORD, "assessment exceeds 2 MiB")
    return json.loads(raw, object_pairs_hook=unique_pairs,
                      parse_constant=lambda value: need(False, "non-finite JSON number: " + value))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("assessment")
    parser.add_argument("--evidence-root", type=Path)
    parser.add_argument("--require-ready", action="store_true")
    args = parser.parse_args()
    try:
        report = check(load_record(args.assessment), args.evidence_root)
    except (ValueError, TypeError, KeyError, OSError, RecursionError) as error:
        print(json.dumps({"record_valid": False, "reason": str(error), "qualification_or_promotion_authority": "none"}))
        return 2
    print(json.dumps(report, sort_keys=True))
    return 1 if args.require_ready and not report["ready_for_independent_review"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
