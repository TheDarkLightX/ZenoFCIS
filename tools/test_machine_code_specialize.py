"""Exact specialization: a proof failure writes nothing, and only the checker accepts a rewritten kernel."""

from contextlib import redirect_stdout
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from machine_code import library, model, specialize, verify2
from machine_code.run import main as cli

ROOT = Path(__file__).resolve().parents[1]
KERNELS = ROOT / "test-data/machine-code/kernels"
MASK = (1 << 64) - 1


def kernel(name):
    return library._validate(json.loads((KERNELS / (name + ".json")).read_text()))


def main(argv):
    output = io.StringIO()
    with redirect_stdout(output):
        status = cli(argv)
    return status, json.loads(output.getvalue())


class Specialization(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="zeno-specialize-")
        self.root = Path(self.temporary.name)

    def tearDown(self):
        self.temporary.cleanup()

    def test_a_specialization_proof_failure_writes_nothing(self):
        problem, _ = kernel("add128-x86")
        wrong = lambda expr: ["add", expr, ["const", "1"]]
        out = self.root / "wrong"
        with mock.patch.object(specialize, "simplify", wrong):
            result = specialize.specialize(problem, ["a_hi=0"], out)
        self.assertEqual(result["status"], "counterexample")
        self.assertEqual(result["reason"], "SpecializationProofFailed")
        self.assertEqual(result["proof"]["answer"], "sat")
        self.assertFalse(out.exists())
        path = self.root / "problem.json"
        path.write_bytes(model.canonical(problem))
        with mock.patch.object(specialize, "simplify", wrong):
            status, printed = main(["specialize", "--problem", str(path), "--fix", "a_hi=0", "--out", str(out)])
        self.assertEqual((status, printed["status"]), (1, "counterexample"))
        self.assertFalse(out.exists())

    def test_a_specialized_library_kernel_checks_against_its_specialized_problem(self):
        for name, fixes in (("add128-x86", ["a_hi=0", "b_hi=0"]), ("add128-rv64i", ["a_lo=5"]),
                            ("saturating-add-x86", ["y=1"]), ("sub128-x86", ["b_lo=18446744073709551615"])):
            with self.subTest(name=name, fixes=fixes):
                out = self.root / name
                args = ["specialize", "--kernel", str(KERNELS / (name + ".json")), "--out", str(out)]
                status, printed = main(args + [x for fix in fixes for x in ("--fix", fix)])
                self.assertEqual((status, printed["status"]), (0, "specialized"), printed)
                record = json.loads((out / "specialization.json").read_text())
                original, code = kernel(name)
                special = json.loads((out / "problem.json").read_text())
                special_code = (out / "kernel.bin").read_bytes()
                # The record names both problem hashes and both kernel hashes.
                self.assertEqual(record["original_problem_sha256"], model.digest(model.canonical(original)))
                self.assertEqual(record["specialized_problem_sha256"], model.digest(model.canonical(special)))
                self.assertEqual(record["kernel"]["original_code_sha256"], model.digest(code))
                self.assertEqual(record["kernel"]["specialized_code_sha256"], model.digest(special_code))
                self.assertEqual(record["proof"]["answer"], "unsat")
                # The remaining inputs keep their order and are renumbered from the first argument.
                fixed = {fix.split("=")[0] for fix in fixes}
                self.assertEqual(special["inputs"], [n for n in original["inputs"] if n not in fixed])
                # An independent recheck with the existing checker, and a recomputed replay.
                self.assertEqual(verify2.check(special, special_code)["status"], "equivalent")
                status, printed = main(["specialize-replay", "--dir", str(out)])
                self.assertEqual((status, printed["status"]), (0, "replayed"), printed)
                # Concrete agreement with the original kernel at the fixed values.
                values = {fix.split("=")[0]: int(fix.split("=")[1]) for fix in fixes}
                for sample in ([0] * 6, [MASK] * 6, [3, 1 << 63, 7, 12345, 1, 2]):
                    rest = sample[:len(special["inputs"])]
                    env = dict(zip(special["inputs"], rest))
                    full = [values.get(n, env.get(n)) for n in original["inputs"]]
                    self.assertEqual(
                        verify2.run_concrete(special["target"], special_code, rest, results=len(special["outputs"])),
                        verify2.run_concrete(original["target"], code, full, results=len(original["outputs"])))

    def test_fixing_every_input_gives_a_constant_kernel(self):
        problem, _ = kernel("add128-x86")
        out = self.root / "constant"
        fixes = ["a_lo=18446744073709551615", "a_hi=7", "b_lo=1", "b_hi=9"]
        status, printed = main(["specialize", "--kernel", str(KERNELS / "add128-x86.json"), "--out", str(out)]
                               + [x for fix in fixes for x in ("--fix", fix)])
        self.assertEqual((status, printed["status"]), (0, "specialized"), printed)
        special = json.loads((out / "problem.json").read_text())
        self.assertEqual(special["inputs"], [])
        self.assertEqual(special["outputs"], [["const", "0"], ["const", "17"]])
        forms = [i.form() for i in verify2.decode(special["target"], (out / "kernel.bin").read_bytes())[:-1]]
        self.assertTrue(all(form[0] in ("imm", "xor") for form in forms), forms)
        self.assertEqual(verify2.check(special, (out / "kernel.bin").read_bytes())["status"], "equivalent")
        self.assertEqual(verify2.run_concrete(special["target"], (out / "kernel.bin").read_bytes(), [], results=2),
                         [0, 17])

    def test_a_planted_wrong_constant_fold_is_caught_by_the_checker(self):
        problem, code = kernel("add128-x86")
        real = specialize.fold

        def off_by_one(target, form, registers, flags):
            values, flag_values = real(target, form, registers, flags)
            return {name: (value + 1) & MASK for name, value in values.items()}, flag_values
        out = self.root / "planted"
        with mock.patch.object(specialize, "fold", off_by_one):
            result = specialize.specialize(problem, ["a_hi=0", "b_hi=0"], out, code)
        self.assertEqual(result["status"], "counterexample", result)
        self.assertEqual(result["reason"], "SpecializedKernelDidNotCheck")
        self.assertEqual(result["verification"]["reason"], "Full64Mismatch")
        self.assertFalse(out.exists())

    def test_fixes_and_profiles_are_refused_exactly(self):
        problem, code = kernel("add128-x86")
        for fixes, reason in ((["c=1"], "FixedNameIsNotAnInput"), (["a_hi=1", "a_hi=2"], "InputFixedTwice"),
                              (["a_hi=01"], "canonical"), (["a_hi=18446744073709551616"], "exceeds"),
                              (["a_hi"], "FixMustBeNameEqualsValue"), ([], "SpecializationNeedsAFixedInput")):
            with self.subTest(fixes=fixes), self.assertRaisesRegex(ValueError, reason):
                specialize.specialize(problem, fixes, None)
        profile1, _ = kernel("add64-x86")
        with self.assertRaisesRegex(ValueError, "Profile2ProblemsOnly"):
            specialize.specialize(profile1, ["x=1"], None)
        out = self.root / "exists"
        out.mkdir()
        status, printed = main(["specialize", "--kernel", str(KERNELS / "add128-x86.json"), "--fix", "a_hi=0",
                                "--out", str(out)])
        self.assertEqual((status, printed["status"]), (65, "refused"))
        wrong = code[:-2] + bytes([code[-2] ^ 1]) + code[-1:]
        result = specialize.specialize(problem, ["a_hi=0"], None, wrong)
        self.assertEqual((result["status"], result["reason"]), ("refused", "OriginalKernelDidNotCheck"))

    def test_every_library_kernel_specializes_over_every_input_subset(self):
        """Each rewrite either checks equivalent or is refused for size before checking; never a counterexample."""
        import itertools
        import random
        words = [0, 1, 2047, 2048, 1 << 63, MASK]
        rng = random.Random("specialize")
        outcomes = {}
        for path in sorted(KERNELS.glob("*.json")):
            problem, code = library._validate(json.loads(path.read_text()))
            if problem["schema"] != "zeno-fcis/machine-problem/2":
                continue
            names = problem["inputs"]
            for size in range(1, len(names) + 1):
                for subset in itertools.combinations(names, size):
                    fixes = [f"{n}={rng.choice(words + [rng.getrandbits(64)])}" for n in subset]
                    try:
                        status = specialize.specialize(problem, fixes, None, code)["status"]
                    except ValueError as error:
                        status = str(error)
                    with self.subTest(kernel=path.stem, fixes=fixes):
                        self.assertIn(status, ("specialized", "SpecializedKernelExceeds16Instructions"))
                    outcomes[status] = outcomes.get(status, 0) + 1
        self.assertGreaterEqual(outcomes.get("specialized", 0), 55, outcomes)

    def test_problem_identities_simplify_exactly(self):
        x = ["input", "x"]
        cases = [(["add", x, ["const", "0"]], x), (["mul", ["const", "1"], x], x), (["and", x, ["const", "0"]], ["const", "0"]),
                 (["ite", ["const", "2"], x, ["const", "5"]], x), (["shl", x, 0], x),
                 (["sub", ["const", "3"], ["const", "5"]], ["const", str(MASK - 1)])]
        for expr, expected in cases:
            with self.subTest(expr=expr):
                self.assertEqual(specialize.simplify(expr), expected)

    def test_replay_refuses_a_changed_record(self):
        out = self.root / "replayed"
        status, _ = main(["specialize", "--kernel", str(KERNELS / "add128-x86.json"), "--fix", "b_hi=3", "--out", str(out)])
        self.assertEqual(status, 0)
        record = json.loads((out / "specialization.json").read_text())
        record["fixed"]["b_hi"] = "4"
        (out / "specialization.json").write_bytes(model.canonical(record) + b"\n")
        status, printed = main(["specialize-replay", "--dir", str(out)])
        self.assertEqual((status, printed["status"]), (65, "refused"), printed)


if __name__ == "__main__":
    unittest.main()
