"""Source-bound finite comparisons for two definition-only reading patterns."""
from itertools import product
import json
from pathlib import Path
import re
import unittest

import contract_patterns as p
import ladder as l


def compact(text):
    return re.sub(r"\s+", "", text)


class ContractPatternTests(unittest.TestCase):
    def trees(self, name):
        data = json.loads(p.documents()[name + ".json"])
        ctx = l.Context.from_data(data)
        canonical = l.parse(data["formula"], "typed", ctx)
        trees = [l.parse(l.render(canonical, rung, ctx), rung, ctx) for rung in l.RUNGS]
        for tree in trees:
            self.assertEqual(l.normal(tree, ctx), l.normal(canonical, ctx))
        precise, plain = (l.render(canonical, rung, ctx) for rung in ("precise", "plain"))
        self.assertLess(len(plain.split()), len(precise.split()), (name, precise, plain))
        print(f"{name}: precise {len(precise.split())} words; plain {len(plain.split())} words; definition retained")
        return data, ctx, trees

    def compare(self, trees, ctx, fixture, expected):
        for tree in trees:
            self.assertIs(l.evaluate(tree, ctx, fixture), expected, fixture)

    def test_generated_examples_and_their_source_pins_are_current(self):
        for name, text in p.documents().items():
            self.assertEqual((p.OUTPUT / name).read_text(), text)
            data = json.loads(text)
            source = data["contract_source"]
            self.assertEqual(source, p.source(source["path"], source["law"]))

    def test_only_chosen_copy_changes_three_copy_contract(self):
        data, ctx, trees = self.trees("library_three_frame")
        # Independent spelling of the scalar contract's three guarded equalities.
        self.assertEqual(compact(data["contract_source"]["body"]), compact(
            "(command.101.121 == 1 -> post.100.111 == pre.100.111 && post.100.112 == pre.100.112) && "
            "(command.101.121 == 2 -> post.100.110 == pre.100.110 && post.100.112 == pre.100.112) && "
            "(command.101.121 == 3 -> post.100.110 == pre.100.110 && post.100.111 == pre.100.111)"))
        self.assertIn("type105intHolderin0..=3;", compact((p.ROOT / data["contract_source"]["path"]).read_text()))
        count = 0
        for before in product(range(4), repeat=3):
            for after in product(range(4), repeat=3):
                equal = [a == b for a, b in zip(before, after, strict=True)]
                for chosen, commits in product((1, 2, 3), (False, True)):
                    expected = not commits or ((chosen != 1 or equal[1] and equal[2]) and
                                              (chosen != 2 or equal[0] and equal[2]) and
                                              (chosen != 3 or equal[0] and equal[1]))
                    self.compare(trees, ctx, p.frame_fixture([1, 2, 3], chosen, before, after, commits), expected)
                    count += 1
        print(f"THREE COPIES: {count} complete input valuations, {count * len(trees)} rung evaluations")

    def test_only_chosen_copy_changes_six_copy_contract(self):
        data, ctx, trees = self.trees("library_six_frame")
        self.assertEqual(compact(data["contract_source"]["body"]), compact(
            "forall i in table 110 where i != command.101.121 { post.100.110[i] == pre.100.110[i] }"))
        text = compact((p.ROOT / data["contract_source"]["path"]).read_text())
        self.assertIn("table110100copies105size6;", text)
        self.assertIn("type105intHolderin0..=3;", text)
        count = 0
        # The source law is a conjunction of independent per-copy equalities.
        # Exhaust each chosen/observed pair and both holder values; this is
        # pointwise coverage, not all 4^12 complete state pairs.
        for chosen, observed, before_value, after_value, commits in product(range(6), range(6), range(4), range(4), (False, True)):
            before, after = [0] * 6, [0] * 6
            before[observed], after[observed] = before_value, after_value
            expected = not commits or observed == chosen or before_value == after_value
            self.compare(trees, ctx, p.frame_fixture(range(6), chosen, before, after, commits), expected)
            count += 1
        print(f"SIX COPIES: {count} pointwise valuations, {count * len(trees)} rung evaluations")

    def test_missing_entries_hold_zero_in_two_contracts(self):
        for name, maximum, body, declaration in (
            ("token_missing_zero", 3, "forall e in entries 110 { post.100.e == 1 || post.100.e.140 == 0 }", "type105intAmountin0..=3;"),
            ("member_missing_zero", 2, "forall m in entries 110 { post.100.m == 1 || post.100.m.140 == 0 }", "type107intLoansin0..=2;")):
            data, ctx, trees = self.trees(name)
            self.assertEqual(compact(data["contract_source"]["body"]), compact(body))
            self.assertEqual(data["contract_source"]["when"], "commit, genesis")
            self.assertIn(declaration, compact((p.ROOT / data["contract_source"]["path"]).read_text()))
            count = 0
            for presence in product((0, 1), repeat=2):
                for amounts in product(range(maximum + 1), repeat=2):
                    # Source spelling: present == 1 OR value == 0, per entry.
                    expected = all(exists == 1 or amount == 0 for exists, amount in zip(presence, amounts, strict=True))
                    self.compare(trees, ctx, p.missing_fixture(presence, amounts), expected)
                    count += 1
            print(f"{name}: {count} complete two-slot valuations, {count * len(trees)} rung evaluations")


if __name__ == "__main__":
    unittest.main()
