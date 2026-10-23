"""Profile 2 why-not, what-if and narratives: goldens, brute-force oracles and an independent story oracle."""

import itertools
import json
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path
import random
import sys
import tempfile
import unittest
from unittest.mock import patch

import behavior2 as B
from behavior import Refusal
import explain
import explain2 as E
import run as CLI

HERE = Path(__file__).parent
sys.path.insert(0, str(HERE / "oracles"))
import temporal_oracle as O  # noqa: E402  (independent atom and lasso evaluator)

LANE = (HERE / "examples/withdrawal2.zal").read_text()
KEEPER = (HERE / "examples/keeper2.zal").read_text()
ORDER = (HERE / "examples/order2.zal").read_text()
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
UNTIL = """zal 2;
machine handoff;
states idle, held, paid, lost;
events ask, tick;
context drop;
initial idle;
step atomic; frame control_and_variables;
default reject not_enabled unchanged;
rule hold: idle + ask [true] -> accept held;
rule keep: held + tick [!drop] -> accept held;
rule lose: held + tick [drop] -> accept lost;
rule rest: lost + tick [true] -> accept lost;
rule done: paid + tick [true] -> accept idle;
fairness ticks: infinitely often (event == tick);
property held_until_paid: always((event == ask & accepted) => until(state' == held, state' == paid));
"""
COUNTEREXAMPLES = {
    "range": COUNTER,
    "range-below": COUNTER.replace("count += 1", "count -= 1"),
    "requirement": ORDER.replace("[authorized]", "[true]"),
    "requirement-lane": LANE.replace("accept idle, balance -= reserved", "accept idle, balance -= 1"),
    "liveness": LANE.replace(RESETTABLE, RESETTABLE.replace(", must_serve = 1", "")),
    "liveness-finite": UNTIL,
    "safety": KEEPER.replace("rule wait_count: waiting + tick [pause == 2] -> accept waiting", "rule wait_count: waiting + tick [pause == 2] -> accept idle"),
    "bounded": KEEPER.replace("within(4,", "within(2,"),
    "next": KEEPER.replace("[pause == 0 & (!alarm | must_serve == 1)] -> accept idle", "[pause == 0 & !alarm] -> accept idle"),
    "determinism": COUNTER + "rule also: on + bump [count == 0] -> accept off;\n",
    "invariant": COUNTER.replace("on + bump [true]", "on + bump [count < 2]") + "invariant small: count <= 1;\n",
    "invariant-preservation": COUNTER.replace("states off, on;", "states off, on, spare;").replace("on + bump [true]", "on + bump [count < 2]")
                              + "rule spare_go: spare + bump [true] -> accept off, count = 1;\n"
                              + "invariant reset_clears: state == off => count == 0;\n",
}
LANE_START = {"balance": 0, "must_serve": 0, "pause": 0, "reserved": 0}


def random_v2(rng):
    atoms = ["p", "!p", "x == 0", "x >= 1", "x < 2", "n == 1", "n == 0", "true"]
    updates = ["", ", x += 1", ", x -= 1", ", x = n", ", x = 0"]
    rules = []
    for i in range(rng.randrange(1, 5)):
        guard = rng.choice(atoms) + (" & " + rng.choice(atoms) if rng.random() < .3 else "")
        outcome = (f"accept {rng.choice(['aa', 'bb'])}{rng.choice(updates)}" if rng.random() < .7 else
                   f"reject {rng.choice(['denied', 'late'])} unchanged")
        rules.append(f"rule r{i}: {rng.choice(['aa', 'bb'])} + {rng.choice(['go', 'stop'])} [{guard}] -> {outcome};")
    return B.parse("zal 2;\nmachine oracle;\nstates aa, bb;\nevents go, stop;\ncontext p;\ninput n in 0..=1;\n"
                   "var x in 0..=2 initially 0;\ninitial aa;\nstep atomic; frame control_and_variables;\n"
                   "default reject no_match unchanged;\n" + "\n".join(rules) + "\n")


def outcome(model, s, v, e, c):
    result = model.outcome(s, dict(v), e, dict(c))
    if result[0] == "overlap":
        return None
    if result[0] == "trap":
        return {"class": "Trap", "rule": result[1].id, "variable": result[2], "value": result[3]}
    step = result[1]
    return {"class": "Accept" if step.accepted else "Reject", "state": step.post_state,
            "variables": dict(step.post_values), "reason": step.reason, "rule": step.rule}


def wanted(found, target):
    return found is not None and all(
        all(found.get("variables", {}).get(k) == x for k, x in value.items()) if key == "variables" else found.get(key) == value
        for key, value in target.items())


class WhyNot2Tests(unittest.TestCase):
    def test_why_not_goldens(self):
        lane = B.parse(LANE)
        result = E.why_not(lane, "idle", LANE_START, "request", {"alarm": False, "amount": 1}, {"class": "Accept"}, "input", "test")
        self.assertEqual(result["summary"], [
            "From idle with balance=0, must_serve=0, pause=0, reserved=0, event request with alarm=no, amount=1: rule "
            "request_too_large rejects because insufficient_balance and keeps the state and variables unchanged.",
            "The fewest input changes that give Accept: 1 change; 6 such inputs in scope.",
            "Changing event from request to deposit: rule deposit_idle accepts and moves to idle with balance=1, must_serve=0, "
            "pause=0, reserved=0.",
            "Changing event from request to tick: rule idle_quiet accepts and moves to idle with balance=0, must_serve=0, pause=0, "
            "reserved=0.",
            *(f"Changing balance from 0 to {n}: rule request_withdrawal accepts and moves to waiting with balance={n}, "
              "must_serve=0, pause=0, reserved=1." for n in range(1, 5)),
            "Rule request_withdrawal would give it here, but its guard (amount is at most balance) is false for this input."])
        self.assertEqual(result["schema"], "zal/why-not/2")
        self.assertIn({"input": "variable:balance", "from": 0, "to": 1},
                      [c for item in result["minimal_changes"] for c in item["changes"]])
        trap = E.why_not(B.parse(COUNTER), "on", {"count": 0}, "bump", {}, {"class": "Trap"}, "input")
        self.assertEqual(trap["summary"][1:], ["The fewest input changes that give Trap: 1 change; 1 such input in scope.",
                                               "Changing count from 0 to 2: rule bump_count would set count to 3, outside 0..=2, "
                                               "so the step is refused."])
        self.assertFalse(trap["minimal_changes"][0]["source_reachable"] is None)
        none = E.why_not(B.parse(COUNTER), "on", {"count": 0}, "bump", {}, {"class": "Trap"})
        self.assertEqual(none["summary"][1:], [
            "No input in the searched scope (state on, variables and event bump fixed, every valuation of no context) gives Trap.",
            "1 other declared input outside this scope gives it; search scope input to list them."])

    def test_why_not_minimal_against_brute_force(self):
        rng, compared = random.Random(20261019), 0
        targets = [{"class": "Accept"}, {"class": "Trap"}, {"variables": {"x": 2}}, {"state": "bb"}, {"rule": "default"},
                   {"class": "Reject", "reason": "denied"}]
        for _ in range(20):
            model = random_v2(rng)
            reasons = {r.reason for r in model.rules} | {model.default}
            domain = list(model.inputs_domain())
            for (s, v, e, c), target, scope in itertools.product(domain[::3], targets, ["context", "input"]):
                if outcome(model, s, v, e, c) is None or "reason" in target and target["reason"] not in reasons:
                    continue
                found = []
                for s2, v2, e2, c2 in domain:
                    if scope == "context" and (s2, v2, e2) != (s, v, e):
                        continue
                    if wanted(outcome(model, s2, v2, e2, c2), target):
                        distance = (s2 != s) + (e2 != e) + sum(v2[k] != v[k] for k in v) + sum(c2[k] != c[k] for k in c)
                        found.append((distance, (s2, tuple(v2.items()), e2, tuple(sorted(c2.items())))))
                best = min((d for d, _ in found), default=None)
                expected = sorted(item for d, item in found if d == best)
                result = E.why_not(model, s, v, e, c, target, scope)
                got = sorted((w["input"]["state"], tuple(w["input"]["variables"].items()), w["input"]["event"],
                              tuple(sorted(w["input"]["context"].items()))) for w in result["minimal_changes"])
                self.assertEqual((result["distance"], got), (best, expected), (model.render(), s, v, e, c, target, scope))
                compared += 1
        self.assertGreater(compared, 2000)
        print(f"WHY-NOT 2: {compared} searches over 20 seeded profile 2 models match the brute-force minimal sets, traps included")

    def test_refusals(self):
        lane = B.parse(LANE)
        for want in [{}, {"class": "Maybe"}, {"variables": {"balance": 9}}, {"variables": {"nope": 1}}, {"variable": "nope"}]:
            with self.assertRaises(Refusal, msg=want):
                E.why_not(lane, "idle", LANE_START, "request", {"alarm": False, "amount": 1}, want)
        with self.assertRaises(Refusal):
            E.why_not(lane, "idle", {**LANE_START, "balance": 9}, "request", {"alarm": False, "amount": 1}, {"class": "Accept"})


class WhatIf2Tests(unittest.TestCase):
    def test_what_if_goldens(self):
        lane = B.parse(LANE)
        result = E.what_if(lane, "idle", LANE_START, "request", {"alarm": False, "amount": 1}, {"variables": {"balance": 2}}, "test")
        self.assertEqual(result["summary"], [
            "Changes: balance from 0 to 2.",
            "Before: rule request_too_large rejects because insufficient_balance and keeps the state and variables unchanged. "
            "After: rule request_withdrawal accepts and moves to waiting with balance=2, must_serve=0, pause=0, reserved=1.",
            "Rule request_too_large becomes disabled.", "Rule request_withdrawal becomes enabled."])
        trap = E.what_if(B.parse(COUNTER), "on", {"count": 1}, "bump", {}, {"variables": {"count": 2}})
        self.assertEqual(trap["after"]["outcome"], {"class": "Trap", "rule": "bump_count", "variable": "count", "value": 3, "range": [0, 2]})
        self.assertEqual(trap["summary"][1], "Before: rule bump_count accepts and moves to on with count=2. "
                                             "After: rule bump_count would set count to 3, outside 0..=2, so the step is refused.")
        for change in [{}, {"variables": {}}, {"variables": {"nope": 1}}, {"context": {"nope": True}}, {"variables": {"count": 7}}]:
            with self.assertRaises(Refusal, msg=change):
                E.what_if(B.parse(COUNTER), "on", {"count": 1}, "bump", {}, change)

    def test_what_if_applies_every_named_change(self):
        rng, compared = random.Random(11), 0
        for _ in range(20):
            model = random_v2(rng)
            for s, v, e, c in list(model.inputs_domain())[::2]:
                change = {}
                if rng.random() < .3:
                    change["state"] = rng.choice(model.states)
                if rng.random() < .3:
                    change["event"] = rng.choice(model.events)
                if rng.random() < .6:
                    change["variables"] = {"x": rng.randrange(3)}
                context = {name: (rng.random() < .5 if name == "p" else rng.randrange(2)) for name in c if rng.random() < .6}
                if context or not change:
                    change["context"] = context or {"n": 1 - c["n"]}
                after = (change.get("state", s), {**v, **change.get("variables", {})}, change.get("event", e),
                         {**c, **change.get("context", {})})
                if outcome(model, s, v, e, c) is None or outcome(model, *after) is None:
                    continue
                result = E.what_if(model, s, v, e, c, change)
                self.assertEqual(result["after"]["input"], {"state": after[0], "variables": after[1], "event": after[2], "context": after[3]})
                got = result["after"]["outcome"]
                self.assertEqual({k: got[k] for k in got if k != "range"}, outcome(model, *after))
                expected = [*(["state"] if "state" in change else []), *(["variable:x"] if "variables" in change else []),
                            *(["event"] if "event" in change else []),
                            *(("context:p" if name == "p" else "input:n") for name in ["p", "n"] if name in change.get("context", {}))]
                self.assertEqual([item["input"] for item in result["changes"]], expected)
                compared += 1
        self.assertGreater(compared, 200)
        print(f"WHAT-IF 2: {compared} random change sets over variables, facts and integer inputs applied and re-decided")


class Narrative2Tests(unittest.TestCase):
    def narrate(self, model, report):
        try:
            return explain.narrate_check(model, report, "test")
        except Refusal as error:
            self.fail(str(error))

    def test_narrative_goldens(self):
        stories = {label: self.narrate(m, B.check(m)) for label, m in ((k, B.parse(v)) for k, v in COUNTEREXAMPLES.items())}
        self.assertEqual({k: s.get("obligation") for k, s in stories.items()}, {
            "range": "range", "range-below": "range", "requirement": "requirement", "requirement-lane": "requirement",
            "liveness": "property", "liveness-finite": "property", "safety": "property", "bounded": "property",
            "next": "property", "determinism": "determinism", "invariant": "invariant",
            "invariant-preservation": "invariant-preservation"})
        self.assertEqual(stories["range"]["english"][-2:], [
            "In on with count=2, event bump arrives with no context facts.",
            "Rule bump_count would set count to 3, outside 0..=2, so the step is refused; a step never wraps around."])
        self.assertEqual(stories["requirement"]["english"][-2:], [
            "Requirement approve_only_if_authorized requires (if (accepted and event is approve and state after this event is approved), "
            "then authorized), which is false at step 2.",
            "Its condition (accepted and event is approve and state after this event is approved) holds at step 2, and its conclusion "
            "(authorized) does not."])
        self.assertEqual(stories["liveness"]["english"][3:], [
            "From here the next 3 steps repeat forever.",
            "Cycle step 1: in waiting with balance=1, must_serve=0, pause=0, reserved=1, event tick arrives with alarm=yes, amount=1; "
            "rule wait_alarm accepts and moves to waiting with balance=1, must_serve=0, pause=2, reserved=1.",
            "Cycle step 2: in waiting with balance=1, must_serve=0, pause=2, reserved=1, event tick arrives with alarm=no, amount=1; "
            "rule wait_count accepts and moves to waiting with balance=1, must_serve=0, pause=1, reserved=1.",
            "Cycle step 3: in waiting with balance=1, must_serve=0, pause=1, reserved=1, event tick arrives with alarm=no, amount=1; "
            "rule wait_last accepts and moves to waiting with balance=1, must_serve=0, pause=0, reserved=1.",
            "Fairness keeper_ticks (event is tick) holds at cycle step 1, so this execution satisfies it.",
            "Step 2 triggers property never_freezes (always (if (accepted and event is request), then eventually (state is waiting "
            "and state after this event is idle))): accepted and event is request holds there.",
            "state is waiting and state after this event is idle is false at every step from step 2 on, including every step of "
            "the repeating cycle, so it never happens."])
        self.assertEqual(stories["liveness"]["symbolic"][4], "waiting {balance=1, must_serve=0, pause=0, reserved=1} + tick "
                         "(alarm=true, amount=1) -> accept waiting {balance=1, must_serve=0, pause=2, reserved=1} [wait_alarm]")
        self.assertTrue(stories["liveness-finite"]["english"][-1].startswith("At step 2, neither state after this event is paid nor "
                                                                             "state after this event is held holds"))
        self.assertEqual(stories["bounded"]["english"][-1], "state is waiting and state after this event is idle is false at steps "
                         "1 through 3: the triggering step and the 2 events after it.")
        self.assertEqual(stories["next"]["english"][-1], "The next step, step 4, exists and state after this event is idle is false there.")
        self.assertEqual(stories["safety"]["english"][-1], "Property no_payout_while_paused requires never (pause is greater than 0 "
                         "and state is waiting and state after this event is idle); at step 2, pause is greater than 0 and state is "
                         "waiting and state after this event is idle holds.")
        self.assertIn("From here the next 1 step repeats forever.", stories["liveness-finite"]["english"])
        self.assertEqual(stories["invariant-preservation"]["english"], [
            "Take spare with count=0, an admissible configuration whether or not it is reachable, and event bump with no context facts.",
            "Rule spare_go accepts and moves to off with count=1.",
            "Invariant reset_clears requires (if state is off, then count equals 0), which is false in off with count=1."])
        lane = B.parse(LANE)
        trace = E.narrate_trace(lane, [{"event": "deposit", "context": {"alarm": False, "amount": 2}}])
        self.assertEqual(trace["english"][-1], "After 1 step the machine is in idle with balance=2, must_serve=0, pause=0, reserved=0.")
        self.assertEqual(explain.narrate_trace(lane, [])["schema"], "zal/narrative/2")
        self.assertEqual(self.narrate(lane, B.check(lane))["status"], "no-counterexample")

    def test_game_failure_and_unknown_remain_explicit_in_narratives(self):
        source = (HERE / "examples/withdrawal_game2.zal").read_text().replace(
            "fairness reopens: infinitely often (state' == open);\n", "")
        model = B.parse(source)
        failed = B.check(model)
        story = self.narrate(model, failed)
        self.assertEqual((story["status"], story["check_status"], story["obligation"], story["property"]),
                         ("counterexample", "counterexample", "game", "keep_withdrawing"))
        self.assertEqual(story["initial"], failed["initial"])
        self.assertEqual(story["sentences"], [])
        self.assertIn("no finite trace or strategy is claimed", story["method"])

        with patch.object(B, "_parity_winners", return_value=frozenset()):
            unknown = B.check(B.parse((HERE / "examples/withdrawal_game2.zal").read_text()))
        story = self.narrate(B.parse((HERE / "examples/withdrawal_game2.zal").read_text()), unknown)
        self.assertEqual((story["status"], story["check_status"], story["diagnostic"]),
                         ("unknown", "unknown", "solvers disagree"))

    def test_narrative_sentences_are_backed_by_the_witness(self):
        """Independent oracle: re-derive every step, the lasso and each claim with the separate temporal oracle."""
        told = 0
        for label, source in COUNTEREXAMPLES.items():
            model = B.parse(source)
            report = B.check(model)
            sentences = self.narrate(model, report)["sentences"]
            self.assertEqual(E.verify(model, report, sentences), [], label)
            for item in sentences:
                for key, path in item["backing"].items():
                    self.assertEqual(E.lookup(report, path), item["facts"][key], (label, item["template"]))
            steps = [s["facts"] for s in sentences if s["template"] == "step"]
            prefix = [s for s in steps if s["phase"] == "prefix"]
            cycle = [s for s in steps if s["phase"] == "cycle"]
            if any(s["template"] == "start" for s in sentences):
                state, values = model.initial, model.initial_values()
            else:
                first = next(s["facts"] for s in sentences if s["template"] in {"input", "raw_input"})
                state, values = first["state"], first["variables"]
            for step in steps:
                self.assertEqual((step["state"], step["variables"]), (state, values), label)
                self.assertEqual(model.decide(step["state"], step["variables"], step["event"], step["context"]), step["outcome"])
                state, values = step["outcome"]["state"], step["outcome"]["variables"]
            witness = report["witness"]
            if report["obligation"] == "range":
                low, high = witness["range"]
                trap = next(s["facts"] for s in sentences if s["template"] == "trap")
                self.assertFalse(low <= trap["value"] <= high, label)
                self.assertEqual(trap["value"], witness["value"])
                with self.assertRaisesRegex(Refusal, "never wraps around"):
                    model.decide(witness["state"], witness["variables"], witness["event"], witness["context"])
            if report["obligation"] == "requirement":
                formula = dict(model.requirements)[report["requirement"]].data()
                self.assertFalse(O.on_word(formula, [{key: steps[-1][key] for key in E.ROW_KEYS}], 0), label)
            if report["obligation"] == "property":
                formula = dict(model.properties)[report["property"]].data()
                rows = [{key: s[key] for key in E.ROW_KEYS} for s in prefix + cycle]
                if cycle:
                    self.assertEqual(len(cycle), len(witness["cycle"]), label)
                    self.assertEqual((state, values), (cycle[0]["state"], cycle[0]["variables"]), label)
                    self.assertFalse(O.on_lasso(formula, rows, len(prefix), 0), label)
                    for item in [s["facts"] for s in sentences if s["template"] == "fair"]:
                        self.assertTrue(O.on_word(dict(model.fairness)[item["fairness"]].data(), [rows[len(prefix) + item["position"] - 1]], 0))
                else:
                    self.assertEqual(len(prefix), len(witness["trace"]), label)
            told += 1
        self.assertEqual(told, len(COUNTEREXAMPLES))
        print(f"NARRATIVES 2: {told} counterexamples, ranges, requirements, safety, next, within-k and fair lassos re-derived")


class CommandLineTests(unittest.TestCase):
    def run_cli(self, *arguments):
        import json, subprocess, tempfile
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "model.zal"
            source.write_text(arguments[0])
            done = subprocess.run([sys.executable, "-B", str(HERE / "run.py"), arguments[1], str(source), *arguments[2:]],
                                  capture_output=True, text=True, timeout=120)
        return done.returncode, json.loads(done.stdout or done.stderr)

    def test_run_commands_explain_both_profiles(self):
        code, story = self.run_cli(COUNTEREXAMPLES["range"], "story")
        self.assertEqual((code, story["schema"], story["obligation"]), (0, "zal/narrative/2", "range"))
        code, why = self.run_cli(LANE, "why-not", "--input", '{"state": "idle", "variables": {"balance": 0, "must_serve": 0, '
                                 '"pause": 0, "reserved": 0}, "event": "request", "context": {"alarm": false, "amount": 1}}',
                                 "--want", '{"class": "Accept"}', "--scope", "input")
        self.assertEqual((code, why["schema"], why["distance"]), (0, "zal/why-not/2", 1))
        code, what = self.run_cli(COUNTER, "what-if", "--input", '{"state": "on", "variables": {"count": 2}, "event": "bump", '
                                  '"context": {}}', "--change", '{"variables": {"count": 0}}')
        self.assertEqual((code, what["before"]["outcome"]["class"], what["after"]["outcome"]["class"]), (0, "Trap", "Accept"))
        v1 = (HERE / "examples/order.zal").read_text()
        code, why = self.run_cli(v1, "why-not", "--input", '{"state": "pending", "event": "approve", "context": {"authorized": false}}',
                                 "--want", '{"class": "Accept"}')
        self.assertEqual((code, why["schema"], why["minimal_changes"][0]["rule"]), (0, "zal/why-not/1", "approve_order"))
        code, refused = self.run_cli(v1, "what-if", "--input", '{"state": "pending", "event": "approve"}', "--change", "{}")
        self.assertEqual((code, refused["status"]), (2, "refused"))

    def test_story_preserves_game_failure_and_unknown_exit_status(self):
        game = (HERE / "examples/withdrawal_game2.zal").read_text().replace(
            "fairness reopens: infinitely often (state' == open);\n", "")
        code, story = self.run_cli(game, "story")
        self.assertEqual((code, story["status"], story["obligation"]), (1, "counterexample", "game"))

        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "model.zal"
            source.write_text((HERE / "examples/withdrawal_game2.zal").read_text())
            output = StringIO()
            with patch.object(B, "_parity_winners", return_value=frozenset()), \
                    patch.object(sys, "argv", ["run.py", "story", str(source)]), redirect_stdout(output):
                code = CLI.main()
        story = json.loads(output.getvalue())
        self.assertEqual((code, story["status"], story["check_status"], story["diagnostic"]),
                         (1, "unknown", "unknown", "solvers disagree"))


if __name__ == "__main__":
    unittest.main()
