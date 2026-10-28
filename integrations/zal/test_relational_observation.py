"""Paired closure controls and an independent bounded-word reference.

The word oracle below is separately written for the tiny copy/reveal machines.
It does not use the checker graph, its observation function or ZAL evaluation.
"""
import copy
from dataclasses import FrozenInstanceError, replace
import itertools
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import behavior2 as b
import relational_observation as r


HERE = Path(__file__).resolve().parent
EXAMPLES = HERE / "examples" / "observations"
PRIVATE = (EXAMPLES / "relational_private.zal").read_text()
LEAK = (EXAMPLES / "relational_delayed_leak.zal").read_text()


def profile(model, **updates):
    value = {"schema": r.SCHEMA, "model_revision": model.revision,
             "initial_secret_values": {"secret": [0, 1]},
             "public_context": ["allowed"], "secret_context": ["noise"],
             "observe": {"variables": ["public_value"], "control": True}}
    value.update(updates)
    return r.parse_profile(value, model)


def word_oracle(leak, horizon):
    """Enumerate all finite paired words, independently of the production checker."""
    inputs = list(itertools.product(("copy", "reveal"), (False, True), (False, True), (False, True)))
    for length in range(horizon + 1):
        for word in itertools.product(inputs, repeat=length):
            for left_secret, right_secret in itertools.product((0, 1), repeat=2):
                left = [0, 0, left_secret]
                right = [0, 0, right_secret]
                for event, allowed, _left_noise, _right_noise in word:
                    observed = []
                    for values in (left, right):
                        if allowed:
                            if event == "copy":
                                values[0] = values[2]
                            else:
                                values[1] = values[0] if leak else 0
                        observed.append(("Accept" if allowed else "Reject",
                                         None if allowed else "not_allowed", values[1]))
                    if observed[0] != observed[1]:
                        return {"status": "COUNTEREXAMPLE", "length": length}
    return {"status": "CHECKED_THROUGH_HORIZON", "horizon": horizon}


def replay_counterexample(model, report):
    """Replay the concrete pair without the checker's reachability machinery."""
    witness = report["counterexample"]
    left, right = copy.deepcopy(witness["initial"]["left"]), copy.deepcopy(witness["initial"]["right"])
    for step in witness["steps"]:
        for side, state in (("left", left), ("right", right)):
            actual = model.decide(state["control"], state["variables"], step["event"], step[side]["context"])
            if actual != step[side]["outcome"]:
                raise AssertionError("counterexample outcome mismatch")
            state.update(control=actual["state"], variables=actual["variables"])
    return left, right


class RelationalObservation(unittest.TestCase):
    def test_complete_private_product_and_owned_inputs(self):
        model = b.parse(PRIVATE)
        wire = profile(model).wire()
        admitted = r.parse_profile(wire, model)
        before = (model.data(), admitted.wire())
        result = r.check(model, admitted)
        self.assertEqual(result["status"], "CHECKED")
        # Four initial pairs plus three new pairs after copying a secret one.
        self.assertEqual(result["measured"]["states"], 7)
        self.assertEqual(result["measured"]["edges"], 112)
        self.assertEqual(result["nonvacuity"]["distinct_ordered_initial_pairs"], 2)
        self.assertEqual(result["observer"]["mandatory"], ["class", "reason"])
        wire["initial_secret_values"]["secret"].clear()
        result["profile"]["initial_secret_values"]["secret"].clear()
        self.assertEqual((model.data(), admitted.wire()), before)
        self.assertEqual(r.check(model, admitted)["status"], "CHECKED")
        with self.assertRaises(FrozenInstanceError):
            admitted.observed_control = False

    def test_delayed_leak_shortest_pair_witness(self):
        model = b.parse(LEAK)
        result = r.check(model, profile(model))
        self.assertEqual(result["status"], "COUNTEREXAMPLE")
        self.assertEqual(result["counterexample"]["length"], 2)
        self.assertEqual([s["event"] for s in result["counterexample"]["steps"]], ["copy", "reveal"])
        left, right = replay_counterexample(model, result)
        self.assertNotEqual(left["variables"]["public_value"], right["variables"]["public_value"])
        self.assertEqual(word_oracle(True, 1)["status"], "CHECKED_THROUGH_HORIZON")
        self.assertEqual(word_oracle(True, 2), {"status": "COUNTEREXAMPLE", "length": 2})

    def test_independent_word_oracle(self):
        # Agreement through depth three is differential evidence only; the
        # graph closure, rather than this finite-word sample, justifies CHECKED.
        self.assertEqual(word_oracle(False, 3)["status"], "CHECKED_THROUGH_HORIZON")
        self.assertEqual(r.check(b.parse(PRIVATE), profile(b.parse(PRIVATE)))["status"], "CHECKED")

    def test_outcome_class_only_leak(self):
        source = PRIVATE.replace("[allowed]", "[allowed & secret == 0]")
        model = b.parse(source)
        result = r.check(model, profile(model, observe={"variables": [], "control": False}))
        self.assertEqual(result["status"], "COUNTEREXAMPLE")
        observations = result["counterexample"]["observations"]
        self.assertNotEqual(observations["left"]["class"], observations["right"]["class"])

    def test_reason_only_leak(self):
        source = PRIVATE[:PRIVATE.index("rule remember:")] + """rule deny_zero: ready + copy [secret == 0] -> reject denied_zero unchanged;
rule deny_one: ready + copy [secret == 1] -> reject denied_one unchanged;
"""
        model = b.parse(source)
        result = r.check(model, profile(model, observe={"variables": [], "control": False}))
        self.assertEqual(result["status"], "COUNTEREXAMPLE")
        obs = result["counterexample"]["observations"]
        self.assertEqual(obs["left"]["class"], obs["right"]["class"])
        self.assertNotEqual(obs["left"]["reason"], obs["right"]["reason"])

    def test_secret_context_and_integer_input_coupling(self):
        source = PRIVATE.replace("context allowed, noise;", "context allowed, noise;\ninput hint in 0..=1;")
        source = source.replace("public_value = 0;", "public_value = hint;")
        model = b.parse(source)
        coupled = profile(model, public_context=["allowed", "hint"], secret_context=["noise"])
        self.assertEqual(r.check(model, coupled)["status"], "CHECKED")
        independent = profile(model, public_context=["allowed"], secret_context=["hint", "noise"])
        result = r.check(model, independent)
        self.assertEqual(result["status"], "COUNTEREXAMPLE")
        step = result["counterexample"]["steps"][0]
        self.assertNotEqual(step["left"]["context"]["hint"], step["right"]["context"]["hint"])

    def test_initial_observation_can_refute_without_a_step(self):
        model = b.parse(PRIVATE)
        result = r.check(model, profile(model, observe={"variables": ["secret"], "control": False}))
        self.assertEqual(result["status"], "COUNTEREXAMPLE")
        self.assertEqual(result["counterexample"]["length"], 0)

    def test_source_invariants_are_not_silent_domain_filters(self):
        model = b.parse(PRIVATE + "invariant only_zero: secret == 0;\n")
        result = r.check(model, profile(model))
        self.assertEqual(result["status"], "UNKNOWN")
        self.assertIn("only_zero", result["reason"])
        model = b.parse(PRIVATE + "invariant cache_zero: hidden == 0;\n")
        result = r.check(model, profile(model))
        self.assertEqual(result["status"], "UNKNOWN")
        self.assertIn("cache_zero", result["reason"])

    def test_undefined_model_decisions_are_unknown(self):
        source = PRIVATE.replace("hidden = secret", "hidden += 1")
        model = b.parse(source)
        result = r.check(model, profile(model))
        self.assertEqual(result["status"], "UNKNOWN")
        self.assertIn("undefined model decision", result["reason"])

    def test_budget_boundaries_and_failed_expansion(self):
        model = b.parse(PRIVATE)
        admitted = profile(model)
        full = r.check(model, admitted)
        for name, count in full["measured"].items():
            if name == "decisions":
                continue
            self.assertEqual(r.check(model, admitted, **{"max_" + name: count})["status"], "CHECKED")
            self.assertEqual(r.check(model, admitted, **{"max_" + name: count - 1})["status"], "UNKNOWN")
        for option in ({"max_states": 0}, {"max_edges": 0}, {"max_work": 0}):
            result = r.check(model, admitted, **option)
            self.assertEqual(result["status"], "UNKNOWN")
            self.assertIsNone(result["counterexample"])
        self.assertEqual(r.check(model, admitted, max_states=3)["measured"]["states"], 0)
        for value in (-1, True, 1000001):
            with self.assertRaises(b.Refusal):
                r.check(model, admitted, max_work=value)

    def test_strict_profile_admission(self):
        model = b.parse(PRIVATE)
        mutations = [
            lambda v: v.update(model_revision="0" * 64),
            lambda v: v.update(extra=True),
            lambda v: v.update(public_context=[]),
            lambda v: v.update(secret_context=["allowed", "noise"]),
            lambda v: v.update(public_context=["allowed", "allowed"]),
            lambda v: v.update(initial_secret_values={}),
            lambda v: v.update(initial_secret_values={"secret": [0]}),
            lambda v: v.update(initial_secret_values={"secret": [0, True]}),
            lambda v: v.update(initial_secret_values={"secret": [1, 0]}),
            lambda v: v.update(initial_secret_values={"secret": [0, 2]}),
            lambda v: v["observe"].update(control=1),
            lambda v: v["observe"].update(variables=["missing"]),
            lambda v: v["observe"].update(omit_reason=True),
        ]
        for mutation in mutations:
            value = profile(model).wire()
            mutation(value)
            with self.subTest(value=value), self.assertRaises(b.Refusal):
                r.parse_profile(value, model)

    def test_unhandled_source_properties_refused(self):
        for extra in ("require harmless: accepted => public_value' == 0;\n",
                      "property stays_zero: always(public_value == 0);\n",
                      "unresolved effects: delivery remains outside this model;\n"):
            model = b.parse(PRIVATE + extra)
            with self.assertRaisesRegex(b.Refusal, "transitions and state invariants only"):
                profile(model)

    def test_direct_profile_revalidated(self):
        model = b.parse(PRIVATE)
        admitted = replace(profile(model), secret_context=())
        with self.assertRaisesRegex(b.Refusal, "partition"):
            r.check(model, admitted)

    def test_canonical_example_profiles(self):
        for name, expected in (("relational_private", "CHECKED"), ("relational_delayed_leak", "COUNTEREXAMPLE")):
            model = b.parse((EXAMPLES / (name + ".zal")).read_text())
            value = json.loads((EXAMPLES / (name + ".json")).read_text())
            self.assertEqual(r.check(model, r.parse_profile(value, model))["status"], expected)

    def test_cli_source_binding_exits_and_overwrite_refusal(self):
        with tempfile.TemporaryDirectory() as temp:
            out = Path(temp) / "report.json"
            argv = [sys.executable, str(HERE / "relational_observation.py"),
                    str(EXAMPLES / "relational_private.zal"), str(EXAMPLES / "relational_private.json")]
            result = subprocess.run([*argv, "--out", str(out)], text=True, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            packet = json.loads(result.stdout)
            self.assertEqual(packet, json.loads(out.read_text()))
            self.assertEqual(set(packet["binding"]["checker_sources"]), set(r.SOURCES))
            before = out.read_bytes()
            again = subprocess.run([*argv, "--out", str(out)], text=True, capture_output=True)
            self.assertEqual(again.returncode, 2)
            self.assertEqual(out.read_bytes(), before)
            limited = subprocess.run([*argv, "--max-work", "0"], text=True, capture_output=True)
            self.assertEqual(limited.returncode, 2)
            leak = subprocess.run([a.replace("relational_private", "relational_delayed_leak") for a in argv],
                                  text=True, capture_output=True)
            self.assertEqual(leak.returncode, 1, leak.stdout + leak.stderr)

    def test_cli_rejects_duplicate_json_and_symlink_output(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            malformed = root / "profile.json"
            malformed.write_text('{"schema": "a", "schema": "b"}')
            argv = [sys.executable, str(HERE / "relational_observation.py"),
                    str(EXAMPLES / "relational_private.zal")]
            result = subprocess.run([*argv, str(malformed)], text=True, capture_output=True)
            self.assertEqual(result.returncode, 2)
            self.assertIn("duplicate JSON", result.stdout)
            target = root / "protected"
            target.write_text("keep")
            link = root / "link"
            link.symlink_to(target)
            result = subprocess.run([*argv, str(EXAMPLES / "relational_private.json"), "--out", str(link)],
                                    text=True, capture_output=True)
            self.assertEqual(result.returncode, 2)
            self.assertEqual(target.read_text(), "keep")


if __name__ == "__main__":
    unittest.main()
