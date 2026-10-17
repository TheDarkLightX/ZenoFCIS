"""Arithmetic syntax and routing controls. Runtime arithmetic belongs to Rust."""
from __future__ import annotations

import unittest
from pathlib import Path
from unittest.mock import patch

import loop_doors
import loop_judges as J
import loop_language as Z
from behavior import Refusal
from loop_workspace import from_program
from test_loop_language import subject


class ArithmeticLanguage(unittest.TestCase):
    def setUp(self):
        self.ctx = Z.Context(subject("bounded-counter", {"C": 8}, ["Int", "Int", "Int"]))

    def test_calls_round_trip_both_surfaces_with_nested_holes_and_parameters(self):
        for text in (
            "mul_div_floor(value, ?c[0..40], $D)",
            "mul_div_ceil(value + 1, ?{1 | 2}, 10000)",
            "products_ge(value, 2, value + 1, 1)",
            "products_ge(value, 2, 3, value + 1, 1, 4)",
            "div_floor(value, 2) + div_ceil(value, 3)",
            "mul_div_floor(mul_div_ceil(value, 30, 10000), 40, 41)",
            "choose(value == 0, mul_div_floor(value, 2, 3), mul_div_ceil(value, 2, 3))",
        ):
            with self.subTest(text=text):
                node = Z.parse_expression(text)
                self.assertEqual(Z.parse_expression(Z.print_symbolic(node)), node)
                english = Z.show(node, self.ctx, True)
                self.assertEqual(Z.parse_english(english, self.ctx), node, english)
                self.assertEqual(Z.names_of(node), {"value"})

    def test_call_arity_unknown_calls_and_budgets_refuse(self):
        for text in ("mul_div_floor(1, 2)", "mul_div_ceil(1, 2, 3, 4)",
                     "products_ge(1, 2, 3)", "products_ge(1, 2, 3, 4, 5)", "unknown(1)",
                     "products_ge(1, 1, 1, keep)",
                     "mul_div_floor(" * 200 + "1" + ", 1, 1)" * 200):
            with self.subTest(text=text[:60]), self.assertRaises(Refusal):
                Z.parse_expression(text)

    def test_program_lifting_preserves_all_six_product_operands(self):
        nodes = [["Input", 0], ["Int", "2"], ["Int", "3"],
                 ["MulDivFloor", 0, 1, 2], ["MulDivCeil", 0, 2, 1],
                 ["ProductsGe", 0, 1, 2, 3, 4, 0]]
        lifted = from_program({"nodes": nodes, "roots": [5]}, ["value"])
        text = Z.print_symbolic(lifted[0])
        self.assertIn("products_ge", text)
        self.assertIn("mul_div_floor", text)
        self.assertIn("mul_div_ceil", text)
        self.assertEqual(Z.parse_expression(text), lifted[0])

    def test_capture_arithmetic_round_trips_with_the_real_act_handlers(self):
        import capture_workspace

        abi = {"inputs": [{"kind": "Int", "min": "0", "max": "40"}],
               "outputs": [{"kind": "Int", "min": "0", "max": "40"}, {"kind": "Bool"}],
               "tuples": 41, "order": "ordered-product-last-input-fastest-v1"}
        ctx = capture_workspace.context(abi, "capture", "a" * 64)
        for text, output in (("mul_div_floor(x0, 3, 2)", "out0"),
                             ("mul_div_ceil(x0, 30, 10000)", "out0"),
                             ("products_ge(x0, 2, 3, 1)", "out1"),
                             ("products_ge(x0, 2, 3, 3, 1, 2)", "out1")):
            act = Z.act("capture", "arithmetic", output=output, expr=Z.read(text, ctx, False))
            doc = Z.Document(ctx.subject_ref, "model", (act,))
            for english in (False, True):
                self.assertEqual(Z.parse(Z.render(doc, ctx, english), ctx), doc)

    def test_requirements_and_learn_contexts_preserve_exact_call_vocabulary(self):
        import loop_learn
        import loop_versions

        spec = {"inputs": [{"name": "x", "domain": {"kind": "Int", "min": 0, "max": 40}}],
                "outputs": [{"kind": "Int", "min": 0, "max": 40}],
                "vocabulary": {"constants": [30, 10000], "arithmetic": ["mul_div_floor", "mul_div_ceil"],
                               "compare": ["products_ge"]}, "examples": [], "max_size": 4}
        contexts = (Z.Context(loop_learn.subject(spec)), Z.Context(loop_versions._subject(spec)))
        for ctx in contexts:
            for text in ("mul_div_floor(x, 30, 10000)", "mul_div_ceil(x, 30, 10000)",
                         "products_ge(x, 2, 3, 1)", "products_ge(x, 2, 3, 3, 1, 2)"):
                node = Z.read(text, ctx, False)
                self.assertEqual(Z.read(Z.show(node, ctx, True), ctx, True), node)

    def test_law_writer_keeps_exact_calls_and_multiplication_precedence(self):
        for text in (
            "mul_div_floor(pre.value, 30, 10000) <= mul_div_ceil(pre.value, 30, 10000)",
            "products_ge(post.value, 2, pre.value, 2)",
            "products_ge(post.value, 2, 3, pre.value, 2, 3)",
            "(pre.value + 1) * (pre.value + 2) >= pre.value * pre.value",
        ):
            node = Z.read(text, self.ctx, False, law=True)
            written = J.law_formula(node, self.ctx.subject)
            self.assertIn("pre.100.110", written)
            if "products_ge" in text:
                self.assertTrue(written.startswith("products_ge("), written)
            if "mul_div_floor" in text:
                self.assertIn("mul_div_floor(", written)
                self.assertIn("mul_div_ceil(", written)
            if text.startswith("("):
                self.assertTrue(written.startswith("0 + "), written)

    def test_stubbed_product_writer_is_detected(self):
        node = Z.parse_expression("products_ge(post.value, 2, pre.value, 2)")
        original = J._Writer.truth
        def stub(writer, tree):
            return "true" if tree[0] == "call" and tree[1] == "products_ge" else original(writer, tree)
        with patch.object(J._Writer, "truth", stub), self.assertRaises(AssertionError):
            self.assertTrue(J.law_formula(node, self.ctx.subject).startswith("products_ge("))

    def test_each_new_call_has_a_semantic_comparison_and_a_failing_stub_control(self):
        from test_loop_workspace import SWEEP_ENVS, SWEEP_SUBJECT, law_value, loop_value
        def control(compare, observed, expected, wrong):
            return compare(observed, expected) and not compare(wrong, expected)
        for text in ("mul_div_floor(x, 2, 3) == 1", "mul_div_ceil(x, 2, 3) == 1",
                     "products_ge(x, 2, 3, 1)", "products_ge(x, 2, 1, 3, 1, 1)"):
            node = Z.parse_expression(text)
            written = J.plain_formula(node, SWEEP_SUBJECT)
            expected = [loop_value(node, env) == 1 for env in SWEEP_ENVS]
            observed = [bool(law_value(written, env)) for env in SWEEP_ENVS]
            with patch.object(J._Writer, "call", lambda _, n: "true" if n[1] == "products_ge" else "0"):
                mutant = J.plain_formula(node, SWEEP_SUBJECT)
            wrong = [bool(law_value(mutant, env)) for env in SWEEP_ENVS]
            self.assertTrue(control(lambda a, b: a == b, observed, expected, wrong), text)
            self.assertFalse(control(lambda *_: True, observed, expected, wrong), f"stubbed comparison must fail: {text}")

    def test_scenario_arithmetic_routes_to_library_and_never_falls_back(self):
        node = Z.parse_expression("mul_div_ceil(value, 30, 10000)")
        step = Z.act("step", "s", n=1, inputs=(("action", 150), ("authorized", 1)),
                     outcome=Z.Option("Accept", sets=(("value", node),)))
        with self.assertRaises(J.Unrouted):
            J.scenario_text("s", (("value", 7),), [step], self.ctx.subject)
        calls = []
        def library(tree, env):
            calls.append((tree, env))
            return 1
        with patch.object(J, "evaluate", side_effect=AssertionError("Python arithmetic evaluator called")):
            text = J.scenario_text("s", (("value", 7),), [step], self.ctx.subject, library)
        self.assertEqual(text, "# scenario s\n7 150 1 | accept - 1 | -\n")
        self.assertEqual(calls, [(node, {"value": 7, "action": 150, "authorized": 1})])

    def scenario_workspace(self):
        # Reuse the shell fixture; the external CLI is a recorded adapter.
        # Submit still runs the real parser, dispatcher, arithmetic bridge,
        # _scenario verdict branch, fact construction and transcript write.
        from test_loop_workspace import LoopWorkspaceTests
        case = LoopWorkspaceTests()
        case.setUp()
        self.addCleanup(case.tearDown)
        return case

    def test_scenario_successor_calls_round_trip_both_surfaces(self):
        for text in ("mul_div_floor(mul_div_ceil(value, 3, 2), 3, 2)",
                     "mul_div_floor(value, value == 1 ? 3 : 2, 2)",
                     "products_ge(value, 2, 1, 3, 1, 1) ? 2 : 1"):
            doc = Z.Document(self.ctx.subject_ref, "model", (
                Z.act("scenario", "arithmetic", start=(("value", 1),)),
                Z.act("step", "arithmetic", n=1, inputs=(("action", 150), ("authorized", 1)),
                      outcome=Z.Option("Accept", sets=(("value", Z.parse_expression(text)),))),
            ))
            for english in (False, True):
                rendered = Z.render(doc, self.ctx, english)
                self.assertEqual(Z.parse(rendered, self.ctx), doc, rendered)

    def test_workspace_scenario_verdict_checks_wrong_arithmetic_results(self):
        for k, (expression, value) in enumerate((
            ("mul_div_floor(value, 3, 2)", 1),
            ("mul_div_ceil(value, 3, 2)", 2),
            ("mul_div_floor(mul_div_ceil(value, 3, 2), 3, 2)", 3),
            ("mul_div_floor(value, value == 1 ? 3 : 2, 2)", 1),
            ("products_ge(value, 2, 3, 1) ? 2 : 1", 1),
            ("products_ge(value, 2, 1, 3, 1, 1) ? 2 : 1", 1),
        )):
            case = self.scenario_workspace()
            for verdict in ("passed", "failed"):
                name = f"arith{k}{verdict}"
                run = {"name": name, "verdict": verdict, "steps": 1,
                       "failure": {"step": 1, "reason": "post-state differs"}}
                case.fake.reports.append({"scenarios": [run]})
                with patch.object(J, "evaluate", side_effect=AssertionError("Python arithmetic called")), \
                     patch.object(loop_doors, "run", return_value=(0, {
                         "status": "evaluated", "detail": {"values": [str(value)]}})) as library:
                    events = case.submit(f"scenario {name} from value = 1;\n"
                                         f"step {name} 1: action = Inc, authorized = true -> accept value = {expression};\n")
                library.assert_called_once()
                self.assertIn("--input=1,150,1", library.call_args.args[1])
                self.assertEqual(events[-1]["detail"]["scenario"]["verdict"], verdict)
                fact = events[-1]["symbolic"]
                if verdict == "passed":
                    self.assertIn("passed: all 1 steps match the core", fact)
                else:
                    self.assertIn("step 1 differs: post-state differs", fact)
                    self.assertNotIn("passed:", fact)
                path = case.fake.calls[-1][1]
                self.assertEqual(case.fake.calls[-1][0], "scenario")
                self.assertIn(f"1 150 1 | accept - {value} | -", path.read_text())

    def test_workspace_scenario_arithmetic_refusal_never_reaches_the_judge(self):
        for k, expression in enumerate(("mul_div_floor(value, 1, 0)", "mul_div_ceil(value, 1, 0)",
                                       "products_ge(value, 1, -1, 1) ? 1 : 0",
                                       "products_ge(value, 1, 1, -1, 1, 1) ? 1 : 0")):
            case = self.scenario_workspace()
            name = f"undefined{k}"
            before = len(case.fake.calls)
            with patch.object(loop_doors, "run", return_value=(1, {"status": "refused"})), \
                 patch.object(loop_doors, "scenario", side_effect=AssertionError("undefined arithmetic reached the scenario judge")), \
                 patch.object(J, "evaluate", side_effect=AssertionError("Python arithmetic called")):
                events = case.submit(f"scenario {name} from value = 1;\n"
                                     f"step {name} 1: action = Inc, authorized = true -> accept value = {expression};\n")
            self.assertIn("refused:", events[-1]["symbolic"])
            self.assertNotIn("passed:", events[-1]["symbolic"])
            self.assertEqual(len(case.fake.calls), before)

    def test_library_bridge_refuses_failure_missing_and_malformed_values(self):
        args = (Path("cli"), Path("program"), Path("expressions"), [7])
        with patch.object(loop_doors, "run", return_value=(0, {"status": "evaluated", "detail": {"values": ["1"]}})) as run:
            self.assertEqual(loop_doors.evaluate_expression(*args), 1)
            self.assertIn("--input=7", run.call_args.args[1])
        for code, report in [(1, {"status": "refused"}), (0, {"status": "equivalent"}),
                             (0, {"status": "evaluated", "detail": {"values": [True]}}),
                             (0, {"status": "evaluated", "detail": {"values": [str(1 << 63)]}})]:
            with patch.object(loop_doors, "run", return_value=(code, report)), self.assertRaises(Refusal):
                loop_doors.evaluate_expression(*args)


if __name__ == "__main__":
    unittest.main()
