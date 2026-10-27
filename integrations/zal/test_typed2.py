"""Typed profile 2 machines: exact elaboration, round trips, enumeration caps and typed-name refusals."""

import itertools
from pathlib import Path
import unittest

import behavior2
import typed2
from adt import Ty
from behavior import Refusal


ZAL = Path(__file__).resolve().parent
RELAY = (ZAL / "examples/relay_typed.zal").read_text()
# The relay's elaboration, written by hand in profile 2: answer sent=0, not_sent=1, unknown=2; false=0, true=1.
ELABORATED = """zal 2;
machine relay;
states awaiting, delivered, pending, stuck;
events reply, send;
context none;
input answer in 0..=2;
var acknowledged in 0..=1 initially 0;
var tries in 0..=2 initially 0;
initial pending;
step atomic; frame control_and_variables;
default reject not_enabled unchanged;
rule acknowledge: awaiting + reply [answer == 0] -> accept delivered, acknowledged = 1;
rule give_up: awaiting + reply [(answer != 0 & tries == 2)] -> accept stuck;
rule retry: awaiting + reply [(answer != 0 & tries <= 1)] -> accept pending;
rule send_once: pending + send [tries <= 1] -> accept awaiting, tries += 1;
invariant acknowledged_only_when_delivered: (acknowledged == 1 => state == delivered);
require delivered_only_on_a_sent_report: ((accepted & state' == delivered) => answer == 0);
property never_stuck_early: always((accepted & state' == stuck) => tries == 2);
"""
DOOR = """zal typed 1;
machine door;
states closed, open;
events push, tick;
context authorized;
type Level = int 1..=3;
type Lock = unlocked | locked(Level) | jammed;
var lock: Lock initially unlocked;
initial closed;
step atomic; frame control_and_variables;
default reject not_enabled unchanged;
rule escalate: closed + tick [lock == locked(2)] -> accept closed, lock = jammed;
rule lock_up: closed + tick [lock == unlocked] -> accept closed, lock = locked(2);
rule open_door: closed + push [(lock != jammed & authorized)] -> accept open;
rule shut: open + push [true] -> accept closed;
invariant never_jammed: lock != jammed;
"""


def canonical_source(text):
    return "\n".join(line for line in text.splitlines() if line.strip() and not line.startswith("#")) + "\n"


class ElaborationTests(unittest.TestCase):
    def test_elaboration_is_an_unchanged_profile_2_machine(self):
        model = typed2.parse(RELAY)
        expected = behavior2.parse(ELABORATED)
        self.assertEqual(model.machine, expected)
        self.assertEqual(model.machine.revision, expected.revision)
        self.assertEqual(model.machine.render(), ELABORATED)
        report = typed2.check(model)
        self.assertEqual(report["status"], "pass-with-scope")
        self.assertEqual({k: v for k, v in report.items() if k not in {"profile", "revision", "machine_revision", "encoding",
                                                                        "elaboration"}},
                         {k: v for k, v in typed2._decode(model, behavior2.check(expected)).items()
                          if k not in {"profile", "revision"}})
        self.assertEqual(next(e for e in report["encoding"] if e["name"] == "answer")["values"], ["sent", "not_sent", "unknown"])

    def test_both_surfaces_round_trip(self):
        for source in (RELAY, DOOR):
            model = typed2.parse(source)
            self.assertEqual(model.render(), canonical_source(source))
            english = model.render(True)
            again = typed2.parse(english, "english")
            self.assertEqual(again, model)
            self.assertEqual(again.render(True), english)
        self.assertIn("Rule acknowledge: when the state is awaiting and the event is reply, if answer equals sent, "
                      "move to delivered with acknowledged set to true.", typed2.parse(RELAY).render(True))

    def test_enumeration_is_a_bijection(self):
        """Equal values have equal indices and no two values share one, so == and != are preserved exactly."""
        model = typed2.parse(DOOR)
        values = model.values("lock")
        self.assertEqual(len(values), 5)
        for left, right in itertools.product(range(len(values)), repeat=2):
            self.assertEqual(values[left] == values[right], left == right)
        self.assertEqual([model.value_text("lock", i) for i in range(5)],
                         ["unlocked", "locked(1)", "locked(2)", "locked(3)", "jammed"])


class CapTests(unittest.TestCase):
    def refused(self, source, message):
        try:
            typed2.parse(source)
        except Refusal as error:
            self.assertIn(message, str(error))
            return
        except Exception as error:  # A missing check must fail here, not crash the suite.
            self.fail(f"not refused by name: {type(error).__name__}: {error}")
        self.fail("admitted")

    def test_enumeration_above_cap_refused_by_name(self):
        self.refused(DOOR.replace("type Level = int 1..=3;", "type Level = int 1..=63;"),
                     "Type Lock of lock has 65 values; a profile 2 variable or input admits at most 64")
        grid = DOOR.replace("type Level = int 1..=3;", "type Level = int 1..=3;\ntype Grid = {x: Eight, y: Eight};\n"
                            "type Eight = int 0..=7;\ntype Wide = {x: Eight, y: Eight, z: bool};")
        model = typed2.parse(grid.replace("var lock: Lock initially unlocked;",
                                          "var lock: Lock initially unlocked;\nvar cell: Grid initially {x: 0, y: 0};"))
        self.assertEqual(len(model.values("cell")), 64)
        self.refused(grid.replace("var lock: Lock initially unlocked;",
                                  "var lock: Lock initially unlocked;\nvar cell: Wide initially {x: 0, y: 0, z: false};"),
                     "Type Wide of cell has 128 values")
        self.refused(DOOR.replace("var lock: Lock initially unlocked;", "var lock: Lock initially unlocked;\n"
                                  "var blob: bytes ..=1 initially 0x;"), "has 257 values")

    def test_typed_names_only_compare_for_equality(self):
        rule = "rule probe: open + tick [{}] -> accept open;\n"
        for guard, message in [("lock < jammed", "supports only == and !="), ("lock + 1 == 2", "supports only == and !="),
                               ("lock == 2", "Not a value of Lock"), ("lock == locked(4)", "outside 1..=3"),
                               ("lock == locked( 2 )", "Write the value of lock canonically, as locked(2)")]:
            with self.subTest(guard=guard):
                self.refused(DOOR + rule.format(guard), message)
        self.refused(DOOR + "rule probe: open + tick [true] -> accept open, lock += 1;\n", "supports only == and !=")
        self.refused(DOOR + "rule probe: open + tick [true] -> accept open, lock = 1;\n", "Not a value of Lock")
        two = DOOR.replace("var lock: Lock initially unlocked;", "var lock: Lock initially unlocked;\nvar flag: bool initially false;\n"
                           "var count in 0..=2 initially 0;")
        typed2.parse(two + rule.format("lock == lock"))
        for guard in ["flag == lock", "count == lock", "count == flag"]:
            with self.subTest(guard=guard):
                self.refused(two + rule.format(guard), "a typed name is compared only with == or !=")
        self.refused(two + "rule probe: open + tick [true] -> accept open, count = flag;\n", "a typed variable is only assigned")
        self.refused(two + "rule probe: open + tick [true] -> accept open, flag = lock;\n", "a typed variable is only assigned")
        self.refused(DOOR.replace("context authorized;", "context jammed;").replace("& authorized)", "& jammed)"),
                     "A case name may not also name a context fact, input or variable: jammed")
        self.refused(DOOR.replace("var lock: Lock initially unlocked;", "var lock: Missing initially unlocked;"),
                     "refers to an undeclared type")


class WitnessTests(unittest.TestCase):
    def test_check_decodes_typed_witnesses(self):
        model = typed2.parse(DOOR)
        report = typed2.check(model)
        self.assertEqual((report["status"], report["obligation"]), ("counterexample", "invariant"))
        self.assertEqual(report["witness"]["variables"], {"lock": "jammed"})
        self.assertEqual([step["outcome"]["variables"]["lock"] for step in report["witness"]["trace"]],
                         ["locked(2)", "jammed"])
        self.assertEqual(report["revision"], model.revision)
        self.assertNotEqual(report["machine_revision"], model.revision)

    def test_explain_takes_and_returns_typed_values(self):
        model = typed2.parse(DOOR)
        result = typed2.explain(model, "closed", {"lock": "locked(2)"}, "tick", {"authorized": False})
        self.assertEqual((result["outcome"]["rule"], result["outcome"]["variables"]), ("escalate", {"lock": "jammed"}))
        guard = next(r for r in result["rules"] if r["id"] == "escalate")["guard"]
        self.assertEqual((guard["left"]["value"], guard["right"]["value"]), ("locked(2)", "locked(2)"))
        update = next(r for r in result["rules"] if r["id"] == "escalate")["updates"][0]
        self.assertEqual((update["before"], update["after"], update["english"]), ("locked(2)", "jammed", "lock set to jammed"))
        with self.assertRaises(Refusal):
            typed2.explain(model, "closed", {"lock": "locked(9)"}, "tick", {"authorized": False})
        with self.assertRaises(Refusal):
            typed2.explain(model, "closed", {"lock": 2}, "tick", {"authorized": False})


if __name__ == "__main__":
    unittest.main()
