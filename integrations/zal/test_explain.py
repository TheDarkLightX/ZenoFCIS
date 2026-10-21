"""Why-not, what-if, narratives and the registry guide: goldens, oracles and drift checks."""

import itertools
import json
from pathlib import Path
import random
import re
import unittest

from behavior import FUTURE, Formula, Refusal, check, grammar_patterns, help_topic, legend, parse, parse_formula
import constructs
import explain as E
import ladder as l


ORDER = (Path(__file__).parent / "examples/order.zal").read_text()
RAW = """zal 1;
machine raw;
states a, b, c;
events go;
context none;
initial a;
step atomic; frame control_only;
default reject no_rule unchanged;
rule from_b: b + go [true] -> accept c;
invariant never_c: !(state == c);
"""
DEFAULT = ("From {0}, event {1} with authorized=no: no rule is enabled, so the default rejects because not_enabled "
           "and keeps the state unchanged.")
VALUES = list(itertools.product((False, True), repeat=2))


def dnf(mask, names=("p", "q")):
    terms = ["(" + " & ".join(n if v else "!" + n for n, v in zip(names, values)) + ")"
             for i, values in enumerate(VALUES) if mask & (1 << i)]
    return " | ".join(terms) or "false"


def small(masks):
    """Two states, two events and facts p, q; each rule's guard is a DNF bit mask."""
    rules = [f"rule r{i}: {src} + {ev} [{dnf(mask)}] -> {out};"
             for i, (src, ev, mask, out) in enumerate(masks)]
    return parse("zal 1;\nmachine oracle;\nstates aa, bb;\nevents go, stop;\ncontext p, q;\ninitial aa;\n"
                 "step atomic; frame control_only;\ndefault reject no_match unchanged;\n" + "\n".join(rules) + "\n")


def random_models(count, seed=20261019):
    rng = random.Random(seed)
    outcomes = ["accept aa", "accept bb", "reject denied unchanged", "reject late unchanged"]
    for _ in range(count):
        masks = [(rng.choice(["aa", "bb"]), rng.choice(["go", "stop"]), rng.randrange(16), rng.choice(outcomes))
                 for _ in range(rng.randrange(1, 5))]
        yield small(masks)


def oracle_minimal(model, state, event, context, target, scope):
    """Independent brute force: every in-scope input with the wanted outcome, at the fewest changes."""
    found = []
    for s, e, c in model.inputs():
        if scope == "context" and (s, e) != (state, event) or len(model.enabled(s, e, c)) > 1:
            continue
        outcome = model.decide(s, e, c)
        if all(outcome[k] == v for k, v in target.items()):
            distance = (s != state) + (e != event) + sum(c[f] != context[f] for f in model.facts)
            found.append((distance, (s, e, tuple(sorted(c.items())))))
    best = min((d for d, _ in found), default=None)
    return best, sorted(item for d, item in found if d == best)


class WhyNotTests(unittest.TestCase):
    def setUp(self):
        self.model = parse(ORDER)

    def test_why_not_goldens_on_the_order_example(self):
        cases = [
            (("pending", "approve", {"authorized": False}, {"class": "Accept"}, "context"),
             ("found", 1, [DEFAULT.format("pending", "approve"), "The fewest input changes that give Accept: 1 change; 1 such input in scope.",
                           "Changing authorized from no to yes: rule approve_order accepts and moves to approved.",
                           "Rule approve_order would give it here, but its guard (authorized) is false for this input."])),
            (("draft", "approve", {"authorized": False}, {"class": "Accept", "state": "approved"}, "context"),
             ("none-in-scope", None, [DEFAULT.format("draft", "approve"),
                                      "No input in the searched scope (state draft and event approve fixed, every valuation of authorized) gives Accept to approved.",
                                      "1 other declared input outside this scope gives it; search scope input to list them."])),
            (("draft", "approve", {"authorized": False}, {"class": "Accept", "state": "approved"}, "input"),
             ("found", 2, [DEFAULT.format("draft", "approve"),
                           "The fewest input changes that give Accept to approved: 2 changes; 1 such input in scope.",
                           "Changing state from draft to pending and authorized from no to yes: rule approve_order accepts and moves to approved."])),
            (("pending", "approve", {"authorized": True}, {"class": "Accept"}, "context"),
             ("already", 0, ["From pending, event approve with authorized=yes: rule approve_order accepts and moves to approved.",
                             "The current input already gives Accept."])),
            (("approved", "cancel", {"authorized": False}, {"class": "Accept", "rule": "default"}, "context"),
             ("none-in-scope", None, [DEFAULT.format("approved", "cancel"),
                                      "No input in the searched scope (state approved and event cancel fixed, every valuation of authorized) gives Accept by the default.",
                                      "No input in the whole declared domain gives it either."])),
        ]
        for arguments, (status, distance, summary) in cases:
            with self.subTest(arguments=arguments):
                result = E.why_not(self.model, *arguments, checker_identity="test")
                self.assertEqual((result["status"], result["distance"], result["summary"]), (status, distance, summary))
                self.assertEqual((result["schema"], result["authority"], result["checker"]), ("zal/why-not/1", "none", "test"))
                self.assertEqual(result["revision"], self.model.revision)
        found = E.why_not(self.model, "pending", "approve", {"authorized": False}, {"class": "Accept"})
        self.assertEqual(found["minimal_changes"][0]["changes"], [{"input": "context:authorized", "from": False, "to": True}])
        self.assertEqual(found["minimal_changes"][0]["statement"]["symbolic"],
                         "rule approve_order: pending + approve [authorized] -> accept approved;")
        self.assertTrue(found["minimal_changes"][0]["source_reachable"])

    def test_why_not_minimal_against_brute_force(self):
        targets = [{"class": "Accept"}, {"class": "Reject"}, {"state": "aa"}, {"class": "Accept", "state": "bb"},
                   {"rule": "default"}, {"reason": "denied"}]
        compared = 0
        for model in random_models(60):
            reasons = {rule.reason for rule in model.rules} | {model.default}
            for (state, event, context), target, scope in itertools.product(model.inputs(), targets, ["context", "input"]):
                if "reason" in target and target["reason"] not in reasons or len(model.enabled(state, event, context)) > 1:
                    continue
                result = E.why_not(model, state, event, context, target, scope)
                best, expected = oracle_minimal(model, state, event, context, target, scope)
                got = sorted((w["input"]["state"], w["input"]["event"], tuple(sorted(w["input"]["context"].items())))
                             for w in result["minimal_changes"])
                self.assertEqual((result["distance"], got), (best, expected), (model.render(), state, event, context, target, scope))
                self.assertEqual(result["minimal_count"], len(expected))
                for item in result["minimal_changes"]:
                    self.assertEqual(len(item["changes"]), best)
                    self.assertTrue(all(item["outcome"][k] == v for k, v in target.items()))
                    self.assertEqual(item["outcome"], model.decide(item["input"]["state"], item["input"]["event"], item["input"]["context"]))
                self.assertEqual(result["status"], "none-in-scope" if best is None else "already" if best == 0 else "found")
                compared += 1
        self.assertGreater(compared, 10000)
        print(f"WHY-NOT: {compared} searches over 60 seeded models match the brute-force minimal sets")

    def test_why_not_refusals_and_overlap_scope(self):
        for want in [{}, {"class": "Maybe"}, {"state": "nowhere"}, {"rule": "nope"}, {"reason": "unknown"}, {"colour": "red"}]:
            with self.assertRaises(Refusal):
                E.why_not(self.model, "pending", "approve", {"authorized": False}, want)
        with self.assertRaises(Refusal):
            E.why_not(self.model, "pending", "approve", {}, {"class": "Accept"})
        with self.assertRaises(Refusal):
            E.why_not(self.model, "pending", "approve", {"authorized": False}, {"class": "Accept"}, "everything")
        overlap = parse(ORDER + "rule another: pending + approve [authorized] -> accept cancelled;\n")
        result = E.why_not(overlap, "pending", "approve", {"authorized": False}, {"class": "Accept"})
        self.assertEqual((result["status"], result["scope"]["overlapping_inputs_skipped"]), ("none-in-scope", 1))


class WhatIfTests(unittest.TestCase):
    def setUp(self):
        self.model = parse(ORDER)

    def test_what_if_golden(self):
        result = E.what_if(self.model, "pending", "approve", {"authorized": False}, {"context": {"authorized": True}}, "test")
        self.assertEqual(result["summary"], [
            "Changes: authorized from no to yes.",
            "Before: no rule is enabled, so the default rejects because not_enabled and keeps the state unchanged. "
            "After: rule approve_order accepts and moves to approved.",
            "Rule approve_order becomes enabled."])
        self.assertEqual(result["outcome_changes"], ["class", "state", "reason", "rule"])
        approve = next(rule for rule in result["rules"] if rule["id"] == "approve_order")
        self.assertEqual((approve["changed"], approve["changed_facts_read"]), (["enabled", "guard_value"], ["authorized"]))
        result = E.what_if(self.model, "pending", "approve", {"authorized": False}, {"event": "cancel", "context": {"authorized": False}})
        self.assertEqual(result["summary"][:2], ["Changes: event from approve to cancel; authorized from no to no.",
                                                 "authorized is already no; that change has no effect."])
        self.assertEqual([c["effective"] for c in result["changes"]], [True, False])

    def test_what_if_applies_and_reports_every_named_input(self):
        rng = random.Random(7)
        compared = 0
        for model in random_models(40, seed=11):
            for state, event, context in model.inputs():
                for _ in range(3):
                    change = {}
                    if rng.random() < .3:
                        change["state"] = rng.choice(model.states)
                    if rng.random() < .3:
                        change["event"] = rng.choice(model.events)
                    facts = {name: rng.random() < .5 for name in model.facts if rng.random() < .7}
                    if facts or not change:
                        change["context"] = facts or {"p": not context["p"]}
                    after = (change.get("state", state), change.get("event", event), {**context, **change.get("context", {})})
                    if len(model.enabled(state, event, context)) > 1 or len(model.enabled(*after)) > 1:
                        continue
                    result = E.what_if(model, state, event, context, change)
                    self.assertEqual(result["after"]["input"], {"state": after[0], "event": after[1], "context": after[2]})
                    self.assertEqual(result["after"]["outcome"], model.decide(*after))
                    self.assertEqual(result["before"]["outcome"], model.decide(state, event, context))
                    expected = [*(["state"] if "state" in change else []), *(["event"] if "event" in change else []),
                                *("context:" + name for name in model.facts if name in change.get("context", {}))]
                    self.assertEqual([c["input"] for c in result["changes"]], expected)
                    for item in result["changes"]:
                        name = item["input"].removeprefix("context:")
                        self.assertEqual(item["to"], change[name] if name in {"state", "event"} else change["context"][name])
                    for rule, row in zip(model.rules, result["rules"]):
                        self.assertEqual(row["after"]["enabled"], rule in model.enabled(*after))
                        self.assertEqual(row["after"]["guard_value"], rule.guard.value(after[0], after[2]))
                    compared += 1
        self.assertGreater(compared, 300)
        print(f"WHAT-IF: {compared} random change sets applied, reported and re-decided independently")

    def test_what_if_refuses_undeclared_or_empty_changes(self):
        for change in [{}, {"context": {}}, {"context": {"nope": True}}, {"colour": "red"}, {"state": "nowhere"}]:
            with self.assertRaises(Refusal, msg=change):
                E.what_if(self.model, "pending", "approve", {"authorized": False}, change)


class NarrativeTests(unittest.TestCase):
    def narrate(self, model):
        try:
            return E.narrate_check(model, check(model), "test")
        except Refusal as error:
            self.fail(str(error))

    def test_narrative_goldens(self):
        model = parse(ORDER + "invariant never_cancelled: !(state == cancelled);\n")
        result = self.narrate(model)
        self.assertEqual(result["english"], [
            "The machine starts in draft.",
            "Step 1: in draft, event submit arrives with authorized=no; rule submit_order accepts and moves to pending.",
            "Step 2: in pending, event cancel arrives with authorized=no; rule cancel_order accepts and moves to cancelled.",
            "After 2 steps the state is cancelled.",
            "Invariant never_cancelled requires (not state is cancelled), which is false when the state is cancelled."])
        self.assertEqual(result["symbolic"], [
            "initial draft;", "draft + submit (authorized=false) -> accept pending [submit_order]",
            "pending + cancel (authorized=false) -> accept cancelled [cancel_order]", "state == cancelled",
            "invariant never_cancelled: (!state == cancelled); state == cancelled gives false"])
        overlap = parse(ORDER + "rule another: pending + approve [true] -> accept cancelled;\n")
        self.assertEqual(self.narrate(overlap)["english"], [
            "In pending, event approve arrives with authorized=yes.",
            "Rule another is enabled here (guard: true), and it would accept and move to cancelled.",
            "Rule approve_order is enabled here (guard: authorized), and it would accept and move to approved.",
            "2 rules are enabled together, so the next state is not determined; this profile gives no precedence."])
        raw = parse(RAW)
        self.assertEqual(self.narrate(raw)["english"], [
            "Take state b, whether or not it is reachable, and event go with no context facts.",
            "Rule from_b accepts and moves to c.",
            "Invariant never_c requires (not state is c), which is false when the state is c."])
        start = parse(ORDER + "invariant not_draft: !(state == draft);\n")
        self.assertEqual(self.narrate(start)["english"], [
            "The machine starts in draft.", "After 0 steps the state is draft.",
            "Invariant not_draft requires (not state is draft), which is false when the state is draft."])
        trace = E.narrate_trace(parse(ORDER), [{"event": "submit", "context": {"authorized": False}},
                                               {"event": "approve", "context": {"authorized": True}}])
        self.assertEqual(trace["english"][-1], "After 2 steps the state is approved.")
        self.assertEqual(self.narrate(parse(ORDER))["status"], "no-counterexample")

    def test_narrative_sentences_are_backed_by_the_witness(self):
        """Independent oracle: replay each witness and re-check every narrated fact."""
        seeds = [ORDER + "invariant never_cancelled: !(state == cancelled);\n",
                 ORDER + "invariant never_approved: !(state == approved);\n",
                 ORDER + "invariant not_draft: !(state == draft);\n",
                 ORDER + "rule another: pending + approve [true] -> accept cancelled;\n", RAW,
                 RAW.replace("rule from_b: b + go [true] -> accept c;", "rule from_a: a + go [true] -> accept b;\nrule from_b: b + go [true] -> accept c;")]
        models = [parse(source) for source in seeds] + list(random_models(80, seed=3))
        told = 0
        for model in models:
            report = check(model)
            result = self.narrate(model)
            if report["status"] != "counterexample":
                self.assertEqual(result["sentences"], [])
                continue
            told += 1
            sentences = result["sentences"]
            self.assertEqual(E.verify(model, report, sentences), [])
            for item in sentences:
                for key, path in item["backing"].items():
                    self.assertEqual(E.lookup(report, path), item["facts"][key])
            witness = report["witness"]
            if report["obligation"] == "invariant":
                state = model.initial
                steps = [s["facts"] for s in sentences if s["template"] == "step"]
                self.assertEqual(len(steps), len(witness["trace"]))
                for step, row in zip(steps, witness["trace"]):
                    self.assertEqual(step["pre"], state)
                    self.assertEqual(step["outcome"], model.decide(state, row["event"], row["context"]))
                    state = step["outcome"]["state"]
                self.assertEqual(sentences[-2]["facts"]["state"], state)
                self.assertEqual(sentences[-1]["facts"]["state"], state)
                self.assertFalse(dict(model.invariants)[report["invariant"]].value(state, {}))
            elif report["obligation"] == "determinism":
                enabled = [r.id for r in model.enabled(witness["state"], witness["event"], witness["context"])]
                self.assertEqual([s["facts"]["rule"] for s in sentences if s["template"] == "enabled"], enabled)
                self.assertGreater(len(enabled), 1)
            else:
                target = model.decide(witness["state"], witness["event"], witness["context"])["state"]
                self.assertEqual(sentences[-1]["facts"]["state"], target)
                self.assertFalse(dict(model.invariants)[report["invariant"]].value(target, {}))
            self.assertTrue(all(s["english"] and s["symbolic"] for s in sentences))
        trace_inputs = [{"event": "submit", "context": {"authorized": False}}, {"event": "approve", "context": {"authorized": False}},
                        {"event": "cancel", "context": {"authorized": True}}]
        model = parse(ORDER)
        result = E.narrate_trace(model, trace_inputs)
        state = model.initial
        for step, given in zip([s["facts"] for s in result["sentences"] if s["template"] == "step"], trace_inputs):
            self.assertEqual((step["pre"], step["event"], step["context"]), (state, given["event"], given["context"]))
            state = model.decide(state, given["event"], given["context"])["state"]
        self.assertEqual(result["sentences"][-1]["facts"], {"count": 3, "state": state})
        self.assertGreater(told, 20)
        print(f"NARRATIVES: {told} checker counterexamples and one trace; every sentence re-derived from its witness")

    def test_tampered_sentences_are_refused(self):
        model = parse(ORDER + "invariant never_cancelled: !(state == cancelled);\n")
        report = check(model)
        good = E.narrate_check(model, report)["sentences"]
        tampered = [dict(good[0], english="The machine starts somewhere pleasant."),
                    dict(good[1], facts={**good[1]["facts"], "pre": "approved"}),
                    dict(good[-1], facts={**good[-1]["facts"], "state": "draft"}, backing={"invariant": ["invariant"]})]
        for index, sentence in zip([0, 1, len(good) - 1], tampered):
            changed = list(good)
            changed[index] = sentence
            self.assertTrue(E.verify(model, report, changed), sentence)
        with self.assertRaises(Refusal):
            E.narrate_check(model, {**report, "obligation": "liveness"})


class RegistryHelpTests(unittest.TestCase):
    MODELS = [ORDER, ORDER + "invariant never_cancelled: !(state == cancelled);\nunresolved effects: notify the owner;\n", RAW]

    def test_every_construct_has_help(self):
        model = parse(self.MODELS[1])
        concepts = {entry["concept"] for entry in legend(model)["entries"]}
        registry = set(constructs.BY_ID)
        self.assertLessEqual(set(grammar_patterns()) | set(grammar_patterns(True)), registry)
        self.assertLessEqual(concepts, registry)
        self.assertLessEqual({"true", "false", "fact", "state", "not", "and", "or"},
                             {c.formula_op for c in constructs.REGISTRY if c.formula_op})
        self.assertLessEqual(set(FUTURE), registry)
        self.assertLessEqual({x["symbol"] for x in legend(model)["operators"]} - {"state == name"}, registry)
        help_topics = set(help_topic(model)["available_topics"]) - {k for k in help_topic(model)["available_topics"] if ":" in k}
        self.assertLessEqual(help_topics - {"initial", "default", "frame"}, registry)
        for construct in constructs.REGISTRY:
            with self.subTest(construct=construct.id):
                self.assertTrue(construct.meaning and construct.title)
                self.assertTrue(all(constructs.forms(construct).values()))
                guide = constructs.guide(model, "grammar:" + construct.id, "test")
                self.assertEqual((guide["construct"], guide["meaning"], guide["schema"]), (construct.id, construct.meaning, "zal/help/2"))
        print(f"HELP DRIFT: {len(constructs.REGISTRY)} registry constructs cover {len(concepts)} live statement kinds, "
              "every Boolean node, every unsupported kind and every zal/help/1 topic")

    def test_every_help_topic_names_a_real_construct(self):
        """Fill each registry form with model names; the live parser must accept it as that construct."""
        model = parse(ORDER)
        slots = {False: {"STATE_FORMULA": "state == draft", "OUTCOME": "accept approved", "GUARD": "authorized",
                         "TARGET": "approved", "REASON": "denied", "STATE": "pending", "EVENT": "approve", "FACT": "authorized",
                         "TEXT": "kept visible", "KIND": "effects", "ID": "probe", "P": "authorized", "Q": "state == draft"},
                 True: {"STATE_FORMULA": "state is draft", "OUTCOME": "move to approved"}}
        def fill(form, english):
            table = {**slots[False], **(slots[True] if english else {})}
            return re.sub("|".join(sorted(table, key=len, reverse=True)), lambda m: table[m.group(0)], form)
        rule = {e: fill(constructs.forms(constructs.BY_ID["rule"])[k], e) for e, k in [(False, "symbolic"), (True, "english")]}
        def extend(lines, english):
            try:
                return parse(model.render(english) + "\n".join(lines) + "\n", "english" if english else "symbolic")
            except Refusal as error:
                self.fail(f"the parser refuses a registry form: {error}")
        def formula(text, english=False):
            try:
                return parse_formula(text, model.facts, model.states, english=english)
            except Refusal as error:
                self.fail(f"the parser refuses a registry form: {error}")
        for construct in constructs.REGISTRY:
            forms = constructs.forms(construct)
            symbolic, english = fill(forms["symbolic"], False), fill(forms["english"], True)
            with self.subTest(construct=construct.id):
                if construct.kind in {"operator", "atom"}:
                    parsed = formula(symbolic)
                    self.assertEqual(formula(english, True), parsed)
                    self.assertIn(construct.formula_op, constructs.formula_ops(parsed))
                elif construct.id in grammar_patterns():
                    for surface in (False, True):
                        rendered = [line for line in model.render(surface).splitlines()
                                    if re.fullmatch(grammar_patterns(surface)[construct.id], line)]
                        self.assertEqual(len(rendered), 1)
                elif construct.kind in {"outcome", "syntax"}:
                    probe = rule[False].replace("accept approved", symbolic) if construct.kind == "outcome" else rule[False]
                    probe_english = rule[True].replace("move to approved", english) if construct.kind == "outcome" else rule[True]
                    self.assertIn(symbolic, probe)
                    self.assertIn(english, probe_english)
                    self.assertEqual(extend([probe], False), extend([probe_english], True))
                else:
                    extended = extend([symbolic], False)
                    self.assertEqual(extend([english], True), extended)
                    self.assertIn(symbolic, extended.render().splitlines())
                    if construct.kind == "unsupported":
                        self.assertIn((construct.id, "kept visible"), extended.obligations)

    def test_guide_resolution_matches_help_v1_and_goldens(self):
        for source in self.MODELS:
            model = parse(source)
            topics = help_topic(model)["available_topics"]
            for topic in [*topics, "", "machine", "not", "is", "grammar:rule", "approve_order"]:
                with self.subTest(topic=topic):
                    try:
                        old = help_topic(model, topic)
                    except Refusal:
                        with self.assertRaises(Refusal):
                            constructs.guide(model, topic)
                        continue
                    new = constructs.guide(model, topic)
                    self.assertEqual((new["topic"], new["revision"]), (old["topic"], old["revision"]))
                    if old["entry_kind"] == "object":
                        self.assertEqual((new["entries"], new["meaning"]), (old["entries"], old["meaning"]))
            for topic in ["grammar:nope", "nope", "state:nowhere"]:
                with self.assertRaises(Refusal):
                    constructs.guide(model, topic)
        model = parse(ORDER)
        guide = constructs.guide(model, "==", "test")
        self.assertEqual((guide["forms"], guide["entries"], guide["used_in"]),
                         ({"symbolic": "state == STATE", "english": "state is STATE"},
                          [{"symbolic": "state == draft", "english": "state is draft"}], []))
        guide = constructs.guide(model, "fact")
        self.assertEqual(guide["used_in"], ["rule:approve_order"])
        guide = constructs.guide(model, "context")
        self.assertEqual(guide["forms"], {"symbolic": "context NAME, NAME;", "english": "The context observations are NAME, NAME."})
        guide = constructs.guide(model, "effects")
        self.assertEqual(guide["forms"]["symbolic"], "unresolved effects: TEXT;")
        self.assertIn("blocks factory lowering", guide["meaning"])
        self.assertEqual(len(constructs.guide(model)["constructs"]), len(constructs.REGISTRY))

    def test_ladder_registry_covers_every_constructor(self):
        constructors = {*l.COMPARE, *l.LOGIC, *l.ARITH, *l.BIND, "not", "group"}
        for theory, info in l.REGISTRY["theories"].items():
            with self.subTest(theory=theory):
                self.assertEqual(set(info["symbols"]) - {"prime"}, constructors)
        for rung, templates in l.REGISTRY["templates"].items():
            with self.subTest(rung=rung):
                self.assertLessEqual(constructors | {"call", "int", "var", "prime_call"}, set(templates))
                extra = set(templates) - constructors - {"call", "int", "var", "prime_call"}
                self.assertLessEqual(extra, {"atom", "function", "prime_state", "state", "zero_atom", "zero_call", "zero_function"})
        self.assertEqual(set(l.REGISTRY["temporal_legend"]), set(l.TEMPORAL))
        for mode, templates in l.REGISTRY["temporal_templates"].items():
            self.assertEqual(set(templates) - {"never"}, set(l.TEMPORAL), mode)
        self.assertEqual(set(l.REGISTRY["inheritance"]), {"intuitive", "plain", "question"})


if __name__ == "__main__":
    unittest.main()
