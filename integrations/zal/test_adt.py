"""Finite algebraic data types: round trips, exact counts against enumeration, named refusals."""

import itertools
import random
import re
import unittest

import adt
from adt import Decl, Refusal, Ty, Types, card_text, parse_decl


SOURCE = {
    "Bounded": "{version: Version, cap: Any}", "Any": "int 0..=18446744073709551615",
    "Mode": "unversioned | at(Version) | bounded_at(Bounded)", "Version": "int 0..=9223372036854775807",
    "Status": "committed | replay", "Pair": "{a: Status, b: bool}", "Empty": "{}", "Hash": "bytes 32",
    "Envelope": "bytes ..=1048576", "Small": "list ..=3 of Status", "Signed": "int -3..=3",
    "Nested": "{pair: Pair, maybe: Maybe}", "Maybe": "none | some(Status)", "Tiny": "bytes ..=1",
}


def types_of(source, prelude=()):
    return Types(tuple(sorted((parse_decl(n, b) for n, b in source.items()), key=lambda d: d.name)), prelude)


def statement(decl, english):
    """Parse one rendered statement back through the statement pattern."""
    match = re.fullmatch(adt.type_patterns(english), decl.render(english))
    return parse_decl(match[1], match[2], english)


def random_types(rng, count):
    """Small random declarations: each refers only to earlier names, so none is recursive."""
    source, names = {}, []
    primitives = ["unit", "bool", "int 0..=1", "int -1..=1", "int 2..=4", "bytes 1", "bytes ..=1"]
    def ty(depth):
        roll = rng.randrange(10)
        if names and roll < 4:
            return rng.choice(names)
        if depth < 2 and roll == 4:
            return f"list ..={rng.randrange(1, 3)} of {ty(depth + 1)}"
        choice = rng.choice(primitives)
        return "bool" if choice == "unit" else choice
    for index in range(count):
        name = f"T{index}"
        form = rng.randrange(3)
        members = rng.randrange(0 if form == 1 else 1, 4)
        if form == 0:
            body = ty(0)
        elif form == 1:
            body = "{" + ", ".join(f"f{i}: {ty(0)}" for i in range(members)) + "}"
        else:
            body = " | ".join(f"c{i}" if rng.randrange(2) else f"c{i}({ty(0)})" for i in range(members))
        source[name] = body
        names.append(name)
    return source


def brute_count(types, ty, limit=4096):
    """Independent count: walk the enumeration and stop early above the limit."""
    count = 0
    for _ in types._enumerate(ty):
        count += 1
        if count > limit:
            return None
    return count


class RoundTripTests(unittest.TestCase):
    def test_both_surfaces_round_trip(self):
        types = types_of(SOURCE)
        for decl in types.decls:
            with self.subTest(decl=decl.name):
                self.assertEqual(statement(decl, False), decl)
                self.assertEqual(statement(decl, True), decl)
                english = decl.render(True)
                self.assertEqual(statement(statement(decl, True), False).render(True), english)
        self.assertEqual(types.by_name["Mode"].render(True),
                         "The type Mode is one of unversioned, at (Version), bounded_at (Bounded).")
        self.assertEqual(types.by_name["Pair"].render(True), "The type Pair is a record with fields a (Status), b (a Boolean).")
        self.assertEqual(types.by_name["Empty"].render(True), "The type Empty is a record with no fields.")
        self.assertEqual(types.by_name["Small"].render(True), "The type Small is a list of at most 3 items of Status.")

    def test_random_declarations_round_trip(self):
        rng = random.Random(20261019)
        checked = 0
        for _ in range(300):
            types = types_of(random_types(rng, rng.randrange(1, 6)))
            for decl in types.decls:
                self.assertEqual(statement(decl, False), decl, decl.render())
                self.assertEqual(statement(decl, True), decl, decl.render(True))
                checked += 1
        print(f"ADT ROUND TRIP: {checked} random declarations in both surfaces")

    def test_values_round_trip_in_canonical_order(self):
        types = types_of(SOURCE)
        for name in ["Pair", "Status", "Small", "Nested", "Maybe", "Signed", "Empty"]:
            ty = Ty("ref", (name,))
            values = types.values(ty, 64)
            self.assertEqual(len(values), len(set(values)))
            for value in values:
                self.assertEqual(types.parse_value(ty, types.render_value(ty, value)), value)
        self.assertEqual([types.render_value(Ty("ref", ("Pair",)), v) for v in types.values(Ty("ref", ("Pair",)), 64)],
                         ["{a: committed, b: false}", "{a: committed, b: true}", "{a: replay, b: false}", "{a: replay, b: true}"])
        self.assertEqual(types.render_value(Ty("ref", ("Maybe",)), ("some", ("replay", ()))), "some(replay)")
        with self.assertRaises(Refusal):
            types.parse_value(Ty("ref", ("Pair",)), "{b: true, a: replay}")
        with self.assertRaises(Refusal):
            types.parse_value(Ty("ref", ("Signed",)), "4")


class CardinalityTests(unittest.TestCase):
    def test_counts_of_the_shell_types(self):
        types = types_of(SOURCE)
        count = lambda name: types.cardinality(Ty("ref", (name,)))
        self.assertEqual(count("Bounded"), 2 ** 63 * 2 ** 64)
        self.assertEqual(card_text(count("Mode")), "2^127 + 2^63 + 1")
        self.assertEqual(card_text(count("Hash")), "2^256")
        self.assertEqual(card_text(count("Envelope")), "more than 2^4096")
        self.assertEqual(count("Tiny"), 257)
        self.assertEqual(count("Small"), 1 + 2 + 4 + 8)
        self.assertEqual(count("Empty"), 1)
        self.assertEqual(count("Pair"), 4)
        self.assertEqual(count("Nested"), 4 * 3)

    def test_cardinality_matches_enumeration(self):
        """A record multiplies and a variant adds; every count equals the enumeration's length."""
        rng = random.Random(20261019)
        compared = 0
        for _ in range(400):
            types = types_of(random_types(rng, rng.randrange(1, 6)))
            for decl in types.decls:
                ty = Ty("ref", (decl.name,))
                expected = types.cardinality(ty)
                if expected is not None and expected <= 4096:
                    self.assertEqual(brute_count(types, ty), expected, decl.render())
                    compared += 1
        types = types_of({"A": "{x: Three, y: Three}", "B": "x(Three) | y(Three)", "Three": "int 0..=2"})
        self.assertEqual(types.cardinality(Ty("ref", ("A",))), 9)
        self.assertEqual(types.cardinality(Ty("ref", ("B",))), 6)
        print(f"ADT CARDINALITY: {compared} random types counted by enumeration")

    def test_explanations_name_the_rule(self):
        types = types_of(SOURCE)
        explained = types.explain("Mode")
        self.assertEqual((explained["operator"], explained["cardinality"]["text"]), ("sum", "2^127 + 2^63 + 1"))
        self.assertEqual(explained["rule"], "sum of its cases: unversioned (1) + at (9223372036854775808) + bounded_at (2^127)")
        explained = types.explain("Pair")
        self.assertEqual(explained["rule"], "product of its fields: a (2) x b (2)")
        self.assertEqual(types.explain("Empty")["rule"], "an empty record has exactly one value")


class RefusalTests(unittest.TestCase):
    def refused(self, source, message):
        try:
            types_of(source)
        except Refusal as error:
            self.assertIn(message, str(error))
            return
        except Exception as error:  # A missing check must fail here, not crash the suite.
            self.fail(f"not refused by name: {type(error).__name__}")
        self.fail("admitted: " + repr(source))

    def test_duplicate_tags_and_fields_refused(self):
        self.refused({"V": "a | b | a"}, "Variant V repeats case a")
        self.refused({"R": "{x: bool, x: bool}"}, "Record R repeats field x")
        with self.assertRaises(Refusal):
            parse_decl("V", "a | unit")

    def test_recursion_refused(self):
        self.refused({"List": "nil | cons(List)"}, "Type List is recursive (List -> List)")
        self.refused({"A": "{b: B}", "B": "x | y(C)", "C": "list ..=2 of A"}, "is recursive (A -> B -> C -> A)")
        self.refused({"A": "B", "B": "A"}, "is recursive")

    def test_bounds_and_names_refused(self):
        self.refused({"U": "Missing"}, "Type U refers to undeclared type Missing")
        self.refused({"B": "bytes ..=1048577"}, "A byte cap or length lies within 1..=1048576")
        self.refused({"L": "list ..=0 of bool"}, "A list item cap lies within 1..=1048576")
        self.refused({"I": "int 3..=2"}, "An integer type needs LOW <= HIGH")
        deep = {f"D{i}": f"{{x: D{i + 1}}}" for i in range(9)} | {"D9": "bool"}
        self.refused(deep, "nests deeper than 8 levels")
        with self.assertRaises(Refusal):
            parse_decl("Big", "{" + ", ".join(f"f{i}: bool" for i in range(33)) + "}")
        with self.assertRaises(Refusal):
            types_of({"A": "bool"}, (Decl("A", "opaque", (2, "two")),))


if __name__ == "__main__":
    unittest.main()
