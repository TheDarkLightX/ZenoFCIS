"""Gate admission controls; synthetic reports are not mathematical proof evidence."""
import copy
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest import mock

import check_numeric256 as gate
import record_numeric256_profile as producer


class SourceAdmission(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "source"
        for path in (*gate.SOURCES, Path("verification/verus/ordering_types256_profile.json")):
            target = self.root / path
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(gate.ROOT / path, target)
        self.profile = json.loads((self.root / gate.PROFILE).read_text())

    def test_all_source_and_configuration_bindings(self):
        self.assertEqual(set(gate.configuration(self.root, self.profile)), {str(p) for p in gate.UNIT})

    def test_source_drift_refuses_before_tool_preparation(self):
        path = self.root / gate.SUBJECT
        path.write_text(path.read_text() + "\n")
        with mock.patch.object(gate.verifier, "prepare_tools") as prepare:
            with self.assertRaisesRegex(ValueError, "source changed"):
                gate.check(Path("unused"), False, Path(self.temporary.name), self.root)
            prepare.assert_not_called()

    def test_unknown_native_configuration_refuses_even_with_refreshed_hash(self):
        path = self.root / gate.SUBJECT
        path.write_text(path.read_text() + "\n#[cfg(not(verus_keep_ghost))]\nfn hidden() {}\n")
        self.profile["source_hashes"][str(gate.SUBJECT)] = gate.verifier.digest(path)
        with self.assertRaises(gate.verifier.ConfigurationRefused):
            gate.configuration(self.root, self.profile)

    def test_assumption_refuses_even_with_refreshed_hash(self):
        path = self.root / gate.MODEL
        path.write_text(path.read_text() + "\nverus! { proof fn escape() { assume(false); } }\n")
        self.profile["source_hashes"][str(gate.MODEL)] = gate.verifier.digest(path)
        with self.assertRaisesRegex(ValueError, "proof escape"):
            gate.configuration(self.root, self.profile)

    def test_missing_and_symbolic_sources_refuse(self):
        path = self.root / gate.SUBJECT
        path.unlink()
        with self.assertRaises(ValueError):
            gate.configuration(self.root, self.profile)
        path.symlink_to(gate.ROOT / gate.SUBJECT)
        with self.assertRaisesRegex(ValueError, "symbolic-link"):
            gate.configuration(self.root, self.profile)

    def test_missing_or_duplicate_obligation_refuses(self):
        for mode in ("missing", "duplicate"):
            profile = copy.deepcopy(self.profile)
            if mode == "missing":
                profile["obligations"].pop(0)
            else:
                profile["obligations"].append(profile["obligations"][0])
            with self.subTest(mode=mode), self.assertRaises(ValueError):
                gate.configuration(self.root, profile)

    def test_conversion_stage_and_its_dependencies_must_be_complete(self):
        complete = copy.deepcopy(self.profile)
        producer.include_conversions(complete)
        gate.configuration(self.root, complete)
        for mode in ("conversion", "dependency", "duplicate"):
            profile = copy.deepcopy(complete)
            if mode == "conversion":
                profile["obligations"].pop()
            elif mode == "dependency":
                profile["supporting_obligations"].pop()
            else:
                profile["supporting_obligations"].append(profile["supporting_obligations"][0])
            with self.subTest(mode=mode), self.assertRaises(ValueError):
                gate.configuration(self.root, profile)

    def test_controls_change_only_the_declared_production_expression(self):
        original = (self.root / gate.SUBJECT).read_text()
        for control, changed in gate.control_sources(self.root, self.profile):
            first, last = control["error_region"]
            lines = original.splitlines(keepends=True)
            selected = "".join(lines[first - 1:last])
            self.assertEqual(selected.count(control["old"]), 1)
            self.assertEqual(changed, "".join(lines[:first - 1])
                             + selected.replace(control["old"], control["new"], 1)
                             + "".join(lines[last:]))
            profile = copy.deepcopy(self.profile)
            for entry in profile["native_configuration_exceptions"][str(gate.SUBJECT)]:
                entry["following"] = entry["following"].replace(control["old"], control["new"])
            (self.root / gate.SUBJECT).write_text(changed)
            hashes = {**profile["source_hashes"], str(gate.SUBJECT): gate.verifier.digest(self.root / gate.SUBJECT)}
            gate.configuration(self.root, profile, hashes)
        (self.root / gate.SUBJECT).write_text(original)

    def test_incomplete_duplicate_noop_and_unbound_controls_refuse(self):
        for mode in ("incomplete", "duplicate", "noop", "unbound", "ambiguous", "target"):
            profile = copy.deepcopy(self.profile)
            if mode == "incomplete":
                profile["controls"] = profile["controls"][:2]
            elif mode == "duplicate":
                profile["controls"][1] = profile["controls"][0]
            elif mode == "noop":
                profile["controls"][0]["new"] = profile["controls"][0]["old"]
            elif mode == "unbound":
                profile["controls"][0]["member"] = "Unknown::method"
            elif mode == "ambiguous":
                profile["controls"][0]["old"] = "Self"
            else:
                profile["controls"][0]["error_function"] = "wrong::refinement"
            with self.subTest(mode=mode), self.assertRaises(ValueError):
                gate.control_sources(self.root, profile)

    def test_profile_producer_requires_whole_positive_proof(self):
        proc = subprocess.CompletedProcess([], 1, "", "")
        before = (self.root / gate.PROFILE).read_bytes()
        with mock.patch.object(gate.verifier, "prepare_tools", return_value=Path("fixture")), \
                mock.patch.object(gate, "prove", return_value=(proc, {})):
            with self.assertRaisesRegex(ValueError, "complete positive"):
                producer.record(self.root, Path("unused"), Path("unused"), False)
        self.assertEqual(before, (self.root / gate.PROFILE).read_bytes())


class ProofAdmission(unittest.TestCase):
    def setUp(self):
        self.profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        self.pin = {"version": "fixture", "commit": "fixture", "rust_toolchain": "fixture"}
        self.report = {"verus": {**self.pin, "toolchain": "fixture"},
                       "verification-results": {"success": True, "verified": self.profile["expected_verified"],
                           "errors": 0, "encountered-error": False, "encountered-vir-error": False,
                           "is-verifying-entire-crate": True},
                       "func-details": {n: {} for n in self.profile["report_functions"]}}
        self.proc = subprocess.CompletedProcess([], 0, "", "")

    def typed_nodes(self):
        def parameter(t):
            return ["Param", ":typ", t, ":mode", "Exec", ":user_mut", "false"]
        return [["Function", ["Fun", ":path", o["refinement"]],
                 ":params", [parameter(gate.expected_type(t)) for t in o["parameters"]],
                 ":ret", parameter(gate.expected_type(o["result"]))]
                for o in self.profile["obligations"] + self.profile.get("supporting_obligations", [])]

    def test_complete_report(self):
        self.assertTrue(gate.positive_accepted(self.proc, self.report, self.pin, self.profile))

    def test_filtered_incomplete_wrong_pin_extra_and_missing_functions_refuse(self):
        for mode in ("filtered", "count", "pin", "extra", "missing", "exit"):
            report = copy.deepcopy(self.report)
            proc = self.proc
            if mode == "filtered":
                report["verification-results"]["is-verifying-entire-crate"] = False
            elif mode == "count":
                report["verification-results"]["verified"] -= 1
            elif mode == "pin":
                report["verus"]["commit"] = "different"
            elif mode == "extra":
                report["func-details"][gate.NAMESPACE + "unreviewed"] = {}
            elif mode == "missing":
                report["func-details"].pop(next(iter(report["func-details"])))
            else:
                proc = subprocess.CompletedProcess([], 1, "", "")
            with self.subTest(mode=mode):
                self.assertFalse(gate.positive_accepted(proc, report, self.pin, self.profile))

    def test_complete_inventory(self):
        with mock.patch.object(gate, "inventory", return_value=self.profile["functions"]), \
                mock.patch.object(gate, "parse_vir", return_value=self.typed_nodes()):
            gate.check_inventory("fixture", self.profile)

    def test_omitted_body_added_function_and_narrowed_refinement_refuse(self):
        for mode in ("body", "function", "premise"):
            profile = copy.deepcopy(self.profile)
            name = profile["obligations"][0]["refinement"]
            if mode == "body":
                profile["functions"][name].pop("body_sha256")
            elif mode == "function":
                profile["functions"][gate.NAMESPACE + "extra"] = {"mode": "Proof", "body_sha256": "fixture"}
            else:
                profile["functions"][name]["requires"] = 1
            with self.subTest(mode=mode), mock.patch.object(gate, "inventory", return_value=profile["functions"]), \
                    mock.patch.object(gate, "parse_vir", return_value=self.typed_nodes()), \
                    self.assertRaises(ValueError):
                gate.check_inventory("fixture", profile)

    def test_narrow_constant_return_refuses_even_with_matching_fingerprints(self):
        nodes = self.typed_nodes()
        nodes[0][-1][2] = gate.expected_type("u128")
        with mock.patch.object(gate, "inventory", return_value=self.profile["functions"]), \
                mock.patch.object(gate, "parse_vir", return_value=nodes), \
                self.assertRaisesRegex(ValueError, "full-width types"):
            gate.check_inventory("fixture", self.profile)

    def test_narrow_result_payload_and_wrong_failure_type_refuse(self):
        for mode in ("payload", "failure"):
            nodes = self.typed_nodes()
            node = next(n for n in nodes if n[-1][2] == gate.expected_type("Result<U256,Failure>"))
            node[-1][2][3][0 if mode == "payload" else 1] = gate.expected_type("u128")
            with self.subTest(mode=mode), \
                    mock.patch.object(gate, "inventory", return_value=self.profile["functions"]), \
                    mock.patch.object(gate, "parse_vir", return_value=nodes), \
                    self.assertRaisesRegex(ValueError, "full-width types"):
                gate.check_inventory("fixture", self.profile)

    def test_diagnostic_identity_cannot_be_substituted_for_another_method(self):
        profile = copy.deepcopy(self.profile)
        profile["obligations"][0]["report_function"] = profile["obligations"][1]["report_function"]
        with mock.patch.object(gate, "inventory", return_value=profile["functions"]), \
                mock.patch.object(gate, "parse_vir", return_value=self.typed_nodes()), \
                self.assertRaisesRegex(ValueError, "diagnostic identity"):
            gate.check_inventory("fixture", profile)

    def test_exact_solver_sections_and_query_count(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            def render(s):
                return f";; Function-Def {s['function']}\n;; {root / s['source']}:{s['line']}:1: end\n(check-sat)\n"
            text = "".join(render(s) for s in self.profile["smt_sections"])
            path = root / "all.smt2"
            path.write_text(text)
            gate.smt_coverage(root, root, self.profile)
            for bad in (text.replace("(check-sat)\n", "", 1), text + render(self.profile["smt_sections"][0]),
                        text.replace(";; Function-Def ", ";; Function-Def wrong::", 1),
                        text.replace(self.profile["smt_sections"][0]["source"], "copied.rs", 1)):
                with self.subTest(bad=bad[:80]):
                    path.write_text(bad)
                    with self.assertRaises(ValueError):
                        gate.smt_coverage(root, root, self.profile)


if __name__ == "__main__":
    unittest.main()
