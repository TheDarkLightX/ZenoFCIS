"""Arithmetic contracts: a checked, saturating or signed meaning never becomes modular u64 silently."""

from contextlib import redirect_stdout
import io
import itertools
import json
from pathlib import Path
import tempfile
import unittest

from machine_code import contracts, model, profile2, session, targets, verify2
from machine_code.run import main

ROOT = Path(__file__).resolve().parents[1]
DATA = ROOT / "test-data/machine-code"
X86, RV64 = profile2.TARGETS
TOP = (1 << 64) - 1
PALETTE = [["xor", "rdx", "rdx"], ["mov", "rax", "rdi"], ["add", "rax", "rsi"], ["setb", "rdx"]]


def contract(meaning, operation="add", signedness="unsigned", bits=64, target=X86, grammar=PALETTE,
             max_instructions=4):
    return {"schema": contracts.SCHEMA, "target": target, "operation": operation, "signedness": signedness,
            "bits": bits, "meaning": meaning, "inputs": ["x", "y"], "grammar": grammar,
            "max_instructions": max_instructions, "objective": "bytes"}


def results(meaning, x, y, **fields):
    problem = contracts.lower(contract(meaning, **fields))["problem"]
    return [profile2.evaluate(output, {"x": x, "y": y}) for output in problem["outputs"]]


def oracle(meaning, operation, signedness, bits, target):
    """Independent routing table; no production helper is used."""
    known = {"meaning": ("modular", "checked", "saturating", "trapping"), "operation": ("add", "sub", "mul"),
             "signedness": ("unsigned", "signed"), "bits": (64, 128, 256)}
    for name, value in (("meaning", meaning), ("operation", operation), ("signedness", signedness),
                        ("bits", bits)):
        if value not in known[name]:
            return "unsupported", name
    if not target.endswith("/2"):
        return "unsupported", "target"
    if meaning == "trapping":
        return "missing-bridge", "effectful-overflow"
    if bits > 64:
        return "missing-bridge", "multiword"
    if (operation, meaning) in (("mul", "checked"), ("mul", "saturating")):
        return "missing-bridge", "high-product"
    return "profile-2", None


def library(name):
    return model.read_json(DATA / "kernels" / (name + ".json"))


class Routing(unittest.TestCase):
    def test_every_routing_case_matches_an_independent_table(self):
        domain = list(itertools.product((*contracts.MEANINGS, "wrapping"), (*contracts.OPERATIONS, "div"),
                                        (*contracts.SIGNEDNESS, "sign-magnitude"), (*contracts.WIDTHS, 32),
                                        (X86, RV64, "x86_64-sysv-register/1")))
        self.assertEqual(len(domain), 5 * 4 * 3 * 4 * 3)
        for case in domain:
            kind, detail = contracts.route(*case)
            expected_kind, expected_detail = oracle(*case)
            self.assertEqual(kind, expected_kind, case)
            if kind == "missing-bridge":
                self.assertEqual(detail, expected_detail, case)
            elif kind == "unsupported":
                self.assertIn(expected_detail, detail, case)

    def test_refusals_name_the_missing_bridge_or_the_exact_reason(self):
        for fields, route, bridge in (
                ({"meaning": "trapping"}, "missing-bridge", "effectful-overflow"),
                ({"meaning": "checked", "bits": 128}, "missing-bridge", "multiword"),
                ({"meaning": "modular", "bits": 256}, "missing-bridge", "multiword"),
                ({"meaning": "checked", "operation": "mul"}, "missing-bridge", "high-product"),
                ({"meaning": "saturating", "operation": "mul", "signedness": "signed"}, "missing-bridge",
                 "high-product"),
                ({"meaning": "checked", "target": "x86_64-sysv-register/1"}, "unsupported", None),
                ({"meaning": "rounding"}, "unsupported", None)):
            record = contracts.lower(contract(**fields))
            self.assertEqual((record["status"], record["route"], record["bridge"]), ("refused", route, bridge), fields)
            self.assertNotIn("problem", record)
            self.assertEqual(record["authority"], "none")
            self.assertEqual(record["reason"], contracts.BRIDGES[bridge] if bridge else contracts.route(
                record["contract"]["meaning"], record["contract"]["operation"], record["contract"]["signedness"],
                record["contract"]["bits"], record["contract"]["target"])[1])

    def test_malformed_contracts_are_refused_before_routing(self):
        for change in ({"inputs": ["x"]}, {"inputs": ["x", "y", "z"]}, {"bits": "64"}, {"meaning": 1},
                       {"schema": "zeno-fcis/machine-problem/2"}, {"extra": True}):
            with self.assertRaises(ValueError, msg=change):
                contracts.lower({**contract("checked"), **change})
        with self.assertRaises(ValueError, msg="a palette the target cannot encode"):
            contracts.lower(contract("checked", grammar=[["add", "rax", "r12"]]))
        bad = contract("checked")
        del bad["objective"]
        with self.assertRaises(ValueError):
            contracts.lower(bad)


class Meanings(unittest.TestCase):
    def test_max_plus_one_under_modular_checked_and_saturating(self):
        self.assertEqual(results("modular", TOP, 1), [0])
        self.assertEqual(results("checked", TOP, 1), [0, 1])
        self.assertEqual(results("saturating", TOP, 1), [TOP])
        # The same bits as signed words are -1 + 1, which does not overflow.
        self.assertEqual(results("checked", TOP, 1, signedness="signed"), [0, 0])
        self.assertEqual(results("saturating", TOP, 1, signedness="signed"), [0])
        # INT64_MAX + 1 overflows only as a signed sum, and clamps to INT64_MAX.
        high = (1 << 63) - 1
        self.assertEqual(results("checked", high, 1, signedness="signed"), [1 << 63, 1])
        self.assertEqual(results("saturating", high, 1, signedness="signed"), [high])
        self.assertEqual(results("checked", high, 1), [1 << 63, 0])
        self.assertEqual(results("checked", 0, 1, operation="sub"), [TOP, 1])
        self.assertEqual(results("saturating", 0, 1, operation="sub"), [0])

    def test_a_checked_contract_never_lowers_to_one_modular_result(self):
        for operation, signedness in itertools.product(("add", "sub"), contracts.SIGNEDNESS):
            fields = {"operation": operation, "signedness": signedness}
            modular, checked = (contracts.lower(contract(m, **fields)) for m in ("modular", "checked"))
            self.assertEqual(len(modular["problem"]["outputs"]), 1)
            self.assertEqual(len(checked["problem"]["outputs"]), 2, fields)
            self.assertEqual(checked["problem"]["outputs"][0], modular["problem"]["outputs"][0])
            self.assertIn(checked["problem"]["outputs"][1][0], ("ult", "slt"), "a 0/1 status word")
            self.assertNotEqual(checked["problem_sha256"], modular["problem_sha256"])
            self.assertEqual(checked["status_word"], contracts.STATUS)
            self.assertIn("Read result 2 first", checked["status_word"]["precedence"])
            self.assertIsNone(modular["status_word"])

    def test_every_lowering_equals_its_mathematical_meaning_for_all_inputs(self):
        """Z3, over all 2^128 pairs: the checker's expression semantics against wide exact arithmetic."""
        import z3 as z
        context = z.Context()
        x, y = z.BitVec("x", 64, ctx=context), z.BitVec("y", 64, ctx=context)
        for operation, signedness, meaning in itertools.product(contracts.OPERATIONS, contracts.SIGNEDNESS,
                                                                ("modular", "checked", "saturating")):
            if contracts.route(meaning, operation, signedness, 64, X86)[0] != "profile-2":
                continue
            extend = (lambda v: z.ZeroExt(66, v)) if signedness == "unsigned" else (lambda v: z.SignExt(66, v))
            exact = {"add": extend(x) + extend(y), "sub": extend(x) - extend(y), "mul": extend(x) * extend(y)}[operation]
            low, high = (0, TOP) if signedness == "unsigned" else (-(1 << 63), (1 << 63) - 1)
            # Extended operands make every exact result a signed 130-bit integer, so signed order is exact.
            bound = lambda value: z.BitVecVal(value, 130, ctx=context)
            fits = z.And(exact >= bound(low), exact <= bound(high))
            wrapped = z.Extract(63, 0, exact)
            if meaning == "modular":
                expected = [wrapped]
            elif meaning == "checked":
                expected = [wrapped, z.If(fits, z.BitVecVal(0, 64, ctx=context), z.BitVecVal(1, 64, ctx=context))]
            else:
                clamp = z.If(exact < bound(low), z.BitVecVal(low % (1 << 64), 64, ctx=context),
                             z.BitVecVal(high % (1 << 64), 64, ctx=context))
                expected = [z.If(fits, wrapped, clamp)]
            problem = contracts.lower(contract(meaning, operation=operation, signedness=signedness))["problem"]
            actual = [verify2._ast_smt(z, output, {"x": x, "y": y}, context) for output in problem["outputs"]]
            solver = z.Solver(ctx=context)
            solver.add(z.Or([e != a for e, a in zip(expected, actual)]))
            self.assertEqual(len(actual), len(expected))
            self.assertEqual(solver.check(), z.unsat, (operation, signedness, meaning))


class Kernels(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="zeno-machine-contracts-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)

    def code(self, *instructions):
        return targets.encode_program(X86, list(instructions))

    def run_cli(self, *argv):
        output = io.StringIO()
        with redirect_stdout(output):
            code = main(list(argv))
        return code, json.loads(output.getvalue())

    def test_a_kernel_correct_for_modular_fails_the_checked_contract(self):
        # Correct wrapped sum, and a status word that never reports overflow.
        kernel = self.code(["xor", "rdx", "rdx"], ["mov", "rax", "rdi"], ["add", "rax", "rsi"])
        self.assertEqual(contracts.check(contract("modular"), kernel)["status"], "equivalent")
        failed = contracts.check(contract("checked"), kernel)
        self.assertEqual(failed["status"], "counterexample")
        self.assertEqual([failed["expected"][1], failed["actual"][1]], ["1", "0"])
        self.assertEqual(verify2.run_concrete(X86, kernel, [TOP, 1], results=2), [0, 0])
        self.assertEqual(results("checked", TOP, 1), [0, 1])
        self.assertEqual(contracts.check(contract("saturating"), kernel)["status"], "counterexample")
        carry = self.code(*PALETTE)
        self.assertEqual(contracts.check(contract("checked"), carry)["status"], "equivalent")
        self.assertEqual(verify2.run_concrete(X86, carry, [TOP, 1], results=2), [0, 1])

    def test_cli_lowers_refuses_and_checks(self):
        paths = {}
        for name, value in (("checked", contract("checked")), ("trapping", contract("trapping"))):
            paths[name] = self.root / (name + ".json")
            paths[name].write_bytes(model.canonical(value))
        problem = self.root / "problem.json"
        code, lowered = self.run_cli("contract", "lower", "--contract", str(paths["checked"]),
                                     "--problem-out", str(problem))
        self.assertEqual((code, lowered["status"]), (0, "lowered"))
        self.assertEqual(model.digest(model.canonical(model.read_json(problem))), lowered["problem_sha256"])
        self.assertEqual(self.run_cli("contract", "lower", "--contract", str(paths["checked"]),
                                      "--problem-out", str(problem))[0], 65, "never overwrites")
        code, refused = self.run_cli("contract", "lower", "--contract", str(paths["trapping"]))
        self.assertEqual((code, refused["status"], refused["bridge"]), (1, "refused", "effectful-overflow"))
        kernel = self.root / "kernel.bin"
        kernel.write_bytes(self.code(["xor", "rdx", "rdx"], ["mov", "rax", "rdi"], ["add", "rax", "rsi"]))
        code, checked = self.run_cli("contract", "check", "--contract", str(paths["checked"]), "--code", str(kernel))
        self.assertEqual((code, checked["status"]), (1, "counterexample"))
        self.assertEqual(checked["contract_problem_sha256"], lowered["contract_problem_sha256"])
        session.open_session(problem, self.root / "session")


class Identities(unittest.TestCase):
    def test_library_problems_keep_their_identities_and_contracts_reproduce_three(self):
        for path in [*sorted((DATA / "kernels").glob("*.json")), *sorted(DATA.glob("*.json"))]:
            value = model.read_json(path)
            problem = value["problem"] if "problem" in value else value
            self.assertEqual(model.canonical(session._validated(problem)), model.canonical(problem), path.name)
        for name, fields in (("add-carry-out-x86", {"meaning": "checked"}),
                             ("saturating-add-x86", {"meaning": "saturating"}),
                             ("signed-saturating-add-x86", {"meaning": "saturating", "signedness": "signed"})):
            stored = library(name)["problem"]
            record = contracts.lower(contract(grammar=stored["grammar"], max_instructions=stored["max_instructions"],
                                              **fields))
            self.assertEqual(record["problem_sha256"], model.digest(model.canonical(stored)), name)

    def test_the_contract_is_bound_into_the_identity(self):
        unsigned, signed = (contracts.lower(contract("modular", signedness=s)) for s in contracts.SIGNEDNESS)
        self.assertEqual(unsigned["problem_sha256"], signed["problem_sha256"], "the same word function")
        self.assertNotEqual(unsigned["contract_problem_sha256"], signed["contract_problem_sha256"])
        self.assertNotEqual(unsigned["contract_sha256"], signed["contract_sha256"])
        self.assertEqual(unsigned["contract_problem_sha256"], model.digest(model.canonical(
            {"contract_sha256": unsigned["contract_sha256"], "problem_sha256": unsigned["problem_sha256"]})))


if __name__ == "__main__":
    unittest.main()
