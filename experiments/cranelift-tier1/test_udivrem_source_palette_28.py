#!/usr/bin/env python3
"""Pure, reproducible vocabulary/refusal and exhaustive 8-bit controls."""
import unittest

from qualify_udivrem_source_palette_28 import (
    MODES, WATS, ROT, MASK, reference, vectors, disasm_cost,
)


class TestUdivRemPalette(unittest.TestCase):
    def test_closed_two_formulation_palette(self):
        self.assertEqual(MODES, ("direct", "quotient_reuse"))
        self.assertEqual(set(WATS), set(MODES))
        self.assertNotEqual(WATS[MODES[0]], WATS[MODES[1]])
        for mode in MODES:
            w = WATS[mode]
            self.assertTrue(w.startswith("(module\n"))
            self.assertEqual(w.count('(export "work")'), 1)
            self.assertEqual(w.count("i64.div_u"), 1)
            self.assertIn("i64.rotl", w)
            self.assertTrue(w.endswith(")\n"))
        self.assertEqual(WATS["direct"].count("i64.rem_u"), 1)
        self.assertEqual(WATS["quotient_reuse"].count("i64.rem_u"), 0)
        self.assertEqual(WATS["quotient_reuse"].count("i64.mul"), 1)
        self.assertEqual(WATS["quotient_reuse"].count("i64.sub"), 1)

    def test_exhaustive_unsigned_8bit_euclidean_identity(self):
        pairs = 0
        for a in range(256):
            for b in range(1, 256):
                q, r = divmod(a, b)
                reconstructed = (a - q * b) & 255
                self.assertEqual(r, reconstructed)
                pairs += 1
        self.assertEqual(pairs, 65280)

    def test_64bit_signed_display_oracle(self):
        self.assertEqual(reference(7, 3), (2 ^ (1 << ROT)))
        self.assertEqual(reference(0, 1), 0)
        self.assertEqual(reference(-1, -1), 1)
        self.assertTrue(all(y != 0 for x, y in vectors()))
        self.assertGreaterEqual(len(vectors()), 40)

    def test_zero_and_out_of_domain_refusals(self):
        with self.assertRaises(ZeroDivisionError):
            reference(1, 0)
        with self.assertRaises(ValueError):
            reference(1 << 63, 1)
        with self.assertRaises(ValueError):
            reference(0, -(1 << 63) - 1)

    def test_byte_parser_and_exact_function_cardinality(self):
        fake = ("\nwasm[0]::function[0]::work:\n"
                "            48 89 f8  movq %rdi, %rax\n"
                "            48 f7 f1  divq %rcx\n"
                "            c3  retq\n")
        record = disasm_cost(fake)
        self.assertEqual(record["through_ret_bytes"], 7)
        self.assertEqual(record["static_div_instructions"], 1)
        self.assertEqual(record["decoded_instruction_lines"], 3)
        self.assertTrue(record["not_dynamic"])
        with self.assertRaises(ValueError):
            disasm_cost(fake.replace("retq", "nop"))
        with self.assertRaises(ValueError):
            disasm_cost(fake.replace("function[0]", "function[1]"))
        with self.assertRaises(ValueError):
            disasm_cost(fake + "wasm[0]::function[1]::extra:\n")

if __name__ == "__main__":
    unittest.main()
