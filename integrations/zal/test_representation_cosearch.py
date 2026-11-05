"""Executable development, evaluation, adversarial, and negative controls."""
from dataclasses import replace
from itertools import product
import json
from pathlib import Path
import unittest

import discovery_loop as d
import representation_cosearch as r

CASES_PATH = Path(__file__).with_name("test-data") / "representation_cosearch_cases.json"
CASES = json.loads(CASES_PATH.read_text())


def make_task(spec):
    if spec["domain"] == "boolean2":
        rows = tuple(product((0, 1), repeat=2))
    elif spec["domain"] == "boolean3":
        rows = tuple(product((0, 1), repeat=3))
    elif spec["domain"] == "grid2":
        rows = tuple(product(range(spec["low"], spec["high"] + 1), repeat=2))
    else:
        raise ValueError("unknown task domain")
    def predicate(row):
        if spec["kind"] == "xor":
            return row[0] != row[1]
        if spec["kind"] == "xnor":
            return row[0] == row[1]
        if spec["kind"] == "radial":
            return row[0] ** 2 + row[1] ** 2 >= spec["cutoff"]
        if spec["kind"] == "oblique":
            return (row[0] + row[1]) ** 2 >= spec["cutoff"]
        if spec["kind"] == "parity":
            return bool(sum(row) % 2)
        raise ValueError("unknown predicate")
    return r.Task(spec["id"], rows, tuple(bool(predicate(row)) for row in rows))


class RepresentationSearchTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.development = {entry["id"]: r.search(make_task(entry)) for entry in CASES["development"]}

    def test_frozen_benchmark_roles_and_search_completeness(self):
        self.assertEqual(CASES["schema"], "zeno-fcis/representation-cosearch-cases/1")
        self.assertEqual(len(CASES["development"]), 3)
        self.assertEqual(len(CASES["evaluation"]), 3)
        self.assertEqual(len(CASES["negative"]), 1)
        self.assertTrue(set(e["id"] for e in CASES["development"]).isdisjoint(e["id"] for e in CASES["evaluation"]))
        for entry in CASES["development"]:
            report = self.development[entry["id"]]
            task = make_task(entry)
            self.assertEqual(report.task_identity, task.identity())
            self.assertEqual(report.baseline.status, "infeasible-under-bounded-weights")
            self.assertTrue(report.complete)
            self.assertEqual(report.generated, report.enumerated)
            self.assertTrue(r.replay(task, report.candidate))
            self.assertGreaterEqual(len(report.candidate_trace.counterexample_indices), 1)

    def test_discovery_composes_arithmetic_rules_not_preset_feature_list(self):
        feature = {name: report.candidate.features[-1].pretty() for name, report in self.development.items()}
        self.assertEqual(feature["xor"], "(x0*x1)")
        self.assertEqual(feature["radial"], "((x0*x0)+(x1*x1))")
        self.assertEqual(feature["oblique"], "((x0+x1)*(x0+x1))")
        self.assertEqual(self.development["oblique"].candidate.features[-1].size, 7)
        self.assertEqual(self.development["radial"].candidate.features[-1].size, 7)

    def test_reuse_frozen_representation_on_three_separate_evaluation_cases(self):
        # No feature enumeration or feature modification occurs on these cases.
        for spec in CASES["evaluation"]:
            with self.subTest(case=spec["id"]):
                task = make_task(spec)
                source = self.development[spec["source"]]
                feature_before = source.candidate.features[-1]
                trace, receipt = r.reuse_feature(source, task)
                self.assertEqual(trace.status, "finite-equivalent")
                self.assertIsNotNone(receipt)
                self.assertEqual(receipt.features[-1], feature_before)
                self.assertTrue(r.replay(task, receipt))
                self.assertEqual(receipt.task_digest, task.identity())
                self.assertTrue(all((receipt.separator.bias + sum(
                    weight * expr.evaluate(row)
                    for weight, expr in zip(receipt.separator.weights, receipt.features)) > 0) == expected
                    for row, expected in zip(task.rows, task.labels)))
                self.assertIn(len(task.rows), (4, 49))

    def test_exhaustive_boolean_two_input_classifier_portfolio(self):
        rows = tuple(product((0, 1), repeat=2))
        examined = 0
        non_linear = []
        for bits in range(1, 15):
            labels = tuple(bool((bits >> (a + 2 * b)) & 1) for a, b in rows)
            task = r.Task(f"truth-{bits}", rows, labels)
            report = r.search(task)
            if report.baseline.separator is None:
                non_linear.append(bits)
                self.assertIsNotNone(report.candidate)
                self.assertTrue(r.replay(task, report.candidate))
            else:
                self.assertTrue(r.replay(task, r.Receipt(task.identity(),
                    tuple(r.Expr('var', (), i) for i in range(2)), report.baseline.separator)))
            self.assertIsNotNone(report.bdd_nodes)
            self.assertEqual(report.bdd_orders, 2)
            examined += 1
        self.assertEqual(examined, 14)
        self.assertEqual(sorted(non_linear), [6, 9])

    def test_cross_representations_projection_then_feature_lift_then_bdd(self):
        # Three-variable relation with hidden h == a AND b. Existential
        # projection recovers XOR(a,b), then separate searches act on it.
        relation = d.Table.from_predicate(3, lambda row:
            ((row >> 2) & 1) == (((row >> 0) & 1) & ((row >> 1) & 1))
            and bool(((row >> 0) & 1) ^ ((row >> 1) & 1)))
        projected = d.project(relation, 2, d.discover("exists"))
        self.assertTrue(d.replay_projection(relation, projected))
        task = r.Task.from_boolean_table("hidden-product-relation", projected.output)
        report = r.search(task)
        self.assertEqual(task.labels, (False, True, True, False))
        self.assertIsNone(report.baseline.separator)
        self.assertEqual(report.candidate.features[-1].pretty(), "(x0*x1)")
        self.assertTrue(r.replay(task, report.candidate))
        self.assertEqual(report.bdd_nodes, 3)
        self.assertEqual(report.bdd_orders, 2)

    def test_negative_out_of_reach_parity_still_has_exact_bdd(self):
        task = make_task(CASES["negative"][0])
        report = r.search(task)
        self.assertIsNone(report.baseline.separator)
        self.assertIsNone(report.candidate) # one added feature within <=7 AST nodes
        self.assertTrue(report.complete)
        self.assertIsNotNone(report.bdd_nodes)
        self.assertEqual(report.bdd_orders, 6)

    def test_mutation_refutations_and_source_binding(self):
        spec = next(s for s in CASES["development"] if s["id"] == "radial")
        task = make_task(spec)
        receipt = self.development["radial"].candidate
        self.assertTrue(r.replay(task, receipt))
        mutated_task = r.Task("radial-other", task.rows, task.labels)
        self.assertFalse(r.replay(mutated_task, receipt))
        self.assertFalse(r.replay(task, replace(receipt, task_digest="bad")))
        broken = replace(receipt, separator=replace(receipt.separator, bias=10000))
        self.assertFalse(r.replay(task, broken))
        changed_feature = replace(receipt, features=receipt.features[:-1] + (r.Expr("var", (), 0),))
        self.assertFalse(r.replay(task, changed_feature))
        self.assertFalse(r.replay(task, replace(receipt, separator=replace(receipt.separator,
                                                                        weights=(1, 1, 99)))))

    def test_invalid_arithmetic_rewrite_and_oversized_receipt_refused(self):
        # x*x == x is valid for Boolean inputs, NOT for the signed integer grid.
        task = make_task(CASES["development"][1])
        x = r.Expr("var", (), 0)
        square = r.Expr("*", (x, x))
        self.assertTrue(any(square.evaluate(row) != x.evaluate(row) for row in task.rows))
        original = self.development["radial"].candidate
        invalid = replace(original, features=original.features[:-1] + (r.Expr("+", (x,)),))
        self.assertFalse(r.replay(task, invalid))
        deep = x
        for _ in range(4):
            deep = r.Expr("*", (deep, deep))
        self.assertFalse(r.valid_expr(deep, task.arity))
        self.assertFalse(r.replay(task, replace(original, features=original.features[:-1] + (deep,))))
        with self.assertRaises(ValueError):
            r.cegis(task, (x, deep))

    def test_enumeration_truncation_not_reported_as_complete(self):
        task = make_task(CASES["development"][1])
        limited = r.search(task, max_candidates=1)
        self.assertFalse(limited.complete)
        self.assertEqual(limited.generated, 1)
        self.assertEqual(limited.enumerated, 1)
        limited = r.search(task, max_evaluations=2)
        self.assertFalse(limited.complete)
        self.assertEqual(limited.enumerated, 2)

    def test_strict_domain_refusals_and_transfer_arity(self):
        with self.assertRaises(ValueError):
            r.Task("duplicate", ((0,0), (0,0)), (True, False))
        with self.assertRaises(ValueError):
            r.Task("invalid", ((0,0), (1,1)), (1, 0))
        with self.assertRaises(ValueError):
            r.Task("oversize", ((0,0,0,0), (1,1,1,1)), (True, False))
        with self.assertRaises(ValueError):
            r.search(make_task(CASES["development"][0]), max_evaluations=-1)
        with self.assertRaises(ValueError):
            r.reuse_feature(self.development["xor"], make_task(CASES["negative"][0]))


if __name__ == "__main__":
    unittest.main()
