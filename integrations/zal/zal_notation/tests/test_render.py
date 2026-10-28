"""Renderer checks: topology, source identity, custom glyphs and refusal cases."""
import json
import unittest
import xml.etree.ElementTree as ET
from copy import deepcopy

from zal_notation import core
from zal_notation.render import render_svg

NS = {"s": "http://www.w3.org/2000/svg"}


def expression_grammar():
    return {
        "schema": "zal.grammar/1", "id": "test/expressions", "version": 1,
        "kind": "term", "root": "Expr",
        "constructors": {
            "Number": {"result": "Expr", "fields": {"value": "int"}, "children": []},
            "Name": {"result": "Expr", "fields": {"name": "str"}, "children": []},
            "Add": {"result": "Expr", "fields": {}, "children": ["Expr", "Expr"]},
        },
    }


def term(tag, fields=None, *children):
    return {"tag": tag, "fields": fields or {}, "children": list(children)}


def tree_pack(grammar):
    return {
        "schema": "zal.notation/1", "id": "test/custom-glyphs", "version": 1,
        "grammar": {"id": grammar["id"], "version": grammar["version"]},
        "layout": "tree",
        "glyphs": {
            tag: {
                "label": "{_tag}",
                "primitives": [
                    {"kind": "polygon", "points": [[50, 5], [95, 50], [50, 95], [5, 50]], "fill": "paper"},
                    {"kind": "path", "commands": [["M", 25, 50], ["Q", 50, 10, 75, 50], ["C", 75, 80, 25, 80, 25, 50]], "stroke": "accent"},
                    {"kind": "text", "x": 50, "y": 57, "text": "{_tag}", "fill": "ink"},
                ],
            }
            for tag in grammar["constructors"]
        },
    }


def lambda_pack():
    from zal_notation.builtin import lambda_grammar
    grammar = lambda_grammar()
    pack = tree_pack(grammar)
    pack["id"] = "test/tromp"
    pack["layout"] = "tromp"
    return grammar, pack


def graph_example(style="wavy"):
    grammar = {
        "schema": "zal.grammar/1", "id": "test/messages", "version": 1,
        "kind": "graph",
        "nodes": {"Actor": {
            "fields": {"name": "str"},
            "ports": {"send": {"direction": "out", "sort": "Message"}, "receive": {"direction": "in", "sort": "Message"}},
        }},
        "edges": {"Message": {"fields": {"name": "str"}}},
    }
    model = {
        "schema": "zal.model/1", "grammar": {"id": grammar["id"], "version": 1}, "kind": "graph",
        "body": {
            "nodes": [{"id": "a", "tag": "Actor", "fields": {"name": "Alice"}}, {"id": "b", "tag": "Actor", "fields": {"name": "Bob"}}],
            "edges": [{"id": "msg", "tag": "Message", "source": ["a", "send"], "target": ["b", "receive"], "fields": {"name": "ping"}}],
        },
    }
    pack = {
        "schema": "zal.notation/1", "id": "test/interactions", "version": 1,
        "grammar": {"id": grammar["id"], "version": 1}, "layout": "graph",
        "glyphs": {"Actor": {
            "label": "{name}", "primitives": [{"kind": "ellipse", "cx": 50, "cy": 50, "rx": 47, "ry": 47, "fill": "paper"}],
            "anchors": {"send": [100, 50], "receive": [0, 50]},
        }},
        "edges": {"Message": {"style": style, "label": "{name}", "arrow": True}},
    }
    return grammar, model, pack


def metadata(svg):
    root = ET.fromstring(svg)
    return json.loads(root.find("s:metadata", NS).text)


class RenderTests(unittest.TestCase):
    def test_custom_composite_glyph_and_ordered_children(self):
        grammar = expression_grammar()
        model = core.term_model(grammar, term("Add", {}, term("Number", {"value": 1}), term("Number", {"value": 2})))
        pack = tree_pack(grammar)
        root = ET.fromstring(render_svg(grammar, model, pack))
        self.assertEqual(len(root.findall(".//s:polygon", NS)), 3)
        refs = [e.get("data-model-ref") for e in root.iter() if e.get("data-role") == "ordered-child"]
        self.assertEqual(refs, ["root.children.0", "root.children.1"])
        self.assertTrue(any("Q" in p.get("d", "") and "C" in p.get("d", "") for p in root.findall(".//s:path", NS)))

    def test_determinism(self):
        grammar, model, pack = graph_example()
        self.assertEqual(render_svg(grammar, model, pack), render_svg(deepcopy(grammar), deepcopy(model), deepcopy(pack)))

    def test_labels_are_xml_text_not_markup(self):
        grammar = expression_grammar()
        hostile = '<script>alert("x")</script><image href="https://example.invalid">'
        model = core.term_model(grammar, term("Name", {"name": hostile}))
        pack = tree_pack(grammar)
        pack["glyphs"]["Name"]["label"] = "{name}"
        svg = render_svg(grammar, model, pack)
        root = ET.fromstring(svg)
        self.assertFalse(root.findall(".//s:script", NS))
        self.assertFalse(root.findall(".//s:image", NS))
        self.assertFalse(any(key.startswith("on") for e in root.iter() for key in e.attrib))
        self.assertEqual(metadata(svg)["model"], model)
        self.assertIn("&lt;script&gt;", svg)

    def test_fold_preserves_source_and_model_identity(self):
        grammar = expression_grammar()
        model = core.term_model(grammar, term("Add", {}, term("Number", {"value": 4}), term("Number", {"value": 7})))
        pack = tree_pack(grammar)
        open_svg = render_svg(grammar, model, pack)
        folded_svg = render_svg(grammar, model, pack, folded=("root",))
        a, b = ET.fromstring(open_svg), ET.fromstring(folded_svg)
        self.assertEqual(a.get("data-model-digest"), b.get("data-model-digest"))
        self.assertNotEqual(a.get("data-view-digest"), b.get("data-view-digest"))
        self.assertEqual(metadata(open_svg)["model"], metadata(folded_svg)["model"])
        self.assertIn("2 retained nodes", folded_svg)
        self.assertFalse(any(e.get("data-role") == "ordered-child" for e in b.iter()))

    def test_folded_invalid_body_still_refused(self):
        grammar = expression_grammar()
        model = core.term_model(grammar, term("Add", {}, term("Number", {"value": 4}), term("Number", {"value": 7})))
        model["body"]["children"][1]["fields"]["value"] = "wrong type"
        with self.assertRaises(core.NotationError):
            render_svg(grammar, model, tree_pack(grammar), folded=("root",))

    def test_unknown_fold_path_refused(self):
        grammar = expression_grammar()
        model = core.term_model(grammar, term("Number", {"value": 1}))
        with self.assertRaises(core.NotationError):
            render_svg(grammar, model, tree_pack(grammar), folded=("root.children.8",))

    def test_tromp_binder_and_occurrence_are_connected(self):
        grammar, pack = lambda_pack()
        model = core.term_model(grammar, term("Lam", {"name": "x"}, term("Var", {"name": "x"})))
        root = ET.fromstring(render_svg(grammar, model, pack))
        binder = next(e for e in root.iter() if e.get("data-role") == "binder")
        occurrence = next(e for e in root.iter() if e.get("data-role") == "occurrence")
        self.assertEqual(binder.tag.rsplit("}", 1)[-1], "line")
        self.assertEqual(binder.get("y1"), binder.get("y2"))
        self.assertEqual(occurrence.get("x1"), occurrence.get("x2"))
        self.assertEqual(occurrence.get("y1"), binder.get("y1"))
        self.assertEqual(occurrence.get("data-binder-ref"), "root")

    def test_tromp_shadowing_and_unused_binder(self):
        grammar, pack = lambda_pack()
        model = core.term_model(grammar, term("Lam", {"name": "x"}, term("Lam", {"name": "x"}, term("Var", {"name": "x"}))))
        root = ET.fromstring(render_svg(grammar, model, pack))
        binders = [e for e in root.iter() if e.get("data-role") == "binder"]
        occurrence = next(e for e in root.iter() if e.get("data-role") == "occurrence")
        self.assertEqual(len(binders), 2)
        self.assertEqual(occurrence.get("data-binder-ref"), "root.children.0")
        self.assertEqual(occurrence.get("y1"), binders[1].get("y1"))

    def test_tromp_application_links_leftmost_occurrences(self):
        grammar, pack = lambda_pack()
        var = term("Var", {"name": "x"})
        model = core.term_model(grammar, term("Lam", {"name": "x"}, term("App", {}, var, deepcopy(var))))
        root = ET.fromstring(render_svg(grammar, model, pack))
        occurrences = [e for e in root.iter() if e.get("data-role") == "occurrence"]
        application = next(e for e in root.iter() if e.get("data-role") == "application")
        self.assertEqual(application.get("x1"), occurrences[0].get("x1"))
        self.assertEqual(application.get("x2"), occurrences[1].get("x1"))
        self.assertEqual(application.get("y1"), application.get("y2"))

    def test_tromp_free_name_and_collapsed_subterm_refused(self):
        grammar, pack = lambda_pack()
        model = core.term_model(grammar, term("Var", {"name": "free"}))
        with self.assertRaisesRegex(core.NotationError, "unbound"):
            render_svg(grammar, model, pack)
        closed = core.term_model(grammar, term("Lam", {"name": "x"}, term("Var", {"name": "x"})))
        with self.assertRaisesRegex(core.NotationError, "collapsed"):
            render_svg(grammar, closed, pack, folded=("root",))

    def test_alpha_rename_preserves_tromp_line_geometry(self):
        grammar, pack = lambda_pack()
        geometry = []
        for name in ("x", "renamed"):
            model = core.term_model(grammar, term("Lam", {"name": name}, term("Var", {"name": name})))
            root = ET.fromstring(render_svg(grammar, model, pack))
            geometry.append([(line.get("data-role"), line.get("x1"), line.get("y1"), line.get("x2"), line.get("y2")) for line in root.findall(".//s:line", NS)])
        self.assertEqual(*geometry)

    def test_graph_wavy_edge_uses_declared_ports(self):
        grammar, model, pack = graph_example()
        root = ET.fromstring(render_svg(grammar, model, pack))
        edge = next(e for e in root.iter() if e.get("data-role") == "edge")
        self.assertEqual(edge.get("data-edge-style"), "wavy")
        self.assertGreater(edge.get("d").count("L"), 20)
        self.assertEqual(edge.get("data-model-ref"), "edges.msg")
        self.assertEqual(len([e for e in root.iter() if e.get("data-role") == "port"]), 4)

    def test_graph_dangling_edge_refused(self):
        grammar, model, pack = graph_example()
        model["body"]["edges"][0]["target"][0] = "missing"
        with self.assertRaises(core.NotationError):
            render_svg(grammar, model, pack)

    def test_layout_budget_refuses_before_huge_output(self):
        grammar = expression_grammar()
        tree = term("Number", {"value": 1})
        for _ in range(6):
            tree = term("Add", {}, deepcopy(tree), deepcopy(tree))
        model = core.term_model(grammar, tree)
        with self.assertRaisesRegex(core.NotationError, "width budget"):
            render_svg(grammar, model, tree_pack(grammar))
        self.assertIn("retained nodes", render_svg(grammar, model, tree_pack(grammar), folded=("root",)))


if __name__ == "__main__":
    unittest.main()
