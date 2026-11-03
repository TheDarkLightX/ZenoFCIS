#!/usr/bin/env python3
"""Deliberate nonqualifying and positive parser cases for AOT symbol audit."""
import unittest
from qualify_wasmtime_symbols_28 import SAT, WORKLOAD, parse_symbols, code_prefix
GOOD = ("signatures[0]::wasm_to_array_trampoline t 262 71\n"
        "signatures[1]::wasm_to_array_trampoline t 2d3 6b\n"
        "wasm[0]::array_to_wasm_trampoline[0] t e0 c4\n"
        "wasm[0]::array_to_wasm_trampoline[1] t 1a4 be\n"
        "wasm[0]::function[0]::sat t 0 40\n"
        "wasm[0]::function[1] t 40 a0\n")
class AuditTests(unittest.TestCase):
    def test_complete_symbol_set(self):
        x = parse_symbols(GOOD)
        self.assertEqual(x[SAT]["bytes"], 64)
        self.assertEqual(x[WORKLOAD]["bytes"], 160)
        self.assertEqual(len(x), 6)
    def test_missing_duplicate_zero_refusal(self):
        for bad in (GOOD.replace("wasm[0]::function[0]::sat t 0 40\n", ""),
                    GOOD + "wasm[0]::function[0]::sat t 0 40\n",
                    GOOD.replace("wasm[0]::function[0]::sat t 0 40",
                                 "wasm[0]::function[0]::sat t 0 0")):
            with self.subTest(bad=len(bad)), self.assertRaises(ValueError):
                parse_symbols(bad)
    def test_ret_boundary(self):
        dis = ("0000000000000000 <wasm[0]::function[0]::sat>:\n"
               " 0: 55 push rbp\n 1: 48 89 e5 mov rbp,rsp\n 4: c3 ret\n"
               "0000000000000010 <wasm[0]::function[1]>:\n 10: c3 ret\n")
        self.assertEqual(code_prefix(dis, SAT), 5)
        with self.assertRaises(ValueError):
            code_prefix(dis.replace("4: c3 ret", "4: 90 nop"), SAT)
if __name__ == "__main__":
    unittest.main()
