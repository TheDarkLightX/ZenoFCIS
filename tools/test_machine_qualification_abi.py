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
MEMORY = Path(__file__).resolve().parents[1] / "test-data/machine-code/memory-kernels"
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

    def test_a_premise_folded_into_the_single_assertion_fails_a06(self):
        # Review 3: the narrowing as one assertion, And(x = 0, mismatch); the wrong kernel then looks equivalent.
        problem, _ = entry("add64-x86.json")
        problem = {"schema": "zeno-fcis/machine-problem/2", "target": "x86_64-sysv-register/2",
                   "inputs": ["x", "y"], "outputs": [["add", ["input", "x"], ["input", "y"]]],
                   "grammar": [["mov", "rax", "rsi"]], "max_instructions": 1, "objective": "bytes"}
        code = bytes.fromhex("4889f0c3")  # mov rax, rsi: wrong whenever x != 0
        original, real_add = verify2.check, z3.Solver.add

        def folded(self, *formulas):
            return real_add(self, z3.And(z3.BitVec("mc_input_x", 64, ctx=self.ctx) == 0, *formulas))

        def narrowed(problem, code, timeout_ms=1000):  # only the checker's own query is narrowed
            with patch.object(z3.Solver, "add", folded):
                return original(problem, code, timeout_ms)
        with patch.object(verify2, "check", narrowed):
            result = abi.obligations(problem, code)
        self.assertEqual(result["obligations"]["A-02"]["verdict"], "equivalent", "the planted narrowing hides the fault")
        self.assertEqual((result["obligations"]["A-06"]["status"], result["obligations"]["A-06"]["query"]),
                         ("failed", "differs"))

    def test_a_write_outside_the_caller_saved_set_fails_a03(self):
        problem, code = entry("umin-x86.json")
        with patch.dict(abi.CALLER_SAVED, {"x86": abi.CALLER_SAVED["x86"] - {"rax"}}):
            result = abi.obligations(problem, code)
        self.assertEqual(result["obligations"]["A-03"]["outside_caller_saved"], ["rax"])


@unittest.skipIf(abi._gcc() is None, "needs gcc")
class NativeHarness(unittest.TestCase):
    def run_bytes(self, code, **options):
        problem, _ = entry("add64-x86.json")
        options.setdefault("vectors", 64)
        with tempfile.TemporaryDirectory() as folder:
            return abi.native(problem, code, Path(folder) / "work", **options)

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

    def test_review_three_harness_faults_are_reported(self):
        # The kernel enters with RSP + 8 a multiple of 16 (System V): mov rax, rsp; and rax, 15 returns 8.
        aligned = self.run_bytes(bytes.fromhex("4889e04883e00fc3"), expected=lambda vector: [8])
        self.assertEqual(aligned["status"], "conforms", aligned)
        # add qword [rsp], 7 before ret: the kernel returns past the true continuation.
        skipped = self.run_bytes(bytes.fromhex("48830424074889f0c3"))
        self.assertIn(skipped["status"], ("violated", "crashed"), skipped)
        if skipped["status"] == "violated":
            self.assertGreater(skipped["faults"]["continuation"], 0, skipped)
        # Rounding control toggled in MXCSR (red-zone word saved and restored), result still correct.
        mxcsr = bytes.fromhex("4c8b5424f80fae5c24f8817424f8002000000fae5424f84c895424f8") + LEA_SUM + b"\xc3"
        changed = self.run_bytes(mxcsr)
        self.assertEqual(changed["status"], "violated", changed)
        self.assertGreater(changed["faults"]["control-state"], 0, changed)
        with self.assertRaises(ValueError):
            self.run_bytes(LEA_SUM + b"\xc3", vectors=0)

    def test_the_cet_profile_names_its_unsupported_active_modes(self):
        problem, code = entry("umin-x86.json")
        with tempfile.TemporaryDirectory() as folder:
            result = abi.cet(problem, code, Path(folder) / "work")
        self.assertEqual((result["status"], result["active_modes"]["status"]), ("inactive-mode-only", "unsupported"))
        self.assertEqual(result["callable_bytes"], 4 + len(code))


def memory_entry(name):
    data = json.loads((MEMORY / (name + ".json")).read_text())
    return data["problem"], bytes.fromhex(data["code_hex"])


class MemoryObligations(unittest.TestCase):
    def test_every_memory_kernel_meets_a01_to_a07_as_a_region_leaf(self):
        for path in sorted(MEMORY.glob("*.json")):
            problem, code = memory_entry(path.stem)
            with self.subTest(kernel=path.stem):
                result = abi.obligations(problem, code)
                self.assertEqual(result["status"], "qualified", result["obligations"])
                self.assertTrue(result["profile"].endswith("-u64-region-leaf/1"))
                frame = result["obligations"]["A-03"]
                self.assertEqual((frame["admission"], frame["outside_read_write_words"]), ("passed", []))
                self.assertTrue(result["obligations"]["A-06"]["regions_disjoint_aligned_away_from_stack"])
        problem, code = memory_entry("copy2-sum-x86")
        self.assertEqual(abi.obligations(problem, code)["obligations"]["A-03"]["memory_written"], ["dst[0]", "dst[1]"])

    def test_a_store_into_a_read_only_region_fails_a03_and_a05(self):
        problem, code = memory_entry("copy2-sum-x86")
        result = abi.obligations(problem, code[:-1] + bytes.fromhex("48890f") + b"\xc3")  # mov [rdi], rcx
        frame = result["obligations"]["A-03"]
        self.assertEqual((frame["status"], frame["admission"], frame["outside_read_write_words"]),
                         ("failed", "StoreToReadOnlyRegion", ["src[0]"]))
        self.assertEqual(result["obligations"]["A-05"]["status"], "failed")
        self.assertEqual(result["status"], "not-qualified")

    def test_overlapping_regions_fail_a06_and_a_region_on_the_return_slot_fails_a04(self):
        problem, code = memory_entry("copy2-sum-x86")
        with patch.object(abi, "REGION_STRIDE", 8):
            result = abi.obligations(problem, code)
        self.assertEqual(result["obligations"]["A-06"]["status"], "failed")
        self.assertFalse(result["obligations"]["A-06"]["regions_disjoint_aligned_away_from_stack"])
        problem, code = memory_entry("swap-words-x86")
        with patch.object(abi, "REGION_BASE", abi.LAYOUT["x86"]["S"]):
            result = abi.obligations(problem, code)
        self.assertEqual((result["obligations"]["A-04"]["status"], result["obligations"]["A-06"]["status"]),
                         ("failed", "failed"))


@unittest.skipIf(abi._gcc() is None, "needs gcc")
class MemoryNativeHarness(unittest.TestCase):
    def run_entry(self, name, code=None, **options):
        problem, stored = memory_entry(name)
        with tempfile.TemporaryDirectory() as folder:
            return abi.native(problem, stored if code is None else code, Path(folder) / "work", vectors=128, **options)

    def test_the_three_x86_memory_kernels_conform(self):
        for name in ("swap-words-x86", "load-add-store-x86", "copy2-sum-x86"):
            with self.subTest(kernel=name):
                result = self.run_entry(name)
                self.assertEqual(result["status"], "conforms", result)
                self.assertEqual(result["guard_words"], 8)
        self.assertEqual(self.run_entry("swap-words-rv64i")["status"], "not-run")

    def test_each_planted_memory_fault_is_reported_as_exactly_that_fault(self):
        # Fixtures outside every palette; each returns normally to the trampoline.
        _, swap = memory_entry("swap-words-x86")
        _, copy = memory_entry("copy2-sum-x86")
        fixtures = {
            "out-of-region": ("swap-words-x86", bytes.fromhex("48894710") + swap),    # mov [rdi+16], rax: one past p
            "read-only-region": ("copy2-sum-x86", copy[:-1] + bytes.fromhex("48890f") + b"\xc3"),  # mov [rdi], rcx
            "callee-saved": ("swap-words-x86", bytes.fromhex("4889fb") + swap),       # mov rbx, rdi
        }
        for fault, (name, code) in fixtures.items():
            with self.subTest(fault=fault):
                result = self.run_entry(name, code)
                self.assertEqual(result["status"], "violated", result)
                self.assertGreater(result["faults"][fault], 0, result)
                self.assertFalse(any(v for k, v in result["faults"].items() if k != fault), result)

    def test_a_dropped_store_is_a_region_word_fault(self):
        _, swap = memory_entry("swap-words-x86")
        dropped = swap[:-5] + b"\xc3"  # the final mov [rdi+8], rax removed
        result = self.run_entry("swap-words-x86", dropped)
        self.assertGreater(result["faults"]["region-words"], 0, result)
        self.assertFalse(any(v for k, v in result["faults"].items() if k != "region-words"), result)


if __name__ == "__main__":
    unittest.main()
