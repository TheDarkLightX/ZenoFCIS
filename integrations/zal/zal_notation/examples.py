"""Custom grammar example; the original MIT notice is retained in LICENSE."""
from __future__ import annotations

from .builtin import generic_pack
from .core import term_model, validate_pack
from .syntax import parse_text


def custom_recipe() -> tuple[dict, dict, dict, str]:
    grammar = {"schema": "zal.grammar/1", "id": "user/recipe", "version": 1, "kind": "term", "root": "Recipe",
               "constructors": {"Ingredient": {"result": "Recipe", "fields": {"name": "str"}, "children": []},
                                "Mix": {"result": "Recipe", "fields": {}, "children": ["Recipe", "Recipe"]}},
               "syntax": {"start": "Recipe", "rules": [
                   {"lhs": "Recipe", "rhs": [{"token": "identifier"}], "tag": "Ingredient", "children": [], "fields": {"name": {"token": 0, "as": "str"}}},
                   {"lhs": "Recipe", "rhs": [{"lit": "mix"}, {"lit": "("}, {"ref": "Recipe"}, {"lit": ","}, {"ref": "Recipe"}, {"lit": ")"}], "tag": "Mix", "children": [2, 4], "fields": {}}
               ]}, "description": "User-defined recipe syntax. This example has no culinary or nutrition evaluator."}
    pack = generic_pack(grammar)
    pack["id"] = "user/recipe-symbols"
    pack["description"] = "Original four-lobed composition glyph: a memorable idea-symbol, inspired by the general communicative role of Adinkra; no traditional symbol or traditional meaning is claimed."
    pack["glyphs"]["Ingredient"] = {"primitives": [{"kind": "ellipse", "cx": 50, "cy": 50, "rx": 25, "ry": 25, "stroke": "ink", "fill": "paper"},
                                                      {"kind": "ellipse", "cx": 50, "cy": 50, "rx": 5, "ry": 5, "stroke": "ink", "fill": "accent"}], "label": "{name}"}
    pack["glyphs"]["Mix"] = {"primitives": [{"kind": "path", "commands": [["M", 50, 50], ["C", 10, 10, 10, 90, 50, 50], ["C", 10, 10, 90, 10, 50, 50], ["C", 90, 10, 90, 90, 50, 50], ["C", 90, 90, 10, 90, 50, 50]], "stroke": "accent", "fill": "none"}], "label": "mix"}
    source = "mix(oats, mix(milk, cocoa))"
    document = term_model(grammar, parse_text(source, grammar))
    validate_pack(grammar, pack)
    return grammar, pack, document, source
