"""Per-result backward slices: each slice is checked as a one-result kernel, and a wrong slice is caught."""

from contextlib import redirect_stdout
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from machine_code import library, model, slicing, verify2
from machine_code.run import main as cli

ROOT = Path(__file__).resolve().parents[1]
KERNELS = ROOT / "test-data/machine-code/kernels"


def kernel(name):
    return library._validate(json.loads((KERNELS / (name + ".json")).read_text()))


def main(argv):
    output = io.StringIO()
    with redirect_stdout(output):
        status = cli(argv)
    return status, json.loads(output.getvalue())


class Slicing(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="zeno-slice-")
        self.root = Path(self.temporary.name)

    def tearDown(self):
        self.temporary.cleanup()

    def test_every_library_kernel_slices_into_checked_one_result_kernels(self):
        seen = 0
        for path in sorted(KERNELS.glob("*.json")):
            problem, code = library._validate(json.loads(path.read_text()))
            if problem["schema"] != "zeno-fcis/machine-problem/2":
                continue
            with self.subTest(kernel=path.stem):
                result = slicing.slices(problem, code)
                self.assertEqual(result["status"], "sliced", result)
                self.assertEqual(len(result["slices"]), len(problem["outputs"]))
                for row in result["slices"]:
                    self.assertEqual(row["verification"]["status"], "equivalent")
                    self.assertEqual(row["problem"]["outputs"], [problem["outputs"][row["result"]]])
                seen += 1
        self.assertGreaterEqual(seen, 9)

    def test_the_low_word_of_a_128_bit_add_needs_only_its_own_instructions(self):
        problem, code = kernel("add128-x86")
        out = self.root / "add128"
        status, printed = main(["slice", "--kernel", str(KERNELS / "add128-x86.json"), "--out", str(out)])
        self.assertEqual((status, printed["status"]), (0, "sliced"), printed)
        record = json.loads((out / "slices.json").read_text())
        self.assertEqual([row["instructions"] for row in record["slices"]], [[0, 1], [0, 1, 2, 3]])
        self.assertEqual(record["original_problem_sha256"], model.digest(model.canonical(problem)))
        self.assertEqual(record["original_code_sha256"], model.digest(code))
        for row in record["slices"]:
            name = f"result{row['result']}"
            one = json.loads((out / (name + ".json")).read_text())
            sliced = (out / (name + ".bin")).read_bytes()
            self.assertEqual(row["problem_sha256"], model.digest(model.canonical(one)))
            self.assertEqual(row["code_sha256"], model.digest(sliced))
            self.assertEqual(verify2.check(one, sliced)["status"], "equivalent")
        # The second result's slice ends with a copy into the first result register.
        forms = [i.form() for i in verify2.decode(problem["target"], (out / "result1.bin").read_bytes())[:-1]]
        self.assertEqual(forms[-1], ["mov", "rax", "rdx"])
        status, printed = main(["slice", "--kernel", str(KERNELS / "add128-x86.json"), "--out", str(out)])
        self.assertEqual((status, printed["status"]), (65, "refused"))

    def test_a_planted_footprint_defect_is_caught_by_the_checker(self):
        # Dropping ADC's carry read loses the low ADD from the high word's slice.
        problem, code = kernel("add128-x86")
        real = slicing.footprint

        def no_carry(target, form):
            reads, writes, flag_reads, defined, undefined = real(target, form)
            return reads, writes, (() if form[0] == "adc" else flag_reads), defined, undefined
        out = self.root / "planted"
        with mock.patch.object(slicing, "footprint", no_carry):
            result = slicing.slices(problem, code)
            status, printed = main(["slice", "--kernel", str(KERNELS / "add128-x86.json"), "--out", str(out)])
        self.assertEqual(result["status"], "counterexample")
        self.assertEqual(result["slices"][1]["instructions"], [2, 3])
        self.assertEqual(result["slices"][1]["verification"]["reason"], "Full64Mismatch")
        self.assertEqual((status, printed["status"]), (1, "counterexample"))
        self.assertFalse(out.exists())

    def test_an_unchecked_kernel_or_profile1_problem_is_refused(self):
        problem, code = kernel("add128-x86")
        wrong = code[:-2] + bytes([code[-2] ^ 1]) + code[-1:]
        self.assertEqual(slicing.slices(problem, wrong)["reason"], "KernelDidNotCheck")
        profile1, code1 = kernel("add64-x86")
        with self.assertRaisesRegex(ValueError, "Profile2Only"):
            slicing.slices(profile1, code1)


if __name__ == "__main__":
    unittest.main()
