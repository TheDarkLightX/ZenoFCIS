"""Review desk controls: what the page shows is ZAL's own answer, the desk's additions are bounded and labelled,
and the loopback page answers only itself. No browser, network or provider is used."""

import hashlib
import json
from pathlib import Path
import random
import re
import tempfile
import unittest
from unittest import mock

import behavior
import behavior2
import constructs
import explain
import explain2
import ladder
import review_desk as desk
from behavior import Refusal
from workflow import checker_identity

HERE = Path(__file__).resolve().parent
EXAMPLES = HERE / "examples"
SAFE = (1 << 53) - 1

# A counted limit. Five requirements pin the counter; nothing is claimed about locking.
LIMIT = """zal 2;
machine daily_limit;
states unlocked, locked;
events withdraw, lock, unlock, reset;
context none;
var used in 0..=3 initially 0;
initial unlocked;
step atomic; frame control_and_variables;
default reject not_enabled unchanged;
rule take: unlocked + withdraw [used < 3] -> accept unlocked, used += 1;
rule lock_it: unlocked + lock [true] -> accept locked;
rule unlock_it: locked + unlock [true] -> accept unlocked;
rule new_day_open: unlocked + reset [true] -> accept unlocked, used = 0;
rule new_day_shut: locked + reset [true] -> accept locked, used = 0;
require only_when_allowed: (accepted & event == withdraw) => (state == unlocked & used < 3);
require allowed_goes_through: (state == unlocked & used < 3 & event == withdraw) => accepted;
require counts_each_one: (accepted & event == withdraw) => used' == used + 1;
require reset_clears: (accepted & event == reset) => used' == 0;
require nothing_else_counts: (accepted & !(event == withdraw) & !(event == reset)) => used' == used;
"""
# The same statement with the guard of `take` opened: the fourth withdrawal leaves the declared range.
OVERFLOWS = LIMIT.replace("[used < 3] -> accept unlocked, used += 1", "[true] -> accept unlocked, used += 1")
# A two-player game. The merchant can always meet the assumption, until the rule that pays is deleted.
REFUND = """zal 2;
machine refund_game;
states requested, reviewing, refunded, refused;
events ask, look, pay, deny, appeal;
context none;
var denials in 0..=2 initially 0;
initial requested;
step atomic; frame control_and_variables;
default reject not_allowed unchanged;
rule customer_asks: requested + ask [true] -> accept reviewing;
rule merchant_pays: reviewing + pay [true] -> accept refunded;
rule merchant_denies: reviewing + deny [denials < 2] -> accept refused, denials += 1;
rule merchant_stalls: reviewing + look [true] -> accept reviewing;
rule customer_appeals: refused + appeal [true] -> accept reviewing;
player customer moves in requested, refused, refunded;
player merchant moves in reviewing;
fairness merchant_decides: infinitely often (!(state' == reviewing));
property refund_in_the_end: customer can force eventually (state == refunded);
"""
# The same game with an assumption the customer can make false by never coming back.
STARVED = (REFUND.replace("var denials in 0..=2 initially 0;\n", "")
           .replace("[denials < 2] -> accept refused, denials += 1", "[true] -> accept refused")
           .replace("fairness merchant_decides: infinitely often (!(state' == reviewing));",
                    "fairness merchant_returns: infinitely often (state' == reviewing);"))
# One invariant and one requirement. Two declared configurations break the invariant and are never reached.
LEDGER = """zal 2;
machine closing_ledger;
states open, closed;
events post, close;
context none;
var pending in 0..=2 initially 0;
initial open;
step atomic; frame control_and_variables;
default reject not_enabled unchanged;
rule post_entry: open + post [pending < 2] -> accept open, pending += 1;
rule close_ledger: open + close [true] -> accept closed, pending = 0;
invariant closed_has_nothing_pending: (state == closed => pending == 0);
require posting_counts: (accepted & event == post) => pending' == pending + 1;
"""
UNRESOLVED = """zal 2;
machine confirm_order;
states placed, paid, confirmed;
events pay, confirm;
context none;
initial placed;
step atomic; frame control_and_variables;
default reject not_enabled unchanged;
rule take_payment: placed + pay [true] -> accept paid;
rule confirm_it: paid + confirm [true] -> accept confirmed;
require confirmed_only_after_payment: (accepted & state' == confirmed) => state == paid;
unresolved effects: send a confirmation email when the order is confirmed;
"""
# Every operator of the three profiles once, to pin how each is written in symbols. Its liveness properties do not hold.
TOUR = """zal 3;
machine symbol_tour;
states high, low;
events hold, tune;
context busy, ready;
input turn in -1..=1;
var level in 0..=3 initially 1;
var spare in 0..=2 initially 0;
initial low;
step atomic; frame control_and_variables;
default reject idle unchanged;
rule lift: low + tune [(ready | !busy) & level + turn >= 0 & level + turn <= 3 & !(spare != 0)] -> accept high, level += turn;
rule rest: high + hold [(ready <=> busy) & level >= 1] -> accept low, spare = level - 1;
rule shed: high + tune [(busy => ready) & level > 1] -> accept high, level -= turn + 1;
rule stay: low + hold [false] -> reject closed unchanged;
invariant bounded: level <= 3 & spare >= 0;
require closed_is_quiet: !accepted => (level' == level & spare' == spare);
require lift_moves: (accepted & event == tune & state == low) => (state' == high & level' == level + turn);
fairness tunes: infinitely often (event == tune);
property calm: never(level' < 0);
property keeps: always(accepted => until(level >= 0, spare <= 2));
property rises: always((state == low & ready) => eventually(state' == high | busy));
property soon: always(accepted => within(2, true));
property walks: always(state' == high => next(event == tune | event == hold));
"""


def example(name):
    return (EXAMPLES / name).read_text(encoding="ascii")


def bundle_of(text, name="statement.zal", **options):
    return desk.statement_bundle(text, name, **options)


def zal_explain(model, profile, given, checker):
    if profile is behavior2:
        return behavior2.explain(model, given["state"], given["variables"], given["event"], given["context"], checker)
    return behavior.explain(model, given["state"], given["event"], given["context"], checker)


def zal_why_not(model, profile, given, want, scope, checker):
    if profile is behavior2:
        return explain2.why_not(model, given["state"], given["variables"], given["event"], given["context"], want, scope, checker)
    return explain.why_not(model, given["state"], given["event"], given["context"], want, scope, checker)


def edit(bundle, label):
    return next(row for row in bundle["edits"]["rows"] if row["label"] == label)


class Sentences(unittest.TestCase):
    """The one line the desk composes itself, compared with ZAL's why-not answer on whole small domains."""

    def test_composed_decision_sentence_is_the_first_line_of_why_not_in_every_profile(self):
        checker, compared = checker_identity(), {}
        for name, text, stride in (("order.zal", example("order.zal"), 1), ("limit", LIMIT, 1), ("refund", REFUND, 1),
                                   ("overflows", OVERFLOWS, 1), ("prepared_batch3.zal", example("prepared_batch3.zal"), 27),
                                   ("withdrawal2.zal", example("withdrawal2.zal"), 135)):
            model, profile = desk.read_statement(text)
            for given in list(desk.input_tuples(model, profile))[::stride]:
                expected = zal_why_not(model, profile, given, {"class": "Accept"}, "context", checker)["summary"][0]
                self.assertEqual(desk.decision_sentence(model, profile, given), expected, name)
                compared[model.data()["profile"]] = compared.get(model.data()["profile"], 0) + 1
        self.assertEqual(sorted(compared), [behavior.PROFILE, behavior2.PROFILE, behavior2.PROFILE_3])
        self.assertTrue(all(count >= 16 for count in compared.values()), compared)
        trap = {"state": "unlocked", "variables": {"used": 3}, "event": "withdraw", "context": {}}
        self.assertIn("would set used to 4, outside 0..=3, so the step is refused",
                      desk.decision_sentence(*desk.read_statement(OVERFLOWS), trap))


class ZalResults(unittest.TestCase):
    """The page data repeats ZAL's answers; it does not restate them."""

    SOURCES = {"order.zal": None, "order2.zal": None, "keeper2.zal": None, "withdrawal_game2.zal": None,
               "limit": LIMIT, "refund": REFUND, "ledger": LEDGER, "unresolved": UNRESOLVED, "overflows": OVERFLOWS}

    @classmethod
    def setUpClass(cls):
        cls.checker = checker_identity()
        cls.built = {name: bundle_of(text if text else example(name), name) for name, text in cls.SOURCES.items()}

    def test_report_legend_and_both_renderings_are_zals_own(self):
        for name, (bundle, model, profile) in self.built.items():
            with self.subTest(source=name):
                self.assertEqual(bundle["check"], profile.check(model, self.checker))
                self.assertEqual(bundle["check"]["authority"], "none")
                self.assertEqual((bundle["revision"], bundle["symbolic"]), (model.revision, model.render()))
                self.assertEqual([entry["symbolic"] for entry in bundle["declared"]], model.render().splitlines())
                self.assertEqual([entry["english"] for entry in bundle["declared"]], model.render(True).splitlines())
                self.assertEqual(bundle["checker"], self.checker)
                self.assertEqual(bundle["readings"], profile.legend(model).get("readings", []))

    def test_declared_names_are_the_models_own(self):
        bundle, model, _ = self.built["refund"]
        data = model.data()
        named = lambda concept: [entry["id"] for entry in bundle["declared"] if entry["concept"] == concept]
        self.assertEqual(named("rule"), [rule["id"] for rule in data["rules"]])
        self.assertEqual(named("game"), [game[0] for game in data["games"]])
        self.assertEqual(named("fairness"), [fair[0] for fair in data["fairness"]])
        self.assertEqual(named("player"), [player[0] for player in data["players"]])
        self.assertEqual([entry["id"] for entry in self.built["unresolved"][0]["declared"] if entry["concept"] == "unresolved"], [None])

    def test_decision_table_is_zals_explanation_of_every_input(self):
        for name, (bundle, model, profile) in self.built.items():
            table, tuples = bundle["decisions"], list(desk.input_tuples(model, profile))
            with self.subTest(source=name):
                self.assertEqual((table["status"], table["count"], len(table["index"])), ("complete", len(tuples), len(tuples)))
                self.assertEqual(len(tuples), profile.check(model, self.checker).get("input_tuples", len(tuples)))
                for given in tuples:
                    told = zal_explain(model, profile, given, self.checker)
                    row = table["rows"][table["index"][desk.input_key(bundle["axes"], given)]]
                    shown_rules = [{**fixed, **table["kept"]["rule"][at]} for fixed, at in zip(table["static"]["rules"], row[2])]
                    self.assertEqual(table["kept"]["outcome"][row[0]], told["outcome"])
                    self.assertEqual(bool(row[1]), bool(told.get("admissible", True)))
                    self.assertEqual([(rule["id"], rule["enabled"]) for rule in shown_rules],
                                     [(rule["id"], rule["enabled"]) for rule in told["rules"]])
                    for shown, rule in zip(shown_rules, told["rules"]):
                        if rule["state_matches"] and rule["event_matches"]:
                            self.assertEqual((shown["guard"], shown.get("updates")), (rule["guard"], rule.get("updates")))
                    self.assertEqual([table["kept"]["claim"][at] for at in row[3]],
                                     [{"value": r["value"], "tree": r.get("tree")} for r in told.get("requirements", [])])

    def test_configurations_that_break_an_invariant_are_marked_as_zal_marks_them(self):
        bundle, model, profile = self.built["ledger"]
        table = bundle["decisions"]
        inadmissible = sorted(key for key, at in table["index"].items() if not table["rows"][at][1])
        self.assertEqual(inadmissible, ["closed|1|close", "closed|1|post", "closed|2|close", "closed|2|post"])
        self.assertEqual((bundle["check"]["configurations"], bundle["check"]["admissible_configurations"]), (6, 4))
        row = table["rows"][table["index"]["closed|2|post"]]
        self.assertEqual(table["kept"]["claim"][row[4][0]]["before"], False)
        self.assertEqual(bundle["reachable"], ["closed|0", "open|0", "open|1", "open|2"])

    def test_every_table_sentence_is_the_first_line_of_why_not(self):
        compared = 0
        for name, (bundle, model, profile) in self.built.items():
            table = bundle["decisions"]
            for given in desk.input_tuples(model, profile):
                expected = zal_why_not(model, profile, given, {"class": "Accept"}, "context", self.checker)["summary"][0]
                row = table["rows"][table["index"][desk.input_key(bundle["axes"], given)]]
                self.assertEqual(table["kept"]["sentence"][row[5]], expected, name)
                compared += 1
        self.assertEqual(compared, sum(bundle["decisions"]["count"] for bundle, _, _ in self.built.values()))
        self.assertGreater(compared, 200)

    def test_a_changed_why_not_line_refuses_the_statement_instead_of_showing_a_stale_sentence(self):
        _, model, profile = self.built["limit"]
        actual = desk.why_not

        def reworded(*arguments):
            told = actual(*arguments)
            return {**told, "summary": ["Starting " + told["summary"][0], *told["summary"][1:]]}

        self.assertEqual(desk.check_sentences(model, profile, 32, self.checker), desk.SENTENCE_SAMPLES)
        with mock.patch.object(desk, "why_not", reworded):
            with self.assertRaisesRegex(Refusal, "no longer matches ZAL's why-not answer"):
                desk.check_sentences(model, profile, 32, self.checker)
            with self.assertRaises(Refusal):
                bundle_of(LIMIT)

    def test_nearest_opposite_decision_is_why_nots_own_answer(self):
        for name in ("limit", "order.zal", "keeper2.zal"):
            bundle, model, profile = self.built[name]
            table = bundle["decisions"]
            for given in list(desk.input_tuples(model, profile))[::3]:
                row = table["rows"][table["index"][desk.input_key(bundle["axes"], given)]]
                shown, outcome = table["kept"]["nearest"][row[6]], table["kept"]["outcome"][row[0]]
                want = {"class": "Reject" if outcome["class"] == "Accept" else "Accept"}
                told = zal_why_not(model, profile, given, want, "input", self.checker)
                first = told["minimal_changes"][0] if told["minimal_changes"] else None
                self.assertEqual((shown["want"], shown["status"], shown["distance"], shown["count"]),
                                 (want["class"], told["status"], told["distance"], told["minimal_count"]), name)
                self.assertEqual(shown["first"], first and {key: first[key] for key in ("changes", "input", "outcome")})

    def test_failing_statement_carries_zals_story_and_no_planted_edits(self):
        bundle, model, profile = self.built["overflows"]
        report = profile.check(model, self.checker)
        self.assertEqual((bundle["check"]["status"], bundle["check"]["obligation"]), ("counterexample", "range"))
        self.assertEqual(bundle["story"]["english"], explain.narrate_check(model, report, self.checker)["english"])
        self.assertIn("outside 0..=3", bundle["story"]["english"][-1])
        self.assertIsNone(bundle["edits"])
        self.assertIsNone(bundle["reachable"])

    def test_unresolved_obligations_keep_zals_status_and_text(self):
        bundle, model, _ = self.built["unresolved"]
        self.assertEqual(bundle["check"]["status"], "unsupported")
        self.assertEqual(bundle["check"]["unresolved"], list(model.obligations))
        self.assertEqual(len(model.obligations), 1)
        self.assertIsNotNone(bundle["edits"])

    def test_cross_checks_agree_with_zals_counts(self):
        for name, (bundle, model, profile) in self.built.items():
            report = profile.check(model, self.checker)
            with self.subTest(source=name):
                self.assertTrue(bundle["cross_checks"]["tuples"])
                self.assertGreater(bundle["cross_checks"]["sentences_compared"], 0)
                if report["status"] in desk.PASSING:
                    expected = report.get("reachable_configurations", len(report.get("reachable_states", [])))
                    self.assertEqual((bundle["cross_checks"]["reachable"], len(bundle["reachable"])), (True, expected))

    def test_every_number_is_an_integer_the_browser_keeps_exactly(self):
        def walk(value, where):
            if isinstance(value, dict):
                for key, child in value.items():
                    walk(child, f"{where}.{key}")
            elif isinstance(value, list):
                for child in value:
                    walk(child, where)
            elif not isinstance(value, (str, bool, type(None))):
                self.assertIs(type(value), int, where)
                self.assertLessEqual(abs(value), SAFE, where)
        for name, (bundle, _, _) in self.built.items():
            walk(json.loads(json.dumps(bundle)), name)

    def test_surface_and_profile_are_selected_as_run_py_selects_them(self):
        _, model, _ = self.built["limit"]
        english, profile = desk.read_statement(model.render(True))
        self.assertEqual((english.revision, profile), (model.revision, behavior2))
        self.assertIs(desk.read_statement(example("order.zal"))[1], behavior)
        self.assertEqual(desk.read_statement(example("prepared_batch3.zal"))[0].profile, behavior2.PROFILE_3)
        for name in ("relay_typed.zal", "shell_effects.zal"):
            with self.assertRaisesRegex(Refusal, "typed or effects profile"):
                desk.read_statement(example(name))
        for text in (None, 7, "", "zal 9;\n"):
            with self.assertRaises(Refusal):
                desk.read_statement(text)

    def test_large_domains_leave_out_the_table_and_the_breakdown_and_say_why(self):
        with mock.patch.object(desk, "MAX_TABLE_TUPLES", 10), mock.patch.object(desk, "MAX_BREAKDOWN_TUPLES", 10):
            bundle = bundle_of(LIMIT)[0]
        self.assertEqual(bundle["decisions"]["status"], "not-precomputed")
        self.assertIn("32 input tuples", bundle["decisions"]["reason"])
        self.assertIsNone(bundle["reachable"])
        self.assertEqual((bundle["edits"]["breakdown"], bundle["edits"]["columns"][0]["id"]), (False, "allowed_goes_through"))
        self.assertIn("stops at 10", bundle["edits"]["breakdown_note"])
        self.assertTrue(all(row["noticed_by"] == [] for row in bundle["edits"]["rows"]))
        self.assertEqual({row["class"] for row in bundle["edits"]["rows"]}, {"claim", "structure", "unnoticed"})
        with mock.patch.object(desk, "MAX_NEAREST_TUPLES", 10):
            table = bundle_of(LIMIT, with_edits=False)[0]["decisions"]
        self.assertEqual((table["status"], table["kept"]["nearest"]), ("complete", [None]))


class Symbols(unittest.TestCase):
    """Each line in symbols is the desk's spelling: the ladder's symbols, ZAL's structure, and ZAL's own checks of both."""

    @staticmethod
    def read(text):
        """The model and what a bundle says about its symbols, without the decision table a whole bundle carries."""
        model, profile = desk.read_statement(text)
        declared, count = desk.declarations(model, profile), sum(1 for _ in desk.input_tuples(model, profile))
        return {"declared": declared, "key": desk.reading_key(model, profile, declared),
                "formal": desk.check_formal(model, profile, count), "count": count}, model, profile

    @staticmethod
    def spelled(bundle):
        """{name or concept: (the line without its notes, what the rule leaves unchanged, its notes)}"""
        return {entry["id"] or (entry["symbolic"].split()[1] if entry["concept"] in ("input", "var") else entry["concept"]):
                ("".join(s[0] for s in entry["formal"] or [] if s[2:] != ["note"]), desk.plain(entry["more"]),
                 [s[0] for s in entry["formal"] or [] if s[2:] == ["note"]]) for entry in bundle["declared"]}

    def test_every_operator_is_spelled_with_the_symbol_of_zals_ladder(self):
        lines = self.spelled(self.read(TOUR)[0])
        self.assertEqual({name: lines[name] for name in ("states", "events", "facts", "turn", "level", "initial", "frame", "default")}, {
            "states": ("state ∈ {high, low}", "", []), "events": ("event ∈ {hold, tune}", "", []),
            "facts": ("busy, ready ∈ {false, true}", "", []), "turn": ("-1 ≤ turn ≤ 1", "", []),
            "level": ("0 ≤ level ≤ 3, initially level = 1", "", []), "initial": ("initially state = low", "", []),
            "frame": ("", "", []), "default": ("otherwise ⇒ ¬accepted ∧ unchanged", "", ["reason idle"])})
        self.assertEqual({name: lines[name] for name in ("lift", "rest", "shed", "stay")}, {
            "lift": ("state = low ∧ event = tune ∧ level+turn ≤ 3 ∧ level+turn ≥ 0 ∧ ¬(spare ≠ 0) ∧ (ready ∨ ¬busy) "
                     "⇒ accepted ∧ state′ = high ∧ level′ = level+turn", " ∧ spare′ = spare", []),
            "rest": ("state = high ∧ event = hold ∧ level ≥ 1 ∧ (ready ⇔ busy) ⇒ accepted ∧ state′ = low ∧ spare′ = level−1",
                     " ∧ level′ = level", []),
            "shed": ("state = high ∧ event = tune ∧ level > 1 ∧ (busy ⇒ ready) ⇒ accepted ∧ state′ = high ∧ level′ = level−turn−1",
                     " ∧ spare′ = spare", []),
            "stay": ("state = low ∧ event = hold ∧ false ⇒ ¬accepted ∧ state′ = low", " ∧ level′ = level ∧ spare′ = spare", ["reason closed"])})
        self.assertEqual({name: lines[name][0] for name in ("bounded", "closed_is_quiet", "lift_moves", "tunes", "calm", "keeps", "rises", "soon", "walks")}, {
            "bounded": "level ≤ 3 ∧ spare ≥ 0", "closed_is_quiet": "¬accepted ⇒ (level′ = level ∧ spare′ = spare)",
            "lift_moves": "(accepted ∧ event = tune ∧ state = low) ⇒ (level′ = level+turn ∧ state′ = high)",
            "tunes": "□◇ (event = tune)", "calm": "□(¬(level′ < 0))", "keeps": "□(accepted ⇒ ((level ≥ 0) U (spare ≤ 2)))",
            "rises": "□((ready ∧ state = low) ⇒ ◇(busy ∨ state′ = high))", "soon": "□(accepted ⇒ ◇[0,2](true))",
            "walks": "□(state′ = high ⇒ ○(event = hold ∨ event = tune))"})
        flagged = {entry["id"]: [segment[:2] for segment in entry["formal"] if segment[2:] == ["main"]]
                   for entry in self.read(TOUR)[0]["declared"] if entry["formal"] and entry["id"]}
        self.assertEqual({name: flagged[name] for name in ("lift", "bounded", "closed_is_quiet", "lift_moves", "calm")}, {     # the connective that joins the halves of a line
            "lift": [["⇒", "implies"]], "bounded": [["∧", "and"]], "closed_is_quiet": [["⇒", "implies"]], "lift_moves": [["⇒", "implies"]], "calm": []})
        order = self.spelled(self.read(example("order.zal"))[0])     # profile 1 has rules and no variables
        self.assertEqual(order["approve_order"], ("state = pending ∧ event = approve ∧ authorized ⇒ accepted ∧ state′ = approved", "", []))

    def test_the_symbols_and_their_shapes_are_read_from_the_ladder_registry(self):
        formula, temporal = ladder.REGISTRY["templates"]["formula"], ladder.REGISTRY["temporal_templates"]["formula"]
        node, p, q = behavior2.Expr, behavior2.Expr("fact", ("p",)), behavior2.Expr("fact", ("q",))
        value, one = node("var", ("n",)), node("int", (1,))
        written = lambda formed: desk.plain(desk.spell(formed, desk.FORMAL))
        for op, name in (("and", "and"), ("or", "or"), ("implies", "=>"), ("iff", "<=>")):
            self.assertEqual(written(node(op, (p, q))), "(" + formula[name].format(a="p", b="q") + ")", op)
        for op, name in (("==", "="), ("!=", "!="), ("<", "<"), ("<=", "<="), (">", ">"), (">=", ">=")):
            self.assertEqual(written(node("cmp", (op, value, one))), formula[name].format(a="n", b="1"), op)
        self.assertEqual((written(node("add", (value, one))), written(node("sub", (value, one))), written(node("not", (p,)))),
                         (formula["+"].format(a="n", b="1"), formula["-"].format(a="n", b="1"), formula["not"].format(a="p")))
        self.assertEqual(written(node("var_next", ("n",))) + "()", formula["prime_call"].format(name="n", args=""))
        for op in ("always", "eventually", "next"):
            self.assertEqual(written(node(op, (p,))), temporal[op].format(a="p"), op)
        self.assertEqual((written(node("until", (p, q))), written(node("within", (3, p)))),
                         (temporal["until"].format(a="p", b="q"), temporal["within"].format(k=3, a="p")))
        self.assertEqual(written(node("never", (p,))), temporal["always"].format(a=formula["not"].format(a="(p)")))   # the ladder: never(P) is always(not P)
        self.assertEqual(desk.OFTEN, "□◇")

    def test_zal_checks_the_spelling_of_every_formula_and_every_rule(self):
        for name, text in (("tour", TOUR), ("limit", LIMIT), ("refund", REFUND), ("ledger", LEDGER), ("order", example("order.zal")),
                           ("withdrawal2", example("withdrawal2.zal")), ("keeper2", example("keeper2.zal")),
                           ("prepared_batch3", example("prepared_batch3.zal")), ("ladder_symmetry2", example("ladder_symmetry2.zal"))):
            with self.subTest(source=name):
                told, model, profile = self.read(text)
                decided = sum(1 for _ in desk.decided_steps(model, profile))
                goals = sum(len(payload) if isinstance(payload, tuple) else 1 for _, _, _, payload in getattr(model, "games", ()))
                formulas = (len(model.rules) + len(model.invariants) + goals
                            + sum(len(getattr(model, part, ())) for part in ("requirements", "fairness", "properties")))
                self.assertEqual(told["formal"], {"formulas": formulas, "rules": len(model.rules), "rules_parsed": len(model.rules),
                                                  "inputs": decided, "of": told["count"]})
                for formula in [rule.guard for rule in model.rules] + [formula for _, formula in model.invariants]:
                    self.assertEqual(desk.plain(desk.spell(formula, desk.TYPED, exact=True)), formula.render())
        with mock.patch.object(desk, "MAX_FORMAL_INPUTS", 100):          # a large domain is sampled, and the page says how many
            told = self.read(example("withdrawal2.zal"))[0]["formal"]
        self.assertTrue(0 < told["inputs"] <= 100 < told["of"])
        whole = bundle_of(LEDGER, with_edits=False)[0]                   # a bundle carries the same three parts
        self.assertEqual((whole["declared"], whole["key"], whole["cross_checks"]["formal"]),
                         tuple(self.read(LEDGER)[0][part] for part in ("declared", "key", "formal")))

    def test_a_spelling_that_differs_from_zal_refuses_the_statement(self):
        genuine, node = desk.rule_formula, behavior2.Expr
        def changed(change):
            def formula(model, rule):
                before, then, unchanged = genuine(model, rule)
                return change(list(before), list(then), list(unchanged), rule)
            return formula
        wrong = {
            "another target": lambda before, then, unchanged, rule: (before, [then[0], node("state_next", ("locked" if rule.target == "unlocked" else "unlocked",)), *then[2:]], unchanged),
            "the event left out": lambda before, then, unchanged, rule: ([before[0], *before[2:], node("true")], then, unchanged),
            "an update left out": lambda before, then, unchanged, rule: (before, then[:2], unchanged),
            "an updated variable called unchanged": lambda before, then, unchanged, rule: (
                before, then, [node("cmp", ("==", node("var_next", ("used",)), node("var", ("used",))))]),
            "accepted where it rejects": lambda before, then, unchanged, rule: (before, [node("not", (then[0],)), *then[1:]], unchanged),
            "a left side that never holds": lambda before, then, unchanged, rule: ([*before, node("false")], then, unchanged),
        }
        for label, change in wrong.items():
            with self.subTest(defect=label), mock.patch.object(desk, "rule_formula", changed(change)):
                with self.assertRaisesRegex(Refusal, "symbols no longer match ZAL \\(step formula of rule"):
                    bundle_of(LIMIT, with_edits=False)                  # the statement is refused, not shown with a wrong line
        with mock.patch.dict(desk.TYPED, {"and": " | "}), self.assertRaisesRegex(Refusal, "typed spelling of"):
            self.read(LIMIT)
        genuine_spell = desk.spell
        def without_parentheses(formula, way, exact=False):            # the plain typed form loses its grouping; the exact form does not
            segments = genuine_spell(formula, way, exact)
            return segments if exact or way is not desk.TYPED else [segment for segment in segments if segment[0] not in ("(", ")")]
        with mock.patch.object(desk, "spell", without_parentheses), self.assertRaisesRegex(Refusal, "grouping of"):
            self.read(TOUR)
        with mock.patch.object(desk, "FORCES", ("<<", ">>")), self.assertRaisesRegex(Refusal, "game property refund_in_the_end"):
            self.read(REFUND)

    def test_what_a_rule_leaves_unchanged_continues_its_line(self):
        told, model, _ = self.read(example("withdrawal2.zal"))
        lines = self.spelled(told)
        names = {variable.name for variable in model.variables}
        for rule in model.rules:
            kept = names - {update.variable for update in rule.updates}
            self.assertEqual({part.split("′")[0] for part in lines[rule.id][1].split(" ∧ ") if part}, kept, rule.id)
            self.assertEqual(lines[rule.id][2], ["reason " + rule.reason] if rule.reason else [], rule.id)
        self.assertEqual(lines["pay"], ("state = waiting ∧ event = tick ∧ pause = 0 ∧ (must_serve = 1 ∨ ¬alarm) ⇒ accepted ∧ state′ = idle "
                                        "∧ balance′ = balance−reserved ∧ must_serve′ = 0 ∧ reserved′ = 0", " ∧ pause′ = pause", []))
        self.assertEqual(lines["lane_occupied"][0], "state = waiting ∧ event = request ⇒ ¬accepted ∧ state′ = waiting")

    def test_a_game_line_is_zals_own_formula_rung_with_its_goal_respelled(self):
        cases = {"withdrawal_game2.zal": ("⟨⟨user⟩⟩ □◇ (state = withdrawn)", ["[all fairness assumptions hold infinitely often]"]),
                 "deadline_game2.zal": ("⟨⟨user⟩⟩ within 1 (state = withdrawn)", ["[the other player (operator) chooses every context]"]),
                 "recurring_goals_game2.zal": ("⟨⟨user⟩⟩ (□◇ (state = open) ∧ □◇ (state = withdrawn))", [])}
        for name, (line, notes) in cases.items():
            bundle, model, _ = self.read(example(name))
            entry = next(entry for entry in bundle["declared"] if entry["concept"] == "game")
            self.assertEqual(self.spelled(bundle)[entry["id"]], (line, "", notes), name)
            self.assertEqual(behavior2.game_readings(model)[0]["formula_rung"], " ".join([line, *notes]), name)   # these goals need no respelling
        wider = REFUND.replace("eventually (state == refunded)", "eventually (state == refunded | denials >= 2)")
        bundle, model, _ = self.read(wider)
        self.assertEqual(self.spelled(bundle)["refund_in_the_end"][0], "⟨⟨customer⟩⟩ ◇ (denials ≥ 2 ∨ state = refunded)")
        self.assertIn("(denials >= 2 | state = refunded)", behavior2.game_readings(model)[0]["formula_rung"])

    def test_a_line_without_a_formula_form_has_none(self):
        for name, text in (("ladder_symmetry2", example("ladder_symmetry2.zal")), ("unresolved", UNRESOLVED), ("order", example("order.zal"))):
            bundle, model, _ = self.read(text)
            bare = {entry["concept"] for entry in bundle["declared"] if entry["formal"] is None}
            self.assertEqual(bare, {entry["concept"] for entry in bundle["declared"]} & {"header", "name", "frame", "player", "symmetry", "unresolved", "facts"}
                             - ({"facts"} if model.facts else set()), name)
            self.assertTrue(all(entry["more"] is None or entry["concept"] == "rule" for entry in bundle["declared"]))

    def test_the_reading_key_quotes_zals_registries_and_marks_the_desks_own_rows(self):
        for name, text in (("tour", TOUR), ("refund", REFUND), ("order", example("order.zal")), ("deadline", example("deadline_game2.zal"))):
            with self.subTest(source=name):
                bundle, model, profile = self.read(text)
                key = bundle["key"]
                used = {segment[1] for entry in bundle["declared"] for segment in (entry["formal"] or []) + (entry["more"] or []) if segment[1:] and segment[1]}
                self.assertEqual(set(key["entries"]) | {"name:" + name for name in key["names"]}, used)
                for row, operator in desk.ZAL_OPERATOR.items():
                    if row in key["entries"]:
                        read, _, meaning = behavior2.OPERATORS[operator]
                        self.assertEqual((key["entries"][row]["read"], key["entries"][row]["meaning"], key["entries"][row].get("desk")), (read, meaning, None))
                self.assertEqual({row for row, told in key["entries"].items() if told.get("desk")}, used & {"in", "otherwise", "unchanged", "infinitely_often"})
                shown = {segment[1]: segment[0] for entry in bundle["declared"] for segment in entry["formal"] or [] if segment[1:] and segment[1] in key["entries"]}
                for row in set(shown) - {"within", "within_steps", "can_force"}:
                    self.assertEqual(key["entries"][row]["symbol"], shown[row], row)
                kinds = ({concept: told[2] for concept, told in behavior2.CONSTRUCTS.items()} if profile is behavior2 else
                         {construct.id: construct.meaning for construct in constructs.STATEMENTS})
                self.assertEqual(key["kinds"], {entry["concept"]: kinds[entry["concept"]] for entry in bundle["declared"]})
                self.assertEqual(key["spelling"], {op: desk.FORMAL[op] for op in desk.TYPED})
                self.assertEqual(key["wording"], {concept: note for concept, note in desk.WORDING.items() if concept in key["kinds"]})
                for variable in getattr(model, "variables", ()):
                    if variable.name in key["names"]:
                        self.assertEqual(key["names"][variable.name], {"role": "variable", "english": next(
                            entry["english"] for entry in bundle["declared"] if entry["symbolic"].startswith(f"var {variable.name} in "))})
        tour = self.read(TOUR)[0]["key"]
        self.assertEqual((tour["entries"]["implies"]["symbol"], tour["entries"]["implies"]["typed"], tour["entries"]["within"]["symbol"], tour["entries"]["within"]["typed"]),
                         ("⇒", "=>", "◇[0,K]", "within(K, P)"))
        self.assertEqual((tour["names"]["turn"]["role"], tour["names"]["busy"]["role"]), ("input", "context fact"))
        self.assertNotIn("always", self.read(example("order.zal"))[0]["key"]["entries"])
        self.assertEqual((set(tour["wording"]), set(self.read(example("order.zal"))[0]["key"]["wording"])), ({"rule", "default", "require"}, {"rule", "default"}))

    def test_generated_statements_keep_the_spelling_and_zal_in_agreement(self):
        pick = random.Random(20261110)
        def term():
            return pick.choice(["n", "m", "x"]) + "".join(pick.choice([" + ", " - "]) + pick.choice(["n", "m", "x", "1", "2", "-1"]) for _ in range(pick.randint(0, 2)))
        def formula(depth, step):
            roll = pick.random()
            if depth > 2 or roll < 0.35:
                atoms = ["f", "g", "true", "false", f"{term()} {pick.choice(behavior2.COMPARISONS)} {term()}", f"state == {pick.choice('ab')}"]
                if step:
                    atoms += ["accepted", f"event == {pick.choice(['go', 'stop'])}", f"state' == {pick.choice('ab')}", f"n' {pick.choice(['==', '>=', '<'])} {term()}"]
                return pick.choice(atoms)
            if roll < 0.5:
                return "!" + formula(depth + 1, step) if roll < 0.42 else "!(" + formula(depth + 1, step) + ")"
            if roll < 0.75:
                return "(" + pick.choice([" & ", " | "]).join(formula(depth + 1, step) for _ in range(pick.randint(2, 3))) + ")"
            return "(" + formula(depth + 1, step) + pick.choice([" => ", " <=> "]) + formula(depth + 1, step) + ")"
        accepted = 0
        for _ in range(120):
            third = pick.random() < 0.7
            lines = [f"zal {3 if third else 2};", "machine generated;", "states a, b;", "events go, stop;", "context f, g;", "input x in -2..=2;",
                     "var m in -3..=3 initially 0;", "var n in 0..=5 initially 1;", "initial a;", "step atomic; frame control_and_variables;", "default reject nope unchanged;"]
            for index in range(pick.randint(1, 3)):
                update = pick.choice(["", ", n += x - 1", ", m -= n - x + 1", ", n = m + 2", ", m = 0 - x, n += 1"] if third else ["", ", n += x", ", n -= 1", ", m = x, n = 2"])
                outcome = f"accept {pick.choice('ab')}{update}" if pick.random() < 0.8 else "reject why unchanged"
                lines.append(f"rule r{index}: {pick.choice('ab')} + {pick.choice(['go', 'stop'])} [{formula(0, False)}] -> {outcome};")
            lines.append(f"require q0: {formula(0, True)};")
            lines.append(f"property p0: always({formula(1, True)} => next({formula(1, True)}));")
            try:
                model, profile = desk.read_statement("\n".join(lines) + "\n")
            except Refusal:
                continue
            accepted += 1
            desk.reading_key(model, profile, desk.declarations(model, profile))
            told = desk.check_formal(model, profile, sum(1 for _ in desk.input_tuples(model, profile)))
            self.assertEqual((told["rules"], told["rules_parsed"]), (len(model.rules), len(model.rules)))
        self.assertGreater(accepted, 80)


class PlantedEdits(unittest.TestCase):
    """The desk's own instrument: each edit is re-checked and compared by ZAL, and classified from ZAL's answer."""

    @classmethod
    def setUpClass(cls):
        cls.checker = checker_identity()
        cls.limit, cls.limit_model, cls.limit_profile = bundle_of(LIMIT)
        cls.refund = bundle_of(REFUND)[0]

    def test_every_rule_is_edited_and_zal_rechecks_and_compares_each_edit(self):
        for text in (LIMIT, REFUND, example("keeper2.zal"), example("order.zal")):
            bundle, model, profile = bundle_of(text)
            lines, rows = model.render().splitlines(), bundle["edits"]["rows"]
            rules = model.data()["rules"]
            self.assertEqual([row["rule"] for row in rows if row["kind"] == "delete"], [rule["id"] for rule in rules])
            self.assertEqual(sum(row["kind"] == "open" for row in rows), sum(rule["guard"] != ["true"] for rule in rules))
            self.assertEqual(sum(row["kind"] == "strip" for row in rows), sum(bool(rule.get("updates")) for rule in rules))
            for row in rows:
                with self.subTest(machine=bundle["name"], edit=row["label"]):
                    self.assertEqual(lines[row["line"]], row["before"])
                    self.assertTrue(row["before"].startswith(f"rule {row['rule']}: "))
                    edited = lines[:row["line"]] + ([row["after"]] if row["after"] else []) + lines[row["line"] + 1:]
                    candidate, _ = desk.read_statement("\n".join(edited) + "\n")
                    report = profile.check(candidate, self.checker)
                    diff = profile.semantic_diff(model, candidate)
                    self.assertEqual(row["status"], report["status"])
                    self.assertEqual((row["diff"]["comparison"], row["diff"]["witness_count"]),
                                     (diff["comparison"], len(diff["witnesses"])))
                    failed = [check for check, status in report["checks"].items() if status == "counterexample"]
                    self.assertEqual([row["failed_check"]], failed or [None])
                    self.assertEqual(len(failed), int(report["status"] == "counterexample"))
                    if report["status"] == "counterexample":           # structure exactly when ZAL's obligation is structural
                        self.assertEqual(row["class"], "structure" if report["obligation"] in ("determinism", "range") else "claim")
                    for shown, witness in zip(row["diff"]["witnesses"], diff["witnesses"]):
                        self.assertEqual({key: value for key, value in shown.items() if key != "sentences"}, witness)

    def test_classes_follow_zals_answer_on_a_counted_limit(self):
        opened = edit(self.limit, "Replace the guard of rule take with true")
        self.assertEqual((opened["class"], opened["failed_check"], opened["noticed_by"]), ("structure", "update_ranges", []))
        stripped = edit(self.limit, "Remove the updates of rule take")
        self.assertEqual((stripped["class"], stripped["failed_check"], stripped["failed_name"], stripped["noticed_by"]),
                         ("claim", "transition_requirements", "counts_each_one", ["counts_each_one"]))
        self.assertEqual(edit(self.limit, "Delete rule take")["noticed_by"], ["allowed_goes_through"])
        for label in ("Delete rule lock_it", "Delete rule unlock_it"):  # nothing is claimed about locking
            self.assertEqual((edit(self.limit, label)["class"], edit(self.limit, label)["status"]), ("unnoticed", "pass-with-scope"))
        self.assertEqual([column["usable"] for column in self.limit["edits"]["columns"]], [True] * 5)
        self.assertTrue(self.limit["edits"]["breakdown"])

    def test_an_edit_is_attributed_to_the_claim_zal_names_and_an_invariant_failure_stops_the_asking(self):
        ledger = bundle_of(LEDGER)[0]
        self.assertEqual(ledger["edits"]["columns"], [{"id": "invariants", "label": "the invariants together", "usable": True},
                                                      {"id": "posting_counts", "label": "posting_counts", "usable": True}])
        unchecked = edit(ledger, "Remove the updates of rule close_ledger")
        self.assertEqual((unchecked["class"], unchecked["obligation"], unchecked["failed_check"], unchecked["failed_name"]),
                         ("claim", "invariant", "reachable_invariants", "closed_has_nothing_pending"))
        self.assertEqual(unchecked["noticed_by"], ["invariants"])        # the requirement is not asked once the invariant fails
        uncounted = edit(ledger, "Remove the updates of rule post_entry")
        self.assertEqual((uncounted["class"], uncounted["failed_check"], uncounted["noticed_by"]),
                         ("claim", "transition_requirements", ["posting_counts"]))
        overflow = edit(ledger, "Replace the guard of rule post_entry with true")
        self.assertEqual((overflow["class"], overflow["failed_check"], overflow["noticed_by"]), ("structure", "update_ranges", []))
        twice = LIMIT + "require withdraw_changes_count: (accepted & event == withdraw) => !(used' == used);\n"
        self.assertEqual(edit(bundle_of(twice)[0], "Remove the updates of rule take")["noticed_by"],
                         ["counts_each_one", "withdraw_changes_count"])   # each claim is asked alone, not only the first ZAL reports
        self.assertEqual(desk.failing_claim({"status": "counterexample", "obligation": "invariant-preservation", "invariant": "x"}), "invariants")
        self.assertIsNone(desk.failing_claim({"status": "counterexample", "obligation": "range"}))
        self.assertIsNone(desk.failing_claim({"status": "pass-with-scope"}))
        self.assertIsNone(desk.failing_claim(None))

    def test_distinguishing_inputs_carry_zals_sentence_before_and_after(self):
        row = edit(self.limit, "Delete rule lock_it")
        witness = row["diff"]["witnesses"][0]
        self.assertIn("rule lock_it accepts and moves to locked", witness["sentences"]["before"])
        self.assertIn("no rule is enabled, so the default rejects because not_enabled", witness["sentences"]["after"])
        given = {key: witness[key] for key in ("state", "variables", "event", "context")}
        self.assertEqual(witness["sentences"]["before"],
                         zal_why_not(self.limit_model, self.limit_profile, given, {"class": "Accept"}, "context", self.checker)["summary"][0])

    def test_the_documented_order2_weakening_is_noticed_by_its_requirement(self):
        row = edit(bundle_of(example("order2.zal"))[0], "Replace the guard of rule approve_order with true")
        self.assertEqual((row["class"], row["failed_check"], row["noticed_by"]),
                         ("claim", "transition_requirements", ["approve_only_if_authorized"]))

    def test_a_statement_without_claims_notices_nothing_and_says_so(self):
        bundle = bundle_of(example("order.zal"))[0]
        edits = bundle["edits"]
        self.assertEqual((edits["columns"], edits["breakdown"]), ([], False))
        self.assertIn("makes no claim", edits["breakdown_note"])
        self.assertEqual({row["class"] for row in edits["rows"]}, {"unnoticed"})
        self.assertEqual([advisory["kind"] for advisory in bundle["check"]["advisories"]], ["no-application-invariants"])

    def test_a_pass_that_lost_its_assumption_is_flagged(self):
        self.assertEqual(self.refund["edits"]["probes"],
                         [{"assumption": "merchant_decides", "claimant": "customer", "other": "merchant", "status": "pass-with-scope"}])
        row = edit(self.refund, "Delete rule merchant_pays")
        self.assertEqual((row["status"], row["class"], [lost["assumption"] for lost in row["lost_assumptions"]]),
                         ("pass-with-scope", "vacuous", ["merchant_decides"]))
        kept = edit(self.refund, "Delete rule merchant_stalls")
        self.assertEqual((kept["class"], kept["lost_assumptions"]), ("unnoticed", []))
        self.assertEqual(edit(self.refund, "Delete rule customer_asks")["noticed_by"], ["refund_in_the_end"])

    def test_an_assumption_the_claimant_can_starve_is_reported(self):
        probes = bundle_of(STARVED)[0]["edits"]["probes"]
        self.assertEqual(probes, [{"assumption": "merchant_returns", "claimant": "customer", "other": "merchant", "status": "counterexample"}])

    def test_probe_avoids_declared_names_and_stays_silent_when_zal_does_not_decide(self):
        renamed = REFUND.replace("rule customer_asks:", "rule desk_probe0:")   # ZAL keeps one namespace for these names
        self.assertEqual([probe["status"] for probe in bundle_of(renamed)[0]["edits"]["probes"]], ["pass-with-scope"])
        # ZAL admits only post-state fairness beside a game, which is why removing the primes restates it exactly.
        with self.assertRaisesRegex(Refusal, "Game fairness reads only the state after a step"):
            desk.read_statement(REFUND.replace("infinitely often (!(state' == reviewing))", "infinitely often (event == pay)"))
        with mock.patch.object(desk, "report_of", lambda lines, checker: None):
            model, profile = desk.read_statement(REFUND)
            declared = desk.declarations(model, profile)
            self.assertEqual(desk.assumption_probes([entry["symbolic"] for entry in declared], [entry["concept"] for entry in declared],
                                                    model.data(), self.checker), [])

    def test_edits_zal_refuses_are_reported_as_refused_with_its_reason(self):
        bundle = bundle_of(example("ladder_symmetry2.zal"))[0]
        refused = [row for row in bundle["edits"]["rows"] if row["class"] == "refused"]
        self.assertEqual({row["rule"] for row in refused}, {"bob_pledges", "carol_pledges"})
        self.assertTrue(all(row["status"] == "refused" and row["diagnostic"].startswith("Symmetry rivals") for row in refused))
        self.assertEqual(edit(bundle, "Delete rule lender_reopens")["class"], "vacuous")

    def test_a_rule_line_in_another_form_is_refused_not_guessed(self):
        declared = [{"concept": "rule", "symbolic": "rule odd: standing + push -> accept fallen;", "id": "odd"}]
        with self.assertRaisesRegex(Refusal, "not in the rendered form"):
            list(desk.rule_edits([declared[0]["symbolic"]], declared))


class Page(unittest.TestCase):
    def test_source_text_cannot_leave_the_data_slot(self):
        hostile = "# </script><script>alert(1)</script><!-- <b>\n" + LIMIT
        bundle = bundle_of(hostile)[0]
        self.assertIn("</script><script>alert(1)</script><!-- <b>", bundle["comment"][0])
        template = desk.PAGE.read_text(encoding="utf-8")
        before, after = template.split(desk.DATA_SLOT)
        shown = desk.page({"mode": "snapshot", "bundles": [bundle]}, "page-token")
        after = after.replace(desk.TOKEN_SLOT, "page-token")
        self.assertTrue(shown.startswith(before) and shown.endswith(after))
        data = shown[len(before):len(shown) - len(after)]
        self.assertNotIn("<", data)
        self.assertEqual(json.loads(data)["bundles"][0]["comment"], bundle["comment"])
        self.assertEqual((shown.count("</script>"), shown.count("<script>")), (1, 1))

    def test_page_has_one_slot_each_and_loads_nothing_else(self):
        template = desk.PAGE.read_text(encoding="utf-8")
        self.assertEqual((template.count(desk.DATA_SLOT), template.count(desk.TOKEN_SLOT)), (1, 1))
        self.assertEqual(set(re.findall(r"https?://[^\s\"'<>)]+", template)), {"http://www.w3.org/2000/svg"})
        self.assertIsNone(re.search(r"<(?:link|img|iframe|object|embed)\b|\bsrc=|@import|url\((?!#)", template))
        with mock.patch.object(desk, "PAGE", Path(__file__)):
            with self.assertRaisesRegex(Refusal, "exactly one data slot"):
                desk.page({})

    def test_page_names_every_check_zal_reports_and_agrees_on_which_are_structural(self):
        template = desk.PAGE.read_text(encoding="utf-8")
        glossed = dict(re.findall(r'^  (\w+): \[".*", "([sc])"\],$', template, re.M))
        self.assertEqual({name for name, group in glossed.items() if group == "s"}, set(desk.STRUCTURAL_CHECKS))
        reported = set()
        for text in (LIMIT, REFUND, example("order.zal")):
            model, profile = desk.read_statement(text)
            reported |= set(profile.check(model, checker_identity())["checks"])
        self.assertLessEqual(reported, set(glossed))
        self.assertLessEqual(set(desk.STRUCTURAL_CHECKS), reported)

    def test_snapshot_is_one_page_and_leaves_out_what_zal_refuses(self):
        with tempfile.TemporaryDirectory(prefix="zal-review-desk-") as folder:
            good, bad, out = Path(folder) / "limit.zal", Path(folder) / "bad.zal", Path(folder) / "page.html"
            good.write_text(LIMIT, encoding="ascii")
            bad.write_text("zal 2;\nmachine incomplete;\n", encoding="ascii")
            with mock.patch("sys.stderr"):
                desk.write_snapshot([good, bad], out)
                with self.assertRaises(SystemExit):
                    desk.write_snapshot([bad], Path(folder) / "none.html")
            page = out.read_text(encoding="utf-8")
            self.assertEqual(sorted(path.name for path in Path(folder).iterdir()), ["bad.zal", "limit.zal", "page.html"])
        data = json.loads(re.search(r"const DESK = (\{.*\});\nconst TOKEN = \"\";", page).group(1))
        self.assertEqual((data["mode"], [bundle["name"] for bundle in data["bundles"]]), ("snapshot", ["daily_limit"]))


class Exchanges(unittest.TestCase):
    """The request decisions of the loopback page, called directly: no socket is opened by these tests."""

    PORT, TOKEN = 8770, "the-page-token"

    def setUp(self):
        self.folder = tempfile.TemporaryDirectory(prefix="zal-review-desk-")
        self.addCleanup(self.folder.cleanup)
        self.path = Path(self.folder.name) / "limit.zal"
        self.path.write_text(LIMIT, encoding="ascii")
        self.statement = desk.OpenStatement(self.path)
        self.revision = desk.read_statement(LIMIT)[0].revision

    def call(self, method, target, body=None, raw=None, **headers):
        sent = {"Host": f"127.0.0.1:{self.PORT}", **{name.replace("_", "-"): value for name, value in headers.items()}}
        payload = raw if raw is not None else json.dumps(body) if body is not None else ""
        if payload:
            sent.setdefault("Content-Length", str(len(payload.encode())))
        reads = []

        def read(count):
            reads.append(count)
            return payload.encode()[:count]

        status, told, mime = desk.answer(self.statement, self.TOKEN, self.PORT, method, target, sent, read)
        self.assertLessEqual(sum(reads), desk.REQUEST_BOUND)
        return status, told if isinstance(told, str) else json.loads(json.dumps(told)), mime

    def post(self, route, body=None, raw=None, **headers):
        return self.call("POST", "/api/" + route, body, raw, X_ZAL_Review_Desk=self.TOKEN, **headers)[:2]

    def page_data(self):
        status, text, mime = self.call("GET", "/")
        self.assertEqual((status, mime), (200, "text/html"))
        return json.loads(re.search(r"const DESK = (\{.*\});\nconst TOKEN = ", text).group(1))

    def changed(self):
        return self.call("GET", "/api/changed", X_ZAL_Review_Desk=self.TOKEN)[1]["changed"]

    def test_only_this_server_and_this_page_are_answered(self):
        status, text, _ = self.call("GET", "/")
        self.assertEqual(status, 200)
        self.assertIn(f'const TOKEN = "{self.TOKEN}";', text)
        body = {"revision": self.revision, "input": {"state": "unlocked", "variables": {"used": 0}, "event": "lock", "context": {}}}
        refused = (403, {"error": "The review desk page's own token is required"}, "application/json")
        self.assertEqual(self.call("GET", "/", Host="review.example:80")[0], 403)
        self.assertEqual(self.call("GET", "/", Host=f"localhost:{self.PORT}")[0], 403)
        self.assertEqual(self.call("POST", "/api/explain", body), refused)
        self.assertEqual(self.call("POST", "/api/explain", body, X_ZAL_Review_Desk="not-the-token"), refused)
        self.assertEqual(self.call("POST", "/api/explain", body, X_ZAL_Review_Desk="caf\u00e9"), refused)   # a non-ASCII token is refused, not an error
        self.assertEqual(self.call("POST", "/api/explain", body, X_ZAL_Review_Desk=self.TOKEN, Origin="http://review.example"), refused)
        self.assertEqual(self.call("POST", "/api/explain", body, X_ZAL_Review_Desk=self.TOKEN, Host="review.example"), refused)
        self.assertEqual(self.call("PUT", "/api/explain", body, X_ZAL_Review_Desk=self.TOKEN), refused)
        self.assertEqual(self.call("GET", "/api/changed")[0], 404)
        self.assertEqual(self.call("GET", "/api/changed", X_ZAL_Review_Desk=self.TOKEN, Origin="http://review.example")[0], 404)
        self.assertEqual(self.call("GET", "/api/changed", X_ZAL_Review_Desk=self.TOKEN), (200, {"changed": False}, "application/json"))
        self.assertEqual(self.call("GET", "/favicon.ico")[0], 204)
        self.assertEqual(self.call("GET", "/review_desk.py")[0], 404)
        origin = f"http://127.0.0.1:{self.PORT}"
        self.assertEqual(self.post("explain", body, Origin=origin)[0], 200)
        for route in ("accept", "export", "reload", "discuss", "connect", ""):   # nothing records, exports or reaches a provider
            self.assertEqual(self.post(route, body), (400, {"error": "Unknown review desk action"}))

    def test_replies_forbid_framing_caching_and_outside_content(self):
        headers = dict(desk.RESPONSE_HEADERS)
        policy = dict(part.strip().split(" ", 1) for part in headers["Content-Security-Policy"].split(";"))
        self.assertEqual((policy["default-src"], policy["frame-ancestors"], policy["connect-src"], policy["base-uri"], policy["form-action"]),
                         ("'none'", "'none'", "'self'", "'none'", "'none'"))
        self.assertEqual((headers["Cache-Control"], headers["X-Content-Type-Options"], headers["Referrer-Policy"]),
                         ("no-store", "nosniff", "no-referrer"))

    def test_explain_is_zals_explanation_with_its_sentence_and_nearest_opposite(self):
        checker, (model, profile) = checker_identity(), desk.read_statement(LIMIT)
        given = {"state": "unlocked", "variables": {"used": 3}, "event": "withdraw", "context": {}}
        status, told = self.post("explain", {"revision": self.revision, "input": given})
        self.assertEqual(status, 200)
        expected = json.loads(json.dumps(behavior2.explain(model, "unlocked", {"used": 3}, "withdraw", {}, checker)))
        self.assertEqual({key: told[key] for key in expected}, expected)
        self.assertEqual((told["outcome"]["class"], told["authority"]), ("Reject", "none"))
        self.assertEqual(told["sentence"], zal_why_not(model, profile, given, {"class": "Accept"}, "context", checker)["summary"][0])
        nearest = zal_why_not(model, profile, given, {"class": "Accept"}, "input", checker)
        self.assertEqual((told["nearest"]["status"], told["nearest"]["distance"], told["nearest"]["count"]),
                         (nearest["status"], nearest["distance"], nearest["minimal_count"]))

    def test_stale_revisions_malformed_requests_and_inexact_numbers_are_refused(self):
        given = {"state": "unlocked", "variables": {"used": 0}, "event": "lock", "context": {}}
        self.assertEqual(self.post("explain", {"revision": "0" * 64, "input": given}),
                         (400, {"error": "This page shows an older revision; reload"}))
        self.assertEqual(self.post("explain", {"revision": self.revision, "input": {**given, "state": "open"}})[0], 400)
        self.assertEqual(self.post("explain", {"revision": self.revision})[0], 400)
        self.assertEqual(self.post("explain", raw="[]"), (400, {"error": "Expected a review desk request object"}))
        self.assertEqual(self.post("explain", raw="{")[0], 400)
        self.assertEqual(self.post("explain", raw=""), (400, {"error": "Request is empty or exceeds the review desk bound"}))
        self.assertEqual(self.post("explain", raw="{}", Content_Length="many")[0], 400)
        for lexeme, reason in (("1e400", "json-number-nonfinite"), ("-0", "json-number-negative-zero-integer"),
                               ("0.30000000000000001", "json-number-inexact-decimal"), ("NaN", "json-number-nonfinite")):
            raw = '{"revision":"%s","input":{"state":"unlocked","variables":{"used":%s},"event":"lock","context":{}}}' % (self.revision, lexeme)
            self.assertEqual(self.post("explain", raw=raw), (400, {"error": f"request: {reason}: {lexeme}"}))
        self.assertEqual(self.post("candidate", {"revision": self.revision, "source": "x" * desk.REQUEST_BOUND}),
                         (400, {"error": "Request is empty or exceeds the review desk bound"}))
        self.assertEqual(self.post("candidate", {"revision": self.revision, "source": LIMIT + "# " + "x" * desk.MAX_SOURCE})[1]["status"], "refused")
        self.assertEqual(self.post("candidate", {"revision": self.revision, "source": 7})[1]["status"], "refused")

    def test_candidate_is_parsed_checked_and_compared_by_zal(self):
        send = lambda source: self.post("candidate", {"revision": self.revision, "source": source})[1]
        same = send(LIMIT)
        self.assertEqual((same["status"], same["revision"], same["diff"]["witness_count"]), ("pass-with-scope", self.revision, 0))
        self.assertNotEqual(same["diff"]["comparison"], "different")
        overflow = send(OVERFLOWS)
        self.assertEqual((overflow["status"], overflow["obligation"], overflow["failed_check"]), ("counterexample", "range", "update_ranges"))
        self.assertIn("outside 0..=3", overflow["story"]["english"][-1])
        self.assertEqual(overflow["diff"]["witness_count"], 1)
        broken = send(LIMIT.replace("rule take:", "rule take now:"))
        self.assertEqual(broken["status"], "refused")
        self.assertEqual((broken["source_diagnostic"]["location"]["line"], broken["source_diagnostic"]["source_line"][:14]), (10, "rule take now:"))
        freer = send(LIMIT.replace("rule lock_it: unlocked + lock [true] -> accept locked;\n", ""))
        self.assertEqual((freer["status"], freer["diff"]["comparison"], freer["diff"]["witness_count"]), ("pass-with-scope", "different", 4))
        self.assertEqual(send(example("order.zal"))["diff"]["comparison"], "different-profile")
        self.assertEqual(self.page_data()["bundles"][0]["revision"], self.revision)   # a candidate replaces nothing

    def test_a_changed_or_refused_file_is_noticed_described_and_not_reread_forever(self):
        first = self.page_data()
        self.assertEqual((self.changed(), first["mode"], first["bundles"][0]["name"], first["file"]), (False, "live", "daily_limit", "limit.zal"))
        self.path.write_text(LIMIT.replace("machine daily_limit;", "machine weekly_limit;"), encoding="ascii")
        self.assertTrue(self.changed())
        self.assertEqual(self.post("explain", {"revision": self.revision, "input": {}})[0], 400)   # still the page's revision
        self.assertEqual((self.page_data()["bundles"][0]["name"], self.changed()), ("weekly_limit", False))
        self.assertEqual(self.post("explain", {"revision": self.revision, "input": {}}),
                         (400, {"error": "This page shows an older revision; reload"}))
        self.path.write_text(LIMIT.replace("rule take:", "rule take now:"), encoding="ascii")
        refused = self.page_data()
        self.assertEqual((refused["bundles"], refused["error"]["location"]["line"], refused["error"]["location"]["file"]), ([], 10, "limit.zal"))
        self.assertIn("controlled grammar", refused["error"]["reason"])
        self.assertEqual(self.post("explain", {"revision": self.revision}), (400, {"error": "ZAL refuses the file on disk; correct it and reload"}))
        for unreadable in (b"\xff" + LIMIT.encode(), LIMIT.encode() + b"#" * desk.MAX_SOURCE):
            self.path.write_bytes(unreadable)
            self.assertTrue(self.changed())
            self.assertEqual(self.page_data()["error"]["reason"], "Source must be ASCII text at most 64 KiB")
            self.assertFalse(self.changed())
        # Two game properties with one name make ZAL's parser raise a TypeError. The page says so and stays up.
        self.path.write_text(REFUND + "property refund_in_the_end: customer can force eventually (state == refused);\n", encoding="ascii")
        self.assertTrue(self.changed())
        failed = self.page_data()
        self.assertEqual((failed["bundles"], failed["error"]["unexpected"], failed["error"]["reason"][:10], self.changed()), ([], True, "TypeError:", False))
        # A line of the desk's own that no longer agrees with ZAL is the desk's error, not ZAL refusing the source.
        self.path.write_text(LEDGER, encoding="ascii")
        with mock.patch.dict(desk.TYPED, {"and": " | "}):
            own = self.page_data()
        self.assertEqual((own["bundles"], own["error"].get("unexpected")), ([], True))
        self.assertIn("The review desk's symbols no longer match ZAL", own["error"]["reason"])
        self.assertTrue(issubclass(desk.Disagreement, Refusal))
        self.path.unlink()
        self.assertTrue(self.changed())
        self.assertIn("could not be read", self.page_data()["error"]["reason"])
        self.assertFalse(self.changed())
        self.path.write_text(LIMIT, encoding="ascii")
        self.assertEqual((self.changed(), self.page_data()["bundles"][0]["revision"]), (True, self.revision))

    def test_an_oversized_file_is_not_read_in_full(self):
        self.path.write_bytes(b"#" * (4 * desk.MAX_SOURCE))
        self.assertEqual(len(desk.read_bounded(self.path)), desk.MAX_SOURCE + 1)
        with self.assertRaisesRegex(Refusal, "at most 64 KiB"):
            desk.source_text(desk.read_bounded(self.path))

    def test_nothing_is_written_beside_or_into_the_source_and_no_socket_is_opened(self):
        digest = lambda: {path.name: hashlib.sha256(path.read_bytes()).hexdigest() for path in Path(self.folder.name).iterdir()}
        before = digest()
        given = {"state": "unlocked", "variables": {"used": 1}, "event": "withdraw", "context": {}}
        with mock.patch("socket.socket", side_effect=AssertionError("No network is permitted")):
            self.page_data()
            self.assertEqual(self.post("explain", {"revision": self.revision, "input": given})[0], 200)
            self.assertEqual(self.post("candidate", {"revision": self.revision, "source": OVERFLOWS})[0], 200)
            self.assertFalse(self.changed())
            bundle_of(REFUND)
        self.assertEqual(digest(), before)


if __name__ == "__main__":
    unittest.main()
