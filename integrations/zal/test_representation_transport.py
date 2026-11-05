"""Cross-representation algebra and soundness barriers."""
from dataclasses import replace
from itertools import product
import random
import unittest

import representation_transport as t
import representation_cosearch as c


class TransportTests(unittest.TestCase):
    def test_primitive_roots_are_exact_order(self):
        for n in t.LENGTHS:
            root = t.primitive_root(n)
            self.assertEqual(pow(root, n, t.PRIME), 1)
            self.assertNotEqual(pow(root, n // 2, t.PRIME), 1)

    def test_exhaustive_all_binary_four_vector_pairs(self):
        vectors = list(product((0, 1), repeat=4))
        checked = 0
        for left in vectors:
            for right in vectors:
                receipt = t.transport_convolution(left, right)
                self.assertTrue(t.replay_convolution(left, right, receipt))
                self.assertEqual(receipt.output, t.direct_cyclic_convolution(left, right))
                checked += 1
        self.assertEqual(checked, 256)

    def test_basis_vectors_and_randomized_large_lengths(self):
        rng = random.Random(20261105)
        for n in (2, 8, 16, 32):
            unit = [tuple(int(i == j) for i in range(n)) for j in range(n)]
            for a in unit:
                for b in unit:
                    receipt = t.transport_convolution(a, b)
                    self.assertTrue(t.replay_convolution(a, b, receipt))
            for _ in range(20):
                a = tuple(rng.randrange(t.PRIME) for i in range(n))
                b = tuple(rng.randrange(t.PRIME) for i in range(n))
                self.assertTrue(t.replay_convolution(a, b, t.transport_convolution(a, b)))

    def test_all_representations_are_invertible_on_test_vectors(self):
        rng = random.Random(77)
        for n in t.LENGTHS:
            for _ in range(40):
                data = tuple(rng.randrange(t.PRIME) for _ in range(n))
                fast, _ = t.fast_transform(data)
                self.assertEqual(fast, t.direct_transform(data))
                back, _ = t.fast_transform(fast, inverse=True)
                self.assertEqual(back, data)
                self.assertEqual(t.direct_transform(fast, inverse=True), data)

    def test_counted_data_multiplication_crossover_not_a_runtime_claim(self):
        values = t.demo()["convolution"]
        self.assertGreater(values["4"]["transformed_multiplies"],
                           values["4"]["direct_multiplies"])
        for n in (8, 16, 32):
            self.assertLess(values[str(n)]["transformed_multiplies"],
                            values[str(n)]["direct_multiplies"])
            self.assertTrue(values[str(n)]["direct_replay"])

    def test_receipt_mutations_and_stale_bindings_refuse(self):
        left = (1, 2, 3, 4)
        right = (3, 1, 4, 1)
        good = t.transport_convolution(left, right)
        self.assertTrue(t.replay_convolution(left, right, good))
        self.assertFalse(t.replay_convolution(right, right, good))
        self.assertFalse(t.replay_convolution(left, right, replace(good, output=(0,)*4)))
        self.assertFalse(t.replay_convolution(left, right, replace(good, spectral_product=(0,)*4)))
        self.assertFalse(t.replay_convolution(left, right, replace(good, relation="changed-axioms")))
        self.assertFalse(t.replay_convolution(left, right, replace(good, authority="qualified")))
        self.assertFalse(t.replay_convolution(left, right, replace(good, transformed_data_multiplies=-1)))

    def test_changes_in_mathematical_theory_are_not_silently_accepted(self):
        witness = t.check_equality_modulo(0, t.PRIME)
        self.assertEqual(witness.status, "refuted-by-witness")
        self.assertFalse(witness.source_truth)
        self.assertTrue(witness.target_truth)
        self.assertEqual(witness.witness, (0, 97))
        same = t.check_equality_modulo(1, 1)
        self.assertEqual(same.status, "instance-agrees-not-general-proof")
        axioms = t.changing_axioms_without_interpretation("Euclidean-geometry", "hyperbolic-geometry")
        self.assertEqual(axioms.status, "unproved-no-transport-map")

    def test_additive_vs_polynomial_syntax_gate(self):
        x, y = (c.Expr("var", (), i) for i in range(2))
        self.assertTrue(t.additive_language_admits(c.Expr("+", (x, y))))
        self.assertFalse(t.additive_language_admits(c.Expr("*", (x, y))))
        self.assertFalse(t.additive_language_admits(c.Expr("*", (x, x))))
        self.assertTrue(t.additive_language_admits(x))

    def test_invalid_vectors_and_unregistered_geometries_refuse(self):
        for value in ((0, 1, 2), (0, 97), (0, False), (0, 1.2)):
            with self.subTest(value=value), self.assertRaises(ValueError):
                t.fast_transform(value)
        with self.assertRaises(ValueError):
            t.direct_cyclic_convolution((0, 1), (0,)*4)
        with self.assertRaises(ValueError):
            t.changing_axioms_without_interpretation("wrong", "geometry")


if __name__ == "__main__":
    unittest.main()
