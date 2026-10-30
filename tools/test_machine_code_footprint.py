"""Controls for the exact footprint pruning of machine-code search."""
from itertools import product
import json
from pathlib import Path
import random
import unittest

from machine_code import footprint, profile2, verify, verify2
from machine_code.session import _grammar
from machine_code.targets import encode_program

DATA = Path(__file__).resolve().parents[1] / "test-data/machine-code"
MASK = (1 << 64) - 1


def load(name):
    return json.loads((DATA / name).read_text())


def behaviour(problem, code, states):
    """Observed results on fixed entry states; a statically refused candidate has its own value."""
    rows = []
    try:
        for registers, flags in states:
            if problem["schema"] == profile2.SCHEMA:
                final, _ = verify2.execute(problem["target"], code, registers, flags)
                rows.append(tuple(final[n] for n in sorted(footprint.observed(problem))))
            else:
                arguments = ("rdi", "rsi") if problem["target"] == verify.X86 else ("a0", "a1")
                initial = {n: registers[n] for n in verify._SCRATCH[problem["target"]]}
                rows.append(verify.run_concrete(problem["target"], code,
                                                {"x": registers[arguments[0]], "y": registers[arguments[1]]}, initial))
    except ValueError:
        return "refused"
    return tuple(rows)


def entry_states(problem, count=24):
    generator = random.Random(20261030)
    if problem["schema"] == profile2.SCHEMA:
        names, flags = verify2.REGISTERS[problem["target"]], verify2.FLAGS
    else:
        names, flags = (*verify._SCRATCH[problem["target"]],
                        *(("rdi", "rsi") if problem["target"] == verify.X86 else ("a0", "a1"))), ()
    x86 = problem["target"] in (verify.X86, verify2.X86)
    special = (0, 1, MASK, 1 << 63)
    states = []
    for row in range(count):
        registers = {n: (special[row] if row < 4 else generator.getrandbits(64)) for n in names}
        states.append((registers, {f: generator.getrandbits(1) for f in flags} if x86 and flags else None))
    return states


class FootprintTests(unittest.TestCase):
    def test_footprints_follow_the_checker_semantics(self):
        prints = dict(zip(map(tuple, load("add128-x86.json")["grammar"]), footprint.footprints(load("add128-x86.json"))))
        adc = prints[("adc", "rdx", "rcx")]
        self.assertEqual(adc.reads, {"cf", "rcx", "rdx"})
        self.assertIn("cf", adc.writes)
        self.assertNotIn("cf", adc.overwrites, "the new carry depends on the old one")
        self.assertEqual(prints[("mov", "rax", "rdi")], footprint.Footprint(frozenset({"rdi"}), frozenset({"rax"}),
                                                                         frozenset({"rax"}),
                                                                         (("rax", frozenset({"rdi"})),)))
        self.assertEqual(prints[("nop",)], footprint.Footprint(frozenset(), frozenset(), frozenset()))
        umin = dict(zip(map(tuple, load("umin-x86.json")["grammar"]), footprint.footprints(load("umin-x86.json"))))
        self.assertEqual(umin[("cmova", "rax", "rsi")].reads, {"cf", "zf", "rax", "rsi"})
        self.assertEqual(umin[("cmova", "rax", "rsi")].overwrites, frozenset(), "a conditional move keeps the old value")
        rv = dict(zip(map(tuple, load("add128-rv64i.json")["grammar"]), footprint.footprints(load("add128-rv64i.json"))))
        self.assertEqual(rv[("sltu", "t0", "a0", "a2")].overwrites, {"t0"})

    def test_an_underivable_footprint_is_never_pruned_against(self):
        problem = load("add64-x86.json")
        pruner = footprint.Pruner(problem, _grammar(problem))
        pruner.prints = [None] * len(pruner.prints)
        pruner.order = [[False] * len(pruner.prints) for _ in pruner.prints]
        self.assertIsNone(pruner.judge([4, 4], 2))

    def test_skipping_keeps_every_behaviour_at_its_least_cost_and_bytes(self):
        """Concrete classes on fixed entry states: the cheapest kept member equals the cheapest member overall."""
        x, y = ["input", "x"], ["input", "y"]
        flags = {"schema": profile2.SCHEMA, "target": verify2.X86, "inputs": ["x", "y"],
                 "outputs": [["add", x, y], ["ult", x, y]], "max_instructions": 3, "objective": "bytes",
                 "grammar": [["mov", "rax", "rdi"], ["add", "rax", "rsi"], ["cmp", "rdi", "rsi"], ["setb", "rdx"]]}
        # Both results observed, so neither order of the two flag writers is dead; only the order decides the carry.
        both = {**flags, "grammar": [["add", "rdx", "rsi"], ["add", "rax", "rdi"], ["setb", "rax"]]}
        # Operand-swapped RV64I additions have equal final states, so one of each pair is a duplicate.
        swapped = {**load("add128-rv64i.json"),
                   "grammar": [["add", "a0", "a0", "a2"], ["add", "a0", "a2", "a0"], ["sltu", "t0", "a0", "a2"],
                               ["add", "a1", "a1", "a3"], ["add", "a1", "a3", "a1"], ["add", "a1", "a1", "t0"]]}
        skipped_total = 0
        for name in ("add128-x86.json", "add128-rv64i.json", "umin-x86.json", "add64-x86.json", "add64-rv64i.json",
                     flags, both, swapped):
            problem = load(name) if isinstance(name, str) else name
            name = name if isinstance(name, str) else "synthetic-" + "-".join(map(str, name["grammar"][1][:2]))
            problem["max_instructions"] = min(problem["max_instructions"], 3)
            grammar = _grammar(problem)
            terminal = encode_program(problem["target"], [])
            pruner = footprint.Pruner(problem, grammar)
            # Lost-input pruning drops only non-equivalent candidates, whole classes of them; it has its own test.
            pruner.required = frozenset()
            states = entry_states(problem)
            best_all, best_kept, skipped, total = {}, {}, {}, 0
            for length in range(problem["max_instructions"] + 1):
                kept = set(footprint.sequences(len(grammar), length, pruner.judge, skipped))
                for choices in product(range(len(grammar)), repeat=length):
                    total += 1
                    code = b"".join(grammar[i] for i in choices) + terminal
                    key = behaviour(problem, code, states)
                    rank = (len(choices), len(code), code.hex())
                    best_all[key] = min(best_all.get(key, rank), rank)
                    if choices in kept:
                        best_kept[key] = min(best_kept.get(key, rank), rank)
            with self.subTest(problem=name):
                self.assertEqual(best_kept, best_all)
            skipped_total += sum(skipped.values())
        self.assertGreater(skipped_total, 500)


    def test_operand_swapped_duplicates_are_found_by_proof(self):
        problem = {**load("add128-rv64i.json"),
                   "grammar": [["add", "a0", "a0", "a2"], ["add", "a0", "a2", "a0"], ["sub", "a1", "a1", "a3"],
                               ["sub", "a1", "a3", "a1"]]}
        _, dominated, _ = footprint.analyse(problem)
        encoded = _grammar(problem)
        self.assertEqual(len(dominated & {0, 1}), 1, "exactly one of the two additions is kept")
        self.assertEqual(min({0, 1} - dominated, key=lambda i: encoded[i]), min((0, 1), key=lambda i: encoded[i]))
        self.assertFalse(dominated & {2, 3}, "subtraction does not commute")

    def test_required_inputs_are_semantic(self):
        x, y = ["input", "x"], ["input", "y"]
        problem = {"schema": profile2.SCHEMA, "target": verify2.X86, "inputs": ["x", "y"],
                   "outputs": [["xor", ["xor", x, x], y]], "max_instructions": 2, "objective": "instructions",
                   "grammar": [["mov", "rdi", "rsi"], ["mov", "rax", "rdi"]]}
        self.assertEqual(footprint.analyse(problem)[2], {"rsi"}, "x occurs only syntactically")
        # The only kernel overwrites x's register first; a syntactic notion of "required" would prune it.
        grammar = _grammar(problem)
        pruner = footprint.Pruner(problem, grammar)
        self.assertIn((0, 1), set(footprint.sequences(len(grammar), 2, pruner.judge, {})))
        pruner.required = frozenset({"rdi", "rsi"})
        self.assertNotIn((0, 1), set(footprint.sequences(len(grammar), 2, pruner.judge, {})))

    def test_every_lost_candidate_is_refuted_by_the_checker(self):
        refuted = 0
        for name in ("add128-x86.json", "add128-rv64i.json", "umin-x86.json"):
            problem = load(name)
            grammar = _grammar(problem)
            terminal = encode_program(problem["target"], [])
            pruner = footprint.Pruner(problem, grammar)
            self.assertTrue(footprint.analyse(problem)[2], name)
            checker = verify2 if problem["schema"] == profile2.SCHEMA else verify
            # Through each problem's full length, so the carry chains' own kernels are covered.
            for length in range(1, problem["max_instructions"] + 1):
                for choices in product(range(len(grammar)), repeat=length):
                    if not any(pruner.lost(list(choices[:k])) for k in range(1, length + 1)):
                        continue
                    verdict = checker.check(problem, b"".join(grammar[i] for i in choices) + terminal)
                    self.assertIn(verdict["status"], ("counterexample", "refused"), (name, choices))
                    refuted += 1
        self.assertGreater(refuted, 50)


if __name__ == "__main__":
    unittest.main()
