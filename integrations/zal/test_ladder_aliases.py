"""The s.t. input forms preserve the pre-alias ladder meaning and output.

The adjacent data file contains the six pre-change syntax trees, all seven
rendered views, and eight stipulated finite relation tables.  It is an oracle
for this compatibility test; no other checkout or generated source is needed.
"""

import json
import unittest
from pathlib import Path

import ladder as l

DATA = json.loads(Path(__file__).with_suffix(".json").read_text(encoding="utf-8"))


def tuples(value):
    if isinstance(value, list):
        return tuple(tuples(item) for item in value)
    return value


def relation_truths(fixture):
    """Independently calculate the six declared formulas from finite sets."""
    values = fixture["domains"]["D"]
    tables = fixture["symbols"]

    def relation(name, *args):
        table = tables[name]
        return next((result for row, result in table["rows"] if tuple(row) == args),
                    table["default"])

    cars = [item for item in values if relation("Car", item)]
    owners = [item for item in values if relation("Owner", item)]
    owned = [sum(relation("Has", owner, car) for car in cars) for owner in owners]
    return (bool(cars), len(cars) == 1, len(cars) >= 2,
            all(count >= 1 for count in owned), all(count == 1 for count in owned),
            all(count == 2 for count in owned))


class LadderAliasTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.context = l.Context.from_data(DATA["context"])
        cls.records = DATA["records"]
        cls.fixtures = DATA["context"]["fixtures"]

    def test_fixture_truths_are_independently_stipulated(self):
        self.assertEqual((len(self.records), len(self.fixtures)), (6, 8))
        for index, fixture in enumerate(self.fixtures):
            expected = tuple(row["fixture_truth"][index] for row in self.records)
            self.assertEqual(relation_truths(fixture), expected, fixture["id"])

    def test_original_42_canonical_views_and_trees(self):
        checked = 0
        for row in self.records:
            tree = l.parse(row["formula"], "typed", self.context)
            self.assertEqual(tree, tuples(row["ast"]))
            for rung in l.RUNGS:
                with self.subTest(formula=row["formula"], rung=rung):
                    self.assertEqual(l.render(tree, rung, self.context), row["views"][rung])
                    checked += 1
        self.assertEqual(checked, 42)

    def test_24_alias_rung_cases_preserve_ast_and_192_fixture_results(self):
        phrases = comparisons = 0
        for row in self.records:
            expected_tree = tuples(row["ast"])
            for rung in ("formal", "precise", "intuitive", "plain"):
                surface = "formal" if rung == "formal" else "precise"
                text = row["aliases"][surface]
                with self.subTest(formula=row["formula"], rung=rung, text=text):
                    parsed = l.parse(text, rung, self.context)
                    self.assertEqual(parsed, expected_tree)
                    self.assertEqual(l.render(parsed, surface, self.context),
                                     row["views"][surface])
                    for fixture, expected in zip(self.fixtures, row["fixture_truth"]):
                        self.assertIs(l.evaluate(parsed, self.context, fixture), expected,
                                      (text, fixture["id"]))
                        comparisons += 1
                    phrases += 1
        self.assertEqual((phrases, comparisons), (24, 192))
        print("ALIASES: 24 phrase/rung cases, 192 finite fixture comparisons, 42 canonical views")

    def test_malformed_aliases_and_open_or_unknown_bindings_are_refused(self):
        row = self.records[0]
        for rung in ("formal", "precise", "intuitive", "plain"):
            surface = "formal" if rung == "formal" else "precise"
            text = row["aliases"][surface]
            for spelling in ("st", "s t", "s..t.", "such s.t.", "s.t"):
                with self.subTest(rung=rung, spelling=spelling), self.assertRaises(l.Refusal):
                    l.parse(text.replace("s.t.", spelling), rung, self.context)
            with self.subTest(rung=rung, extra="trailing"), self.assertRaises(l.Refusal):
                l.parse(text + " Accept it anyway.", rung, self.context)
        with self.assertRaises(l.Refusal):
            l.parse("There exists an entity x s.t. y is a car.", "precise", self.context)
        with self.assertRaises(l.Refusal):
            l.parse("There exists an x in Missing s.t. Car of x is true.",
                    "formal", self.context)


if __name__ == "__main__":
    unittest.main()
