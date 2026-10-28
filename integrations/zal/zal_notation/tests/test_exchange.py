import copy
import unittest

from zal_notation.builtin import generic_pack, get_builtin, model, term
from zal_notation.core import NotationError, validate_model, validate_pack
from zal_notation.exchange import audit_svg, recover_source
from zal_notation.render import render_svg


class ExchangeTests(unittest.TestCase):
    def test_exact_source_roundtrip_and_replay(self):
        for name in ("lambda", "state-machine", "zal-folds", "petri-net"):
            g, p, m = get_builtin(name)
            svg = render_svg(g, m, p)
            restored = recover_source(svg)
            self.assertEqual((restored["grammar"], restored["notation"], restored["model"]), (g, p, m))
            self.assertEqual(audit_svg(svg)["status"], "matches-current-renderer")

    def test_geometry_drift_is_not_metadata_proof(self):
        g, p, m = get_builtin("lambda")
        svg = render_svg(g, m, p)
        changed = svg.replace('stroke-width="1.6"', 'stroke-width="9"', 1)
        self.assertNotEqual(svg, changed)
        self.assertEqual(recover_source(changed)["model"], m)
        self.assertEqual(audit_svg(changed)["status"], "rendering-drift")

    def test_source_tampering_refused(self):
        g, p, m = get_builtin("zal-folds")
        svg = render_svg(g, m, p)
        changed = svg.replace('&quot;amount&quot;:&quot;q&quot;', '&quot;amount&quot;:&quot;2q&quot;', 1)
        self.assertNotEqual(svg, changed)
        with self.assertRaises(NotationError):
            recover_source(changed)

    def test_entities_and_duplicate_metadata_refused(self):
        with self.assertRaises(NotationError):
            recover_source('<!DOCTYPE svg [<!ENTITY a "xxx">]><svg xmlns="http://www.w3.org/2000/svg">&a;</svg>')
        g, p, m = get_builtin("lambda")
        svg = render_svg(g, m, p)
        changed = svg.replace("</svg>", '<metadata id="zal-source">{}</metadata></svg>')
        with self.assertRaises(NotationError):
            recover_source(changed)

    def test_combined_source_budget_refused_before_unrecoverable_export(self):
        # These three documents are individually admissible.  Their aggregate
        # exceeds the same structure limit enforced when recovering metadata.
        # Previously this emitted a 751 KB SVG which could not be recovered.
        fields = {f"f{i}": "bool" for i in range(32)}
        grammar = {
            "schema": "zal.grammar/1", "id": "test/recover-budget",
            "version": 1, "kind": "term", "root": "S",
            "constructors": {f"N{i}": {"result": "S", "fields": fields.copy(),
                                       "variadic": "S"} for i in range(128)},
        }
        values = {name: False for name in fields}
        document = model(grammar, term("N0", *[
            term("N0", **values) for _ in range(511)], **values))
        notation = generic_pack(grammar)
        for glyph in notation["glyphs"].values():
            glyph["primitives"] = [copy.deepcopy(glyph["primitives"][0])
                                   for _ in range(8)]
        validate_model(grammar, document)
        validate_pack(grammar, notation)
        with self.assertRaisesRegex(NotationError, "JSON structure budget exceeded"):
            render_svg(grammar, document, notation, folded=("root",))


if __name__ == "__main__":
    unittest.main()
