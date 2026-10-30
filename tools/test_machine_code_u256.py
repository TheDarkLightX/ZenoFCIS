"""U256 add, sub and the unsigned and signed compares as checked profile-3 x86 kernels over a[4], b[4] and out[4].

Each library entry is rechecked at its recorded cost. Each problem's expressions are proved equal to 256-bit
arithmetic on the concatenated limbs (Z3, over the checker's own expression semantics) and agree with Python
integers on random states. Native runs and the calling-convention gate run on an x86-64 Linux host with gcc, and
the cvc5 confirmation runs when ZENO_FCIS_CVC5 names the pinned binary.
"""
import json
import os
from pathlib import Path
import platform
import random
import tempfile
import unittest

import z3

from machine_code import library, profile3, targets, verify2, verify3
from machine_qualification import abi

ROOT = Path(__file__).resolve().parents[1]
U256 = ROOT / "test-data/machine-code/u256-kernels"
NAMES = ("u256-add-x86", "u256-sub-x86", "u256-ult-x86", "u256-slt-x86")
MASK, FULL = (1 << 64) - 1, 1 << 256
NATIVE = platform.system() == "Linux" and platform.machine() == "x86_64" and abi._gcc() is not None


def entry(name):
    return json.loads((U256 / (name + ".json")).read_text())


def whole(words):
    return sum(word << (64 * i) for i, word in enumerate(words))


def signed(value):
    return value - FULL if value >> 255 else value


def meaning(name, a, b):
    """The intended 256-bit result: out words for add and sub, the 0/1 result for the compares."""
    x, y = whole(a), whole(b)
    if name == "u256-add-x86":
        return [((x + y) % FULL >> (64 * i)) & MASK for i in range(4)]
    if name == "u256-sub-x86":
        return [((x - y) % FULL >> (64 * i)) & MASK for i in range(4)]
    return int(x < y) if name == "u256-ult-x86" else int(signed(x) < signed(y))


class U256Kernels(unittest.TestCase):
    def test_every_entry_rechecks_at_its_recorded_cost(self):
        report = library.check(U256, timeout_ms=10_000)
        self.assertEqual(report["status"], "checked", report["kernels"])
        self.assertEqual({row["name"]: row["cost"]["bytes"] for row in report["kernels"]},
                         {"u256-add-x86": 58, "u256-sub-x86": 58, "u256-ult-x86": 49, "u256-slt-x86": 50})

    def test_each_specification_is_256_bit_arithmetic_for_all_inputs(self):
        for name in NAMES:
            problem = entry(name)["problem"]
            context = z3.Context()
            words = {r["name"]: [z3.BitVec(f"{r['name']}{i}", 64, ctx=context) for i in range(4)]
                     for r in problem["regions"]}
            env = {profile3.slot(n, i): v for n, ws in words.items() for i, v in enumerate(ws)}
            a, b = (z3.Concat(*reversed(words[n])) for n in ("a", "b"))
            outputs, memory = profile3.lifted(problem)
            if memory:
                total = a + b if name == "u256-add-x86" else a - b
                claims = [verify2._ast_smt(z3, e, env, context) == z3.Extract(64 * i + 63, 64 * i, total)
                          for i, e in enumerate(memory["out"])]
            else:
                less = z3.ULT(a, b) if name == "u256-ult-x86" else a < b
                claims = [verify2._ast_smt(z3, outputs[0], env, context)
                          == z3.If(less, z3.BitVecVal(1, 64, ctx=context), z3.BitVecVal(0, 64, ctx=context))]
            solver = z3.Solver(ctx=context)
            solver.add(z3.Not(z3.And(claims)))
            with self.subTest(kernel=name):
                self.assertEqual(solver.check(), z3.unsat)

    def test_the_reference_agrees_with_python_integers(self):
        rng = random.Random(20261030)
        edge = (0, 1, MASK, 1 << 63, MASK - 1)
        for name in NAMES:
            problem = entry(name)["problem"]
            for k in range(600):
                draw = (lambda: rng.choice(edge)) if k % 2 else (lambda: rng.getrandbits(64))
                a, b = [draw() for _ in range(4)], [draw() for _ in range(4)]
                if k % 5 == 0:
                    b[2:] = a[2:]
                results, writable = profile3.expected(problem, {}, {"a": a, "b": b, "out": [0] * 4})
                wanted = meaning(name, a, b)
                self.assertEqual(writable["out"] if "out" in writable else results[0], wanted, (name, a, b))

    def test_dropped_carry_and_unsigned_flag_are_refuted(self):
        add, slt = entry("u256-add-x86"), entry("u256-slt-x86")
        body = [ins.form() for ins in verify3.decode(verify3.X86, bytes.fromhex(add["code_hex"]))[:-1]]
        body[6] = ["add", "rax", "rcx"]  # limb 1 without the carry
        problem = dict(add["problem"], grammar=add["problem"]["grammar"] + [["add", "rax", "rcx"]])
        result = verify3.check(problem, targets.encode_program(verify3.X86, body), timeout_ms=10_000)
        self.assertEqual(result["status"], "counterexample")
        body = [ins.form() for ins in verify3.decode(verify3.X86, bytes.fromhex(slt["code_hex"]))[:-1]]
        body[-2] = ["setb", "rcx"]  # unsigned less instead of signed
        problem = dict(slt["problem"], grammar=slt["problem"]["grammar"] + [["setb", "rcx"]])
        result = verify3.check(problem, targets.encode_program(verify3.X86, body), timeout_ms=10_000)
        self.assertEqual(result["status"], "counterexample")

    @unittest.skipUnless(NATIVE, "actual Linux x86-64 host with gcc required")
    def test_native_runs_agree_and_the_calling_convention_gate_qualifies(self):
        report = library.check(U256, timeout_ms=10_000, native=True)
        self.assertEqual(report["status"], "checked")
        self.assertEqual({row["name"]: row["native"]["status"] for row in report["kernels"]},
                         dict.fromkeys(NAMES, "agreed"))
        for name in NAMES:
            value = entry(name)
            problem, code = value["problem"], bytes.fromhex(value["code_hex"])
            with self.subTest(kernel=name):
                self.assertEqual(abi.obligations(problem, code, timeout_ms=10_000)["status"], "qualified")
                with tempfile.TemporaryDirectory() as folder:
                    self.assertEqual(abi.native(problem, code, Path(folder) / "work", vectors=128)["status"],
                                     "conforms")

    @unittest.skipUnless(os.environ.get("ZENO_FCIS_CVC5"), "ZENO_FCIS_CVC5 names no pinned cvc5")
    def test_cvc5_confirms_every_equivalent_verdict(self):
        from machine_code import second_solver
        for name in NAMES:
            value = entry(name)
            confirmed = second_solver.confirm(value["problem"], bytes.fromhex(value["code_hex"]),
                                              Path(os.environ["ZENO_FCIS_CVC5"]), timeout_ms=60_000)
            with self.subTest(kernel=name):
                self.assertEqual((confirmed["status"], confirmed["second_solver"]["answer"]), ("equivalent", "unsat"))


if __name__ == "__main__":
    unittest.main()
