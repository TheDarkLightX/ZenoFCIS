"""Reusable concepts with exact definitions; no automatic ontology entailment."""
from __future__ import annotations

import re
from copy import deepcopy

from .core import (
    NotationError as NotationError,  # noqa: PLC0414 - preserve the research module attribute
)
from .core import canonical, digest, keys, name, need
from .sharing import anchors, make_context, make_discussion, validate_discussion

RELATIONS = ("uses", "related", "specializes", "analogy", "equivalence", "refinement")


def text(value, label, maximum=8000, empty=False):
    need(type(value) is str and (empty or bool(value.strip())) and len(value) <= maximum,
         f"{label}: expected bounded text")


def make_entry(identifier, title, *, story, discussion=None, requirements=None,
               open_questions=None, aliases=None, tags=None, laws=None, examples=None,
               dependencies=None, purpose=""):
    entry = {"schema": "zal.abstraction/1", "id": identifier, "title": title, "purpose": purpose,
             "story": story, "discussion": deepcopy(discussion), "requirements": deepcopy(requirements or []),
             "open_questions": deepcopy(open_questions or []), "aliases": list(aliases or []),
             "tags": list(tags or []), "laws": deepcopy(laws or []), "examples": list(examples or []),
             "dependencies": deepcopy(dependencies or [])}
    validate_entry(entry)
    return entry


def validate_entry(entry):
    canonical(entry)
    keys(entry, {"schema", "id", "title", "purpose", "story", "discussion", "requirements",
                 "open_questions", "aliases", "tags", "laws", "examples", "dependencies"}, where="abstraction")
    need(entry["schema"] == "zal.abstraction/1", "unsupported abstraction schema")
    name(entry["id"], "abstraction id")
    text(entry["title"], "title", 200)
    text(entry["purpose"], "purpose", empty=True)
    text(entry["story"], "story")
    valid = set()
    if entry["discussion"] is not None:
        validate_discussion(entry["discussion"])
        valid = set(anchors(entry["discussion"]))
    for field in ("aliases", "tags", "examples", "open_questions"):
        values = entry[field]
        need(type(values) is list and len(values) <= 64, f"{field}: list budget exceeded")
        for value in values:
            text(value, field, 200 if field in ("aliases", "tags") else 8000)
        need(len(set(values)) == len(values), f"duplicate {field}")
    for field in ("requirements", "laws"):
        rows = entry[field]
        need(type(rows) is list and len(rows) <= 64, f"{field}: list budget exceeded")
        seen = set()
        for row in rows:
            keys(row, {"id", "statement", "anchors"}, where=field)
            name(row["id"])
            need(row["id"] not in seen, f"duplicate {field} id")
            seen.add(row["id"])
            text(row["statement"], "statement")
            refs = row["anchors"]
            need(type(refs) is list and len(refs) <= 64 and all(type(r) is str for r in refs), "invalid model anchors")
            need(len(set(refs)) == len(refs) and set(refs) <= valid, "unknown or duplicate model anchor")
    deps = entry["dependencies"]
    need(type(deps) is list and len(deps) <= 32, "dependency budget exceeded")
    for dep in deps:
        keys(dep, {"entry_digest", "context_digest", "operation"}, where="dependency")
        for field in ("entry_digest", "context_digest"):
            need(type(dep[field]) is str and re.fullmatch(r"[0-9a-f]{64}", dep[field]) is not None, "invalid dependency identity")
        name(dep["operation"])
    return entry


def inspect_entry(entry):
    validate_entry(entry)
    discussion = entry["discussion"]
    unresolved = list(entry["open_questions"])
    unresolved += ["Unlinked requirement: " + r["id"] for r in entry["requirements"] if not r["anchors"]]
    if discussion is None:
        unresolved.append("No formal interpretation has been supplied.")
    return {"entry_digest": digest(entry), "context_digest": discussion["context_digest"] if discussion else None,
            "notation_digest": digest(discussion["notation"]) if discussion else None,
            "status": "draft" if unresolved else "model-linked", "open_obligations": unresolved,
            "law_status": "declared; use a registered checker for a specified claim",
            "identity_scope": "exact content, not mathematical equivalence", "authority": "none"}


def require_formal(entry):
    report = inspect_entry(entry)
    need(report["status"] == "model-linked", "unresolved abstraction cannot be lowered: " + "; ".join(report["open_obligations"]))
    return entry["discussion"]


def new_library(label="My abstraction library"):
    result = {"schema": "zal.abstraction-library/1", "label": label, "entries": [], "relations": []}
    validate_library(result)
    return result


def validate_library(library):
    canonical(library)
    keys(library, {"schema", "label", "entries", "relations"}, where="library")
    need(library["schema"] == "zal.abstraction-library/1", "unsupported library schema")
    text(library["label"], "library label", 200)
    need(type(library["entries"]) is list and len(library["entries"]) <= 64, "library entry budget exceeded")
    identities = set()
    for entry in library["entries"]:
        validate_entry(entry)
        identity = digest(entry)
        need(identity not in identities, "duplicate library entry")
        identities.add(identity)
    need(type(library["relations"]) is list and len(library["relations"]) <= 256, "relationship budget exceeded")
    for relation in library["relations"]:
        keys(relation, {"source", "target", "kind", "statement", "assumptions", "status"}, where="relationship")
        need(type(relation["source"]) is str and type(relation["target"]) is str and
             relation["source"] in identities and relation["target"] in identities, "relationship endpoint absent")
        need(type(relation["kind"]) is str and relation["kind"] in RELATIONS, "unknown relationship kind")
        text(relation["statement"], "relationship statement")
        need(type(relation["assumptions"]) is list and len(relation["assumptions"]) <= 64, "invalid relationship assumptions")
        for assumption in relation["assumptions"]:
            text(assumption, "relationship assumption")
        need(relation["status"] == "declared", "a library edge does not establish a proof")
    return library


def register(library, entry):
    validate_library(library)
    validate_entry(entry)
    result = deepcopy(library)
    result["entries"].append(deepcopy(entry))
    validate_library(result)
    return result


def relate(library, source, target, kind, statement, assumptions=None):
    validate_library(library)
    result = deepcopy(library)
    result["relations"].append({"source": source, "target": target, "kind": kind, "statement": statement,
                                "assumptions": list(assumptions or []), "status": "declared"})
    validate_library(result)
    return result


def search(library, query):
    validate_library(library)
    text(query, "query", 500)
    terms = set(re.findall(r"\w+", query.casefold()))
    matches = []
    for entry in library["entries"]:
        surface = " ".join([entry["title"], entry["purpose"], entry["story"], *entry["aliases"], *entry["tags"], *entry["examples"]])
        found = sorted(terms & set(re.findall(r"\w+", surface.casefold())))
        if found:
            matches.append({"entry_digest": digest(entry), "title": entry["title"], "matched_terms": found})
    matches.sort(key=lambda row: (-len(row["matched_terms"]), row["title"], row["entry_digest"]))
    return {"scope": "lexical retrieval; not semantic inference", "matches": matches}


def enumerate_lenses(library, entry_digest, registry=None):
    """Discover registered viewpoints on one exact entry, without adopting it."""
    from .lens_library import discover
    return discover(library, entry_digest, registry)


def select(library, identities):
    validate_library(library)
    need(type(identities) is list and all(type(x) is str for x in identities), "identities must be a list")
    available = {digest(e): e for e in library["entries"]}
    need(len(set(identities)) == len(identities) and set(identities) <= available.keys(), "unknown or duplicate selected entry")
    result = new_library(library["label"])
    result["entries"] = [deepcopy(available[k]) for k in identities]
    result["relations"] = [deepcopy(r) for r in library["relations"] if r["source"] in identities and r["target"] in identities]
    validate_library(result)
    return result


def library_diagram(library):
    validate_library(library)
    need(bool(library["entries"]), "empty library has no concept graph")
    from .builtin import generic_pack, model
    from .render import render_svg
    ports = {"in": {"direction": "in", "sort": "Concept"}, "out": {"direction": "out", "sort": "Concept"}}
    grammar = {"schema": "zal.grammar/1", "id": "zal/abstraction-links", "version": 1, "kind": "graph",
               "nodes": {"concept": {"fields": {"name": "str"}, "ports": ports}},
               "edges": {kind: {"fields": {"note": "str"}, "sort": "Concept"} for kind in RELATIONS}}
    body = {"nodes": [{"id": digest(e), "tag": "concept", "fields": {"name": e["title"]}} for e in library["entries"]],
            "edges": [{"id": "relation_" + str(i), "tag": r["kind"], "source": [r["source"], "out"],
                       "target": [r["target"], "in"], "fields": {"note": r["kind"] + " (declared)"}}
                      for i, r in enumerate(library["relations"])]}
    graph = model(grammar, body)
    return {"svg": render_svg(grammar, graph, generic_pack(grammar)),
            "nodes": [{"entry_digest": digest(e), "title": e["title"], "laws": deepcopy(e["laws"]),
                       "assumptions": deepcopy(e["discussion"]["context"]["assumptions"]) if e["discussion"] else [],
                       "open_questions": deepcopy(e["open_questions"])} for e in library["entries"]],
            "relations": deepcopy(library["relations"]), "status": "declared-relationships"}


def compose_lambda(first, second, identifier="composition"):
    """Construct x -> second(first(x)); supported only for closed lambda terms."""
    from .builtin import get_builtin, lambda_grammar, model, term
    from .semantics import _to_db
    left, right = require_formal(first), require_formal(second)
    grammar, notation, _ = get_builtin("lambda")
    for bundle in (left, right):
        context = bundle["context"]
        need(context["grammar"] == lambda_grammar() and context["semantics"]["profile"] == "zal/lambda-normal-order/1",
             "composition requires the registered lambda profile")
        _to_db(context["model"]["body"])  # Refuses unbound variables before introducing a binder.
    body = term("Lam", term("App", deepcopy(right["context"]["model"]["body"]),
                           term("App", deepcopy(left["context"]["model"]["body"]), term("Var", name="x"))), name="x")
    context = make_context(grammar, model(grammar, body), semantics={"profile": "zal/lambda-normal-order/1", "parameters": {},
        "description": "Closed lambda composition: apply the first term, then the second."},
        assumptions=list(dict.fromkeys(left["context"]["assumptions"] + right["context"]["assumptions"])),
        observation={"id": "whole-term", "description": "Retain the complete lambda term and its reduction semantics.", "keeps": ["term"], "omits": []})
    return make_entry(identifier, "Compose " + first["title"][:80] + " then " + second["title"][:80],
        story="Pass the value through the first transformation, then pass its result through the second.",
        discussion=make_discussion(context, notation), dependencies=[
            {"entry_digest": digest(e), "context_digest": e["discussion"]["context_digest"], "operation": "lambda-composition"}
            for e in (first, second)])
