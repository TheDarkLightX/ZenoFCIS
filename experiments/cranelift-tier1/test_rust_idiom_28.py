#!/usr/bin/env python3
"""Two independent oracle implementations and contract refusal tests."""
from __future__ import annotations
import random
import unittest
from qualify_rust_idiom_wasm_28 import PROFILES, SOURCE, TARGET, value
from independent_recheck_rust_idiom_28 import independent_result

LOW, HIGH = -(1 << 63), (1 << 63) - 1


class RustWasmIdioms(unittest.TestCase):
    def test_profiles_and_exact_rust_surface(self):
        self.assertEqual(PROFILES, {"debug": "0", "optimized": "3"})
        self.assertEqual(TARGET, "x86_64-unknown-linux")
        source = SOURCE.read_text()
        self.assertEqual(source.count("pub extern \"C\" fn "), 5)
        self.assertEqual(source.count(".saturating_add(y)"), 3)
        for name in ("sat_add", "sat_add_xor", "sat_add_mix", "sign_clamp", "wrap_add"):
            self.assertIn("fn " + name + "(", source)

    def test_independent_references_agree_and_stay_in_bounds(self):
        rng = random.Random(0xFF21)
        xs = (LOW, LOW + 1, -1, 0, 1, HIGH - 1, HIGH)
        pairs = [(x, y) for x in xs for y in xs]
        pairs += [(rng.randrange(LOW, HIGH + 1),
                   rng.randrange(LOW, HIGH + 1)) for _ in range(300)]
        for x, y in pairs:
            for name in ("sat_add", "sat_add_xor", "sat_add_mix", "wrap_add"):
                with self.subTest(name=name, x=x, y=y):
                    one, two = value(name, x, y), independent_result(name, x, y)
                    self.assertEqual(one, two)
                    self.assertTrue(LOW <= one <= HIGH)
            self.assertEqual(value("sign_clamp", x),
                             independent_result("sign_clamp", x))

    def test_known_overflow_and_bitwise_semantics(self):
        self.assertEqual(value("sat_add", HIGH, 1), HIGH)
        self.assertEqual(value("sat_add", LOW, -1), LOW)
        self.assertEqual(value("wrap_add", HIGH, 1), LOW)
        self.assertEqual(value("sat_add_xor", LOW, -1), LOW + 1)
        self.assertEqual(value("sat_add_mix", 0, 0), 0)
        self.assertEqual(value("sign_clamp", -1), LOW)
        self.assertEqual(value("sign_clamp", 0), HIGH)

    def test_refuse_invalid_inputs_and_unknown_operations(self):
        for name in ("sat_add", "sat_add_xor", "sat_add_mix", "wrap_add"):
            for x, y in ((LOW - 1, 0), (HIGH + 1, 0),
                         (0, LOW - 1), (0, HIGH + 1)):
                with self.subTest(name=name, x=x, y=y):
                    with self.assertRaises(ValueError):
                        value(name, x, y)
                    with self.assertRaises(ValueError):
                        independent_result(name, x, y)
        with self.assertRaises(ValueError):
            value("sat_add", 0)
        with self.assertRaises(ValueError):
            independent_result("bogus", 0, 0)
        with self.assertRaises(ValueError):
            value("sign_clamp", LOW - 1)


if __name__ == "__main__":
    unittest.main()
