#!/usr/bin/env python3
"""Independently validate Rust outputs and describe the preregistered comparison."""

from __future__ import annotations

import argparse
from collections import Counter
import copy
import hashlib
import json
import math
from pathlib import Path
from statistics import median

from corpus import FAMILIES, cost, lower, parse, signatures

REQUIRED_VARIANTS = ("raw", "local_baseline", "rewrite_egg", "semantic_egg")
CONTROL_IDS = (
    "altered_truth_function", "non_boolean_region", "eager_dead_add_overflow",
    "eager_select_arm_overflow", "invalid_input_shape", "invalid_input_domain",
)
FROZEN_CORPUS_SHA256 = "f0ad16732a453ef6c176e48aa46e544ad07015f4793bf32c84459a271ee9bc18"
INITIAL_SEARCH_CONFIG = {"iterations": 6, "node_limit": 5000, "lp_seconds": 2.0}
TIMING_COMPONENTS = {
    "saturation": "E-graph construction, bounded rewrite search, and final semantic unions where enabled.",
    "extraction": "Extractor construction, solving, and expression reconstruction; the LP budget limits solving only.",
    "evaluation": "Candidate parsing/lowering, admission, complete finite checks, and receipt serialization.",
    "total": "This variant's complete Rust proposal/check interval; excludes the separately precomputed Python baseline search.",
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def program_signatures(program, nvars, output_count):
    """Separate interpreter for the serialized actual FCIS Boolean DAG."""
    require(len(program["inputs"]) == nvars, "FCIS input arity")
    require(len(program["outputs"]) == output_count, "FCIS output arity")
    require(all(domain == {"kind": "bool"} for domain in program["inputs"] + program["outputs"]), "non-Boolean serialized domain")
    roots, instructions = program["roots"], program["nodes"]
    require(len(roots) == output_count, "FCIS root count")
    result = ["" for _ in roots]
    for mask in range(1 << nvars):
        values = []
        for instruction in instructions:
            op = instruction["op"]
            args = instruction.get("args", [])
            require(all(type(arg) is int and 0 <= arg < len(values) for arg in args), "FCIS non-topological argument")
            if op == "input":
                index = instruction["index"]
                require(type(index) is int and 0 <= index < nvars and not args, "FCIS invalid input")
                value = bool(mask & (1 << index))
            elif op == "bool":
                require(type(instruction["value"]) is bool and not args, "FCIS invalid Bool constant")
                value = instruction["value"]
            elif op == "and":
                require(len(args) == 2, "FCIS And arity")
                value = values[args[0]] and values[args[1]]
            elif op == "not":
                require(len(args) == 1, "FCIS Not arity")
                value = not values[args[0]]
            elif op == "select":
                require(len(args) == 3, "FCIS Select arity")
                value = values[args[1]] if values[args[0]] else values[args[2]]
            else:
                raise ValueError(f"unsupported serialized FCIS instruction: {op}")
            values.append(value)
        for index, root in enumerate(roots):
            require(type(root) is int and 0 <= root < len(values), "FCIS invalid root")
            result[index] += "1" if values[root] else "0"
    return result


def expected_observation_trace(truth_signatures, nvars):
    """Complete Boolean observations plus the declared finite invalid probes."""
    checks = []
    for mask in range(1 << nvars):
        values = [int(signature[mask]) for signature in truth_signatures]
        checks.append({"input": [(mask >> index) & 1 for index in range(nvars)], "admitted": True, "source": {"ok": values}, "candidate": {"ok": values}})
    invalid = {(0,) * (nvars + 1)}
    if nvars:
        invalid.add((0,) * (nvars - 1))
    for index in range(nvars):
        for value in (-1, 2, -(1 << 63), (1 << 63) - 1):
            probe = [0] * nvars
            probe[index] = value
            invalid.add(tuple(probe))
    rejection = {"error": 'Invalid("input-domain")'}
    checks.extend({"input": list(probe), "admitted": False, "source": rejection, "candidate": rejection} for probe in sorted(invalid))
    return checks, len(invalid)


def distribution(values):
    return {
        "sum": sum(values), "minimum": min(values), "maximum": max(values),
        "median": median(values),
        "frequency": {str(k): v for k, v in sorted(Counter(values).items())},
    }


def compare(rows, candidate, reference):
    deltas = [row["nodes"][candidate] - row["nodes"][reference] for row in rows]
    winning = [row for row, delta in zip(rows, deltas) if delta < 0]
    return {
        "candidate": candidate, "reference": reference,
        "candidate_nodes": sum(row["nodes"][candidate] for row in rows),
        "reference_nodes": sum(row["nodes"][reference] for row in rows),
        "strict_wins": sum(delta < 0 for delta in deltas),
        "ties": sum(delta == 0 for delta in deltas),
        "regressions": sum(delta > 0 for delta in deltas),
        "winning_families": sorted({row["family"] for row in winning}),
        "winning_case_ids": [row["id"] for row in winning],
        "node_delta_distribution": distribution(deltas),
    }


def analyze(corpus, corpus_hash, results):
    require(corpus_hash == FROZEN_CORPUS_SHA256, "preregistered corpus identity changed")
    require(results.get("schema_version") == 1, "unsupported Rust result schema")
    require(results.get("corpus_sha256") == corpus_hash, "Rust result uses another corpus")
    require(results.get("gate", {}).get("passed") is True, "Rust verification/tool gate failed")
    controls = results.get("controls", [])
    require(len(controls) == len(CONTROL_IDS), "negative controls are missing or extra")
    require({control.get("id") for control in controls} == set(CONTROL_IDS), "negative-control identity drift")
    require(all(control.get("passed") is True for control in controls), "a negative/control check failed")
    frozen = {case["id"]: case for case in corpus["cases"]}
    observed = results["cases"]
    require(len(observed) == len(frozen) == 100, "case count mismatch")
    require(len({case["id"] for case in observed}) == len(observed), "duplicate Rust case")
    require({case["id"] for case in observed} == set(frozen), "case set mismatch")
    rows, timing_samples, solver_statuses, policy_rows = [], {}, {}, []
    for case in observed:
        original = frozen[case["id"]]
        require((case["nvars"], case["family"]) == (original["nvars"], original["family"]), "case definition drift")
        variants = case["variants"]
        require(all(name in variants for name in REQUIRED_VARIANTS), "missing comparison variant")
        nodes = {}
        for name, variant in variants.items():
            where = f"{case['id']}/{name}"
            require(variant["status"] == "accepted" and not variant.get("error"), f"unaccepted variant: {where}")
            if name in ("rewrite_egg", "semantic_egg"):
                require(variant.get("optimization", {}).get("solver_status", "").lower() == "optimal", f"nonoptimal or unrecorded extraction solver: {where}")
                solver_statuses.setdefault(name, Counter())[variant["optimization"]["solver_status"]] += 1
            outputs = [parse(source) for source in variant["outputs"]]
            require(len(outputs) == len(original["outputs"]), f"output order/arity drift: {where}")
            actual = signatures(outputs, case["nvars"])
            require(actual == original["truth_signatures"], f"Python expression mismatch: {where}")
            evaluation = variant["evaluation"]
            require(evaluation["gate_passed"] is True and not evaluation.get("mismatches"), f"Rust case gate failed: {where}")
            require(evaluation["truth_signatures"] == actual, f"Rust truth signature differs: {where}")
            require(evaluation["valid_tuples"] == 1 << case["nvars"], f"incomplete Rust tuple coverage: {where}")
            expected_trace, invalid_count = expected_observation_trace(actual, case["nvars"])
            require(evaluation.get("checks") == expected_trace, f"complete ordered observation trace differs: {where}")
            require(evaluation.get("invalid_tuples") == invalid_count, f"invalid-input probe count differs: {where}")
            require(program_signatures(variant["program"], case["nvars"], len(outputs)) == actual, f"Python serialized-program mismatch: {where}")
            independent_cost = cost(outputs)
            require(independent_cost == variant["fcis_nodes"] == len(variant["program"]["nodes"]), f"FCIS node count mismatch: {where}")
            require(independent_cost <= 256, f"FCIS node bound: {where}")
            if name in ("raw", "local_baseline"):
                expected = original["outputs" if name == "raw" else "baseline_outputs"]
                require(outputs == [parse(source) for source in expected], f"frozen variant drift: {where}")
            nodes[name] = independent_cost
            for component, value in variant.get("timings_ms", {}).items():
                require(type(value) in (int, float) and math.isfinite(value) and value >= 0, f"invalid timing: {where}/{component}")
                timing_samples.setdefault(name, {}).setdefault(component, []).append(value)
            if name in ("rewrite_egg", "semantic_egg", "tree_egg"):
                for component in ("saturation", "extraction"):
                    value = variant.get("optimization", {}).get(component + "_ms")
                    require(type(value) in (int, float) and math.isfinite(value) and value >= 0, f"invalid optimizer timing: {where}/{component}")
                    timing_samples.setdefault(name, {}).setdefault(component, []).append(value)
        rows.append({"id": case["id"], "family": case["family"], "nvars": case["nvars"], "nodes": nodes})
        if "accepted_policy" in case:
            strict_decrease = nodes["semantic_egg"] < nodes["local_baseline"]
            chosen = "semantic_egg" if strict_decrease else "local_baseline"
            policy = case["accepted_policy"]
            require(policy.get("chosen_variant") == chosen and policy.get("strict_actual_decrease") is strict_decrease and policy.get("fcis_nodes") == nodes[chosen], f"protected selection differs: {case['id']}")
            policy_rows.append({"id": case["id"], "chosen_variant": chosen, "fcis_nodes": nodes[chosen]})
    rows.sort(key=lambda row: list(frozen).index(row["id"]))
    require({family: sum(row["family"] == family for row in rows) for family in FAMILIES} == FAMILIES, "preregistered family counts drifted")
    comparisons = {
        "semantic_vs_local": compare(rows, "semantic_egg", "local_baseline"),
        "rewrite_vs_local": compare(rows, "rewrite_egg", "local_baseline"),
        "semantic_vs_rewrite": compare(rows, "semantic_egg", "rewrite_egg"),
        "local_vs_raw": compare(rows, "local_baseline", "raw"),
    }
    criterion = comparisons["semantic_vs_local"]
    success = criterion["candidate_nodes"] < criterion["reference_nodes"] and criterion["strict_wins"] >= 10 and len(criterion["winning_families"]) >= 2
    observed_config = results.get("config", {})
    initial_search_unchanged = all(observed_config.get(key) == value for key, value in INITIAL_SEARCH_CONFIG.items())
    timings = {
        name: {component: {"samples": len(values), "sum_ms": sum(values), "median_ms": median(values), "minimum_ms": min(values), "maximum_ms": max(values)} for component, values in components.items()}
        for name, components in timing_samples.items()
    }
    return {
        "schema_version": 1, "corpus_sha256": corpus_hash, "cases": len(rows),
        "independent_checks_passed": True,
        "success_rule": corpus["success_rule"], "fixed_cost_criterion_met": success,
        "preregistered_success": success and initial_search_unchanged,
        "observed_search_configuration": observed_config,
        "initial_search_configuration": INITIAL_SEARCH_CONFIG,
        "initial_search_configuration_unchanged": initial_search_unchanged,
        "aggregate_nodes": {name: sum(row["nodes"][name] for row in rows) for name in REQUIRED_VARIANTS},
        "comparisons": comparisons,
        "families": [{"family": family, "cases": len(selected), "aggregate_nodes": {name: sum(row["nodes"][name] for row in selected) for name in REQUIRED_VARIANTS}, "semantic_vs_local": compare(selected, "semantic_egg", "local_baseline"), "semantic_vs_rewrite": compare(selected, "semantic_egg", "rewrite_egg")} for family in FAMILIES for selected in [[row for row in rows if row["family"] == family]]],
        "timings": timings, "timing_component_definitions": TIMING_COMPONENTS,
        "controls": results.get("controls", []),
        "protected_selection": {"receipt_cases_checked": len(policy_rows), "complete": len(policy_rows) == len(rows), "aggregate_nodes": sum(row["fcis_nodes"] for row in policy_rows) if len(policy_rows) == len(rows) else None, "chosen_variants": dict(Counter(row["chosen_variant"] for row in policy_rows))},
        "extraction_solver_statuses": solver_statuses,
        "scope": "Complete finite Bool domains per frozen case, exact output order and duplicate roots, eager Boolean FCIS basis only.",
        "limitations": [
            "No arithmetic/error-producing expression, authoritative state, effects, shell behavior, temporal or weak-omega equivalence is established by this corpus.",
            "This is a deterministic synthetic sample, not an application distribution or an optimum-extraction theorem.",
            "Recorded solver optimality applies to the extraction surrogate, not necessarily actual shared FCIS-basis node count.",
            "Rust timings are component observations; baseline outputs were precomputed in Python, so lowering time alone omits local optimization overhead.",
            "Fewer modeled DAG instructions need not imply faster generated code or lower protocol-visible logical budgets.",
        ],
    }


def markdown(summary):
    config = summary["observed_search_configuration"]
    lines = [f"Independent checks passed for {summary['cases']} frozen cases.", "", f"Fixed node-count criterion: **{'passed' if summary['fixed_cost_criterion_met'] else 'not met'}**.", "", f"Observed search: {config.get('iterations')} iterations, {config.get('node_limit')} nodes as a threshold checked between rewrite batches, and {config.get('lp_seconds')} seconds as the LP solving budget."]
    if not summary["initial_search_configuration_unchanged"]:
        lines.extend(["", "This search configuration was amended after the initial run. Meeting the fixed cost threshold in this rerun does not establish success under the original registered search protocol."])
    lines.extend(["", "| Family | Cases | Raw | Local | Rewrite egg | Semantic egg | Semantic wins vs local |", "| --- | ---: | ---: | ---: | ---: | ---: | ---: |"])
    for family in summary["families"]:
        counts = family["aggregate_nodes"]
        lines.append(f"| {family['family']} | {family['cases']} | {counts['raw']} | {counts['local_baseline']} | {counts['rewrite_egg']} | {counts['semantic_egg']} | {family['semantic_vs_local']['strict_wins']} |")
    lines.extend(["", "All comparisons use actual shared FCIS-basis node counts. Negative deltas mean fewer nodes.", ""])
    for name, comparison in summary["comparisons"].items():
        lines.append(f"- {name}: {comparison['candidate_nodes']} vs {comparison['reference_nodes']} nodes; {comparison['strict_wins']} wins, {comparison['ties']} ties, {comparison['regressions']} regressions.")
    policy = summary["protected_selection"]
    if policy["complete"]:
        lines.extend(["", f"Independently checked protected selection uses a semantic candidate only when it strictly reduces actual nodes; otherwise it retains the checked local baseline. Its aggregate is {policy['aggregate_nodes']} nodes over {policy['receipt_cases_checked']} cases, with selected variants {policy['chosen_variants']}."])
    lines.extend(["", "Descriptive Rust timing observations (milliseconds):", "", "| Variant | Component | Samples | Sum | Median | Maximum |", "| --- | --- | ---: | ---: | ---: | ---: |"])
    for name, components in summary["timings"].items():
        for component in ("saturation", "extraction", "evaluation", "total"):
            if component in components:
                timing = components[component]
                lines.append(f"| {name} | {component} | {timing['samples']} | {timing['sum_ms']:.3f} | {timing['median_ms']:.3f} | {timing['maximum_ms']:.3f} |")
    lines.extend(["", "Timing components:", ""] + [f"- {name}: {meaning}" for name, meaning in summary["timing_component_definitions"].items()])
    if "separate_python_baseline_timings" in summary:
        timing = summary["separate_python_baseline_timings"]
        lines.extend(["", f"Separately measured Python {timing['python_version']} baseline search: {timing['repetitions']} repetitions per case, {timing['sum_case_medians_ms']:.3f} ms sum of case medians and {timing['median_case_ms']:.3f} ms median case. This cross-language observation does not establish a fair algorithm-speed ratio."])
    lines.extend(["", "Limits:", ""] + [f"- {limit}" for limit in summary["limitations"]])
    return "\n".join(lines) + "\n"


def baseline_timings(path, corpus):
    """Read separately captured Python timings without claiming speed ratios."""
    data = path.read_bytes()
    measurement = json.loads(data)
    frozen = {case["id"]: case for case in corpus["cases"]}
    rows = measurement["cases"]
    require(len(rows) == len(frozen) and {row["id"] for row in rows} == set(frozen), "baseline timing case set mismatch")
    values = []
    for row in rows:
        samples = row["samples_ms"]
        require(len(samples) == measurement["repetitions"] and bool(samples), "baseline timing repetitions")
        require(all(type(sample) in (int, float) and math.isfinite(sample) and sample >= 0 for sample in samples), "invalid baseline timing")
        require(row["steps"] == frozen[row["id"]]["baseline_steps"], "baseline timing step count drift")
        require(math.isclose(median(samples), row["median_ms"], abs_tol=1e-10), "baseline timing median drift")
        values.append(row["median_ms"])
    return {
        "sha256": hashlib.sha256(data).hexdigest(), "cases": len(rows),
        "python_version": measurement["python_version"], "repetitions": measurement["repetitions"],
        "sum_case_medians_ms": sum(values), "median_case_ms": median(values),
        "measurement": measurement["measurement"],
    }


def self_test(corpus, corpus_hash):
    """Schema/gate checks on fabricated fixtures, not experiment observations."""
    def variant(case, key):
        expressions = [parse(source) for source in case[key]]
        instructions, roots = lower(expressions)
        serialized = []
        for instruction in instructions:
            op, *args = instruction
            value = {"op": op, "args": args}
            if op == "input":
                value.update(index=args[0], args=[])
            elif op == "bool":
                value.update(value=args[0], args=[])
            serialized.append(value)
        checks, invalid_count = expected_observation_trace(case["truth_signatures"], case["nvars"])
        return {
            "status": "accepted", "outputs": case[key], "fcis_nodes": len(instructions),
            "program": {"inputs": [{"kind": "bool"}] * case["nvars"], "outputs": [{"kind": "bool"}] * len(expressions), "nodes": serialized, "roots": roots},
            "evaluation": {"valid_tuples": 1 << case["nvars"], "invalid_tuples": invalid_count, "truth_signatures": case["truth_signatures"], "checks": checks, "gate_passed": True, "mismatches": []},
            "timings_ms": {}, "error": None, "optimization": {"solver_status": "optimal", "saturation_ms": 0.0, "extraction_ms": 0.0},
        }

    fixture = {"schema_version": 1, "corpus_sha256": corpus_hash, "config": INITIAL_SEARCH_CONFIG, "gate": {"passed": True}, "controls": [{"id": control_id, "expected_outcome": "fabricated schema self-test", "passed": True} for control_id in CONTROL_IDS], "cases": []}
    for case in corpus["cases"]:
        fixture["cases"].append({"id": case["id"], "nvars": case["nvars"], "family": case["family"], "variants": {name: variant(case, "outputs" if name == "raw" else "baseline_outputs") for name in REQUIRED_VARIANTS}, "accepted_policy": {"chosen_variant": "local_baseline", "strict_actual_decrease": False, "fcis_nodes": case["baseline_fcis_nodes"]}})
    summary = analyze(corpus, corpus_hash, fixture)
    require(summary["preregistered_success"] is False, "tie-only fixture claimed success")

    def bad_truth(data):
        data["cases"][0]["variants"]["semantic_egg"]["outputs"] = ["(not v0)"]

    def bad_domain(data):
        data["cases"][0]["variants"]["semantic_egg"]["program"]["inputs"][0] = {"kind": "int"}

    def bad_reference(data):
        data["cases"][0]["variants"]["semantic_egg"]["program"]["nodes"][0]["args"] = [0]

    def bad_cost(data):
        data["cases"][0]["variants"]["semantic_egg"]["fcis_nodes"] += 1

    def bad_gate(data):
        data["gate"]["passed"] = False

    def bad_hash(data):
        data["corpus_sha256"] = "0" * 64

    def bad_case_set(data):
        data["cases"][1]["id"] = data["cases"][0]["id"]

    def bad_coverage(data):
        data["cases"][0]["variants"]["semantic_egg"]["evaluation"]["valid_tuples"] -= 1

    def bad_control(data):
        data["controls"][0]["passed"] = False

    def bad_solver(data):
        data["cases"][0]["variants"]["semantic_egg"]["optimization"]["solver_status"] = "TimeLimit"

    def bad_optimization_timing(data):
        data["cases"][0]["variants"]["semantic_egg"]["optimization"]["extraction_ms"] = float("nan")

    def bad_trace(data):
        data["cases"][0]["variants"]["semantic_egg"]["evaluation"]["checks"][-1]["candidate"] = {"ok": [0]}

    def bad_protected_selection(data):
        data["cases"][0]["accepted_policy"]["fcis_nodes"] += 1

    mutations = [bad_truth, bad_domain, bad_reference, bad_cost, bad_gate, bad_hash, bad_case_set, bad_coverage, bad_control, bad_solver, bad_optimization_timing, bad_trace, bad_protected_selection]
    for mutation in mutations:
        changed = copy.deepcopy(fixture)
        mutation(changed)
        try:
            analyze(corpus, corpus_hash, changed)
        except ValueError:
            continue
        raise AssertionError(f"analyzer admitted {mutation.__name__}")
    return {"self_test": "passed", "fabricated_positive_cases": len(fixture["cases"]), "negative_mutations_rejected": len(mutations), "experiment_results": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("results", type=Path, nargs="?")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--corpus", type=Path, default=Path(__file__).with_name("corpus.json"))
    parser.add_argument("--out", type=Path)
    parser.add_argument("--markdown", type=Path)
    parser.add_argument("--baseline-timings", type=Path)
    args = parser.parse_args()
    data = args.corpus.read_bytes()
    if args.self_test:
        print(json.dumps(self_test(json.loads(data), hashlib.sha256(data).hexdigest())))
        return
    if args.results is None:
        parser.error("results file is required unless --self-test is selected")
    corpus = json.loads(data)
    summary = analyze(corpus, hashlib.sha256(data).hexdigest(), json.loads(args.results.read_text()))
    if args.baseline_timings:
        summary["separate_python_baseline_timings"] = baseline_timings(args.baseline_timings, corpus)
    text = json.dumps(summary, indent=2, sort_keys=True) + "\n"
    if args.out:
        args.out.write_text(text)
    else:
        print(text, end="")
    if args.markdown:
        args.markdown.write_text(markdown(summary))


if __name__ == "__main__":
    main()
