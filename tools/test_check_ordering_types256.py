"""Gate controls; synthetic outputs are not mathematical verification evidence."""
import copy
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest import mock

import check_ordering_types256 as gate


class SourceAdmission(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "source"
        for path in gate.SOURCES:
            target = self.root / path
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(gate.ROOT / path, target)
        self.profile = json.loads((self.root / gate.PROFILE).read_text())

    def test_exact_current_source_and_all_configurations(self):
        admitted = gate.configuration(self.root, self.profile)
        self.assertEqual(len(admitted), 8)
        self.assertEqual(sum(map(len, admitted.values())), 10)

    def test_drift_refuses_before_tools(self):
        with (self.root / gate.SUBJECT).open("a") as stream:
            stream.write("\n")
        with mock.patch.object(gate.verifier, "prepare_tools") as prepare:
            with self.assertRaisesRegex(ValueError, "source changed"):
                gate.check(Path("unused"), False, Path(self.temporary.name), self.root)
            prepare.assert_not_called()

    def test_native_only_return_refuses_even_with_updated_hash(self):
        path = self.root / gate.SUBJECT
        source = path.read_text().replace("        Self(words)",
            "        #[cfg(not(verus_keep_ghost))]\n        return Self([0, 0]);\n        Self(words)")
        path.write_text(source)
        self.profile["source_hashes"][str(gate.SUBJECT)] = gate.verifier.digest(path)
        with self.assertRaises(gate.verifier.ConfigurationRefused):
            gate.configuration(self.root, self.profile)

    def test_added_assumption_refuses_even_with_updated_hash(self):
        path = self.root / gate.MODEL
        path.write_text(path.read_text() + "\nverus! { proof fn escape() { assume(false); } }\n")
        self.profile["source_hashes"][str(gate.MODEL)] = gate.verifier.digest(path)
        with self.assertRaisesRegex(ValueError, "proof escape"):
            gate.configuration(self.root, self.profile)

    def test_unused_or_relocated_config_exception_refuses(self):
        for mode in ("unused", "relocated"):
            profile = copy.deepcopy(self.profile)
            entries = profile["native_configuration_exceptions"][str(gate.SUBJECT)]
            if mode == "unused":
                entries.append({"text": "#[cfg(feature = \"unknown\")]", "following": "fn fake()",
                                "reason": "A deliberately stale exception."})
            else:
                entries[0]["following"] = "pub struct DifferentType;"
            with self.subTest(mode=mode), self.assertRaises((ValueError, RuntimeError)):
                gate.configuration(self.root, profile)

    def test_symbolic_link_input_refuses(self):
        path = self.root / gate.SUBJECT
        path.unlink()
        path.symlink_to(gate.ROOT / gate.SUBJECT)
        with self.assertRaisesRegex(ValueError, "symbolic-link"):
            gate.configuration(self.root, self.profile)

    def test_both_cmp_methods_have_distinct_correct_regions(self):
        source = (self.root / gate.SUBJECT).read_text()
        unsigned = gate.method_region(source, "U256", "cmp")
        signed = gate.method_region(source, "I256", "cmp")
        self.assertLess(unsigned[1], signed[0])
        lines = source.splitlines()
        self.assertIn("        compare(&self.0, &other.0)", lines[unsigned[0] - 1:unsigned[1]])
        self.assertIn("            (true, false) => Ordering::Less,", lines[signed[0] - 1:signed[1]])

    def test_seven_controls_change_only_the_intended_actual_method(self):
        original = (self.root / gate.SUBJECT).read_text().splitlines(keepends=True)
        controls = gate.control_sources(self.root, self.profile)
        self.assertEqual(len(controls), 7)
        self.assertEqual(len({changed for _, changed in controls}), 7)
        for control, changed in controls:
            first, last = gate.method_region("".join(original), control["type"], control["method"])
            altered = changed.splitlines(keepends=True)
            self.assertEqual(original[:first - 1], altered[:first - 1])
            self.assertEqual(original[last:], altered[last:])
            self.assertNotEqual(original[first - 1:last], altered[first - 1:last])

    def test_incomplete_duplicate_noop_and_unknown_controls_refuse(self):
        for mode in ("missing", "duplicate", "noop", "unknown"):
            profile = copy.deepcopy(self.profile)
            if mode == "missing":
                profile["controls"].pop()
            elif mode == "duplicate":
                profile["controls"][1] = profile["controls"][0]
            elif mode == "noop":
                profile["controls"][0]["new"] = profile["controls"][0]["old"]
            else:
                profile["controls"][0]["old"] = "not in this method"
            with self.subTest(mode=mode), self.assertRaises(ValueError):
                gate.control_sources(self.root, profile)


class ProofAdmission(unittest.TestCase):
    def setUp(self):
        self.profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        self.pin = {"version": "fixture", "commit": "fixture-commit", "rust_toolchain": "fixture-toolchain"}
        aliases = {m["vir"]: m["display"] for m in
                   (*self.profile["methods"], *self.profile["view_aliases"])}
        self.report = {"verus": {**self.pin, "toolchain": self.pin["rust_toolchain"]},
                       "verification-results": {"success": True, "verified": 21, "errors": 0,
                            "encountered-error": False, "encountered-vir-error": False,
                            "is-verifying-entire-crate": True},
                       "func-details": {aliases.get(n, n): {} for n in self.profile["functions"]}}
        self.proc = subprocess.CompletedProcess([], 0, "", "")

    def test_complete_report_uses_display_method_names(self):
        self.assertTrue(gate.positive_accepted(self.proc, self.report, self.pin, self.profile))

    def test_filtered_partial_wrong_pin_and_extra_function_refuse(self):
        for mode in ("filtered", "partial", "pin", "extra", "missing", "exit"):
            report = copy.deepcopy(self.report)
            proc = self.proc
            if mode == "filtered":
                report["verification-results"]["is-verifying-entire-crate"] = False
            elif mode == "partial":
                report["verification-results"]["verified"] = 20
            elif mode == "pin":
                report["verus"]["commit"] = "other"
            elif mode == "extra":
                report["func-details"][gate.NAMESPACE + "unreviewed"] = {}
            elif mode == "missing":
                report["func-details"].pop(self.profile["methods"][0]["display"])
            else:
                proc = subprocess.CompletedProcess([], 1, "", "")
            with self.subTest(mode=mode):
                self.assertFalse(gate.positive_accepted(proc, report, self.pin, self.profile))

    def typed_nodes(self):
        def parameter(t):
            return ["Param", ":typ", t, ":mode", "Exec", ":user_mut", "false"]
        return [["Function", ["Fun", ":path", m["vir"]],
                 ":params", [parameter(gate.expected_type(t)) for t in m["parameters"]],
                 ":ret", parameter(gate.expected_type(m["result"]))]
                for m in self.profile["methods"]]

    def check_fixture(self, profile, actual, nodes):
        with mock.patch.object(gate, "inventory", return_value=actual), \
                mock.patch.object(gate, "parse_vir", return_value=nodes):
            return gate.check_inventory("fixture", profile)

    def test_full_inventory_and_typed_method_shapes(self):
        self.check_fixture(self.profile, copy.deepcopy(self.profile["functions"]), self.typed_nodes())

    def test_narrow_word_array_refuses_despite_matching_fingerprints(self):
        nodes = self.typed_nodes()
        # Deliberately permit the hash check, then alter the actual argument type.
        nodes[0][3][0][2][3][0] = ["Typ", "Int", ["IntRange", "U", "8"]]
        with self.assertRaisesRegex(ValueError, "full-width types"):
            self.check_fixture(self.profile, copy.deepcopy(self.profile["functions"]), nodes)

    def test_hidden_input_premise_refuses_even_with_updated_profile(self):
        profile = copy.deepcopy(self.profile)
        profile["functions"][profile["methods"][0]["vir"]]["requires"] = 1
        with self.assertRaisesRegex(ValueError, "conditional"):
            self.check_fixture(profile, copy.deepcopy(profile["functions"]), self.typed_nodes())

    def test_omitted_body_and_added_function_refuse_even_with_updated_profile(self):
        for mode in ("body", "function"):
            profile = copy.deepcopy(self.profile)
            if mode == "body":
                profile["functions"][profile["methods"][0]["vir"]].pop("body_sha256")
            else:
                profile["functions"][gate.NAMESPACE + "new_spec"] = {
                    "mode": "Spec", "requires": 0, "ensures": 0, "body_sha256": "fixture"}
            with self.subTest(mode=mode), self.assertRaisesRegex(ValueError, "body coverage"):
                self.check_fixture(profile, copy.deepcopy(profile["functions"]), self.typed_nodes())

    def test_duplicate_typed_method_binding_refuses(self):
        profile = copy.deepcopy(self.profile)
        profile["methods"][-1] = profile["methods"][-2]
        with self.assertRaisesRegex(ValueError, "duplicate or missing"):
            self.check_fixture(profile, copy.deepcopy(profile["functions"]), self.typed_nodes())

    def test_every_solver_condition_requires_its_exact_source_function_and_line(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            def render(section):
                return (f";; Function-Def {section['function']}\n"
                        f";; {root / section['source']}:{section['line']}:1: end\n (check-sat)\n")
            valid = "".join(map(render, self.profile["smt_sections"]))
            path = root / "all.smt2"
            path.write_text(valid)
            self.assertEqual(gate.smt_coverage(root, root, self.profile)["conditions"], 21)
            variants = [valid.replace(" (check-sat)\n", "", 1), valid + render(self.profile["smt_sections"][0]),
                        valid.replace(self.profile["smt_sections"][0]["source"], "copied.rs", 1),
                        valid.replace(";; Function-Def ", ";; Function-Def wrong::", 1)]
            for text in variants:
                with self.subTest(text=text[:100]):
                    path.write_text(text)
                    with self.assertRaises(ValueError):
                        gate.smt_coverage(root, root, self.profile)


if __name__ == "__main__":
    unittest.main()
