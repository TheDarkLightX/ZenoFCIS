"""The planted pruning-defect campaign: its registry and its outcome classes on real specimens."""

from pathlib import Path
import sys
import tempfile
import unittest

from machine_qualification import pruning_defects as planted

ROOT = Path(__file__).resolve().parents[1]
MODE = ("-O", "-B") if sys.flags.optimize else ("-B",)
BRIEF = {"flag-writes-omitted", "conditional-write-as-overwrite", "write-write-conflict-ignored",
         "second-result-unobserved", "profile1-result-unobserved"}


class Registry(unittest.TestCase):
    def test_every_edit_applies_once_to_the_current_pruning(self):
        source = (ROOT / "tools/machine_code/footprint.py").read_text()
        names = [d.name for d in planted.DEFECTS]
        self.assertEqual(len(names), len(set(names)))
        self.assertLessEqual(BRIEF, set(names))
        for defect in planted.DEFECTS:
            with self.subTest(defect=defect.name):
                self.assertEqual(source.count(defect.anchor), 1)
                self.assertNotEqual(defect.anchor, defect.replacement)


class Outcomes(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="zeno-pruning-defects-test-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)

    def judge(self, defect, name):
        specimen = self.root / name
        binding = planted.build(specimen, defect)
        if "invalid" in binding:
            return "invalid-specimen", binding["invalid"]
        outcome, evidence, *_ = planted.judge(specimen, MODE)
        return outcome, evidence

    def test_unmodified_exact_planted_detected_missing_invalid_and_crash_inconclusive(self):
        second = next(d for d in planted.DEFECTS if d.name == "second-result-unobserved")
        syntactic = next(d for d in planted.DEFECTS if d.name == "input-required-syntactically")
        cases = [
            (None, "survived"),
            (second, "detected"),
            (syntactic, "detected"),
            (planted.Defect("missing", "this text is not in footprint.py", "x", "test"), "invalid-specimen"),
            (planted.Defect("crash", 'SCHEME = "exact-footprint/', 'raise RuntimeError("planted crash")\n'
                            'SCHEME = "exact-footprint/', "test"), "inconclusive"),
        ]
        for index, (defect, expected) in enumerate(cases):
            with self.subTest(defect=defect.name if defect else "unmodified"):
                outcome, evidence = self.judge(defect, f"case-{index}")
                self.assertEqual(outcome, expected, evidence)
                if defect is second:
                    self.assertIn("two-results", evidence)
                if defect is syntactic:  # only where the specification mentions an input it ignores
                    self.assertRegex(evidence, r"^unwitnessed removals: [a-z0-9-]*irrelevant-input \d+"
                                               r"(, [a-z0-9-]*irrelevant-input \d+)*$")


if __name__ == "__main__":
    unittest.main()
