"""Exact, bounded controlled-English syntax for the admitted ontology profile.

Names are JSON-quoted opaque symbols. Every declaration occupies one line;
whole-line # comments are ignored. There is no free-text/NLP fallback, implicit
entity declaration, existential head, or interpretation of metadata as logic.
The fixed templates map directly to ontology.py's admitted rule forms.
"""
from __future__ import annotations

import json
import re

from .core import NotationError, canonical, loads, need
from .ontology import SCHEMA, validate_ontology


MAX_SOURCE_BYTES = 512_000
MAX_LINES = 2048
MAX_LINE_BYTES = 16_000
MAX_TOKENS = 512
_WORD = re.compile(r"[A-Za-z_][A-Za-z0-9_-]*")
_DECIMAL = re.compile(r"[0-9]{1,7}(?![0-9])")


def _tokens(line):
    tokens, position = [], 0
    decoder = json.JSONDecoder()
    while position < len(line):
        need(len(tokens) < MAX_TOKENS, "controlled-English token budget exceeded")
        if line[position].isspace():
            position += 1
            continue
        char = line[position]
        if char == '"':
            try:
                value, end = decoder.raw_decode(line, position)
            except (ValueError, RecursionError) as error:
                raise NotationError("invalid JSON-quoted name or text") from error
            need(type(value) is str, "expected a quoted string")
            tokens.append(("string", value))
            position = end
        elif char in ".,:":
            tokens.append(("literal", char))
            position += 1
        else:
            found = _WORD.match(line, position)
            if found:
                tokens.append(("literal", found[0]))
            else:
                found = _DECIMAL.match(line, position)
                need(found is not None, "unsupported controlled-English token")
                tokens.append(("integer", int(found[0])))
            position = found.end()
    return tokens


class _Line:
    def __init__(self, line):
        self.tokens = _tokens(line)
        self.index = 0

    def peek(self, literal):
        return self.index < len(self.tokens) and self.tokens[self.index] == ("literal", literal)

    def take(self, *literals):
        for literal in literals:
            need(self.peek(literal), f"expected {literal!r}")
            self.index += 1

    def value(self, kind="string"):
        need(self.index < len(self.tokens) and self.tokens[self.index][0] == kind, f"expected {kind}")
        value = self.tokens[self.index][1]
        self.index += 1
        return value

    def end(self):
        self.take(".")
        need(self.index == len(self.tokens), "unexpected text after statement terminator")


def _axiom(line):
    line.take("Axiom")
    result = {"id": line.value()}
    line.take(":")
    if line.peek("every"):
        line.take("every")
        if line.peek("link"):
            line.take("link", "of")
            result.update(kind="subproperty", sub=line.value())
            line.take("is", "a", "link", "of")
        else:
            result.update(kind="subclass", sub=line.value())
            line.take("is")
        result["super"] = line.value()
    elif line.peek("subjects") or line.peek("objects"):
        subjects = line.peek("subjects")
        line.take("subjects" if subjects else "objects", "of")
        result.update(kind="domain" if subjects else "range", property=line.value())
        line.take("are")
        result["class"] = line.value()
    elif line.peek("no"):
        line.take("no", "individual", "is", "both")
        first = line.value()
        line.take("and")
        result.update(kind="disjoint", classes=[first, line.value()])
    else:
        line.take("if", "an", "individual")
        result.update(kind="classify", all=[], some=[])
        while True:
            if line.peek("is"):
                line.take("is")
                result["all"].append(line.value())
            else:
                line.take("has")
                prop = line.value()
                line.take("some")
                result["some"].append({"property": prop, "class": line.value()})
            if not line.peek("and"):
                break
            line.take("and")
        line.take(",", "then", "it", "is")
        result["class"] = line.value()
    line.end()
    return result


def _fact(line):
    line.take("Fact")
    result = {"id": line.value()}
    line.take(":")
    subject = line.value()
    if line.peek("is"):
        line.take("is")
        result.update(kind="type", individual=subject)
        result["class"] = line.value()
    else:
        line.take("relates", "by")
        result.update(kind="property", subject=subject, property=line.value())
        line.take("to")
        result["object"] = line.value()
    line.end()
    return result


def parse_ontology_cnl(source: str) -> dict:
    """Parse all statements and validate their common context; never guess a reading."""
    need(type(source) is str, "controlled-English source must be text")
    try:
        need(len(source.encode("utf-8")) <= MAX_SOURCE_BYTES, "controlled-English source byte budget exceeded")
    except UnicodeError as error:
        raise NotationError("invalid controlled-English Unicode") from error
    # LF is the statement separator; Unicode line-separator characters inside
    # JSON strings remain string data rather than silently splitting a name.
    lines = source.split("\n")
    need(len(lines) <= MAX_LINES, "controlled-English line budget exceeded")
    document = {"schema": SCHEMA, "classes": [], "properties": [], "individuals": [], "axioms": [], "facts": []}
    for number, raw in enumerate(lines, 1):
        line_text = raw.strip()
        if not line_text or line_text.startswith("#"):
            continue
        try:
            need(len(raw.encode("utf-8")) <= MAX_LINE_BYTES, "controlled-English statement byte budget exceeded")
            # Metadata is opaque bounded JSON. Strict loads rejects duplicate
            # object keys and nonfinite values before the document is admitted.
            origin = re.fullmatch(r"Origin\s+(.+)\.", line_text)
            if origin:
                need("origin" not in document, "duplicate Origin statement")
                document["origin"] = loads('{"value":' + origin[1] + '}')["value"]
                continue
            line = _Line(line_text)
            if line.peek("Ontology"):
                need("id" not in document, "duplicate Ontology statement")
                line.take("Ontology")
                document["id"] = line.value()
                line.take("version")
                document["version"] = line.value("integer")
                line.end()
            elif line.peek("Description"):
                need("description" not in document, "duplicate Description statement")
                line.take("Description")
                document["description"] = line.value()
                line.end()
            elif any(line.peek(word) for word in ("Class", "Property", "Individual")):
                word = line.tokens[0][1]
                line.take(word)
                key = {"Class": "classes", "Property": "properties", "Individual": "individuals"}[word]
                document[key].append(line.value())
                line.end()
            elif line.peek("Axiom"):
                document["axioms"].append(_axiom(line))
            elif line.peek("Fact"):
                document["facts"].append(_fact(line))
            else:
                raise NotationError("unsupported controlled-English statement; no NLP fallback")
        except (NotationError, UnicodeError) as error:
            raise NotationError(f"controlled-English line {number}: {error}") from error
    need("id" in document, "missing Ontology statement")
    validate_ontology(document)
    return document


def render_ontology_cnl(document: dict) -> str:
    """Render exact admitted declarations and inert metadata, without information loss."""
    validate_ontology(document)
    quote = lambda value: json.dumps(value, ensure_ascii=True)
    lines = [f"Ontology {quote(document['id'])} version {document['version']}."]
    if "description" in document:
        lines.append(f"Description {quote(document['description'])}.")
    if "origin" in document:
        lines.append("Origin " + json.dumps(document["origin"], ensure_ascii=True,
                                             sort_keys=True, separators=(",", ":")) + ".")
    for key, word in (("classes", "Class"), ("properties", "Property"), ("individuals", "Individual")):
        lines.extend(f"{word} {quote(value)}." for value in document[key])
    for axiom in document["axioms"]:
        kind = axiom["kind"]
        if kind == "subclass":
            body = f"every {quote(axiom['sub'])} is {quote(axiom['super'])}"
        elif kind == "subproperty":
            body = f"every link of {quote(axiom['sub'])} is a link of {quote(axiom['super'])}"
        elif kind in {"domain", "range"}:
            body = f"{'subjects' if kind == 'domain' else 'objects'} of {quote(axiom['property'])} are {quote(axiom['class'])}"
        elif kind == "disjoint":
            body = f"no individual is both {quote(axiom['classes'][0])} and {quote(axiom['classes'][1])}"
        else:
            guards = [f"is {quote(cls)}" for cls in axiom["all"]]
            guards.extend(f"has {quote(g['property'])} some {quote(g['class'])}" for g in axiom["some"])
            body = "if an individual " + " and ".join(guards) + f", then it is {quote(axiom['class'])}"
        lines.append(f"Axiom {quote(axiom['id'])}: {body}.")
    for fact in document["facts"]:
        body = (f"{quote(fact['individual'])} is {quote(fact['class'])}" if fact["kind"] == "type" else
                f"{quote(fact['subject'])} relates by {quote(fact['property'])} to {quote(fact['object'])}")
        lines.append(f"Fact {quote(fact['id'])}: {body}.")
    result = "\n".join(lines) + "\n"
    need(len(result.encode("utf-8")) <= MAX_SOURCE_BYTES and len(lines) <= MAX_LINES
         and all(len(line.encode("utf-8")) <= MAX_LINE_BYTES for line in lines),
         "controlled-English rendering exceeds its source/statement budget")
    return result
