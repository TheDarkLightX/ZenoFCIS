"""Bounded semantic cases, independent enumeration and comparison self-tests."""
from __future__ import annotations

import copy
import hashlib
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
    return {"status": a["status"], "authority": "none", "integer_encoding": R.INTEGER_ENCODING, "detail": {
        **{k: a[k] for k in ("counts", "before", "by_size", "core")}, "question": R.reported_question(a["question"]),
        "complete": True,
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
        spec["inputs"][0]["domain"] = {"kind": "Int", "min": R.I128_MIN, "max": R.I128_MIN + 1}
        spec["outputs"] = [spec["inputs"][0]["domain"].copy()]
        spec["vocabulary"] = {"constants": [R.I128_MIN], "arithmetic": ["-"], "macros": ["abs"]}
        spec["max_size"] = 3
        a = R.analyze(spec)
        self.assertLess(a["before"][0], sum(row[0] for row in a["by_size"]))
        self.assertEqual(R.apply("abs", [R.value(R.I128_MIN)]), R.ARITHMETIC)
        self.assertEqual(R.apply("+", [R.value(R.I128_MAX), R.value(1)]), R.ARITHMETIC)

    def test_arithmetic_is_checked_at_the_i128_width(self):
        # The former i64 bounds are ordinary values of the current profile.
        i64_min, i64_max = -(1 << 63), (1 << 63) - 1
        self.assertEqual(R.apply("abs", [R.value(i64_min)]), R.value(1 << 63))
        self.assertEqual(R.apply("+", [R.value(i64_max), R.value(1)]), R.value(1 << 63))
        self.assertEqual(R.apply("-", [R.value(i64_min), R.value(1)]), R.value(i64_min - 1))
        self.assertEqual(R.apply("-", [R.value(R.I128_MIN), R.value(1)]), R.ARITHMETIC)

    def test_question_values_are_compared_in_the_report_integer_encoding(self):
        spec = declaration()
        spec["inputs"][0]["domain"] = {"kind": "Int", "min": -(1 << 63), "max": -(1 << 63) + 1}
        spec["outputs"] = [spec["inputs"][0]["domain"].copy()]
        spec["vocabulary"] = {"constants": [-(1 << 63)], "arithmetic": ["-"], "macros": ["abs"]}
        spec["max_size"] = 3
        good = fixture_report(spec)
        question = good["detail"]["question"]
        self.assertEqual(question["input"], [str(-(1 << 63) + 1)])
        self.assertTrue(R.check_report(spec, good))
        unencoded = copy.deepcopy(good)
        unencoded["detail"]["question"]["input"] = [-(1 << 63) + 1]
        undeclared = copy.deepcopy(good)
        del undeclared["integer_encoding"]
        for bad in (unencoded, undeclared):
            self.assertFalse(R.check_report(spec, bad))
        self.assertEqual([R.reported_integer(n) for n in (R.SAFE_INTEGER, -R.SAFE_INTEGER, 1 << 53)],
                         [R.SAFE_INTEGER, -R.SAFE_INTEGER, str(1 << 53)])

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
        # A historical finite-i64/1 artifact keeps its own profile and bytes.
        self.assertEqual(R.program_digest(decode(body)), hashlib.sha256(body).hexdigest())

    def test_reported_program_digest_uses_the_current_i128_profile(self):
        # x + 1, encoded by hand from the public ZCVE/1 tags, independent of any encoder.
        def signed(n):
            return "04" + n.to_bytes(16, "big", signed=True).hex()
        profile = b"zeno-fcis/finite-i128/1"
        expected = bytes.fromhex(
            "0800000004"                                            # (profile, schema, nodes, roots)
            + "06" + len(profile).to_bytes(4, "big").hex() + profile.hex()
            + "0800000002"                                          # (inputs, outputs)
            + "0800000001" + "0800000003" + "01" + signed(0) + signed(2)
            + "0800000001" + "0800000003" + "01" + signed(-10) + signed(10)
            + "0800000003"                                          # nodes
            + "0800000002" + signed(0) + signed(0)                  # Input 0
            + "0800000002" + signed(1) + signed(1)                  # Int 1
            + "0800000003" + signed(3) + signed(0) + signed(1)      # Add 0 1
            + "0800000001" + "03" + (2).to_bytes(16, "big").hex())  # roots
        reported = {"inputs": [{"kind": "Int", "min": "0", "max": "2"}],
                    "outputs": [{"kind": "Int", "min": "-10", "max": "10"}],
                    "nodes": [["Input", 0], ["Int", "1"], ["Add", 0, 1]], "roots": [2]}
        program = R.program_fixture(reported)
        self.assertEqual(R.program_digest(program), hashlib.sha256(expected).hexdigest())
        self.assertEqual(decode(expected), program)

    def test_reported_programs_rebuild_committed_i128_program_bytes(self):
        # Candidates are reported as JSON; rebuilding committed current-profile
        # programs from that form must reproduce their canonical bytes exactly.
        names = {0: "Input", 1: "Int", 2: "Bool", 3: "Add", 4: "Sub", 5: "Eq", 6: "Lt", 7: "And", 8: "Not",
                 9: "Select"}

        def domain(d):
            return {"kind": "Bool"} if d[0] else {"kind": "Int", "min": str(d[1]), "max": str(d[2])}

        def node(n):
            if n[0] == 1:
                return ["Int", str(n[1])]
            if n[0] == 2:
                return ["Bool", bool(n[1])]
            return [names[n[0]], *n[1:]]
        root = Path(__file__).resolve().parents[1]
        paths = sorted((root / "crates/zeno-fcis-cli/tests/fixtures/i128-door-programs").glob("*.zcve"))
        self.assertTrue(paths)
        for path in paths:
            with self.subTest(path.name):
                body = path.read_bytes()
                program = decode(body)
                self.assertEqual(program.profile, b"zeno-fcis/finite-i128/1")
                reported = {"inputs": [domain(d) for d in program.inputs],
                            "outputs": [domain(d) for d in program.outputs],
                            "nodes": [node(n) for n in program.nodes], "roots": list(program.roots)}
                self.assertEqual(R.program_digest(R.program_fixture(reported)), hashlib.sha256(body).hexdigest())


if __name__ == "__main__":
    unittest.main()
