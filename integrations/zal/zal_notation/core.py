"""Bounded, declarative syntax and notation admission. No user code execution."""
from __future__ import annotations

import hashlib
import json
import math
import re as re  # noqa: PLC0414 - preserve the research module attribute
from typing import Any

MAX_BYTES = 2_000_000
MAX_NODES = 512
MAX_DEPTH = 64
MAX_EDGES = 2048
MAX_PRIMITIVES = 128
COLORS = {"ink", "accent", "muted", "paper", "none"}


class NotationError(ValueError):
    """An unsupported or malformed declaration; never a semantic verdict."""


def need(ok: bool, message: str) -> None:
    if not ok:
        raise NotationError(message)


def keys(value: Any, required: set[str], optional: set[str] = frozenset(), where: str = "object") -> None:
    need(type(value) is dict, f"{where}: expected an object")
    need(required <= value.keys(), f"{where}: missing keys {sorted(required - value.keys())}")
    need(value.keys() <= required | optional, f"{where}: unknown keys {sorted(value.keys() - required - optional)}")


def name(value: Any, where: str = "name") -> None:
    need(type(value) is str and 0 < len(value) <= 128, f"{where}: expected a nonempty string of at most 128 characters")
    need(all(ord(c) >= 32 and c not in "\x7f" for c in value), f"{where}: control characters forbidden")


def _bounded(value: Any, depth: int = 0, counter: list[int] | None = None) -> None:
    if counter is None:
        counter = [0]
    counter[0] += 1
    need(counter[0] <= 50000 and depth <= 150, "JSON structure budget exceeded")
    if type(value) is dict:
        for k, v in value.items():
            need(type(k) is str, "JSON keys must be strings")
            _bounded(k, depth + 1, counter)
            _bounded(v, depth + 1, counter)
    elif type(value) is list:
        for v in value:
            _bounded(v, depth + 1, counter)
    elif type(value) is str:
        need(len(value) <= 16000, "JSON string budget exceeded")
        need(all(ord(c) >= 32 or c in "\n\r\t" for c in value), "JSON contains unsupported control characters")
        need(not any(0xD800 <= ord(c) <= 0xDFFF or ord(c) in (0xFFFE, 0xFFFF) for c in value), "unsupported XML/Unicode character")
    elif type(value) is float:
        need(math.isfinite(value), "nonfinite numbers forbidden")
    else:
        need(value is None or type(value) in (int, bool), "not JSON data")
        if type(value) is int:
            need(value.bit_length() <= 4096, "integer size budget exceeded")


def canonical(value: Any) -> str:
    _bounded(value)
    result = json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"), allow_nan=False)
    need(len(result.encode("utf-8")) <= MAX_BYTES, "serialized data budget exceeded")
    return result


def digest(value: Any) -> str:
    return hashlib.sha256(canonical(value).encode("utf-8")).hexdigest()


def loads(source: str) -> dict:
    need(type(source) is str, "JSON input must be text")
    try:
        need(len(source.encode("utf-8")) <= MAX_BYTES, "JSON input budget exceeded")
    except UnicodeError as exc:
        raise NotationError("invalid Unicode input") from exc
    def pairs(items: list) -> dict:
        out = {}
        for k, v in items:
            need(k not in out, f"duplicate JSON key: {k}")
            out[k] = v
        return out
    try:
        result = json.loads(source, object_pairs_hook=pairs,
                            parse_constant=lambda value: (_ for _ in ()).throw(NotationError(f"invalid number: {value}")))
        canonical(result)
    except (json.JSONDecodeError, RecursionError, UnicodeError, OverflowError) as exc:
        raise NotationError(f"invalid or over-budget JSON: {exc}") from exc
    need(type(result) is dict, "document must be an object")
    return result


def _field_schema(fields: Any) -> None:
    need(type(fields) is dict and len(fields) <= 32, "fields must be an object with at most 32 entries")
    for k, v in fields.items():
        name(k, "field name")
        need(not k.startswith("_"), "field names starting with _ are reserved")
        need(v in ("str", "int", "bool"), "field types are str, int or bool")


def _fields(fields: Any, schema: dict) -> None:
    keys(fields, set(schema), where="fields")
    types = {"str": str, "int": int, "bool": bool}
    for k, t in schema.items():
        need(type(fields[k]) is types[t], f"field {k}: expected {t}")
        if t == "str":
            need(len(fields[k]) <= 1024, f"field {k}: text budget exceeded")


def validate_grammar(grammar: dict) -> None:
    canonical(grammar)
    common = {"schema", "id", "version", "kind"}
    need(type(grammar) is dict, "grammar must be an object")
    kind = grammar.get("kind")
    need(kind in ("term", "graph"), "grammar kind must be term or graph")
    required = common | ({"root", "constructors"} if kind == "term" else {"nodes", "edges"})
    keys(grammar, required, {"description", "provenance", "syntax"} if kind == "term" else {"description", "provenance"}, "grammar")
    need(grammar["schema"] == "zal.grammar/1", "unsupported grammar schema")
    name(grammar["id"], "grammar id")
    need(type(grammar["version"]) is int and 1 <= grammar["version"] <= 1_000_000, "grammar version must be a positive integer")
    if kind == "term":
        name(grammar["root"], "root sort")
        constructors = grammar["constructors"]
        need(type(constructors) is dict and 0 < len(constructors) <= 128, "expected 1..128 constructors")
        results = set()
        for tag, spec in constructors.items():
            name(tag, "constructor")
            keys(spec, {"result", "fields"}, {"children", "variadic"}, "constructor")
            need(("children" in spec) != ("variadic" in spec), "constructor needs children or variadic")
            name(spec["result"], "result sort")
            results.add(spec["result"])
            _field_schema(spec["fields"])
            if "children" in spec:
                need(type(spec["children"]) is list and len(spec["children"]) <= 32, "invalid child sorts")
                for sort in spec["children"]:
                    name(sort, "child sort")
            else:
                name(spec["variadic"], "variadic sort")
        need(grammar["root"] in results, "root sort has no constructor")
        for spec in constructors.values():
            sorts = spec.get("children", [spec.get("variadic")])
            need(all(s in results for s in sorts), "child sort has no constructor")
        if "syntax" in grammar:
            from .syntax import validate_syntax
            validate_syntax(grammar)
    else:
        for collection in ("nodes", "edges"):
            need(type(grammar[collection]) is dict and 0 < len(grammar[collection]) <= 128, f"expected 1..128 {collection} kinds")
        for tag, spec in grammar["nodes"].items():
            name(tag, "node kind")
            keys(spec, {"fields", "ports"}, where="node kind")
            _field_schema(spec["fields"])
            need(type(spec["ports"]) is dict and len(spec["ports"]) <= 32, "invalid ports")
            for p, port in spec["ports"].items():
                name(p, "port")
                keys(port, {"direction", "sort"}, where="port")
                need(port["direction"] in ("in", "out"), "port direction must be in or out")
                name(port["sort"], "port sort")
        for tag, spec in grammar["edges"].items():
            name(tag, "edge kind")
            keys(spec, {"fields"}, {"sort"}, where="edge kind")
            _field_schema(spec["fields"])
            if "sort" in spec:
                name(spec["sort"], "edge sort")


def validate_model(grammar: dict, model: dict) -> None:
    validate_grammar(grammar)
    canonical(model)
    keys(model, {"schema", "grammar", "kind", "body"}, where="model")
    need(model["schema"] == "zal.model/1", "unsupported model schema")
    keys(model["grammar"], {"id", "version"}, where="model grammar reference")
    need(type(model["grammar"]["version"]) is int, "model grammar version must be integer")
    need(model["grammar"] == {"id": grammar["id"], "version": grammar["version"]}, "model/grammar identity mismatch")
    need(model["kind"] == grammar["kind"], "model/grammar kind mismatch")
    if grammar["kind"] == "term":
        count = 0
        def walk(term: Any, expected: str, depth: int) -> None:
            nonlocal count
            count += 1
            need(count <= MAX_NODES and depth <= MAX_DEPTH, "term size/depth budget exceeded")
            keys(term, {"tag", "fields", "children"}, where="term")
            need(type(term["tag"]) is str and term["tag"] in grammar["constructors"], "unknown constructor")
            spec = grammar["constructors"][term["tag"]]
            need(spec["result"] == expected, "constructor result sort mismatch")
            _fields(term["fields"], spec["fields"])
            need(type(term["children"]) is list, "children must be an ordered array")
            children = term["children"]
            sorts = [spec["variadic"]] * len(children) if "variadic" in spec else spec["children"]
            need(len(children) == len(sorts), "constructor arity mismatch")
            for child, sort in zip(children, sorts):
                walk(child, sort, depth + 1)
        walk(model["body"], grammar["root"], 0)
    else:
        body = model["body"]
        keys(body, {"nodes", "edges"}, where="graph")
        need(type(body["nodes"]) is list and 0 < len(body["nodes"]) <= MAX_NODES, "graph node budget exceeded or empty graph")
        need(type(body["edges"]) is list and len(body["edges"]) <= MAX_EDGES, "graph edge budget exceeded")
        by_id = {}
        for node in body["nodes"]:
            keys(node, {"id", "tag", "fields"}, where="node")
            name(node["id"], "node id")
            need(node["id"] not in by_id, "duplicate node id")
            need(type(node["tag"]) is str and node["tag"] in grammar["nodes"], "unknown node kind")
            _fields(node["fields"], grammar["nodes"][node["tag"]]["fields"])
            by_id[node["id"]] = node
        ids = set()
        for edge in body["edges"]:
            keys(edge, {"id", "tag", "source", "target", "fields"}, where="edge")
            name(edge["id"], "edge id")
            need(edge["id"] not in ids, "duplicate edge id")
            ids.add(edge["id"])
            need(type(edge["tag"]) is str and edge["tag"] in grammar["edges"], "unknown edge kind")
            _fields(edge["fields"], grammar["edges"][edge["tag"]]["fields"])
            ports = []
            for end, direction in (("source", "out"), ("target", "in")):
                endpoint = edge[end]
                need(type(endpoint) is list and len(endpoint) == 2 and all(type(v) is str for v in endpoint), "endpoint must be [node, port]")
                n, p = endpoint
                need(n in by_id, "dangling edge")
                available = grammar["nodes"][by_id[n]["tag"]]["ports"]
                need(p in available, "unknown port")
                port = available[p]
                need(port["direction"] == direction, "port direction mismatch")
                ports.append(port)
            need(ports[0]["sort"] == ports[1]["sort"], "edge port sort mismatch")
            declared_sort = grammar["edges"][edge["tag"]].get("sort")
            need(declared_sort is None or declared_sort == ports[0]["sort"], "edge kind/port sort mismatch")


def format_label(template: str, fields: dict, tag: str = "", node_id: str = "") -> str:
    need(type(template) is str and len(template) <= 1024, "invalid label template")
    values = dict(fields, _tag=tag, _id=node_id)
    parts = []
    i = 0
    while i < len(template):
        c = template[i]
        if c in "{}" and template[i:i+2] == c * 2:
            parts.append(c)
            i += 2
        elif c == "{":
            end = template.find("}", i + 1)
            need(end >= 0, "unclosed label placeholder")
            key = template[i+1:end]
            need(key in values, f"unknown label placeholder: {key}")
            value = values[key]
            parts.append(str(value).lower() if type(value) is bool else str(value))
            i = end + 1
        else:
            need(c != "}", "unmatched label brace")
            parts.append(c)
            i += 1
    output = "".join(parts)
    need(len(output) <= 4096, "rendered label budget exceeded")
    return output


def _coord(value: Any) -> None:
    need(type(value) in (float, int) and 0 <= value <= 100 and math.isfinite(value), "glyph coordinates must be finite numbers in 0..100")


def validate_pack(grammar: dict, pack: dict) -> None:
    validate_grammar(grammar)
    canonical(pack)
    keys(pack, {"schema", "id", "version", "grammar", "layout", "glyphs"}, {"edges", "description", "provenance"}, "notation pack")
    need(pack["schema"] == "zal.notation/1", "unsupported notation schema")
    name(pack["id"], "notation id")
    need(type(pack["version"]) is int and 1 <= pack["version"] <= 1_000_000, "invalid notation version")
    keys(pack["grammar"], {"id", "version"}, where="notation grammar reference")
    need(type(pack["grammar"]["version"]) is int, "notation grammar version must be integer")
    need(pack["grammar"] == {"id": grammar["id"], "version": grammar["version"]}, "notation/grammar identity mismatch")
    need(type(pack["layout"]) is str and pack["layout"] in ({"tree", "tromp"} if grammar["kind"] == "term" else {"graph"}), "layout incompatible with grammar")
    if pack["layout"] == "tromp":
        from .builtin import lambda_grammar
        need(grammar == lambda_grammar(), "Tromp projection requires the exact supported lambda grammar")
    specs = grammar["constructors"] if grammar["kind"] == "term" else grammar["nodes"]
    keys(pack["glyphs"], set(specs), where="glyph mapping")
    for tag, glyph in pack["glyphs"].items():
        keys(glyph, {"primitives"}, {"anchors", "label"}, "glyph")
        fields = {k: "value" if t == "str" else 0 if t == "int" else False for k, t in specs[tag]["fields"].items()}
        format_label(glyph.get("label", "{_tag}"), fields, tag, "node")
        primitives = glyph["primitives"]
        need(type(primitives) is list and 0 < len(primitives) <= MAX_PRIMITIVES, "glyph primitive budget exceeded")
        for p in primitives:
            need(type(p) is dict and type(p.get("kind")) is str, "invalid glyph primitive")
            kind = p["kind"]
            geometry = {"line": {"x1", "y1", "x2", "y2"}, "rect": {"x", "y", "width", "height"},
                        "ellipse": {"cx", "cy", "rx", "ry"}, "polygon": {"points"},
                        "path": {"commands"}, "text": {"x", "y", "text"}}
            need(kind in geometry, "unknown primitive: raw SVG and executable code are forbidden")
            keys(p, {"kind"} | geometry[kind], {"stroke", "fill"}, "primitive")
            for color in (p.get("stroke", "ink"), p.get("fill", "none")):
                need(type(color) is str and color in COLORS, "colors must be palette tokens; CSS/URLs forbidden")
            if kind == "polygon":
                need(type(p["points"]) is list and 3 <= len(p["points"]) <= 64, "invalid polygon")
                for point in p["points"]:
                    need(type(point) is list and len(point) == 2, "invalid polygon point")
                    for v in point:
                        _coord(v)
            elif kind == "path":
                commands = p["commands"]
                need(type(commands) is list and 1 <= len(commands) <= 64, "invalid path command count")
                need(type(commands[0]) is list and len(commands[0]) and commands[0][0] == "M", "path must begin with M")
                for command in commands:
                    need(type(command) is list and len(command) > 0 and type(command[0]) is str, "invalid path command")
                    sizes = {"M": 3, "L": 3, "Q": 5, "C": 7, "Z": 1}
                    need(command[0] in sizes and len(command) == sizes[command[0]], "unsupported path command")
                    for v in command[1:]:
                        _coord(v)
            else:
                for field in geometry[kind] - {"text"}:
                    _coord(p[field])
                if kind == "rect":
                    need(p["x"] + p["width"] <= 100 and p["y"] + p["height"] <= 100, "rectangle exceeds glyph bounds")
                if kind == "ellipse":
                    need(0 <= p["cx"] - p["rx"] <= p["cx"] + p["rx"] <= 100 and 0 <= p["cy"] - p["ry"] <= p["cy"] + p["ry"] <= 100, "ellipse exceeds glyph bounds")
                if kind == "text":
                    format_label(p["text"], fields, tag, "node")
        anchors = glyph.get("anchors", {})
        needed = set(specs[tag].get("ports", {}))
        need(type(anchors) is dict and (grammar["kind"] == "term" or set(anchors) == needed), "glyph must declare exactly its graph port anchors")
        need(len(anchors) <= 32, "too many anchors")
        for anchor, point in anchors.items():
            name(anchor, "anchor")
            need(type(point) is list and len(point) == 2, "anchor must be [x,y]")
            for value in point:
                _coord(value)
    if grammar["kind"] == "graph":
        keys(pack.get("edges"), set(grammar["edges"]), where="edge rules")
        for tag, rule in pack["edges"].items():
            keys(rule, {"style", "label", "arrow"}, where="edge rule")
            need(rule["style"] in ("straight", "dashed", "curved", "wavy"), "unsupported edge style")
            need(type(rule["arrow"]) is bool, "edge arrow must be Boolean")
            format_label(rule["label"], {k: "value" for k in grammar["edges"][tag]["fields"]}, tag, "edge")
    else:
        need(pack.get("edges", {}) == {}, "term notation has no graph edge rules")


def model_digest(grammar: dict, model: dict) -> str:
    return digest({"grammar": grammar, "model": model})


def term_model(grammar: dict, term: dict) -> dict:
    model = {"schema": "zal.model/1", "grammar": {"id": grammar["id"], "version": grammar["version"]}, "kind": "term", "body": term}
    validate_model(grammar, model)
    return model


class AdapterRegistry:
    """Trusted host registration; untrusted packs cannot import an adapter."""
    def __init__(self) -> None:
        self._parsers: dict[str, Any] = {}

    def register_parser(self, adapter_id: str, parser: Any) -> None:
        name(adapter_id, "adapter id")
        need(adapter_id not in self._parsers and callable(parser), "duplicate or invalid parser registration")
        self._parsers[adapter_id] = parser

    def parse(self, adapter_id: str, source: str, grammar: dict) -> dict:
        need(adapter_id in self._parsers, "parser adapter is not explicitly registered by the host")
        # Host callbacks are trusted code, not sandboxed and not named by a pack.
        model = self._parsers[adapter_id](source)
        validate_model(grammar, model)
        return model
