"""Independent literal-byte controls for the profile-3 memory-region decoder, admission rules and SMT checker.

x86 bytes were checked against GNU objdump and are written here as literals; nothing imports the proposal
encoder, search or session. Memory kernels are never executed natively here. Solver unknown and replay mismatch
controls are deterministic mocks.
"""
from pathlib import Path
import copy
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
from machine_code import library, profile3, verify3 as v

X, MASK = v.X86, (1 << 64) - 1
ROOT = Path(__file__).resolve().parents[1]
LOAD_RAX_P0, LOAD_RCX_P1 = "488b07", "488b4f08"          # mov rax,[rdi]  mov rcx,[rdi+8]
STORE_P0_RCX, STORE_P1_RAX = "48890f", "48894708"        # mov [rdi],rcx  mov [rdi+8],rax
STORE_P0_RAX, STORE_P0_RSI = "488907", "488937"          # mov [rdi],rax  mov [rdi],rsi
ADD_RAX_RSI, ADD_RAX_RCX, MOV_RAX_RDI = "4801f0", "4801c8", "4889f8"
STORE_Q0_RAX, STORE_Q1_RCX = "488906", "48894e08"        # mov [rsi],rax  mov [rsi+8],rcx
SWAP = LOAD_RAX_P0 + LOAD_RCX_P1 + STORE_P0_RCX + STORE_P1_RAX + "c3"


def code(text):
    return bytes.fromhex(text)


def initial(region, index):
    return ["initial", region, index]


def region(name, base, words, access):
    return {"name": name, "base": base, "words": words, "access": access}


def problem(regions, memory, outputs=(), inputs=(), grammar=None, bound=8):
    return {"schema": profile3.SCHEMA, "target": X, "inputs": list(inputs), "regions": regions,
            "outputs": list(outputs), "memory": memory, "grammar": grammar if grammar is not None else [],
            "max_instructions": bound, "objective": "bytes"}


def forms(text):
    return [i.form() for i in v.decode(code(text))[:-1]]


def swap():
    return problem([region("p", "rdi", 2, "read-write")], {"p": [initial("p", 1), initial("p", 0)]},
                   grammar=forms(SWAP))


def load_add_store(grammar_text=LOAD_RAX_P0 + ADD_RAX_RSI + STORE_P0_RAX + "c3"):
    return problem([region("p", "rdi", 1, "read-write")], {"p": [["add", initial("p", 0), ["input", "x"]]]},
                   inputs=["x"], grammar=forms(grammar_text))


def copy_sum(text):
    return problem([region("src", "rdi", 2, "read-only"), region("dst", "rsi", 2, "read-write")],
                   {"dst": [initial("src", 0), initial("src", 1)]},
                   outputs=[["add", initial("src", 0), initial("src", 1)]], grammar=forms(text))


COPY = LOAD_RAX_P0 + LOAD_RCX_P1 + STORE_Q0_RAX + STORE_Q1_RCX + ADD_RAX_RCX + "c3"


class Profile3VerifierTests(unittest.TestCase):
    def check(self, p, text):
        return v.check(p, code(text))

    def refused(self, p, text, reason):
        result = self.check(p, text)
        self.assertEqual((result["status"], result.get("reason")), ("refused", reason), result)

    def test_swap_and_copy_kernels_are_equivalent(self):
        for p, text in ((swap(), SWAP), (load_add_store(), LOAD_RAX_P0 + ADD_RAX_RSI + STORE_P0_RAX + "c3"),
                        (copy_sum(COPY), COPY)):
            with self.subTest(text=text):
                result = self.check(p, text)
                self.assertEqual(result["status"], "equivalent", result)
        self.assertEqual(self.check(swap(), SWAP)["cost"], {"bytes": 15, "instructions": 4})

    def test_concrete_swap_moves_the_two_words(self):
        self.assertEqual(v.run_concrete(swap(), code(SWAP), {}, {"p": 4096}, {"p": [7, 9]}), ([], {"p": [9, 7]}))

    def test_wrong_store_order_and_dropped_store_are_counterexamples_that_replay(self):
        # Store word 0 before reading it: word 1 receives the new word 0.
        wrong = LOAD_RCX_P1 + STORE_P0_RCX + LOAD_RAX_P0 + STORE_P1_RAX + "c3"
        p = swap(); p["grammar"] = forms(wrong)
        result = self.check(p, wrong)
        self.assertEqual(result["status"], "counterexample", result)
        words = [int(w) for w in result["initial_regions"]["p"]]
        self.assertNotEqual(words[0], words[1])
        self.assertEqual(result["expected_regions"]["p"], [str(words[1]), str(words[0])])
        self.assertEqual(result["actual_regions"]["p"], [str(words[1]), str(words[1])])
        dropped = LOAD_RAX_P0 + LOAD_RCX_P1 + STORE_P0_RCX + "c3"
        p["grammar"] = forms(dropped)
        self.assertEqual(self.check(p, dropped)["status"], "counterexample")

    def test_initial_region_words_are_quantified(self):
        # Storing x is correct only when the word was 0 before the call.
        text = STORE_P0_RSI + "c3"
        result = self.check(load_add_store(text), text)
        self.assertEqual(result["status"], "counterexample", result)
        self.assertNotEqual(result["initial_regions"]["p"], ["0"])
        # A read-only source is quantified too: returning a constant sum is refuted.
        p = copy_sum(COPY)
        p["outputs"] = [["const", "0"]]
        self.assertEqual(self.check(p, COPY)["status"], "counterexample")

    def test_base_pointer_values_are_quantified(self):
        text = MOV_RAX_RDI + "c3"
        p = problem([region("p", "rdi", 1, "read-only")], {}, outputs=[["const", "0"]], grammar=forms(text))
        result = self.check(p, text)
        self.assertEqual(result["status"], "counterexample", result)
        self.assertNotEqual(result["bases"]["p"], "0")
        self.assertEqual(result["actual"], [result["bases"]["p"]])

    def test_scratch_and_flags_are_quantified(self):
        text = STORE_P0_RCX + "c3"     # stores whatever rcx held at entry
        p = problem([region("p", "rdi", 1, "read-write")], {"p": [["const", "0"]]}, grammar=forms(text))
        result = self.check(p, text)
        self.assertEqual(result["status"], "counterexample", result)
        self.assertEqual(result["actual_regions"]["p"], [result["initial_registers"]["rcx"]])
        text = "0f92c2" + "c3"           # setb dl reads the entry carry flag
        p = problem([region("p", "rdi", 1, "read-only")], {}, outputs=[["const", "0"], ["const", "0"]],
                    grammar=forms(text))
        self.assertEqual(self.check(p, text)["status"], "counterexample")

    def test_store_to_read_only_region_refuses(self):
        text = LOAD_RAX_P0 + STORE_P0_RAX + "c3"    # writes back the same value
        p = problem([region("p", "rdi", 1, "read-only")], {}, outputs=[initial("p", 0)], grammar=forms(text))
        self.refused(p, text, "StoreToReadOnlyRegion")
        self.assertRaisesRegex(ValueError, "StoreToReadOnlyRegion", v.execute, p, code(text),
                               dict.fromkeys(v.REGISTERS[X], 0), dict.fromkeys(v.FLAGS, 0), {"p": [1]})

    def test_misaligned_access_refuses(self):
        text = "488b4704" + "c3"         # mov rax,[rdi+4]
        p = problem([region("p", "rdi", 2, "read-only")], {}, outputs=[initial("p", 0)], grammar=[["load", "rax", "rdi", 4]])
        with self.assertRaisesRegex(ValueError, "MisalignedAccess"):
            v._palette(p)
        p["grammar"] = []
        self.assertEqual(v.check(p, code(text))["reason"], "InstructionOutsideGrammar")
        with self.assertRaisesRegex(ValueError, "MisalignedAccess"):
            v.admit(p, v.decode(code(text)))

    def test_out_of_region_displacements_refuse(self):
        p = problem([region("p", "rdi", 2, "read-write")], {"p": [initial("p", 0), initial("p", 1)]})
        for text in ("488b4710" + "c3", "488b47f8" + "c3"):    # [rdi+16] and [rdi-8]
            with self.subTest(text=text), self.assertRaisesRegex(ValueError, "DisplacementOutsideRegion"):
                v.admit(p, v.decode(code(text)))
        with self.assertRaisesRegex(ValueError, "DisplacementOutsideRegion"):
            v._palette({**p, "grammar": [["store", "rdi", 16, "rax"]]})

    def test_a_redefined_base_refuses(self):
        p = problem([region("p", "rdi", 1, "read-only")], {}, outputs=[initial("p", 0)], inputs=["x"])
        for text in ("4889f7" + LOAD_RAX_P0 + "c3", "488b3f" + LOAD_RAX_P0 + "c3"):  # mov rdi,rsi / mov rdi,[rdi]
            with self.subTest(text=text), self.assertRaisesRegex(ValueError, "RedefinedRegionBase"):
                v.admit(p, v.decode(code(text)))
        # Writing the base after its last access is admitted.
        v.admit(p, v.decode(code(LOAD_RAX_P0 + "4889f7" + "c3")))

    def test_other_addressing_and_bases_refuse(self):
        p = problem([region("p", "rdi", 1, "read-only")], {}, outputs=[initial("p", 0)], inputs=["x"])
        with self.assertRaisesRegex(ValueError, "MemoryBaseNotARegion"):
            v.admit(p, v.decode(code("488b06" + "c3")))        # mov rax,[rsi]; rsi holds x
        for text, reason in (("488b0424c3", "UnsupportedAddressing"), ("488b0500000000c3", "UnsupportedAddressing"),
                             ("488b8708000000c3", "UnsupportedAddressing"), ("488b4700c3", "NonCanonicalDisplacement"),
                             ("4a8b07c3", "UnusedRexBits"), ("488b07", "MissingTerminalReturn")):
            with self.subTest(text=text), self.assertRaisesRegex(ValueError, reason):
                v.decode(code(text))

    def test_problem_schema_refusals(self):
        good = swap()
        for change in ({"regions": []}, {"regions": [region("p", "rdi", 17, "read-write")]},
                       {"regions": [region("p", "rax", 2, "read-write")]},
                       {"regions": [region("p", "rdi", 2, "read-write"), region("q", "rdi", 1, "read-only")]},
                       {"regions": [region("p", "rdi", 2, "write-only")]}, {"memory": {}},
                       {"memory": {"p": [initial("p", 1)]}}, {"memory": {"p": [initial("p", 2), initial("p", 0)]}},
                       {"inputs": ["p"]}, {"inputs": ["a", "b", "c", "d", "e", "f"]}, {"extra": 1}):
            with self.subTest(change=change), self.assertRaises(ValueError):
                profile3.validate_problem({**good, **change})
        read_only = problem([region("p", "rdi", 1, "read-only")], {})
        with self.assertRaisesRegex(ValueError, "needs a result or a read-write region"):
            profile3.validate_problem(read_only)

    def test_word_inputs_take_the_argument_registers_no_region_names(self):
        p = problem([region("p", "rsi", 1, "read-write")], {"p": [["input", "y"]]}, inputs=["x", "y"])
        self.assertEqual(profile3.registers(profile3.validate_problem(p)), ({"x": "rdi", "y": "rdx"}, {"p": "rsi"}))

    def test_solver_absent_unknown_and_replay_mismatch_are_never_equivalent(self):
        with patch.dict(sys.modules, {"z3": None}):
            self.assertEqual(v.check(swap(), code(SWAP))["reason"], "Z3Unavailable")
        import z3
        class Unknown:
            def set(self, **kwargs): pass
            def add(self, *args): pass
            def check(self): return z3.unknown
            def reason_unknown(self): return "timeout"
        with patch.object(z3, "Solver", lambda **kwargs: Unknown()):
            result = v.check(swap(), code(SWAP))
        self.assertEqual(result["status"], "inconclusive"); self.assertIn("timeout", result["reason"])
        wrong = LOAD_RCX_P1 + STORE_P0_RCX + LOAD_RAX_P0 + STORE_P1_RAX + "c3"
        p = swap(); p["grammar"] = forms(wrong)
        real = v.run_concrete
        def perturbed(*args, **kwargs):
            results, final = real(*args, **kwargs)
            return results, {name: [word ^ 1 for word in words] for name, words in final.items()}
        with patch.object(v, "run_concrete", perturbed):
            result = v.check(p, code(wrong))
        self.assertEqual((result["status"], result["reason"]), ("inconclusive", "CounterexampleReplayMismatch"))

    def test_memory_library_rechecks_at_recorded_costs(self):
        report = library.check(ROOT / "test-data/machine-code/memory-kernels")
        self.assertEqual(report["status"], "checked", report["kernels"])
        self.assertEqual({row["name"] for row in report["kernels"]},
                         {"swap-words-x86", "load-add-store-x86", "copy2-sum-x86"})
        self.assertEqual({row["target"] for row in report["kernels"]}, {X})

    def test_refused_problem_or_palette_is_never_checked(self):
        p = copy.deepcopy(swap())
        p["grammar"].append(["store", "rdi", 3, "rax"])
        self.assertEqual(self.check(p, SWAP), {"status": "refused", "reason": "MisalignedAccess"})


if __name__ == "__main__":
    unittest.main()
