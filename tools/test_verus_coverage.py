"""Parser controls for concrete coverage and separately retained declarations."""
import copy
from pathlib import Path
import unittest
import verus_coverage as coverage

NAMESPACE = "projection_skeleton::"
PREFIX = NAMESPACE + "projection::"
FIXTURE = Path(__file__).parent / "test-data/descriptor-declarations.vir"
PRODUCTION_FIXTURE = Path(__file__).parent / "test-data/authority-descriptor-declarations.vir"


def emit(node):
    return "(" + " ".join(emit(child) for child in node) + ")" if isinstance(node, list) else node


class DescriptorDeclarations(unittest.TestCase):
    def setUp(self):
        self.source = FIXTURE.read_text()
        self.nodes = coverage.parse_vir(self.source)

    def changed(self, name, field, value):
        nodes = copy.deepcopy(self.nodes)
        node = next(node for node in nodes if node[1][2] == PREFIX + name)
        node[node.index(field) + 1] = value
        return "\n".join(emit(node) for node in nodes)

    def test_default_collector_still_refuses_abstract_declarations(self):
        with self.assertRaisesRegex(ValueError, "missing contract fields"):
            coverage.inventory(self.source, NAMESPACE)

    def test_all_six_records_retained_with_real_implementations_separate(self):
        functions, declarations = coverage.descriptor_inventory(self.source, NAMESPACE, PREFIX)
        self.assertEqual(len(functions), 4)
        self.assertEqual(len(declarations), 2)
        self.assertEqual(set(functions) | set(declarations), {node[1][2] for node in self.nodes})
        for name, record in functions.items():
            self.assertIn("body_sha256", record)
            self.assertEqual(record["requires"], 0)
        for name, record in declarations.items():
            raw = next(node for node in self.nodes if node[1][2] == name)
            self.assertEqual(record["raw_vir"], raw)
            self.assertEqual(record["raw_vir_sha256"], coverage.fingerprint(raw))
            self.assertNotIn("body_sha256", record)

    def test_missing_concrete_implementation_is_refused(self):
        source = "\n".join(emit(node) for node in self.nodes if node[1][2] != PREFIX + "impl&%3::project")
        with self.assertRaisesRegex(ValueError, "concrete descriptor implementation"):
            coverage.descriptor_inventory(source, NAMESPACE, PREFIX)

    def test_concrete_implementation_cannot_become_abstract(self):
        source = self.changed("impl&%2::project", ":body", "None")
        with self.assertRaisesRegex(ValueError, "missing contract fields"):
            coverage.descriptor_inventory(source, NAMESPACE, PREFIX)

    def test_unknown_bodyless_entry_is_refused(self):
        extra = copy.deepcopy(self.nodes[0]); extra[1][2] = PREFIX + "Unknown::model"
        with self.assertRaisesRegex(ValueError, "missing contract fields"):
            coverage.descriptor_inventory(self.source + "\n" + emit(extra), NAMESPACE, PREFIX)

    def test_narrowed_abstract_or_concrete_domain_is_refused(self):
        for name in ("DescriptorData::project", "impl&%3::project"):
            with self.subTest(name=name), self.assertRaises(ValueError):
                coverage.descriptor_inventory(self.changed(name, ":require", ["narrowed"]), NAMESPACE, PREFIX)

    def test_missing_declaration_is_refused(self):
        source = "\n".join(emit(node) for node in self.nodes if node[1][2] != PREFIX + "DescriptorData::model")
        with self.assertRaisesRegex(ValueError, "missing descriptor declaration"):
            coverage.descriptor_inventory(source, NAMESPACE, PREFIX)

    def test_raw_fingerprint_retains_changed_abstract_postcondition(self):
        _, before = coverage.descriptor_inventory(self.source, NAMESPACE, PREFIX)
        _, after = coverage.descriptor_inventory(self.changed("DescriptorData::project", ":ensure", ["changed"]), NAMESPACE, PREFIX)
        name = PREFIX + "DescriptorData::project"
        self.assertNotEqual(before[name]["raw_vir_sha256"], after[name]["raw_vir_sha256"])
        self.assertNotEqual(before[name]["ensures_sha256"], after[name]["ensures_sha256"])

    def production_profile(self):
        namespace = "authority_v2::"
        prefix = namespace + "execution_v2::composition::descriptor::"
        source = self.source.replace(PREFIX, prefix)
        functions, declarations = coverage.descriptor_inventory(source, namespace, prefix)
        bodies = sorted(n for n, r in functions.items() if r["mode"] == "Exec")
        return source, {
            "namespace": namespace,
            "functions": functions,
            "abstract_declarations": declarations,
            "body_covered_functions": bodies,
        }

    def test_reviewed_profile_checks_declarations_and_concrete_bodies(self):
        source, profile = self.production_profile()
        self.assertEqual(coverage.require_coverage(source, profile), profile["functions"])
        omitted = copy.deepcopy(profile)
        del omitted["abstract_declarations"]
        with self.assertRaisesRegex(ValueError, "missing contract fields"):
            coverage.require_coverage(source, omitted)

    def test_changed_abstract_postcondition_cannot_keep_old_coverage(self):
        _source, profile = self.production_profile()
        prefix = profile["namespace"] + "execution_v2::composition::descriptor::"
        changed = self.changed("DescriptorData::project", ":ensure", ["changed"]).replace(PREFIX, prefix)
        with self.assertRaisesRegex(ValueError, "descriptor declarations changed"):
            coverage.require_coverage(changed, profile)

    def test_corrupt_declaration_receipt_is_refused(self):
        source, profile = self.production_profile()
        name = next(iter(profile["abstract_declarations"]))
        profile["abstract_declarations"][name]["raw_vir_sha256"] = "unreviewed"
        with self.assertRaisesRegex(ValueError, "descriptor declarations changed"):
            coverage.require_coverage(source, profile)

    def test_observed_inventory_retains_declarations_only_where_they_exist(self):
        source, profile = self.production_profile()
        functions, declarations = coverage.observed_inventory(source, profile["namespace"],
                                                              tuple(profile["body_covered_functions"]))
        self.assertEqual((functions, declarations), (profile["functions"], profile["abstract_declarations"]))
        self.assertEqual(coverage.profile_inventory(source, profile), profile["functions"])
        concrete = "\n".join(emit(node) for node in coverage.parse_vir(source)
                             if not node[1][2].endswith(("DescriptorData::model", "DescriptorData::project")))
        functions, declarations = coverage.observed_inventory(concrete, profile["namespace"])
        self.assertIsNone(declarations)
        self.assertEqual(set(functions), set(profile["functions"]))
        # A profile without the declarations keeps the strict inventory.
        strict = {key: value for key, value in profile.items() if key != "abstract_declarations"}
        with self.assertRaisesRegex(ValueError, "missing contract fields"):
            coverage.profile_inventory(source, strict)


class ProductionDescriptorDeclarations(unittest.TestCase):
    def setUp(self):
        self.namespace = "authority_v2::"
        self.prefix = self.namespace + "execution_v2::composition::descriptor::"
        self.source = PRODUCTION_FIXTURE.read_text()
        self.nodes = coverage.parse_vir(self.source)

    def changed(self, suffix, field, value):
        nodes = copy.deepcopy(self.nodes)
        node = next(node for node in nodes if node[1][2] == self.prefix + suffix)
        node[node.index(field) + 1] = value
        return "\n".join(emit(node) for node in nodes)

    def test_actual_translation_retains_two_declarations_and_four_bodies(self):
        namespace = "authority_v2::"
        prefix = namespace + "execution_v2::composition::descriptor::"
        source = PRODUCTION_FIXTURE.read_text()
        functions, declarations = coverage.descriptor_inventory(source, namespace, prefix)
        self.assertEqual(set(functions), {
            prefix + f"impl&%{index}::{method}"
            for index in (2, 3) for method in ("model", "project")
        })
        self.assertEqual(set(declarations), {
            prefix + "DescriptorData::model", prefix + "DescriptorData::project"
        })
        for record in functions.values():
            self.assertIn("body_sha256", record)
            self.assertEqual(record["requires"], 0)
            self.assertEqual(record["ensures"], 1 if record["mode"] == "Exec" else 0)

    def test_other_implementation_ids_cannot_replace_required_projections(self):
        namespace = "authority_v2::"
        prefix = namespace + "execution_v2::composition::descriptor::"
        source = PRODUCTION_FIXTURE.read_text()
        for before, after in ((2, 0), (3, 1)):
            with self.subTest(index=before), self.assertRaisesRegex(
                ValueError, "concrete descriptor implementation"
            ):
                coverage.descriptor_inventory(
                    source.replace(f"impl&%{before}::", f"impl&%{after}::"),
                    namespace, prefix
                )

    def test_each_of_the_six_required_records_cannot_be_omitted(self):
        for missing in self.nodes:
            name = missing[1][2]
            source = "\n".join(emit(node) for node in self.nodes if node[1][2] != name)
            with self.subTest(name=name), self.assertRaises(ValueError):
                coverage.descriptor_inventory(source, self.namespace, self.prefix)

    def test_concrete_project_contract_and_body_cannot_be_weakened(self):
        for index in (2, 3):
            name = f"impl&%{index}::project"
            for field, value in ((":body", "None"), (":require", ["narrowed"]),
                                 (":ensure", []), (":ensure", ["first", "second"])):
                with self.subTest(name=name, field=field, value=value), self.assertRaises(ValueError):
                    coverage.descriptor_inventory(
                        self.changed(name, field, value), self.namespace, self.prefix
                    )

    def test_declarations_cannot_gain_bodies_or_unknown_declarations(self):
        for name in ("DescriptorData::model", "DescriptorData::project"):
            with self.subTest(name=name), self.assertRaises(ValueError):
                coverage.descriptor_inventory(
                    self.changed(name, ":body", ["unexpected-body"]), self.namespace, self.prefix
                )
        extra = copy.deepcopy(self.nodes[0])
        extra[1][2] = self.prefix + "Unreviewed::model"
        with self.assertRaises(ValueError):
            coverage.descriptor_inventory(
                self.source + "\n" + emit(extra), self.namespace, self.prefix
            )

    def test_changed_receiver_or_swapped_bodies_cannot_keep_the_old_profile(self):
        functions, declarations = coverage.descriptor_inventory(
            self.source, self.namespace, self.prefix
        )
        profile = {"namespace": self.namespace, "functions": functions,
                   "abstract_declarations": declarations,
                   "body_covered_functions": sorted(
                       name for name, record in functions.items() if record["mode"] == "Exec"
                   )}
        nodes = copy.deepcopy(self.nodes)
        projects = [next(node for node in nodes if node[1][2] == self.prefix + f"impl&%{i}::project")
                    for i in (2, 3)]
        for field in (":params", ":body"):
            changed = copy.deepcopy(nodes)
            left, right = [next(node for node in changed if node[1][2] == project[1][2])
                           for project in projects]
            a, b = left.index(field) + 1, right.index(field) + 1
            left[a], right[b] = right[b], left[a]
            with self.subTest(field=field), self.assertRaises(ValueError):
                coverage.require_coverage("\n".join(emit(node) for node in changed), profile)


if __name__ == "__main__":
    unittest.main()
