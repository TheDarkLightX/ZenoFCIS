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
                                                                         frozenset({"rax"})))
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
        skipped_total = 0
        for name in ("add128-x86.json", "add128-rv64i.json", "umin-x86.json", "add64-x86.json", "add64-rv64i.json",
                     flags, both):
            problem = load(name) if isinstance(name, str) else name
            name = name if isinstance(name, str) else "flag-order-" + name["grammar"][0][1]
            problem["max_instructions"] = min(problem["max_instructions"], 3)
            grammar = _grammar(problem)
            terminal = encode_program(problem["target"], [])
            pruner = footprint.Pruner(problem, grammar)
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


if __name__ == "__main__":
    unittest.main()
