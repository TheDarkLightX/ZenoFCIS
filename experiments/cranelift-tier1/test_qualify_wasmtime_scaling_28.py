#!/usr/bin/env python3
"""Source layout, count and disassembly refusal tests for scaling fixture."""
import unittest
from qualify_wasmtime_scaling_28 import COUNTS, render_wat, function_headers, all_function_byte_costs, SOURCE_WAT
from independent_recheck_scaling_28 import independently_count_function_bytes


class Tests(unittest.TestCase):
    def test_renders_exact_number_of_independent_exports(self):
        for n in COUNTS:
            with self.subTest(copies=n):
                s = render_wat(n)
                self.assertTrue(s.startswith("(module\n"))
                self.assertTrue(s.endswith(")\n"))
                self.assertEqual(s.count('(func $sat'), n)
                self.assertEqual(s.count('(export "sat'), n)
                self.assertNotIn('export "workload"', s)
                self.assertIn(f'(export "sat{n-1}")', s)
                self.assertEqual(s, render_wat(n))

    def test_invalid_sizes_refused(self):
        for n in (0, -1, 2, 31, 512):
            with self.subTest(copies=n), self.assertRaises(ValueError):
                render_wat(n)

    def test_disassembly_export_count_and_name_checks(self):
        good = ("wasm[0]::function[0]::sat0:\n    c3 retq\n"
                "wasm[0]::function[1]::sat1:\n    c3 retq\n")
        function_headers(good, 2)
        for bad in (good.replace("sat1", "foo"), good.replace("[1]", "[0]"),
                    good.replace("function[1]", "function[2]"),
                    good + good, good.replace("function[1]::sat1:", "function[1]:")):
            with self.subTest(bad=bad[-30:]), self.assertRaises(ValueError):
                function_headers(bad, 2)

    def test_independent_byte_parsers_and_post_return_padding(self):
        source = ("wasm[0]::function[0]::sat0:\n"
                  "            48 89 c0                   movq    %rax, %rax\n"
                  "            c3                         retq\n"
                  "            90                         nop\n"
                  "wasm[0]::function[1]::sat1:\n"
                  "            31 c0                      xorl    %eax, %eax\n"
                  "            c3                         retq\n")
        a = all_function_byte_costs(source, 2)
        b = independently_count_function_bytes(source, 2)
        self.assertEqual(a, b)
        self.assertEqual(a["bytes_through_first_return"], 7)
        self.assertEqual(a["symbol_region_bytes"], 8)
        self.assertEqual(a["post_return_region_bytes"], 1)
        for corrupt in (source.replace("retq", "nop", 1),
                        source.replace("48 89", "xx 89"),
                        source.replace("function[1]::sat1:", "function[1]::sat9:")):
            with self.subTest(corrupt=corrupt[-30:]), self.assertRaises(ValueError):
                all_function_byte_costs(corrupt, 2)
            with self.subTest(corrupt=corrupt[-30:]), self.assertRaises(ValueError):
                independently_count_function_bytes(corrupt, 2)

    def test_source_fixture_exists(self):
        self.assertTrue(SOURCE_WAT.is_file())

if __name__ == "__main__":
    unittest.main()
