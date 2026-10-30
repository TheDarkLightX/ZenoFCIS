"""Controls for the optional second-solver confirmation (needs the pinned cvc5 named by ZENO_FCIS_CVC5)."""
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from machine_code import second_solver, verify2
from machine_code.targets import encode_program

DATA = Path(__file__).resolve().parents[1] / "test-data/machine-code"
CVC5 = os.environ.get("ZENO_FCIS_CVC5")


@unittest.skipUnless(CVC5, "ZENO_FCIS_CVC5 names no pinned cvc5")
class SecondSolverTests(unittest.TestCase):
    def kernel(self, name):
        entry = json.loads((DATA / "kernels" / name).read_text())
        return entry["problem"], bytes.fromhex(entry["code_hex"])

    def test_library_kernels_are_confirmed_by_both_solvers(self):
        for name in ("add128-x86.json", "umin-x86.json", "add128-rv64i.json"):
            verdict = second_solver.confirm(*self.kernel(name), Path(CVC5))
            self.assertEqual((verdict["status"], verdict["second_solver"]["answer"]), ("equivalent", "unsat"), name)
            self.assertEqual(verdict["second_solver"]["version"], second_solver.VERSION)

    def test_a_counterexample_passes_through_without_the_second_solver(self):
        problem, _ = self.kernel("add64-x86.json")
        verdict = second_solver.confirm(problem, encode_program(problem["target"], [["mov", "rax", "rdi"]]),
                                        Path(CVC5))
        self.assertEqual(verdict["status"], "counterexample")
        self.assertNotIn("answer", verdict["second_solver"])

    def test_a_disagreeing_second_answer_is_never_equivalent(self):
        # Planted: the query text handed to cvc5 loses its assertions, so cvc5 answers sat.
        problem, code = self.kernel("umin-x86.json")
        with patch("z3.Solver.to_smt2", lambda self: "(check-sat)\n"):
            verdict = second_solver.confirm(problem, code, Path(CVC5))
        self.assertEqual((verdict["status"], verdict["reason"]), ("inconclusive", "SecondSolverDisagrees:sat"))

    def test_overlapping_confirmations_each_send_their_own_query(self):
        # Review 3: without the hook lock, a second call could record the first call's solver and restore a stale
        # class. The barrier tries to interleave the two checker calls; with the lock the second waits instead.
        import threading
        import z3
        from machine_code import session
        sequential = {name: second_solver.confirm(*self.kernel(name), Path(CVC5))["second_solver"]["query_sha256"]
                      for name in ("add64-x86.json", "umin-x86.json")}
        barrier, lookup, results = threading.Barrier(2, timeout=1), session._checker, {}

        def paused(problem):
            try:
                barrier.wait()
            except threading.BrokenBarrierError:
                pass
            return lookup(problem)

        def run(name):
            results[name] = second_solver.confirm(*self.kernel(name), Path(CVC5))
        original = z3.Solver
        with patch.object(session, "_checker", paused):
            threads = [threading.Thread(target=run, args=(name,)) for name in sequential]
            for thread in threads:
                thread.start()
            for thread in threads:
                thread.join()
        self.assertIs(z3.Solver, original)
        for name, digest in sequential.items():
            self.assertEqual((results[name]["status"], results[name]["second_solver"]["query_sha256"]),
                             ("equivalent", digest), name)

    def test_another_cvc5_version_is_refused(self):
        with tempfile.TemporaryDirectory() as folder:
            fake = Path(folder) / "cvc5"
            fake.write_text("#!/bin/sh\necho 'This is cvc5 version 1.1.2'\n")
            fake.chmod(0o755)
            verdict = second_solver.confirm(*self.kernel("umin-x86.json"), fake)
        self.assertEqual((verdict["status"], verdict["reason"]), ("inconclusive", "SecondSolverVersion:1.1.2"))


if __name__ == "__main__":
    unittest.main()
