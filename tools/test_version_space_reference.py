#!/usr/bin/env python3
"""Focused reference semantics; no CLI, controls campaign or study is run."""
import unittest
import version_space_reference as O


def spec(sketch, inputs=None, outputs=None, requirements=()):
    return {"schema": "zeno-fcis/loop-version-space/1", "inputs": inputs or [{"name": "x", "domain": {"kind": "Bool"}}],
            "outputs": outputs or [{"kind": "Bool"}], "sketch": sketch, "requirements": list(requirements)}


class Reference(unittest.TestCase):
    def test_duplicate_spellings_count_meanings(self):
        self.assertEqual(O.expected(spec(["?{x | x && true | false | true}"]))["count"], 3)

    def test_multi_output_full_tuples(self):
        result = O.expected(spec(["?c", "?c"], outputs=[{"kind": "Bool"}]*2))
        self.assertEqual(result["count"], 4)
        self.assertEqual([a["remaining"] for a in result["question"]["answers"]], [1]*4)

    def test_minimax_threshold(self):
        result = O.expected(spec(["x < ?c[0..4]"], inputs=[{"name": "x", "domain": {"kind": "Int", "min": 0, "max": 3}}]))
        self.assertEqual(result["count"], 5)
        self.assertEqual(result["question"]["ordinal"], 1)
        self.assertEqual(result["question"]["worst_remaining"], 3)

    def test_minimum_conflict_lexical_tie_and_empty_grammar(self):
        s = spec(["?c"], requirements=[{"id": i, "input": [x], "output": [y]} for i, x, y in
                                      (("d", 0, 1), ("c", 0, 0), ("b", 1, 1), ("a", 1, 0))])
        self.assertEqual(O.expected(s)["conflict"], ["a", "b"])
        s = spec(["?{2 | 3}"], outputs=[{"kind": "Int", "min": 0, "max": 1}])
        self.assertEqual(O.expected(s), {"count": 0, "conflict": []})

    def test_laws_examples_eager_overflow_and_outside_grammar(self):
        s = spec(["?{false | true ? false : 170141183460469231731687303715884105727 + 1}"])
        self.assertEqual(O.expected(s)["count"], 1)
        # Evaluation is checked at i128: the former i64 bound is an ordinary value.
        s = spec(["?{false | true ? true : 9223372036854775807 + 1 > 0}"])
        self.assertEqual(O.expected(s)["count"], 2)
        s = spec(["?{false | true ? true : 170141183460469231731687303715884105727 + 1 > 0}"])
        self.assertEqual(O.expected(s)["count"], 1)
        s = spec(["?c"], requirements=[{"id": "zero", "law": "out0 == false"}, {"id": "one", "input": [0], "output": [1]}])
        self.assertEqual(O.expected(s)["count"], 0)
        s = spec(["?c[0..1]"], outputs=[{"kind": "Int", "min": 0, "max": 2}], requirements=[{"id": "outside", "input": [0], "output": [2]}])
        self.assertEqual(O.expected(s)["conflict"], ["outside"])


if __name__ == "__main__":
    unittest.main()
