"""The search benchmark's counters, budget attribution, comparison and one small real run."""

from contextlib import redirect_stderr
import hashlib
import io
import json
from pathlib import Path
import tempfile
import unittest

from machine_qualification import search_bench as bench

ROOT = Path(__file__).resolve().parents[1]
BUDGETS = dict(bench.BUDGETS)


def report(**problems):
    return {"schema": bench.SCHEMA, "commit": "c", "problems": problems}


def row(cost, status="checked-candidate", seconds=0.5, objective="bytes", **counters):
    return {"cost": cost, "status": status, "wall_median_seconds": seconds, "objective": objective,
            "counters": {"solver_checks": 1, "proposed": 10, "concrete_refutations": 2, "objective_pruned": 3,
                         **counters}}


class Counters(unittest.TestCase):
    def test_every_numeric_result_field_is_a_counter_except_the_timer_and_incumbent(self):
        result = {"status": "checked-candidate", "proposed": 9, "solver_checks": 2, "dead_pruned": 4,
                  "order_pruned": 1, "split": {"left": 1}, "elapsed_seconds": 0.2, "incumbent": 3,
                  "global_optimality": False, "checker_verdicts": [{"status": "equivalent"},
                                                                   {"status": "counterexample"},
                                                                   {"status": "counterexample"}]}
        self.assertEqual(bench.counters(result), {"proposed": 9, "solver_checks": 2, "dead_pruned": 4,
                                                  "order_pruned": 1, "verdicts_counterexample": 2,
                                                  "verdicts_equivalent": 1})

    def test_the_stopping_budget_is_named(self):
        cases = [({"stop_reason": "grammar-exhausted"}, 1, None),
                 ({"stop_reason": "budget-exhausted", "proposed": BUDGETS["max_candidates"]}, 1, "candidates"),
                 ({"stop_reason": "budget-exhausted", "proposed": 5, "solver_checks": BUDGETS["max_checks"]}, 1,
                  "checks"),
                 ({"stop_reason": "budget-exhausted", "proposed": 5, "solver_checks": 1}, 61, "deadline"),
                 ({"stop_reason": "budget-exhausted", "proposed": 5, "solver_checks": 1}, 1, "other")]
        for result, seconds, expected in cases:
            with self.subTest(result=result):
                self.assertEqual(bench.budget_hit(result, seconds, BUDGETS), expected)


class Comparison(unittest.TestCase):
    def test_identical_costs_pass_and_new_counters_become_columns(self):
        before = report(a=row({"bytes": 5, "instructions": 1}))
        after = report(a=row({"bytes": 5, "instructions": 1}, seconds=0.2, dead_pruned=7))
        text, code = bench.compare(before, after)
        self.assertEqual(code, 0, text)
        self.assertIn("dead_pruned", text.splitlines()[0])
        self.assertIn("identical", text)

    def test_a_worse_or_lost_cost_or_missing_problem_fails(self):
        base = row({"bytes": 5, "instructions": 1})
        for after in (report(a=row({"bytes": 8, "instructions": 2})),
                      report(a=row(None, status="no-checked-candidate")), report()):
            with self.subTest(after=after):
                text, code = bench.compare(report(a=base), after)
                self.assertEqual(code, 1, text)

    def test_a_problem_added_after_the_baseline_is_listed_but_does_not_fail(self):
        text, code = bench.compare(report(), report(b=row({"bytes": 5, "instructions": 1})))
        self.assertEqual(code, 0, text)
        self.assertIn("b | new", text)

    def test_a_cost_found_only_after_or_lower_after_is_an_improvement(self):
        for old, new in ((None, {"bytes": 31, "instructions": 7}),
                         ({"bytes": 8, "instructions": 2}, {"bytes": 5, "instructions": 1})):
            with self.subTest(old=old):
                text, code = bench.compare(report(a=row(old)), report(a=row(new)))
                self.assertEqual(code, 0, text)
                self.assertIn("improved", text)


    def test_costs_are_ranked_by_the_recorded_objective(self):
        # Constant one under the instruction objective: imm rax,1 (11 bytes, 1 instruction) against
        # xor rax,rax; or rax,1 (8 bytes, 2 instructions). Both are checker-equivalent.
        from machine_code import verify2
        from machine_code.targets import encode_program
        one, two = [["imm", "rax", "1"]], [["xor", "rax", "rax"], ["or", "rax", 1]]
        problem = {"schema": "zeno-fcis/machine-problem/2", "target": verify2.X86, "inputs": [],
                   "outputs": [["const", "1"]], "grammar": one + two, "max_instructions": 2,
                   "objective": "instructions"}
        costs = []
        for body in (one, two):
            result = verify2.check(problem, encode_program(problem["target"], body))
            self.assertEqual(result["status"], "equivalent")
            costs.append(result["cost"])
        self.assertEqual(costs, [{"bytes": 11, "instructions": 1}, {"bytes": 8, "instructions": 2}])
        for objective, expected, exit_code in (("instructions", "worse", 1), ("bytes", "improved", 0)):
            with self.subTest(objective=objective):
                text, code = bench.compare(report(a=row(costs[0], objective=objective)),
                                           report(a=row(costs[1], objective=objective)))
                self.assertEqual((code, text.splitlines()[1].split(" | ")[6].split()[0]), (exit_code, expected), text)

    def test_unknown_or_changed_objectives_are_not_ranked(self):
        old, new = {"bytes": 11, "instructions": 1}, {"bytes": 8, "instructions": 2}
        text, code = bench.compare(report(a=row(old, objective=None)), report(a=row(new, objective=None)))
        self.assertEqual(code, 1, text)
        self.assertIn("unranked", text)
        # One report without the objective (an older baseline) is ranked by the other's.
        text, code = bench.compare(report(a=row(old, objective=None)), report(a=row(new, objective="bytes")))
        self.assertEqual((code, "improved" in text), (0, True), text)
        text, code = bench.compare(report(a=row(old)), report(a=row(old, objective="instructions")))
        self.assertEqual((code, "objective changed" in text), (1, True), text)


class SmallRun(unittest.TestCase):
    def test_a_real_search_is_measured_repeatably_without_changing_the_repository(self):
        names = {"add64-rv64i.json", "kernels/add64-rv64i.json", "kernels/signed-saturating-add-x86.json",
                 "memory-kernels/swap-words-x86.json"}
        self.assertLessEqual(names, set(bench.problems()))
        data = ROOT / "test-data/machine-code"
        self.assertEqual(len(bench.problems()), sum(len(list((data / folder).glob("*.json")))
                                                    for folder in (".", "kernels", "memory-kernels")))
        before = {p: hashlib.sha256(p.read_bytes()).hexdigest()
                  for p in sorted((ROOT / "test-data/machine-code").rglob("*.json"))}
        with redirect_stderr(io.StringIO()):
            measured = bench.run(["add64-rv64i.json"], BUDGETS, 2)
        result = measured["problems"]["add64-rv64i.json"]
        self.assertEqual((result["status"], result["cost"], result["deterministic"], result["objective"]),
                         ("checked-candidate", {"bytes": 8, "instructions": 1}, True, "bytes"))
        # With the grammar exhausted, every sequence is either proposed or counted as skipped by pruning or by
        # the cost bound. Objective prunes are counted among the proposed sequences.
        counted = result["counters"]
        skipped = sum(value for key, value in counted.items()
                      if key.endswith("_skipped") or (key.endswith("_pruned") and key != "objective_pruned"))
        self.assertEqual(counted["proposed"] + skipped, result["space"])
        self.assertEqual(len(result["wall_seconds"]), 2)
        self.assertGreaterEqual(result["counters"]["solver_checks"], 1)
        after = {p: hashlib.sha256(p.read_bytes()).hexdigest()
                 for p in sorted((ROOT / "test-data/machine-code").rglob("*.json"))}
        self.assertEqual(after, before)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "report.json"
            path.write_text(json.dumps(measured))
            self.assertEqual(bench.compare(measured, json.loads(path.read_text()))[1], 0)


if __name__ == "__main__":
    unittest.main()
