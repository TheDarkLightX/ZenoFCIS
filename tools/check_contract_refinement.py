#!/usr/bin/env python3
"""Compare a built refine-check CLI with independent finite semantics.

No build is started here. --controls plants contract and abstraction faults.
--all-invalid-rows adds 228 native invalid-row faults. --self-test demonstrates
that stubbing the report comparison fails its controls; --stub-comparison must
exit nonzero. A pass is evidence for these finite subjects and this binary only.
"""
# SPDX-License-Identifier: MIT OR Apache-2.0
# LLM-generated implementation; checked by the repository's qualification gates.
import argparse
import copy
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

import make_refinement_example as fixture
import refinement_reference as ref


class Mismatch(AssertionError):
    pass


def projection(answer):
    witness = None if answer.obligation is None else {
        "obligation": answer.obligation, "state": answer.state, "input": answer.input}
    return {"verdict": answer.verdict, "checked_rows": answer.checked_rows, "witness": witness}


def compare(actual, expected):
    encode = lambda value: json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False)
    return [field for field, value in expected.items()
            if field not in actual or encode(actual[field]) != encode(value)]


def comparison_controls(comparator):
    expected = projection(ref.Answer("counterexample", 58, "successor-refinement", 9, 3))
    if comparator(expected, expected):
        raise Mismatch("comparison rejects an equal report")
    for field, value in (("verdict", "refines"), ("checked_rows", 57), ("witness", None)):
        bad = copy.deepcopy(expected)
        bad[field] = value
        if not comparator(bad, expected):
            raise Mismatch(f"disabled comparison: failed to detect planted {field} fault")
    expected = projection(ref.Answer("inconclusive", 1))
    bad = dict(expected, checked_rows=True)
    if not comparator(bad, expected):
        raise Mismatch("comparison accepted boolean/integer alias")


def self_test():
    comparison_controls(compare)
    try:
        comparison_controls(lambda *_: [])
    except Mismatch:
        return
    raise Mismatch("stubbed comparison incorrectly passed")


def repin(files):
    mapping = json.loads(files["abstraction.json"])
    for label in ("abstract", "concrete"):
        for kind, name in (("project", "project.zeno"), ("policy", "v2/policy.json")):
            mapping[f"{label}_{kind}_sha256"] = hashlib.sha256(files[f"{label}/{name}"].encode()).hexdigest()
    files["abstraction.json"] = fixture.dump(mapping)


def edit_policy(files, state, command, change):
    policy = json.loads(files["concrete/v2/policy.json"])
    row = next(case for case in policy["cases"] if case["when"] == f"state == {state} && action == {command}")
    change(row)
    files["concrete/v2/policy.json"] = fixture.dump(policy)
    repin(files)


def expected(files, budget):
    tables = [ref.fixture_table(files[f"{label}/project.zeno"],
                               json.loads(files[f"{label}/v2/policy.json"]))
              for label in ("abstract", "concrete")]
    mapping = json.loads(files["abstraction.json"])
    return ref.check(*tables, tuple(mapping["abstraction"]), ref.Observation("Refused", mapping["invalid_state_law"]), budget)


def subjects(controls):
    original = {name: (fixture.EXAMPLE / name).read_text() for name in fixture.files()}
    for budget in (0, 1, 383, 384):
        yield f"budget-{budget}", original, budget, expected(original, budget)
    if not controls:
        return
    for name, state, command, mutate in (
        ("lost-retirement", 9, 3, lambda row: row["post"].update({"110": "0"})),
        ("payload", 0, 0, lambda row: row["outbox"][0]["payload"].update({"140": "1"})),
        ("destination", 0, 0, lambda row: row["outbox"][0].update(destination="elsewhere")),
        ("idempotency", 0, 0, lambda row: row["outbox"][0].update(idempotency_ordinal=1)),
        ("ordinal", 0, 0, lambda row: row["outbox"][0].update(ordinal=1)),
        ("delivery-count", 0, 0, lambda row: row.update(outbox=[])),
        ("reason", 9, 0, lambda row: row.update(reason=201)),
        ("valid-to-invalid", 0, 0, lambda row: row["post"].update({"110": "1"})),
    ):
        files = dict(original)
        edit_policy(files, state, command, mutate)
        answer = expected(files, 384)
        if answer.verdict != "counterexample":
            raise Mismatch(f"fault not semantically effective: {name}")
        yield name, files, 384, answer
    # Two admitted, ordered deliveries first agree; exchanging their content
    # then exercises the observation comparison, not a malformed ordinal list.
    files = dict(original)
    for label in ("abstract", "concrete"):
        policy = json.loads(files[f"{label}/v2/policy.json"])
        deliveries = policy["cases"][0]["outbox"]
        second = copy.deepcopy(deliveries[0])
        second.update(ordinal=1, idempotency_ordinal=1)
        second["payload"]["140"] = "1"
        deliveries.append(second)
        files[f"{label}/v2/policy.json"] = fixture.dump(policy)
    repin(files)
    yield "ordered-positive", files, 384, expected(files, 384)
    policy = json.loads(files["concrete/v2/policy.json"])
    policy["cases"][0]["outbox"][0]["payload"]["140"] = "1"
    policy["cases"][0]["outbox"][1]["payload"]["140"] = "0"
    files = dict(files)
    files["concrete/v2/policy.json"] = fixture.dump(policy)
    repin(files)
    yield "delivery-order", files, 384, expected(files, 384)
    files = dict(original)
    project = files["concrete/project.zeno"]
    law = next(line for line in project.splitlines() if line.startswith("law 500 "))
    files["concrete/project.zeno"] = project.replace(law, "law 500 valid_representation on any = true;")
    repin(files)
    yield "invalid-state", files, 384, expected(files, 384)
    for name in ("initial", "coverage"):
        files = dict(original)
        if name == "initial":
            policy = json.loads(files["concrete/v2/policy.json"])
            policy["genesis"]["110"] = 9
            files["concrete/v2/policy.json"] = fixture.dump(policy)
            repin(files)
        else:
            mapping = json.loads(files["abstraction.json"])
            mapping["abstraction"] = [0 if x == 1 else x for x in mapping["abstraction"]]
            files["abstraction.json"] = fixture.dump(mapping)
        yield name, files, 384, expected(files, 384)


def invalid_row_subjects():
    original = {name: (fixture.EXAMPLE / name).read_text() for name in fixture.files()}
    mapping = json.loads(original["abstraction.json"])["abstraction"]
    law = next(line for line in original["concrete/project.zeno"].splitlines() if line.startswith("law 500 "))
    for state, value in enumerate(mapping):
        if value is not None:
            continue
        for command in range(6):
            files = dict(original)
            # Exempt only this raw state/input pair from the malformed-state law.
            exception = f" || (pre.100.110 == {state} && command.101.120 == {command});"
            files["concrete/project.zeno"] = files["concrete/project.zeno"].replace(law, law[:-1] + exception)
            repin(files)
            yield (f"invalid-{state}-{command}", files, 384,
                   ref.Answer("counterexample", state * 6 + command + 1, "malformed-state", state, command))


def invoke(cli, directory, files, budget):
    for name, content in files.items():
        path = directory / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
    args = [str(cli), "contract", "refine-check", str(directory / "abstract"), str(directory / "concrete"),
            "--abstraction", str(directory / "abstraction.json"), "--max-checks", str(budget), "--format", "json"]
    result = subprocess.run(args, capture_output=True, text=True, timeout=120)
    try:
        report = json.loads(result.stdout)
    except ValueError as error:
        raise Mismatch(f"CLI returned non-JSON: {result.stderr[:400]}") from error
    return result.returncode, report


def run(cli, controls, all_invalid, comparator):
    comparison_controls(comparator)
    count = 0
    with tempfile.TemporaryDirectory(prefix="contract-refinement-") as temporary:
        directory = Path(temporary)
        cases = list(subjects(controls))
        if all_invalid:
            cases.extend(invalid_row_subjects())
        for name, files, budget, answer in cases:
            code, report = invoke(cli, directory, files, budget)
            expected_code = {"refines": 0, "counterexample": 1, "inconclusive": 2}[answer.verdict]
            errors = comparator(report, projection(answer))
            if code != expected_code or report.get("authority") != "none" or errors:
                raise Mismatch(f"{name}: exit {code}/{expected_code}, differences {errors}; report {report}")
            if name == "budget-384":
                baseline = {"raw_states": 64, "abstract_states": 26, "valid_states": 26, "inputs": 6, "authority_evaluations": 542}
                if comparator(report, baseline):
                    raise Mismatch("reservation domain or measured call count differs")
                # Replay means recomputation here; no persisted evidence is admitted.
                if invoke(cli, directory, files, budget) != (code, report):
                    raise Mismatch("repeat report differs")
            count += 1
        if controls:
            files = dict(cases[0][1])
            mapping = json.loads(files["abstraction.json"])
            mapping["concrete_policy_sha256"] = "0" * 64
            files["abstraction.json"] = fixture.dump(mapping)
            code, report = invoke(cli, directory, files, 384)
            if code != 65 or report.get("status") != "malformed":
                raise Mismatch("stale source pin was not refused")
            count += 1
    return count


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path)
    parser.add_argument("--controls", action="store_true")
    parser.add_argument("--all-invalid-rows", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--stub-comparison", action="store_true", help="negative control; must fail")
    args = parser.parse_args()
    comparator = (lambda *_: []) if args.stub_comparison else compare
    try:
        comparison_controls(comparator)
        if args.self_test:
            self_test()
            print("refinement comparison self-test: PASS (disabled comparison rejected)")
        if args.cli:
            count = run(args.cli.resolve(), args.controls, args.all_invalid_rows, comparator)
            print(f"refinement differential check: PASS ({count} subjects)")
        elif not args.self_test:
            parser.error("supply --cli or --self-test")
    except (Mismatch, ValueError, OSError, subprocess.SubprocessError) as error:
        print(f"refinement differential check: FAIL: {error}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
