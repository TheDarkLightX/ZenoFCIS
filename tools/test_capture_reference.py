"""Focused pure Python reference tests; never execute the reference or CLI."""
import unittest
import capture_reference as R


class CaptureReferenceTests(unittest.TestCase):
    def setUp(self):
        self.spec = {"inputs": [{"kind": "Int", "min": "0", "max": "3"}], "outputs": [{"kind": "Bool"}]}
        self.rows = [[1], [1], [0], [0]]
        self.wrong = {**self.spec, "nodes": [["Bool", False]], "roots": [0]}
        self.right = {**self.spec, "nodes": [["Input", 0], ["Int", "2"], ["Lt", 0, 1]], "roots": [2]}

    def test_examples_only_precheck_repeat_and_distant_failure(self):
        r = R.policies(self.spec, self.rows, self.wrong, [2])
        self.assertIsNone(r["precheck_failure"])
        self.assertEqual(r["bad"], [0, 1])
        self.assertEqual(r["chosen"], 0)
        self.assertEqual(R.policies(self.spec, self.rows, self.wrong, [0])["precheck_failure"], 0)
        self.assertEqual(R.policies(self.spec, self.rows, self.wrong, [1, 0])["precheck_failure"], 1)
        self.assertFalse(R.replay(self.spec, self.rows, self.wrong))
        self.assertTrue(R.replay(self.spec, self.rows, self.right))
        rows = [[1]] * 4
        self.assertEqual(R.policies(self.spec, rows, self.wrong, [0], anchor=0)["chosen"], 3)

    def test_nullary_product_and_last_input_fastest(self):
        self.assertEqual(R.points({"inputs": []}), ((),))
        self.assertEqual(R.points({"inputs": [{"kind": "Bool"}] * 2}), ((0, 0), (0, 1), (1, 0), (1, 1)))
        with self.assertRaises(ValueError):
            R.points(self.spec, maximum=3)

    def test_unreachable_and_eager_trapping_prefixes(self):
        p = {**self.spec, "nodes": [["Bool", True], ["Bool", False], ["Select", 0, 1, 0]], "roots": [2]}
        outcomes = R.policies(self.spec, self.rows, p, [0])["coverage"]
        self.assertFalse(outcomes[0]["reachable"])
        self.assertTrue(outcomes[1]["covered"])
        p = {**self.spec, "nodes": [["Int", str(2**63 - 1)], ["Int", "1"], ["Add", 0, 1],
            ["Bool", True], ["Bool", False], ["Select", 3, 3, 4]], "roots": [5]}
        self.assertTrue(all(not o["reachable"] for o in R.policies(self.spec, self.rows, p, [0])["coverage"]))

    def test_authorization_rejection_keeps_explicit_state(self):
        spec = {"inputs": [{"kind": "Int", "min": "0", "max": "2"}, {"kind": "Bool"}],
                "outputs": [{"kind": "Bool"}, {"kind": "Int", "min": "0", "max": "2"}]}
        rows = [[int(a == 1 and x < 2), x + int(a == 1 and x < 2)] for x, a in R.points(spec)]
        nodes = [["Input", 0], ["Input", 1], ["Int", "2"], ["Lt", 0, 2], ["And", 1, 3],
                 ["Int", "1"], ["Add", 0, 5], ["Select", 4, 6, 0]]
        self.assertFalse(R.replay(spec, rows, {**spec, "nodes": nodes, "roots": [4, 6]}))
        self.assertTrue(R.replay(spec, rows, {**spec, "nodes": nodes, "roots": [4, 7]}))

    def test_malformed_reference_labels_never_count_as_matching_failures(self):
        for rows in ([], [[True]] * 4, [[2]] * 4, [[0.0]] * 4, [{"error": "Arithmetic"}] * 4):
            with self.assertRaises(ValueError):
                R.table_rows(self.spec, rows)

    def test_interval_absolute_and_guarded_semantic_cases(self):
        integer = {"kind": "Int", "min": "0", "max": "7"}
        for lo, hi in ((0, 0), (0, 7), (2, 5), (7, 7)):
            spec = {"inputs": [integer], "outputs": [{"kind": "Bool"}]}
            nodes = [["Input", 0], ["Int", str(lo)], ["Int", str(hi)], ["Lt", 0, 1],
                     ["Not", 3], ["Lt", 2, 0], ["Not", 5], ["And", 4, 6]]
            rows = [[int(lo <= p[0] <= hi)] for p in R.points(spec)]
            self.assertTrue(R.replay(spec, rows, {**spec, "nodes": nodes, "roots": [7]}))
        for center in (0, 3, 7):
            spec = {"inputs": [integer], "outputs": [integer]}
            nodes = [["Input", 0], ["Int", str(center)], ["Lt", 0, 1], ["Sub", 1, 0],
                     ["Sub", 0, 1], ["Select", 2, 3, 4]]
            rows = [[abs(p[0] - center)] for p in R.points(spec)]
            self.assertTrue(R.replay(spec, rows, {**spec, "nodes": nodes, "roots": [5]}))


if __name__ == "__main__":
    unittest.main()
