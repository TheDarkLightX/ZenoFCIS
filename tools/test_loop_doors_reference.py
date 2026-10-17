#!/usr/bin/env python3
"""Tests of the independent loop-door reference: semantics, the fixture verdicts and the stubbed self-test."""
from __future__ import annotations

import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import loop_doors_reference as R  # noqa: E402

FIXTURES = json.loads((Path(__file__).resolve().parent / "test_data/loop_doors/fixtures.json").read_text())


class Semantics(unittest.TestCase):
    def program(self, nodes, roots, outputs=None):
        return {"inputs": [{"kind": "Int", "min": "0", "max": "2"}, {"kind": "Bool"}],
                "outputs": outputs or [{"kind": "Int", "min": "-10", "max": "10"}], "nodes": nodes, "roots": roots}

    def test_tuples_run_last_input_fastest(self):
        self.assertEqual(list(R.tuples(self.program([["Input", 0]], [0]))),
                         [(0, 0), (0, 1), (1, 0), (1, 1), (2, 0), (2, 1)])

    def test_evaluation_is_eager_and_checked(self):
        big = str((1 << 63) - 1)
        # the overflowing Add sits in the unselected arm, and still traps
        program = self.program([["Input", 1], ["Int", big], ["Int", "1"], ["Add", 1, 2], ["Input", 0],
                                ["Select", 0, 4, 3]], [5],
                               [{"kind": "Int", "min": "-9223372036854775808", "max": "9223372036854775807"}])
        self.assertEqual(R.observe(program, (1, 1)), R.TRAP)
        bounded = self.program([["Input", 0], ["Int", "5"], ["Add", 0, 1]], [2],
                               [{"kind": "Int", "min": "0", "max": "6"}])
        self.assertEqual(R.observe(bounded, (1, 0)), (6,))
        self.assertEqual(R.observe(bounded, (2, 0)), R.OUT_OF_DOMAIN)

    def test_the_expression_reader(self):
        env = {"x": 2, "flag": 1}
        for text, expected in [("x + 1 < 4 && flag", 1), ("flag ? x : 0", 2), ("!flag || x == 2", 1),
                               ("flag -> x == 1", 0), ("x - -3", 5), ("x == Two", 1), ("true", 1)]:
            self.assertEqual(R.value(R.parse(text), env, {"Two": 2}, {}), expected, text)
        with self.assertRaises(R.Invalid):
            R.parse("x +")


class Fixtures(unittest.TestCase):
    def test_every_fixture_gets_its_expected_verdict(self):
        self.assertEqual({f["expect"] for f in FIXTURES}, {"accepted", "rejected"})
        # every kind of claim has an accepted fixture and a planted false one
        for kind in ("region", "unrealizable", "teach-back", "interface", "question", "rule-meaning"):
            self.assertEqual({f["expect"] for f in FIXTURES if f["kind"] == kind}, {"accepted", "rejected"}, kind)
        for fixture in FIXTURES:
            problems = R.run_fixture(fixture)
            self.assertEqual(bool(problems), fixture["expect"] == "rejected", (fixture["name"], problems))

    def test_each_check_fails_when_its_comparison_is_stubbed(self):
        self.assertEqual(R.self_test(FIXTURES), [])
        # and the self-test itself notices a check that does not depend on its comparison
        planted = [dict(f, stub_sensitive=True) for f in FIXTURES if f["name"] == "region-queue-planted-wrong-count"]
        self.assertTrue(R.self_test(planted))


if __name__ == "__main__":
    unittest.main()
