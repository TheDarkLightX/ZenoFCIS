"""Bounded semantic cases, independent enumeration and comparison self-tests."""
from __future__ import annotations

import copy
import json
from pathlib import Path
import unittest
from unittest.mock import patch

import loop_learn_reference as R
from program_search_reference import decode


def declaration():
    return {"schema": R.SCHEMA, "inputs": [{"name": "x", "domain": {"kind": "Int", "min": 0, "max": 2}}],
            "outputs": [{"kind": "Int", "min": -10, "max": 10}], "vocabulary": {"constants": [0], "arithmetic": []},
            "max_size": 1, "examples": []}


def fixture_report(spec):
    a = R.analyze(spec)
    # These test declarations have x as their smallest representative.
    program = {"inputs": [i["domain"] for i in spec["inputs"]], "outputs": spec["outputs"],
               "nodes": [["Input", 0]], "roots": [0]}
    candidate = None if a["status"] == "outside-vocabulary" else {
        "program": program, "sha256": R.program_digest(R.program_fixture(program)), "status": "examples-reproduced",
        "examples_check": {"decided_by": "library-evaluator", "checked": len(spec["examples"]),
                           "vacuous": not spec["examples"], "verdict": "reproduced"}}
    return {"status": a["status"], "authority": "none", "detail": {
        **{k: a[k] for k in ("counts", "before", "by_size", "question", "core")}, "complete": True,
        "enumeration": "proposal", "attempts_spent": 0, "candidate": candidate,
        "representatives": [] if candidate is None else [{"size": s} for s in a["minimum_sizes"]]}}


def comparison_cases():
    spec = declaration()
    good = fixture_report(spec)
    wrong_count = copy.deepcopy(good)
    wrong_count["detail"]["counts"][0] += 1
    wrong_question = copy.deepcopy(good)
    wrong_question["detail"]["question"]["ordinal"] = 2
    outside = copy.deepcopy(spec)
    outside["examples"] = [{"input": [0], "output": [7]}]
    good_outside = fixture_report(outside)
    wrong_core = copy.deepcopy(good_outside)
    wrong_core["detail"]["core"] = []
    determined = copy.deepcopy(spec)
    determined["examples"] = [{"input": [1], "output": [1]}]
    good_determined = fixture_report(determined)
    wrong_program = copy.deepcopy(good_determined)
    wrong_program["detail"]["candidate"]["program"]["nodes"] = [["Int", "0"]]
    wrong_program["detail"]["candidate"]["sha256"] = R.program_digest(R.program_fixture(wrong_program["detail"]["candidate"]["program"]))
    alias_count = copy.deepcopy(good_determined)
    alias_count["detail"]["counts"] = [True]
    return [(spec, good, True), (spec, wrong_count, False), (spec, wrong_question, False),
            (outside, good_outside, True), (outside, wrong_core, False),
            (determined, good_determined, True), (determined, wrong_program, False), (determined, alias_count, False)]


def comparison_self_test():
    """Must return False (FAIL) when the reference comparison is stubbed."""
    return all(R.check_report(spec, report) is expected for spec, report, expected in comparison_cases())


class Semantics(unittest.TestCase):
    def test_declared_semantic_fixture_statuses_are_independent_reference_cases(self):
        path = Path(__file__).parent / "test_data/loop_learn/cases.json"
        for case in json.loads(path.read_text())["cases"]:
            with self.subTest(case=case["id"]):
                self.assertEqual(R.analyze(case["spec"])["status"], case["status"])

    def test_syntax_goldens_and_exact_minimax_tie_break(self):
        spec = declaration()
        spec["inputs"].append({"name": "y", "domain": {"kind": "Int", "min": 0, "max": 2}})
        spec["vocabulary"] = {"constants": [1], "arithmetic": ["+", "-"]}
        spec["max_size"] = 3
        a = R.analyze(spec)
        self.assertEqual(a["by_size"], [[3, 0], [0, 0], [13, 0]])
        self.assertEqual(a["counts"], [16])
        self.assertEqual((a["question"]["worst"], a["question"]["squares"], a["question"]["ordinal"]), ("3", "44", 8))
        spec["outputs"] *= 2
        q = R.analyze(spec)["question"]
        self.assertEqual((q["worst"], q["squares"]), ("9", "1936"))

    def test_standard_macros_boolean_and_conditionals_are_in_the_declared_language(self):
        spec = declaration()
        spec["inputs"].append({"name": "y", "domain": {"kind": "Int", "min": 0, "max": 2}})
        spec["vocabulary"] = {"constants": [], "arithmetic": [], "macros": ["min", "max"]}
        spec["max_size"] = 3
        spec["examples"] = [{"input": [1, 2], "output": [1]}, {"input": [2, 0], "output": [0]},
                            {"input": [0, 2], "output": [0]}]
        a = R.analyze(spec)
        self.assertEqual((a["status"], a["minimum_sizes"]), ("determined", [3]))
        spec["outputs"] = [{"kind": "Bool"}]
        spec["vocabulary"] = {"constants": [1], "arithmetic": [], "compare": ["<", "<="], "boolean": ["!"]}
        spec["examples"] = [{"input": [x, y], "output": [int(x < y)]} for x in range(3) for y in range(3)]
        self.assertEqual(R.analyze(spec)["status"], "determined")
        self.assertEqual(R.apply("?", [R.value(0), R.value(4), R.value(7)]), R.value(7))
        self.assertEqual(R.apply("?", [R.value(1), R.value(4), R.ARITHMETIC]), R.ARITHMETIC)

    def test_exhaustive_syntax_at_size_six_is_quotiented_only_after_generation(self):
        spec = declaration()
        spec["inputs"].append({"name": "y", "domain": {"kind": "Int", "min": 0, "max": 2}})
        spec["vocabulary"] = {"constants": [1], "arithmetic": ["+", "-"], "compare": ["<", "=="],
                              "boolean": ["&&", "!"], "conditional": True}
        spec["max_size"] = 6
        a = R.analyze(spec)
        self.assertGreater(sum(row[0] for row in a["by_size"]), 16)
        self.assertGreater(sum(row[1] for row in a["by_size"]), 2)

    def test_conflict_is_a_smallest_set_including_cores_larger_than_four(self):
        spec = declaration()
        spec["inputs"].append({"name": "y", "domain": {"kind": "Int", "min": 0, "max": 2}})
        spec["vocabulary"]["constants"] = [1]
        spec["examples"] = [{"input": [0, 0], "output": [0]}, {"input": [1, 2], "output": [2]},
                            {"input": [2, 1], "output": [2]}]
        self.assertEqual(R.analyze(spec)["core"], [1, 2])
        # Six Boolean inputs, one leaf each. Each example excludes precisely one
        # leaf; no set of five examples excludes all six. No four-example cap.
        spec = {"schema": R.SCHEMA, "inputs": [{"name": f"b{k}", "domain": {"kind": "Bool"}} for k in range(6)],
                "outputs": [{"kind": "Bool"}], "max_size": 1, "vocabulary": {"constants": [], "arithmetic": []},
                "examples": [{"input": [int(j == k) for j in range(6)], "output": [0]} for k in range(6)]}
        self.assertEqual(len(R.analyze(spec)["core"]), 6)

    def test_raw_values_are_retained_until_root_projection_and_failures_use_tags(self):
        spec = declaration()
        spec["inputs"][0]["domain"] = {"kind": "Int", "min": R.I64_MIN, "max": R.I64_MIN + 1}
        spec["outputs"] = [spec["inputs"][0]["domain"].copy()]
        spec["vocabulary"] = {"constants": [R.I64_MIN], "arithmetic": ["-"], "macros": ["abs"]}
        spec["max_size"] = 3
        a = R.analyze(spec)
        self.assertLess(a["before"][0], sum(row[0] for row in a["by_size"]))
        self.assertEqual(R.apply("abs", [R.value(R.I64_MIN)]), R.ARITHMETIC)
        self.assertEqual(R.apply("+", [R.value(R.I64_MAX), R.value(1)]), R.ARITHMETIC)

    def test_comparison_self_test_kills_each_planted_fault_and_fails_when_stubbed(self):
        self.assertTrue(comparison_self_test())
        with patch.object(R, "agree", return_value=True):
            self.assertFalse(comparison_self_test())
        with patch.object(R, "agree", return_value=False):
            self.assertFalse(comparison_self_test())

    def test_digest_matches_an_existing_canonical_program_fixture(self):
        root = Path(__file__).resolve().parents[1]
        path = root / "docs/benchmarks/withdrawal-queue/artifacts/boolean-kernel-original.zcve"
        body = path.read_bytes()
        import hashlib
        self.assertEqual(R.program_digest(decode(body)), hashlib.sha256(body).hexdigest())


if __name__ == "__main__":
    unittest.main()
