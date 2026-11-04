#!/usr/bin/env python3
"""Parser and refusal controls for the archival ELF symbol-gap explanation."""
import unittest

from qualify_aot_symbol_gaps_28 import (
    admit_symbols, next_symbol, parse_symbols, parse_text_section,
)

SECTION = "  [ 1] .text PROGBITS 0000000000000000 000040 000080 00  AX  0   0 4096\n"
SYMBOLS = """
Symbol table '.symtab' contains 3 entries:
   Num:    Value          Size Type    Bind   Vis      Ndx Name
     0: 0000000000000000     0 NOTYPE  LOCAL  DEFAULT  UND
     1: 0000000000000000    64 FUNC    GLOBAL DEFAULT    1 wasm[0]::function[0]::sat0
     2: 0000000000000040    52 FUNC    GLOBAL DEFAULT    1 wasm[0]::function[1]::sat1
"""


class Cases(unittest.TestCase):
    def test_realistic_section_and_symbol_parser(self):
        text = parse_text_section(SECTION)
        self.assertEqual((text["section_index"], text["size"], text["alignment"]), (1, 128, 4096))
        syms = parse_symbols(SYMBOLS, text)
        self.assertEqual(len(syms), 2)
        selected = admit_symbols(syms, 2)
        self.assertEqual(sorted(selected), [0, 1])
        self.assertEqual(next_symbol(syms, syms[0])["offset"], 64)
        self.assertIsNone(next_symbol(syms, syms[1]))

    def test_padding_replaces_shrinking_body_without_shifting_next(self):
        common = {"name": "wasm[0]::function[1]::sat1", "offset": 64, "size": 20}
        base = {"name": "wasm[0]::function[0]::sat0", "offset": 0, "size": 64}
        patched = {"name": base["name"], "offset": 0, "size": 52}
        first_gap = next_symbol([base, common], base)["offset"] - base["offset"] - base["size"]
        second_gap = next_symbol([patched, common], patched)["offset"] - patched["offset"] - patched["size"]
        self.assertEqual((first_gap, second_gap), (0, 12))
        self.assertEqual(second_gap - first_gap, base["size"] - patched["size"])

    def test_ambiguous_sections_and_out_of_bounds_rejected(self):
        for variant in (SECTION + SECTION, SECTION.replace("4096", "3000"), "", SECTION.replace("000080", "000000")):
            with self.subTest(raw=variant), self.assertRaises(ValueError):
                parse_text_section(variant)
        text = parse_text_section(SECTION)
        with self.assertRaises(ValueError):
            parse_symbols(SYMBOLS.replace("0000000000000040", "0000000000000090"), text)

    def test_missing_names_must_be_inconclusive(self):
        text = parse_text_section(SECTION)
        syms = parse_symbols(SYMBOLS.replace("wasm[0]::function[1]::sat1", "opaque"), text)
        self.assertEqual(admit_symbols(syms, 2), {})
        with self.assertRaises(ValueError):
            admit_symbols(parse_symbols(SYMBOLS.replace("function[1]", "function[0]"), text), 2)

    def test_duplicate_names_conflict_is_rejected(self):
        text = parse_text_section(SECTION)
        altered = SYMBOLS + "     3: 0000000000000060    20 FUNC GLOBAL DEFAULT 1 wasm[0]::function[0]::sat0\n"
        with self.assertRaises(ValueError):
            parse_symbols(altered, text)

if __name__ == "__main__":
    unittest.main()
