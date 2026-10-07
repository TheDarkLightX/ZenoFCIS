#!/usr/bin/env python3
"""Admission checks for the API custody fixture runner; they do not compile anything."""
import re
import unittest

import check_api_refusals as gate


class FixtureTable(unittest.TestCase):
    def test_every_fixture_has_exactly_one_expectation(self):
        names = {path.stem for path in gate.FIXTURES.glob("*.rs")}
        self.assertEqual(names, set(gate.EXPECTED))

    def test_expectations_are_error_codes_or_positive(self):
        for name, expected in gate.EXPECTED.items():
            with self.subTest(name=name):
                self.assertTrue(expected is None or re.fullmatch(r"E\d{4}", expected))
        positives = sorted(name for name, expected in gate.EXPECTED.items() if expected is None)
        self.assertEqual(positives, ["legacy_oracle_positive", "normal_positive"])

    def test_refusal_fixtures_outnumber_positive_routes(self):
        refusals = [name for name, expected in gate.EXPECTED.items() if expected is not None]
        self.assertEqual(len(refusals), 30)


if __name__ == "__main__":
    unittest.main()
