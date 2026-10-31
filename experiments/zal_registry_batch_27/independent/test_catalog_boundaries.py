"""Independent negative and ownership tests, active with Python -O as well.

Run against a pinned source root without adding files to its source-bound
notation package. Synthetic suppliers are scoped and restored in every test.
"""
from __future__ import annotations

import argparse
import builtins
from copy import deepcopy
import json
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from catalog_sizes import FIELDS, suppliers

V = L = None


class CatalogBoundaryTests(unittest.TestCase):
    def test_no_mutable_alias_between_rows_fields_catalogs_or_adapter_constants(self):
        first, second = V.default_registry(), V.default_registry()
        original = V.core.canonical(second)
        constants = deepcopy((L.SPECS, L.ASSUMPTIONS))
        mutable = [first, first["definitions"], *first["definitions"],
                   *(row[field] for row in first["definitions"] for field in FIELDS)]
        self.assertEqual(len({id(x) for x in mutable}), len(mutable))
        other = [second, second["definitions"], *second["definitions"],
                 *(row[field] for row in second["definitions"] for field in FIELDS)]
        self.assertTrue({id(x) for x in mutable}.isdisjoint({id(x) for x in other}))
        for row in first["definitions"]:
            for field in FIELDS:
                row[field].append("Consumer-local mutation")
        self.assertEqual(V.core.canonical(second), original)
        self.assertEqual(V.core.canonical(V.default_registry()), original)
        self.assertEqual((L.SPECS, L.ASSUMPTIONS), constants)

    def test_register_success_owns_registry_and_proposed_fields(self):
        base = V.default_registry()
        proposed = deepcopy(base["definitions"][0])
        proposed["id"] = "independent-custom"
        before = deepcopy(base)
        result = V.register(base, proposed)
        self.assertEqual(base, before)
        for row in base["definitions"]:
            for field in FIELDS:
                row[field].append("caller base mutation")
        for field in FIELDS:
            proposed[field].append("caller proposal mutation")
        self.assertEqual(result["definitions"][:-1], before["definitions"])
        self.assertNotEqual(result["definitions"][-1], proposed)

    def test_register_refusal_preserves_both_inputs_and_registry_error_precedence(self):
        base = V.default_registry()
        proposed = deepcopy(base["definitions"][0])
        base["schema"] = "wrong-registry"
        proposed["schema"] = "wrong-definition"
        before = deepcopy((base, proposed))
        with self.assertRaisesRegex(V.core.NotationError, "unsupported lens registry"):
            V.register(base, proposed)
        self.assertEqual((base, proposed), before)

    def test_registration_refusal_on_duplicate_does_not_mutate_inputs(self):
        base = V.default_registry()
        proposed = deepcopy(base["definitions"][0])
        before = deepcopy((base, proposed))
        with self.assertRaisesRegex(V.core.NotationError, "duplicate lens id"):
            V.register(base, proposed)
        self.assertEqual((base, proposed), before)

    def test_fixed_supplier_duplicate_cannot_escape_final_validation(self):
        rows = V._structural_definitions()
        rows.append(deepcopy(rows[0]))
        before = deepcopy(rows)
        with patch.object(V, "_structural_definitions", return_value=rows):
            with self.assertRaisesRegex(V.core.NotationError, "duplicate lens id"):
                V.default_registry()
        self.assertEqual(rows, before)

    def test_optional_supplier_duplicate_cannot_escape_final_validation(self):
        specs = deepcopy(L.SPECS)
        specs["typed-category"] = next(iter(specs.values()))
        with patch.object(L, "SPECS", specs):
            with self.assertRaisesRegex(V.core.NotationError, "duplicate lens id"):
                V.default_registry()

    def test_malformed_fixed_supplier_is_refused_without_mutation(self):
        rows = V._structural_definitions()
        rows[0]["assumptions"] = {"not": "a list"}
        before = deepcopy(rows)
        with patch.object(V, "_structural_definitions", return_value=rows):
            with self.assertRaisesRegex(V.core.NotationError, "lens declaration list budget exceeded"):
                V.default_registry()
        self.assertEqual(rows, before)

    def test_malformed_optional_spec_is_not_treated_as_import_absence(self):
        specs = deepcopy(L.SPECS)
        value = list(next(iter(specs.values())))
        value[2] = True
        specs[next(iter(specs))] = tuple(value)
        with patch.object(L, "SPECS", specs):
            with self.assertRaises(V.core.NotationError):
                V.default_registry()

    def test_registry_boundary_64_succeeds_65_refuses(self):
        with suppliers(V, L, 64):
            self.assertEqual(len(V.default_registry()["definitions"]), 64)
        with suppliers(V, L, 65):
            with self.assertRaisesRegex(V.core.NotationError, "lens registry budget exceeded"):
                V.default_registry()

    def test_import_fallback_is_valid_fresh_and_bounded(self):
        original_import = builtins.__import__
        for kind in (ImportError, ValueError):
            with self.subTest(exception=kind.__name__):
                def imported(name, globals=None, locals=None, fromlist=(), level=0):
                    if level == 1 and "lenses" in (fromlist or ()):
                        raise kind("optional finite adapter unavailable")
                    return original_import(name, globals, locals, fromlist, level)
                with patch.object(builtins, "__import__", side_effect=imported):
                    first, second = V.default_registry(), V.default_registry()
                    self.assertEqual([r["id"] for r in first["definitions"]],
                        ["inductive-syntax", "relational-graph", "sequence-chunks", "typed-category"])
                    V.validate_registry(first)
                    first["definitions"][0]["assumptions"].append("local edit")
                    self.assertNotEqual(first, second)
                    self.assertEqual(second, V.default_registry())

    def test_unexpected_import_failure_propagates(self):
        original_import = builtins.__import__
        def imported(name, globals=None, locals=None, fromlist=(), level=0):
            if level == 1 and "lenses" in (fromlist or ()):
                raise RuntimeError("unexpected adapter failure")
            return original_import(name, globals, locals, fromlist, level)
        with patch.object(builtins, "__import__", side_effect=imported):
            with self.assertRaisesRegex(RuntimeError, "unexpected adapter failure"):
                V.default_registry()

    def test_annotation_or_claim_edits_never_borrow_builtin_admission(self):
        for row in V.default_registry()["definitions"]:
            for field in ("summary", *FIELDS):
                with self.subTest(lens=row["id"], field=field):
                    altered = deepcopy(row)
                    if field == "summary":
                        altered[field] += " Consumer annotation."
                    else:
                        altered[field].append("Consumer annotation.")
                    self.assertIsNone(V._adapter(altered))


def main():
    global V, L
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True, type=Path)
    parser.add_argument("--record", required=True, type=Path)
    args = parser.parse_args()
    sys.path.insert(0, str(args.root.resolve() / "integrations/zal"))
    from zal_notation import lens_library, lenses
    V, L = lens_library, lenses
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(CatalogBoundaryTests)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    record = {"schema": "zal/independent-catalog-boundary-tests/1", "tests_run": result.testsRun,
              "failures": len(result.failures), "errors": len(result.errors), "pass": result.wasSuccessful(),
              "python_optimize": sys.flags.optimize,
              "failure_details": [(str(case), detail) for case, detail in result.failures + result.errors]}
    with args.record.open("x") as stream:
        stream.write(json.dumps(record, indent=2, sort_keys=True) + "\n")
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
