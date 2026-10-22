"""Ten recurring source mistakes, physical positions and advisory repair text."""

import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import behavior
import behavior2
from diagnostics import describe


HERE = Path(__file__).resolve().parent
ORDER = (HERE / "examples/order2.zal").read_text()
COUNTER = (HERE / "examples/context_free2.zal").read_text()
PREFIX = "# A comment; its punctuation is ignored;\n\n"


def mistakes(english):
    model = behavior2.parse(ORDER)
    source = model.render(english)
    rule = next(line for line in source.splitlines() if "approve_order:" in line)
    context = "The context observations are authorized." if english else "context authorized;"
    states = next(line for line in source.splitlines() if line.startswith("The states" if english else "states "))
    default = next(line for line in source.splitlines() if line.startswith("Otherwise" if english else "default "))
    counter = behavior2.parse(COUNTER).render(english)
    variable = next(line for line in counter.splitlines() if "variable count" in line or line.startswith("var count "))
    return [
        ("identifier-too-long", source.replace("approve_order:", "a" * 33 + ":"), "a" * 33),
        ("missing-terminator", source.replace(states, states[:-1]), states[:-1]),
        ("duplicate-declaration", source + context + "\n", context, True),
        ("missing-declaration", source.replace(default + "\n", ""), None),
        ("reserved-name", source.replace(context, context.replace("authorized", "next")), context.replace("authorized", "next")),
        ("undeclared-reference", source.replace(rule, rule.replace("authorized", "unlisted")), "approve_order:"),
        ("boolean-as-integer", source.replace(rule, rule.replace("authorized", "authorized plus 1 is greater than 0" if english else "authorized + 1 > 0")), "approve_order:"),
        ("formula-scope", source.replace(rule, rule.replace("authorized", "event is approve" if english else "event == approve")), "approve_order:"),
        ("invalid-range", counter.replace(variable, variable.replace("initially 0", "initially 5")), "variable count" if english else "var count"),
        ("invalid-outcome", source.replace(rule, rule.replace("move to approved" if english else "accept approved", "reject because denied" if english else "reject denied")), "approve_order:"),
    ]


class DiagnosticTests(unittest.TestCase):
    def test_ten_mistakes_in_both_surfaces_have_physical_positions_and_fixes(self):
        for english in [False, True]:
            for case in mistakes(english):
                code, bad, needle, *last = case
                source = PREFIX + bad
                with self.subTest(code=code, english=english):
                    with self.assertRaises(behavior.Refusal) as raised:
                        behavior2.parse_any(source, "english" if english else "symbolic")
                    result = describe(raised.exception, source, "example.zal")
                    self.assertEqual(result["code"], code, result)
                    self.assertEqual(result["location_scope"], "parser", result)
                    matches = [n for n, line in enumerate(source.splitlines(), 1) if needle and needle in line]
                    expected = (matches[-1] if last else matches[0]) if needle else len(source.splitlines()) + 1
                    self.assertEqual(result["location"]["line"], expected, result)
                    self.assertGreaterEqual(result["location"]["column"], 1)
                    self.assertTrue(result["reason"] and result["suggested_fix"])
                    self.assertNotIn("\n", result["reason"])
                    if code == "identifier-too-long":
                        self.assertEqual(result["location"]["column"], 6)
                        self.assertIn("32", result["reason"])

    def test_comment_semicolons_and_name_boundary_preserve_identity(self):
        for name in ["order", "order2", "context_free2", "revoked_mail2"]:
            text = (HERE / f"examples/{name}.zal").read_text()
            before = behavior2.parse_any(text)
            self.assertEqual(before, behavior2.parse_any(PREFIX + text))
            self.assertEqual(before.revision, behavior2.parse_any(PREFIX + text).revision)
        for length in [31, 32]:
            text = ORDER.replace("approve_order:", "a" * length + ":")
            self.assertEqual(behavior2.check(behavior2.parse(text))["status"], "pass-with-scope")
        for profile in [1, 2]:
            text = (HERE / f"examples/order{'2' if profile == 2 else ''}.zal").read_text()
            text = text.replace("context authorized;", "context " + "a" * 33 + ";")
            with self.assertRaises(behavior.Refusal) as raised:
                behavior2.parse_any(text)
            self.assertEqual(describe(raised.exception, text, "example.zal")["code"], "identifier-too-long")

    def test_profile_one_formula_error_keeps_the_physical_line(self):
        source = PREFIX + (HERE / "examples/order.zal").read_text().replace("[authorized]", "[unlisted]")
        with self.assertRaises(behavior.Refusal) as raised:
            behavior.parse(source)
        line = raised.exception.location[0]
        self.assertIn("unlisted", source.splitlines()[line - 1])

    def test_cli_emits_reason_location_fix_without_modifying_source(self):
        code, bad, *_ = mistakes(False)[0]
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "input.zal"
            text = PREFIX + bad
            source.write_text(text)
            run = subprocess.run([sys.executable, "-B", str(HERE / "run.py"), "check", str(source)],
                                 capture_output=True, text=True)
            self.assertEqual(run.returncode, 2, run.stderr)
            self.assertEqual(run.stdout, "")
            result = json.loads(run.stderr)
            detail = result["source_diagnostic"]
            self.assertEqual(detail["code"], code)
            self.assertEqual(detail["location"]["file"], str(source))
            self.assertTrue(detail["suggested_fix"])
            self.assertEqual(source.read_text(), text)


if __name__ == "__main__":
    unittest.main()
