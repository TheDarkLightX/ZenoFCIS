import dataclasses
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import unittest

import behavior
import behavior2 as b
from behavior2 import Expr, Machine, Refusal, check, explain, parse, parse_any, semantic_diff, trace_properties
import factory

HERE = Path(__file__).parent
ORDER = (HERE / "examples/order2.zal").read_text()
LANE = (HERE / "examples/withdrawal2.zal").read_text()
KEEPER = (HERE / "examples/keeper2.zal").read_text()
V1_ORDER = (HERE / "examples/order.zal").read_text()
COUNTER = """zal 2;
machine counter;
states off, on;
events bump, reset;
context none;
var count in 0..=2 initially 0;
initial on;
step atomic; frame control_and_variables;
default reject not_enabled unchanged;
rule bump_count: on + bump [true] -> accept on, count += 1;
rule reset_count: on + reset [true] -> accept off, count = 0;
"""
RESETTABLE = "rule wait_last: waiting + tick [pause == 1] -> accept waiting, pause = 0, must_serve = 1;"


def oracle():
    sys.path.insert(0, str(HERE / "oracles"))
    import temporal_oracle
    return temporal_oracle


def lane_values(**values):
    return {"balance": 0, "must_serve": 0, "pause": 0, "reserved": 0, **values}


class Profile2Tests(unittest.TestCase):
    def test_paired_roundtrip_and_reordering(self):
        for source in [ORDER, LANE, KEEPER, COUNTER]:
            model = parse(source)
            self.assertEqual(parse(model.render()), model)
            self.assertEqual(parse(model.render(True), "english"), model)
            self.assertEqual(parse("\n".join(reversed(source.splitlines()))).revision, model.revision)
            self.assertEqual(model.data()["profile"], "zal/finite-fsm/2")
        swapped = parse(ORDER.replace("(event == approve & accepted & state' == approved)",
                                      "(state' == approved & accepted & event == approve)"))
        self.assertEqual(swapped.revision, parse(ORDER).revision)

    def test_english_goldens_reuse_ladder_temporal_templates(self):
        ladder = json.loads((HERE / "ladder_en.json").read_text())["temporal_templates"]
        for key, template in b.TEMPORAL_ENGLISH.items():
            self.assertEqual(ladder["intuitive"][key], template)
        for key in ["always", "eventually", "next", "until", "within"]:
            self.assertEqual(ladder["typed"][key], b.TEMPORAL_SYMBOLIC[key])
        english = parse(KEEPER).render(True).splitlines() + parse(LANE).render(True).splitlines()
        for line in [
            "Property paid_within_four_ticks: always (if (state is idle and state after this event is waiting), then "
            "now or within the next 4 events, (state is waiting and state after this event is idle)).",
            "Property must_serve_pays_next: always (if (must_serve after this event equals 1 and state after this event "
            "is waiting), then at the next event, which must exist, (state after this event is idle)).",
            "Property no_payout_while_paused: never (pause is greater than 0 and state is waiting and state after this "
            "event is idle).",
            "Property reserved_until_paid: always (if (accepted and event is request), then (state after this event is "
            "waiting) holds until (state is waiting and state after this event is idle) happens, and that must happen).",
            "Fairness keeper_ticks: assume (event is tick) happens infinitely often.",
            "Rule pay: when the state is waiting and the event is tick, if (pause equals 0 and (must_serve equals 1 or "
            "(not alarm))), move to idle with balance decreased by reserved and must_serve set to 0 and reserved set to 0.",
            "The variable balance is an integer from 0 to 4 inclusive, initially 0.",
            "The input amount is an integer from 1 to 2 inclusive."]:
            self.assertIn(line, english)
        symbolic = parse(KEEPER).render().splitlines()
        self.assertIn("property paid_within_four_ticks: always((state == idle & state' == waiting) => "
                      "within(4, state == waiting & state' == idle));", symbolic)

    def test_connective_renderings_and_truth(self):
        declarations = {"facts": {"aa", "bb"}, "inputs": set(), "variables": set(), "states": {"s"}, "events": {"e"}}
        implication = b.parse_formula("aa => bb", declarations, "guard")
        equivalence = b.parse_formula("aa <=> bb", declarations, "guard")
        self.assertEqual(implication.render(True), "(if aa, then bb)")
        self.assertEqual(equivalence.render(True), "(aa if and only if bb)")
        self.assertEqual(b.parse_formula("if aa, then bb", declarations, "guard", english=True), implication)
        self.assertEqual(b.parse_formula(equivalence.render(True), declarations, "guard", english=True), equivalence)
        rows = []
        for aa, bb in [(False, False), (False, True), (True, False), (True, True)]:
            step = b._Step("s", {}, "e", {"aa": aa, "bb": bb})
            rows.append((b.holds(implication, step), b.holds(equivalence, step)))
        self.assertEqual(rows, [(True, True), (True, False), (False, False), (True, True)])
        line = next(x for x in parse(ORDER).render(True).splitlines() if x.startswith("Requirement"))
        self.assertEqual(line, "Requirement approve_only_if_authorized: (if (accepted and event is approve and "
                               "state after this event is approved), then authorized).")
        with self.assertRaisesRegex(Refusal, "chained implications"):
            b.parse_formula("aa => bb => aa", declarations, "guard")

    def test_weakened_order_seed_fails_with_requirement_counterexample(self):
        self.assertEqual(check(parse(ORDER))["status"], "pass-with-scope")
        report = check(parse(ORDER.replace("[authorized]", "[true]")))
        self.assertEqual((report.get("status"), report.get("obligation"), report.get("requirement")),
                         ("counterexample", "requirement", "approve_only_if_authorized"))
        witness = report["witness"]
        self.assertEqual((witness["step"]["state"], witness["step"]["event"], witness["step"]["context"]),
                         ("pending", "approve", {"authorized": False}))
        self.assertTrue(witness["reachable"])
        self.assertEqual([s["event"] for s in witness["trace"]], ["submit"])
        self.assertFalse(witness["tree"]["value"])
        # Profile 1 keeps its documented intent gap byte for byte.
        weakened_v1 = behavior.parse(V1_ORDER.replace("[authorized]", "[true]"))
        self.assertEqual(behavior.check(weakened_v1)["status"], "pass-with-scope")

    def test_update_leaving_range_traps(self):
        model = parse(COUNTER)
        report = check(model)
        self.assertEqual((report.get("status"), report.get("obligation")), ("counterexample", "range"))
        witness = report["witness"]
        self.assertEqual((witness["variable"], witness["value"], witness["range"], witness["reachable"]),
                         ("count", 3, [0, 2], True))
        self.assertEqual([s["event"] for s in witness["trace"]], ["bump", "bump"])
        with self.assertRaisesRegex(Refusal, "outside 0..=2; a step never wraps around"):
            model.decide("on", {"count": 2}, "bump", {})
        self.assertEqual(explain(model, "on", {"count": 2}, "bump", {})["outcome"]["class"], "Trap")
        self.assertEqual(model.decide("on", {"count": 1}, "bump", {})["variables"], {"count": 2})
        guarded = parse(COUNTER.replace("on + bump [true]", "on + bump [count < 2]"))
        self.assertEqual(check(guarded)["status"], "pass-with-scope")
        below = check(parse(COUNTER.replace("count += 1", "count -= 1")))
        self.assertEqual((below.get("obligation"), below.get("witness", {}).get("value")), ("range", -1))

    def test_requirement_reads_post_state(self):
        self.assertEqual(check(parse(LANE))["status"], "pass-with-scope")
        report = check(parse(LANE.replace("accept idle, balance -= reserved", "accept idle, balance -= 1")))
        self.assertEqual((report.get("obligation"), report.get("requirement")), ("requirement", "payout_moves_reserved"))
        step = report["witness"]["step"]
        self.assertEqual(step["outcome"]["variables"]["balance"], step["variables"]["balance"] - 1)
        self.assertGreater(step["variables"]["reserved"], 1)
        counted = parse(COUNTER.replace("on + bump [true]", "on + bump [count < 2]") +
                        "require bump_counts_one: (event == bump & accepted) => count' == count + 1;\n")
        self.assertEqual(check(counted)["status"], "pass-with-scope")
        self.assertEqual(check(parse(counted.render().replace("count' == count + 1", "count' == count"))).get("obligation"),
                         "requirement")

    def test_liveness_needs_fairness(self):
        unfair = LANE.replace("fairness keeper_ticks: infinitely often (event == tick);\n", "")
        with self.assertRaisesRegex(Refusal, "Property never_freezes: liveness needs a fairness declaration"):
            parse(unfair)
        with self.assertRaisesRegex(Refusal, "Property reserved_until_paid: liveness needs a fairness declaration"):
            parse("\n".join(line for line in unfair.splitlines() if "never_freezes" not in line))
        with self.assertRaisesRegex(Refusal, "liveness needs a fairness declaration"):
            dataclasses.replace(parse(LANE), fairness=())
        self.assertEqual(parse(KEEPER).fairness, ())

    def test_liveness_counterexample_is_a_fair_lasso(self):
        model = parse(LANE.replace(RESETTABLE, RESETTABLE.replace(", must_serve = 1", "")))
        report = check(model)
        self.assertEqual((report.get("obligation"), report.get("property")), ("property", "never_freezes"))
        witness = report["witness"]
        prefix, cycle = witness["prefix"], witness["cycle"]
        self.assertTrue(cycle, "a liveness counterexample needs a nonempty cycle")
        word = prefix + cycle
        state, values = model.initial, model.initial_values()
        for row in word:
            self.assertEqual((row["state"], row["variables"]), (state, values))
            self.assertEqual(model.decide(row["state"], row["variables"], row["event"], row["context"]), row["outcome"])
            state, values = row["outcome"]["state"], row["outcome"]["variables"]
        self.assertEqual((state, values), (cycle[0]["state"], cycle[0]["variables"]))
        temporal = oracle()
        self.assertTrue(any(temporal.atom(model.fairness[0][1].data(), row) for row in cycle))
        self.assertFalse(temporal.on_lasso(model.properties[0][1].data(), word, len(prefix), 0))

    def test_within_is_inclusive_and_exact(self):
        self.assertEqual(check(parse(KEEPER))["status"], "pass-with-scope")
        report = check(parse(KEEPER.replace("within(4,", "within(3,")))
        self.assertEqual((report.get("obligation"), report.get("property")), ("property", "paid_within_four_ticks"))
        trace = report["witness"]["trace"]
        trigger = len(trace) - 4
        self.assertEqual((trace[trigger]["state"], trace[trigger]["outcome"]["state"]), ("idle", "waiting"))
        self.assertTrue(all(not (row["state"] == "waiting" and row["outcome"]["state"] == "idle") for row in trace[trigger:]))
        now = "property now_waiting: always((state == idle & state' == waiting) => within(0, state' == waiting));\n"
        self.assertEqual(check(parse(KEEPER + now))["status"], "pass-with-scope")
        later = now.replace("within(0, state' == waiting)", "within(0, state == waiting)")
        self.assertEqual(check(parse(KEEPER + later)).get("property"), "now_waiting")

    def test_finite_trace_strong_next_and_inclusive_within(self):
        model = parse(KEEPER)
        arrive = {"event": "tick", "context": {"alarm": True, "request": True}}
        quiet = {"event": "tick", "context": {"alarm": False, "request": False}}
        result = trace_properties(model, [arrive, quiet, quiet, arrive])
        self.assertEqual(result["properties"]["must_serve_pays_next"], {"result": "satisfied", "position": None})
        self.assertEqual(result["properties"]["paid_within_four_ticks"],
                         {"result": "refused", "diagnostic": "within k needs k less than the trace length in finite mode"})
        # The keeper sets must_serve on its third tick here, and the trace ends there: strong next is false.
        ended = trace_properties(model, [arrive, quiet, quiet])
        self.assertEqual(ended["properties"]["must_serve_pays_next"], {"result": "counterexample", "position": 2})
        pending = trace_properties(model, [arrive, quiet, quiet, quiet, quiet])
        self.assertEqual(pending["properties"]["paid_within_four_ticks"]["result"], "satisfied")
        temporal = oracle()
        counts = {"finite_verdicts": 0, "finite_refusals": 0}
        temporal.finite_truth_table([(0, 1, 2, 0), (2, 2, 1, 0), (1, 0, 0, 2)], counts)
        self.assertGreater(counts["finite_verdicts"], 2000)

    def test_machine_verdicts_match_independent_oracle(self):
        result = oracle().main(quick=True)
        self.assertEqual(result["status"], "passed")
        self.assertGreater(result["counts"]["lassos_validated"], 50)
        self.assertGreater(result["counts"]["traces_validated"], 50)

    def test_explain_counters_updates_requirements(self):
        model = parse(LANE)
        report = explain(model, "waiting", lane_values(balance=2, reserved=1), "tick", {"alarm": False, "amount": 1}, "id")
        self.assertEqual(report["outcome"], {"class": "Accept", "state": "idle", "rule": "pay", "reason": None,
                                             "variables": lane_values(balance=1)})
        pay = next(r for r in report["rules"] if r["id"] == "pay")
        self.assertTrue(pay["enabled"])
        self.assertIn({"variable": "balance", "operation": "-=", "term": "reserved", "english": "balance decreased by reserved",
                       "before": 2, "after": 1, "in_range": True, "range": [0, 4]}, pay["updates"])
        moved = next(r for r in report["requirements"] if r["id"] == "payout_moves_reserved")
        self.assertTrue(moved["value"])
        self.assertEqual(moved["tree"]["operands"][1]["operands"][0]["left"], {"operator": "var_next", "value": 1, "name": "balance"})
        self.assertTrue(all(i["before"] and i["after"] for i in report["invariants"]))
        self.assertTrue(report["admissible"])
        self.assertFalse(explain(model, "idle", lane_values(reserved=1, balance=1), "tick", {"alarm": False, "amount": 1})["admissible"])
        self.assertEqual(report["supplied_assumptions"], {"observations": {"alarm": False, "amount": 1}})
        order = parse(ORDER)
        for state, values, event, context in order.inputs_domain():
            self.assertEqual(explain(order, state, values, event, context)["outcome"], order.decide(state, values, event, context))
        for bad in [({}, {"alarm": False, "amount": 1}), (lane_values(balance=5), {"alarm": False, "amount": 1}),
                    (lane_values(), {"alarm": 0, "amount": 1}), (lane_values(), {"alarm": False, "amount": True}),
                    (lane_values(), {"alarm": False, "amount": 1, "extra": 1}), (lane_values(), {"alarm": False, "amount": 3})]:
            with self.subTest(bad=bad), self.assertRaises(Refusal):
                explain(model, "idle", bad[0], "tick", bad[1])

    def test_semantic_diff_profile2(self):
        model = parse(LANE)
        self.assertEqual(semantic_diff(model, model)["comparison"], "same-functional-observations-on-declared-domain")
        changed = semantic_diff(model, parse(LANE.replace("pause = 2;", "pause = 1;")))
        self.assertEqual(changed["comparison"], "different")
        self.assertIn("idle_alarm", changed["changed_objects"])
        self.assertTrue(any(w["after"]["variables"]["pause"] == 1 for w in changed["witnesses"]))
        trap = semantic_diff(parse(COUNTER.replace("[true] -> accept on", "[count < 2] -> accept on")), parse(COUNTER))
        self.assertTrue(any(w["after"]["class"] == "Trap" for w in trap["witnesses"]))
        self.assertEqual(semantic_diff(model, parse(LANE.replace("0..=4 initially 0", "0..=3 initially 0")))["comparison"],
                         "domain-changed")
        initial = semantic_diff(model, parse(LANE.replace("0..=4 initially 0", "0..=4 initially 1")))
        self.assertEqual(initial["witnesses"][0]["kind"], "initial-configuration")

    def test_named_refusals_and_bounds(self):
        refusals = {
            "reserved word": ORDER.replace("context authorized", "context next"),
            "Unsupported temporal shape": KEEPER + "property nested: eventually(always(state == idle));\n",
            "within needs a literal bound": KEEPER.replace("within(4,", "within(65,"),
            "not allowed in guards": ORDER.replace("[authorized]", "[event == approve]"),
            "not allowed in invariants": LANE.replace("invariant reserve_covered: reserved <= balance;",
                                                       "invariant reserve_covered: reserved <= amount;"),
            "Boolean observation, not an integer": ORDER.replace("[authorized]", "[authorized + 1 > 0]"),
            "updates one variable more than once": COUNTER.replace("count += 1", "count += 1, count = 0"),
            "explicit outcome": COUNTER.replace("accept off, count = 0", "reject refused unchanged, count = 0"),
            "undeclared variable": COUNTER.replace("count += 1", "total += 1"),
            "Declared finite domain": LANE.replace("var reserved in 0..=2", "var reserved in 0..=40"),
            "inclusive range of 1..64": COUNTER.replace("0..=2 initially", "0..=64 initially"),
            "starts outside": COUNTER.replace("initially 0", "initially 3"),
            "not in the controlled grammar": ORDER + "Please also check fairness.\n",
            "Unknown state": ORDER.replace("state' == approved", "state' == shipped"),
        }
        for expected, source in refusals.items():
            with self.subTest(expected=expected), self.assertRaisesRegex(Refusal, expected):
                parse(source)
        for shape in ["always(state == idle) & always(pause == 0)", "next(next(state == idle))",
                      "always(pause == 0 => always(state == idle))", "within(1, eventually(state == idle))"]:
            with self.subTest(shape=shape), self.assertRaisesRegex(Refusal, "temporal|not allowed"):
                parse(KEEPER + f"property extra: {shape};\n")
        with self.assertRaisesRegex(Refusal, "Trace exceeds 64 steps"):
            trace_properties(parse(KEEPER), [{"event": "tick", "context": {"alarm": False, "request": False}}] * 65)

    def test_direct_ir_admission(self):
        model = parse(LANE)
        with self.assertRaises(Refusal):
            dataclasses.replace(model, variables=tuple(reversed(model.variables)))
        with self.assertRaises(Refusal):
            Expr("cmp", ("=<", Expr("int", (1,)), Expr("int", (2,))))
        with self.assertRaises(Refusal):
            Expr("add", (Expr("int", (1,)), Expr("add", (Expr("int", (1,)), Expr("int", (2,))))))
        with self.assertRaises(Refusal):
            Expr("int", (True,))
        rule = dataclasses.replace(model.rules[0], guard=Expr("event", ("tick",)))
        with self.assertRaisesRegex(Refusal, "not allowed in guards"):
            dataclasses.replace(model, rules=(rule, *model.rules[1:]))
        reject = dataclasses.replace(model.rules[3], target=None, reason="refused")
        with self.assertRaisesRegex(Refusal, "updates need an Accept"):
            dataclasses.replace(model, rules=(*model.rules[:3], reject, *model.rules[4:]))

    def test_player_name_cannot_shadow_a_rule_rejection_reason(self):
        source = (HERE / "examples/withdrawal_game2.zal").read_text()
        source = source.replace("open + withdraw [true] -> accept withdrawn",
                                "open + withdraw [true] -> reject blocked unchanged")
        source = source.replace("player user moves in open", "player blocked moves in open")
        source = source.replace("user can force", "blocked can force")
        message = "Players need distinct names and disjoint declared states; at most two players"
        with self.assertRaisesRegex(Refusal, message):
            parse(source)

        valid = parse((HERE / "examples/withdrawal_game2.zal").read_text())
        rejected = dataclasses.replace(valid.rules[0], target=None, reason="blocked", updates=())
        players = (("blocked", ("open",)), ("operator", ("paused", "withdrawn")))
        games = tuple((name, "blocked" if player == "user" else player, objective, payload)
                      for name, player, objective, payload in valid.games)
        with self.assertRaisesRegex(Refusal, message):
            dataclasses.replace(valid, players=players, games=games, rules=(rejected, *valid.rules[1:]))

    def test_lowering_profile2(self):
        for source, name in [(LANE, "temporal property never_freezes"), (KEEPER, "temporal property must_serve_pays_next")]:
            with self.assertRaisesRegex(Refusal, "unsupported lowering: " + name):
                factory.lower(parse(source))
        with self.assertRaisesRegex(Refusal, "unsupported lowering: unresolved arithmetic"):
            factory.lower(parse(ORDER + "unresolved arithmetic: fees round down;\n"))
        game = parse("""zal 2;
machine simple_game;
states aa, bb;
events tick;
context none;
initial aa;
step atomic; frame control_and_variables;
default reject not_allowed unchanged;
rule advance: aa + tick [true] -> accept bb;
rule remain: bb + tick [true] -> accept bb;
player operator moves in bb;
player user moves in aa;
property reaches: user can force eventually (state == bb);
""")
        with self.assertRaisesRegex(Refusal, "unsupported lowering: game players and properties; use export-game"):
            factory.lower(game)
        files = factory.lower(parse(ORDER))
        self.assertIn("law 520 require_approve_only_if_authorized_accept on accept = "
                      "((command.101 == 120 && post.100.110 == 150) -> context.102.160 == 1);", files["project.zeno"])
        self.assertNotIn("_reject on reject", files["project.zeno"])
        lane = dataclasses.replace(parse(LANE), fairness=(), properties=())
        files = factory.lower(lane)
        policy, mapping = json.loads(files["v2/policy.json"]), json.loads(files["zal-source-map.json"])
        self.assertEqual(len(policy["cases"]), len(lane.rules) + 1)
        self.assertEqual(policy["genesis"], {"110": 150, "170": 0, "171": 0, "172": 0, "173": 0})
        self.assertIn("type 107 int Variable0 in 0..=4;", files["project.zeno"])
        self.assertEqual(mapping["correspondence"], "tested correspondence, not a theorem")
        rows = files["tests/zal-authority-examples.txt"].splitlines()
        admissible = check(lane)["admissible_configurations"] * len(lane.events) * 4
        self.assertEqual((len(rows), mapping["reference_domain"]["admissible_tuples"]), (admissible, admissible))
        pay = next(c for c in policy["cases"] if c["rule"] == "pay")
        self.assertEqual(pay["post"], {"110": 150, "170": "pre.100.170 - pre.100.173", "171": 0, "173": 0})
        self.assertTrue(files["tests/decision-examples.txt"].startswith("150 0 0 0 0 120 0 1 | accept - 150 1 0 0 0 | -"))

    def test_game_export_decision_returns_full_result_and_refusal(self):
        game = parse((HERE / "examples/withdrawal_game2.zal").read_text())
        decision = b.export_game_decision(game, "keep_withdrawing", "/tmp/game.zeno")
        self.assertEqual((decision.status, decision.refusal, decision.node_count,
                          decision.exit_code), ("exported", None, 3, 0))
        self.assertEqual(json.loads(decision.status_json), {"status": "exported", "format": "contract-tables",
                                               "property": "keep_withdrawing", "nodes": 3,
                                               "out": "/tmp/game.zeno", "authority": "none"})
        self.assertTrue(decision.text.startswith("zeno 1;\n"))

        wrong_profile = b.export_game_decision(behavior.parse(V1_ORDER), "keep_withdrawing", "/tmp/game.zeno")
        self.assertEqual((wrong_profile.status, wrong_profile.refusal, wrong_profile.text,
                          wrong_profile.node_count, wrong_profile.exit_code),
                         ("refused", "Game export needs a profile-2 game property model", None, None, 2))

    def test_parse_any_dispatch_and_cli(self):
        self.assertIsInstance(parse_any(V1_ORDER), behavior.Model)
        self.assertEqual(parse_any(V1_ORDER).revision, behavior.parse(V1_ORDER).revision)
        self.assertIsInstance(parse_any(ORDER), Machine)
        self.assertEqual(parse_any(parse(ORDER).render(True), "english"), parse(ORDER))
        with self.assertRaises(Refusal):
            behavior.parse(ORDER)
        run = [sys.executable, "-B", str(HERE / "run.py")]
        checked = subprocess.run(run + ["check", str(HERE / "examples/keeper2.zal")], capture_output=True, text=True)
        self.assertEqual((checked.returncode, json.loads(checked.stdout)["profile"]), (0, "zal/finite-fsm/2"))
        mixed = subprocess.run(run + ["diff", str(HERE / "examples/order.zal"), str(HERE / "examples/order2.zal")],
                               capture_output=True, text=True)
        self.assertEqual(mixed.returncode, 2)
        self.assertIn("same profile", mixed.stderr)

    def test_registry_help_and_legend_have_no_drift(self):
        statements = set(b.grammar_patterns()) | set(b.statement_patterns())
        self.assertEqual(set(b.CONSTRUCTS), statements)
        for source in [ORDER, LANE, KEEPER]:
            model = parse(source)
            for entry in b.legend(model)["entries"]:
                self.assertIn(entry["concept"], b.CONSTRUCTS)
            for topic in [*b.CONSTRUCTS, *b.OPERATORS, *b._objects(model)]:
                result = b.help_topic(model, topic)
                self.assertEqual(result["revision"], model.revision)
                self.assertTrue(result["entries"], topic)
                if result["entry_kind"] == "object":
                    for entry in result["entries"]:
                        self.assertIn(entry["symbolic"], model.render().splitlines())
                        self.assertIn(entry["english"], model.render(True).splitlines())
        temporal_words = {"always", "never", "eventually", "next", "within", "until", "accepted"}
        symbols = set(b.COMPARISONS) | {"!", "&", "|", "=>", "<=>", "+", "-", "'"}
        self.assertEqual(set(b.OPERATORS), temporal_words | symbols)
        for word in temporal_words:
            self.assertIn(word, b.RESERVED)
        with self.assertRaisesRegex(Refusal, "Unknown help topic"):
            b.help_topic(parse(ORDER), "fuzzy")
        self.assertEqual(b.help_topic(parse(KEEPER), "idle_count")["topic"], "rule:idle_count")
        shared = parse(COUNTER.replace("events bump, reset;", "events bump, on;").replace("on + reset", "on + on"))
        with self.assertRaisesRegex(Refusal, "Ambiguous"):
            b.help_topic(shared, "on")
        self.assertEqual(b.help_topic(shared, "event:on")["entry_kind"], "object")

    def test_advisories_profile2(self):
        vacuous = parse(ORDER + "require draft_approval: (state == draft & state' == approved) => authorized;\n")
        kinds = {(a["kind"], a.get("requirement")) for a in check(vacuous)["advisories"]}
        self.assertIn(("requirement-premise-never-true", "draft_approval"), kinds)
        unreached = parse(KEEPER + "property idle_must_serve: always((must_serve == 1 & state == idle) => next(state == idle));\n")
        self.assertIn("property-trigger-never-reached", {a["kind"] for a in check(unreached)["advisories"]})
        impossible = parse(LANE.replace("infinitely often (event == tick)", "infinitely often (event == tick & state == idle & state' == waiting)"))
        report = check(impossible)
        self.assertEqual(report["status"], "pass-with-scope")
        self.assertIn("no-fair-execution", {a["kind"] for a in report["advisories"]})
        bare = parse(COUNTER.replace("on + bump [true]", "on + bump [count < 2]"))
        self.assertEqual(check(bare)["advisories"][0]["kind"], "no-application-claims")
        tautology = parse(KEEPER + "invariant pause_declared: pause >= 0;\n")
        self.assertIn("invariant-domain-tautology", {a["kind"] for a in check(tautology)["advisories"]})

    def test_admissible_domain_scope(self):
        source = """zal 2;
machine admissible;
states aa, bb;
events tick;
context none;
var x in 0..=2 initially 0;
initial aa;
step atomic; frame control_and_variables;
default reject not_enabled unchanged;
rule stay: aa + tick [x == 0] -> accept aa;
invariant small: x <= 1;
"""
        excluded = check(parse(source + "rule keep_large: bb + tick [x == 2] -> accept bb;\n"))
        self.assertEqual((excluded["status"], excluded["admissible_configurations"]), ("pass-with-scope", 4))
        admitted = check(parse(source + "rule grow: bb + tick [x == 1] -> accept bb, x += 1;\n"))
        self.assertEqual((admitted["obligation"], admitted["witness"]["state"]), ("invariant-preservation", "bb"))

    def test_profile1_goldens_unchanged(self):
        model = behavior.parse(V1_ORDER)
        self.assertEqual(model.revision, "a2f11dc7c2519e58354c4f0da47a17d0577a75d04fedf57f23dd55127bab02f6")
        digest = hashlib.sha256(json.dumps(behavior.check(model, "fixed"), sort_keys=True, default=str).encode()).hexdigest()
        self.assertEqual(digest[:16], "c6ab9bc15b87f431")


if __name__ == "__main__":
    unittest.main()
