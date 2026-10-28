"""Acceptance and adversarial checks for the declarative CFG parser."""

import copy
import unittest
from itertools import product

from zal_notation.syntax import (
    Ambiguous,
    ParseError,
    ParseLimit,
    parse_text,
    validate_syntax,
)


def grammar(rules, start="E", constructors=None, **syntax_options):
    return {
        "schema": "zal.grammar/1", "id": "tests/arithmetic", "version": 1,
        "kind": "term", "root": "Expr",
        "constructors": constructors or {
            "Int": {"result": "Expr", "fields": {"value": "int"}, "children": []},
            "Add": {"result": "Expr", "fields": {}, "children": ["Expr", "Expr"]},
            "Mul": {"result": "Expr", "fields": {}, "children": ["Expr", "Expr"]},
            "Empty": {"result": "Expr", "fields": {}, "children": []},
            "Pair": {"result": "Expr", "fields": {}, "children": ["Expr", "Expr"]},
        },
        "syntax": {"start": start, "rules": rules, **syntax_options},
    }


def integer_rule(lhs="E"):
    return {"lhs": lhs, "rhs": [{"token": "int"}], "tag": "Int",
            "fields": {"value": {"token": 0, "as": "int"}}}


def term(tag, *children, **fields):
    return {"tag": tag, "fields": fields, "children": list(children)}


def arithmetic():
    return grammar([
        {"lhs": "E", "rhs": [{"ref": "E"}, {"lit": "+"}, {"ref": "P"}],
         "tag": "Add", "children": [0, 2]},
        {"lhs": "E", "rhs": [{"ref": "P"}], "forward": 0},
        {"lhs": "P", "rhs": [{"ref": "P"}, {"lit": "*"}, {"ref": "A"}],
         "tag": "Mul", "children": [0, 2]},
        {"lhs": "P", "rhs": [{"ref": "A"}], "forward": 0},
        {"lhs": "A", "rhs": [{"lit": "("}, {"ref": "E"}, {"lit": ")"}],
         "forward": 1},
        integer_rule("A"),
    ])


class SyntaxTests(unittest.TestCase):
    def test_custom_arithmetic_left_recursion_and_precedence(self):
        result = parse_text(" 1 + 2 * 3 + 4 ", arithmetic())
        self.assertEqual(result, term("Add",
            term("Add", term("Int", value=1),
                 term("Mul", term("Int", value=2), term("Int", value=3))),
            term("Int", value=4)))

    def test_parentheses_and_signed_integer(self):
        self.assertEqual(parse_text("(1+2)*-3", arithmetic()),
                         term("Mul", term("Add", term("Int", value=1),
                                          term("Int", value=2)), term("Int", value=-3)))

    def test_full_consumption(self):
        for source in ("1 trailing", "1+", "", "1.5", "(1+2"):
            with self.subTest(source=source), self.assertRaises(ParseError):
                parse_text(source, arithmetic())

    def test_ambiguous_association(self):
        g = grammar([
            {"lhs": "E", "rhs": [{"ref": "E"}, {"lit": "+"}, {"ref": "E"}],
             "tag": "Add", "children": [0, 2]}, integer_rule(),
        ])
        with self.assertRaises(Ambiguous):
            parse_text("1+2+3", g)
        self.assertEqual(parse_text("1+2", g),
                         term("Add", term("Int", value=1), term("Int", value=2)))

    def test_duplicate_derivations_rejected_even_with_same_ast(self):
        with self.assertRaises(Ambiguous):
            parse_text("3", grammar([integer_rule(), integer_rule()]))

    def test_epsilon_and_nullable_completion(self):
        g = grammar([
            {"lhs": "E", "rhs": [{"ref": "A"}, {"ref": "A"}],
             "tag": "Pair", "children": [0, 1]},
            {"lhs": "A", "rhs": [], "tag": "Empty"},
        ])
        result = parse_text("   ", g)
        self.assertEqual(result, term("Pair", term("Empty"), term("Empty")))
        self.assertIsNot(result["children"][0], result["children"][1])

    def test_productive_nullable_cycle_is_ambiguous(self):
        g = grammar([
            {"lhs": "E", "rhs": [{"ref": "E"}], "forward": 0},
            {"lhs": "E", "rhs": [], "tag": "Empty"},
        ])
        with self.assertRaises(Ambiguous):
            parse_text("", g)

    def test_nonproductive_left_recursion_has_no_parse(self):
        g = grammar([{"lhs": "E", "rhs": [{"ref": "E"}], "forward": 0}])
        with self.assertRaises(ParseError) as caught:
            parse_text("", g)
        self.assertNotIsInstance(caught.exception, (Ambiguous, ParseLimit))

    def test_indirect_nullable_recursion_is_ambiguous(self):
        g = grammar([
            {"lhs": "E", "rhs": [{"ref": "B"}], "forward": 0},
            {"lhs": "B", "rhs": [{"ref": "E"}], "forward": 0},
            {"lhs": "B", "rhs": [], "tag": "Empty"},
        ])
        with self.assertRaises(Ambiguous):
            parse_text("", g)

    def test_whitespace_can_be_significant(self):
        g = grammar([integer_rule()], skip_whitespace=False)
        self.assertEqual(parse_text("2", g), term("Int", value=2))
        for source in (" 2", "2 "):
            with self.assertRaises(ParseError):
                parse_text(source, g)

    def test_identifier_json_string_and_boolean_coercion(self):
        g = grammar([{
            "lhs": "E", "rhs": [{"token": "identifier"}, {"token": "string"}],
            "tag": "Record", "fields": {
                "enabled": {"token": 0, "as": "bool"},
                "label": {"token": 1, "as": "str"},
            },
        }], constructors={"Record": {"result": "Expr", "fields": {
            "enabled": "bool", "label": "str"}, "children": []}})
        self.assertEqual(parse_text('false "<script>\\n\\u03bb"', g),
                         term("Record", enabled=False, label="<script>\nλ"))
        for source in ('False "x"', 'truthy "x"', 'true "unterminated',
                       'true "\\ud800"', 'true "raw\nnewline"'):
            with self.subTest(source=source), self.assertRaises(ParseError):
                parse_text(source, g)

    def test_quoted_integer_coercion_is_exact(self):
        rule = integer_rule()
        rule["rhs"] = [{"token": "string"}]
        g = grammar([rule])
        self.assertEqual(parse_text('"-12"', g), term("Int", value=-12))
        for source in ('" 12"', '"1.5"', '"__import__(1)"', '"١٢"'):
            with self.assertRaises(ParseError):
                parse_text(source, g)

    def test_literal_field_binding(self):
        g = grammar([{"lhs": "E", "rhs": [{"lit": "true"}], "tag": "Int",
                      "fields": {"value": {"token": 0, "as": "bool"}}}])
        # Constructor-field typing is deliberately the core validator's job.
        self.assertEqual(parse_text("true", g), term("Int", value=True))

    def test_item_and_step_budgets(self):
        with self.assertRaises(ParseLimit):
            parse_text("1+2", arithmetic(), max_items=8)
        with self.assertRaises(ParseLimit):
            parse_text("1+2", arithmetic(), max_steps=10)
        with self.assertRaises(ParseLimit):
            parse_text(" " * 1000 + "1", grammar([integer_rule()]), max_steps=100)
        for keyword in ({"max_items": True}, {"max_steps": 0}, {"max_steps": 1.2}):
            with self.assertRaises(ParseError):
                parse_text("1", grammar([integer_rule()]), **keyword)

    def test_integer_character_bound(self):
        with self.assertRaises(ParseLimit):
            parse_text("9" * 1025, grammar([integer_rule()]))

    def test_unknown_syntax_and_symbol_fields_refused(self):
        mutations = [
            lambda g: g["syntax"].update({"regex": ".*"}),
            lambda g: g["syntax"].update({"skip_whitespace": "yes"}),
            lambda g: g["syntax"]["rules"][0].update({"eval": "1"}),
            lambda g: g["syntax"]["rules"][0].update({"rhs": [{"regex": ".*"}]}),
            lambda g: g["syntax"]["rules"][0].update({"rhs": [{"token": "python"}]}),
            lambda g: g["syntax"]["rules"][0].update({"rhs": [{"lit": ""}]}),
            lambda g: g["syntax"]["rules"][0].update({"rhs": [{"ref": "Missing"}]}),
            lambda g: g["syntax"].update({"start": "Missing"}),
            lambda g: g["syntax"]["rules"][0].update({"tag": "Missing"}),
        ]
        for mutation in mutations:
            g = grammar([integer_rule()])
            mutation(g)
            with self.subTest(g=g), self.assertRaises(ParseError):
                validate_syntax(g)

    def test_malformed_actions_refused(self):
        mutations = [
            {"forward": 0},
            {"children": [0]},
            {"fields": {"value": {"token": 99, "as": "int"}}},
            {"fields": {"value": {"token": True, "as": "int"}}},
            {"fields": {"value": {"token": 0, "as": "eval"}}},
            {"fields": {"value": {"token": 0, "as": "int", "default": 0}}},
        ]
        for mutation in mutations:
            g = grammar([integer_rule()])
            g["syntax"]["rules"][0].update(mutation)
            with self.subTest(mutation=mutation), self.assertRaises(ParseError):
                validate_syntax(g)
        g = grammar([{"lhs": "E", "rhs": [{"token": "int"}], "forward": 0}])
        with self.assertRaises(ParseError):
            validate_syntax(g)

    def test_optional_syntax_and_unchanged_input(self):
        validate_syntax({"constructors": {}})
        g = arithmetic()
        before = copy.deepcopy(g)
        validate_syntax(g)
        parse_text("1+2", g)
        self.assertEqual(g, before)

    def test_epsilon_ast_expansion_is_bounded(self):
        rules = [{"lhs": "N0", "rhs": [], "tag": "Empty"}]
        for index in range(1, 12):
            rules.append({"lhs": f"N{index}",
                          "rhs": [{"ref": f"N{index-1}"}, {"ref": f"N{index-1}"}],
                          "tag": "Pair", "children": [0, 1]})
        with self.assertRaises(ParseLimit):
            parse_text("", grammar(rules, start="N11"), max_items=1000)

    def test_small_cfgs_against_independent_bounded_tree_counts(self):
        # Independent recurrence over parse-tree height, not Earley items or
        # parser-produced ASTs.  This is a finite regression corpus, not a
        # completeness theorem for unrestricted CFG parsing.
        options = [(), ("a",), ("S",), ("S", "S"), ("a", "S"), ("S", "a")]
        compared = 0
        for enabled in product((False, True), repeat=len(options)):
            productions = [p for p, on in zip(options, enabled) if on]
            if not productions:
                continue
            rules = [{"lhs": "S", "rhs": [
                {"ref": x} if x == "S" else {"lit": x} for x in production],
                "tag": "Pair", "children": [
                    i for i, x in enumerate(production) if x == "S"]}
                for production in productions]
            g = grammar(rules, start="S")
            counts = [0] * 4
            for _ in range(10):
                current = [0] * 4
                for length in range(4):
                    for production in productions:
                        remaining = length - production.count("a")
                        children = production.count("S")
                        ways = 0
                        if remaining >= 0:
                            if children == 0:
                                ways = int(remaining == 0)
                            elif children == 1:
                                ways = counts[remaining]
                            else:
                                ways = sum(counts[k] * counts[remaining-k]
                                           for k in range(remaining+1))
                        current[length] = min(2, current[length] + ways)
                counts = current
            for length, expected in enumerate(counts):
                try:
                    parse_text("a" * length, g)
                    actual = 1
                except Ambiguous:
                    actual = 2
                except ParseError:
                    actual = 0
                self.assertEqual(actual, expected, (productions, length))
                compared += 1
        self.assertEqual(compared, 252)


if __name__ == "__main__":
    unittest.main()
