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


class RealizeCrossCheck(unittest.TestCase):
    def test_every_small_problem_agrees_with_exhaustive_enumeration(self):
        cases = problems()
        self.assertGreaterEqual(len(cases), 15)
        for name, problem in cases:
            with self.subTest(problem=name), tempfile.TemporaryDirectory() as directory:
                least = exhaustive(problem, Path(directory))
                if least is None:
                    record = realize.realize(problem, None, 2000)
                    self.assertEqual(record["status"], "unrealizable", record)
                    self.assertEqual([e["length"] for e in record["lengths"]],
                                     list(range(problem["max_instructions"] + 1)))
                    continue
                below = realize.realize(problem, least, 2000)
                self.assertEqual(below["status"], "unrealizable", below)
                self.assertTrue(all(e["status"] == "unrealizable" for e in below["lengths"]), below)
                at = realize.realize(problem, least + 1, 2000)
                self.assertEqual(at["status"], "realizable", at)
                found = at["lengths"][-1]
                self.assertEqual(found["verification"]["status"], "equivalent")
                self.assertEqual(primary(problem, found["verification"]["cost"]), least)

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
