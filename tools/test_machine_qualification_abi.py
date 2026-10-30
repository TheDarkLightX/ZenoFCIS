"""Controls for the calling-convention gate: obligations A-01..A-07, the native harness and its planted faults."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import z3

from machine_code import verify2
from machine_qualification import abi

KERNELS = Path(__file__).resolve().parents[1] / "test-data/machine-code/kernels"
LEA_SUM = bytes.fromhex("488d0437")  # lea rax, [rdi+rsi]


def entry(name):
    data = json.loads((KERNELS / name).read_text())
    return data["problem"], bytes.fromhex(data["code_hex"])


class Obligations(unittest.TestCase):
    def test_every_library_kernel_meets_a01_to_a07(self):
        for path in sorted(KERNELS.glob("*.json")):
            problem, code = entry(path.name)
            with self.subTest(kernel=path.name):
                result = abi.obligations(problem, code)
                self.assertEqual(result["status"], "qualified", result["obligations"])
                self.assertEqual(sorted(result["obligations"]), [f"A-0{k}" for k in range(1, 8)])

    def test_a_far_or_popping_return_fails_a01(self):
        problem, code = entry("add64-x86.json")
        result = abi.obligations(problem, code[:-1] + bytes.fromhex("c20800"))  # ret 8
        self.assertEqual((result["status"], result["obligations"]["A-01"]["status"]), ("refused", "failed"))

    def test_a_premise_narrowing_the_inputs_fails_a06(self):
        # Planted: the checker's query gains the premise x = 0, so an unsat answer would cover only part of the domain.
        problem, code = entry("umin-x86.json")
        original = verify2.check

        def narrowed(problem, code, timeout_ms=1000):
            real = z3.Solver.check

            def check(self, *args):
                self.add(z3.BitVec("mc_input_x", 64, ctx=self.ctx) == 0)
                return real(self, *args)
            with patch.object(z3.Solver, "check", check):
                return original(problem, code, timeout_ms)
        with patch.object(verify2, "check", narrowed):
            result = abi.obligations(problem, code)
        self.assertEqual(result["obligations"]["A-06"]["status"], "failed")

    def test_a_write_outside_the_caller_saved_set_fails_a03(self):
        problem, code = entry("umin-x86.json")
        with patch.dict(abi.CALLER_SAVED, {"x86": abi.CALLER_SAVED["x86"] - {"rax"}}):
            result = abi.obligations(problem, code)
        self.assertEqual(result["obligations"]["A-03"]["outside_caller_saved"], ["rax"])


@unittest.skipIf(abi._gcc() is None, "needs gcc")
class NativeHarness(unittest.TestCase):
    def run_bytes(self, code, **options):
        problem, _ = entry("add64-x86.json")
        with tempfile.TemporaryDirectory() as folder:
            return abi.native(problem, code, Path(folder) / "work", vectors=64, **options)

    def test_a_conforming_kernel_and_its_endbr64_prefixed_callable(self):
        self.assertEqual(self.run_bytes(LEA_SUM + b"\xc3")["status"], "conforms")
        self.assertEqual(self.run_bytes(LEA_SUM + b"\xc3", prefix=abi.ENDBR64)["status"], "conforms")

    def test_each_planted_fault_is_reported_as_exactly_that_fault(self):
        # Fixtures outside every palette; each returns normally to the trampoline.
        fixtures = {
            "callee-saved": bytes.fromhex("4889fb") + LEA_SUM + b"\xc3",           # mov rbx, rdi
            "stack-memory": bytes.fromhex("5759") + LEA_SUM + b"\xc3",              # push rdi; pop rcx
            "direction-flag": bytes.fromhex("fd") + LEA_SUM + b"\xc3",              # std
            "stack-pointer": bytes.fromhex("594883c408") + LEA_SUM + bytes.fromhex("ffe1"),  # pop rcx; add rsp,8; jmp rcx
            "result": bytes.fromhex("4889f0c3"),                                    # mov rax, rsi
        }
        for fault, code in fixtures.items():
            with self.subTest(fault=fault):
                result = self.run_bytes(code)
                self.assertEqual(result["status"], "violated", result)
                self.assertGreater(result["faults"][fault], 0, result)
                others = {k: v for k, v in result["faults"].items() if k != fault and k != "stack-memory"}
                if fault != "stack-pointer":
                    self.assertFalse(any(others.values()), result)

    def test_the_cet_profile_names_its_unsupported_active_modes(self):
        problem, code = entry("umin-x86.json")
        with tempfile.TemporaryDirectory() as folder:
            result = abi.cet(problem, code, Path(folder) / "work")
        self.assertEqual((result["status"], result["active_modes"]["status"]), ("inactive-mode-only", "unsupported"))
        self.assertEqual(result["callable_bytes"], 4 + len(code))


if __name__ == "__main__":
    unittest.main()
