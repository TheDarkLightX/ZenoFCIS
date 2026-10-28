"""Observation adapter controls; existing arena tests cover its solver separately."""
import copy
import unittest

import behavior as Z
import observation_game as G


SOURCE = """zal 1;
machine uncertain_delivery;
states start, applied, absent, good, bad;
events apply, drop, retry, stop, hold;
context none;
initial start;
step atomic; frame control_only;
default reject unavailable unchanged;
rule apply_effect: start + apply [true] -> accept applied;
rule drop_effect: start + drop [true] -> accept absent;
rule retry_applied: applied + retry [true] -> accept bad;
rule stop_applied: applied + stop [true] -> accept good;
rule retry_absent: absent + retry [true] -> accept good;
rule stop_absent: absent + stop [true] -> accept bad;
rule hold_good: good + hold [true] -> accept good;
rule hold_bad: bad + hold [true] -> accept bad;
"""


def fixture(source=SOURCE, *, informed=False, max_cost=None, response_bound=2):
    model = Z.parse(source)
    value = dict(schema=G.SCHEMA, model_revision=model.revision,
                 observation="perfect" if informed else "partial", bad_states=["bad"],
                 states=[dict(state=s, owner=0 if s in ("applied", "absent") else 1,
                              observation="unknown" if s in ("applied", "absent") else s,
                              events=["retry", "stop"] if s in ("applied", "absent") else
                              ["apply", "drop"] if s == "start" else ["hold"]) for s in model.states],
                 costs=[dict(event=e, units=0 if e == "hold" else 1) for e in model.events],
                 max_cost=max_cost, service_states=["good"] if response_bound is not None else [],
                 response_bound=response_bound)
    return model, value


class ObservationGame(unittest.TestCase):
    def run_case(self, **kwargs):
        model, value = fixture(**kwargs)
        profile = G.parse_profile(value, model)
        report = G.check(model, profile)
        if report["status"] != "UNKNOWN":
            self.assertEqual(G.verify(model, profile, report), report["verdict"])
        return model, value, profile, report

    def test_uncertainty_and_quantifier_order(self):
        _, _, _, hidden = self.run_case()
        self.assertEqual(hidden["status"], "UNREALIZABLE")
        model, _, _, informed = self.run_case(informed=True)
        self.assertEqual(informed["status"], "CHECKED")
        # Inspect the checked belief itself, not a caller-declared Unknown tag.
        metadata = hidden["compilation"]["nodes"]
        beliefs = [{metadata[i].get("state") for i in n["belief"]}
                   for n in hidden["packet"]["knowledge"]["nodes"] if n["action"] is None]
        self.assertIn({"applied", "absent"}, beliefs)
        self.assertEqual(hidden["model_revision"], model.revision)

    def test_idempotency_repairs_uniform_strategy(self):
        source = SOURCE.replace("applied + retry [true] -> accept bad", "applied + retry [true] -> accept good")
        _, _, _, report = self.run_case(source=source)
        self.assertEqual(report["status"], "CHECKED")
        choices = dict(report["packet"]["certificate"]["strategy"])
        graph = report["packet"]["knowledge"]
        meta = report["compilation"]["nodes"]
        found = False
        for i, node in enumerate(graph["nodes"]):
            if (node["action"] is None and i in choices and
                    {meta[s].get("state") for s in node["belief"]} == {"applied", "absent"}):
                self.assertEqual(choices[i], "retry")
                found = True
        self.assertTrue(found)

    def test_source_and_profile_binding(self):
        model, value, profile, report = self.run_case()
        for field in ("model_revision", "profile_revision"):
            bad = copy.deepcopy(report); bad[field] = "0" * 64
            with self.assertRaisesRegex(Z.Refusal, "stale"):
                G.verify(model, profile, bad)
        bad = copy.deepcopy(report); bad["scope"]["completion"] = "unconditional eventual delivery"
        with self.assertRaisesRegex(Z.Refusal, "scope"):
            G.verify(model, profile, bad)
        value["states"][0]["observation"] = "different"
        with self.assertRaisesRegex(Z.Refusal, "stale"):
            G.verify(model, G.parse_profile(value, model), report)

    def test_omitted_world_and_forged_certificate_refused(self):
        model, _, profile, report = self.run_case()
        bad = copy.deepcopy(report)
        bad["compilation"]["arena"]["nodes"][0]["edges"].pop()
        with self.assertRaisesRegex(Z.Refusal, "adapter"):
            G.verify(model, profile, bad)
        bad = copy.deepcopy(report)
        for node in bad["packet"]["knowledge"]["nodes"]:
            if len(node["belief"]) > 1:
                node["belief"].pop()
                break
        with self.assertRaises(ValueError):
            G.verify(model, profile, bad)
        bad = copy.deepcopy(report); bad["authority"] = "production"
        with self.assertRaisesRegex(Z.Refusal, "scope"):
            G.verify(model, profile, bad)

    def test_strict_profile_and_owned_snapshot(self):
        model, value = fixture()
        profile = G.parse_profile(value, model)
        saved = profile.wire()
        value["states"][0]["events"].clear()
        self.assertEqual(profile.wire(), saved)
        tests = []
        bad = copy.deepcopy(saved); bad["states"].pop(); tests.append(bad)
        bad = copy.deepcopy(saved); bad["costs"][0]["units"] = True; tests.append(bad)
        bad = copy.deepcopy(saved); bad["response_bound"] = True; tests.append(bad)
        bad = copy.deepcopy(saved); bad["max_cost"] = 256; tests.append(bad)
        bad = copy.deepcopy(saved); bad["model_revision"] = "0" * 64; tests.append(bad)
        bad = copy.deepcopy(saved); bad["hidden_authority"] = True; tests.append(bad)
        bad = copy.deepcopy(saved); bad["service_states"] = []; tests.append(bad)
        bad = copy.deepcopy(saved); bad["states"][0]["events"] = ["retry", "retry"]; tests.append(bad)
        for bad in tests:
            with self.subTest(bad=bad), self.assertRaises(Z.Refusal):
                G.parse_profile(bad, model)

    def test_enabled_actions_must_not_reveal_hidden_world(self):
        model, value = fixture()
        next(s for s in value["states"] if s["state"] == "absent")["events"] = ["retry"]
        with self.assertRaisesRegex(Z.Refusal, "enabled"):
            G.parse_profile(value, model)

    def test_reject_response_is_observed_even_when_state_unchanged(self):
        source = SOURCE.replace("applied + retry [true] -> accept bad",
                                "applied + retry [true] -> reject already_done unchanged")
        model, value = fixture(source, response_bound=None)
        graph = G.compile_game(model, G.parse_profile(value, model))
        labels = [n["observation"] for n in graph["arena"].wire()["nodes"]]
        self.assertTrue(any('"outcome":"Reject"' in o and '"reason":"already_done"' in o for o in labels))
        self.assertTrue(any('"outcome":"Accept"' in o for o in labels))
        self.assertTrue(all('retry_applied' not in o for o in labels))

    def test_cost_bound_counts_hidden_environment_events(self):
        source = SOURCE.replace("applied + retry [true] -> accept bad", "applied + retry [true] -> accept good")
        _, _, _, tight = self.run_case(source=source, max_cost=2)
        _, _, _, too_small = self.run_case(source=source, max_cost=1)
        self.assertEqual(tight["status"], "CHECKED")
        self.assertEqual(too_small["status"], "UNREALIZABLE")

    def test_response_monitor_counts_original_steps_only(self):
        source = SOURCE.replace("applied + retry [true] -> accept bad", "applied + retry [true] -> accept good")
        for bound, status in ((0, "UNREALIZABLE"), (1, "UNREALIZABLE"), (2, "CHECKED")):
            _, _, _, report = self.run_case(source=source, response_bound=bound)
            self.assertEqual(report["status"], status)

    def test_reject_all_does_not_establish_service(self):
        source = SOURCE.replace("-> accept good;", "-> reject unavailable unchanged;").replace(
            "-> accept bad;", "-> reject unavailable unchanged;")
        _, _, _, safety = self.run_case(source=source, response_bound=None)
        _, _, _, service = self.run_case(source=source, response_bound=2)
        self.assertEqual(safety["status"], "CHECKED")
        self.assertEqual(safety["scope"]["completion"], "not claimed")
        self.assertEqual(service["status"], "UNREALIZABLE")

    def test_limits_are_unknown_without_certificate(self):
        model, value = fixture()
        profile = G.parse_profile(value, model)
        for limits in (dict(max_states=0), dict(max_edges=0), dict(max_work=0), dict(seconds=0)):
            report = G.check(model, profile, **limits)
            self.assertEqual(report["status"], "UNKNOWN")
            self.assertNotIn("packet", report)
            with self.assertRaisesRegex(Z.Refusal, "not a certificate"):
                G.verify(model, profile, report)

    def test_source_initial_and_invariants_preserved(self):
        source = SOURCE + "invariant no_start: !(state == start);\n"
        _, _, _, report = self.run_case(source=source)
        self.assertEqual(report["status"], "UNREALIZABLE")
        self.assertEqual(report["compilation"]["nodes"][0]["state"], "start")

    def test_invalid_unselected_source_transition_not_hidden(self):
        source = SOURCE + "rule overlapping: start + retry [true] -> accept good;\nrule overlapping_more: start + retry [true] -> accept bad;\n"
        _, _, _, report = self.run_case(source=source)
        self.assertEqual(report["status"], "UNKNOWN")
        self.assertIn("Overlapping", report["stop"])


if __name__ == "__main__":
    unittest.main()
