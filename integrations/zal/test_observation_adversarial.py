"""Independent controls for observation histories, paired inputs and replay.

The finite Boolean-function oracle below uses the truth table directly, not
the ZAL evaluator or the paired graph search.
"""
import copy
import hashlib
import itertools
import json
from pathlib import Path
import tempfile
import unittest

import behavior
import behavior2
import observation_game as game
import observations
import relational_observation as relational


HERE = Path(__file__).resolve().parent
EXAMPLES = HERE / "examples" / "observations"


def probe_fixture(revealing):
    source = """zal 1;
machine observe_then_choose;
states start, applied, absent, good, bad;
events apply, drop, probe, retry, stop, hold;
context none;
initial start;
step atomic; frame control_only;
default reject unavailable unchanged;
rule physical_apply: start + apply [true] -> accept applied;
rule physical_drop: start + drop [true] -> accept absent;
rule probe_applied: applied + probe [true] -> reject was_applied unchanged;
rule probe_absent: absent + probe [true] -> reject was_absent unchanged;
rule retry_applied: applied + retry [true] -> accept bad;
rule stop_applied: applied + stop [true] -> accept good;
rule retry_absent: absent + retry [true] -> accept good;
rule stop_absent: absent + stop [true] -> accept bad;
rule good_hold: good + hold [true] -> accept good;
rule bad_hold: bad + hold [true] -> accept bad;
"""
    if not revealing:
        source = source.replace("was_applied", "unavailable").replace("was_absent", "unavailable")
    model = behavior.parse(source)
    profile = {
        "schema": game.SCHEMA, "model_revision": model.revision,
        "observation": "partial", "bad_states": ["bad"],
        "states": [dict(state=s, owner=0 if s in ("applied", "absent") else 1,
                        observation="unknown" if s in ("applied", "absent") else s,
                        events=["probe", "retry", "stop"] if s in ("applied", "absent")
                        else ["apply", "drop"] if s == "start" else ["hold"])
                   for s in model.states],
        "costs": [dict(event=e, units=0) for e in model.events],
        "max_cost": None, "service_states": ["good"], "response_bound": 3,
    }
    return model, game.parse_profile(profile, model)


class ObservationAdversarial(unittest.TestCase):
    def test_rejected_probe_can_supply_knowledge_without_changing_business_state(self):
        for revealing, expected in ((False, "UNREALIZABLE"), (True, "CHECKED")):
            with self.subTest(revealing=revealing):
                model, profile = probe_fixture(revealing)
                report = game.check(model, profile)
                self.assertEqual(report["status"], expected)
                self.assertEqual(game.verify(model, profile, report), report["verdict"])
                if revealing:
                    graph = report["packet"]["knowledge"]
                    rows = report["compilation"]["nodes"]
                    choices = dict(report["packet"]["certificate"]["strategy"])
                    initial_choice = [i for i, node in enumerate(graph["nodes"])
                                      if node["action"] is None and
                                      all(rows[s].get("phase") == "world" for s in node["belief"]) and
                                      {rows[s].get("state") for s in node["belief"]} == {"applied", "absent"} and
                                      {rows[s].get("elapsed") for s in node["belief"]} == {1}]
                    self.assertEqual(len(initial_choice), 1)
                    self.assertEqual(choices[initial_choice[0]], "probe")

    def test_hidden_cost_monitor_does_not_reveal_environment_choice(self):
        source = """zal 1;
machine hidden_cost;
states start, choose, good;
events cheap, expensive, finish, hold;
context none;
initial start;
step atomic; frame control_only;
default reject unavailable unchanged;
rule cheap_path: start + cheap [true] -> accept choose;
rule expensive_path: start + expensive [true] -> accept choose;
rule finish_work: choose + finish [true] -> accept good;
rule hold_good: good + hold [true] -> accept good;
"""
        model = behavior.parse(source)
        value = dict(schema=game.SCHEMA, model_revision=model.revision, observation="partial",
                     bad_states=[], states=[
                         dict(state="start", owner=1, observation="start", events=["cheap", "expensive"]),
                         dict(state="choose", owner=0, observation="choose", events=["finish"]),
                         dict(state="good", owner=1, observation="good", events=["hold"])],
                     costs=[dict(event=e, units=int(e == "expensive")) for e in model.events],
                     max_cost=1, service_states=["good"], response_bound=2)
        report = game.check(model, game.parse_profile(value, model))
        self.assertEqual(report["status"], "CHECKED")
        rows = report["compilation"]["nodes"]
        beliefs = [{(rows[s].get("state"), rows[s].get("spent")) for s in node["belief"]}
                   for node in report["packet"]["knowledge"]["nodes"] if node["action"] is None]
        self.assertIn({("choose", 0), ("choose", 1)}, beliefs)

    def test_all_boolean_secret_functions_match_truth_table_oracle(self):
        counts = {"CHECKED": 0, "COUNTEREXAMPLE": 0}
        for truth_table in itertools.product((0, 1), repeat=4):
            lines = ["zal 2;", "machine finite_boolean_function;", "states ready;", "events publish;",
                     "context allowed, noise;", "var secret in 0..=1 initially 0;",
                     "var output in 0..=1 initially 0;", "initial ready;",
                     "step atomic; frame control_and_variables;", "default reject unavailable unchanged;"]
            for row, (secret, noise) in enumerate(itertools.product((0, 1), repeat=2)):
                guard = f"allowed & secret == {secret} & " + ("noise" if noise else "!noise")
                lines.append(f"rule row_{row}: ready + publish [{guard}] -> accept ready, output = {truth_table[row]};")
            model = behavior2.parse("\n".join(lines))
            profile = relational.parse_profile(dict(
                schema=relational.SCHEMA, model_revision=model.revision,
                initial_secret_values={"secret": [0, 1]}, public_context=["allowed"], secret_context=["noise"],
                observe=dict(variables=["output"], control=True)), model)
            report = relational.check(model, profile)
            expected = "CHECKED" if len(set(truth_table)) == 1 else "COUNTEREXAMPLE"
            self.assertEqual(report["status"], expected, truth_table)
            if expected == "COUNTEREXAMPLE":
                self.assertEqual(report["counterexample"]["length"], 1)
            counts[expected] += 1
        self.assertEqual(counts, {"CHECKED": 2, "COUNTEREXAMPLE": 14})

    def test_game_bundle_cannot_relabel_its_execution_limits(self):
        model_path = EXAMPLES / "delivery_idempotent.zal"
        profile_path = EXAMPLES / "delivery_idempotent.partial.json"
        model_bytes, profile_bytes = model_path.read_bytes(), profile_path.read_bytes()
        model, profile = observations.prepare("game", model_bytes, profile_bytes)
        limits = dict(max_states=4096, max_edges=100000, max_work=1000000)
        report = observations.evaluate("game", model, profile, limits)
        self.assertEqual(report["status"], "CHECKED")
        with tempfile.TemporaryDirectory() as folder:
            out = Path(folder) / "evidence"
            observations.write_bundle(out, "game", model_bytes, profile_bytes, report,
                                      observations.identities(), limits)
            binding_path = out / "binding.json"
            binding = json.loads(binding_path.read_text())
            binding["limits"] = dict(max_states=0, max_edges=0, max_work=0)
            binding_path.write_bytes(observations.encode(binding))
            with self.assertRaises(ValueError):
                observations.verify(out, model_path, profile_path)

    def test_rehashed_game_results_still_need_the_complete_checking_allowance(self):
        model_path = EXAMPLES / "delivery_idempotent.zal"
        profile_path = EXAMPLES / "delivery_idempotent.partial.json"
        model_bytes, profile_bytes = model_path.read_bytes(), profile_path.read_bytes()
        model, profile = observations.prepare("game", model_bytes, profile_bytes)
        limits = dict(max_states=4096, max_edges=100000, max_work=1000000)
        report = observations.evaluate("game", model, profile, limits)
        self.assertEqual(report["status"], "CHECKED")
        forged_limits = (
            dict(max_states=0, max_edges=0, max_work=0),
            {**limits, "max_work": report["compilation"]["work"]},
        )
        with tempfile.TemporaryDirectory() as folder:
            for index, claimed in enumerate(forged_limits):
                with self.subTest(limits=claimed):
                    out = Path(folder) / str(index)
                    observations.write_bundle(out, "game", model_bytes, profile_bytes, report,
                                              observations.identities(), limits)
                    forged = copy.deepcopy(report)
                    forged["limits"] = claimed
                    result_bytes = observations.encode(forged)
                    (out / "result.json").write_bytes(result_bytes)
                    binding_path = out / "binding.json"
                    binding = json.loads(binding_path.read_text())
                    binding["limits"] = claimed
                    binding["artifacts"]["result.json"] = hashlib.sha256(result_bytes).hexdigest()
                    binding_path.write_bytes(observations.encode(binding))
                    with self.assertRaises(ValueError):
                        observations.verify(out, model_path, profile_path)


if __name__ == "__main__":
    unittest.main()
