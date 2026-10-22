#!/usr/bin/env python3
"""Pure-function tests for tools/measure_program_search.py (no CLI needed)."""
from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import measure_program_search as measure  # noqa: E402
import program_search_reference as reference  # noqa: E402

FIXTURES = Path(__file__).resolve().parents[1] / "crates/zeno-fcis-program-search/tests/data"


def tuple_(items: list[bytes]) -> bytes:
    return b"\x08" + len(items).to_bytes(4, "big") + b"".join(items)


def i128(value: int) -> bytes:
    return b"\x04" + value.to_bytes(16, "big", signed=True)


def u128(value: int) -> bytes:
    return b"\x03" + value.to_bytes(16, "big")


def program(nodes: list[list[int]], roots: list[int], low: int = 0, high: int = 3,
            out_high: int = 3, profile: bytes = b"zeno-fcis/finite-i64/1") -> bytes:
    domain = tuple_([b"\x01", i128(low), i128(high)])
    output = tuple_([b"\x01", i128(low), i128(out_high)])
    return tuple_([b"\x06" + len(profile).to_bytes(4, "big") + profile,
                   tuple_([tuple_([domain]), tuple_([output])]),
                   tuple_([tuple_([i128(v) for v in node]) for node in nodes]),
                   tuple_([u128(r) for r in roots])])


def row(name: str, family: str, original: int, **columns) -> dict:
    result = {"instance": name, "family": family, "input_tuples": 4, "defects": [], "controls": {},
              "original": {"nodes": original, "bytes": 100 + original}, "search_deterministic": True}
    for key in measure.COLUMNS[1:]:
        value = columns.get(key)
        if value is None:
            result[key] = None
        else:
            nodes, accepted = value
            result[key] = {"nodes": nodes, "bytes": 100 + nodes, "check": {"accepted": accepted}}
            if not accepted:
                result["defects"].append({"column": key})
    result["best"] = measure.best_of(result)
    return result


class NodeCount(unittest.TestCase):
    def test_counts_instructions_and_refuses_trailing_bytes(self):
        body = program([[0, 0], [1, 7], [3, 0, 1]], [2])
        self.assertEqual(measure.node_count(body), 3)
        with self.assertRaises(measure.MeasureError):
            measure.node_count(body + b"\x00")
        with self.assertRaises(measure.MeasureError):
            measure.node_count(body[:-1])


class Summary(unittest.TestCase):
    def test_a_refused_program_counts_as_a_defect_and_the_original_stands_in(self):
        rows = [row("x-1", "x", 10, a=(8, True), b=(6, True), c=(6, True)),
                row("x-2", "x", 10, a=(9, True), b=(5, False)),
                row("y-1", "y", 4)]
        self.assertEqual(rows[0]["best"]["column"], "b")
        self.assertEqual(rows[1]["best"]["column"], "a")
        summary = measure.summarize(rows)
        self.assertEqual(summary["totals"]["original"]["nodes"], 24)
        self.assertEqual(summary["totals"]["a"]["nodes"], 8 + 9 + 4)
        # The refused (b) program of x-2 is replaced by its original.
        self.assertEqual(summary["totals"]["b"]["nodes"], 6 + 10 + 4)
        self.assertEqual(summary["totals"]["best"]["nodes"], 6 + 9 + 4)
        self.assertEqual(summary["proposer_defects"], 1)
        self.assertEqual(summary["b_smaller_than_a"], 1)
        self.assertEqual(summary["b_larger_than_a"], 1)
        self.assertEqual(summary["reported_programs"], 5)
        self.assertEqual(summary["accepted_and_replayed"], 4)
        self.assertEqual(summary["families"]["x"]["b_smaller_than_a"], 1)
        text = measure.markdown({"summary": summary, "rows": rows, "cli_sha256": "c", "search_sha256": "s"})
        self.assertIn("5 / 105 DEFECT", text)
        self.assertIn("| **Total** |", text)


class Controls(unittest.TestCase):
    def test_a_search_output_without_all_its_controls_is_missing_some(self):
        bare = row("x-1", "x", 10, a=(8, True), b=(6, True))
        self.assertEqual(measure.missing_controls(bare), 1)
        planned = dict(bare, control_points={"b": {"ordinals": [0, 3, 7], "outputs": 2,
                                                   "expected_controls": 6}})
        planned["controls"] = {f"b-o{o}-t{t}": {"ran": True} for o in range(2) for t in (0, 3)}
        self.assertEqual(measure.missing_controls(planned), 2)
        planned["controls"].update({f"b-o{o}-t7": {"ran": True} for o in range(2)})
        self.assertEqual(measure.missing_controls(planned), 0)
        self.assertEqual(measure.summarize([bare])["controls_missing"], 1)


class Reference(unittest.TestCase):
    def test_eager_traps_output_domain_and_first_difference(self):
        big = (1 << 63) - 1
        # x + 1 over x in [MAX - 1, MAX], unused: the last tuple still traps.
        trapping = reference.decode(program([[0, 0], [1, 1], [3, 0, 1]], [0], big - 1, big, big))
        self.assertEqual(reference.table(trapping), [("ok", (big - 1,)), ("error", "arithmetic")])
        narrow = reference.decode(program([[0, 0]], [0], 0, 3, 1))
        self.assertEqual([kind for kind, _ in reference.table(narrow)], ["ok", "ok", "error", "error"])
        plain = reference.decode(program([[0, 0]], [0]))
        shifted = reference.decode(program([[0, 0], [1, 0], [5, 0, 1], [1, 2], [9, 2, 3, 0]], [4]))
        self.assertEqual(reference.compare(plain, shifted)["ordinal"], 0)
        self.assertEqual(reference.compare(plain, plain)["status"], "equal")
        self.assertEqual(reference.compare(plain, narrow)["status"], "abi-differs")

    def test_profiles_are_the_ones_the_rust_source_defines(self):
        root = Path(__file__).resolve().parents[1] / "crates/zeno-fcis-synthesis/src"
        current = (root / "finite/ir.rs").read_text()
        historical = (root / "finite_runtime.rs").read_text()
        self.assertIn(f'pub const PROFILE: &str = "{reference.PROFILE.decode()}";', current)
        self.assertIn(f'pub const I64_PROFILE: &str = "{reference.I64_PROFILE.decode()}";', historical)

    def test_the_profile_sets_the_integer_width(self):
        big, top = (1 << 63) - 1, (1 << 127) - 1
        add_one = [[0, 0], [1, 1], [3, 0, 1]]
        current = reference.decode(program(add_one, [0], big - 1, big, big, reference.PROFILE))
        self.assertEqual(current.profile, reference.PROFILE)
        self.assertEqual(reference.table(current), [("ok", (big - 1,)), ("ok", (big,))])
        wide = reference.decode(program(add_one, [0], top - 1, top, top, reference.PROFILE))
        self.assertEqual(reference.table(wide), [("ok", (top - 1,)), ("error", "arithmetic")])
        for bad in (program([[0, 0]], [0], 0, 1 << 63, 3),
                    program([[0, 0], [1, 1 << 63], [3, 0, 1]], [0]),
                    program([[0, 0]], [0], profile=b"zeno-fcis/finite-i128/2")):
            with self.assertRaises(reference.ProgramRefused):
                reference.decode(bad)
        self.assertEqual(reference.decode(program([[0, 0]], [0], 0, 1 << 63, 3, reference.PROFILE)).inputs,
                         ((False, 0, 1 << 63),))

    def test_refuses_noncanonical_or_ill_typed_bytes(self):
        body = program([[0, 0], [1, 7], [3, 0, 1]], [2])
        for bad in (body + b"\x00", body[:-1], program([[3, 0, 1]], [0]), program([[0, 0], [7, 0, 0]], [1])):
            with self.assertRaises(reference.ProgramRefused):
                reference.decode(bad)

    def test_agrees_with_checked_optimizer_output_on_core_fixtures(self):
        names = sorted({path.name.split(".")[0] for path in FIXTURES.glob("*.zcve")})
        self.assertEqual(len(names), 7)
        for name in names:
            original = reference.decode((FIXTURES / f"{name}.original.zcve").read_bytes())
            optimized = reference.decode((FIXTURES / f"{name}.optimized.zcve").read_bytes())
            self.assertEqual(reference.compare(original, optimized)["status"], "equal", name)
            self.assertEqual(measure.node_count((FIXTURES / f"{name}.optimized.zcve").read_bytes()),
                             len(optimized.nodes))


if __name__ == "__main__":
    unittest.main()
