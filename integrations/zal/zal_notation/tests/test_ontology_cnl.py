"""Controlled-English round trips and strict refusal boundaries."""
import unittest

from zal_notation.core import NotationError, digest
from zal_notation.ontology import classify_ontology, ontology_example
from zal_notation.ontology_cnl import parse_ontology_cnl, render_ontology_cnl


class OntologyCNLTests(unittest.TestCase):
    def test_example_round_trip_preserves_metadata_and_identity(self):
        doc = ontology_example()
        text = render_ontology_cnl(doc)
        parsed = parse_ontology_cnl(text)
        self.assertEqual(parsed, doc)
        self.assertEqual(digest(parsed), digest(doc))
        self.assertEqual(classify_ontology(parsed), classify_ontology(doc))
        self.assertEqual(render_ontology_cnl(parsed), text)

    def test_all_admitted_axiom_forms(self):
        text = '''Ontology "forms" version 1.
Class "A".
Class "B".
Class "C".
Property "p".
Property "q".
Individual "x".
Individual "y".
Axiom "ab": every "A" is "B".
Axiom "pq": every link of "p" is a link of "q".
Axiom "domain": subjects of "p" are "A".
Axiom "range": objects of "q" are "B".
Axiom "disjoint": no individual is both "A" and "C".
Axiom "rule": if an individual is "A" and has "q" some "B", then it is "B".
Fact "typed": "y" is "B".
Fact "linked": "x" relates by "p" to "y".
'''
        doc = parse_ontology_cnl(text)
        self.assertEqual(doc, parse_ontology_cnl(render_ontology_cnl(doc)))
        self.assertIn("B", classify_ontology(doc)["types"]["x"])

    def test_metadata_is_inert_and_not_a_status(self):
        doc = ontology_example()
        doc["origin"] = {"status": "checked", "author": "self-declared", "nested": [True, None, {"x": 3}]}
        parsed = parse_ontology_cnl(render_ontology_cnl(doc))
        self.assertEqual(parsed, doc)
        self.assertEqual(classify_ontology(parsed)["authority"], "none")

    def test_quoted_unicode_punctuation_and_line_separators_round_trip(self):
        doc = {"schema": "zal.ontology/1", "id": "Unicode λ", "version": 1,
               "classes": ['some "quoted", class.\u2028'], "properties": [], "individuals": ["and λ"],
               "axioms": [], "facts": [{"id": "assert", "kind": "type", "individual": "and λ", "class": 'some "quoted", class.\u2028'}],
               "description": "line\nnext\u2028paragraph", "origin": {"label": "a\u2029b"}}
        self.assertEqual(parse_ontology_cnl(render_ontology_cnl(doc)), doc)

    def test_comments_and_declaration_order_are_explicit(self):
        doc = parse_ontology_cnl('# Whole-line comment.\nOntology "x" version 1.\nIndividual "i".\nClass "A".\nFact "f": "i" is "A".\n')
        self.assertEqual(doc["facts"][0]["class"], "A")

    def test_no_free_text_unknown_logic_or_implicit_declarations(self):
        prefixes = 'Ontology "x" version 1.\nClass "A".\nIndividual "i".\n'
        statements = ['Usually i is A.', 'Fact "f": "i" is "B".',
                      'Axiom "r": every "A" is some "A".',
                      'Fact "f": "i" is "A". trailing', 'Fact "f": "i" is "A"',
                      'Import "https://example.invalid/ontology".', 'Class unquoted.']
        for statement in statements:
            with self.subTest(statement=statement), self.assertRaises(NotationError):
                parse_ontology_cnl(prefixes + statement)

    def test_duplicate_headers_metadata_and_json_keys_refuse(self):
        for source in ['Ontology "a" version 1.\nOntology "b" version 1.',
                       'Ontology "a" version true.',
                       'Ontology "a" version 1.\nOrigin {"a":1,"a":2}.',
                       'Ontology "a" version 1.\nOrigin {}.\nOrigin {}.',
                       'Ontology "a" version 1.\nDescription "x".\nDescription "y".']:
            with self.subTest(source=source), self.assertRaises(NotationError):
                parse_ontology_cnl(source)

    def test_source_and_statement_budgets_are_refusals(self):
        for source in ['#' * 512001, '\n' * 2049, 'Ontology "' + 'x' * 16000 + '" version 1.']:
            with self.assertRaises(NotationError):
                parse_ontology_cnl(source)


if __name__ == "__main__":
    unittest.main()
