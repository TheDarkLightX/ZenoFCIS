"""Independent negative controls for the finite ontology classification boundary."""
from copy import deepcopy
from itertools import product
import unittest
from unittest.mock import patch

from zal_notation.tests.ontology_guard_fixture import guard_fixture, TARGET
from zal_notation.core import NotationError
from zal_notation.ontology import classify_ontology, ontology_example, replay_ontology


class OntologyAdversarialTests(unittest.TestCase):
    def assertNotReplayed(self, document, report):
        try:
            outcome = replay_ontology(document, report)
        except NotationError:
            return
        self.assertNotEqual(outcome["status"], "replayed")

    def test_independent_full_boolean_guard_domain(self):
        for bits in product((False, True), repeat=4):
            with self.subTest(bits=bits):
                document = guard_fixture(bits)
                result = classify_ontology(document)
                self.assertEqual(result["status"], "classified")
                self.assertIs(result["complete"], True)
                self.assertEqual(TARGET in result["types"].get("run", []), all(bits))
                self.assertEqual(replay_ontology(document, result)["status"], "replayed")

    def test_unrelated_typed_filler_does_not_satisfy_link(self):
        document = guard_fixture((True, True, True, True))
        document["individuals"].append("unrelated")
        for fact in document["facts"]:
            if fact["id"] == "feedback_type":
                fact["individual"] = "unrelated"
        result = classify_ontology(document)
        self.assertNotIn(TARGET, result["types"].get("run", []))

    def test_different_subject_witnesses_do_not_combine(self):
        document = guard_fixture((True, True, True, True))
        document["individuals"].append("other")
        for fact in document["facts"]:
            if fact["id"] == "feedback_link":
                fact["subject"] = "other"
        result = classify_ontology(document)
        self.assertNotIn(TARGET, result["types"].get("run", []))
        self.assertNotIn(TARGET, result["types"].get("other", []))

    def test_reversing_a_witness_edge_does_not_satisfy_the_rule(self):
        document = guard_fixture((True, True, True, True))
        for fact in document["facts"]:
            if fact["id"] == "feedback_link":
                fact["subject"], fact["object"] = fact["object"], fact["subject"]
        result = classify_ontology(document)
        self.assertNotIn(TARGET, result["types"].get("run", []))

    def test_asserted_checked_status_does_not_install_authority(self):
        document = guard_fixture((True, True, False, True))
        for name, value in (("status", "checked"), ("checked", True), ("equivalent", True)):
            with self.subTest(name=name):
                forged = deepcopy(document)
                forged[name] = value
                with self.assertRaises(NotationError):
                    classify_ontology(forged)

    def test_permutation_preserves_closure_but_exact_input_identity_changes(self):
        document = guard_fixture((True, True, True, True))
        permuted = deepcopy(document)
        for key in ("classes", "properties", "individuals", "axioms", "facts"):
            permuted[key].reverse()
        original = classify_ontology(document)
        reversed_result = classify_ontology(permuted)
        for key in ("facts", "proofs", "types"):
            self.assertEqual(original[key], reversed_result[key], key)
        self.assertNotEqual(original["ontology_digest"], reversed_result["ontology_digest"])
        self.assertNotReplayed(permuted, original)

    def test_changed_fact_and_changed_axiom_invalidate_replay(self):
        document = guard_fixture((True, True, True, True))
        result = classify_ontology(document)
        changed_fact = deepcopy(document)
        changed_fact["facts"] = [f for f in changed_fact["facts"] if f["id"] != "feedback_link"]
        self.assertNotReplayed(changed_fact, result)
        changed_axiom = deepcopy(document)
        changed_axiom["axioms"][0]["some"] = changed_axiom["axioms"][0]["some"][:1]
        self.assertNotReplayed(changed_axiom, result)

    def test_input_and_returned_report_have_no_mutable_alias(self):
        document = guard_fixture((True, True, True, True))
        before = deepcopy(document)
        result = classify_ontology(document)
        baseline = deepcopy(result)
        result["types"]["run"].clear()
        result["proofs"].clear()
        self.assertEqual(document, before)
        self.assertEqual(classify_ontology(document), baseline)

    def test_limits_reject_boolean_and_nonintegral_aliases(self):
        document = guard_fixture((True, True, True, True))
        for key in ("max_work", "max_facts"):
            for value in (True, False, 1.0, "1", -1):
                with self.subTest(key=key, value=value):
                    with self.assertRaises(NotationError):
                        classify_ontology(document, **{key: value})

    def test_unsupported_axiom_does_not_leave_a_supported_prefix(self):
        document = guard_fixture((True, True, True, True))
        document["axioms"].append({"id": "identity", "kind": "sameAs", "left": "run", "right": "output"})
        result = classify_ontology(document)
        self.assertEqual(result["status"], "unsupported")
        self.assertIs(result["complete"], False)
        self.assertEqual((result["facts"], result["proofs"], result["types"]), ([], [], {}))
        self.assertEqual(result["consistency"], "unknown")

    def test_budget_exhaustion_replays_without_becoming_classified(self):
        document = guard_fixture((True, True, True, True))
        for limits in ({"max_work": 0}, {"max_work": 5}, {"max_facts": 0}, {"max_facts": 7}):
            with self.subTest(limits=limits):
                result = classify_ontology(document, **limits)
                self.assertEqual(result["status"], "incomplete")
                self.assertIs(result["complete"], False)
                self.assertEqual(result["consistency"], "unknown")
                self.assertLessEqual(result["usage"]["work"], result["limits"]["max_work"])
                self.assertLessEqual(result["usage"]["facts"], result["limits"]["max_facts"])
                replay = replay_ontology(document, result)
                self.assertEqual(replay["status"], "replayed")
                self.assertEqual(replay["classification_status"], "incomplete")
                self.assertIs(replay["complete"], False)

    def test_guard_proof_has_every_grounded_premise_and_no_cycles(self):
        document = guard_fixture((True, True, True, True))
        result = classify_ontology(document)
        facts = {f["id"]: f for f in result["facts"]}
        proofs = {p["fact"]: p for p in result["proofs"]}
        assertions = {f["id"]: {k: v for k, v in f.items() if k != "id"} for f in document["facts"]}
        self.assertEqual(set(facts), set(proofs))
        target = next(f["id"] for f in facts.values() if f["kind"] == "type" and f["class"] == TARGET)
        proof = proofs[target]
        self.assertEqual((proof["rule"], proof["axiom"], proof["assertion"]), ("classify", "guided", None))
        self.assertEqual(len(proof["premises"]), 7)
        def visit(identifier, active):
            self.assertNotIn(identifier, active, "cyclic explanation")
            self.assertIn(identifier, facts)
            entry = proofs[identifier]
            if entry["rule"] == "asserted":
                self.assertEqual(entry["premises"], [])
                self.assertEqual({k: v for k, v in facts[identifier].items() if k != "id"}, assertions[entry["assertion"]])
            for premise in entry["premises"]:
                visit(premise, active | {identifier})
        visit(target, set())
        self.assertEqual({proofs[p]["assertion"] for p in proof["premises"]}, set(assertions))

    def test_omitted_forged_and_cyclic_explanations_cannot_replay(self):
        document = guard_fixture((True, True, True, True))
        result = classify_ontology(document)
        derived = next(i for i, p in enumerate(result["proofs"]) if p["rule"] == "classify")
        for mutate in (lambda p: p["premises"].pop(),
                       lambda p: p["premises"].append("unasserted-premise"),
                       lambda p: p["premises"].append(p["fact"]),
                       lambda p: p.update(axiom="unregistered-rule")):
            forged = deepcopy(result)
            mutate(forged["proofs"][derived])
            self.assertNotReplayed(document, forged)

    def test_profile_checker_limits_and_claimed_status_are_bound(self):
        document = guard_fixture((True, True, True, True))
        result = classify_ontology(document)
        for key in ("profile", "checker", "ontology_digest", "status", "authority"):
            with self.subTest(key=key):
                changed = deepcopy(result)
                changed[key] = "forged"
                self.assertNotReplayed(document, changed)
        changed = deepcopy(result)
        changed["limits"]["max_work"] = 0
        self.assertNotReplayed(document, changed)

    def test_subclass_cycles_terminate_without_reverse_or_identity_inference(self):
        document = guard_fixture((False, False, False, False))
        document["classes"] = ["A", "B", "C", "D"]
        document["properties"] = []
        document["individuals"] = ["left", "right"]
        document["axioms"] = [
            {"id": "ab", "kind": "subclass", "sub": "A", "super": "B"},
            {"id": "ba", "kind": "subclass", "sub": "B", "super": "A"},
            {"id": "bc", "kind": "subclass", "sub": "B", "super": "C"},
        ]
        document["facts"] = [{"id": "a", "kind": "type", "individual": "left", "class": "A"},
                             {"id": "c", "kind": "type", "individual": "right", "class": "C"}]
        result = classify_ontology(document)
        self.assertEqual((result["status"], result["types"]), ("classified", {"left": ["A", "B", "C"], "right": ["C"]}))
        self.assertEqual(len(result["facts"]), 4)
        self.assertLess(result["usage"]["work"], 100)
        # Reasserting the complete positive fact set cannot add a consequence.
        saturated = deepcopy(document)
        saturated["facts"] = [dict(f, id="seed-" + str(i)) for i, f in enumerate(result["facts"])]
        repeated = classify_ontology(saturated)
        self.assertEqual(repeated["facts"], result["facts"])

    def test_inconsistency_has_a_grounded_witness_without_explosion(self):
        document = guard_fixture((True, True, True, True))
        document["classes"].extend(["Forbidden", "Unrelated"])
        document["axioms"].append({"id": "disjoint", "kind": "disjoint", "classes": [TARGET, "Forbidden"]})
        document["facts"].append({"id": "forbidden", "kind": "type", "individual": "run", "class": "Forbidden"})
        result = classify_ontology(document)
        self.assertEqual((result["status"], result["consistency"], result["complete"]), ("inconsistent", "inconsistent", False))
        fact_ids = {f["id"] for f in result["facts"]}
        self.assertTrue(set(result["contradiction"]["premises"]) <= fact_ids)
        self.assertNotIn("Unrelated", result["types"]["run"])
        replay = replay_ontology(document, result)
        self.assertEqual(replay["classification_status"], "inconsistent")
        self.assertIs(replay["complete"], False)

    def test_imported_origin_status_is_inert_and_identity_bound(self):
        document = guard_fixture((True, True, False, True))
        plain = classify_ontology(document)
        document["origin"] = {"status": "checked", "proof": "externally-certified", "equivalent": True}
        with_origin = classify_ontology(document)
        for key in ("facts", "types", "proofs"):
            self.assertEqual(plain[key], with_origin[key])
        self.assertEqual(with_origin["authority"], "none")
        self.assertNotIn(TARGET, with_origin["types"]["run"])
        self.assertNotReplayed(document, plain)

    def test_examples_share_conditional_classes_without_equivalence(self):
        document = ontology_example()
        result = classify_ontology(document)
        for name in ("trainingRun", "programmingSession"):
            self.assertIn(TARGET, result["types"][name])
        for name in ("alignmentReview", "requirementsReview"):
            self.assertIn("IntentCriterionReview", result["types"][name])
        self.assertNotIn("ProgrammingProcess", result["types"]["trainingRun"])
        self.assertNotIn("TrainingProcess", result["types"]["programmingSession"])
        self.assertNotIn("RequirementsAnalysis", result["types"]["alignmentReview"])
        self.assertTrue(all(f["kind"] in {"type", "property"} for f in result["facts"]))
        original_property_bodies = {tuple(sorted((k, v) for k, v in f.items() if k != "id")) for f in document["facts"] if f["kind"] == "property"}
        derived_property_bodies = {tuple(sorted((k, v) for k, v in f.items() if k != "id")) for f in result["facts"] if f["kind"] == "property"}
        self.assertEqual(original_property_bodies, derived_property_bodies)
        document["facts"] = [f for f in document["facts"] if not (f["kind"] == "property" and
                              f["subject"] == "trainingRun" and f["property"] == "usesFeedback")]
        changed = classify_ontology(document)
        self.assertNotIn(TARGET, changed["types"]["trainingRun"])
        self.assertIn(TARGET, changed["types"]["programmingSession"])


class OntologyHostAdversarialTests(unittest.TestCase):
    def test_changed_loaded_source_refuses_before_engine_execution(self):
        from zal_notation import ontology_tools as host
        changed = deepcopy(host.implementation_identity())
        changed["digest"] = "0" * 64
        with patch.object(host, "implementation_identity", return_value=changed), \
             patch.object(host, "classify_ontology") as engine:
            with self.assertRaisesRegex(NotationError, "restart"):
                host.classify(guard_fixture((True, True, True, True)))
            engine.assert_not_called()

    def test_source_change_during_evaluation_refuses(self):
        from zal_notation import ontology_tools as host
        current = host.implementation_identity()
        changed = deepcopy(current)
        changed["digest"] = "0" * 64
        with patch.object(host, "implementation_identity", side_effect=[current, changed]):
            with self.assertRaisesRegex(NotationError, "restart"):
                host.classify(guard_fixture((True, True, True, True)))

    def test_postcheck_read_cannot_relabel_loaded_code(self):
        from zal_notation import ontology_tools as host
        current = host.implementation_identity()
        changed = deepcopy(current)
        changed["digest"] = "0" * 64
        with patch.object(host, "implementation_identity", side_effect=[current, current, changed]):
            try:
                result = host.classify(guard_fixture((True, True, True, True)))
            except NotationError:
                return  # A final checked read may safely refuse the race.
        self.assertEqual(result["implementation"], current)

    def test_returned_source_identity_cannot_mutate_loaded_binding(self):
        from zal_notation import ontology_tools as host
        document = guard_fixture((True, True, True, True))
        original = host.implementation_identity()
        run = host.classify(document)
        run["implementation"]["files"].clear()
        run["implementation"]["digest"] = "0" * 64
        self.assertEqual(host.classify(document)["implementation"], original)

    def test_forged_host_identity_or_explanation_cannot_replay(self):
        from zal_notation import ontology_tools as host
        document = guard_fixture((True, True, True, True))
        run = host.classify(document)
        for kind in ("source", "proof"):
            forged = deepcopy(run)
            if kind == "source":
                forged["implementation"]["digest"] = "0" * 64
            else:
                proof = next(p for p in forged["result"]["proofs"] if p["rule"] == "classify")
                proof["premises"] = []
            self.assertIs(host.replay(document, forged)["matches_replay"], False)

    def test_host_missing_query_remains_unknown_and_retains_budget_status(self):
        from zal_notation import ontology_tools as host
        document = guard_fixture((True, True, False, True))
        result = host.query(document, individual="run", class_name=TARGET)
        self.assertEqual(result["matches"], [])
        self.assertIn("not a negative", result["absence"])
        partial = host.query(document, individual="run", class_name=TARGET, max_work=0)
        self.assertEqual(partial["matches"], [])
        self.assertEqual(partial["run"]["result"]["status"], "incomplete")

    def test_host_explanation_contains_exact_grounded_guard_subgraph(self):
        from zal_notation import ontology_tools as host
        document = guard_fixture((True, True, True, True))
        run = host.classify(document)
        target = next(f["id"] for f in run["result"]["facts"] if f["kind"] == "type" and f["class"] == TARGET)
        explanation = host.explain(document, target)
        self.assertEqual(explanation["status"], "explained")
        self.assertEqual(len(explanation["facts"]), 8)
        self.assertEqual(len(explanation["proofs"]), 8)
        self.assertTrue(all(set(p["premises"]) <= {f["id"] for f in explanation["facts"]} for p in explanation["proofs"]))


if __name__ == "__main__":
    unittest.main()
