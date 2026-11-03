#!/usr/bin/env python3
"""Fail-closed parser and cross-target matrix regression tests."""
import unittest
from qualify_cross_target_28 import (
    parse_header, parse_symbol, parse_text_size, summarize, expected_commands, digest, CLIF_SHA, HERE,
)

def row(target, mode, a=56, b=39):
    return {"target": target, "opt_level": mode,
            "baseline": {"target": target, "function": {"bytes": a}},
            "patched": {"target": target, "function": {"bytes": b}},
            "symbol_delta_baseline_minus_patched": a-b}

class CrossTargetTests(unittest.TestCase):
    def test_exact_clif_source_pin(self):
        self.assertEqual(digest((HERE / 'satadd.clif').read_bytes()), CLIF_SHA)

    def test_elf_machine(self):
        parse_header(" Class: ELF64\n Machine: AArch64\n", "aarch64")
        with self.assertRaisesRegex(ValueError, "architecture"):
            parse_header(" Class: ELF64\n Machine: AArch64\n", "x86_64")
        with self.assertRaisesRegex(ValueError, "ELF64"):
            parse_header(" Class: ELF32\n Machine: AArch64\n", "aarch64")

    def test_exact_function(self):
        self.assertEqual(parse_symbol("%tier1_signed_satadd T 0 38\n")["bytes"], 56)
        for bad in ("%tier1_signed_satadd T 0 0", "other T 0 38",
                    "%tier1_signed_satadd T 0 38\nother T 40 1"):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                parse_symbol(bad)

    def test_section(self):
        self.assertEqual(parse_text_size(" [ 1] .text PROGBITS 0000 000040 000038 00 AX 0 0 8"), 56)
        with self.assertRaises(ValueError):
            parse_text_size(" [ 1] .data PROGBITS 0000 000040 000038 00 WA 0 0 8")

    def test_regressions_preserved(self):
        rows = [row(t, m) for t in ("x86_64", "aarch64")
                for m in ("speed", "speed_and_size")]
        rows[2] = row("aarch64", "speed", 30, 42)
        x = summarize(rows)
        self.assertEqual(x["regressions"], [{"target": "aarch64", "mode": "speed", "delta": -12}])
        self.assertEqual(len(x["wins"]), 3)

    def test_missing_duplicate_forged_and_wrong_target_refused(self):
        rows = [row(t, m) for t in ("x86_64", "aarch64")
                for m in ("speed", "speed_and_size")]
        wrong = dict(rows[1]); wrong["baseline"] = {"target": "aarch64", "function": {"bytes": 56}}
        forged = dict(rows[1]); forged["symbol_delta_baseline_minus_patched"] = 99
        for bad in (rows[:-1], rows+[rows[0]], [rows[0], wrong, *rows[2:]],
                    [rows[0], forged, *rows[2:]]):
            with self.assertRaises(ValueError):
                summarize(bad)
        with self.assertRaisesRegex(ValueError, "positive-control"):
            summarize([row("x86_64", "speed"), row("x86_64", "speed_and_size", 55, 39),
                       row("aarch64", "speed"), row("aarch64", "speed_and_size")])

    def test_compile_manifest_shape(self):
        cmds = expected_commands()
        self.assertEqual(len(cmds), 8)
        self.assertEqual(len(set(cmds)), 8)
        self.assertTrue(cmds[0].startswith("phase=baseline target=x86_64"))
        self.assertTrue(cmds[-1].startswith("phase=patched target=aarch64"))

if __name__ == "__main__":
    unittest.main()
