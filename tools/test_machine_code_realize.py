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
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))

from machine_code import realize, run, session  # noqa: E402
from machine_code.model import read_json  # noqa: E402

DATA = ROOT / "test-data/machine-code"
SMALL = 20_000  # candidate sequences, all lengths
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


def problems():
    found = [(path.stem, read_json(path)) for path in sorted(DATA.glob("*.json"))]
    found += [("kernel " + path.stem, read_json(path)["problem"]) for path in sorted((DATA / "kernels").glob("*.json"))]
    found += [("last form needed", LAST_FORM_NEEDED), ("no program", NO_PROGRAM)]
    return [(name, p) for name, p in found
            if sum(len(p["grammar"]) ** k for k in range(p["max_instructions"] + 1)) <= SMALL]


def primary(problem, cost):
    return cost["bytes"] if problem["objective"] == "bytes" else cost["instructions"]


def exhaustive(problem, work: Path):
    """The exact enumerator's least primary cost, or None; it must exhaust the grammar."""
    (work / "problem.json").write_text(json.dumps(problem))
    session.open_session(work / "problem.json", work / "session")
    result = session.search(work / "session", max_candidates=100_000, max_checks=128, deadline_ms=60_000,
                            timeout_ms=2000, prune=True)
    if result["stop_reason"] != "grammar-exhausted":
        raise AssertionError(f"enumeration did not exhaust: {result['stop_reason']}")
    return None if result["cost"] is None else primary(problem, result["cost"])


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
                    record = realize.realize(problem, None, 2000, **mode)
                    self.assertEqual(record["status"], "unrealizable", record)
                    self.assertEqual([e["length"] for e in record["lengths"]],
                                     list(range(problem["max_instructions"] + 1)))
                    self.partition_scope(problem, record, **mode)
                    continue
                below = realize.realize(problem, least, 2000, **mode)
                self.assertEqual(below["status"], "unrealizable", below)
                self.assertTrue(all(e["status"] == "unrealizable" for e in below["lengths"]), below)
                self.partition_scope(problem, below, **mode)
                at = realize.realize(problem, least + 1, 2000, **mode)
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
                below = realize.realize(problem, least, 2000, workers=2, split_ms=1)
                self.assertEqual(below["status"], "unrealizable", below)
                self.assertGreater(sum(e["splits"] for e in below["lengths"]), 0)
                for entry in below["lengths"]:
                    self.assertTrue(covers(entry, len(problem["grammar"])), entry)
                self.assertEqual(realize.replay(problem, below)["status"], "replayed")
                at = realize.realize(problem, least + 1, 2000, workers=2, split_ms=1)
                self.assertEqual(at["status"], "realizable", at)
                self.assertEqual(primary(problem, at["lengths"][-1]["verification"]["cost"]), least)

    def test_an_undecided_subspace_makes_the_length_inconclusive(self):
        # One weak example state and one round: every subspace's first solution agrees on it and is then refuted,
        # so no subspace is decided.
        record = realize.realize(NO_PROGRAM, None, 2000, max_rounds=1, seeds=[((0, 0), {}, None)], groups=2)
        self.assertEqual(record["status"], "inconclusive", record)
        self.assertTrue(all(e["status"] == "inconclusive" for e in record["lengths"]), record)

    def test_a_missing_palette_form_is_caught_by_the_cross_check(self):
        with tempfile.TemporaryDirectory() as directory:
            least = exhaustive(LAST_FORM_NEEDED, Path(directory))
        self.assertEqual(least, 5)
        at = realize.realize(LAST_FORM_NEEDED, least + 1, 2000)
        self.assertEqual(at["status"], "realizable", at)
        self.assertEqual(at["lengths"][-1]["code_hex"], "488d0437c3")

    def test_records_state_scope_and_never_global_optimality(self):
        record = realize.realize(LAST_FORM_NEEDED, 5, 2000)
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
        record = realize.realize(problem, 11, 2000)
        self.assertEqual(record["status"], "unrealizable", record)
        self.assertEqual(realize.replay(problem, record)["status"], "replayed")
        tampered = json.loads(json.dumps(record))
        tampered["lengths"][-1]["examples"][0]["arguments"][0] = "7"
        result = realize.replay(problem, tampered)
        self.assertEqual(result["status"], "failed")
        self.assertFalse(result["replayed"][-1]["query_hash_matches"])
        with self.assertRaises(ValueError):
            realize.replay(read_json(DATA / "add64-x86.json"), record)

    def test_cli_realize_writes_a_record_and_replays_it(self):
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory) / "record.json"
            argv = ["realize", "--problem", str(DATA / "add64-rv64i.json"), "--below", "8", "--out", str(out)]
            with contextlib.redirect_stdout(io.StringIO()) as printed:
                code = run.main(argv)
            self.assertEqual(code, 0, printed.getvalue())
            self.assertEqual(json.loads(printed.getvalue())["status"], "unrealizable")
            with contextlib.redirect_stdout(io.StringIO()) as printed:
                code = run.main(["realize", "--problem", str(DATA / "add64-rv64i.json"), "--replay", str(out)])
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
                    "--workers", "2", "--out", str(out)]
            with contextlib.redirect_stdout(io.StringIO()) as printed:
                code = run.main(argv)
            self.assertEqual(code, 0, printed.getvalue())
            record = read_json(out)
        self.assertEqual(record["status"], "unrealizable")
        self.assertEqual([e["subspace_count"] for e in record["lengths"]], [1, 2, 4, 8])
        self.assertEqual(realize.replay(problem, record)["status"], "replayed")
        missing = json.loads(json.dumps(record))
        missing["lengths"][-1]["subspaces"].pop()
        result = realize.replay(problem, missing)
        self.assertEqual(result["status"], "failed")
        self.assertFalse(result["replayed"][-1]["coverage"])

    def test_cli_session_mode_asks_below_the_rechecked_incumbent(self):
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory)
            exhaustive(read_json(DATA / "add64-x86.json"), work)
            with contextlib.redirect_stdout(io.StringIO()) as printed:
                code = run.main(["realize", "--session", str(work / "session")])
            result = json.loads(printed.getvalue())
            self.assertEqual(code, 0, result)
            self.assertEqual(result["status"], "unrealizable")
            self.assertEqual(result["bound"], 5)
            self.assertEqual(result["incumbent"]["recheck"], "equivalent")


if __name__ == "__main__":
    unittest.main()
