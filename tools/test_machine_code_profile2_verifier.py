"""Independent literal-byte controls for the profile-2 decoder, flag model and SMT checker, never native execution.

x86 gold bytes were produced by GNU `as` and are written here as literals; RV64I words are packed by the local
field helpers below from the ISA's R- and I-type layouts. Neither imports the proposal encoder, search or session.
Solver unknown and replay mismatch controls are deterministic mocks, not timeout benchmarks.
"""
from pathlib import Path
import copy
import struct
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
from machine_code import verify as v1
from machine_code import verify2 as v

X, R, MASK = v.X86, v.RV64, (1 << 64) - 1
MIN, MAX = 1 << 63, (1 << 63) - 1
RET_R = "67800000"
RV = {"zero": 0, "t0": 5, "t1": 6, "t2": 7, **{f"a{i}": 10 + i for i in range(8)}}


def code(text):
    return bytes.fromhex(text)


def rtype(funct7, rd, rs1, rs2, funct3):
    return struct.pack("<I", (funct7 << 25) | (RV[rs2] << 20) | (RV[rs1] << 15) | (funct3 << 12) | (RV[rd] << 7) | 0x33).hex()


def itype(immediate, rd, rs1, funct3):
    return struct.pack("<I", ((immediate & 0xFFF) << 20) | (RV[rs1] << 15) | (funct3 << 12) | (RV[rd] << 7) | 0x13).hex()


def i(name):
    return ["input", name]


def c(value):
    return ["const", str(value)]


def problem(target, inputs, outputs, grammar, bound=16):
    return {"schema": "zeno-fcis/machine-problem/2", "target": target, "inputs": inputs,
            "outputs": outputs, "grammar": grammar, "max_instructions": bound, "objective": "bytes"}


def signed(value):
    return value - (1 << 64) if value >> 63 else value


def smt_point(target, raw, registers, flags=None):
    """The SMT model at one constant state: each register and flag simplifies to a value; undefined stays None."""
    import z3
    context = z3.Context()
    symbolic = {name: z3.BitVecVal(value, 64, context) for name, value in registers.items()}
    truth = {name: z3.BoolVal(bool(value), context) for name, value in (flags or {}).items()}
    after, after_flags = v.machine_smt(z3, target, v.decode(target, code(raw)), symbolic, truth, context)
    return ({name: z3.simplify(value).as_long() for name, value in after.items()},
            {name: None if value is None else int(z3.is_true(z3.simplify(value))) for name, value in after_flags.items()})


def state(target, **values):
    registers = dict.fromkeys(v.REGISTERS[target], 0)
    registers.update(values)
    return registers


ADD128 = [["add", i("a_lo"), i("b_lo")],
          ["add", ["add", i("a_hi"), i("b_hi")], ["ult", ["add", i("a_lo"), i("b_lo")], i("a_lo")]]]
SUB128 = [["sub", i("a_lo"), i("b_lo")],
          ["sub", ["sub", i("a_hi"), i("b_hi")], ["ult", i("a_lo"), i("b_lo")]]]
WORDS4 = ["a_lo", "a_hi", "b_lo", "b_hi"]
UMIN = ["ite", ["ule", i("x"), i("y")], i("x"), i("y")]
SMAX = ["ite", ["slt", i("x"), i("y")], i("y"), i("x")]
ABS = ["ite", ["slt", i("x"), c(0)], ["sub", c(0), i("x")], i("x")]
SATURATING = ["ite", ["ult", ["add", i("x"), i("y")], i("x")], c(MASK), ["add", i("x"), i("y")]]
CARRY = [["add", i("x"), i("y")], ["ult", ["add", i("x"), i("y")], i("x")]]


class Profile2VerifierTests(unittest.TestCase):
    def check(self, target, inputs, outputs, raw, grammar=None, bound=16):
        grammar = grammar if grammar is not None else [i.form() for i in v.decode(target, code(raw))[:-1]]
        p = problem(target, inputs, outputs, grammar, bound)
        before = copy.deepcopy(p)
        answer = v.check(p, code(raw), timeout_ms=5000)
        self.assertEqual(p, before)
        return answer

    def equivalent(self, target, inputs, outputs, raw, grammar=None, bound=16):
        answer = self.check(target, inputs, outputs, raw, grammar, bound)
        self.assertEqual(answer["status"], "equivalent", answer)
        self.assertIn("z3_version", answer)
        self.assertEqual(answer["cost"]["bytes"], len(code(raw)))
        return answer

    def counterexample(self, target, inputs, outputs, raw, grammar=None, bound=16):
        answer = self.check(target, inputs, outputs, raw, grammar, bound)
        self.assertEqual(answer["status"], "counterexample", answer)
        arguments = [int(answer["inputs"][n]) for n in inputs]
        initial = {k: int(x) for k, x in answer["initial_registers"].items()}
        flags = answer.get("initial_flags")
        self.assertEqual(v.run_concrete(target, code(raw), arguments, initial, flags, len(outputs)),
                         [int(x) for x in answer["actual"]])
        self.assertNotEqual(answer["expected"], answer["actual"])
        return answer

    def refused(self, target, cases):
        for raw in cases:
            with self.subTest(raw=raw), self.assertRaises(ValueError):
                v.decode(target, code(raw))

    def test_x86_128_bit_add_and_sub_carry_through_adc_and_sbb(self):
        answer = self.equivalent(X, WORDS4, ADD128, "4889f84801d04889f24811cac3")
        self.assertEqual(answer["cost"], {"bytes": 13, "instructions": 4})
        self.assertEqual(answer["scope"]["results"], "rax,rdx")
        self.assertEqual(answer["scope"]["arguments"], "a_lo=rdi,a_hi=rsi,b_lo=rdx,b_hi=rcx")
        self.equivalent(X, WORDS4, SUB128, "4889f84829d04889f24819cac3")
        lost = self.counterexample(X, WORDS4, ADD128, "4889f84801d04889f24801cac3")
        self.assertEqual(lost["expected"][0], lost["actual"][0])
        self.assertEqual((int(lost["expected"][1]) - int(lost["actual"][1])) & MASK, 1)
        self.assertEqual(v.run_concrete(X, code("4889f84801d04889f24811cac3"), [MASK, 0, 1, 0], results=2), [0, 1])

    def test_x86_compare_and_select_min_max_and_their_swapped_mutants(self):
        self.equivalent(X, ["x", "y"], [UMIN], "4889f84839f7480f47c6c3")
        self.equivalent(X, ["x", "y"], [SMAX], "4889f84839f7480f4cc6c3")
        for raw, spec in [("4889f84839f7480f42c6c3", UMIN), ("4889f84839f7480f4fc6c3", SMAX),
                          ("4889f84839f7480f4cc6c3", ["ite", ["ult", i("x"), i("y")], i("y"), i("x")])]:
            with self.subTest(raw=raw):
                self.counterexample(X, ["x", "y"], [spec], raw)

    def test_x86_setcc_writes_only_the_low_byte(self):
        partial = self.counterexample(X, ["x", "y"], CARRY, "4889f84801f00f92c2c3")
        self.assertNotEqual(int(partial["initial_registers"]["rdx"]) >> 8, 0)
        self.equivalent(X, ["x", "y"], CARRY, "4831d24889f84801f00f92c2c3")
        self.equivalent(X, ["x", "y"], CARRY, "4889f84801f00f92c2480fb6d2c3")
        self.equivalent(X, ["x", "y"], [["eq", ["and", i("x"), i("y")], c(0)]], "4885f70f94c0480fb6c0c3")
        self.equivalent(X, ["x", "y"], [["slt", i("x"), i("y")]], "4839f70f9cc0480fb6c0c3")
        final, _ = v.execute(X, code("400f92c6410f92c0400f90c7c3"),
                             dict.fromkeys(v.REGISTERS[X], MASK), {"cf": 0, "zf": 0, "sf": 0, "of": 1})
        self.assertEqual((final["rsi"], final["r8"], final["rdi"]), (MASK - 255, MASK - 255, MASK - 254))

    def test_x86_entry_flags_are_universally_quantified(self):
        witness = self.counterexample(X, ["x", "y"], [["add", i("x"), i("y")]], "4889f84811f0c3")
        self.assertEqual(witness["initial_flags"]["cf"], 1)
        self.assertEqual(set(witness["initial_flags"]), {"cf", "zf", "sf", "of"})
        self.assertEqual(v.run_concrete(X, code("4889f84811f0c3"), [2, 3], flags={"cf": 1}), [6])

    def test_x86_saturating_add_absolute_value_and_signed_saturation(self):
        self.equivalent(X, ["x", "y"], [SATURATING], "4889f84801f04819d24809d0c3")
        self.equivalent(X, ["x"], [ABS], "4889f848f7d8480f48c7c3")
        self.equivalent(X, ["x"], [ABS], "4889fa48c1fa3f4889f84831d04829d0c3")
        total = ["add", i("x"), i("y")]
        overflow = ["slt", ["and", ["xor", i("x"), total], ["xor", i("y"), total]], c(0)]
        signed = ["ite", overflow, ["ite", ["slt", i("x"), c(0)], c(MIN), c(MAX)], total]
        # mov rdx,rdi; sar rdx,63; movabs rcx,MAX; xor rdx,rcx; mov rax,rdi; add rax,rsi; cmovo rax,rdx
        self.equivalent(X, ["x", "y"], [signed],
                        "4889fa48c1fa3f48b9ffffffffffffff7f4831ca4889f84801f0480f40c2c3")
        self.assertEqual(v.run_concrete(X, code("4889f848f7d8480f48c7c3"), [MIN]), [MIN])

    def test_x86_reading_a_flag_left_undefined_refuses(self):
        for raw, why in [("4889f8480fafc60f94c1c3", "ZF after IMUL"), ("4889f848c1e0020f90c1c3", "OF after SHL 2")]:
            with self.subTest(why=why):
                result = self.check(X, ["x", "y"], [["mul", i("x"), i("y")]], raw)
                self.assertEqual((result["status"], result["reason"]), ("refused", "ReadsUndefinedFlag"))
                with self.assertRaisesRegex(ValueError, "ReadsUndefinedFlag"):
                    v.run_concrete(X, code(raw), [1, 2])
        # IMUL defines OF, and a one-bit SHL defines OF as the result's top bit XOR CF.
        self.equivalent(X, ["x", "y"], [["mul", i("x"), i("y")]], "4889f8480fafc60f90c1c3")
        state = dict.fromkeys(v.REGISTERS[X], 0); state["rdi"] = MIN
        final, flags = v.execute(X, code("4889f848d1e00f90c1c3"), state, dict.fromkeys(v.FLAGS, 0))
        self.assertEqual((final["rax"], final["rcx"], flags), (0, 1, {"cf": 1, "zf": 1, "sf": 0, "of": 1}))
        final, flags = v.execute(X, code("48c1f900c3"), dict.fromkeys(v.REGISTERS[X], 5),
                                 {"cf": 1, "zf": 0, "sf": 1, "of": 0})
        self.assertEqual((final["rcx"], flags), (5, {"cf": 1, "zf": 0, "sf": 1, "of": 0}))

    def test_x86_sign_flag_is_bit_63_of_the_result(self):
        # add rax, rcx. The first result has bit 63 set and bit 62 clear, the second the reverse.
        raw = "4801c8c3"
        flags = dict.fromkeys(v.FLAGS, 0)
        for left, right, sign in ((MIN, 0, 1), (1 << 62, 0, 0), (MAX, 1, 1), (MASK, 1, 0), (3 << 62, 0, 1)):
            registers = state(X, rax=left, rcx=right)
            with self.subTest(left=hex(left), right=right):
                self.assertEqual(v.execute(X, code(raw), registers, flags)[1]["sf"], sign)
                self.assertEqual(smt_point(X, raw, registers, flags)[1]["sf"], sign)

    def test_x86_le_holds_on_equal_values_through_the_zero_flag(self):
        # cmp rdi, rsi on equal values gives ZF=1 and SF=OF=0, so LE holds only because of ZF.
        move = "4839f7480f4ec2c3"  # cmp rdi, rsi; cmovle rax, rdx
        flags = dict.fromkeys(v.FLAGS, 0)
        for value in (0, 5, MIN, MASK):
            registers = state(X, rdi=value, rsi=value, rax=1, rdx=2)
            with self.subTest(value=hex(value)):
                final, after = v.execute(X, code(move), registers, flags)
                self.assertEqual((final["rax"], after["zf"], after["sf"], after["of"]), (2, 1, 0, 0))
                self.assertEqual(smt_point(X, move, registers, flags)[0]["rax"], 2)
        # cmp rdi, rsi; setle al; movzx rax, al computes x <= y, equal values included.
        self.equivalent(X, ["x", "y"], [["sle", i("x"), i("y")]], "4839f70f9ec0480fb6c0c3")

    def test_x86_two_results_quantify_the_free_second_register(self):
        witness = self.counterexample(X, ["x", "y"], [["input", "x"], ["input", "y"]], "4889f8c3")
        self.assertEqual(set(witness["initial_registers"]), {"rax", "rcx", "rdx", "r8", "r9", "r10", "r11"})
        self.equivalent(X, ["x", "y"], [["input", "x"], ["input", "y"]], "4889f84889f2c3")
        six = self.equivalent(X, ["a", "b", "c", "d", "e", "f"], [["add", i("e"), i("f")]], "4b8d0408c3")
        self.assertEqual(six["scope"]["initial_registers"], "all 64-bit assignments to rax,r10,r11")

    def test_x86_parity_byte_register_and_width_variants_refuse(self):
        self.refused(X, ["480f4ac6c3", "0f9ac0c3", "0f9bc0c3", "0f92c4c3", "0f92c3c3", "0f92c7c3",
                         "400f92c0c3", "480f92c0c3", "410f92c4c3", "420f92c0c3", "0f92c8c3", "0f9200c3",
                         "0fb6c0c3", "480fb6c4c3", "480fb6c5c3", "0f42c6c3", "480f4206c3", "48f7d0c3",
                         "48f7c000000000c3", "48f71fc3", "48c1f801c3", "481906c3", "4811c3c3",
                         "4885d8c3", "4839e0c3", "4ef7d8c3", "4cf7d8c3", "48d3e0c3", "480fa3c6c3"])

    def test_rv_comparisons_immediates_and_arithmetic_shifts(self):
        ret = RET_R
        self.equivalent(R, ["x", "y"], [["slt", i("x"), i("y")]], rtype(0, "a0", "a0", "a1", 2) + ret)
        self.equivalent(R, ["x", "y"], [["ult", i("x"), i("y")]], rtype(0, "a0", "a0", "a1", 3) + ret)
        self.equivalent(R, ["x"], [["slt", i("x"), c(MASK - 4)]], itype(-5, "a0", "a0", 2) + ret)
        self.equivalent(R, ["x"], [["ult", i("x"), c(MASK)]], itype(-1, "a0", "a0", 3) + ret)
        self.equivalent(R, ["x"], [["not", i("x")]], itype(-1, "a0", "a0", 4) + ret)
        self.equivalent(R, ["x"], [["or", i("x"), c(MASK - 2047)]], itype(-2048, "a0", "a0", 6) + ret)
        self.equivalent(R, ["x"], [["and", i("x"), c(2047)]], itype(2047, "a0", "a0", 7) + ret)
        self.equivalent(R, ["x"], [["ashr", i("x"), 63]], itype((0x10 << 6) | 63, "a0", "a0", 5) + ret)
        self.equivalent(R, ["x", "y"], [["ashr", i("x"), 7]],
                        itype(7, "a1", "zero", 0) + rtype(0x20, "a0", "a0", "a1", 5) + ret)
        sra = code(rtype(0x20, "a0", "a0", "a1", 5) + ret)
        self.assertEqual(v.run_concrete(R, sra, [MIN, 64 + 1]), [MIN | (MIN >> 1)])
        self.counterexample(R, ["x"], [["lshr", i("x"), 1]], itype((0x10 << 6) | 1, "a0", "a0", 5) + ret)

    def test_rv_sra_uses_six_count_bits_on_negative_values(self):
        # Counts 32 to 63 need the sixth count bit; a five-bit count would shift by count - 32.
        raw = rtype(0x20, "a0", "a0", "a1", 5) + RET_R
        value = MIN | 0x0123456789ABCDEF
        for count in (32, 40, 63, 64 + 40):
            expected = (signed(value) >> (count & 63)) & MASK
            with self.subTest(count=count):
                self.assertEqual(v.run_concrete(R, code(raw), [value, count]), [expected])
                self.assertEqual(smt_point(R, raw, state(R, a0=value, a1=count))[0]["a0"], expected)

    def test_rv_two_results_and_abs_without_branches(self):
        add128 = (rtype(0, "a0", "a0", "a2", 0) + rtype(0, "t0", "a0", "a2", 3)
                  + rtype(0, "a1", "a1", "a3", 0) + rtype(0, "a1", "a1", "t0", 0) + RET_R)
        answer = self.equivalent(R, WORDS4, ADD128, add128)
        self.assertEqual(answer["scope"]["results"], "a0,a1")
        missing = self.counterexample(R, WORDS4, ADD128, add128[:16] + add128[24:])
        self.assertEqual(missing["expected"][0], missing["actual"][0])
        self.equivalent(R, ["x"], [ABS], itype((0x10 << 6) | 63, "t0", "a0", 5) + rtype(0, "a0", "a0", "t0", 4)
                        + rtype(0x20, "a0", "a0", "t0", 0) + RET_R)
        witness = self.counterexample(R, ["x"], [c(0), c(0)], itype(0, "a0", "zero", 0) + RET_R)
        self.assertIn("a1", witness["initial_registers"])
        self.assertNotIn("initial_flags", witness)

    def test_rv_unadmitted_shift_and_word_forms_refuse(self):
        self.refused(R, [rtype(0, "a0", "a0", "a1", 1) + RET_R, rtype(0, "a0", "a0", "a1", 5) + RET_R,
                         itype((0x11 << 6) | 1, "a0", "a0", 5) + RET_R, itype((0x30 << 6) | 1, "a0", "a0", 5) + RET_R,
                         itype((0x10 << 6) | 1, "a0", "a0", 1) + RET_R, "1b05150067800000", "3b05b50067800000",
                         itype(1, "zero", "a0", 2) + RET_R, itype(1, "zero", "a0", 3) + RET_R,
                         rtype(1, "a0", "a0", "a1", 2) + RET_R, "3305850067800000", "0335050067800000",
                         "6300b50067800000"])

    def test_profile2_problem_shape_refusals(self):
        good = problem(X, ["x"], [i("x")], [], 0)
        self.assertEqual(v.check(good, code("4889f8c3"))["status"], "refused")
        self.assertEqual(v.check({**good, "grammar": [["mov", "rax", "rdi"]], "max_instructions": 1},
                                 code("4889f8c3"))["status"], "equivalent")
        for name, mutation in [
                ("seven inputs", {"inputs": list("abcdefg")}), ("duplicate inputs", {"inputs": ["x", "x"]}),
                ("no inputs but an input output", {"inputs": []}), ("upper-case name", {"inputs": ["X"]}),
                ("three outputs", {"outputs": [i("x")] * 3}), ("no outputs", {"outputs": []}),
                ("undeclared input", {"outputs": [i("y")]}), ("ite arity", {"outputs": [["ite", i("x"), i("x")]]}),
                ("unknown op", {"outputs": [["udiv", i("x"), i("x")]]}), ("shift count", {"outputs": [["ashr", i("x"), 64]]}),
                ("profile1 field", {"expression": i("x")}), ("profile1 target", {"target": v1.X86}),
                ("profile1 schema", {"schema": "zeno-fcis/machine-problem/1"}), ("bool bound", {"max_instructions": True}),
                ("unpadded const", {"outputs": [["const", "01"]]})]:
            with self.subTest(name=name):
                result = v.check({**good, **mutation}, code("c3"))
                self.assertEqual(result["status"], "refused", result)
        # Zero inputs is a constant problem: every argument register is quantified scratch.
        constant = problem(X, [], [c(7)], [["imm", "rax", "7"]], 1)
        self.assertEqual(v.check(constant, code("48b80700000000000000c3"))["status"], "equivalent")
        wrong = v.check(constant, code("4889f8c3"))
        self.assertEqual(v.check({**constant, "grammar": [["mov", "rax", "rdi"]]}, code("4889f8c3"))["status"],
                         "counterexample")
        self.assertEqual(wrong["status"], "refused")
        p1 = {"schema": "zeno-fcis/machine-problem/1", "target": v1.X86, "inputs": ["x", "y"],
              "expression": i("x"), "grammar": [], "max_instructions": 0, "objective": "bytes"}
        self.assertEqual(v.check(p1, code("c3"))["status"], "refused")
        self.assertEqual(v1.check({**good, "grammar": []}, code("c3"))["status"], "refused")

    def test_unused_malformed_or_unsafe_declared_forms_refuse(self):
        for target, raw, forms in [
                (X, "c3", [["setb", "rax", "rax"], ["cmovp", "rax", "rsi"], ["setp", "rax"], ["neg", "rbx"],
                           ["sar", "rax", 64], ["movzx", "rax"], ["adc", "rax", 1], ["cmovb", "rax", "rsp"]]),
                (R, RET_R, [["slti", "zero", "a0", 1], ["srai", "a0", "a0", 64], ["sltiu", "a0", "a0", 2048],
                            ["sra", "a0", "a0", "s0"], ["sll", "a0", "a0", "a1"], ["andi", "a0", "a0", True]])]:
            for form in forms:
                with self.subTest(target=target, form=form):
                    result = v.check(problem(target, ["x"], [i("x")], [form], 0), code(raw))
                    self.assertEqual(result["status"], "refused", result)

    def test_concrete_rejects_implicit_identity_overrides_and_nonwords(self):
        for arguments, initial, flags in [([True], None, None), ([-1], None, None), ([MASK + 1], None, None),
                                          ([0] * 7, None, None), ([0], {"rdi": 1}, None),
                                          ([0], {"rax": True}, None), ([0], None, {"pf": 1}), ([0], None, {"cf": 2}),
                                          ([0], None, {"cf": True})]:
            with self.subTest(arguments=arguments, initial=initial, flags=flags), self.assertRaises(ValueError):
                v.run_concrete(X, code("c3"), arguments, initial, flags)
        # A constant problem has no arguments; every argument register is then scratch.
        self.assertEqual(v.run_concrete(X, code("c3"), [], {"rax": 5}), [5])
        with self.assertRaises(ValueError):
            v.run_concrete(R, code(RET_R), [0], None, {"cf": 0})
        with self.assertRaises(ValueError):
            v.run_concrete(X, code("c3"), [0], results=3)

    def test_solver_absent_unknown_and_replay_mismatch_are_never_equivalent(self):
        p = problem(R, ["x"], [i("x")], [], 0)
        with patch.dict(sys.modules, {"z3": None}):
            self.assertEqual(v.check(p, code(RET_R))["reason"], "Z3Unavailable")
        import z3
        class Unknown:
            def set(self, **kwargs): pass
            def add(self, *args): pass
            def check(self): return z3.unknown
            def reason_unknown(self): return "timeout"
        with patch.object(z3, "Solver", lambda **kwargs: Unknown()):
            result = v.check(p, code(RET_R))
        self.assertEqual(result["status"], "inconclusive"); self.assertIn("timeout", result["reason"])
        with patch.object(v, "run_concrete", return_value=[0]):
            result = v.check(problem(X, ["x"], [c(0)], [], 0), code("c3"))
        self.assertEqual((result["status"], result["reason"]), ("inconclusive", "CounterexampleReplayMismatch"))


if __name__ == "__main__":
    unittest.main()
