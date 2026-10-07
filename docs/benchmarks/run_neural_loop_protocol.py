#!/usr/bin/env python3
"""Run the preregistered neural-loop protocol (PROTOCOL.md sections B, D-G) over a
cases file with the proposers this build ships, and report every result.

Standard library only. Each case becomes one `zeno-fcis loop` session per arm:

  local-only            the deterministic local rewriter (`loop run --proposer local`)
  model-fake            the catalog's recorded candidate replayed as a model's whole
                        proposal through the fake provider (`loop run --proposer fake`)
  fixed-rewrite-egraph  one strategy proposal: the optimizer's default strategy
                        without its semantic-merge phase, run by the wired e-graph
                        engine; its bytes are checked by the loop like any candidate
  semantic-egraph       one strategy proposal: the optimizer's default strategy

The two e-graph arms are driven through the fake provider, so their accounting
shows one model call that no model made. The hybrid arms of PROTOCOL.md section F
need a hosted provider, which is disabled in this build; they are reported as
unavailable, never as zero-cost successes. Every attempted case counts:
refusals, differences, inconclusive checks and tool failures stay in the
denominator (NSE-007). No optimality, convergence or neural benefit is claimed.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
REPORT_SCHEMA = "zeno-fcis/neural-loop-protocol-report/1"
CASES_SCHEMA = "zenofcis-benchmark-seeds-v1"
ARMS = ("local-only", "model-fake", "fixed-rewrite-egraph", "semantic-egraph")
# The optimizer's default strategy (crates/zeno-fcis-cli/src/optimize/strategy.rs,
# DEFAULT_STRATEGY_JSON); the fixed-rewrite arm drops its semantic-merge phase.
DEFAULT_STRATEGY: dict[str, Any] = {
    "schema": "zeno-fcis/optimize-strategy/1",
    "phases": [
        {"phase": "fold", "rounds": 1}, {"phase": "share", "rounds": 1},
        {"phase": "boolean", "rounds": 3}, {"phase": "select", "rounds": 3},
        {"phase": "semantic-merge", "rounds": 2}, {"phase": "boolean", "rounds": 2},
        {"phase": "select", "rounds": 2}, {"phase": "fold", "rounds": 1},
    ],
    "limits": {"max_enodes": 20000, "max_classes": 10000,
               "max_rewrites_per_round": 10000, "max_extraction_rounds": 8},
    "extractor": "dag-greedy",
}
EGRAPH_STRATEGIES: dict[str, dict[str, Any]] = {
    "semantic-egraph": DEFAULT_STRATEGY,
    "fixed-rewrite-egraph": {
        **DEFAULT_STRATEGY,
        "phases": [phase for phase in DEFAULT_STRATEGY["phases"]
                   if phase["phase"] != "semantic-merge"],
    },
}
UNAVAILABLE_ARMS = {
    "hybrid-without-feedback": "hosted provider disabled; no approved provider, disclosure or budget",
    "hybrid-with-feedback": "hosted provider disabled; no approved provider, disclosure or budget",
}
PROFILES = {"FunctionalBoolV1": "functional-bool-v1", "CheckedI64V1": "checked-i64-v1"}


def profile_of(case: dict[str, Any]) -> str | None:
    """The loop profile for a case; the catalog suffixes proposed profiles."""
    for prefix, name in PROFILES.items():
        if str(case.get("profile", "")).startswith(prefix):
            return name
    return None


def locate_cli(explicit: str | None) -> str:
    candidates = [explicit, os.environ.get("ZENO_FCIS_CLI")]
    target = os.environ.get("CARGO_TARGET_DIR")
    if target:
        candidates.append(str(Path(target) / "debug/zeno-fcis"))
    candidates.append(str(ROOT / "target/debug/zeno-fcis"))
    candidates.append(shutil.which("zeno-fcis"))
    for candidate in candidates:
        if candidate and Path(candidate).is_file():
            return str(Path(candidate).resolve())
    raise SystemExit("no zeno-fcis CLI: pass --cli, set ZENO_FCIS_CLI or build crates/zeno-fcis-cli")


def loop(cli: str, arguments: list[str]) -> dict[str, Any]:
    """One fixed `zeno-fcis loop` subcommand; a non-JSON answer is itself a result."""
    try:
        completed = subprocess.run([cli, "loop", *arguments], capture_output=True, text=True,
                                   timeout=120, check=False)
    except (OSError, subprocess.TimeoutExpired) as error:
        return {"exit_code": None, "report": None, "tool_failure": str(error)}
    try:
        report = json.loads(completed.stdout)
    except ValueError:
        report = None
    return {"exit_code": completed.returncode, "report": report,
            "stderr": completed.stderr[:500], "tool_failure": None}


def program_json(case: dict[str, Any], side: str) -> dict[str, Any]:
    return {"inputs": case["input_domains"], "outputs": case["output_domains"],
            "nodes": case[side]["nodes"], "roots": case[side]["roots"]}


def encode(cli: str, work: Path, case: dict[str, Any], side: str) -> tuple[Path | None, dict[str, Any]]:
    source = work / f"{case['id']}-{side}.json"
    source.write_text(json.dumps(program_json(case, side)), encoding="utf-8")
    out = work / f"{case['id']}-{side}.zcve"
    result = loop(cli, ["encode", "--program", str(source), "--out", str(out)])
    return (out if result["exit_code"] == 0 else None), result


def run_arm(cli: str, work: Path, case: dict[str, Any], arm: str, original: Path,
            candidate: Path | None) -> dict[str, Any]:
    session = work / f"{case['id']}-{arm}"
    profile = profile_of(case)
    outcome: dict[str, Any] = {"arm": arm, "case": case["id"], "family": case["family"],
                               "profile": profile}
    if profile is None:
        outcome["class"] = "unsupported-profile"
        return outcome
    opened = loop(cli, ["open", "--original", str(original), "--session", str(session),
                        "--profile", profile])
    outcome["open"] = summarize(opened)
    if opened["exit_code"] != 0:
        outcome["class"] = "request-refused"
        return outcome
    if arm == "local-only":
        ran = loop(cli, ["run", "--session", str(session), "--proposer", "local"])
    elif arm in EGRAPH_STRATEGIES:
        script = work / f"{case['id']}-{arm}-script.json"
        script.write_text(json.dumps({"schema": "zeno-fcis/fake-provider-script/1",
                                      "steps": [{"strategy": EGRAPH_STRATEGIES[arm]}]}),
                          encoding="utf-8")
        ran = loop(cli, ["run", "--session", str(session), "--proposer", "fake",
                         "--script", str(script)])
    else:
        if candidate is None:
            outcome["class"] = "candidate-not-encodable"
            return outcome
        script = work / f"{case['id']}-script.json"
        script.write_text(json.dumps({"schema": "zeno-fcis/fake-provider-script/1",
                                      "steps": [{"candidate": str(candidate)}]}), encoding="utf-8")
        ran = loop(cli, ["run", "--session", str(session), "--proposer", "fake",
                         "--script", str(script)])
    outcome["run"] = summarize(ran)
    report = (ran.get("report") or {}).get("detail", {}).get("report")
    if ran["exit_code"] != 0 or not report:
        outcome["class"] = "tool-failure"
        return outcome
    original_cost = report["original"]["cost"]
    incumbent_cost = report["incumbent"]["cost"]
    attempts = report["attempts"]
    outcome.update({
        "status": report["status"],
        "stop_reason": report["session"].get("stop_reason"),
        "original_cost": original_cost,
        "incumbent_cost": incumbent_cost,
        "delta": {"nodes": original_cost["nodes"] - incumbent_cost["nodes"],
                  "bytes": original_cost["bytes"] - incumbent_cost["bytes"]},
        "node_reduction": (original_cost["nodes"] - incumbent_cost["nodes"]) / original_cost["nodes"],
        "attempt_outcomes": [attempt["outcome"] for attempt in attempts],
        "accounting": report["accounting"],
        "expected_relation": case["expected_relation"],
    })
    outcomes = outcome["attempt_outcomes"]
    if report["status"] == "best-checked-so-far":
        outcome["class"] = "checked-improvement"
    elif any(name == "different" for name in outcomes):
        outcome["class"] = "different"
    elif any(name.startswith("equivalent-without-improvement") for name in outcomes):
        outcome["class"] = "equivalent-without-improvement"
    elif any(name.startswith("duplicate-of-original") for name in outcomes):
        # The catalog's no-improvement controls propose the original itself.
        outcome["class"] = "duplicate-of-original"
    elif any(name == "refused" for name in outcomes):
        outcome["class"] = "refused"
    elif any(name.startswith("search-failed") for name in outcomes):
        # The engine returned no candidate: nothing better found, or the search
        # failed or timed out. The session transcript records which.
        outcome["class"] = "search-no-candidate"
    elif not outcomes:
        outcome["class"] = "no-proposal"
    else:
        outcome["class"] = "inconclusive"
    return outcome


def summarize(result: dict[str, Any]) -> dict[str, Any]:
    report = result.get("report") or {}
    return {"exit_code": result["exit_code"], "status": report.get("status"),
            "tool_failure": result.get("tool_failure")}


def family_balanced_mean(results: list[dict[str, Any]], key: str) -> float | None:
    """Average within each family, then across families (PROTOCOL.md section G)."""
    by_family: dict[str, list[float]] = {}
    for result in results:
        value = result.get(key)
        if isinstance(value, (int, float)):
            by_family.setdefault(result["family"], []).append(float(value))
    if not by_family:
        return None
    means = [sum(values) / len(values) for values in by_family.values()]
    return sum(means) / len(means)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--cases", default=str(ROOT / "docs/benchmarks/cases.json"))
    parser.add_argument("--out", required=True, help="JSON report to write (new file)")
    parser.add_argument("--cli", default=None)
    parser.add_argument("--set", default="public", choices=("public", "held-out"),
                        help="label of the cases file; a public catalog cannot be labeled held-out")
    parser.add_argument("--repeats", type=int, default=1,
                        help="recorded repetitions per arm (deterministic arms repeat identically)")
    parser.add_argument("--keep", default=None, help="directory to keep sessions in")
    args = parser.parse_args()
    out = Path(args.out)
    if out.exists():
        raise SystemExit(f"refusing to overwrite {out}")
    cli = locate_cli(args.cli)
    cases_path = Path(args.cases)
    cases_bytes = cases_path.read_bytes()
    catalog = json.loads(cases_bytes)
    if catalog.get("schema_version") != CASES_SCHEMA:
        raise SystemExit(f"unsupported cases schema: {catalog.get('schema_version')!r}")
    if args.set == "held-out" and catalog.get("status") == "public-development-seeds":
        raise SystemExit("a public development catalog cannot be labeled held-out")
    if not 1 <= args.repeats <= 5:
        raise SystemExit("repeats must be 1..5")
    work_root = Path(args.keep) if args.keep else Path(tempfile.mkdtemp(prefix="zenofcis-loop-protocol-"))
    work_root.mkdir(parents=True, exist_ok=True)
    results: list[dict[str, Any]] = []
    encoding_failures: list[dict[str, Any]] = []
    for case in catalog["cases"]:
        work = work_root / case["id"]
        work.mkdir(exist_ok=True)
        original, encoded = encode(cli, work, case, "original")
        if original is None:
            encoding_failures.append({"case": case["id"], "side": "original", "result": summarize(encoded)})
            for arm in ARMS:
                results.append({"arm": arm, "case": case["id"], "family": case["family"],
                                "class": "original-not-encodable"})
            continue
        candidate, encoded = encode(cli, work, case, "candidate")
        if candidate is None:
            encoding_failures.append({"case": case["id"], "side": "candidate", "result": summarize(encoded)})
        for repeat in range(args.repeats):
            for arm in ARMS:
                arm_work = work / f"repeat-{repeat}"
                arm_work.mkdir(exist_ok=True)
                result = run_arm(cli, arm_work, case, arm, original, candidate)
                result["repeat"] = repeat
                results.append(result)
    summary = {}
    for arm in ARMS:
        arm_results = [result for result in results if result["arm"] == arm]
        classes: dict[str, int] = {}
        for result in arm_results:
            classes[result["class"]] = classes.get(result["class"], 0) + 1
        summary[arm] = {
            "runs": len(arm_results),
            "classes": dict(sorted(classes.items())),
            "improvement_frequency": (classes.get("checked-improvement", 0) / len(arm_results)
                                      if arm_results else None),
            "family_balanced_mean_node_reduction": family_balanced_mean(arm_results, "node_reduction"),
            "micro_total_node_delta": sum(result.get("delta", {}).get("nodes", 0) for result in arm_results),
            "micro_total_byte_delta": sum(result.get("delta", {}).get("bytes", 0) for result in arm_results),
        }
    report = {
        "schema": REPORT_SCHEMA,
        "authority": "none",
        "preregistration": {
            "protocol": "docs/benchmarks/PROTOCOL.md",
            "cases_file": str(cases_path),
            "cases_sha256": hashlib.sha256(cases_bytes).hexdigest(),
            "cases_status": catalog.get("status"),
            "set_label": args.set,
            "enumeration": catalog.get("enumeration"),
            "cases": len(catalog["cases"]),
            "arms_run": list(ARMS),
            "arms_unavailable": UNAVAILABLE_ARMS,
            "egraph_strategies": EGRAPH_STRATEGIES,
            "repeats": args.repeats,
            "seeds": "none: every arm run is deterministic",
            "limits": "compiled ceilings of zeno-fcis loop (8 attempts, 8 checks, 4 calls, 20 s)",
            "stop_rule": "every case runs every available arm once per repeat; nothing is excluded after the fact",
            "hosted_spending": "disabled, zero allowance",
        },
        "cli": cli,
        "summary": summary,
        "encoding_failures": encoding_failures,
        "results": results,
        "claims": {
            "made": "per-case checked node and byte deltas under complete-domain equivalence checks",
            "not_made": ["neural benefit", "global optimality", "convergence", "held-out generalization",
                         "application authority", "runtime speedup"],
        },
    }
    out.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    if not args.keep:
        shutil.rmtree(work_root, ignore_errors=True)
    for arm, totals in summary.items():
        print(f"{arm}: {totals['runs']} runs {totals['classes']}")
    print(f"report written to {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
