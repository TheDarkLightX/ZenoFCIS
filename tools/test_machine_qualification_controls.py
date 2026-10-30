"""The planted-defect campaign's registry, specimens and outcome classes, on small real specimens.

The complete campaign runs separately (`python3 -B -m tools.machine_qualification.controls run`); these tests check
that its outcome classes cannot be confused: a test-side assertion is a detection, while a crash, a skip, a timeout,
a checker-side assertion and an identity refusal are not.
"""

import ast
import hashlib
from pathlib import Path
import subprocess
import sys
import tempfile
import textwrap
import unittest

from machine_qualification import controls

ROOT = Path(__file__).resolve().parents[1]
MODE = tuple(["-O", "-B"] if sys.flags.optimize else ["-B"])
BRIEF_KINDS = {"operand width", "immediate sign extension", "scratch quantification", "dropped instruction",
               "shift count", "zero register", "solver result", "counterexample replay"}
FLAG_KINDS = {"flag rule CF", "flag rule ZF", "flag rule SF", "flag rule OF"}


def defect(name, anchor, replacement, control, file="verify.py"):
    return controls.Defect(name, file, "test", anchor, replacement, control, "planted by this test")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


class Registry(unittest.TestCase):
    def test_every_edit_applies_once_and_names_an_existing_control(self):
        names = [d.name for d in controls.DEFECTS]
        self.assertEqual(len(names), len(set(names)))
        methods = {}
        for module in controls.TEST_MODULES:
            tree = ast.parse((ROOT / "tools" / module).read_text())
            for node in tree.body:
                if isinstance(node, ast.ClassDef):
                    for item in node.body:
                        if isinstance(item, ast.FunctionDef) and item.name.startswith("test_"):
                            methods[f"{module[:-3]}.{node.name}.{item.name}"] = module
        for d in controls.DEFECTS:
            with self.subTest(defect=d.name):
                self.assertIn(d.file, ("verify.py", "verify2.py", "verify3.py", "realize.py", "realize_rules.py"))
                source = (ROOT / "tools/machine_code" / d.file).read_text()
                self.assertEqual(source.count(d.anchor), 1)
                self.assertNotEqual(d.anchor, d.replacement)
                self.assertIn(d.control, methods)
                if d.second is not None:
                    self.assertIn(d.second, methods)
                    self.assertNotEqual(d.second, d.control)
        kinds = {d.kind for d in controls.DEFECTS}
        self.assertLessEqual(BRIEF_KINDS | FLAG_KINDS, kinds)
        self.assertEqual({d.file for d in controls.DEFECTS}, {"verify.py", "verify2.py", "verify3.py", "realize.py", "realize_rules.py"})

    def test_defects_once_left_to_hardware_conformance_have_a_unit_control_first(self):
        # These three edits pass the verifier suites as they were before their unit controls; conformance stays
        # registered as their second control.
        for name in ("v2-rv-sra-count-five-bits", "v2-x86-sign-flag-bit-62", "v2-x86-le-ignores-zero-flag-in-smt"):
            d = next(d for d in controls.DEFECTS if d.name == name)
            with self.subTest(defect=name):
                self.assertTrue(d.control.startswith(controls.V2), d.control)
                self.assertTrue(d.second and d.second.startswith(controls.CONFORMANCE2), d.second)

    def test_positive_controls_include_every_library_kernel_and_both_verifier_suites(self):
        self.assertEqual({name for _, name in controls.POSITIVE},
                         {"test_machine_code_verifier", "test_machine_code_profile2_verifier",
                          "test_machine_code_library", "test_machine_code_profile3_verifier"})
        self.assertEqual(controls.MODES, (("-B",), ("-O", "-B")))


class Classification(unittest.TestCase):
    def setUp(self):
        self.directory = Path("/specimen")
        self.package = str(self.directory / "tools/machine_code")
        self.binding = {"edited": "verify.py", "edited_sha256": "e" * 64}

    def outcome(self, **given):
        value = {"run": 1, "failures": [], "errors": [], "skipped": [], "package": self.package,
                 "identity_sources": {"verify.py": "e" * 64}}
        return {**value, **given}

    def test_only_a_test_assertion_on_the_specimen_is_a_detection(self):
        failure = [{"test": "t", "message": "'equivalent' != 'counterexample'"}]
        cases = [
            (self.outcome(), "survived"),
            (self.outcome(failures=failure), "detected"),
            (self.outcome(errors=[{"test": "t", "message": "ValueError: boom"}]), "inconclusive"),
            (self.outcome(skipped=[{"test": "t", "message": "missing tool"}]), "inconclusive"),
            (self.outcome(failures=[{"test": "t", "message": "session artifact identity changed"}]), "inconclusive"),
            (self.outcome(failures=[{"test": "t", "message": "session problem/checker/solver binding drifted"}]),
             "inconclusive"),
            ({"timeout": True, "seconds": 900}, "inconclusive"),
            (self.outcome(package="/repository/tools/machine_code"), "invalid-specimen"),
            (self.outcome(run=0), "invalid-specimen"),
            (self.outcome(identity_sources={"verify.py": "0" * 64}), "invalid-specimen"),
            (self.outcome(identity_error="ValueError: no z3 library"), "invalid-specimen"),
            ({"load_error": "ImportError: x"}, "invalid-specimen"),
            ({"harness_error": "Traceback"}, "invalid-specimen"),
        ]
        for outcome, expected in cases:
            with self.subTest(outcome=outcome):
                self.assertEqual(controls.classify(outcome, self.directory, self.binding)[0], expected)
        self.assertEqual(controls.classify({}, self.directory, {"invalid": "anchor occurs 0 times"})[0],
                         "invalid-specimen")


class Specimens(unittest.TestCase):
    """Real specimens of the checker package, each run in its own interpreter."""

    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="zeno-controls-test-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.sources = {name: digest(ROOT / "tools/machine_code" / name) for name in ("verify.py", "verify2.py")}

    def tearDown(self):
        # Specimens are copies: the repository's checker never changes.
        self.assertEqual({name: digest(ROOT / "tools/machine_code" / name) for name in self.sources}, self.sources)

    def judge(self, planted, name="specimen", timeout=controls.TIMEOUT):
        specimen = self.root / name
        binding = controls.build_specimen(specimen, planted)
        outcome = {} if "invalid" in binding else controls.run_control(specimen, planted.control, MODE, timeout)
        return controls.classify(outcome, specimen, binding)

    def test_outcome_classes_on_planted_specimens(self):
        unknown = next(d for d in controls.DEFECTS if d.name == "v1-solver-unknown-read-as-equivalent")
        control = unknown.control
        cases = [
            (unknown, "detected"),
            (defect("comment-only", "def decode(target, code):", "def decode(target, code):  # planted no-op",
                    control), "survived"),
            (defect("missing-anchor", "this text is not in the checker", "x", control), "invalid-specimen"),
            (defect("syntax-error", "def decode(target, code):", "def decode(target, code)", control),
             "invalid-specimen"),
            (defect("checker-side-assertion", '    """Consume an entire admitted function, including its unique exact '
                    'return."""', '    """Consume an entire admitted function."""\n'
                    '    raise AssertionError("planted checker-side assertion")',
                    controls.V1 + "test_x86_add_sub_bitwise_and_full_width_imul"), "inconclusive"),
            (defect("unknown-control", "def decode(target, code):", "def decode(target, code):  # no-op",
                    controls.V1 + "test_that_does_not_exist"), "invalid-specimen"),
        ]
        for index, (planted, expected) in enumerate(cases):
            with self.subTest(planted=planted.name):
                outcome, evidence = self.judge(planted, f"case-{index}")
                self.assertEqual(outcome, expected, evidence)
                if planted.name == "checker-side-assertion":
                    self.assertTrue(evidence.startswith("crash:"), evidence)

    def test_a_timeout_is_inconclusive(self):
        planted = defect("sleeps", "def decode(target, code):",
                         "def decode(target, code):\n    __import__('time').sleep(60)",
                         controls.V1 + "test_x86_add_sub_bitwise_and_full_width_imul")
        outcome, evidence = self.judge(planted, timeout=5)
        self.assertEqual(outcome, "inconclusive")
        self.assertTrue(evidence.startswith("timeout"), evidence)

    def test_an_identity_refusal_is_never_a_detection(self):
        # A session opened by the unmodified checker refuses the edited one; that refusal says nothing about
        # the edit's semantics, so the control that hits it is inconclusive.
        specimen = self.root / "identity"
        controls.build_specimen(specimen, None)
        tools = specimen / "tools"
        (tools / "test_identity_probe.py").write_text(textwrap.dedent(f"""\
            import unittest
            from pathlib import Path
            from machine_code import session
            class Probe(unittest.TestCase):
                def test_resume(self):
                    try:
                        session.resume(Path({str(specimen / "session")!r}))
                    except ValueError as error:
                        self.fail(str(error))
            """))
        subprocess.run([sys.executable, *MODE, "-c", "import sys; from pathlib import Path; "
                        "from machine_code import session; session.open_session(Path(sys.argv[1]), Path(sys.argv[2]))",
                        str(specimen / "test-data/machine-code/add64-x86.json"), str(specimen / "session")],
                       cwd=tools, check=True, capture_output=True)
        unchanged = controls.run_control(specimen, "test_identity_probe.Probe.test_resume", MODE)
        self.assertEqual(controls.classify(unchanged, specimen, {"edited": None})[0], "survived")
        verify = tools / "machine_code/verify.py"
        verify.write_text(verify.read_text() + "# planted no-op edit\n")
        binding = {"edited": "verify.py", "edited_sha256": digest(verify)}
        outcome = controls.run_control(specimen, "test_identity_probe.Probe.test_resume", MODE)
        status, evidence = controls.classify(outcome, specimen, binding)
        self.assertEqual(status, "inconclusive")
        self.assertIn("identity refusal", evidence)
        self.assertIn("binding drifted", evidence)


if __name__ == "__main__":
    unittest.main()
