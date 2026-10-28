"""Starter grammar/notation packs. Every pack can be exported and customized."""
from __future__ import annotations

import copy as copy  # noqa: PLC0414 - preserve the research module attribute

from .core import NotationError, digest, validate_grammar


def term(tag: str, *children: dict, **fields) -> dict:
    return {"tag": tag, "fields": fields, "children": list(children)}


def model(grammar: dict, body: dict) -> dict:
    return {"schema": "zal.model/1", "grammar": {"id": grammar["id"], "version": grammar["version"]}, "kind": grammar["kind"], "body": body}


def _grammar(id_: str, kind: str, **rest) -> dict:
    return {"schema": "zal.grammar/1", "id": id_, "version": 1, "kind": kind, **rest}


def lambda_grammar() -> dict:
    rules = []
    for token in ("\\", "λ"):
        rules.append({"lhs": "Term", "rhs": [{"lit": token}, {"token": "identifier"}, {"lit": "."}, {"ref": "Term"}], "tag": "Lam", "children": [3], "fields": {"name": {"token": 1, "as": "str"}}})
    rules += [
        {"lhs": "Term", "rhs": [{"ref": "Application"}], "forward": 0},
        {"lhs": "Application", "rhs": [{"ref": "Application"}, {"ref": "Atom"}], "tag": "App", "children": [0, 1], "fields": {}},
        {"lhs": "Application", "rhs": [{"ref": "Atom"}], "forward": 0},
        {"lhs": "Atom", "rhs": [{"token": "identifier"}], "tag": "Var", "children": [], "fields": {"name": {"token": 0, "as": "str"}}},
        {"lhs": "Atom", "rhs": [{"lit": "("}, {"ref": "Term"}, {"lit": ")"}], "forward": 1},
    ]
    return _grammar("zal/lambda", "term", root="Term", constructors={
        "Var": {"result": "Term", "fields": {"name": "str"}, "children": []},
        "Lam": {"result": "Term", "fields": {"name": "str"}, "children": ["Term"]},
        "App": {"result": "Term", "fields": {}, "children": ["Term", "Term"]},
    }, syntax={"start": "Term", "rules": rules})


def _ports(sort="Control") -> dict:
    return {"in": {"direction": "in", "sort": sort}, "out": {"direction": "out", "sort": sort}}


def fsm_grammar() -> dict:
    return _grammar("zal/state-machine", "graph", nodes={
        tag: {"fields": {"name": "str"}, "ports": _ports()}
        for tag in ("start", "state", "accepting")
    }, edges={"transition": {"fields": {"event": "str", "outcome": "str"}, "sort": "Control"}})


def petri_grammar() -> dict:
    return _grammar("zal/petri-net", "graph", nodes={
        "place": {"fields": {"name": "str", "tokens": "int"}, "ports": _ports("Token")},
        "transition": {"fields": {"name": "str"}, "ports": _ports("Token")},
    }, edges={"arc": {"fields": {"weight": "int"}, "sort": "Token"}})


def patterns_grammar() -> dict:
    return _grammar("zal/patterns", "term", root="Concept", constructors={
        "Atom": {"result": "Concept", "fields": {"name": "str"}, "children": []},
        "Sequence": {"result": "Concept", "fields": {"name": "str"}, "variadic": "Concept"},
        "Guard": {"result": "Concept", "fields": {"predicate": "str"}, "children": ["Concept", "Concept"]},
        "Transfer": {"result": "Concept", "fields": {"amount": "str"}, "children": []},
        "Preserve": {"result": "Concept", "fields": {"name": "str"}, "children": []},
        "Effect": {"result": "Concept", "fields": {"name": "str"}, "children": []},
    }, description="Conceptual patterns. Guard text and effect names are declared descriptions, not executable expressions.")


def relations_grammar() -> dict:
    return _grammar("zal/relations", "graph", nodes={
        "model": {"fields": {"name": "str"}, "ports": _ports("Model")},
        "proposition": {"fields": {"name": "str"}, "ports": _ports("Proposition")},
    }, edges={tag: {"fields": {"note": "str"}, "sort": sort}
              for tag, sort in (("equivalence", "Model"), ("refinement", "Model"), ("abstraction", "Model"), ("implication", "Proposition"))},
       description="Relations are declared claims; edge appearance grants no proof or approval.")


def ports_grammar() -> dict:
    return _grammar("zal/port-graph", "graph", nodes={
        "input": {"fields": {"name": "str"}, "ports": {"out": {"direction": "out", "sort": "Value"}}},
        "operation": {"fields": {"name": "str"}, "ports": _ports("Value")},
        "output": {"fields": {"name": "str"}, "ports": {"in": {"direction": "in", "sort": "Value"}}},
    }, edges={"value": {"fields": {"name": "str"}, "sort": "Value"}})


def interactions_grammar() -> dict:
    return _grammar("zal/interactions", "graph", nodes={
        "endpoint": {"fields": {"name": "str"}, "ports": _ports("Interaction")},
        "vertex": {"fields": {"name": "str"}, "ports": _ports("Interaction")},
    }, edges={tag: {"fields": {"name": "str"}, "sort": "Interaction"} for tag in ("matter", "exchange", "dependency")},
       description="Feynman-inspired interaction notation; no particle physics or amplitude interpretation.")


def _glyph(primitives: list, label: str = "{name}", anchors: dict | None = None) -> dict:
    return {"primitives": primitives, "label": label, "anchors": anchors or {}}


def _rect() -> dict:
    return {"kind": "rect", "x": 8, "y": 20, "width": 84, "height": 60, "stroke": "ink", "fill": "paper"}


def _circle(radius: int = 38) -> dict:
    return {"kind": "ellipse", "cx": 50, "cy": 50, "rx": radius, "ry": radius, "stroke": "ink", "fill": "paper"}


def _line(x1, y1, x2, y2) -> dict:
    return {"kind": "line", "x1": x1, "y1": y1, "x2": x2, "y2": y2, "stroke": "ink"}


def generic_pack(grammar: dict) -> dict:
    validate_grammar(grammar)
    specs = grammar["constructors"] if grammar["kind"] == "term" else grammar["nodes"]
    glyphs = {}
    for tag, spec in specs.items():
        field_names = list(spec["fields"])
        label = "{_tag}" + (": " + ", ".join("{" + f + "}" for f in field_names[:3]) if field_names else "")
        if len(field_names) > 3:
            label += f" (+{len(field_names)-3} fields in source)"
        port_names = list(spec.get("ports", {}))
        anchors = {p: [8 if spec["ports"][p]["direction"] == "in" else 92, 50] for p in port_names}
        glyphs[tag] = _glyph([_rect()], label, anchors)
    pack = {"schema": "zal.notation/1", "id": "zal/generic/" + digest(grammar)[:12], "version": 1,
            "grammar": {"id": grammar["id"], "version": grammar["version"]},
            "layout": "tree" if grammar["kind"] == "term" else "graph", "glyphs": glyphs,
            "description": "Generic structural view. Semantic interpretation is supplied separately."}
    if grammar["kind"] == "graph":
        pack["edges"] = {tag: {"style": "curved", "label": "{_tag}" + (": " + "{" + next(iter(spec["fields"])) + "}" if spec["fields"] else ""), "arrow": True}
                         for tag, spec in grammar["edges"].items()}
    return pack


def get_builtin(name: str) -> tuple[dict, dict, dict]:
    factories = {"lambda": lambda_grammar, "state-machine": fsm_grammar, "petri-net": petri_grammar,
                 "zal-folds": patterns_grammar, "relations": relations_grammar,
                 "port-graph": ports_grammar, "interactions": interactions_grammar}
    if name not in factories:
        raise NotationError(f"unknown built-in: {name}")
    grammar = factories[name]()
    pack = generic_pack(grammar)
    pack["id"] = "zal/notation/" + name
    anchors = {"in": [12, 50], "out": [88, 50]}
    if name == "lambda":
        pack["layout"] = "tromp"
        pack["description"] = "Tromp binder/occurrence/application notation. Closed terms; independent implementation."
        body = term("Lam", term("Lam", term("App", term("Var", name="f"), term("App", term("Var", name="f"), term("Var", name="x"))), name="x"), name="f")
    elif name == "state-machine":
        pack["glyphs"] = {"state": _glyph([_circle()], anchors=anchors),
                          "start": _glyph([_circle(), _line(0, 50, 12, 50), _line(6, 44, 12, 50), _line(6, 56, 12, 50)], "start: {name}", anchors=anchors),
                          "accepting": _glyph([_circle(), _circle(31)], anchors=anchors)}
        pack["edges"]["transition"]["label"] = "{event} / {outcome}"
        nodes = [{"id": s, "tag": "start" if s == "unseen" else "state", "fields": {"name": s}} for s in ("unseen", "held", "released")]
        rows = [("unseen", "reserve", "held", "Accept"), ("unseen", "release", "unseen", "Reject"),
                ("held", "reserve", "held", "Reject"), ("held", "release", "released", "Accept"),
                ("released", "reserve", "released", "Reject"), ("released", "release", "released", "Reject")]
        body = {"nodes": nodes, "edges": [{"id": f"e{i}", "tag": "transition", "source": [s, "out"], "target": [t, "in"], "fields": {"event": event, "outcome": outcome}} for i, (s, event, t, outcome) in enumerate(rows)]}
    elif name == "petri-net":
        pack["glyphs"] = {"place": _glyph([_circle(), {"kind": "text", "x": 50, "y": 55, "text": "{tokens}", "fill": "ink"}], anchors=anchors),
                          "transition": _glyph([{"kind": "rect", "x": 43, "y": 12, "width": 14, "height": 76, "fill": "ink", "stroke": "ink"}], anchors={"in": [43, 50], "out": [57, 50]})}
        pack["edges"]["arc"]["label"] = "{weight}"
        body = {"nodes": [{"id": "available", "tag": "place", "fields": {"name": "Available", "tokens": 3}},
                          {"id": "reserve", "tag": "transition", "fields": {"name": "Reserve"}},
                          {"id": "reserved", "tag": "place", "fields": {"name": "Reserved", "tokens": 0}}],
                "edges": [{"id": "take", "tag": "arc", "source": ["available", "out"], "target": ["reserve", "in"], "fields": {"weight": 1}},
                          {"id": "put", "tag": "arc", "source": ["reserve", "out"], "target": ["reserved", "in"], "fields": {"weight": 1}}]}
    elif name == "zal-folds":
        pack["glyphs"]["Transfer"] = _glyph([_line(20, 15, 20, 85), _line(80, 15, 80, 85), _line(20, 50, 80, 50), _line(70, 40, 80, 50), _line(70, 60, 80, 50),
                                                       {"kind": "text", "x": 9, "y": 55, "text": "−", "fill": "ink"}, {"kind": "text", "x": 92, "y": 55, "text": "+", "fill": "ink"}], "transfer {amount}")
        pack["glyphs"]["Guard"] = _glyph([{"kind": "polygon", "points": [[50, 8], [92, 50], [50, 92], [8, 50]], "stroke": "ink", "fill": "paper"}], "{predicate} · true / false")
        pack["glyphs"]["Preserve"] = _glyph([_line(25, 12, 25, 88), _line(75, 12, 75, 88)], "preserve {name}")
        pack["glyphs"]["Effect"] = _glyph([_rect(), _line(80, 20, 80, 80)], "plan: {name}")
        pack["description"] = "Original ZAL concept glyphs. Ordered Guard children mean declared true/false branches; formulas are uninterpreted text."
        body = term("Sequence", term("Guard", term("Transfer", amount="q"), term("Preserve", name="a, r"), predicate="1 ≤ q ≤ a; r ≤ M − q"), name="Guarded transfer")
    elif name == "relations":
        pack["glyphs"]["proposition"] = _glyph([{"kind": "polygon", "points": [[50, 6], [94, 50], [50, 94], [6, 50]], "fill": "paper", "stroke": "ink"}], anchors=anchors)
        for tag, rule in pack["edges"].items():
            rule["label"] = tag + ": {note} [declared]"
            rule["style"] = "dashed" if tag == "abstraction" else "curved"
        body = {"nodes": [{"id": n, "tag": "model", "fields": {"name": label}} for n, label in (("concrete", "Guarded transfer"), ("total", "Total quantity"), ("implementation", "Implementation"))],
                "edges": [{"id": "observe", "tag": "abstraction", "source": ["concrete", "out"], "target": ["total", "in"], "fields": {"note": "α(a,r)=a+r; omit allocation/outcome"}},
                          {"id": "realize", "tag": "refinement", "source": ["implementation", "out"], "target": ["concrete", "in"], "fields": {"note": "requires independent check"}}]}
    elif name == "port-graph":
        body = {"nodes": [{"id": "in", "tag": "input", "fields": {"name": "Request"}}, {"id": "core", "tag": "operation", "fields": {"name": "Pure decision"}}, {"id": "out", "tag": "output", "fields": {"name": "Effect plan"}}],
                "edges": [{"id": "request", "tag": "value", "source": ["in", "out"], "target": ["core", "in"], "fields": {"name": "request"}}, {"id": "plan", "tag": "value", "source": ["core", "out"], "target": ["out", "in"], "fields": {"name": "plan data"}}]}
    else:
        pack["glyphs"]["vertex"] = _glyph([_circle(7)], anchors={"in": [43, 50], "out": [57, 50]})
        pack["glyphs"]["endpoint"] = _glyph([_circle(20)], anchors={"in": [30, 50], "out": [70, 50]})
        for tag, style in (("matter", "straight"), ("exchange", "wavy"), ("dependency", "dashed")):
            pack["edges"][tag] = {"style": style, "label": "{name}", "arrow": True}
        pack["description"] = "Feynman-inspired interaction graph. Visual conventions only; no QFT calculation."
        body = {"nodes": [{"id": n, "tag": tag, "fields": {"name": label}} for n, tag, label in (("left", "endpoint", "Participant A"), ("meet", "vertex", "Interaction"), ("right", "endpoint", "Participant B"), ("context", "endpoint", "Context"))],
                "edges": [{"id": "a", "tag": "matter", "source": ["left", "out"], "target": ["meet", "in"], "fields": {"name": "input"}}, {"id": "b", "tag": "exchange", "source": ["meet", "out"], "target": ["right", "in"], "fields": {"name": "exchange"}}, {"id": "c", "tag": "dependency", "source": ["context", "out"], "target": ["meet", "in"], "fields": {"name": "dependency"}}]}
    return grammar, pack, model(grammar, body)


BUILTINS = ("lambda", "state-machine", "petri-net", "zal-folds", "relations", "port-graph", "interactions")
