"""Finite symmetry regressions and reproducible mirrored-machine differential."""

from dataclasses import replace
import random
import unittest
from unittest.mock import patch

import behavior2 as b
import symmetry2 as s


BASE = """zal 2;
machine borrowers;
states env, user;
events left, right, tick;
context none;
var aa in 0..=1 initially 0;
var bb in 0..=1 initially 0;
initial user;
step atomic; frame control_and_variables;
default reject unavailable unchanged;
rule borrow_a: user + left [aa == 0] -> accept env, aa = 1;
rule borrow_b: user + right [bb == 0] -> accept env, bb = 1;
rule return_turn: env + tick [true] -> accept user;
player controller moves in user;
player environment moves in env;
symmetry rivals: swap (aa bb) (left right);
"""


class SymmetryDeclarationTests(unittest.TestCase):
    def test_paired_roundtrip(self):
        model = b.parse(BASE)
        self.assertEqual(model, b.parse(model.render()))
        self.assertEqual(model, b.parse(model.render(True), "english"))
        self.assertEqual(model.revision, b.parse(BASE.replace('(aa bb) (left right)', '(right left) (bb aa)')).revision)
        self.assertEqual(model.symmetries, (("rivals", (("aa", "bb"), ("left", "right"))),))

    def test_empty_declarations_do_not_change_data(self):
        model = b.parse(BASE)
        plain = replace(model, symmetries=())
        self.assertNotIn("symmetry", plain.data())
        self.assertNotEqual(model.revision, plain.revision)

    def test_declaration_refusals_name_statement(self):
        for text in ['(aa aa)', '(aa bb) (aa tick)', '(aa tick)', '(missing bb)', '(env aa)', 'aa bb']:
            with self.subTest(text=text), self.assertRaisesRegex(b.Refusal, "Symmetry rivals: declaration"):
                b.parse(BASE.replace('(aa bb) (left right)', text))

    def test_ranges_and_initial_values_match(self):
        for replacement in ['var bb in 0..=2 initially 0;', 'var bb in 0..=1 initially 1;']:
            with self.assertRaisesRegex(b.Refusal, "Symmetry rivals: declaration"):
                b.parse(BASE.replace('var bb in 0..=1 initially 0;', replacement))

    def test_generator_bound(self):
        source = BASE + '\n'.join(f'symmetry g{i}: swap (aa bb);' for i in range(4))
        with self.assertRaisesRegex(b.Refusal, "Symmetry .*: finiteness"):
            b.parse(source)

    def test_owned_ir(self):
        model = b.parse(BASE)
        for items in [[], [('rivals', [('aa', 'bb')])], (('rivals', (['aa', 'bb'],)),)]:
            with self.assertRaises(b.Refusal):
                replace(model, symmetries=items)

    def test_automorphism_control(self):
        model = b.parse(BASE.replace('rule borrow_b: user + right [bb == 0] -> accept env, bb = 1;', ''))
        with self.assertRaisesRegex(b.Refusal, "Symmetry rivals: automorphism"):
            b.check(model)

    def test_claim_control(self):
        model = b.parse(BASE + 'property single_party: controller can force eventually (aa == 1);\n')
        with self.assertRaisesRegex(b.Refusal, "Symmetry rivals: claim-invariance.*observes a swapped name"):
            b.check(model)

    def test_every_claim_family(self):
        for claim in ['invariant observed: aa == 0;', 'require observed: aa\' == 0;',
                      'fairness observed: infinitely often (aa\' == 0);',
                      'property observed: always(aa == 0);']:
            with self.subTest(claim=claim), self.assertRaisesRegex(b.Refusal, 'claim-invariance'):
                b.check(b.parse(BASE + claim))

    def test_initial_fixed(self):
        with self.assertRaisesRegex(b.Refusal, 'Symmetry rivals: automorphism: initial'):
            b.check(b.parse(BASE.replace('(aa bb) (left right)', '(env user)')))

    def test_players_preserved(self):
        model = b.parse(BASE)
        with self.assertRaisesRegex(b.Refusal, 'Symmetry rivals: players'):
            s.check_players(model, 'rivals', {'env': 'user', 'user': 'env'})

    def test_valid_mirrored_rules(self):
        self.assertEqual(b.check(b.parse(BASE))['status'], 'pass-with-scope')

    def test_no_symmetry_still_checks_obligations(self):
        model = b.parse(BASE + 'invariant observed: aa == 0;')
        with self.assertRaisesRegex(b.Refusal, 'Symmetry rivals: claim-invariance'):
            b.check(model, no_symmetry=True)


if __name__ == '__main__':
    unittest.main()
