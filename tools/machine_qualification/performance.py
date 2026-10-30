"""Exact-rational analysis of supplied paired timing rows under the frozen performance protocol.

This module measures nothing and runs nothing. It validates a frozen plan and the raw rows recorded under it,
then computes, with `fractions.Fraction` only:
- each pair's ratio, candidate elapsed nanoseconds over baseline elapsed nanoseconds, at equal call counts;
- each session's summary, the mean of its 3rd and 4th sorted ratios;
- the point estimate, the mean of the 10th and 11th sorted session summaries (the median paired elapsed-time
  ratio);
- the interval from the k-th to the (21-k)-th sorted summary, where k is the largest rank whose two-sided binomial
  tail is at most alpha/m for m preregistered comparisons (k = 6 for one comparison);
- the classification against the practical margin delta = 1/50.

Decimal strings are presentation only: lower endpoints are rounded down and upper endpoints up. Every decision uses
the exact fractions. The coverage statement holds only under the plan's sampling premises, which this analysis
cannot establish.

    python3 -B -m tools.machine_qualification.performance --plan PLAN.json --observations ROWS.json
"""

from __future__ import annotations

import argparse
from fractions import Fraction
import hashlib
import json
from math import comb
from pathlib import Path
import re
import sys

PLAN_SCHEMA = "zeno-fcis/machine-performance-plan/1"
OBSERVATIONS_SCHEMA = "zeno-fcis/machine-performance-observations/1"
REPORT_SCHEMA = "zeno-fcis/machine-performance-report/1"
SESSIONS, PAIRS, EACH_ORDER = 20, 6, 3
ALPHA = Fraction(1, 20)
MARGIN = Fraction(1, 50)
MAX_COMPARISONS = 1_000_000
MAX_INPUT_BYTES = 1 << 20
PLACES = 6
DECIMAL = re.compile(r"0|[1-9][0-9]{0,29}")
IDENTITY = re.compile(r"[A-Za-z0-9][A-Za-z0-9._:-]{0,127}")
PLAN_FIELDS = {"schema", "plan_id", "candidate", "baseline", "baseline_label", "sessions", "pairs_per_session",
               "ab_pairs_per_session", "ba_pairs_per_session", "call_count", "comparisons", "alpha",
               "practical_margin", "schedule_seed", "schedule"}
ROW_FIELDS = {"observation_id", "session", "session_id", "pair", "order", "candidate_calls", "baseline_calls",
              "candidate_elapsed_ns", "baseline_elapsed_ns", "outputs_agreed", "artifact_unchanged"}
NONCLAIMS = ("Analysis of supplied measurements only: no measurement, timing or execution was performed here. The "
             "interval's coverage needs independent identically distributed representative sessions, which the "
             "analysis cannot establish. Not mean execution time, application savings, instruction latency, "
             "amortization, universal optimality or a portable speedup claim.")


def need(value, reason):
    if not value:
        raise ValueError(reason)


def canonical(value) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True, allow_nan=False).encode("ascii")


def digest(value) -> str:
    return hashlib.sha256(canonical(value)).hexdigest()


def load(data: bytes) -> dict:
    """Bounded JSON with no duplicate keys, floats or non-finite constants."""
    need(type(data) is bytes and len(data) <= MAX_INPUT_BYTES, "input must be at most 1 MiB")
    def pairs(items):
        result = {}
        for key, value in items:
            need(key not in result, f"duplicate JSON key: {key}")
            result[key] = value
        return result
    def refused(text):
        raise ValueError(f"non-integer JSON number refused: {text}")
    value = json.loads(data.decode("utf-8"), object_pairs_hook=pairs, parse_float=refused, parse_constant=refused)
    need(type(value) is dict, "JSON root must be an object")
    return value


def _count(value, name, low=0) -> int:
    need(type(value) is str and DECIMAL.fullmatch(value), f"{name} must be a canonical decimal string")
    need(int(value) >= low, f"{name} must be at least {low}")
    return int(value)


def _fraction(value, name) -> Fraction:
    need(type(value) is dict and set(value) == {"numerator", "denominator"}
         and all(type(value[k]) is int for k in value) and value["denominator"] > 0, f"{name} must be a fraction")
    return Fraction(value["numerator"], value["denominator"])


def validate_plan(plan: dict) -> dict:
    need(type(plan) is dict and set(plan) == PLAN_FIELDS, "plan fields differ from the closed version-1 schema")
    need(plan["schema"] == PLAN_SCHEMA, "unsupported plan schema")
    for name in ("plan_id", "candidate", "baseline"):
        need(type(plan[name]) is str and IDENTITY.fullmatch(plan[name]), f"{name} must be a short identity")
    need(type(plan["baseline_label"]) is str and 1 <= len(plan["baseline_label"]) <= 200, "baseline_label")
    frozen = {"sessions": SESSIONS, "pairs_per_session": PAIRS, "ab_pairs_per_session": EACH_ORDER,
              "ba_pairs_per_session": EACH_ORDER}
    for name, value in frozen.items():
        need(type(plan[name]) is int and plan[name] == value, f"{name} must be {value} under the frozen protocol")
    _count(plan["call_count"], "call_count", 1)
    _count(plan["schedule_seed"], "schedule_seed")
    need(type(plan["comparisons"]) is int and 1 <= plan["comparisons"] <= MAX_COMPARISONS,
         f"comparisons must be an integer from 1 to {MAX_COMPARISONS}")
    need(_fraction(plan["alpha"], "alpha") == ALPHA, "alpha must be 1/20 under the frozen protocol")
    need(_fraction(plan["practical_margin"], "practical_margin") == MARGIN,
         "practical_margin must be 1/50 under the frozen protocol")
    schedule = plan["schedule"]
    need(type(schedule) is list and len(schedule) == SESSIONS, "schedule must list 20 sessions")
    for session in schedule:
        need(type(session) is list and len(session) == PAIRS and all(order in ("AB", "BA") for order in session)
             and session.count("AB") == EACH_ORDER, "each scheduled session has six pairs, three AB and three BA")
    return plan


def rank(sessions: int, comparisons: int):
    """Largest k in 1..floor((n+1)/2) with 2*sum(C(n,j), j<k)/2^n <= alpha/m, or None when none exists."""
    best, tail = None, 0
    for k in range(1, (sessions + 1) // 2 + 1):
        tail += comb(sessions, k - 1)
        if Fraction(2 * tail, 2 ** sessions) <= ALPHA / comparisons:
            best = k
    return best


def coverage(sessions: int, k: int) -> Fraction:
    """Per-comparison coverage of the [k, n-k+1] order-statistic interval for a population median."""
    return 1 - Fraction(2 * sum(comb(sessions, j) for j in range(k)), 2 ** sessions)


def classify(lower, upper) -> str:
    if lower is None or upper is None:
        return "inconclusive"
    if upper < 1 - MARGIN:
        return "improved"
    if lower >= 1 - MARGIN and upper <= 1 + MARGIN:
        return "practically_equivalent"
    if lower > 1 + MARGIN:
        return "regressed"
    return "inconclusive"


def decimal(value: Fraction, direction: str) -> str:
    """Fixed six-place presentation rounded down, up or to nearest (ties away from zero); never a decision input."""
    scaled = value * 10 ** PLACES
    whole = {"down": scaled.numerator // scaled.denominator,
             "up": -(-scaled.numerator // scaled.denominator),
             "nearest": (2 * scaled.numerator + scaled.denominator) // (2 * scaled.denominator)}[direction]
    return f"{whole // 10 ** PLACES}.{whole % 10 ** PLACES:0{PLACES}d}"


def _rows(observations: dict, plan: dict) -> tuple:
    """(rows indexed by (session, pair), invalidity reasons) after structural validation."""
    need(type(observations) is dict and set(observations) == {"schema", "plan_sha256", "rows"},
         "observation fields differ from the closed version-1 schema")
    need(observations["schema"] == OBSERVATIONS_SCHEMA, "unsupported observations schema")
    need(observations["plan_sha256"] == digest(plan),
         "observations were recorded under a different plan; a changed plan needs new measurements")
    rows = observations["rows"]
    need(type(rows) is list and len(rows) <= 2 * SESSIONS * PAIRS, "rows must be a bounded list")
    reasons, indexed, seen, sessions = [], {}, set(), {}
    for row in rows:
        need(type(row) is dict and set(row) == ROW_FIELDS, "row fields differ from the closed version-1 schema")
        need(type(row["observation_id"]) is str and IDENTITY.fullmatch(row["observation_id"]), "observation_id")
        need(type(row["session_id"]) is str and IDENTITY.fullmatch(row["session_id"]), "session_id")
        need(type(row["session"]) is int and 0 <= row["session"] < SESSIONS, "session must be 0..19")
        need(type(row["pair"]) is int and 0 <= row["pair"] < PAIRS, "pair must be 0..5")
        need(row["order"] in ("AB", "BA"), "order must be AB or BA")
        need(type(row["outputs_agreed"]) is bool and type(row["artifact_unchanged"]) is bool, "row checks are booleans")
        values = {name: _count(row[name], name) for name in
                  ("candidate_calls", "baseline_calls", "candidate_elapsed_ns", "baseline_elapsed_ns")}
        where = f"session {row['session']} pair {row['pair']}"
        if row["observation_id"] in seen:
            reasons.append(f"observation identity reused: {row['observation_id']}")
        seen.add(row["observation_id"])
        sessions.setdefault(row["session"], set()).add(row["session_id"])
        if (row["session"], row["pair"]) in indexed:
            reasons.append(f"{where}: recorded more than once")
        indexed[(row["session"], row["pair"])] = (row, values)
        if row["order"] != plan["schedule"][row["session"]][row["pair"]]:
            reasons.append(f"{where}: order differs from the frozen schedule")
        if not values["candidate_calls"] == values["baseline_calls"] == int(plan["call_count"]):
            reasons.append(f"{where}: unequal call counts")
        if values["candidate_elapsed_ns"] == 0 or values["baseline_elapsed_ns"] == 0:
            reasons.append(f"{where}: zero duration")
        if not row["outputs_agreed"]:
            reasons.append(f"{where}: wrong outputs")
        if not row["artifact_unchanged"]:
            reasons.append(f"{where}: artifact changed")
    for session in range(SESSIONS):
        identities = sessions.get(session, set())
        if len(identities) > 1:
            reasons.append(f"session {session}: more than one session identity")
        for pair in range(PAIRS):
            if (session, pair) not in indexed:
                reasons.append(f"session {session} pair {pair}: missing")
    owners = {}
    for session, identities in sessions.items():
        for identity in identities:
            owners.setdefault(identity, set()).add(session)
    reasons += [f"session identity reused across sessions: {identity}"
                for identity, owner in sorted(owners.items()) if len(owner) > 1]
    return indexed, reasons


def analyze(plan: dict, observations: dict) -> dict:
    """Validate and analyze; raise ValueError for malformed input, report `invalid` for invalid measurements."""
    plan = validate_plan(plan)
    indexed, reasons = _rows(observations, plan)
    report = {"schema": REPORT_SCHEMA, "authority": "none", "plan_sha256": digest(plan),
              "observations_sha256": digest(observations), "plan_id": plan["plan_id"],
              "candidate": plan["candidate"], "baseline": plan["baseline"], "baseline_label": plan["baseline_label"],
              "nonclaims": NONCLAIMS}
    if reasons:
        return {**report, "status": "invalid", "reasons": reasons}
    ratios = [[Fraction(indexed[(s, p)][1]["candidate_elapsed_ns"], indexed[(s, p)][1]["baseline_elapsed_ns"])
               for p in range(PAIRS)] for s in range(SESSIONS)]
    summaries = []
    for session in ratios:
        ordered = sorted(session)
        summaries.append((ordered[2] + ordered[3]) / 2)
    ordered = sorted(summaries)
    point = (ordered[9] + ordered[10]) / 2
    k = rank(SESSIONS, plan["comparisons"])
    lower, upper = (ordered[k - 1], ordered[SESSIONS - k]) if k else (None, None)
    exact = lambda value: f"{value.numerator}/{value.denominator}"
    interval = ({"ranks": [k, SESSIONS - k + 1], "lower": exact(lower), "upper": exact(upper),
                 "lower_decimal_down": decimal(lower, "down"), "upper_decimal_up": decimal(upper, "up"),
                 "coverage_per_comparison": exact(coverage(SESSIONS, k))}
                if k else {"ranks": None, "reason": "no admissible rank for this comparison family; unbounded"})
    return {**report, "status": "valid", "metric": "median paired elapsed-time ratio, candidate over baseline",
            "pair_ratios": [[exact(r) for r in session] for session in ratios],
            "session_summaries": [exact(z) for z in summaries],
            "point_estimate": {"exact": exact(point), "decimal_nearest": decimal(point, "nearest")},
            "comparisons": plan["comparisons"], "family_alpha": "1/20", "practical_margin": "1/50",
            "interval": interval, "classification": classify(lower, upper)}


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--plan", type=Path, required=True)
    parser.add_argument("--observations", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        report = analyze(load(args.plan.read_bytes()), load(args.observations.read_bytes()))
    except (OSError, ValueError, UnicodeDecodeError, RecursionError) as error:
        print(json.dumps({"status": "refused", "reason": str(error), "authority": "none"}, sort_keys=True))
        return 65
    print(json.dumps(report, sort_keys=True))
    return 0 if report["status"] == "valid" else 1


if __name__ == "__main__":
    sys.exit(main())
