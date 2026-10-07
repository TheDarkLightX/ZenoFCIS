import itertools
from pathlib import Path
import unittest
from unittest.mock import patch

from behavior import Formula, Model, Rule, Refusal, check, explain, help_topic, parse, parse_formula, replay, semantic_diff
from workflow import Session
from factory import lower


SOURCE = (Path(__file__).parent / "examples/order.zal").read_text()


class BehaviorTests(unittest.TestCase):
    def setUp(self):
        self.model = parse(SOURCE)

    def test_paired_roundtrip_and_reordering(self):
        self.assertEqual(parse(self.model.render()), self.model)
        self.assertEqual(parse(self.model.render(True), "english"), self.model)
        self.assertEqual(parse("\n".join(reversed(SOURCE.splitlines()))).revision, self.model.revision)
        swapped = SOURCE.replace("authorized]", "(authorized & true)]")
        model = parse(swapped)
        self.assertEqual(parse(model.render(True), "english"), model)

    def test_overlap_witness_and_explicit_gap(self):
        overlap = parse(SOURCE + "rule another: pending + approve [true] -> accept cancelled;\n")
        report = check(overlap)
        self.assertEqual(report["obligation"], "determinism")
        self.assertEqual(report["witness"], {"state": "pending", "event": "approve", "context": {"authorized": True}})
        self.assertEqual(report["invariant_count"], 0)
        self.assertEqual(report["advisories"][0]["kind"], "no-application-invariants")
        report = check(self.model)
        self.assertEqual(report["input_tuples"], 24)
        self.assertGreater(report["default_count"], 0)
        self.assertEqual(report["default_witness"]["outcome"]["reason"], "not_enabled")

    def test_false_guards_do_not_allow_arbitrary_successor(self):
        model = parse(SOURCE.replace("[authorized]", "[false]"))
        result = model.decide("pending", "approve", {"authorized": True})
        self.assertEqual(result, {"class": "Reject", "state": "pending", "reason": "not_enabled", "rule": "default"})

    def test_context_shapes_and_one_state_factory_subset_are_explicit(self):
        model = parse("zal 1;\nmachine single;\nstates alone;\nevents wait;\ncontext none;\ninitial alone;\nstep atomic; frame control_only;\ndefault reject not_enabled unchanged;\n")
        self.assertEqual(check(model)["status"], "pass-with-scope")
        for context in [[], (), None, False, ""]:
            with self.subTest(context=context), self.assertRaises(Refusal):
                model.decide("alone", "wait", context)
        with self.assertRaisesRegex(Refusal, "at least two states"):
            lower(model)
        self.assertEqual(model.decide("alone", "wait", {})["class"], "Reject")

    def test_long_lived_checker_refuses_source_drift_before_new_evidence(self):
        session = Session(self.model)
        before = session.view()
        with patch("workflow._disk_checker_identity", return_value="changed-source"):
            with self.assertRaisesRegex(Refusal, "restart"):
                Session(self.model)
            with self.assertRaisesRegex(Refusal, "restart"):
                session.propose_human(SOURCE + "unresolved liveness: pending resolves;\n",
                                      "symbolic", self.model.revision)
            with self.assertRaisesRegex(Refusal, "restart"):
                lower(self.model)
        self.assertEqual(session.view(), before)

    def test_local_explanation_owns_exact_input_and_all_guard_results(self):
        context = {"authorized": False}
        report = explain(self.model, "pending", "approve", context, "test-checker")
        self.assertTrue(report["used_default"])
        self.assertEqual(report["outcome"], self.model.decide("pending", "approve", context))
        rule = next(r for r in report["rules"] if r["id"] == "approve_order")
        self.assertTrue(rule["state_matches"] and rule["event_matches"])
        self.assertFalse(rule["guard"]["value"] or rule["enabled"])
        report["input"]["context"]["authorized"] = True
        self.assertFalse(context["authorized"])
        self.assertEqual(report["frame"], "control unchanged")
        for state, event, given in self.model.inputs():
            result = explain(self.model, state, event, given)
            self.assertEqual(result["outcome"], self.model.decide(state, event, given))
            self.assertEqual(sum(r["enabled"] for r in result["rules"]), 0 if result["used_default"] else 1)
        for bad in [{}, None, [], {"authorized": 0}, {"authorized": False, "extra": False}]:
            with self.subTest(context=bad), self.assertRaises(Refusal):
                explain(self.model, "pending", "approve", bad)

    def test_help_uses_canonical_views_and_resolves_operator_roles_without_guessing(self):
        for topic in ["rule:approve_order", "approve_order", "context:authorized", "initial", "frame", "default"]:
            result = help_topic(self.model, topic)
            self.assertEqual(result["revision"], self.model.revision)
            for entry in result["entries"]:
                self.assertIn(entry["symbolic"], self.model.render().splitlines())
                self.assertIn(entry["english"], self.model.render(True).splitlines())
        for topic in ["!", "not", "&", "and", "|", "or", "==", "is", "true", "false"]:
            result = help_topic(self.model, topic)
            entry = result["entries"][0]
            self.assertEqual(parse_formula(entry["symbolic"], self.model.facts, self.model.states),
                             parse_formula(entry["english"], self.model.facts, self.model.states, english=True))
        self.assertIn("not addition", help_topic(self.model, "+")["meaning"])
        self.assertIn("not logical implication", help_topic(self.model, "->")["meaning"])
        self.assertTrue(any("No application invariants declared" in note for note in help_topic(self.model)["notes"]))
        for invalid in ["unsupported_topic", None, "x" * 129]:
            with self.subTest(topic=invalid), self.assertRaises(Refusal):
                help_topic(self.model, invalid)
        ambiguous = parse("zal 1;\nmachine help_case;\nstates aa, bb;\nevents aa;\ncontext none;\ninitial aa;\nstep atomic; frame control_only;\ndefault reject refused unchanged;\n")
        with self.assertRaisesRegex(Refusal, "Ambiguous"):
            help_topic(ambiguous, "aa")
        self.assertEqual(help_topic(ambiguous, "event:aa")["entry_kind"], "object")
        named_initial = parse("zal 1;\nmachine help_case;\nstates initial, bb;\nevents tick;\ncontext none;\ninitial initial;\nstep atomic; frame control_only;\ndefault reject refused unchanged;\n")
        for object_id in Session(named_initial).objects():
            self.assertEqual(help_topic(named_initial, object_id)["entry_kind"], "object")
        self.assertEqual(help_topic(named_initial, "grammar:initial")["entry_kind"], "grammar")
        self.assertEqual(help_topic(named_initial, "grammar:frame")["entry_kind"], "grammar")
        named_accept = parse("zal 1;\nmachine help_case;\nstates accept, bb;\nevents tick;\ncontext none;\ninitial accept;\nstep atomic; frame control_only;\ndefault reject refused unchanged;\n")
        with self.assertRaisesRegex(Refusal, "Ambiguous"):
            help_topic(named_accept, "accept")
        self.assertEqual(help_topic(named_accept, "grammar:accept")["entry_kind"], "grammar")
        self.assertEqual(help_topic(named_accept, "state:accept")["entry_kind"], "object")
        with self.assertRaises(Refusal):
            help_topic(named_accept, "grammar:state:accept")

    def test_advisories_separate_guard_source_reachability_and_domain_tautology(self):
        source = "zal 1;\nmachine diagnostics;\nstates aa, bb;\nevents tick;\ncontext p;\ninitial aa;\nstep atomic; frame control_only;\ndefault reject refused unchanged;\n"
        model = parse(source + "rule contradiction: aa + tick [(p & !p)] -> accept bb;\n"
                      "rule source_inconsistent: aa + tick [state == bb] -> accept bb;\n"
                      "rule unreachable: bb + tick [p] -> accept bb;\n"
                      "invariant domain: (state == aa | state == bb);\n")
        report = check(model)
        self.assertEqual(report["status"], "pass-with-scope")
        pairs = {(a["kind"], a.get("rule", a.get("invariant"))) for a in report["advisories"]}
        self.assertIn(("guard-never-true-at-source", "contradiction"), pairs)
        self.assertIn(("guard-never-true-at-source", "source_inconsistent"), pairs)
        self.assertIn(("unreachable-source", "unreachable"), pairs)
        self.assertIn(("invariant-domain-tautology", "domain"), pairs)
        not_tautology = check(parse(source + "invariant reachable_only: state == aa;\n"))
        self.assertEqual(not_tautology["status"], "pass-with-scope")
        self.assertFalse(any(a["kind"] == "invariant-domain-tautology" for a in not_tautology["advisories"]))

    def test_consistency_does_not_establish_an_unstated_authorization_policy(self):
        weakened = parse(SOURCE.replace("[authorized]", "[true]"))
        self.assertEqual(check(weakened)["status"], "pass-with-scope")
        self.assertEqual(check(weakened)["invariant_count"], 0)
        witnesses = semantic_diff(self.model, weakened)["witnesses"]
        self.assertTrue(any(w["state"] == "pending" and w["event"] == "approve" and
            w["context"] == {"authorized": False} and w["before"]["class"] == "Reject" and
            w["after"]["class"] == "Accept" for w in witnesses))

    def test_empty_init_invariant_and_unknowns(self):
        for bad in [SOURCE.replace("initial draft;", "initial ;"), SOURCE.replace("initial draft;", ""),
                    SOURCE.replace("default reject not_enabled unchanged;", ""),
                    SOURCE.replace("[authorized]", "[someone]"), SOURCE + "Please be sensible.\n"]:
            with self.assertRaises(Refusal):
                parse(bad)
        model = parse(SOURCE + "invariant never_cancelled: !(state == cancelled);\n")
        report = check(model)
        self.assertEqual(report["status"], "counterexample")
        self.assertEqual(report["witness"]["state"], "cancelled")
        self.assertEqual(len(report["witness"]["trace"]), 2)
        with self.assertRaises(Refusal):
            parse(SOURCE + "invariant assumed: authorized;\n")
        future = parse(SOURCE + "unresolved fairness: pending requests eventually resolve;\n")
        self.assertEqual(check(future)["status"], "unsupported")
        self.assertEqual(parse(future.render(True), "english"), future)

    def test_trace_and_mutations(self):
        trace = replay(self.model, [{"event": "submit", "context": {"authorized": False}},
                                    {"event": "approve", "context": {"authorized": False}},
                                    {"event": "cancel", "context": {"authorized": True}}])
        self.assertEqual([r["outcome"]["class"] for r in trace], ["Accept", "Reject", "Accept"])
        self.assertEqual(trace[-1]["outcome"]["state"], "cancelled")
        for mutated in [SOURCE.replace("[authorized]", "[!authorized]"),
                        SOURCE.replace("accept approved", "accept cancelled"),
                        SOURCE.replace("not_enabled", "different_reason")]:
            diff = semantic_diff(self.model, parse(mutated))
            self.assertEqual(diff["comparison"], "different")
            self.assertTrue(diff["witnesses"])
            self.assertIn("budget refusals", diff["excluded_observations"])
        with self.assertRaises(Refusal):
            self.model.decide("draft", "submit", {"authorized": 1})

    def test_exact_bounds(self):
        source = SOURCE.replace("draft, pending, approved, cancelled", ", ".join("s" + str(i) for i in range(8)))
        source = source.replace("submit, approve, cancel", ", ".join("e" + str(i) for i in range(8)))
        source = source.replace("context authorized", "context a, b, c, d").replace("initial draft", "initial s0")
        source = "\n".join(x for x in source.splitlines() if not x.startswith("rule "))
        self.assertEqual(check(parse(source))["input_tuples"], 1024)
        with self.assertRaises(Refusal):
            parse(source.replace("context a, b, c, d", "context a, b, c, d, e"))

    def test_canonical_formula_expansion_and_negation(self):
        for count in range(1, 16):
            model = parse(SOURCE.replace("[authorized]", "[" + "!" * count + "authorized]"))
            self.assertEqual(parse(model.render()), model)
            self.assertEqual(parse(model.render(True), "english"), model)
        model = parse(SOURCE.replace("[authorized]", "[(authorized & state == pending) | (!authorized & state == pending)]"))
        self.assertEqual(parse(model.render()), model)
        self.assertEqual(parse(model.render(True), "english"), model)

    def test_owned_trace_and_direct_ir_admission(self):
        inputs = [{"event": "submit", "context": {"authorized": False}}]
        trace = replay(self.model, inputs)
        inputs[0]["context"]["authorized"] = True
        self.assertFalse(trace[0]["context"]["authorized"])
        with self.assertRaises(Refusal):
            Model("empty", (), (), (), "missing", "refused", (), ())
        with self.assertRaises(Refusal):
            Formula("unknown")
        with self.assertRaises(Refusal):
            Model("bad_order", ("a", "b"), ("tick",), ("a", "b"), "a", "refused",
                  (Rule("r", "a", "tick", Formula("and", (Formula("fact", ("b",)), Formula("fact", ("a",)))), "b", None),), ())

    def test_whole_canonical_projection_bound(self):
        states = ["s" + str(i) + "x" * 30 for i in range(8)]
        facts = ["f" + str(i) + "x" * 30 for i in range(4)]
        event = "e" * 32
        guard = " | ".join(f"({facts[i // 8]} & state == {states[i % 8]})" for i in range(23))
        source = f"zal 1;\nmachine long_model;\nstates {', '.join(states)};\nevents {event};\ncontext {', '.join(facts)};\ninitial {states[0]};\nstep atomic; frame control_only;\ndefault reject refused unchanged;\n"
        source += "\n".join(f"rule r{i:02}: {states[0]} + {event} [{guard}] -> accept {states[1]};" for i in range(32))
        self.assertLess(len(source), 65536)
        with self.assertRaisesRegex(Refusal, "Canonical .* model exceeds"):
            parse(source)

    def test_reachable_and_raw_state_obligations_differ(self):
        source = """zal 1;
machine raw_states;
states bad, good;
events step;
context none;
initial good;
step atomic; frame control_only;
default reject not_enabled unchanged;
invariant good_only: state == good;
"""
        valid = parse(source)
        self.assertEqual(check(valid)["status"], "pass-with-scope")
        self.assertEqual(valid.decide("bad", "step", {}), {"class": "Reject", "state": "bad", "reason": "not_enabled", "rule": "default"})
        invalid = parse(source + "rule unreachable_bad: bad + step [true] -> accept bad;\n")
        report = check(invalid)
        self.assertEqual(report["checks"]["reachable_invariants"], "pass-with-scope")
        self.assertEqual(report["checks"]["raw_commit_invariants"], "counterexample")

    def test_initial_diff_and_identifier_contract(self):
        diff = semantic_diff(self.model, parse(SOURCE.replace("initial draft", "initial approved")))
        self.assertEqual(diff["comparison"], "different")
        self.assertEqual(diff["witnesses"][0]["kind"], "initial-state")
        for text in [SOURCE.replace("[authorized]", "[¬authorized]"), SOURCE.replace("machine order_dialogue", 'machine "order_dialogue"'), SOURCE.replace("context authorized", "context true")]:
            with self.assertRaises(Refusal):
                parse(text)


class WorkflowTests(unittest.TestCase):
    def setUp(self):
        self.session = Session(parse(SOURCE))

    def act(self, kind="explain", surface=None):
        return {"act": kind, "base_revision": self.session.model.revision, "object_ids": ["rule:cancel_order"],
                "message": "Cancellation after approval is currently rejected.", "syntax": "symbolic", "surface": surface}

    def test_explanation_and_selection_do_not_mutate(self):
        before = self.session.model
        self.session.agent_act(self.act())
        self.session.select("rule:cancel_order", before.revision)
        self.assertEqual(self.session.model, before)
        self.assertIsNone(self.session.proposal)
        self.assertEqual(self.session.agreement, "seed-example")

    def test_stale_extra_fields_and_self_agree_refused(self):
        for bad in [{**self.act(), "base_revision": "stale"}, {**self.act(), "accepted": True},
                    {**self.act(), "act": "agree"}, {**self.act(), "object_ids": ["invented"]}]:
            with self.assertRaises(Refusal):
                self.session.agent_act(bad)

    def test_accept_exact_revision_once_and_source_drift(self):
        self.session.agent_act(self.act("propose", SOURCE + "rule cancel_approved: approved + cancel [true] -> accept cancelled;\n"))
        proposal = self.session.proposal
        self.assertEqual(self.session.agreement, "seed-example")
        self.assertEqual(proposal["diff"]["witnesses"][0]["state"], "approved")
        with patch("workflow.checker_identity", return_value="changed"):
            with self.assertRaises(Refusal):
                self.session.accept_human(proposal["base_revision"], proposal["revision"])
        self.session.accept_human(proposal["base_revision"], proposal["revision"])
        self.assertEqual(self.session.agreement, "human-accepted")
        self.assertEqual(self.session.realization, "not-run")
        with self.assertRaises(Refusal):
            self.session.accept_human(proposal["base_revision"], proposal["revision"])

    def test_refused_events_do_not_change_state_and_views_are_owned(self):
        with self.assertRaises(Refusal):
            self.session.agent_act(self.act("propose", SOURCE))
        self.session.events = [{}] * 256
        before = self.session.selected
        with self.assertRaises(Refusal):
            self.session.select("state:pending", self.session.model.revision)
        self.assertEqual(self.session.selected, before)
        with self.assertRaises(Refusal):
            self.session.agent_act(self.act("propose", SOURCE.replace("not_enabled", "other_reason")))
        self.assertIsNone(self.session.proposal)
        view = self.session.view()
        view["evidence"]["status"] = "fake"
        self.assertNotEqual(self.session.evidence["status"], "fake")

    def test_requirement_agreement_does_not_discharge_obligation(self):
        self.session.agent_act(self.act("propose", SOURCE + "unresolved fairness: pending requests eventually resolve;\n"))
        proposal = self.session.proposal
        self.assertEqual(proposal["evidence"]["status"], "unsupported")
        self.session.accept_human(proposal["base_revision"], proposal["revision"])
        self.assertEqual(self.session.agreement, "human-accepted")
        self.assertEqual(self.session.evidence["status"], "unsupported")
        self.assertEqual(self.session.realization, "not-run")


if __name__ == "__main__":
    unittest.main()
