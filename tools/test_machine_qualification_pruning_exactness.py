"""The pruning exactness control on reference pruners: exact ones pass, wrong ones are reported with the sequence."""

import unittest

from machine_qualification import pruning_exactness as exact

X1, X2 = "x86_64-sysv-register/1", "x86_64-sysv-register/2"
NEVER = lambda target, forms, results: None


def dead_x86_mov(target, forms, results):
    """A correct pruner: `mov rax, rdi` directly overwritten by `mov rax, rsi` is dead."""
    pairs = zip(forms, forms[1:])
    return "dead" if any(a == ("mov", "rax", "rdi") and b == ("mov", "rax", "rsi") for a, b in pairs) else None


def everything(target, forms, results):
    """A wrong pruner on every grammar: only the empty sequence is kept."""
    return "wrong" if forms else None


def repeated_first(target, forms, results):
    """A wrong pruner: it removes every sequence that starts with a repeated instruction."""
    return "wrong" if len(forms) >= 2 and forms[0] == forms[1] else None


class ExactnessControl(unittest.TestCase):
    def test_no_pruning_removes_nothing_on_every_case(self):
        for case in exact.CASES:
            with self.subTest(case=case.name):
                report = exact.check(case, NEVER)
                self.assertEqual((report["removed"], report["exact"]), (0, True))
                self.assertEqual(report["kept"], sum(len(case.forms) ** n for n in range(case.length + 1)))
        self.assertEqual({case.target for case in exact.CASES},
                         {"x86_64-sysv-register/1", "rv64i-register/1", "x86_64-sysv-register/2",
                          "rv64i-register/2", "x86_64-sysv-memory/3", "rv64i-memory/3"})
        self.assertEqual(len([case for case in exact.CASES if case.base]), 6)

    def test_a_correct_pruner_has_a_proved_witness_for_every_removal(self):
        report = exact.check(exact.CASES[0], dead_x86_mov)
        self.assertGreater(report["removed"], 0)
        self.assertEqual((report["witnessed"], report["failures"], report["exact"]), (report["removed"], [], True))

    def test_a_wrong_pruner_is_an_exactness_failure_on_every_case(self):
        for case in exact.CASES:
            with self.subTest(case=case.name):
                report = exact.check(case, everything)
                self.assertFalse(report["exact"])
                self.assertEqual(report["witnessed"] + len(report["failures"]) + report["removed_refused"],
                                 report["removed"])
        failure = exact.check(exact.CASES[0], repeated_first)["failures"][0]
        self.assertEqual(failure["reason"], "wrong")
        self.assertEqual(failure["removed"][0], failure["removed"][1])

    def test_the_proof_compares_only_observed_results_from_every_entry_state(self):
        lea = (("lea", "rax", "rdi", "rsi", 1),)
        self.assertTrue(exact.proved_equal(X1, (("mov", "rax", "rdi"), ("add", "rax", "rsi")), lea, 1))
        self.assertFalse(exact.proved_equal(X1, (("mov", "rax", "rsi"),), (("mov", "rax", "rdi"),), 1))
        # Scratch is free at entry: reading rcx is not reading zero.
        self.assertFalse(exact.proved_equal(X1, (("mov", "rax", "rcx"),), (("xor", "rax", "rax"),), 1))
        # A second result is observed only when the problem has two outputs.
        left, right = (("mov", "rax", "rdi"), ("mov", "rdx", "rsi")), (("mov", "rax", "rdi"),)
        self.assertTrue(exact.proved_equal(X2, left, right, 1))
        self.assertFalse(exact.proved_equal(X2, left, right, 2))
        # Entry flags are free: ADC is not ADD.
        self.assertFalse(exact.proved_equal(X2, (("mov", "rax", "rdi"), ("adc", "rax", "rsi")),
                                            (("mov", "rax", "rdi"), ("add", "rax", "rsi")), 1))

    def test_profile3_proofs_observe_results_and_read_write_words(self):
        case = next(case for case in exact.CASES if case.name == "memory-swap-words-x86")
        problem, target = case.problem(), case.target
        loads, stores = case.forms[:2], case.forms[2:]
        self.assertTrue(exact.proved_equal(target, loads + stores, loads[::-1] + stores, 0, problem=problem))
        self.assertFalse(exact.proved_equal(target, loads + stores, loads + stores[:1], 0, problem=problem))
        # Loads alone change no observed location.
        self.assertTrue(exact.proved_equal(target, loads, (), 0, problem=problem))
        sum_case = next(case for case in exact.CASES if case.name == "memory-copy2-sum-rv64i")
        # Adding into a0 redefines the source base, so a later load through it is refused.
        load, add = sum_case.forms[0], sum_case.forms[4]
        self.assertTrue(exact.refused(sum_case.target, (add, load), sum_case.problem()))
        self.assertFalse(exact.refused(sum_case.target, (load, add), sum_case.problem()))

    def test_the_search_pruning_is_exact_on_every_case(self):
        report = exact.run()
        self.assertTrue(report["exact"], [(r["case"], r["twins"]["failures"][:2], r["spec"]["failures"][:2])
                                          for r in report["cases"]])
        for row in report["cases"]:
            with self.subTest(case=row["case"]):
                twins, spec = row["twins"], row["spec"]
                self.assertGreater(twins["removed"], 0)
                self.assertNotIn("lost", twins["reasons"])
                self.assertEqual(twins["witnessed"] + twins["removed_refused"], twins["removed"])
                self.assertEqual(spec["witnessed"] + spec["lost_not_equivalent"] + spec["not_equivalent"]
                                 + spec["removed_refused"], spec["removed"])
                self.assertEqual(spec["lost_not_equivalent"], spec["reasons"].get("lost", 0))
        reasons = {reason for row in report["cases"] for reason in row["spec"]["reasons"]}
        self.assertEqual(reasons, {"dead", "order", "duplicate", "lost"})
        for reason in ("duplicate", "lost"):
            profiles = {row["target"] for row in report["cases"] if row["spec"]["reasons"].get(reason)}
            self.assertEqual({target.split("/")[0] for target in profiles}, {"x86_64-sysv-register", "rv64i-register"})
            self.assertEqual({target.split("/")[1] for target in profiles}, {"1", "2"})
        self.assertEqual({row["case"] for row in report["cases"]}, {case.name for case in exact.CASES})

    def test_a_lost_removal_must_be_refuted_by_the_checker(self):
        case = exact.CASES[0]  # x86 profile 1, specification x + y
        self.assertEqual(case.spec, (["add", ["input", "x"], ["input", "y"]],))
        solution = (("mov", "rax", "rdi"), ("add", "rax", "rsi"))
        wrong = exact.check(case, lambda target, forms, results: "lost" if forms == solution else None, True)
        self.assertEqual(wrong["failures"], [{"removed": [list(f) for f in solution], "reason": "lost",
                                              "checker": "equivalent"}])
        right = exact.check(case, lambda target, forms, results: "lost" if forms == solution[:1] else None, True)
        self.assertEqual((right["lost_not_equivalent"], right["exact"]), (1, True))
        # Removing the only solution of its cost is a failure even when the pass may consult the specification.
        dead = exact.check(case, lambda target, forms, results: "dead" if forms == solution else None, True)
        self.assertEqual(dead["failures"][0]["checker"], "equivalent")

    def test_a_removed_sequence_the_checker_refuses_needs_no_witness(self):
        case = exact.Grammar("refused", X2, (("mov", "rax", "rdi"), ("imul", "rax", "rsi"), ("sete", "rax")), 2)
        report = exact.check(case, lambda target, forms, results: "all" if forms else None)
        self.assertEqual(report["removed_refused"], 1)  # imul then sete reads the undefined ZF
        self.assertTrue(report["failures"])


if __name__ == "__main__":
    unittest.main()
