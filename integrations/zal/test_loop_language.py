"""The ZAL loop profile zal/fcis-loop/1: both surfaces round-trip every act; bounds and refusals."""
from __future__ import annotations

from pathlib import Path
import sys
import tempfile
import unittest

from behavior import Refusal
import loop_language as Z
from loop_workspace import describe

ROOT = Path(__file__).resolve().parents[2]
POOL = ("(?c < available + reserved) ? 0 : (authorized ? (((action < 151) ? (available < quantity) : "
        "((action == 153) ? (?c < available + reserved + quantity) : (reserved < quantity))) ? "
        "((action < 151) ? 2 : ((action == 153) ? 4 : 3)) : action - ?c) : 1)")
COUNTER_C8 = "authorized ? action - (value == (action == Inc ? 8 : 0) ? 149 : 147) : 0"


def subject(family: str, parameters: dict, outputs: list, family_subject: str | None = None) -> dict:
    """A subject described from an instantiated core component (no CLI: the program's ABI is given)."""
    sys.path.insert(0, str(ROOT / "tools"))
    try:
        import instantiate_core
    finally:
        sys.path.pop(0)
    with tempfile.TemporaryDirectory() as temp:
        contract = Path(temp) / "contract"
        instantiate_core.instantiate(family, parameters, contract, include_proof=False)
        from loop_workspace import fields_of
        fields = fields_of(contract)
        inputs = [{"kind": "Bool"} if k == "Bool" else {"kind": "Int", "min": str(lo), "max": str(hi)}
                  for k, (lo, hi) in zip(fields["kinds"], fields["ranges"])]
        program = {"inputs": inputs, "outputs": [{"kind": k} for k in outputs]}
        tag = family + "-" + "-".join(f"{k.lower()}{v}" for k, v in sorted(parameters.items()))
        return describe(contract, program, tag, "0" * 64, parameters, family_subject)


POOL_SUBJECT = subject("reservation-pool", {"C": 2, "Q": 2}, ["Int"] * 5)
COUNTER_SUBJECT = subject("bounded-counter", {"C": 8}, ["Int"] * 3)
QUEUE_SUBJECT = subject("approval-queue", {"K": 2}, ["Int", "Bool", "Bool", "Bool"])
FAMILY_SUBJECT = subject("finite-phase-machine", {"C": 1}, ["Int", "Int"], family_subject="finite-phase-machine")


class Expressions(unittest.TestCase):
    def test_printing_round_trips_and_holes_do_not_swallow_names(self):
        for text in [COUNTER_C8, POOL, "x - -3", "!(a && b) || c -> d", "?c[0..8] + value", "?{value | 0} == ?e",
                     "c ? (a ? b : d) : e ? f : g", "(action == 161) == (161 < action)", "a?count:0",
                     "a -> b -> c", "$C + $K - 1", "0 - x", "a <= b && b >= c || a != c"]:
            ast = Z.parse_expression(text)
            self.assertEqual(Z.parse_expression(Z.print_symbolic(ast)), ast, text)
        self.assertEqual(Z.parse_expression("a?count:0"), ("if", ("var", "a"), ("var", "count"), ("int", 0)))

    def test_refusals_and_bounds(self):
        for text in ["keep + 1", "f(x)", "x + then", "(x", "?{a}", "?{a | ?c}", "?c[5..2]", "x é 1",
                     "170141183460469231731687303715884105728", " + ".join(["?c"] * 17), " + ".join(["x"] * 520), "x" * 9000]:
            with self.assertRaises(Refusal, msg=text):
                Z.parse_expression(text, allow_keep=True)


class ZalLoopProfile(unittest.TestCase):
    def setUp(self):
        self.ctx = Z.Context(POOL_SUBJECT)

    def head(self, author: str = "model") -> str:
        return f"zal fcis loop 1;\nsubject {self.ctx.tag} at {self.ctx.revision};\nauthor {author};\n"

    def document(self) -> str:
        return self.head() + f"""program p1 out0 = {POOL};
program p1 out1 = keep;
law l1 on accept: pre.available + pre.reserved <= 2;
law l2 on reject insufficient_reserved: command.quantity > pre.reserved && context.authorized == 1;
invariant i1: available + reserved <= 2;
scenario s1 from genesis;
step s1 1: action = Reserve, quantity = 2, authorized = true -> accept available = 0, reserved = 2;
scenario s2 from available = 1, reserved = 1;
step s2 1: action = Consume, quantity = 2, authorized = true -> reject insufficient_reserved;
probe q1: depends out0 on quantity where !authorized;
probe q2: values out0 where authorized;
probe q3: distinguish out0, authorized ? out0 : 1 where action == Reserve;
probe q4: implies out0 == 5, action == Reserve;
probe q5: count available + reserved > 2;
probe q6: reachable available == 0 && reserved == 2;
probe q7: explain available = 1, reserved = 1, action = Consume, quantity = 2, authorized = true;
probe q8: freedom;
ask k1 where pre.available + pre.reserved > 2 at available = 1, reserved = 2, action = Reserve, quantity = 1, authorized = false: reject invalid_state | reject not_authorized;
ask k2 at available = 0, reserved = 0, action = Replenish, quantity = 1, authorized = true: accept available = available + quantity | reject over_capacity;
accepted p1: 144 tuples, 33 nodes;
refuted p1 at available = 0, reserved = 0, action = Reserve, quantity = 1, authorized = true: out0 expected 5 got 1;
region p1: 12 of 144 where action == Reserve && authorized;
site p1 out0: 147 -> 145;
completed p1: ?0 = 2; ?1 = 145;
unrealizable p1 out0 at available = 0, reserved = 0, action = Reserve, quantity = 1, authorized = true;
fact q1: out0 does not depend on quantity, exhaustive over 72 inputs;
rule t1: where authorized && action == Reserve && quantity > available -> reject insufficient_available;
"""

    def test_both_surfaces_round_trip_to_the_same_typed_acts(self):
        doc = Z.parse(self.document(), self.ctx)
        kinds = {a.kind for a in doc.acts}
        self.assertGreaterEqual(kinds, {"program", "law", "invariant", "scenario", "step", "probe", "ask",
                                        "accepted", "refuted", "region", "site", "completed", "unrealizable",
                                        "fact", "rule"})
        for english in (False, True):
            again = Z.parse(Z.render(doc, self.ctx, english=english), self.ctx)
            self.assertEqual(again, doc)
            self.assertEqual(again.revision, doc.revision)
            for a in doc.acts:
                line = Z.render_act(a, self.ctx, english)
                self.assertEqual(Z.parse_statement(line, self.ctx, english), a, line)
        answers = (f"ZAL FCIS loop version 1.\nThe subject is {self.ctx.tag} at revision {self.ctx.revision}.\n"
                   "The author is the owner.\nAnswer k1: reject because invalid_state.\n"
                   "Answer k2: any offered option is acceptable.\nAnswer k3: none of the offered options.\n")
        doc2 = Z.parse(answers, self.ctx)
        self.assertEqual(Z.parse(Z.render(doc2, self.ctx), self.ctx), doc2)
        self.assertEqual(Z.parse(Z.render(doc2, self.ctx, english=True), self.ctx), doc2)

    def test_decompose_and_rule_forms_round_trip_in_both_surfaces(self):
        ctx = Z.Context(QUEUE_SUBJECT)
        read = lambda t: Z.read(t, ctx, False)  # noqa: E731
        point = (("phase", 151), ("vote0", 1), ("vote1", 0), ("vote2", 0), ("action", 161), ("principal", 1),
                 ("authorized", 1))
        other = (("phase", 152), ("vote0", 0), ("vote1", 0), ("vote2", 1), ("action", 162), ("principal", 2),
                 ("authorized", 0))
        acts = [Z.act("component", "state", of="d1", owns=("phase", "vote0"), reads=("action",),
                      decides=("invalid_state",)),
                Z.act("component", "guard", of="d1", owns=(), reads=("authorized",), decides=()),
                Z.act("case", "state", n=1, when=read("action == Vote"),
                      option=Z.Option("Accept", None, (("phase", ("int", 151)),
                                                       ("vote0", read("vote0 || principal == 0"))))),
                Z.act("decompose", "d1", order=("state", "guard")),
                Z.act("rule", "t1", where=read("action == Execute && vote0"), option=Z.Option("Reject", "quorum_missing")),
                Z.act("decomposed", "d1", tuples=432, nodes=93, components=3),
                Z.act("blame", "d1", component="guard", count=3, of=40, where=read("!authorized && vote0")),
                Z.act("expected", "d1", component="state", example=point, required=Z.Option("Accept"),
                      actual=Z.Option("NoCase")),
                Z.act("interface", "d1", component="state", pair=(point, other), reads=("vote1", "authorized")),
                Z.act("exact", "t1", count=12), Z.act("holds", "t1", count=12),
                Z.act("exception", "t1", count=3, of=12, where=None, example=point, actual=Z.Option("Refused")),
                Z.act("missing", "t1", count=4, where=read("phase == Empty"), example=other),
                Z.act("same", "p2", **{"as": "p1"}, count=432, over="inputs", event=7, verdict="refuted"),
                Z.act("same", "i2", **{"as": "i1"}, count=27, over="states", event=3, verdict="inductive")]
        for a in acts:
            for english in (False, True):
                self.assertEqual(Z.parse_statement(Z.render_act(a, ctx, english), ctx, english), a)

    def test_family_documents_take_parameters(self):
        ctx = Z.Context(FAMILY_SUBJECT)
        src = ("zal fcis loop 1;\nsubject family finite-phase-machine;\nauthor model;\n"
               "scenario s1 from phase = $C - 1;\nstep s1 1: reset = false, authorized = true -> accept phase = $C;\n"
               "family f1 out0 = authorized ? (reset ? (phase == $C ? 0 : 1) : (phase == $C ? 2 : 3)) : ?c;\n")
        doc = Z.parse(src, ctx)
        for english in (False, True):
            self.assertEqual(Z.parse(Z.render(doc, ctx, english=english), ctx), doc)
        with self.assertRaises(Refusal):
            Z.parse(src.replace("subject family finite-phase-machine;", f"subject {ctx.tag} at {'0' * 64};"), ctx)

    def test_refusals(self):
        good = self.head()
        bad = [
            "program p1 out9 = 0;",                        # unknown output
            "program p1 out0 = nosuchfield;",              # unknown name
            "law l1 on reject nosuchreason: true;",        # unknown reason
            "invariant i1: quantity > 0;",                 # not a state field
            "program p1 out0 = $C;",                       # parameters need a family subject
            "family f1 out0 = 0;",                         # family programs need a family subject
            "step s1 1: action = Reserve -> accept;",      # missing inputs
            "ask k1 at available = 0, reserved = 0, action = Reserve, quantity = 1, authorized = true: "
            "reject invalid_state | reject invalid_state;",
            "Program p1 computes out0 as 0;",              # surface switch mid-document
            "please make it faster;",                      # prose
            "step s1 1: action = Reserve, quantity = 2, authorized = true -> reject not_authorized available = 1;",
            "accept p1: 1 tuples, 2 nodes;",
            "fact f1: {braces};",
        ]
        for line in bad:
            with self.assertRaises(Refusal, msg=line):
                Z.parse(good + line + "\n", self.ctx)
        stale = self.document().replace(self.ctx.revision, "0" * 64)
        with self.assertRaises(Refusal):
            Z.parse(stale, self.ctx)
        with self.assertRaises(Refusal):
            Z.parse("zal fcis loop 1;\nsubject x at y;\nauthor model;\n", self.ctx)
        with self.assertRaises(Refusal):
            Z.parse(good.replace("author model", "author nobody"), self.ctx)

    def test_bounds_fail_closed(self):
        many = self.head() + "fact f1: x;\n" * (Z.MAX_STATEMENTS + 1)
        with self.assertRaises(Refusal):
            Z.parse(many, self.ctx)
        exactly = self.head() + "fact f1: x;\n" * Z.MAX_STATEMENTS
        self.assertEqual(len(Z.parse(exactly, self.ctx).acts), Z.MAX_STATEMENTS)
        with self.assertRaises(Refusal):
            Z.parse(self.head() + "fact f1: " + "x" * 70000 + ";\n", self.ctx)
        with self.assertRaises(Refusal):
            Z.parse(self.head() + "fact f1: café;\n", self.ctx)
        self.assertIn("program ID OUT = EXPR;", Z.legend())

    def test_variant_names_render_and_resolve(self):
        ctx = Z.Context(COUNTER_SUBJECT)
        node = Z.read(COUNTER_C8, ctx, False)
        found = []
        Z.substitute(node, lambda n: found.append(n) if n == ("int", 150) else None)
        self.assertTrue(found, "variant names resolve to their values")
        self.assertNotIn("Inc", Z.names_of(node))
        self.assertIn("action == Inc", Z.show(node, ctx, False))
        self.assertIn("action is Inc", Z.show(node, ctx, True))


if __name__ == "__main__":
    unittest.main()
