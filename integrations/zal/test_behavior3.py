"""Opt-in profile 3, preserving profile 2 and the complete original batch relation.

These are bounded Python/source checks, not native arithmetic/effect qualification.
The prefix oracle is independent of the ZAL AST evaluator and native emitter.
"""
from dataclasses import FrozenInstanceError, replace
import itertools
import json
from pathlib import Path
import unittest
from unittest.mock import patch

import app_profile
import behavior2 as b
import explain2
import factory
import program_synthesis
import typed2

HERE = Path(__file__).parent
TARGET = (HERE / "examples/prepared_batch3.zal").read_text("ascii")
COMPLETE = (HERE / "examples/prepared_batch3_complete.zal").read_text("ascii")
SMALL = """zal 3;
machine arithmetic;
states idle, ready;
events tick;
context none;
var count in 0..=3 initially 0;
initial idle;
step atomic; frame control_and_variables;
default reject inactive unchanged;
rule move: idle + tick [true] -> accept ready, count = 0;
"""


def prefix_oracle(count, first, second, third, authorized):
    if not authorized:
        return {"class": "Reject", "count": count, "reason": "denied"}
    current = count
    for delta in (first, second, third):
        current += delta
        if not 0 <= current <= 3:
            return {"class": "Reject", "count": count, "reason": "capacity"}
    return {"class": "Accept", "count": current, "reason": None}


def observe(model, count, first, second, third, authorized):
    state = "low" if count <= 1 else "high"
    result = model.decide(state, {"count": count}, "apply",
                          {"authorized": authorized, "first": first, "second": second, "third": third})
    return {"class": result["class"], "count": result["variables"]["count"], "reason": result["reason"]}


def admitted_rows():
    return itertools.product(range(4), range(-1, 2), range(-1, 2), range(-1, 2), (False, True))


class Profile3Tests(unittest.TestCase):
    def test_exact_original_216_relation(self):
        model = b.parse(TARGET)
        report = b.check(model)
        self.assertEqual((report["profile"], report["status"], report["input_tuples"],
                          report["configurations"], report["admissible_configurations"]),
                         (b.PROFILE_3, "pass-with-scope", 432, 8, 4))
        counts = {"Accept": 0, "Reject": 0}
        for row in admitted_rows():
            actual = observe(model, *row)
            self.assertEqual(actual, prefix_oracle(*row), row)
            counts[actual["class"]] += 1
        self.assertEqual(counts, {"Accept": 68, "Reject": 148})

    def test_paired_roundtrip_profile_identity_and_order(self):
        model = b.parse(TARGET)
        self.assertEqual(model.profile, b.PROFILE_3)
        self.assertEqual(model.data()["profile"], b.PROFILE_3)
        for english in (False, True):
            source = model.render(english)
            self.assertEqual(b.parse(source, "english" if english else "symbolic"), model)
            self.assertEqual(b.parse_any(source, "english" if english else "symbolic"), model)
        self.assertEqual(b.parse("\n".join(reversed(TARGET.splitlines()))), model)
        with self.assertRaises(FrozenInstanceError):
            model.profile = b.PROFILE
        old = b.parse(SMALL.replace("zal 3;", "zal 2;"))
        new = b.parse(SMALL)
        self.assertEqual(old.profile, b.PROFILE)
        self.assertNotEqual(old.revision, new.revision)
        diff = b.semantic_diff(old, new)
        self.assertEqual(diff["comparison"], "same-functional-observations-on-declared-domain")
        self.assertIn("profile", diff["changed_objects"])

    def test_old_two_input_and_atomic_update_refusals_remain(self):
        with self.assertRaisesRegex(b.Refusal, "Bounds: 2 inputs"):
            b.parse(TARGET.replace("zal 3;", "zal 2;"))
        expression = SMALL.replace("count = 0", "count = count + 0")
        self.assertIsInstance(b.parse(expression), b.Machine)
        with self.assertRaisesRegex(b.Refusal, "an update reads"):
            b.parse(expression.replace("zal 3;", "zal 2;"))
        rule = replace(b.parse(expression).rules[0])
        with self.assertRaisesRegex(b.Refusal, "literal, variable or input"):
            replace(b.parse(SMALL.replace("zal 3;", "zal 2;")), rules=(rule,))
        with self.assertRaisesRegex(b.Refusal, "at most 2"):
            replace(b.parse(TARGET), profile=b.PROFILE)

    def test_unknown_profiles_and_typed1_cannot_smuggle_profile3(self):
        model = b.parse(SMALL)
        for profile in ("zal/finite-fsm/4", "3", None, True):
            with self.subTest(profile=profile), self.assertRaisesRegex(b.Refusal, "Unknown finite-FSM profile"):
                replace(model, profile=profile)
        for source, syntax in ((SMALL.replace("zal 3;", "zal 4;"), "symbolic"),
                               (model.render(True).replace("version 3.", "version 4."), "english")):
            with self.assertRaises(b.Refusal):
                b.parse_any(source, syntax)
        typed = typed2.parse((HERE / "examples/relay_typed.zal").read_text("ascii"))
        self.assertEqual(typed.machine.profile, b.PROFILE)
        with self.assertRaisesRegex(b.Refusal, "Invalid IR typed machine"):
            replace(typed, machine=replace(typed.machine, profile=b.PROFILE_3))

    def test_prestate_simultaneous_updates_and_compound_assignment(self):
        source = SMALL.replace("var count in 0..=3 initially 0;",
                               "var alpha in 0..=3 initially 0;\nvar beta in 0..=3 initially 1;")
        source = source.replace("count = 0", "alpha = beta + 1, beta = alpha + 1")
        model = b.parse(source)
        self.assertEqual(model.decide("idle", {"alpha": 0, "beta": 1}, "tick", {})["variables"],
                         {"alpha": 2, "beta": 1})
        source = source.replace("alpha = beta + 1, beta = alpha + 1", "alpha -= beta - 1, beta += alpha - 1")
        model = b.parse(source)
        result = model.decide("idle", {"alpha": 2, "beta": 1}, "tick", {})
        self.assertEqual(result["variables"], {"alpha": 2, "beta": 2})
        term, _ = factory.profile2_expressions(model)
        self.assertEqual(term(model.rules[0].updates[0].term, ("pre", "post")), "(pre.100.171 - 1)")

    def test_range_trap_is_atomic_and_never_wraps(self):
        model = b.parse(SMALL.replace("count = 0", "count = count + 1"))
        before = {"count": 3}
        with self.assertRaisesRegex(b.Refusal, "outside 0..=3; a step never wraps around"):
            model.decide("idle", before, "tick", {})
        self.assertEqual(before, {"count": 3})
        report = b.check(model)
        self.assertEqual((report["profile"], report["status"], report["obligation"]),
                         (b.PROFILE_3, "counterexample", "range"))
        self.assertEqual(b.explain(model, "idle", before, "tick", {})["outcome"]["class"], "Trap")

    def test_eager_formula_operands_are_retained(self):
        term = b.Expr("input", ("delta",))
        formula = b.Expr("implies", (b.Expr("false"), b.Expr("cmp", ("==", term, b.Expr("int", (0,))))))
        original = b.value_of
        with patch.object(b, "value_of", wraps=original) as called:
            self.assertTrue(b.holds(formula, b._Step("idle", {}, "tick", {"delta": 0})))
        self.assertIn(term, [call.args[0] for call in called.call_args_list])

    def test_prefix_failure_is_not_repaired_by_a_later_delta(self):
        model = b.parse(TARGET)
        for row in ((0, -1, 1, 0, True), (3, 1, -1, 0, True), (0, 0, -1, 1, True)):
            self.assertEqual(observe(model, *row)["reason"], "capacity")
            self.assertEqual(observe(model, *row)["count"], row[0])
        self.assertEqual(observe(model, 0, -1, 1, 0, False)["reason"], "denied")
        mutant = b.parse(TARGET.replace("count + first >= 0 & count + first <= 3 & ", ""))
        # The authored requirements alone do not pin all prefix guards. The
        # independent full-domain oracle rejects this weaker but consistent model.
        self.assertEqual(b.check(mutant)["status"], "pass-with-scope")
        witnesses = [row for row in admitted_rows() if observe(mutant, *row) != prefix_oracle(*row)]
        self.assertTrue(witnesses)
        self.assertEqual(b.semantic_diff(model, mutant)["comparison"], "different")

    def test_separate_completed_requirements_refute_prefix_guard_deletion(self):
        model = b.parse(COMPLETE)
        report = b.check(model)
        self.assertEqual((report["status"], report["input_tuples"], report["requirement_count"]),
                         ("pass-with-scope", 432, 5))
        for row in admitted_rows():
            self.assertEqual(observe(model, *row), prefix_oracle(*row), row)
        self.assertEqual(b.semantic_diff(b.parse(TARGET), model)["comparison"],
                         "same-functional-observations-on-declared-domain")
        mutated = "\n".join(line.replace("count + first >= 0 & count + first <= 3 & ", "")
                              if line.startswith("rule ") else line for line in COMPLETE.splitlines()) + "\n"
        self.assertIn("require exact_admission: accepted <=> (authorized & count + first >= 0", mutated)
        report = b.check(b.parse(mutated))
        self.assertEqual((report["status"], report["obligation"], report["requirement"]),
                         ("counterexample", "requirement", "exact_admission"))

    def test_checks_detect_authorization_and_wrong_final_value_mutants(self):
        for source in (TARGET.replace("[!authorized]", "[false]").replace("[authorized &", "[true &"),
                       TARGET.replace("count = count + first + second + third", "count = count + first + second")):
            report = b.check(b.parse(source))
            self.assertEqual(report["status"], "counterexample")
            self.assertIn(report["obligation"], ("requirement", "range", "invariant", "invariant-preservation"))

    def test_reports_and_existing_pure_export_retain_profile(self):
        model = b.parse(TARGET)
        given = ("low", {"count": 0}, "apply", {"authorized": True, "first": 1, "second": 1, "third": 1})
        trace = [{"event": given[2], "context": given[3]}]
        reports = [b.legend(model), b.help_topic(model, "grammar:header"), b.explain(model, *given),
                   b.trace_properties(model, trace), explain2.what_if(model, *given, {"context": {"third": 0}}),
                   explain2.why_not(model, *given, {"class": "Reject"}),
                   explain2.narrate_check(model, b.check(model)), explain2.narrate_trace(model, trace)]
        for report in reports:
            self.assertEqual(report["profile"], b.PROFILE_3)
        self.assertEqual(b.legend(model)["entries"][0]["concept"], "header")
        self.assertIn("three bounded", reports[1]["meaning"])
        files = factory.lower(model)
        mapping = json.loads(files["zal-source-map.json"])
        self.assertEqual(mapping["profile"], b.PROFILE_3)
        self.assertEqual(mapping["reference_domain"]["admissible_tuples"], 216)
        self.assertEqual(files["tests/zal-authority-examples.txt"].count("\n"), 216)
        policy = json.loads(files["v2/policy.json"])
        self.assertTrue(all(case["outbox"] == [] for case in policy["cases"]))
        self.assertIn("empty deliveries", mapping["scope"])
        with self.assertRaisesRegex(b.Refusal, "only one integer variable and one Boolean context fact"):
            program_synthesis.prepare(TARGET.encode("ascii"), b"{}", "apply")

    def test_app_binding_keeps_three_commands_context_and_pure_scope(self):
        from test_app_profile import binding_for
        model = b.parse(TARGET)
        files = app_profile.lower(TARGET, json.dumps(binding_for(model)))
        mapping = json.loads(files["zal-source-map.json"])
        self.assertEqual(mapping["profile"], b.PROFILE_3)
        self.assertEqual([name for name, value in mapping["bindings"].items() if value["root"] == "command"],
                         ["first", "second", "third"])
        self.assertEqual(mapping["bindings"]["authorized"]["root"], "context")
        self.assertTrue(all(case["outbox"] == [] for case in json.loads(files["v2/policy.json"])["cases"]))


class Profile3RefusalTests(unittest.TestCase):
    def refused(self, source, expected):
        with self.assertRaisesRegex(b.Refusal, expected):
            b.parse(source)

    def test_input_limit_and_fixed_domain_limits(self):
        self.refused(TARGET.replace("input third", "input fourth in 0..=0;\ninput third"), "Bounds: 3 inputs")
        exact = SMALL.replace("var count", "input first in 0..=15;\ninput second in 0..=15;\ninput third in 0..=7;\nvar count")
        model = b.parse(exact)
        self.assertEqual(len(list(model.inputs_domain())), 16384)
        self.refused(exact.replace("third in 0..=7", "third in 0..=8"), "16384")
        configs = SMALL.replace("count in 0..=3", "count in 0..=31").replace("initial idle;", "var other in 0..=63 initially 0;\ninitial idle;")
        self.assertEqual(len(list(b.parse(configs).configurations())), 4096)
        self.refused(configs.replace("count in 0..=31", "count in 0..=32"), "4096")
        self.assertIsInstance(b.parse(SMALL.replace("count in 0..=3", "count in 0..=63")), b.Machine)
        self.refused(SMALL.replace("count in 0..=3", "count in 0..=64"), "inclusive range of 1..64")

    def test_fixed_source_statement_collection_and_literal_limits(self):
        padded = SMALL + "#" + " " * (65536 - len(SMALL) - 2) + "\n"
        self.assertEqual(len(padded), 65536)
        self.assertIsInstance(b.parse(padded), b.Machine)
        self.refused(padded + " ", "at most 64 KiB")
        self.refused(SMALL + "input extra in 0..=0;\n" * 96, "96 statements")
        for kind, count, line in (("variables", 3, "var extra{} in 0..=0 initially 0;"),
                                  ("rules", 31, "rule extra{}: ready + tick [false] -> accept ready;"),
                                  ("invariants", 8, "invariant extra{}: true;"),
                                  ("requirements", 8, "require extra{}: true;")):
            source = SMALL + "\n".join(line.format(i) for i in range(count)) + "\n"
            self.assertIsInstance(b.parse(source), b.Machine, kind)
            self.refused(source + line.format(count) + "\n", "Bounds:")
        self.assertIsInstance(b.parse(SMALL.replace("count = 0", "count = 32767")), b.Machine)
        self.refused(SMALL.replace("count = 0", "count = 32768"), "Integer literals")

    def test_expression_admission_type_scope_depth_nodes_characters_tokens(self):
        at_depth = " + ".join(["0"] * 16)
        self.assertIsInstance(b.parse(SMALL.replace("count = 0", "count = " + at_depth)), b.Machine)
        self.refused(SMALL.replace("count = 0", "count = " + at_depth + " + 0"), "depth 16")
        self.refused(SMALL.replace("count = 0", "count = " + " + ".join(["0"] * 65)), "depth")
        self.refused(SMALL.replace("count = 0", "count = 0" + " " * 4096 + "+ 0"), "4096 characters")
        self.refused(SMALL.replace("count = 0", "count = " + "+".join(["0"] * 513)), "1024 lexical tokens")
        guard = " | ".join("count == " + str(i) for i in range(42))
        self.assertIsInstance(b.parse(SMALL.replace("[true]", "[" + guard + "]")), b.Machine)
        self.refused(SMALL.replace("[true]", "[" + guard + " | count == 42]"), "128 nodes")
        for term in ("count'", "accepted", "true", "count * 1", "(count + 1)", "missing", "count == 0"):
            with self.subTest(term=term), self.assertRaises(b.Refusal):
                b.parse(SMALL.replace("count = 0", "count = " + term))
        model = b.parse(SMALL)
        term = b.Expr("int", (0,))
        for _ in range(16):
            term = b.Expr("add", (term, b.Expr("int", (0,))))
        rule = replace(model.rules[0], updates=(b.Update("count", "=", term),))
        with self.assertRaisesRegex(b.Refusal, "depth 16"):
            replace(model, rules=(rule,))


if __name__ == "__main__":
    unittest.main()
