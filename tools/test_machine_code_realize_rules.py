"""`realize --rules`: the enumerator's exact rules as selector constraints.

The constraints must keep exactly the candidates `footprint.Pruner.judge` keeps (up to the lost-input rule, which is
not encoded), the constrained records must agree with exhaustive enumeration, and replay must bind the mode.
Planted in-memory defects in the constraint generator must be caught by these same checks.
"""
from __future__ import annotations

from itertools import product
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))

from machine_code import footprint, realize, realize_rules, session  # noqa: E402
from machine_code.model import read_json  # noqa: E402
from test_machine_code_realize import DEADLINE_MS, QUERY_MS, least_cost, primary, problems  # noqa: E402

DATA = ROOT / "test-data/machine-code"
X86 = "x86_64-sysv-register/2"
X, Y = ["input", "x"], ["input", "y"]


def constructed(outputs, grammar, inputs=("x", "y"), objective="instructions", max_instructions=3):
    return {"schema": "zeno-fcis/machine-problem/2", "target": X86, "inputs": list(inputs), "outputs": outputs,
            "grammar": grammar, "objective": objective, "max_instructions": max_instructions}


# Palettes that exercise every rule: a dominated form (`sub rax,rax` and `xor rax,rax` leave the same state, so the
# byte-greater one is dominated), independent neighbours in both byte orders, a dead `nop`, flag writers nobody
# reads, a partial setcc write and a cmov.
JUDGE_CASES = {
    "umin-x86": read_json(DATA / "umin-x86.json"),
    "minmax-x86": read_json(DATA / "minmax-x86.json"),
    "dominated-and-dead": constructed(
        [["add", X, Y]],
        [["mov", "rax", "rdi"], ["add", "rax", "rsi"], ["xor", "rax", "rax"], ["sub", "rax", "rax"],
         ["imm", "rcx", "7"], ["cmp", "rdi", "rsi"], ["setb", "rax"], ["cmovb", "rax", "rsi"], ["nop"]]),
    "scratch-copies": constructed(
        [["xor", ["add", X, Y], Y]],
        [["mov", "rcx", "rsi"], ["mov", "rax", "rdi"], ["add", "rax", "rcx"], ["xor", "rax", "rcx"],
         ["mov", "rdx", "rcx"], ["sub", "rdx", "rdx"]]),
}


def disagreements(problem, length):
    """Selector sequences on which the rule constraints (no examples, no bound) and `Pruner.judge` differ.

    A sequence the judge skips only for a lost input is not a disagreement: that rule is not encoded."""
    import z3
    problem = session._validated(problem)
    pruner, _ = realize_rules.tables(problem)
    solver, selectors, _ = realize.query(z3, problem, length, None, [], rules=True)
    found = []
    for choice in product(range(len(problem["grammar"])), repeat=length):
        solver.push()
        for selector, index in zip(selectors, choice):
            solver.add(selector == index)
        admitted = solver.check() == z3.sat
        solver.pop()
        reasons = [pruner.judge(list(choice[:n]), length) for n in range(1, length + 1)]
        kept = all(r is None for r in reasons)
        lost_only = not kept and all(r in (None, "lost") for r in reasons)
        if admitted != kept and not (admitted and lost_only):
            found.append((choice, admitted, reasons))
    return found


class RulesEqualTheJudge(unittest.TestCase):
    def test_constraints_keep_exactly_the_judged_candidates(self):
        total = 0
        for name, problem in JUDGE_CASES.items():
            for length in range(0, 4):
                with self.subTest(problem=name, length=length):
                    self.assertEqual(disagreements(problem, length), [])
                    total += len(problem["grammar"]) ** length
        self.assertGreater(total, 1000)

    def test_tables_come_from_the_pruner(self):
        problem = session._validated(JUDGE_CASES["dominated-and-dead"])
        pruner, locations = realize_rules.tables(problem)
        fresh = footprint.Pruner(problem, session._grammar(problem))
        self.assertEqual(pruner.dominated, fresh.dominated)
        self.assertEqual(pruner.order, fresh.order)
        self.assertEqual(pruner.prints, fresh.prints)
        self.assertIn("rax", locations)
        self.assertEqual(len(pruner.dominated), 1, "one of sub rax,rax and xor rax,rax must dominate the other")

    def planted(self, edit):
        """Run the agreement check with `edit` applied to the generator; return the disagreement count."""
        original = realize_rules.constrain
        realize_rules.constrain = edit(original)
        try:
            return sum(len(disagreements(problem, length)) for problem in JUDGE_CASES.values() for length in (2, 3))
        finally:
            realize_rules.constrain = original

    def test_planted_symmetric_order_is_caught(self):
        def edit(original):
            def constrain(z, solver, selectors, slots, pruner, locations):
                size = len(pruner.prints)
                order = [[pruner.order[a][b] or pruner.order[b][a] for b in range(size)] for a in range(size)]
                pruner.order, saved = order, pruner.order
                try:
                    return original(z, solver, selectors, slots, pruner, locations)
                finally:
                    pruner.order = saved
            return constrain
        self.assertGreater(self.planted(edit), 0)

    def test_planted_unobserved_results_are_caught(self):
        def edit(original):
            def constrain(z, solver, selectors, slots, pruner, locations):
                saved = pruner.observed
                pruner.observed = frozenset()
                try:
                    return original(z, solver, selectors, slots, pruner, locations)
                finally:
                    pruner.observed = saved
            return constrain
        self.assertGreater(self.planted(edit), 0)

    def test_planted_extra_dominated_form_is_caught(self):
        def edit(original):
            def constrain(z, solver, selectors, slots, pruner, locations):
                saved = pruner.dominated
                pruner.dominated = saved | {0}
                try:
                    return original(z, solver, selectors, slots, pruner, locations)
                finally:
                    pruner.dominated = saved
            return constrain
        self.assertGreater(self.planted(edit), 0)

    def test_planted_dead_rule_ignoring_reads_is_caught(self):
        def edit(original):
            def constrain(z, solver, selectors, slots, pruner, locations):
                prints = [footprint.Footprint(frozenset(), f.writes, f.overwrites, f.sources) for f in pruner.prints]
                pruner.prints, saved = prints, pruner.prints
                try:
                    return original(z, solver, selectors, slots, pruner, locations)
                finally:
                    pruner.prints = saved
            return constrain
        self.assertGreater(self.planted(edit), 0)


class RulesCrossCheck(unittest.TestCase):
    def test_every_small_problem_agrees_with_exhaustive_enumeration(self):
        cases = problems()
        self.assertGreaterEqual(len(cases), 15)
        for name, problem in cases:
            with self.subTest(problem=name):
                least = least_cost(name, problem)
                if least is None:
                    record = realize.realize(problem, None, QUERY_MS, deadline_ms=DEADLINE_MS, rules=True)
                    self.assertEqual(record["status"], "unrealizable", record)
                    self.assertEqual(record["rules"], footprint.SCHEME)
                    continue
                below = realize.realize(problem, least, QUERY_MS, deadline_ms=DEADLINE_MS, rules=True)
                self.assertEqual(below["status"], "unrealizable", below)
                self.assertTrue(all(e["status"] == "unrealizable" for e in below["lengths"]), below)
                for entry in below["lengths"]:
                    self.assertIn("rules keep", entry["statement"])
                at = realize.realize(problem, least + 1, QUERY_MS, deadline_ms=DEADLINE_MS, rules=True)
                self.assertEqual(at["status"], "realizable", at)
                found = at["lengths"][-1]
                self.assertEqual(found["verification"]["status"], "equivalent")
                self.assertEqual(primary(problem, found["verification"]["cost"]), least)

    def test_partitioned_mode_takes_the_rules_too(self):
        problem = read_json(DATA / "umin-x86.json")
        least = least_cost("umin-x86", problem)
        below = realize.realize(problem, least, QUERY_MS, deadline_ms=DEADLINE_MS, groups=2, workers=2, rules=True)
        self.assertEqual(below["status"], "unrealizable", below)
        self.assertEqual(below["rules"], footprint.SCHEME)
        self.assertEqual(realize.replay(problem, below, QUERY_MS)["status"], "replayed")

    def test_a_candidate_the_judge_prunes_is_never_returned(self):
        """The CEGIS loop rechecks every solver model against the judge; a symmetric order table makes the
        constraints and the judge disagree, which must surface as a refusal, never as a record."""
        problem = read_json(DATA / "umin-x86.json")
        original = realize_rules.kept
        realize_rules.kept = lambda pruner, choice: False
        try:
            with self.assertRaises(ValueError):
                realize.realize(problem, 12, QUERY_MS, deadline_ms=DEADLINE_MS, rules=True)
        finally:
            realize_rules.kept = original


class RulesReplay(unittest.TestCase):
    def test_replay_binds_the_rules_mode(self):
        problem = read_json(DATA / "umin-x86.json")
        record = realize.realize(problem, 11, QUERY_MS, deadline_ms=DEADLINE_MS, rules=True)
        self.assertEqual(record["status"], "unrealizable")
        self.assertEqual(realize.replay(problem, record, QUERY_MS)["status"], "replayed")
        plain = dict(record)
        del plain["rules"]
        result = realize.replay(problem, plain, QUERY_MS)
        self.assertEqual(result["status"], "failed", result)
        relabelled = dict(realize.realize(problem, 11, QUERY_MS, deadline_ms=DEADLINE_MS), rules=footprint.SCHEME)
        result = realize.replay(problem, relabelled, QUERY_MS)
        self.assertEqual(result["status"], "failed", result)
        wrong_scheme = dict(record, rules="exact-footprint/0")
        self.assertEqual(realize.replay(problem, wrong_scheme, QUERY_MS)["status"], "failed")


if __name__ == "__main__":
    unittest.main()
