"""RDF export boundaries and explicit-record constraints without optional deps."""
import copy
import json
import unittest

from zal_notation import core
from zal_notation.ontology import classify_ontology
from zal_notation.ontology_rdf import OWL, export_jsonld, export_turtle, symbol_iri
from zal_notation.ontology_records import (
    FIELDS, export_records_turtle, export_shapes_turtle, validate_record, validate_records,
)


def guarded_document():
    return {"schema": "zal.ontology/1", "id": "test/guarded", "version": 1,
            "classes": ["Input", "Left", "Right", "Result"], "properties": ["p", "q"],
            "individuals": ["x", "y", "z"],
            "axioms": [{"id": "guard", "kind": "classify", "all": ["Input"],
                        "some": [{"property": "p", "class": "Left"},
                                 {"property": "q", "class": "Right"}], "class": "Result"}],
            "facts": [{"id": "f1", "kind": "type", "individual": "x", "class": "Input"},
                      {"id": "f2", "kind": "type", "individual": "y", "class": "Left"},
                      {"id": "f3", "kind": "type", "individual": "z", "class": "Right"},
                      {"id": "f4", "kind": "property", "subject": "x", "property": "p", "object": "y"},
                      {"id": "f5", "kind": "property", "subject": "x", "property": "q", "object": "z"}]}


def good_record():
    return {"id": "urn:claim:one", "source": "urn:source:one", "target": "urn:target:one",
            "scope": "a" * 64, "checker": "b" * 64, "receipt": "c" * 64}


class OntologyExportTests(unittest.TestCase):
    def test_one_way_guard_export_and_no_input_mutation(self):
        document = guarded_document()
        before = copy.deepcopy(document)
        turtle = export_turtle(document)
        self.assertEqual(turtle, export_turtle(document))
        self.assertEqual(document, before)
        self.assertIn(OWL + "someValuesFrom", turtle)
        self.assertIn(OWL + "intersectionOf", turtle)
        self.assertNotIn(OWL + "equivalentClass", turtle)
        self.assertNotIn(OWL + "sameAs", turtle)
        self.assertNotIn(OWL + "imports", turtle)
        self.assertIn("Result", classify_ontology(document)["types"]["x"])

    def test_jsonld_is_expanded_and_contains_no_remote_context(self):
        data = json.loads(export_jsonld(guarded_document()))
        self.assertIsInstance(data, list)
        self.assertTrue(data)
        self.assertTrue(all("@id" in row and "@context" not in row for row in data))

    def test_export_is_context_pinned_and_names_are_opaque(self):
        document = guarded_document()
        original = symbol_iri(document, "class", "Result")
        document["origin"] = {"note": "different context"}
        self.assertNotEqual(original, symbol_iri(document, "class", "Result"))
        document = {"schema": "zal.ontology/1", "id": "opaque", "version": 1,
                    "classes": ['owl:Thing <α> "quoted"'], "properties": [],
                    "individuals": [], "axioms": [], "facts": []}
        iri = symbol_iri(document, "class", document["classes"][0])
        self.assertNotEqual(iri, OWL + "Thing")
        self.assertIn("%3C", iri)
        self.assertIn("%22", iri)
        self.assertIn("%CE%B1", iri)

    def test_unsupported_or_oversized_input_refused_before_export(self):
        document = guarded_document()
        document["axioms"].append({"id": "bad", "kind": "sameAs"})
        with self.assertRaises(core.NotationError):
            export_turtle(document)
        document = guarded_document()
        value = []
        for _ in range(152):
            value = [value]
        document["origin"] = value
        with self.assertRaises(core.NotationError):
            export_jsonld(document)

    def test_symbol_lookup_rejects_undeclared_or_wrong_kind(self):
        for kind, name in (("class", "absent"), ([], "Result"), ("class", [])):
            with self.assertRaises(core.NotationError):
                symbol_iri(guarded_document(), kind, name)


class ExplicitRecordTests(unittest.TestCase):
    def test_conformance_is_only_syntax_and_is_pure(self):
        record = good_record()
        before = copy.deepcopy(record)
        report = validate_record(record)
        self.assertTrue(report["conforms"])
        self.assertEqual(report["evidence"], "unchecked")
        self.assertEqual(report["authority"], "none")
        report["results"].append({"mutated": True})
        self.assertEqual(record, before)

    def test_every_required_field_reports_explicit_absence(self):
        for field in FIELDS:
            record = good_record()
            del record[field]
            report = validate_record(record)
            self.assertFalse(report["conforms"])
            self.assertIn((field, "MinCountConstraintComponent"),
                          [(row["path"], row["constraint"]) for row in report["results"]])

    def test_digest_datatype_case_and_trailing_newline(self):
        for value in ("A" * 64, "a" * 63, "a" * 64 + "\n", int("1" * 64), True):
            record = good_record()
            record["receipt"] = value
            self.assertFalse(validate_record(record)["conforms"])
        record["receipt"] = "a" * 64 + "\n"
        self.assertIn("MaxLengthConstraintComponent",
                      [row["constraint"] for row in validate_record(record)["results"]])

    def test_counts_distinct_rdf_terms_and_checks_iri_node_kind(self):
        record = good_record()
        record["source"] = ["urn:a", "urn:a"]
        self.assertTrue(validate_record(record)["conforms"])
        record["source"] = ["urn:a", "urn:b"]
        self.assertEqual([row["constraint"] for row in validate_record(record)["results"]],
                         ["MaxCountConstraintComponent"])
        for value in ("relative", "https://", "urn:bad%xy", "urn:bad>value", False):
            record["source"] = value
            self.assertIn("NodeKindConstraintComponent",
                          [row["constraint"] for row in validate_record(record)["results"]])

    def test_record_envelope_and_bounds_are_not_silently_ignored(self):
        for change in ({"unknown": "hidden"}, {"id": "relative"}, {"receipt": None},
                       {"receipt": {"execute": "no"}}, {"source": ["urn:a"] * 17}):
            record = good_record()
            record.update(change)
            with self.assertRaises(core.NotationError):
                validate_record(record)
        with self.assertRaises(core.NotationError):
            validate_records([good_record(), good_record()])

    def test_export_preserves_incomplete_records_as_explicit_targets(self):
        record = {"id": "urn:claim:empty"}
        self.assertFalse(validate_records([record])["conforms"])
        turtle = export_records_turtle([record])
        self.assertIn("EvidenceRecord>", turtle)
        self.assertEqual(turtle, export_records_turtle([record]))
        shapes = export_shapes_turtle()
        self.assertIn("targetSubjectsOf", shapes)
        self.assertNotIn("owl#imports", shapes)

    def test_rdf_expansion_has_an_independent_byte_budget(self):
        records = []
        for index in range(10):
            record = {"id": "urn:" + "x" * 15000 + str(index),
                      "source": ["urn:s:" + str(i) for i in range(16)],
                      "target": ["urn:t:" + str(i) for i in range(16)]}
            for field in ("scope", "checker", "receipt"):
                record[field] = [format(i, "064x") for i in range(16)]
            records.append(record)
        core.canonical(records)  # Small input; repeated subjects expand in RDF.
        with self.assertRaisesRegex(core.NotationError, "RDF export exceeds"):
            export_records_turtle(records)


if __name__ == "__main__":
    unittest.main()
