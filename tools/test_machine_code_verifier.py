"""Independent literal-byte ABI/decoder/SMT controls, never native execution.

Gold bytes are chosen here without importing the encoder/search/session. Solver
unknown and absence controls are deterministic mocks, not timeout benchmarks.
"""
from pathlib import Path
import copy
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
from machine_code import verify as v

X, R, MASK = v.X86, v.RV64, (1 << 64) - 1
RET_R = "67800000"


def code(text):
    return bytes.fromhex(text)


def problem(target, expression, grammar, bound=16):
    return {"schema": "zeno-fcis/machine-problem/1", "target": target,
            "inputs": ["x", "y"], "expression": expression, "grammar": grammar,
            "max_instructions": bound, "objective": "bytes"}


class VerifierTests(unittest.TestCase):
    def equivalent(self, target, expression, grammar, raw, bound=16):
        p = problem(target, expression, grammar, bound)
        before = copy.deepcopy(p)
        answer = v.check(p, code(raw), timeout_ms=1000)
        self.assertEqual(answer["status"], "equivalent", answer)
        self.assertEqual(p, before)
        self.assertIn("z3_version", answer)
        self.assertEqual(answer["cost"]["bytes"], len(code(raw)))
        self.assertEqual(answer["cost"]["instructions"], len(v.decode(target, code(raw))) - 1)
        return answer

    def counterexample(self, target, expression, grammar, raw, bound=16):
        answer = v.check(problem(target, expression, grammar, bound), code(raw))
        self.assertEqual(answer["status"], "counterexample", answer)
        values = {k: int(x) for k, x in answer["inputs"].items()}
        initial = {k: int(x) for k, x in answer["initial_registers"].items()}
        self.assertEqual(v.run_concrete(target, code(raw), values, initial), int(answer["actual"]))
        self.assertNotEqual(answer["expected"], answer["actual"])
        return answer

    def refused_bytes(self, target, cases):
        for raw in cases:
            with self.subTest(raw=raw):
                with self.assertRaises(ValueError):
                    v.decode(target, code(raw))

    def test_x86_add_sub_bitwise_and_full_width_imul(self):
        for name, opcode, ast in [("add", "01", "add"), ("sub", "29", "sub"),
                                  ("and", "21", "and"), ("or", "09", "or"),
                                  ("xor", "31", "xor"), ("imul", "0faf", "mul")]:
            with self.subTest(name=name):
                self.equivalent(X, [ast, ["input", "x"], ["input", "y"]],
                                [["mov", "rax", "rdi"], [name, "rax", "rsi"]],
                                "4889f848" + opcode + "c6c3" if name == "imul" else "4889f848" + opcode + "f0c3")
        self.assertEqual(v.run_concrete(X, code("4889f8480fafc6c3"), {"x": MASK, "y": 2}), MASK - 1)

    def test_x86_literal_lea_reads_inputs_and_scale(self):
        answer = self.equivalent(X, ["add", ["input", "x"], ["shl", ["input", "y"], 1]],
                                 [["lea", "rax", "rdi", "rsi", 2]], "488d0477c3")
        self.assertEqual(answer["cost"], {"bytes": 5, "instructions": 1})
        self.assertEqual(v.run_concrete(X, code("488d0477c3"), {"x": MASK, "y": MASK}), MASK - 2)

    def test_x86_rex_source_destination_and_sib_extension_bits(self):
        self.equivalent(X, ["input", "x"], [["mov", "r8", "rdi"], ["mov", "rax", "r8"]], "4989f84c89c0c3")
        raw = code("4f8d04484c89c0c3")
        self.assertEqual(v.decode(X, raw)[0].form(), ["lea", "r8", "r8", "r9", 2])
        self.assertEqual(v.run_concrete(X, raw, {"x": 0, "y": 0}, {"r8": 3, "r9": 4}), 11)
        self.assertEqual(v.run_concrete(X, code("4a8d04d7c3"), {"x": 2, "y": 0}, {"r10": 3}), 26)

    def test_x86_shifts_are_64_bit_logical_and_counts_zero_one_63(self):
        for op, rawop, amount in [("shl", "48c1e000", 0), ("shl", "48d1e0", 1),
                                   ("shl", "48c1e03f", 63), ("lshr", "48d1e8", 1),
                                   ("lshr", "48c1e83f", 63)]:
            form = "shl" if op == "shl" else "shr"
            self.equivalent(X, [op, ["input", "x"], amount], [["mov", "rax", "rdi"], [form, "rax", amount]], "4889f8" + rawop + "c3")
        self.assertEqual(v.run_concrete(X, code("4889f848d1e8c3"), {"x": 1 << 63, "y": 0}), 1 << 62)

    def test_x86_ret_byte_inside_immediate_is_not_a_return(self):
        raw = "48b8c300000000000000c3"
        self.equivalent(X, ["const", "195"], [["imm", "rax", "195"]], raw)
        d = v.decode(X, code(raw))
        self.assertEqual([(i.offset, i.size) for i in d], [(0, 10), (10, 1)])
        self.equivalent(X, ["const", str(MASK)], [["imm", "rax", str(MASK)]], "48b8ffffffffffffffffc3")

    def test_x86_entry_output_is_unconstrained_not_zero(self):
        witness = self.counterexample(X, ["const", "0"], [], "c3", 0)
        self.assertNotEqual(int(witness["initial_registers"]["rax"]), 0)
        self.assertEqual(set(witness["initial_registers"]), {"rax", "rcx", "rdx", "r8", "r9", "r10", "r11"})

    def test_x86_other_scratch_is_universally_quantified(self):
        witness = self.counterexample(X, ["const", "0"], [["mov", "rax", "r8"]], "4c89c0c3")
        self.assertNotEqual(int(witness["initial_registers"]["r8"]), 0)

    def test_x86_mutant_xor_is_not_add(self):
        self.counterexample(X, ["add", ["input", "x"], ["input", "y"]],
                            [["mov", "rax", "rdi"], ["xor", "rax", "rsi"]], "4889f84831f0c3")

    def test_x86_mutating_argument_register_does_not_mutate_spec_input(self):
        self.equivalent(X, ["add", ["input", "x"], ["input", "y"]],
                        [["add", "rdi", "rsi"], ["mov", "rax", "rdi"]], "4801f74889f8c3")

    def test_x86_unsafe_prefixes_memory_callee_saved_and_width_refuse(self):
        self.refused_bytes(X, ["89f8c3", "4089f8c3", "664889f8c3", "f04801f0c3",
                               "f390c3", "674889f8c3", "654889f8c3", "484889f8c3",
                               "4a89f8c3", "488907c3", "488b07c3", "4889d8c3",
                               "4889e0c3", "4989ecc3", "488d0500000000c3",
                               "488d0424c3", "488d042dc3", "4889f848c1f801c3",
                               "4889f848c1e040c3", "4889f848c1e001c3", "0f0bc3",
                               "c3c3", "c390", "c20000", "48b800c3"])

    def test_x86_all_single_byte_bodies_except_exact_nop_refuse(self):
        for byte in range(256):
            raw = bytes([byte, 0xC3])
            if byte == 0x90:
                self.assertEqual(v.decode(X, raw)[0].op, "nop")
            else:
                with self.assertRaises(ValueError, msg=hex(byte)):
                    v.decode(X, raw)

    def test_rv_add_sub_xor_and_or_literal_words(self):
        for name, raw in [("add", "3305b500"), ("sub", "3305b540"),
                          ("xor", "3345b500"), ("and", "3375b500"), ("or", "3365b500")]:
            self.equivalent(R, [name, ["input", "x"], ["input", "y"]], [[name, "a0", "a0", "a1"]], raw + RET_R)

    def test_rv_addi_sign_extension_and_modular_overflow(self):
        self.equivalent(R, ["sub", ["input", "x"], ["const", "1"]], [["addi", "a0", "a0", -1]], "1305f5ff" + RET_R)
        self.equivalent(R, ["sub", ["input", "x"], ["const", "2048"]], [["addi", "a0", "a0", -2048]], "13050580" + RET_R)
        self.equivalent(R, ["const", "2047"], [["addi", "a0", "zero", 2047]], "1305f07f" + RET_R)
        self.equivalent(R, ["const", str(MASK)], [["addi", "a0", "zero", -1]], "1305f0ff" + RET_R)

    def test_rv_shamt_high_bit_is_legal_and_logical(self):
        self.equivalent(R, ["shl", ["input", "x"], 63], [["slli", "a0", "a0", 63]], "1315f503" + RET_R)
        self.equivalent(R, ["lshr", ["input", "x"], 63], [["srli", "a0", "a0", 63]], "1355f503" + RET_R)
        self.equivalent(R, ["input", "x"], [["slli", "a0", "a0", 0]], "13150500" + RET_R)
        self.assertEqual(v.run_concrete(R, code("13551500" + RET_R), {"x": MASK, "y": 0}), MASK >> 1)

    def test_rv_not_composed_with_literal_scratch_and_xor(self):
        self.equivalent(R, ["not", ["input", "x"]], [["addi", "t1", "zero", -1], ["xor", "a0", "a0", "t1"]], "1303f0ff33456500" + RET_R)

    def test_rv_sltu_uses_unsigned_order_and_can_be_checked(self):
        raw = code("3335b500" + RET_R)
        self.assertEqual(v.run_concrete(R, raw, {"x": MASK, "y": 0}), 0)
        self.assertEqual(v.run_concrete(R, raw, {"x": 0, "y": MASK}), 1)
        self.equivalent(R, ["const", "0"], [["sltu", "a0", "a0", "a0"]], "3335a500" + RET_R)
        self.counterexample(R, ["const", "0"], [["sltu", "a0", "a0", "a1"]], "3335b500" + RET_R)

    def test_rv_scratch_is_not_zero_initialized(self):
        witness = self.counterexample(R, ["const", "0"], [["addi", "a0", "t0", 0]], "13850200" + RET_R)
        self.assertNotEqual(int(witness["initial_registers"]["t0"]), 0)
        self.assertEqual(set(witness["initial_registers"]), {"t0", "t1"})
        self.equivalent(R, ["input", "x"], [["addi", "t0", "a0", 0], ["addi", "a0", "t0", 0]], "9302050013850200" + RET_R)

    def test_rv_ret_only_is_input_identity(self):
        answer = self.equivalent(R, ["input", "x"], [], RET_R, 0)
        self.assertEqual(answer["cost"], {"bytes": 4, "instructions": 0})

    def test_rv_unsafe_and_other_extensions_refuse(self):
        self.refused_bytes(R, ["8280", "130000", "6780000013000000", "67804000",
                              "1300100067800000", "3300b50067800000", "3305b10067800000",
                              "1304050067800000", "2330a10067800000", "3b05b50067800000",
                              "3305b50267800000", "1355154067800000", "7300100067800000",
                              "6f00000067800000", "6310a50067800000", "0000000067800000"])

    def test_sixteen_body_limit_and_complete_consumption(self):
        for target, nop, ret in [(X, code("90"), code("c3")), (R, code("13000000"), code(RET_R))]:
            self.assertEqual(len(v.decode(target, nop * 16 + ret)), 17)
            with self.assertRaises(ValueError): v.decode(target, nop * 17 + ret)
            with self.assertRaises(ValueError): v.decode(target, nop)
            with self.assertRaises(ValueError): v.decode(target, b"")
            with self.assertRaises(ValueError): v.decode(target, b"\x90" * 257)
            with self.assertRaises(ValueError): v.decode(target, bytearray(ret))

    def test_problem_body_bound_and_exact_palette_are_enforced(self):
        raw = code("4889f8c3")
        p = problem(X, ["input", "x"], [["mov", "rax", "rdi"]], 0)
        self.assertEqual(v.check(p, raw)["status"], "refused")
        p["max_instructions"] = 1; p["grammar"] = [["mov", "rax", "rsi"]]
        self.assertEqual(v.check(p, raw)["status"], "refused")
        p["grammar"] = [["mov", "rax", "rdi"]]
        self.assertEqual(v.check(p, raw)["status"], "equivalent")

    def test_unused_malformed_or_unsafe_declared_forms_refuse(self):
        for target, raw, forms in [
            (X, "c3", [["nop", "rax"], ["mov", "rax", "rbx"], ["imm", "rax", "01"],
                       ["imm", "rax", 1], ["lea", "rax", "rdi", "rsi", 3], ["shl", "rax", 64], ["syscall"]]),
            (R, RET_R, [["addi", "zero", "zero", 0], ["addi", "a0", "a0", 2048],
                        ["slli", "a0", "a0", 64], ["add", "a0", "sp", "a1"], ["mul", "a0", "a0", "a1"]])]:
            for form in forms:
                with self.subTest(target=target, form=form):
                    result = v.check(problem(target, ["input", "x"], [form], 0), code(raw))
                    self.assertEqual(result["status"], "refused", result)

    def test_register_self_cancellation_is_independent_of_initial_scratch(self):
        self.equivalent(X, ["const", "0"], [["xor", "rax", "rax"]], "4831c0c3")
        self.equivalent(R, ["const", "0"], [["xor", "a0", "t1", "t1"]], "33456300" + RET_R)

    def test_malformed_problem_and_timeout_refuse_without_a_solver_claim(self):
        p = problem(R, ["input", "x"], [], 0)
        for mutation in [{**p, "extra": 1}, {**p, "max_instructions": True},
                         {**p, "expression": ["const", "01"]},
                         {**p, "expression": ["shl", ["input", "x"], 64]}]:
            result = v.check(mutation, code(RET_R))
            self.assertEqual(result["status"], "refused"); self.assertTrue(result["reason"])
        for timeout in [0, -1, True, 60001, "1000"]:
            self.assertEqual(v.check(p, code(RET_R), timeout)["status"], "refused")

    def test_concrete_rejects_implicit_identity_overrides_and_nonwords(self):
        for values, initial in [({"x": True, "y": 0}, None), ({"x": -1, "y": 0}, None),
                                ({"x": 0, "y": MASK + 1}, None), ({"x": 0}, None),
                                ({"x": 0, "y": 0}, {"rdi": 9}),
                                ({"x": 0, "y": 0}, {"rax": True})]:
            with self.assertRaises(ValueError): v.run_concrete(X, code("c3"), values, initial)

    def test_solver_absent_is_inconclusive(self):
        with patch.dict(sys.modules, {"z3": None}):
            result = v.check(problem(R, ["input", "x"], [], 0), code(RET_R))
        self.assertEqual(result["status"], "inconclusive"); self.assertEqual(result["reason"], "Z3Unavailable")

    def test_solver_unknown_and_exception_are_never_equivalent(self):
        import z3
        class Unknown:
            def set(self, **kwargs): pass
            def add(self, *args): pass
            def check(self): return z3.unknown
            def reason_unknown(self): return "timeout"
        with patch.object(z3, "Solver", lambda **kwargs: Unknown()):
            result = v.check(problem(R, ["input", "x"], [], 0), code(RET_R))
        self.assertEqual(result["status"], "inconclusive"); self.assertIn("timeout", result["reason"])
        class Broken(Unknown):
            def check(self): raise z3.Z3Exception("controlled solver error")
        with patch.object(z3, "Solver", lambda **kwargs: Broken()):
            result = v.check(problem(R, ["input", "x"], [], 0), code(RET_R))
        self.assertEqual(result["status"], "inconclusive"); self.assertIn("controlled solver error", result["reason"])

    def test_intended_decoder_and_semantic_mutants_are_distinguished(self):
        raw = code("4889f848d1e8c3")
        proper = v.run_concrete(X, raw, {"x": 1 << 63, "y": 0})
        arithmetic_shift_mutant = ((1 << 63) - (1 << 64)) >> 1 & MASK
        width32_mutant = ((1 << 63) & 0xFFFFFFFF) >> 1
        self.assertNotEqual(proper, arithmetic_shift_mutant)
        self.assertNotEqual(proper, width32_mutant)
        # Deterministically simulate an incorrect concrete result: witness
        # replay must refuse to promote the solver's counterexample.
        p = problem(X, ["const", "0"], [], 0)
        with patch.object(v, "run_concrete", return_value=0):
            result = v.check(p, code("c3"))
        self.assertEqual(result["status"], "inconclusive")
        self.assertEqual(result["reason"], "CounterexampleReplayMismatch")


if __name__ == "__main__":
    unittest.main()
