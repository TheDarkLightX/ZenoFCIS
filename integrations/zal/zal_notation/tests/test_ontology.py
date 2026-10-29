"""Finite consequence, refusal, contradiction and provenance checks."""
import copy
import unittest

from zal_notation.core import NotationError, canonical
from zal_notation.ontology import classify_ontology, ontology_example, replay_ontology, validate_ontology


def ontology(classes=None, properties=None, individuals=None, axioms=None, facts=None):
    return {"schema": "zal.ontology/1", "id": "test", "version": 1,
            "classes": classes or [], "properties": properties or [], "individuals": individuals or [],
            "axioms": axioms or [], "facts": facts or []}


def typed(identifier, individual, class_name):
    return {"id": identifier, "kind": "type", "individual": individual, "class": class_name}


def linked(identifier, subject, prop, obj):
    return {"id": identifier, "kind": "property", "subject": subject, "property": prop, "object": obj}


class OntologyTests(unittest.TestCase):
    def test_shared_structure_does_not_assert_equivalence(self):
        report = classify_ontology(ontology_example())
        self.assertEqual((report["status"], report["complete"], report["consistency"]), ("classified", True, "consistent"))
        for individual in ("trainingRun", "programmingSession"):
            self.assertIn("FeedbackGuidedConstruction", report["types"][individual])
            self.assertNotIn("IntentCriterionReview", report["types"][individual])
        for individual in ("alignmentReview", "requirementsReview"):
            self.assertIn("IntentCriterionReview", report["types"][individual])
        self.assertEqual({fact["kind"] for fact in report["facts"]}, {"type", "property"})
        self.assertEqual(report["authority"], "none")

    def test_subclass_cycles_terminate_with_acyclic_explanations(self):
        doc = ontology(["A", "B", "C"], individuals=["x"], axioms=[
            {"id": "ab", "kind": "subclass", "sub": "A", "super": "B"},
            {"id": "ba", "kind": "subclass", "sub": "B", "super": "A"},
            {"id": "bc", "kind": "subclass", "sub": "B", "super": "C"}], facts=[typed("given", "x", "A")])
        result = classify_ontology(doc)
        self.assertEqual(result["types"]["x"], ["A", "B", "C"])
        proofs = {p["fact"]: p for p in result["proofs"]}
        def visit(fact, path):
            self.assertNotIn(fact, path)
            for premise in proofs[fact]["premises"]:
                visit(premise, path | {fact})
        for fact in proofs:
            visit(fact, set())

    def test_property_inclusion_domain_range_and_classification(self):
        doc = ontology(["Domain", "Filler", "Goal"], ["p", "q"], ["x", "y"], [
            {"id": "pq", "kind": "subproperty", "sub": "p", "super": "q"},
            {"id": "domain", "kind": "domain", "property": "q", "class": "Domain"},
            {"id": "range", "kind": "range", "property": "q", "class": "Filler"},
            {"id": "goal", "kind": "classify", "all": ["Domain"],
             "some": [{"property": "q", "class": "Filler"}], "class": "Goal"}], [linked("edge", "x", "p", "y")])
        result = classify_ontology(doc)
        self.assertEqual(result["types"], {"x": ["Domain", "Goal"], "y": ["Filler"]})
        proof = next(p for p in result["proofs"] if p["rule"] == "classify")
        self.assertEqual(len(proof["premises"]), 3)

    def test_existential_guards_can_have_different_witnesses(self):
        doc = ontology(["A", "B", "Goal"], ["p"], ["x", "y", "z"], [
            {"id": "goal", "kind": "classify", "all": [], "some": [
                {"property": "p", "class": "A"}, {"property": "p", "class": "B"}], "class": "Goal"}],
            [linked("xy", "x", "p", "y"), linked("xz", "x", "p", "z"), typed("ya", "y", "A"), typed("zb", "z", "B")])
        self.assertEqual(classify_ontology(doc)["types"]["x"], ["Goal"])

    def test_no_existential_witness_or_reverse_rule_is_created(self):
        doc = ontology(["Premise", "Filler", "Head"], ["p"], ["x"], [
            {"id": "rule", "kind": "classify", "all": ["Premise"],
             "some": [{"property": "p", "class": "Filler"}], "class": "Head"}], [typed("head", "x", "Head")])
        result = classify_ontology(doc)
        self.assertEqual(result["types"], {"x": ["Head"]})
        self.assertEqual(len(result["facts"]), 1)

    def test_disjoint_witness_retains_both_derivations(self):
        doc = ontology(["A", "B", "C"], individuals=["x"], axioms=[
            {"id": "ab", "kind": "subclass", "sub": "A", "super": "B"},
            {"id": "bc", "kind": "disjoint", "classes": ["B", "C"]}],
            facts=[typed("a", "x", "A"), typed("c", "x", "C")])
        result = classify_ontology(doc)
        self.assertEqual(result["status"], "inconsistent")
        self.assertEqual(result["contradiction"]["classes"], ["B", "C"])
        self.assertFalse(result["complete"])
        proof_ids = {p["fact"] for p in result["proofs"]}
        self.assertTrue(set(result["contradiction"]["premises"]) <= proof_ids)
        replay = replay_ontology(doc, result)
        self.assertTrue(replay["matches_replay"])
        self.assertEqual(replay["classification_status"], "inconsistent")

    def test_empty_disjoint_classes_are_not_a_contradiction(self):
        doc = ontology(["A", "B"], axioms=[{"id": "d", "kind": "disjoint", "classes": ["A", "B"]}])
        self.assertEqual(classify_ontology(doc)["consistency"], "consistent")

    def test_unsupported_is_all_or_nothing(self):
        doc = ontology_example()
        doc["axioms"].append({"id": "unsupported", "kind": "existential-head"})
        result = classify_ontology(doc)
        self.assertEqual(result["status"], "unsupported")
        self.assertEqual(result["facts"], [])
        self.assertEqual(result["usage"]["work"], 0)
        with self.assertRaises(NotationError):
            validate_ontology(doc)

    def test_budgets_never_claim_completed_absence(self):
        doc = ontology_example()
        for limits in ({"max_work": 0}, {"max_facts": 0}, {"max_work": 50}, {"max_facts": 28}):
            with self.subTest(limits=limits):
                result = classify_ontology(doc, **limits)
                self.assertEqual(result["status"], "incomplete")
                self.assertFalse(result["complete"])
                self.assertEqual(result["consistency"], "unknown")
                self.assertLessEqual(result["usage"]["work"], result["limits"]["max_work"])
                self.assertTrue(replay_ontology(doc, result)["matches_replay"])

    def test_long_unsupported_kinds_still_produce_bounded_reports(self):
        for field in ("axioms", "facts"):
            with self.subTest(field=field):
                doc = ontology()
                doc[field].append({"id": "unsupported", "kind": "u" * 16_000})
                result = classify_ontology(doc)
                self.assertEqual(result["status"], "unsupported")
                self.assertEqual(result["facts"], [])
                canonical(result)
                self.assertTrue(replay_ontology(doc, result)["matches_replay"])

    def test_exact_budget_boundary_and_derived_fact_budget(self):
        doc = ontology_example()
        full = classify_ontology(doc)
        self.assertTrue(classify_ontology(doc, max_work=full["usage"]["work"], max_facts=len(full["facts"]))["complete"])
        self.assertFalse(classify_ontology(doc, max_work=full["usage"]["work"] - 1)["complete"])

    def test_boolean_limits_and_versions_refuse(self):
        for field in ("max_facts", "max_work"):
            with self.subTest(field=field), self.assertRaises(NotationError):
                classify_ontology(ontology_example(), **{field: True})
        doc = ontology_example(); doc["version"] = True
        with self.assertRaises(NotationError):
            classify_ontology(doc)

    def test_undeclared_duplicate_and_punning_refuse(self):
        mutations = [lambda x: x["classes"].append(x["classes"][0]),
                     lambda x: x["individuals"].append(x["classes"][0]),
                     lambda x: x["facts"][0].update({"class": "undeclared"}),
                     lambda x: x["facts"].append(dict(x["facts"][0], id="duplicate-body"))]
        for mutate in mutations:
            doc = ontology_example(); mutate(doc)
            with self.subTest(mutate=mutate), self.assertRaises(NotationError):
                classify_ontology(doc)

    def test_replay_compares_premises_and_all_fields(self):
        doc = ontology_example()
        report = classify_ontology(doc)
        for field in ("authority", "checker", "ontology_digest"):
            changed = copy.deepcopy(report); changed[field] = "forged"
            self.assertFalse(replay_ontology(doc, changed)["matches_replay"])
        changed = copy.deepcopy(report)
        next(p for p in changed["proofs"] if p["rule"] == "classify")["premises"] = []
        self.assertFalse(replay_ontology(doc, changed)["matches_replay"])

    def test_input_and_output_aliases_do_not_escape(self):
        doc = ontology_example(); before = copy.deepcopy(doc)
        report = classify_ontology(doc)
        self.assertEqual(doc, before)
        report["types"]["trainingRun"].append("forged")
        report["proofs"][0]["premises"].append("forged")
        self.assertEqual(doc, before)
        self.assertTrue(classify_ontology(doc)["complete"])


if __name__ == "__main__":
    unittest.main()
