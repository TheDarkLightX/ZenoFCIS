"""Explicit record validation for a named, exported SHACL Core subset.

Checks min/max count, IRI node kind, string datatype, length, and digest pattern
on asserted record fields. This is not a general SHACL engine. Conformance
establishes field syntax only; a receipt hash is never replayed or trusted here.
"""
from __future__ import annotations

import re
from copy import deepcopy
from urllib.parse import urlsplit

from . import core
from .ontology_rdf import RDF, XSD, ZAL, _blank, _iri, _literal, _turtle

SH = "http://www.w3.org/ns/shacl#"
PROFILE = "zal/shacl-explicit-evidence-record/1"
FIELDS = ("source", "target", "scope", "checker", "receipt")
HASH_FIELDS = frozenset(("scope", "checker", "receipt"))
DIGEST_PATTERN = "^[0-9a-f]{64}$"
MAX_RECORDS = 256
MAX_VALUES = 16
SCOPE = ("Explicit asserted record fields only; hash syntax is not receipt validity, "
         "ontology classification, behavioral equivalence, or publication authority.")


def _is_iri(value):
    if type(value) is not str or not re.match(r"^[A-Za-z][A-Za-z0-9+.-]*:.+$", value):
        return False
    if any(c.isspace() or ord(c) < 32 or ord(c) == 127 or c in '<>"{}|^`\\' for c in value):
        return False
    if re.search(r"%(?![0-9a-fA-F]{2})", value):
        return False
    try:
        parts = urlsplit(value)
        return bool(parts.scheme) and (parts.scheme not in ("http", "https") or bool(parts.netloc))
    except ValueError:
        return False


def _value_term(field, value):
    if type(value) is str:
        return _iri(value) if field not in HASH_FIELDS and _is_iri(value) else _literal(value)
    if type(value) is bool:
        return _literal("true" if value else "false", XSD + "boolean")
    if type(value) is int:
        return _literal(str(value), XSD + "integer")
    if type(value) is float:
        return _literal(repr(value), XSD + "double")
    raise core.NotationError("record values must be scalar RDF values or a bounded list of them")


def _snapshot(record):
    core.canonical(record)
    core.keys(record, {"id"}, set(FIELDS), "ontology evidence record")
    core.need(_is_iri(record["id"]), "record id must be an absolute IRI in the supported syntax")
    result = deepcopy(record)
    terms = {}
    for field in FIELDS:
        values = result.get(field, [])
        if type(values) is not list:
            values = [values]
        core.need(len(values) <= MAX_VALUES, "record value count exceeds bound")
        # RDF graphs are sets: repeated identical values are one asserted term.
        terms[field] = sorted(set(_value_term(field, value) for value in values))
    return result, terms


def validate_record(record):
    """Validate explicit fields; optional lists express diagnostic RDF multiplicity.

    Normal records use scalar values. Missing fields generate violations;
    malformed envelopes and unrepresentable RDF values raise NotationError.
    Inputs are copied and returned diagnostics contain no caller-owned aliases.
    """
    record, terms = _snapshot(record)
    results = []

    def violation(field, constraint, message):
        results.append({"focus": record["id"], "path": field,
                        "constraint": constraint + "ConstraintComponent", "message": message})

    for field in FIELDS:
        values = terms[field]
        if not values:
            violation(field, "MinCount", "One explicitly asserted value is required.")
        if len(values) > 1:
            violation(field, "MaxCount", "At most one distinct RDF value is allowed.")
        for term in values:
            if field not in HASH_FIELDS:
                if term[0] != "iri":
                    violation(field, "NodeKind", "Expected an absolute IRI node.")
            else:
                if term[0] != "literal" or term[2] != XSD + "string":
                    violation(field, "Datatype", "Expected an xsd:string literal.")
                lexical = term[1]
                if len(lexical) < 64:
                    violation(field, "MinLength", "Digest text must contain 64 characters.")
                if len(lexical) > 64:
                    violation(field, "MaxLength", "Digest text must contain 64 characters.")
                if re.search(DIGEST_PATTERN, lexical) is None:
                    violation(field, "Pattern", "Expected lowercase hexadecimal digest text.")
    results.sort(key=lambda row: (row["path"], row["constraint"], row["message"]))
    report = {"schema": "zal.ontology-record-validation/1", "profile": PROFILE,
              "record": record["id"], "status": "violations" if results else "conforms",
              "conforms": not results, "results": results, "authority": "none",
              "evidence": "unchecked", "scope": SCOPE}
    core.canonical(report)
    return report


def _records(records):
    core.canonical(records)
    core.need(type(records) is list and len(records) <= MAX_RECORDS, "record collection exceeds bound")
    snapshots = [_snapshot(record) for record in records]
    ids = [record["id"] for record, _ in snapshots]
    core.need(len(ids) == len(set(ids)), "duplicate record id")
    return snapshots


def validate_records(records):
    snapshots = _records(records)
    reports = [validate_record(record) for record, _ in snapshots]
    reports.sort(key=lambda row: row["record"])
    report = {"schema": "zal.ontology-record-validations/1", "profile": PROFILE,
              "conforms": all(row["conforms"] for row in reports), "records": reports,
              "authority": "none", "evidence": "unchecked", "scope": SCOPE}
    core.canonical(report)
    return report


def export_records_turtle(records):
    """Export asserted records even when required fields fail validation."""
    triples = []
    for record, terms in _records(records):
        subject = _iri(record["id"])
        triples.append((subject, _iri(RDF + "type"), _iri(ZAL + "EvidenceRecord")))
        for field in FIELDS:
            for value in terms[field]:
                triples.append((subject, _iri(ZAL + field), value))
    return _turtle(triples, "Explicit ZAL evidence metadata; receipt hashes are unverified data.")


def export_shapes_turtle():
    """Exact SHACL Core constraints implemented by validate_record.

    Target records are explicitly typed by export_records_turtle. Additional
    targetSubjectsOf declarations also catch records with metadata but no type.
    This shape requires inference='none' for the explicit-field comparison.
    """
    shape = _iri(ZAL + "EvidenceRecordShape")
    triples = [(shape, _iri(RDF + "type"), _iri(SH + "NodeShape")),
               (shape, _iri(SH + "targetClass"), _iri(ZAL + "EvidenceRecord"))]
    for field in FIELDS:
        prop = _blank("record_" + field)
        triples += [(shape, _iri(SH + "targetSubjectsOf"), _iri(ZAL + field)),
                    (shape, _iri(SH + "property"), prop),
                    (prop, _iri(SH + "path"), _iri(ZAL + field)),
                    (prop, _iri(SH + "minCount"), _literal("1", XSD + "integer")),
                    (prop, _iri(SH + "maxCount"), _literal("1", XSD + "integer"))]
        if field in HASH_FIELDS:
            triples += [(prop, _iri(SH + "datatype"), _iri(XSD + "string")),
                        (prop, _iri(SH + "minLength"), _literal("64", XSD + "integer")),
                        (prop, _iri(SH + "maxLength"), _literal("64", XSD + "integer")),
                        (prop, _iri(SH + "pattern"), _literal(DIGEST_PATTERN))]
        else:
            triples.append((prop, _iri(SH + "nodeKind"), _iri(SH + "IRI")))
    return _turtle(triples, "zal/shacl-explicit-evidence-record/1; asserted fields; no inference or replay.")
