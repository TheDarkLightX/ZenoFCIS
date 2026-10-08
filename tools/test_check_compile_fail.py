"""Planted controls for the compile-fail diagnostic check.

Each control alters one example in a copy of the shell's delivery module and
requires the check to refuse it: a misuse example that fails for another
reason, one that fails for its reason and another, one that compiles, a
compiling example that stops compiling, and documentation whose stated codes
or pairing drift. The real module must pass.
"""

import unittest

import check_compile_fail as gate

SOURCE = gate.ROOT / "crates" / gate.PACKAGE / "src" / "v2" / "delivery.rs"
NAME = SOURCE.relative_to(gate.ROOT).as_posix()


def altered(old: str, new: str) -> str:
    """The delivery module with one exact, unique piece of text replaced."""

    text = SOURCE.read_text(encoding="utf-8")
    if text.count(old) != 1:
        raise AssertionError(f"control anchor is not unique: {old!r}")
    return text.replace(old, new)


def blocks(text: str) -> list[gate.Block]:
    found = gate.extract(text, NAME)
    gate.pair(found)
    return found


class Documentation(unittest.TestCase):
    """The examples are read from the doc comments rustdoc compiles."""

    def test_reads_every_example_and_its_pairing(self):
        found = blocks(SOURCE.read_text(encoding="utf-8"))
        failing = [block for block in found if block.expected]
        self.assertEqual(len(found), 14)
        self.assertEqual(sorted(block.expected for block in failing),
                         ["E0382", "E0382", "E0451", "E0451", "E0499", "E0505", "E0599"])
        lines = {block.line for block in found if not block.expected}
        self.assertEqual(sorted(block.pair for block in failing),
                         sorted(lines))
        # Hidden `# ` lines are compiled, without the marker.
        for block in found:
            self.assertIn("use zeno_fcis_shell_sqlite::", block.code)
            self.assertFalse(any(line.startswith("#") for line in block.code.splitlines()))

    def test_the_compiled_text_is_the_documented_text(self):
        # An independent reading of the doc comment: drop `/// `, then the
        # `# ` that hides a line from the rendered page.
        text = SOURCE.read_text(encoding="utf-8").splitlines()
        for block in blocks("\n".join(text)):
            body = []
            for line in text[block.line:]:
                line = line.strip().removeprefix("///").removeprefix(" ")
                if line == "```":
                    break
                body.append(line[2:] if line.startswith("# ") else line)
            self.assertEqual(block.code, "\n".join(body) + "\n")

    def test_refuses_drift_between_prose_fence_and_pairing(self):
        end_of_first = ("///         pending.deliver(destination)?.acknowledge()?;\n"
                        "///     }\n///     Ok(())\n/// }\n/// ```\n")
        controls = {
            "no stated code": ("/// ```compile_fail,E0599", "/// ```compile_fail",
                               "exactly one error code"),
            "two stated codes": ("/// ```compile_fail,E0599", "/// ```compile_fail,E0599,E0061",
                                 "exactly one error code"),
            "code without compile_fail": ("/// ```compile_fail,E0599", "/// ```E0599",
                                          "without compile_fail"),
            "prose names another code": ("(E0505).", "(E0506).", "the prose names"),
            "fence states another code": ("/// ```compile_fail,E0505",
                                          "/// ```compile_fail,E0506", "the prose names"),
            # A second misuse example after the first, with its own prose:
            # the first one's compiling example is no longer directly before it.
            "unpaired misuse example": (
                end_of_first,
                end_of_first + "///\n/// Another misuse (E0599).\n///\n"
                "/// ```compile_fail,E0599\n/// fn f() {}\n/// ```\n",
                "must directly follow"),
            "unsupported attribute": ("/// ```compile_fail,E0599", "/// ```ignore",
                                      "unsupported"),
        }
        for label, (old, new, reason) in controls.items():
            with self.subTest(label):
                with self.assertRaises(gate.CheckError) as refused:
                    blocks(altered(old, new))
                self.assertIn(reason, str(refused.exception))


class PlantedControls(unittest.TestCase):
    """Altered examples, compiled on the pinned stable toolchain."""

    @classmethod
    def setUpClass(cls):
        cls.environment = gate.environment(gate.ROOT)

    def problems(self, text: str) -> dict[int, str]:
        results = gate.check(blocks(text), self.environment)
        return {block.line: problem for block, problem in results if problem}

    def test_the_real_examples_pass(self):
        self.assertEqual(self.problems(SOURCE.read_text(encoding="utf-8")), {})

    def assert_one_problem(self, text: str, *expected: str) -> None:
        problems = self.problems(text)
        self.assertEqual(len(problems), 1, problems)
        [problem] = problems.values()
        for part in expected:
            self.assertIn(part, problem)

    def test_a_misuse_example_failing_for_another_reason_is_refused(self):
        # A method called with a missing argument fails with E0061, not E0599.
        self.assert_one_problem(
            altered("///         pending.acknowledge()?;", "///         pending.deliver()?;"),
            "must fail with exactly [E0599]", "E0061")

    def test_a_misuse_example_with_an_extra_error_is_refused(self):
        # A second borrow-check error (E0384) is reported with the stated one;
        # a type error would stop rustc before borrow checking instead.
        self.assert_one_problem(
            altered("///         let again = pending.deliver(destination)?;",
                    "///         let again = pending.deliver(destination)?;\n"
                    "///         let fixed = 1;\n///         fixed = 2;"),
            "must fail with exactly [E0382]", "'E0382'", "'E0384'")

    def test_a_misuse_example_masked_by_a_type_error_is_refused(self):
        self.assert_one_problem(
            altered("///         let again = pending.deliver(destination)?;",
                    "///         let again = pending.deliver(destination)?;\n"
                    "///         let wrong: u8 = \"text\";"),
            "must fail with exactly [E0382]", "['E0308']")

    def test_a_misuse_example_that_compiles_is_refused(self):
        self.assert_one_problem(
            altered("///         delivered.acknowledge()?;\n///         delivered.acknowledge()?;\n",
                    "///         delivered.acknowledge()?;\n"),
            "compiles, but must fail with E0382")

    def test_a_compiling_example_that_stops_compiling_is_refused(self):
        self.assert_one_problem(
            altered("///         pending.deliver(destination)?.acknowledge()?;",
                    "///         pending.deliver(destination)?.acknowledged()?;"),
            "does not compile", "E0599")

    def test_a_restated_code_must_match_the_compiler(self):
        self.assert_one_problem(
            altered("/// ```compile_fail,E0505", "/// ```compile_fail,E0506").replace(
                "(E0505).", "(E0506)."),
            "must fail with exactly [E0506]", "E0505")


if __name__ == "__main__":
    unittest.main()
