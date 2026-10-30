"""Solver unrealizability, cross-checked against exhaustive exact enumeration.

For every problem whose candidate space is small enough, the exact enumerator (`session.search`, footprint pruning
on) must exhaust its grammar. When it finds a least cost c, `realize` below c must prove every asked length
unrealizable, and `realize` below c+1 must find checked bytes of cost exactly c. When it finds nothing, `realize`
without a bound must prove every length unrealizable. Any disagreement, in either direction, is a defect.
"""

from __future__ import annotations

import contextlib
import io
import json
from pathlib import Path
import shutil
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))

from machine_code import realize, run, session  # noqa: E402
from machine_code.model import read_json  # noqa: E402

DATA = ROOT / "test-data/machine-code"
SMALL = 20_000  # candidate sequences, all lengths
# Generous per-query and per-run budgets, so that a loaded host turns no answer into a timeout. An unloaded host
# answers these queries in well under a second; the budgets only bound how long a slow one may take.
QUERY_MS = 20_000
DEADLINE_MS = 600_000
BUDGET = ["--solver-ms", str(QUERY_MS), "--deadline-ms", str(DEADLINE_MS)]
P1 = "zeno-fcis/machine-problem/1"

# The palette's last form is the only single-instruction sum: a selector range that drops one form must show.
LAST_FORM_NEEDED = {"schema": P1, "target": "x86_64-sysv-register/1", "inputs": ["x", "y"],
                    "expression": ["add", ["input", "x"], ["input", "y"]],
                    "grammar": [["mov", "rax", "rdi"], ["add", "rax", "rsi"], ["xor", "rax", "rsi"],
                                ["lea", "rax", "rdi", "rsi", 1]],
                    "max_instructions": 2, "objective": "bytes"}
# No sequence of these forms computes x*y.
NO_PROGRAM = {"schema": P1, "target": "x86_64-sysv-register/1", "inputs": ["x", "y"],
              "expression": ["mul", ["input", "x"], ["input", "y"]],
              "grammar": [["mov", "rax", "rdi"], ["add", "rax", "rsi"], ["xor", "rax", "rsi"]],
              "max_instructions": 2, "objective": "instructions"}


# A profile-3 palette in which some sequences address a region through a base register an earlier form wrote:
# the checker refuses those, and the query must exclude exactly them.
BASE_WRITES = {"schema": "zeno-fcis/machine-problem/3", "target": "x86_64-sysv-memory/3", "inputs": ["y"],
               "regions": [{"name": "a", "base": "rdi", "words": 2, "access": "read-write"}],
               "outputs": [["add", ["initial", "a", 0], ["initial", "a", 1]]],
               "memory": {"a": [["initial", "a", 0], ["initial", "a", 1]]},
               "grammar": [["load", "rax", "rdi", 8], ["load", "rdi", "rdi", 0], ["add", "rax", "rdi"],
                           ["store", "rdi", 0, "rax"]],
               "max_instructions": 3, "objective": "bytes"}


# A profile-2 palette in which IMUL leaves ZF and SF undefined and CMOVE reads ZF: the checker refuses that order.
FLAG_READS = {"schema": "zeno-fcis/machine-problem/2", "target": "x86_64-sysv-register/2", "inputs": ["x", "y"],
              "outputs": [["mul", ["input", "x"], ["input", "y"]]],
              "grammar": [["imul", "rax", "rsi"], ["cmove", "rax", "rdi"], ["cmp", "rdi", "rsi"], ["mov", "rax", "rdi"]],
              "max_instructions": 3, "objective": "bytes"}


# One lea form, at most one instruction: below 1 instruction only length 0 is asked; a one-instruction sum exists.
LEA_ONLY = {"schema": P1, "target": "x86_64-sysv-register/1", "inputs": ["x", "y"],
            "expression": ["add", ["input", "x"], ["input", "y"]],
            "grammar": [["lea", "rax", "rdi", "rsi", 1]], "max_instructions": 1, "objective": "instructions"}
# Two constant forms that never compute x+y; with two groups per slot, lengths 0 to 16 have 2**17 - 1 subspaces.
CONSTANTS_ONLY = {"schema": "zeno-fcis/machine-problem/2", "target": "x86_64-sysv-register/2", "inputs": ["x", "y"],
                  "outputs": [["add", ["input", "x"], ["input", "y"]]],
                  "grammar": [["imm", "rax", "0"], ["imm", "rax", "1"]], "max_instructions": 16,
                  "objective": "instructions"}


# Boundary cases: an empty body is already equivalent (least cost 0), and an empty palette with positive lengths.
ZERO_COST = {"schema": "zeno-fcis/machine-problem/2", "target": "rv64i-register/2", "inputs": ["x"],
             "outputs": [["input", "x"]], "grammar": [], "max_instructions": 0, "objective": "instructions"}
EMPTY_PALETTE = {"schema": "zeno-fcis/machine-problem/2", "target": "x86_64-sysv-register/2", "inputs": [],
                 "outputs": [["const", "0"]], "grammar": [], "max_instructions": 1, "objective": "bytes"}


def problems():
    found = [(path.stem, read_json(path)) for path in sorted(DATA.glob("*.json"))]
    found += [("kernel " + path.stem, read_json(path)["problem"]) for path in sorted((DATA / "kernels").glob("*.json"))]
    found += [("memory " + path.stem, (lambda d: d.get("problem", d))(read_json(path)))
              for path in sorted((DATA / "memory-kernels").glob("*.json"))]
    found += [("last form needed", LAST_FORM_NEEDED), ("no program", NO_PROGRAM), ("base writes", BASE_WRITES),
              ("flag reads", FLAG_READS), ("zero cost", ZERO_COST), ("empty palette", EMPTY_PALETTE)]
    return [(name, p) for name, p in found
            if sum(len(p["grammar"]) ** k for k in range(p["max_instructions"] + 1)) <= SMALL]


def primary(problem, cost):
    return cost["bytes"] if problem["objective"] == "bytes" else cost["instructions"]


def exhaustive(problem, work: Path, attempts=3):
    """The exact enumerator's least primary cost, or None. It must exhaust the grammar with every checker verdict
    decided; search's own limits (5 s per check, 60 s in all) are fixed, so a loaded host gets fresh attempts."""
    (work / "problem.json").write_text(json.dumps(problem))
    folder = work / "session"  # session-mode tests read it afterwards
    for _ in range(attempts):
        if folder.exists():
            shutil.rmtree(folder)
        session.open_session(work / "problem.json", folder)
        result = session.search(folder, max_candidates=100_000, max_checks=128, deadline_ms=60_000,
                                timeout_ms=5000, prune=True)
        undecided = [v for v in result.get("checker_verdicts", []) if v["status"] not in ("equivalent",
                                                                                         "counterexample")]
        if result.get("stop_reason") == "grammar-exhausted" and not undecided:
            return None if result["cost"] is None else primary(problem, result["cost"])
    raise AssertionError(f"enumeration did not exhaust with decided verdicts: {result.get('stop_reason')}, "
                         f"{len(undecided)} undecided")


_LEAST = {}


def least_cost(name, problem):
    """Cached per problem: both modes compare with the same exhaustive enumeration."""
    if name not in _LEAST:
        with tempfile.TemporaryDirectory() as directory:
            _LEAST[name] = exhaustive(problem, Path(directory))
    return _LEAST[name]


def covers(entry, palette):
    """The test's own exact-cover check: every candidate of the length lies in exactly one recorded subspace,
    counted per slot range rather than by the realize code's own check."""
    boxes = [[tuple(r) for r in sub["ranges"]] for sub in entry["subspaces"]]
    if any(len(b) != entry["length"] or any(low < 0 or high > palette or low >= high for low, high in b)
           for b in boxes):
        return False
    total = 0
    for b in boxes:
        size = 1
        for low, high in b:
            size *= high - low
        total += size
    for i, first in enumerate(boxes):
        for second in boxes[i + 1:]:
            if all(max(x[0], y[0]) < min(x[1], y[1]) for x, y in zip(first, second)):
                return False
    return total == palette ** entry["length"]


class RealizeCrossCheck(unittest.TestCase):
    def cross_check(self, **mode):
        cases = problems()
        self.assertGreaterEqual(len(cases), 15)
        for name, problem in cases:
            with self.subTest(problem=name, **mode):
                least = least_cost(name, problem)
                if least is None:
                    record = realize.realize(problem, None, QUERY_MS, deadline_ms=DEADLINE_MS, **mode)
                    self.assertEqual(record["status"], "unrealizable", record)
                    self.assertEqual(sorted([e["length"] for e in record["lengths"]]
                                            + record["excluded_by_empty_palette"]),
                                     list(range(problem["max_instructions"] + 1)))
                    self.partition_scope(problem, record, **mode)
                    continue
                below = realize.realize(problem, least, QUERY_MS, deadline_ms=DEADLINE_MS, **mode)
                self.assertEqual(below["status"], "unrealizable", below)
                self.assertTrue(all(e["status"] == "unrealizable" for e in below["lengths"]), below)
                self.partition_scope(problem, below, **mode)
                at = realize.realize(problem, least + 1, QUERY_MS, deadline_ms=DEADLINE_MS, **mode)
                self.assertEqual(at["status"], "realizable", at)
                found = at["lengths"][-1]
                self.assertEqual(found["verification"]["status"], "equivalent")
                self.assertEqual(primary(problem, found["verification"]["cost"]), least)

    def partition_scope(self, problem, record, groups=1, workers=1):
        if groups == 1:
            self.assertNotIn("partition", record)
            return
        palette = len(problem["grammar"])
        self.assertEqual(record["partition"]["groups_per_slot"], min(groups, palette))
        for entry in record["lengths"]:
            self.assertEqual(entry["subspace_count"], min(groups, palette) ** entry["length"])
            self.assertTrue(covers(entry, palette), entry)
            for subspace in entry["subspaces"]:
                self.assertEqual(subspace["status"], "unrealizable")
                self.assertRegex(subspace["query_sha256"], "^[0-9a-f]{64}$")

    def test_every_small_problem_agrees_with_exhaustive_enumeration(self):
        self.cross_check()

    def test_every_small_problem_agrees_in_partitioned_mode(self):
        self.cross_check(groups=2, workers=4)

    def test_adaptive_splitting_agrees_and_covers_the_space(self):
        # A 1 ms probe makes nearly every query split, down to single forms where needed.
        for name in ("add64-x86", "umin-x86", "triple64-x86"):
            problem = read_json(DATA / f"{name}.json")
            with self.subTest(problem=name):
                least = least_cost(name, problem)
                below = realize.realize(problem, least, QUERY_MS, deadline_ms=DEADLINE_MS, workers=2, split_ms=1)
                self.assertEqual(below["status"], "unrealizable", below)
                self.assertGreater(sum(e["splits"] for e in below["lengths"]), 0)
                for entry in below["lengths"]:
                    self.assertTrue(covers(entry, len(problem["grammar"])), entry)
                self.assertEqual(realize.replay(problem, below, QUERY_MS)["status"], "replayed")
                at = realize.realize(problem, least + 1, QUERY_MS, deadline_ms=DEADLINE_MS, workers=2, split_ms=1)
                self.assertEqual(at["status"], "realizable", at)
                self.assertEqual(primary(problem, at["lengths"][-1]["verification"]["cost"]), least)

    def test_admission_encoding_equals_the_checkers_admission(self):
        """Every selector sequence the query admits (no examples, no cost bound) is one the checker admits, and the
        other way round, on palettes with flag reads and with region-base writes."""
        import z3
        from itertools import product
        from machine_code import verify2, verify3
        from machine_code.session import _validated
        from machine_code.targets import encode_program
        cases = [("umin-x86", read_json(DATA / "umin-x86.json")), ("add128-x86", read_json(DATA / "add128-x86.json")),
                 ("flag reads", FLAG_READS), ("base writes", BASE_WRITES),
                 ("memory swap", read_json(DATA / "memory-kernels/swap-words-x86.json")["problem"])]
        for name, problem in cases:
            problem = _validated(problem)
            memory = problem["schema"].endswith("/3")
            for length in range(min(problem["max_instructions"], 3) + 1):
                with self.subTest(problem=name, length=length):
                    solver, selectors, _ = realize.query(z3, problem, length, None, [])
                    encoded = set()
                    while solver.check() == z3.sat:
                        model = solver.model()
                        choice = tuple(model.eval(v, model_completion=True).as_long() for v in selectors)
                        encoded.add(choice)
                        solver.add(z3.Or([v != c for v, c in zip(selectors, choice)]) if selectors else z3.BoolVal(
                            False, ctx=solver.ctx))
                    admitted = set()
                    for choice in product(range(len(problem["grammar"])), repeat=length):
                        code = encode_program(problem["target"], [problem["grammar"][i] for i in choice])
                        try:
                            if memory:
                                verify3.admit(problem, verify3.decode(problem["target"], code))
                            else:
                                verify2.require_defined_flag_reads(problem["target"],
                                                                   verify2.decode(problem["target"], code))
                        except ValueError:
                            continue
                        admitted.add(choice)
                    self.assertEqual(encoded, admitted)
                    if name in ("base writes", "flag reads") and length >= 2:
                        self.assertLess(len(admitted), len(problem["grammar"]) ** length)

    def test_an_undecided_subspace_makes_the_length_inconclusive(self):
        # One weak example state and one round: every subspace's first solution agrees on it and is then refuted,
        # so no subspace is decided.
        record = realize.realize(NO_PROGRAM, None, QUERY_MS, deadline_ms=DEADLINE_MS, max_rounds=1,
                                 seeds=[((0, 0), {}, None)], groups=2)
        self.assertEqual(record["status"], "inconclusive", record)
        self.assertTrue(all(e["status"] == "inconclusive" for e in record["lengths"]), record)

    def test_a_missing_palette_form_is_caught_by_the_cross_check(self):
        with tempfile.TemporaryDirectory() as directory:
            least = exhaustive(LAST_FORM_NEEDED, Path(directory))
        self.assertEqual(least, 5)
        at = realize.realize(LAST_FORM_NEEDED, least + 1, QUERY_MS, deadline_ms=DEADLINE_MS)
        self.assertEqual(at["status"], "realizable", at)
        self.assertEqual(at["lengths"][-1]["code_hex"], "488d0437c3")

    def test_records_state_scope_and_never_global_optimality(self):
        record = realize.realize(LAST_FORM_NEEDED, 5, QUERY_MS, deadline_ms=DEADLINE_MS)
        self.assertEqual(record["status"], "unrealizable")
        self.assertFalse(record["global_optimality"])
        self.assertEqual(record["authority"], "none")
        self.assertEqual(record["excluded_by_cost"], [2])
        self.assertEqual(record["lengths"][0]["statement"],
                         "no admitted program of length 0 with bytes cost below 5 exists "
                         "under the stated model and palette")
        self.assertRegex(record["lengths"][0]["query_sha256"], "^[0-9a-f]{64}$")


class RealizeReplay(unittest.TestCase):
    def test_replay_rebuilds_the_same_query_and_tampering_fails(self):
        problem = read_json(DATA / "umin-x86.json")
        record = realize.realize(problem, 11, QUERY_MS, deadline_ms=DEADLINE_MS)
        self.assertEqual(record["status"], "unrealizable", record)
        self.assertEqual(realize.replay(problem, record, QUERY_MS)["status"], "replayed")
        tampered = json.loads(json.dumps(record))
        tampered["lengths"][-1]["examples"][0]["arguments"][0] = "7"
        result = realize.replay(problem, tampered, QUERY_MS)
        self.assertEqual(result["status"], "failed")
        self.assertFalse(result["replayed"][-1]["query_hash_matches"])
        with self.assertRaises(ValueError):
            realize.replay(read_json(DATA / "add64-x86.json"), record)

    def test_cli_realize_writes_a_record_and_replays_it(self):
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory) / "record.json"
            argv = ["realize", "--problem", str(DATA / "add64-rv64i.json"), "--below", "8", "--out", str(out),
                    *BUDGET]
            with contextlib.redirect_stdout(io.StringIO()) as printed:
                code = run.main(argv)
            self.assertEqual(code, 0, printed.getvalue())
            self.assertEqual(json.loads(printed.getvalue())["status"], "unrealizable")
            with contextlib.redirect_stdout(io.StringIO()) as printed:
                code = run.main(["realize", "--problem", str(DATA / "add64-rv64i.json"), "--replay", str(out),
                                 *BUDGET])
            self.assertEqual(code, 0, printed.getvalue())
            self.assertEqual(json.loads(printed.getvalue())["status"], "replayed")
            with contextlib.redirect_stdout(io.StringIO()) as printed:
                code = run.main(argv)
            self.assertEqual(code, 65, printed.getvalue())

    def test_partitioned_record_replays_and_a_missing_subspace_fails(self):
        problem = read_json(DATA / "umin-x86.json")
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory) / "record.json"
            argv = ["realize", "--problem", str(DATA / "umin-x86.json"), "--below", "11", "--groups", "2",
                    "--workers", "2", "--out", str(out), *BUDGET]
            with contextlib.redirect_stdout(io.StringIO()) as printed:
                code = run.main(argv)
            self.assertEqual(code, 0, printed.getvalue())
            record = read_json(out)
        self.assertEqual(record["status"], "unrealizable")
        self.assertEqual([e["subspace_count"] for e in record["lengths"]], [1, 2, 4, 8])
        self.assertEqual(realize.replay(problem, record, QUERY_MS)["status"], "replayed")
        missing = json.loads(json.dumps(record))
        missing["lengths"][-1]["subspaces"].pop()
        result = realize.replay(problem, missing, QUERY_MS)
        self.assertEqual(result["status"], "failed")
        self.assertFalse(result["replayed"][-1]["coverage"])

    def test_replay_refuses_a_changed_bound(self):
        # One lea form, at most one instruction: below 1 instruction only length 0 is asked. Raising the bound to 2
        # makes length 1 eligible, and a one-instruction program exists there.
        problem = {"schema": P1, "target": "x86_64-sysv-register/1", "inputs": ["x", "y"],
                   "expression": ["add", ["input", "x"], ["input", "y"]],
                   "grammar": [["lea", "rax", "rdi", "rsi", 1]], "max_instructions": 1, "objective": "instructions"}
        record = realize.realize(problem, 1, QUERY_MS, deadline_ms=DEADLINE_MS)
        self.assertEqual((record["status"], [e["length"] for e in record["lengths"]], record["excluded_by_cost"]),
                         ("unrealizable", [0], [1]))
        self.assertEqual(realize.replay(problem, record, QUERY_MS)["status"], "replayed")
        tampered = json.loads(json.dumps(record))
        tampered["bound"] = 2
        for entry in tampered["lengths"]:
            entry["statement"] = entry["statement"].replace("below 1 ", "below 2 ")
        result = realize.replay(problem, tampered, QUERY_MS)
        self.assertEqual(result["status"], "failed", result)
        self.assertIn("not every eligible length is present", result["failures"])
        self.assertEqual(realize.realize(problem, 2, QUERY_MS, deadline_ms=DEADLINE_MS)["status"], "realizable")

    def test_replay_refuses_a_length_with_no_subspaces(self):
        # Independent review 3, finding 1: a genuine partitioned record below 1, its bound raised to 2, with length 1
        # claimed unrealizable by an empty subspace list. No query is replayed for that length, so the failed
        # coverage must fail the replay on its own, through the API and the CLI.
        record = realize.realize(LEA_ONLY, 1, QUERY_MS, deadline_ms=DEADLINE_MS, groups=2)
        self.assertEqual(realize.replay(LEA_ONLY, record, QUERY_MS)["status"], "replayed")
        forged = json.loads(json.dumps(record))
        forged["bound"] = 2
        forged["excluded_by_cost"] = []
        forged["lengths"][0]["statement"] = realize.STATEMENT.format(length=0, objective="instructions", bound=2)
        statement = realize.STATEMENT.format(length=1, objective="instructions", bound=2)
        forged["lengths"].append({"length": 1, "status": "unrealizable", "subspaces": [], "subspace_count": 0,
                                  "statement": statement})
        result = realize.replay(LEA_ONLY, forged, QUERY_MS)
        self.assertEqual(result["status"], "failed", result)
        self.assertEqual(result["failures"], ["length 1: subspaces do not cover the palette space exactly once"])
        with tempfile.TemporaryDirectory() as directory:
            problem, path = Path(directory) / "problem.json", Path(directory) / "record.json"
            problem.write_text(json.dumps(LEA_ONLY))
            path.write_text(json.dumps(forged))
            with contextlib.redirect_stdout(io.StringIO()) as printed:
                code = run.main(["realize", "--problem", str(problem), "--replay", str(path), *BUDGET])
        self.assertEqual((code, json.loads(printed.getvalue())["status"]), (1, "failed"), printed.getvalue())
        # The known solution the forged claim denies.
        fresh = realize.realize(LEA_ONLY, 2, QUERY_MS, deadline_ms=DEADLINE_MS)
        self.assertEqual((fresh["status"], fresh["lengths"][-1]["code_hex"]), ("realizable", "488d0437c3"))

    def test_replay_binds_the_whole_record(self):
        problem = read_json(DATA / "umin-x86.json")
        record = realize.realize(problem, 11, QUERY_MS, deadline_ms=DEADLINE_MS)
        self.assertEqual(realize.replay(problem, record, QUERY_MS)["status"], "replayed")
        edits = {
            "missing length": lambda r: r["lengths"].pop(1),
            "repeated length": lambda r: r["lengths"].insert(1, json.loads(json.dumps(r["lengths"][1]))),
            "relabelled length": lambda r: r["lengths"][2].__setitem__("length", 1),
            "status": lambda r: r.__setitem__("status", "inconclusive"),
            "entry status": lambda r: r["lengths"][0].__setitem__("status", "inconclusive"),
            "statement": lambda r: r["lengths"][0].__setitem__("statement", "no program exists"),
            "objective": lambda r: r.__setitem__("objective", "instructions"),
            "excluded lengths": lambda r: r.__setitem__("excluded_by_cost", [3]),
            "mode": lambda r: r.__setitem__("partition", {"groups_per_slot": 1}),
        }
        for name, edit in edits.items():
            with self.subTest(edit=name):
                tampered = json.loads(json.dumps(record))
                edit(tampered)
                result = realize.replay(problem, tampered, QUERY_MS)
                self.assertEqual(result["status"], "failed", result)
                self.assertTrue(result["failures"], result)
        found = realize.realize(problem, 12, QUERY_MS, deadline_ms=DEADLINE_MS)
        self.assertEqual((found["status"], realize.replay(problem, found, QUERY_MS)["status"]),
                         ("realizable", "replayed"))
        wrong = json.loads(json.dumps(found))
        # Three palette forms that return x, not the minimum: the recorded bytes must recheck equivalent.
        wrong["lengths"][-1]["code_hex"] = "4889f89090c3"
        result = realize.replay(problem, wrong, QUERY_MS)
        self.assertEqual(result["status"], "failed")
        self.assertEqual(result["failures"], ["length 3: recorded bytes do not recheck below the bound"])

    def test_zero_cost_incumbent_and_empty_palette_are_discharged(self):
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory)
            self.assertEqual(exhaustive(ZERO_COST, work), 0)
            with contextlib.redirect_stdout(io.StringIO()) as printed:
                code = run.main(["realize", "--session", str(work / "session"), *BUDGET])
            result = json.loads(printed.getvalue())
        self.assertEqual(code, 0, result)
        self.assertEqual((result["status"], result["bound"], result["lengths"], result["excluded_by_cost"]),
                         ("unrealizable", 0, [], [0]))
        self.assertEqual(realize.replay(ZERO_COST, result, QUERY_MS)["status"], "replayed")
        record = realize.realize(EMPTY_PALETTE, None, QUERY_MS, deadline_ms=DEADLINE_MS)
        self.assertEqual((record["status"], [e["length"] for e in record["lengths"]],
                          record["excluded_by_empty_palette"]), ("unrealizable", [0], [1]))
        self.assertEqual(realize.replay(EMPTY_PALETTE, record, QUERY_MS)["status"], "replayed")
        partitioned = realize.realize(EMPTY_PALETTE, None, QUERY_MS, deadline_ms=DEADLINE_MS, groups=2, workers=2)
        self.assertEqual((partitioned["status"], realize.replay(EMPTY_PALETTE, partitioned, QUERY_MS)["status"]),
                         ("unrealizable", "replayed"))
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "empty.json"
            path.write_text(json.dumps(EMPTY_PALETTE))
            with contextlib.redirect_stdout(io.StringIO()) as printed:
                code = run.main(["realize", "--problem", str(path), *BUDGET])
        self.assertEqual(code, 0, printed.getvalue())

    def test_cli_session_mode_asks_below_the_rechecked_incumbent(self):
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory)
            exhaustive(read_json(DATA / "add64-x86.json"), work)
            with contextlib.redirect_stdout(io.StringIO()) as printed:
                code = run.main(["realize", "--session", str(work / "session"), *BUDGET])
            result = json.loads(printed.getvalue())
            self.assertEqual(code, 0, result)
            self.assertEqual(result["status"], "unrealizable")
            self.assertEqual(result["bound"], 5)
            self.assertEqual(result["incumbent"]["recheck"], "equivalent")



class RealizeBudget(unittest.TestCase):
    def test_partitioned_search_stops_at_its_deadline(self):
        # Independent review 3, finding 7: after the deadline no further subspace may be generated or decided, with
        # or without workers, and every length is still reported (inconclusive where undecided). With workers the
        # space is cut to lengths 0 to 12 (8,191 subspaces), so that a defect that submits every job still finishes.
        for workers, longest in ((1, 16), (2, 12)):
            with self.subTest(workers=workers):
                problem = dict(CONSTANTS_ONLY, max_instructions=longest)
                began = time.monotonic()
                record = realize.realize(problem, None, QUERY_MS, deadline_ms=100, groups=2, workers=workers)
                elapsed = time.monotonic() - began
                self.assertEqual(record["status"], "inconclusive", record["status"])
                self.assertEqual([e["length"] for e in record["lengths"]], list(range(longest + 1)))
                self.assertEqual(record["lengths"][-1]["status"], "inconclusive")
                self.assertIn("budget-exhausted", record["lengths"][-1]["reason"])
                visited = sum(e.get("subspace_count", 1) for e in record["lengths"])
                self.assertLess(visited, 1000, visited)
                self.assertLess(elapsed, 60, elapsed)


if __name__ == "__main__":
    unittest.main()
