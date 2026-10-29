"""One-way RDF export for the admitted finite ontology fragment.

Opaque ZAL names become private, exact-document-pinned IRIs. They are not
external vocabulary identities. No RDF import, remote context, OWL imports,
equality, fresh individuals, or evidence promotion is performed here.
"""
from __future__ import annotations

import json
from copy import deepcopy
from urllib.parse import quote

from . import core

RDF = "http://www.w3.org/1999/02/22-rdf-syntax-ns#"
RDFS = "http://www.w3.org/2000/01/rdf-schema#"
OWL = "http://www.w3.org/2002/07/owl#"
XSD = "http://www.w3.org/2001/XMLSchema#"
ZAL = "urn:zal:ontology:record:"
MAX_EXPORT_BYTES = 2 * 1024 * 1024


def _iri(value):
    return ("iri", value)


def _blank(value):
    return ("blank", value)


def _literal(value, datatype=XSD + "string"):
    return ("literal", value, datatype)


def _term_text(term):
    if term[0] == "iri":
        return "<" + term[1] + ">"
    if term[0] == "blank":
        return "_:" + term[1]
    return json.dumps(term[1], ensure_ascii=False) + "^^<" + term[2] + ">"


def _bounded_text(chunks):
    """Bound serialization expansion before accumulating a large output."""
    pieces, size = [], 0
    for chunk in chunks:
        size += len(chunk.encode("utf-8"))
        core.need(size <= MAX_EXPORT_BYTES, "RDF export exceeds the 2 MiB byte budget")
        pieces.append(chunk)
    return "".join(pieces)


def _turtle(triples, comment):
    def chunks():
        yield "# " + comment + "\n"
        rows = sorted(set(triples))
        if not rows:
            yield "\n"
        for row in rows:
            yield " ".join(_term_text(term) for term in row) + " .\n"
    return _bounded_text(chunks())


def _jsonld(triples):
    """Expanded JSON-LD has no context to retrieve or reinterpret."""
    records = {}
    for subject, predicate, obj in sorted(set(triples)):
        sid = ("_:" if subject[0] == "blank" else "") + subject[1]
        record = records.setdefault(sid, {"@id": sid})
        if obj[0] == "literal":
            value = {"@value": obj[1], "@type": obj[2]}
        else:
            value = {"@id": ("_:" if obj[0] == "blank" else "") + obj[1]}
        record.setdefault(predicate[1], []).append(value)
    encoder = json.JSONEncoder(sort_keys=True, ensure_ascii=False, indent=2)
    def chunks():
        yield from encoder.iterencode([records[key] for key in sorted(records)])
        yield "\n"
    return _bounded_text(chunks())


def _symbol(namespace, kind, name):
    return namespace + kind + ":" + quote(name, safe="")


def symbol_iri(document, kind, name):
    """Return the export IRI for a declared class, property, or individual.

    Any document change deliberately changes this namespace. Export identity
    therefore binds the exact context and does not assert cross-version identity.
    """
    from .ontology import validate_ontology
    validate_ontology(document)
    fields = {"class": "classes", "property": "properties", "individual": "individuals"}
    core.need(type(kind) is str and kind in fields and type(name) is str
              and name in document[fields[kind]], "undeclared ontology symbol")
    return _symbol("urn:zal:ontology:" + core.digest(document) + ":", kind, name)


def _ontology_triples(document):
    from .ontology import validate_ontology
    validate_ontology(document)
    document = deepcopy(document)
    namespace = "urn:zal:ontology:" + core.digest(document) + ":"
    triples = []

    def add(subject, predicate, obj):
        triples.append((subject, _iri(predicate), obj))

    def symbol(kind, value):
        return _iri(_symbol(namespace, kind, value))

    add(_iri(namespace + "ontology"), RDF + "type", _iri(OWL + "Ontology"))
    for kind, field, declaration in (("class", "classes", "Class"),
                                     ("property", "properties", "ObjectProperty"),
                                     ("individual", "individuals", "NamedIndividual")):
        for value in document[field]:
            add(symbol(kind, value), RDF + "type", _iri(OWL + declaration))
            add(symbol(kind, value), RDFS + "label", _literal(value))

    for axiom in document["axioms"]:
        kind = axiom["kind"]
        if kind == "subclass":
            add(symbol("class", axiom["sub"]), RDFS + "subClassOf", symbol("class", axiom["super"]))
        elif kind == "subproperty":
            add(symbol("property", axiom["sub"]), RDFS + "subPropertyOf", symbol("property", axiom["super"]))
        elif kind in ("domain", "range"):
            add(symbol("property", axiom["property"]), RDFS + kind, symbol("class", axiom["class"]))
        elif kind == "disjoint":
            left, right = axiom["classes"]
            add(symbol("class", left), OWL + "disjointWith", symbol("class", right))
        elif kind == "classify":
            # Each existential guard has its own witness variable. The left
            # side is only a sufficient condition, never an equivalence.
            prefix = "a" + core.digest(axiom)
            guards = [symbol("class", value) for value in axiom["all"]]
            for index, guard in enumerate(axiom["some"]):
                restriction = _blank(prefix + "r" + str(index))
                add(restriction, RDF + "type", _iri(OWL + "Restriction"))
                add(restriction, OWL + "onProperty", symbol("property", guard["property"]))
                add(restriction, OWL + "someValuesFrom", symbol("class", guard["class"]))
                guards.append(restriction)
            if len(guards) == 1:
                left = guards[0]
            else:
                left = _blank(prefix + "intersection")
                add(left, RDF + "type", _iri(OWL + "Class"))
                add(left, OWL + "intersectionOf", _blank(prefix + "list0"))
                for index, guard in enumerate(guards):
                    cell = _blank(prefix + "list" + str(index))
                    add(cell, RDF + "first", guard)
                    tail = (_blank(prefix + "list" + str(index + 1))
                            if index + 1 < len(guards) else _iri(RDF + "nil"))
                    add(cell, RDF + "rest", tail)
            add(left, RDFS + "subClassOf", symbol("class", axiom["class"]))
        else:
            # Defensive fail-closed guard if admission ever gains new kinds.
            raise core.NotationError("ontology RDF export does not support axiom " + kind)

    for fact in document["facts"]:
        if fact["kind"] == "type":
            add(symbol("individual", fact["individual"]), RDF + "type", symbol("class", fact["class"]))
        elif fact["kind"] == "property":
            add(symbol("individual", fact["subject"]), _symbol(namespace, "property", fact["property"]),
                symbol("individual", fact["object"]))
        else:
            raise core.NotationError("ontology RDF export does not support fact " + fact["kind"])
    return triples


def export_turtle(document):
    """Export asserted data/axioms, never a claimed complete inferred closure."""
    return _turtle(_ontology_triples(document),
                   "ZAL finite ontology assertions; OWL 2 RL subset; no evidence authority.")


def export_jsonld(document):
    """Export exactly the same RDF graph as export_turtle, without remote contexts."""
    return _jsonld(_ontology_triples(document))
