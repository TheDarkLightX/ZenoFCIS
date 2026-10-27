#!/usr/bin/env python3
"""Structural, fail-closed preflight for registered algorithm feasibility cards.

This is NOT a benchmark runner, proof checker or automatic adoption authority.
"""
from __future__ import annotations

import argparse
import copy
import json
import math
import re
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CARDS = Path("docs/feasibility/cards")
REGISTRY = Path("docs/feasibility/registry.json")
EXAMPLE = Path("docs/feasibility/example-research.json")
ID = re.compile(r"[a-z][a-z0-9-]{0,63}\Z")
SHA40 = re.compile(r"[0-9a-f]{40}\Z")
SHA64 = re.compile(r"[0-9a-f]{64}\Z")
TIERS = ("research", "qualified-opt-in", "default")
COST = ("encoding_ms", "solving_ms", "evidence_ms", "checking_ms",
        "other_ms", "total_ms", "peak_rss_bytes", "phase_model",
        "overlap_notes")


def shape(value: object, name: str, keys: tuple[str, ...],
          errors: list[str]) -> dict:
    if not isinstance(value, dict):
        errors.append(f"{name}: expected object")
        return {}
    actual = set(value)
    if actual != set(keys):
        errors.append(f"{name}: missing={sorted(set(keys)-actual)}, "
                      f"unknown={sorted(actual-set(keys))}")
    return value


def required(value: object, label: str, errors: list[str]) -> None:
    if not isinstance(value, str) or not value.strip():
        errors.append(f"{label}: nonempty text required")


def evidence_reference(value: object, label: str, errors: list[str]) -> None:
    required(value, label, errors)
    if isinstance(value, str) and value.strip().lower() in {
        "not-run", "unknown", "none", "todo", "n/a", "unreviewed", "pending"
    }:
        errors.append(f"{label}: placeholder is not evidence")


def one_of(value: object, options: tuple[str, ...],
           label: str, errors: list[str]) -> None:
    if value not in options:
        errors.append(f"{label}: expected one of {options}")


def integer(value: object, label: str, errors: list[str]) -> int:
    if type(value) is not int or value < 0:
        errors.append(f"{label}: nonnegative integer required")
        return 0
    return value


def validate_cost(value: object, label: str, errors: list[str]) -> None:
    record = shape(value, label, COST, errors)
    numbers = []
    for key in COST[:6]:
        v = record.get(key)
        if type(v) not in (int, float) or not math.isfinite(v) or v < 0:
            errors.append(f"{label}.{key}: finite nonnegative number required")
            return
        numbers.append(v)
    rss = record.get("peak_rss_bytes")
    if type(rss) is not int or rss < 0:
        errors.append(f"{label}.peak_rss_bytes: nonnegative integer required")
    one_of(record.get("phase_model"), ("sequential", "overlapping"),
           f"{label}.phase_model", errors)
    required(record.get("overlap_notes"), f"{label}.overlap_notes", errors)
    if (record.get("phase_model") == "sequential"
            and numbers[5] + 1e-6 < sum(numbers[:5])):
        errors.append(f"{label}.total_ms: smaller than exclusive measured phases")


def validate_card(value: object, expected_id: str, expected_tier: str) -> list[str]:
    errors: list[str] = []
    root = shape(value, "card",
                 ("schema", "id", "tier", "theory", "correctness",
                  "concrete", "empirical", "assurance", "decision"), errors)
    if root.get("schema") != "zenofcis/computational-feasibility/1":
        errors.append("card.schema: unsupported version")
    if root.get("id") != expected_id or not ID.fullmatch(expected_id):
        errors.append("card.id: does not match registered slug")
    if root.get("tier") != expected_tier or expected_tier not in TIERS:
        errors.append("card.tier: does not match registered tier")

    theory = shape(root.get("theory"), "theory",
                   ("status", "bound", "model", "assumptions", "source"), errors)
    one_of(theory.get("status"),
           ("unreviewed", "conditional", "proved", "formalized"),
           "theory.status", errors)
    for key in ("bound", "model", "source"):
        required(theory.get(key), f"theory.{key}", errors)
    assumptions = theory.get("assumptions")
    if not isinstance(assumptions, list) or not all(
        isinstance(x, str) and x.strip() for x in assumptions
    ):
        errors.append("theory.assumptions: expected text array")
    elif theory.get("status") == "conditional" and not assumptions:
        errors.append("theory.assumptions: conditional claim must name assumptions")

    correctness = shape(root.get("correctness"), "correctness",
                        ("status", "scope", "encoding", "oracle", "evidence"), errors)
    one_of(correctness.get("status"), ("unverified", "checked"),
           "correctness.status", errors)
    for key in ("scope", "encoding", "oracle"):
        required(correctness.get(key), f"correctness.{key}", errors)

    concrete = shape(root.get("concrete"), "concrete",
                     ("status", "operation_count", "memory_model", "overheads"), errors)
    one_of(concrete.get("status"), ("unknown", "modeled", "measured"),
           "concrete.status", errors)
    for key in ("operation_count", "memory_model", "overheads"):
        required(concrete.get(key), f"concrete.{key}", errors)

    empirical = shape(root.get("empirical"), "empirical",
                      ("status", "baseline_commit", "candidate_commit",
                       "corpus_sha256", "reproducer", "environment", "trials",
                       "cases", "baseline_cost", "candidate_cost",
                       "failure_report", "raw_evidence", "crossover",
                       "predeclared_criteria"), errors)
    one_of(empirical.get("status"), ("not-measured", "measured"),
           "empirical.status", errors)
    trials = integer(empirical.get("trials"), "empirical.trials", errors)
    cases = shape(empirical.get("cases"), "empirical.cases",
                  ("development", "held_out"), errors)
    integer(cases.get("development"), "empirical.cases.development", errors)
    held_out = integer(cases.get("held_out"),
                       "empirical.cases.held_out", errors)
    crossover = shape(empirical.get("crossover"), "empirical.crossover",
                      ("status", "detail"), errors)
    one_of(crossover.get("status"), ("not-tested", "not-observed", "observed"),
           "empirical.crossover.status", errors)
    required(crossover.get("detail"), "empirical.crossover.detail", errors)
    if (crossover.get("status") != "not-tested"
            and empirical.get("status") != "measured"):
        errors.append("empirical.crossover: cannot claim a result without measurements")

    if empirical.get("status") == "not-measured":
        for key in ("baseline_commit", "candidate_commit", "corpus_sha256",
                    "reproducer", "environment", "baseline_cost",
                    "candidate_cost", "failure_report", "raw_evidence",
                    "predeclared_criteria"):
            if empirical.get(key) is not None:
                errors.append(f"empirical.{key}: unmeasured record must use null")
        if trials != 0 or cases.get("development") != 0 or held_out != 0:
            errors.append("empirical: unmeasured record must report zero trials and cases")
    if empirical.get("status") == "measured":
        for key, pattern in (("baseline_commit", SHA40),
                             ("candidate_commit", SHA40),
                             ("corpus_sha256", SHA64)):
            value = empirical.get(key)
            if not isinstance(value, str) or not pattern.fullmatch(value):
                errors.append(f"empirical.{key}: lowercase pinned hash required")
        for key in ("reproducer", "environment", "failure_report",
                    "raw_evidence", "predeclared_criteria"):
            evidence_reference(empirical.get(key), f"empirical.{key}", errors)
        for key in ("baseline_cost", "candidate_cost"):
            validate_cost(empirical.get(key), f"empirical.{key}", errors)

    assurance = shape(root.get("assurance"), "assurance",
                      ("status", "trusted_base", "evidence",
                       "fallback", "unknown_policy"), errors)
    one_of(assurance.get("status"), ("pending", "reviewed"),
           "assurance.status", errors)
    for key in ("trusted_base", "fallback", "unknown_policy"):
        required(assurance.get(key), f"assurance.{key}", errors)

    decision = shape(root.get("decision"), "decision",
                     ("scope", "claim", "rationale", "reviewer", "limitations"),
                     errors)
    for key in ("scope", "rationale"):
        required(decision.get(key), f"decision.{key}", errors)
    one_of(decision.get("claim"), ("none", "scoped-benefit"),
           "decision.claim", errors)
    limits = decision.get("limitations")
    if not isinstance(limits, list) or not limits or not all(
        isinstance(x, str) and x.strip() for x in limits
    ):
        errors.append("decision.limitations: nonempty text array required")

    if decision.get("claim") == "scoped-benefit" and empirical.get("status") != "measured":
        errors.append("decision.claim: no measured benefit can be claimed without runs")

    if expected_tier != "research":
        if correctness.get("status") != "checked":
            errors.append("promotion: correctness must be independently checked")
        if concrete.get("status") == "unknown":
            errors.append("promotion: concrete costs cannot remain unknown")
        if empirical.get("status") != "measured" or trials < 3 or held_out < 1:
            errors.append("promotion: paired measured runs, >=3 trials "
                          "and held-out cases required")
        if assurance.get("status") != "reviewed":
            errors.append("promotion: assurance review required")
        for name, field in (("correctness.encoding", correctness.get("encoding")),
                            ("correctness.oracle", correctness.get("oracle")),
                            ("correctness.evidence", correctness.get("evidence")),
                            ("assurance.evidence", assurance.get("evidence")),
                            ("decision.reviewer", decision.get("reviewer"))):
            evidence_reference(field, name, errors)
        if expected_tier == "default" and decision.get("claim") != "scoped-benefit":
            errors.append("default: a scoped, reviewed measured benefit required")
    return errors


def unique_pairs(pairs: list[tuple[str, object]]) -> dict:
    value: dict = {}
    for key, item in pairs:
        if key in value:
            raise ValueError(f"duplicate JSON key: {key}")
        value[key] = item
    return value


def read_json(path: Path) -> object:
    if path.stat().st_size > 65536:
        raise ValueError("file exceeds 64 KiB limit")
    return json.loads(path.read_text(encoding="utf-8"),
                      object_pairs_hook=unique_pairs)


def check_repository(root: Path) -> list[str]:
    errors: list[str] = []
    try:
        manifest = read_json(root / REGISTRY)
    except (OSError, UnicodeError, ValueError) as exc:
        return [f"{REGISTRY}: {exc}"]
    reg = shape(manifest, str(REGISTRY), ("schema", "entries"), errors)
    if reg.get("schema") != "zenofcis/feasibility-registry/1":
        errors.append("registry.schema: unsupported version")
    entries = reg.get("entries")
    if not isinstance(entries, list):
        return errors + ["registry.entries: array required"]

    registered: set[str] = set()
    previous = ""
    for i, entry in enumerate(entries):
        e = shape(entry, f"registry.entries[{i}]", ("id", "tier"), errors)
        slug, tier = e.get("id"), e.get("tier")
        if not isinstance(slug, str) or not ID.fullmatch(slug):
            errors.append(f"registry.entries[{i}].id: invalid slug")
            continue
        if slug in registered or slug <= previous:
            errors.append(f"registry.entries[{i}]: ids must be unique and sorted")
        previous = slug
        registered.add(slug)
        if tier not in TIERS:
            errors.append(f"registry.entries[{i}].tier: invalid")
            continue
        path = root / CARDS / (slug + ".json")
        try:
            record = read_json(path)
        except (OSError, UnicodeError, ValueError) as exc:
            errors.append(f"{path.relative_to(root)}: {exc}")
            continue
        errors.extend(f"{CARDS / (slug + '.json')}: {message}" for message
                      in validate_card(record, slug, tier))

    actual = {p.stem for p in (root / CARDS).glob("*.json")}
    if actual != registered:
        errors.append(f"cards: unregistered={sorted(actual-registered)}, "
                      f"missing={sorted(registered-actual)}")
    try:
        sample = read_json(root / EXAMPLE)
        errors.extend(f"{EXAMPLE}: {issue}" for issue in
                      validate_card(sample, "example-research", "research"))
    except (OSError, UnicodeError, ValueError) as exc:
        errors.append(f"{EXAMPLE}: {exc}")
    return errors


def self_test() -> list[str]:
    sample = read_json(ROOT / EXAMPLE)
    errors: list[str] = []
    if validate_card(sample, "example-research", "research"):
        errors.append("positive example was rejected")
    bad = copy.deepcopy(sample)
    bad["theory"]["status"] = "conditional"
    bad["theory"]["assumptions"] = []
    if not validate_card(bad, "example-research", "research"):
        errors.append("conditional premise omission was accepted")
    if not validate_card(sample, "example-research", "default"):
        errors.append("unmeasured default promotion was accepted")
    promoted = copy.deepcopy(sample)
    promoted["tier"] = "qualified-opt-in"
    promoted["correctness"].update(status="checked", encoding="encoding-audit",
                                   oracle="reference-check", evidence="checked-replay")
    promoted["concrete"]["status"] = "measured"
    cost = {"encoding_ms": 1, "solving_ms": 2, "evidence_ms": 1,
            "checking_ms": 1, "other_ms": 0, "total_ms": 5,
            "peak_rss_bytes": 1024, "phase_model": "sequential",
            "overlap_notes": "No concurrent phases in synthetic fixture."}
    promoted["empirical"].update(
        status="measured", baseline_commit="a" * 40, candidate_commit="b" * 40,
        corpus_sha256="c" * 64, reproducer="harness@pinned",
        environment="synthetic test host", trials=3,
        cases={"development": 2, "held_out": 1},
        baseline_cost=copy.deepcopy(cost), candidate_cost=copy.deepcopy(cost),
        failure_report="fixture failure log", raw_evidence="fixture raw runs",
        predeclared_criteria="fixture criteria",
        crossover={"status": "not-observed", "detail": "Within fixture range"})
    promoted["assurance"].update(status="reviewed", evidence="fixture review")
    promoted["decision"].update(reviewer="fixture reviewer")
    if validate_card(promoted, "example-research", "qualified-opt-in"):
        errors.append("valid synthetic opt-in fixture was rejected")
    promoted["tier"] = "default"
    promoted["decision"]["claim"] = "scoped-benefit"
    if validate_card(promoted, "example-research", "default"):
        errors.append("valid structural default fixture was rejected")
    promoted["empirical"]["candidate_cost"]["total_ms"] = 1
    if not validate_card(promoted, "example-research", "default"):
        errors.append("impossible phase-cost total was accepted")
    promoted["empirical"]["candidate_cost"]["total_ms"] = 5
    promoted["correctness"]["evidence"] = "not-run"
    if not validate_card(promoted, "example-research", "default"):
        errors.append("promoted placeholder evidence was accepted")
    bad = copy.deepcopy(sample)
    bad["empirical"]["crossover"]["status"] = "observed"
    if not validate_card(bad, "example-research", "research"):
        errors.append("invented crossover was accepted")
    with tempfile.TemporaryDirectory(prefix="zeno-feasibility-") as directory:
        root = Path(directory)
        (root / CARDS).mkdir(parents=True)
        (root / EXAMPLE).write_text(json.dumps(sample), encoding="utf-8")
        (root / REGISTRY).write_text(json.dumps({
            "schema": "zenofcis/feasibility-registry/1", "entries": []
        }), encoding="utf-8")
        if check_repository(root):
            errors.append("empty prospective registry was rejected")
        (root / CARDS / "rogue.json").write_text(
            json.dumps(sample), encoding="utf-8")
        if not check_repository(root):
            errors.append("unregistered card was accepted")
        (root / CARDS / "rogue.json").unlink()
        (root / REGISTRY).write_text(json.dumps({
            "schema": "zenofcis/feasibility-registry/1",
            "entries": [{"id": "../escape", "tier": "default"}]
        }), encoding="utf-8")
        if not check_repository(root):
            errors.append("path escape slug was accepted")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    errors = self_test() if args.self_test else check_repository(ROOT)
    for error in errors:
        print(f"FAIL: {error}", file=sys.stderr)
    if errors:
        return 1
    print("computational feasibility preflight: PASS "
          "(structure only; no performance or proof claim)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
