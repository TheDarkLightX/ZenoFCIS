"""Construct registry, zal/help/2 guide and explanations for finite types, typed machines and effects.

Each construct's meaning is written once, here, beside its two forms; the drift test
fills every form and binds it to the live parsers. Explanations are computed from the
checked model only. No language model.
"""

from __future__ import annotations

import hashlib
import re

import adt
import effects
import typed2
from adt import Ty, card_data
from behavior import Refusal, canonical
from constructs import GUIDE_SCHEMA, Construct


TYPE_CONSTRUCTS = (
    Construct("alias", "type", "Type alias",
              "Names another type; the alias has exactly the values of the type it names and adds no level of nesting.",
              "type NAME = TYPE;", "The type NAME is TYPE."),
    Construct("record", "type", "Record type (product)",
              "A record holds one value for every field, in order, so its count of values is the product of its fields' "
              "counts. Field names are distinct; an empty record has exactly one value.",
              "type NAME = {FIELD: TYPE, FIELD: TYPE};", "The type NAME is a record with fields FIELD (TYPE), FIELD (TYPE).",
              aliases=("product", "records")),
    Construct("variant", "type", "Variant type (sum)",
              "A variant holds exactly one of its cases, with that case's payload if it has one, so its count of values is "
              "the sum of its cases' counts. Case names are distinct tags; a variant with no payloads is an enumeration.",
              "type NAME = CASE | CASE(TYPE);", "The type NAME is one of CASE, CASE (TYPE).",
              aliases=("sum", "variants", "enumeration")),
    Construct("unit", "type", "Unit", "Exactly one value, written (). A case without a payload carries it.",
              "unit", "nothing"),
    Construct("bool", "type", "Boolean", "Two values, false and true.", "bool", "a Boolean", aliases=("boolean",)),
    Construct("int", "type", "Bounded integer",
              "Every integer from LOW to HIGH inclusive: HIGH - LOW + 1 values. Ranges never wrap around.",
              "int LOW..=HIGH", "an integer from LOW to HIGH inclusive", aliases=("integer",)),
    Construct("bytes_max", "type", "Byte-capped bytes",
              "Every byte string of 0 through CAP bytes, at most 1048576: 1 + 256 + ... + 256^CAP values.",
              "bytes ..=CAP", "at most CAP bytes"),
    Construct("bytes", "type", "Exact bytes", "Every byte string of exactly LENGTH bytes: 256^LENGTH values.",
              "bytes LENGTH", "exactly LENGTH bytes"),
    Construct("list", "type", "Bounded list",
              "Every list of 0 through CAP items of one type: 1 + n + ... + n^CAP values for n item values.",
              "list ..=CAP of TYPE", "a list of at most CAP items of TYPE"),
    Construct("recursion", "unsupported", "Recursive types",
              "A type that refers to itself, directly or through other types, has no finite count of values; it is refused "
              "by name with its cycle.", "type NAME = CASE | CASE(NAME);", "The type NAME is one of CASE, CASE (NAME)."),
)

TYPED_CONSTRUCTS = (
    Construct("typed_header", "statement", "Typed machine version",
              "Selects profile 2 with finite types: the source elaborates exactly to a zal/finite-fsm/2 machine.",
              "zal typed 1;", "ZAL typed version 1."),
    Construct("typed_var", "statement", "Typed variable",
              "A stored variable of a finite type. An integer type keeps its range; any other type is stored as the index of "
              "its value, and its enumeration must fit profile 2's range of 64 values.",
              "var NAME: TYPE initially VALUE;", "The variable NAME holds TYPE, initially VALUE."),
    Construct("typed_input", "statement", "Typed input",
              "A fresh observation of a finite type on every step; naming it does not authenticate it.",
              "input NAME: TYPE;", "The input NAME holds TYPE."),
    Construct("typed_compare", "operator", "Typed comparison",
              "Compares a typed name with one of its type's values, or with a name of the same type. Typed names have no "
              "ordering and no arithmetic; an update only assigns such a value.",
              "NAME == VALUE", "NAME equals VALUE"),
)

EFFECT_CONSTRUCTS = (
    Construct("header", "statement", "Effects language version",
              "Selects the effect-signature grammar and checks of zal/effects/1.", "zal effects 1;", "ZAL effects version 1."),
    Construct("name", "statement", "Signature name",
              "Names this set of signatures. It is part of the source revision, not of the lowered record.",
              "effects NAME;", "The effect signature is NAME."),
    Construct("schema", "statement", "Record schema",
              "The record this source lowers to; zal/effects/1 lowers only to zeno-fcis/shell-effects/1.",
              "schema SCHEMA;", "It lowers to the record schema SCHEMA."),
    Construct("resource", "statement", "Resource",
              "A logical resource a port may read or write, with its one-bit value in the record's footprint bytes.",
              "resource NAME = BIT;", "Resource NAME has bit value BIT."),
    Construct("capability", "statement", "Capability",
              "The authority a program declares to request a port, with the largest footprint it allows. The ports of a "
              "store capability must map to V2 calls.",
              "capability CAPABILITY = NUMBER: store, reads {NAME}, writes {NAME};",
              "Capability CAPABILITY, number NUMBER, is a store capability that allows reading NAME and writing NAME."),
    Construct("answer", "statement", "Answer shape",
              "A result shape: a variant whose one case T carries the operation's payload. NUMBER is the shape's tag in the "
              "record.", "answer ANSWER = NUMBER: CASE(T) | CASE(TYPE);",
              "Answer ANSWER, number NUMBER, is one of CASE (the result), CASE (TYPE).", aliases=("outcome",)),
    Construct("frame", "statement", "Answer frame",
              "What each case of an answer says about the effect: applied, unchanged or either. The check requires every case "
              "exactly once. Frames are checked, not lowered.",
              "frame ANSWER: CASE applied, CASE unchanged;",
              "For answer ANSWER, CASE means the operation took effect, CASE means nothing changed.",
              aliases=("frames", "exhaustiveness")),
    Construct("ports", "statement", "Port list",
              "The declared ports with their tags. Every port needs one operation and one footprint.",
              "ports PORT = TAG, PORT = TAG;", "The ports are PORT (TAG), PORT (TAG).", aliases=("port",)),
    Construct("type", "statement", "Type declaration",
              "Names a finite type: an alias, a record or a variant. Store is built in: a 32-byte store identity and a "
              "32-bit handle number.", "type NAME = TYPE;", "The type NAME is TYPE.",
              aliases=("types",)),
    Construct("law", "statement", "Law",
              "A handler law with its ID, name and statement. Operations name the laws their handlers must satisfy; the "
              "statement is lowered as written and is not checked here.",
              'law ID LAW: "STATEMENT";', 'Law ID LAW: "STATEMENT".', aliases=("laws",)),
    Construct("operation", "statement", "Operation signature",
              "A port's signature: its argument record, its answer shape and the payload that shape carries.",
              "operation PORT(FIELD: TYPE) -> ANSWER TYPE;", "Operation PORT takes FIELD (TYPE) and answers ANSWER of TYPE.",
              aliases=("operations", "signature")),
    Construct("footprint", "statement", "Operation footprint",
              "What a port may read and write, the capability it needs, its laws and its default V2 mapping. The footprint "
              "must stay within the capability.",
              "footprint PORT: capability CAPABILITY, reads {NAME}, writes {NAME}, laws [ID], v2 calls [CALL];",
              "Operation PORT needs capability CAPABILITY, reads NAME, writes NAME, obeys law ID, and maps to the V2 call CALL.",
              aliases=("footprints",)),
    Construct("store", "type", "Store",
              "Built in: a store handle, a 32-byte store identity and a 32-bit handle number; 2^288 values. Its values "
              "cannot be written in a source.", "Store", "Store"),
)

REGISTRY = (*EFFECT_CONSTRUCTS, *TYPE_CONSTRUCTS, *TYPED_CONSTRUCTS)
BY_ID = {c.id: c for c in REGISTRY}
ALIASES = {alias: c.id for c in REGISTRY for alias in c.aliases}


def legend(model):
    """Every canonical statement in both forms, with its concept."""
    if isinstance(model, effects.Signatures):
        pairs = zip(effects.concept_lines(model), effects.concept_lines(model, True))
        entries = [{"concept": c, "symbolic": s, "english": e} for (c, s), (_, e) in pairs]
        profile = effects.PROFILE
    else:
        entries = [{"concept": _typed_concept(s), "symbolic": s, "english": e}
                   for s, e in zip(model.render().splitlines(), model.render(True).splitlines())]
        profile = typed2.PROFILE
    return {"profile": profile, "entries": entries, "reserved": sorted(effects.RESERVED | adt.KEYWORDS),
            "identifiers": "lowercase names for fields, cases and resources; capitalised names for types, ports, "
                           "capabilities and answers; at most 32 characters"}


def _typed_concept(line):
    for key, pattern in typed2.typed_patterns().items():
        if re.fullmatch(pattern, line):
            return key
    return "typed_header" if line == typed2.HEADER[False] else "profile-2"


def _objects(model):
    if isinstance(model, effects.Signatures):
        return {**{"port:" + p.name: "operation" for p in model.ports},
                **{"type:" + d.name: "type" for d in model.types.decls}, "type:Store": "store",
                **{"law:" + str(law.id): "law" for law in model.laws},
                **{"capability:" + c.name: "capability" for c in model.capabilities},
                **{"resource:" + r.name: "resource" for r in model.resources},
                **{"answer:" + a.name: "answer" for a in model.answers},
                **{"frame:" + f.answer: "frame" for f in model.frames}}
    return {**{"type:" + d.name: "type" for d in model.types.decls},
            **{("variable:" if t.role == "var" else "input:") + t.name: "typed_" + t.role for t in model.typed}}


def guide(model, topic="", renderer_identity="unbound") -> dict:
    """Resolve one construct or declared object from the registry and the current model."""
    if not isinstance(topic, str) or len(topic) > 128:
        raise Refusal("Help topic must be text of at most 128 characters")
    topic = topic.strip()
    objects = _objects(model)
    pairs = legend(model)["entries"]
    profile = effects.PROFILE if isinstance(model, effects.Signatures) else typed2.PROFILE
    explicit = topic.startswith("grammar:")
    if explicit:
        topic = topic.removeprefix("grammar:")
    if topic and topic not in objects and not explicit:
        matches = [key for key in objects if key.partition(":")[2] == topic]
        if len(matches) > 1 or matches and (topic in BY_ID or topic in ALIASES):
            raise Refusal("Ambiguous help topic; use a typed object ID or grammar:TOPIC")
        if matches:
            topic = matches[0]
    result = {"kind": "language-help", "schema": GUIDE_SCHEMA, "profile": profile, "revision": model.revision,
              "renderer": renderer_identity}
    notes = ["Help describes the current canonical model, not arbitrary editor text or source spans."]
    if topic in objects and not explicit:
        construct = BY_ID[objects[topic]]
        name = topic.partition(":")[2]
        entries = [e for e in pairs if _mentions(e["symbolic"], topic, name)]
        result.update(topic=topic, entry_kind="object", construct=construct.id, title="Meaning of " + topic)
    elif topic:
        resolved = ALIASES.get(topic, topic)
        if resolved not in BY_ID:
            raise Refusal("Unknown help topic; use guide for topics or an exact typed object ID")
        construct = BY_ID[resolved]
        entries = [e for e in pairs if e["concept"] == construct.id][:4]
        result.update(topic=resolved, entry_kind=construct.kind, construct=construct.id, title=construct.title)
    else:
        result.update(topic="overview", entry_kind="overview", construct=None, title="ZAL types and effects guide",
                      meaning="One finite typed model has exact symbolic and controlled-English views.",
                      forms=None, entries=[],
                      constructs=[{"id": c.id, "kind": c.kind, "title": c.title, "symbolic": c.symbolic,
                                   "english": c.english} for c in REGISTRY])
        notes.append("Use guide TOPIC for one construct or an exact typed object ID for one declared object.")
    if result["entry_kind"] != "overview":
        result.update(meaning=construct.meaning, forms={"symbolic": construct.symbolic, "english": construct.english},
                      entries=entries)
    if result["entry_kind"] == "object" and topic.startswith("type:"):
        result["cardinality"] = card_data(model.types.cardinality(Ty("ref", (topic[5:],))))
    return {**result, "notes": notes, "used_in": [], "available_topics": sorted(set(BY_ID) | set(objects)),
            "method": "generated from the construct registry, the parsers' statement patterns and the current model; "
                      "no language model", "authority": "none"}


def _mentions(symbolic, topic, name):
    kind = topic.partition(":")[0]
    starts = {"port": (f"operation {name}(", f"footprint {name}:"), "type": (f"type {name} =",),
              "law": (f"law {name} ",), "capability": (f"capability {name} =",), "resource": (f"resource {name} =",),
              "answer": (f"answer {name} =",), "frame": (f"frame {name}:",), "variable": (f"var {name}:",),
              "input": (f"input {name}:",)}[kind]
    return symbolic.startswith(starts)


def explain(model: effects.Signatures, topic: str) -> dict:
    """Explain one port or one type from the checked model; no language model."""
    if not isinstance(topic, str) or len(topic) > 128:
        raise Refusal("Explain topic must be text of at most 128 characters")
    kind, _, name = topic.partition(":")
    if kind == "type":
        return {**model.types.explain(name), "revision": model.revision}
    if kind != "port" or model.lookup("operation", name) is None or model.lookup("footprint", name) is None:
        raise Refusal("Explain a declared port as port:NAME or a type as type:NAME")
    types = model.types
    operation, footprint = model.lookup("operation", name), model.lookup("footprint", name)
    port = model.lookup("port", name)
    answer = model.lookup("answer", operation.answer)
    frame = model.lookup("frame", answer.name)
    frames = dict(frame.entries) if frame else {}
    capability = model.lookup("capability", footprint.capability)
    lines = [e for e in legend(model)["entries"] if _mentions(e["symbolic"], "port:" + name, name)]
    cases = []
    for case, payload in answer.cases:
        ty = operation.payload if payload == effects.RESULT else payload
        cases.append({"case": case, "payload": ty.render(), "english": ty.render(True),
                      "carries_result": payload == effects.RESULT, "cardinality": card_data(types.cardinality(ty)),
                      "frame": frames.get(case), "frame_english": effects.FRAMES.get(frames.get(case))})
    access = []
    for resource in model.resources:
        access.append({"resource": resource.name, "bit": resource.bit,
                       "reads": resource.name in footprint.reads, "writes": resource.name in footprint.writes,
                       "capability_reads": resource.name in capability.reads,
                       "capability_writes": resource.name in capability.writes})
    report = effects.check(model)
    result = {"kind": "effect-explanation", "profile": effects.PROFILE, "revision": model.revision, "port": name,
              "tag": port.tag, "statements": lines,
              "arguments": [{"name": n, "type": t.render(), "english": t.render(True),
                             "cardinality": card_data(types.cardinality(t))} for n, t in operation.arguments],
              "argument_cardinality": card_data(adt.card_product([types.cardinality(t) for _, t in operation.arguments])),
              "answer": {"shape": answer.name, "number": answer.number, "cases": cases},
              "capability": {"name": capability.name, "number": capability.number, "store": capability.store},
              "footprint": access,
              "within_capability": all(r["capability_reads"] or not r["reads"] for r in access)
                                   and all(r["capability_writes"] or not r["writes"] for r in access),
              "laws": [{"id": i, **({"name": law.name, "statement": law.statement} if law else {"name": None})}
                       for i in footprint.laws for law in [next((x for x in model.laws if x.id == i), None)]],
              "v2": render_v2(footprint), "check": report["status"],
              "method": "computed from the checked model; no language model", "authority": "none"}
    if report["status"] == "pass-with-scope":
        record = effects.lower(model)
        result["record"] = {"bytes": len(record), "identity": hashlib.sha256(record).hexdigest()}
    else:
        result["check_failure"] = {"obligation": report["obligation"], "reason": report["reason"]}
    if len(canonical(result).encode()) > 512 * 1024:
        raise Refusal("Explanation exceeds the 512 KiB output bound")
    return result


def render_v2(footprint):
    return {"kind": footprint.v2[0], "symbolic": effects.render_footprint_tail(footprint, False),
            "english": effects.render_footprint_tail(footprint, True)}


def human_text(result):
    """Plain text of a guide or explanation."""
    if result["kind"] == "language-help":
        lines = [f"{result['title']} | {result['schema']} | {result['profile']} | revision {result['revision'][:12]}",
                 result["meaning"]]
        if result.get("forms"):
            lines += ["Symbolic form: " + result["forms"]["symbolic"], "English form: " + result["forms"]["english"]]
        if result.get("cardinality"):
            lines.append("Values: " + result["cardinality"]["text"])
        for entry in result["entries"]:
            lines += [entry["english"], "  Symbolic: " + entry["symbolic"]]
        for item in result.get("constructs", []):
            lines.append(f"{item['id']:<14} {item['kind']:<12} {item['symbolic']}  |  {item['english']}")
        lines += result["notes"]
    elif result["kind"] == "type-explanation":
        lines = [f"Type {result['type']} ({result['form']}) has {result['cardinality']['text']} values: {result['rule']}."]
        if result.get("english"):
            lines += [result["english"], "  Symbolic: " + result["symbolic"]]
    else:
        lines = [f"Port {result['port']} (tag {result['tag']}), checked: {result['check']}"]
        lines += [e["english"] for e in result["statements"]]
        lines.append(f"Arguments admit {result['argument_cardinality']['text']} values.")
        for case in result["answer"]["cases"]:
            lines.append(f"  {case['case']}: {case['english']} ({case['cardinality']['text']} values); "
                         f"{case['frame_english'] or 'no frame'}")
        used = [r["resource"] for r in result["footprint"] if r["reads"] or r["writes"]]
        lines.append(f"Footprint {'stays within' if result['within_capability'] else 'exceeds'} capability "
                     f"{result['capability']['name']}; touches {', '.join(used) or 'nothing'}.")
        lines += [f"  Law {law['id']} {law['name']}: {law.get('statement', 'does not exist')}" for law in result["laws"]]
        if "record" in result:
            lines.append(f"The whole record is {result['record']['bytes']} bytes, identity {result['record']['identity'][:16]}.")
    lines.append("Generated from the checked model and the construct registry; no LLM, agreement or production authority.")
    return "\n".join(lines)
