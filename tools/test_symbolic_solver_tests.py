"""Guard the pinned symbolic solver test runner: a visible skip without
solvers, and every declared pinned test run and passed with them."""

import contextlib
import io
import re
import subprocess
import unittest
from unittest import mock

import symbolic_solver_tests as runner


class SymbolicSolverRunnerTests(unittest.TestCase):
    def test_without_both_solvers_it_prints_the_documented_skip_line(self):
        for environment in ({}, {"ZENO_FCIS_CVC5": "/cvc5"}, {"ZENO_FCIS_Z3": "/z3"},
                            {"ZENO_FCIS_CVC5": "", "ZENO_FCIS_Z3": "/z3"}):
            with self.subTest(environment=environment), \
                    mock.patch.dict(runner.os.environ, environment, clear=True), \
                    mock.patch.object(runner.subprocess, "run") as run:
                output = io.StringIO()
                with contextlib.redirect_stdout(output):
                    self.assertEqual(runner.main(), 0)
                run.assert_not_called()
                self.assertEqual(output.getvalue(), runner.SKIP + "\n")
        documented = (runner.ROOT / "docs" / "SYMBOLIC_CHECKS.md").read_text(encoding="utf-8")
        self.assertIn(runner.SKIP, documented)

    def test_every_pinned_test_is_ignored_and_declared(self):
        source = runner.TESTS.read_text(encoding="utf-8")
        ignored = re.findall(
            r'#\[ignore = "requires the workflow-pinned CVC5 and Z3 executables"\]\nfn (\w+)\(', source)
        self.assertEqual(len(ignored), runner.expected_tests())
        self.assertTrue(all(name.startswith("pinned_symbolic_") for name in ignored))
        self.assertGreaterEqual(len(ignored), 6)

    def run_with(self, returncode, output):
        completed = subprocess.CompletedProcess(runner.COMMAND, returncode, output)
        environment = {"ZENO_FCIS_CVC5": "/cvc5", "ZENO_FCIS_Z3": "/z3"}
        with mock.patch.dict(runner.os.environ, environment, clear=True), \
                mock.patch.object(runner.subprocess, "run", return_value=completed) as run, \
                contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            result = runner.main()
        run.assert_called_once()
        self.assertEqual(run.call_args.args[0], runner.COMMAND)
        return result

    def test_with_solvers_every_declared_test_must_pass(self):
        expected = runner.expected_tests()
        passing = f"test result: ok. {expected} passed; 0 failed; 0 ignored\n"
        self.assertEqual(self.run_with(0, passing), 0)
        short = f"test result: ok. {expected - 1} passed; 0 failed; 0 ignored\n"
        self.assertEqual(self.run_with(0, short), 1)
        self.assertEqual(self.run_with(0, "test result: ok. 0 passed; 0 failed; 6 ignored\n"), 1)
        self.assertEqual(self.run_with(101, passing), 101)


if __name__ == "__main__":
    unittest.main()
