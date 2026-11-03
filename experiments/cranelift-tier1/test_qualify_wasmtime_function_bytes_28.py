#!/usr/bin/env python3
"""Positive and refusal controls for Wasmtime AOT per-function cost evidence."""
import json
import tempfile
import unittest
from pathlib import Path
from qualify_wasmtime_function_bytes_28 import (
    HEADER, NEXT, SRC_SCHEMA, UPSTREAM, digest, parse_function, qualify,
)
BASE = (HEADER + "\n            55                           pushq   %rbp\n"
        "            49 b9 ff ff ff ff ff ff ff   movabsq $123, %r9\n"
        "            7f\n"
        "            c3                           retq\n"
        "            00 00                        addb    %al, (%rax)\n\n"
        + NEXT + "\n            c3                           retq\n")
PATCH = (HEADER + "\n            55                           pushq   %rbp\n"
         "            c3                           retq\n\n"
         + NEXT + "\n            c3                           retq\n")

class Cases(unittest.TestCase):
    def test_wrapped_instr_and_postret_count(self):
        r = parse_function(BASE)
        self.assertEqual(r["x86_bytes_through_first_ret"], 12)
        self.assertEqual(r["x86_bytes_after_first_ret"], 2)
        self.assertEqual(r["x86_bytes_in_disassembler_function_region"], 14)

    def test_ambiguous_or_unsupported_disassembly_rejected(self):
        bad = (
            BASE.replace(NEXT, HEADER + "\n" + NEXT),
            BASE.replace("retq", "nop", 1),
            BASE.replace("00 00", "gg gg"),
            BASE.replace("pushq   %rbp", "pushq %rbp\nunaccounted extra text"),
            BASE.replace("            00 00", "            c3                           retq\n            00 00"),
            BASE.replace(NEXT, "wasm[0]::function[3]:"),
        )
        for x in bad:
            with self.subTest(x=x[-70:]), self.assertRaises(ValueError):
                parse_function(x)

    def _fixture(self, folder):
        compiled = {}
        for name, src in (("baseline", BASE), ("patched", PATCH)):
            p = folder / name
            p.mkdir()
            data = bytes([0, 1, 2]) if name == "baseline" else bytes([4, 5, 6])
            (p / "disassembly.txt").write_text(src)
            (p / "satadd.cwasm").write_bytes(data)
            compiled[name] = {"disassembly_sha256": digest(src.encode()),
                              "aot_sha256": digest(data), "aot_bytes": len(data),
                              "elf_text_bytes": 4096}
        report = {"schema": SRC_SCHEMA,
                  "status": "finite-wasm-aot-check-passed-runtime-unqualified",
                  "source": {"upstream_commit": UPSTREAM, "host_machine": "x86_64"},
                  "compiled": compiled,
                  "finite_checks": {"baseline": {"passed": True}, "patched": {"passed": True}},
                  "aot_text_delta_baseline_minus_patched": 0,
                  "whole_cwasm_delta_baseline_minus_patched": 0}
        (folder / "qualification.json").write_text(json.dumps(report))
        return report

    def test_complete_qualification_positive(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            self._fixture(root)
            report = qualify(root)
            self.assertEqual(report["diff"]["x86_executable_prefix_byte_delta"], 10)
            self.assertEqual(report["diff"]["aot_elf_text_section_byte_delta"], 0)
            self.assertEqual(report["diff"]["whole_cwasm_file_byte_delta"], 0)

    def test_hash_provenance_and_report_tamper_rejected(self):
        for what in ("dis", "object", "upstream", "baseline", "finite", "delta"):
            with tempfile.TemporaryDirectory() as d:
                root = Path(d)
                report = self._fixture(root)
                if what == "dis":
                    (root / "baseline" / "disassembly.txt").write_text(BASE + "\n")
                elif what == "object":
                    (root / "patched" / "satadd.cwasm").write_bytes(b"mutant")
                elif what == "upstream":
                    report["source"]["upstream_commit"] = "f" * 40
                elif what == "baseline":
                    report["compiled"].pop("baseline")
                elif what == "finite":
                    report["finite_checks"]["patched"]["passed"] = False
                else:
                    report["aot_text_delta_baseline_minus_patched"] = 5
                if what not in ("dis", "object"):
                    (root / "qualification.json").write_text(json.dumps(report))
                with self.subTest(what=what), self.assertRaises(ValueError):
                    qualify(root)

if __name__ == "__main__":
    unittest.main()
