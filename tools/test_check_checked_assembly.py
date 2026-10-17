"""Admission tests for the checked-assembly gate; no proof result is claimed."""

import copy
import json
from pathlib import Path
import re
import subprocess
import unittest

import check_checked_assembly as gate


class Profile(unittest.TestCase):
    def setUp(self):
        self.profile = json.loads((gate.ROOT / gate.PROFILE).read_text())

    def test_profile_names_its_targets_and_inventory(self):
        self.assertEqual(self.profile["namespaces"], list(gate.NAMESPACES))
        self.assertTrue(set(self.profile["target_functions"]) <= set(self.profile["functions"]))
        self.assertEqual(gate.counts(self.profile["functions"]), self.profile["inventory"])
        self.assertEqual(self.profile["inventory"]["Exec"], 0)
        self.assertTrue(all(name.startswith(gate.NAMESPACES) for name in self.profile["functions"]))
        self.assertLess(self.profile["module_verified"], self.profile["expected_verified"])

    def test_proof_modules_hold_no_trusted_declaration(self):
        sources = {path: (gate.ROOT / path).read_text() for path in (gate.HARNESS, *gate.PROOFS)}
        self.assertEqual(gate.forbidden_tokens(sources), [])
        harness = dict(sources)
        harness[gate.HARNESS] = sources[gate.HARNESS] + "\n#[verifier::external_body]\n"
        self.assertEqual(len(gate.forbidden_tokens(harness)), 1)
        for token in ("assume(false);", "admit();", "#[verifier::external_body]",
                      "#[verifier(external)]", "unsafe { }"):
            with self.subTest(token=token):
                changed = dict(sources)
                changed[gate.ROUTE] = sources[gate.ROUTE] + "\n" + token + "\n"
                self.assertEqual(len(gate.forbidden_tokens(changed)), 1)


class TrustedBase(unittest.TestCase):
    """One trusted-base list: the composition result's, word for word."""

    def test_the_gate_and_the_documents_state_the_results_list(self):
        source = (gate.ROOT / "crates/zeno-fcis-contract/src/checked_composition.rs").read_text()
        block = re.search(r"pub const TRUSTED_BASE: \[&str; \d+\] = \[(.*?)\];", source, re.S)
        self.assertIsNotNone(block)
        listed = tuple(re.findall(r'"([^"]*)"', block[1]))
        self.assertEqual(listed, gate.SHARED_TRUSTED_BASE)

        def words(text: str) -> str:
            return " ".join(text.replace("\n- ", "\n").split())

        for document in ("verification/verus/README.md", "docs/CLI_REFERENCE.md"):
            text = words((gate.ROOT / document).read_text())
            for item in listed:
                with self.subTest(document=document, item=item[:40]):
                    self.assertIn(words(item), text)


class Controls(unittest.TestCase):
    def setUp(self):
        self.sources = {path: (gate.ROOT / path).read_text() for path in gate.PROOFS}

    def test_control_anchors_remain_applicable(self):
        controls = gate.control_sources(self.sources)
        self.assertEqual(set(controls), set(gate.CONTROLS))
        for name, (path, changed, function) in controls.items():
            with self.subTest(control=name):
                self.assertNotEqual(changed, self.sources[path])
                start, end = gate.function_region(changed, function)
                self.assertLess(start, end)
        for name in gate.CONTROLS:
            path = gate.CONTROLS[name][0]
            changed = dict(self.sources)
            changed[path] = self.sources[path].replace(gate.CONTROLS[name][1], "")
            with self.assertRaisesRegex(RuntimeError, "anchor changed"):
                gate.control_sources(changed)

    def test_function_regions_are_exact(self):
        source = "fn a() {\n}\n\nfn b() {\n  x\n}\nfn c() {}\n"
        self.assertEqual(gate.function_region(source, "a"), (1, 3))
        self.assertEqual(gate.function_region(source, "b"), (4, 6))
        self.assertEqual(gate.function_region(source, "c"), (7, 8))
        for missing in ("d", "f"):
            with self.assertRaisesRegex(ValueError, "not one intended function"):
                gate.function_region(source + "fn f() {}\nfn f() {}\n", missing)


class Admission(unittest.TestCase):
    def test_an_emitted_unreviewed_candidate_cannot_enter_normal_acceptance(self):
        gate.require_reviewed_profile({})
        gate.require_reviewed_profile({'reviewed': True})
        for flag in (False, None, 0, 1, 'true'):
            with self.assertRaisesRegex(ValueError, 'independent technical review'):
                gate.require_reviewed_profile({'reviewed': flag})

    def test_candidate_is_whole_pinned_and_unreviewed_with_proof_only_scope(self):
        pin = json.loads((gate.ROOT / gate.verifier.PIN).read_text())
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        report = {'verus': {'version': pin['version'], 'commit': pin['commit'],
            'toolchain': pin['rust_toolchain']},
            'verification-results': {'success': True, 'errors': 0, 'verified': 1204,
                'encountered-error': False, 'encountered-vir-error': False,
                'is-verifying-entire-crate': True},
            'func-details': {**dict.fromkeys(profile['functions'], {}), 'outside::exec': {}}}
        process = subprocess.CompletedProcess([], 0, '', '')
        candidate = gate.candidate_profile(process, report, pin, profile, profile['functions'])
        self.assertFalse(candidate['reviewed'])
        self.assertEqual(candidate['namespaces'], list(gate.NAMESPACES))
        self.assertEqual(candidate['module_verified'], profile['module_verified'])
        self.assertEqual(candidate['inventory']['Exec'], 0)
        self.assertEqual(candidate['target_functions'], sorted(profile['functions']))
        for section, key, value in (('verus', 'commit', 'other'),
                ('verification-results', 'errors', 1),
                ('verification-results', 'is-verifying-entire-crate', False)):
            wrong = copy.deepcopy(report); wrong[section][key] = value
            with self.assertRaises(ValueError):
                gate.candidate_profile(process, wrong, pin, profile, profile['functions'])
        with self.assertRaises(ValueError):
            gate.candidate_profile(subprocess.CompletedProcess([], 1, '', ''), report,
                pin, profile, profile['functions'])


    def setUp(self):
        self.pin = json.loads((gate.ROOT / gate.verifier.PIN).read_text())
        self.profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        # A synthetic protocol fixture; no proof result is claimed by this test.
        self.report = {
            "verification-results": {
                "success": True, "errors": 0, "verified": self.profile["expected_verified"],
                "encountered-error": False, "encountered-vir-error": False,
                "is-verifying-entire-crate": True,
            },
            "verus": {"commit": self.pin["commit"], "version": self.pin["version"],
                      "toolchain": self.pin["rust_toolchain"]},
            "func-details": {name: {} for name in self.profile["target_functions"]},
        }

    def test_positive_run_requires_the_pinned_complete_result(self):
        def accepted(report, stderr="", code=0):
            process = subprocess.CompletedProcess([], code, "", stderr)
            return gate.positive_accepted(process, report, self.pin, self.profile)
        self.assertTrue(accepted(self.report, "note: automatically chose triggers\n"))
        self.assertFalse(accepted(self.report, code=1))
        self.assertFalse(accepted(self.report, "error: assertion failed\n"))
        self.assertFalse(accepted(self.report, "note: resource limit exceeded\n"))
        for field, value in (("verified", self.profile["expected_verified"] - 1), ("errors", 1),
                             ("success", False), ("is-verifying-entire-crate", False)):
            with self.subTest(field=field):
                changed = copy.deepcopy(self.report)
                changed["verification-results"][field] = value
                self.assertFalse(accepted(changed))
        changed = copy.deepcopy(self.report)
        changed["func-details"].pop(self.profile["target_functions"][0])
        self.assertFalse(accepted(changed))

    def test_control_requires_one_intended_semantic_failure(self):
        report = copy.deepcopy(self.report)
        report["verification-results"].update(
            errors=1, verified=self.profile["module_verified"] - 1,
            **{"encountered-error": True, "is-verifying-entire-crate": False})
        del report["verification-results"]["success"]
        path, specimen = Path("subject.rs"), Path("/tmp/checked-assembly-control-fixture")
        diagnostic = "error: assertion failed\n --> subject.rs:12:5\n"
        summary = "error: aborting due to 1 previous error\n"

        def refused(stderr, current=report, code=1):
            process = subprocess.CompletedProcess([], code, "", stderr)
            return gate.targeted_refusal(process, current, self.pin, self.profile, path,
                                         specimen, (10, 20))
        self.assertTrue(refused(diagnostic + summary))
        self.assertTrue(refused(diagnostic.replace("assertion failed", "postcondition not satisfied")
                                + summary))
        with_success = copy.deepcopy(report)
        with_success["verification-results"]["success"] = False
        self.assertTrue(refused(diagnostic + summary, with_success))
        for wrong in (
            diagnostic + "note: resource limit (rlimit) exceeded\n" + summary,
            diagnostic + "note: solver returned unknown\n" + summary,
            diagnostic.replace("subject.rs", "other.rs") + summary,
            diagnostic.replace(":12:5", ":9:5") + summary,
            diagnostic.replace("assertion failed", "mismatched types") + summary,
            diagnostic.replace("error: ", "error[E0308]: ") + summary,
            diagnostic * 2 + "error: aborting due to 2 previous errors\n",
            diagnostic, summary, "",
        ):
            with self.subTest(stderr=wrong):
                self.assertFalse(refused(wrong))
        for code in (0, 101, 124, -9):
            self.assertFalse(refused(diagnostic + summary, code=code))
        for group, key, value in (
            ("verus", "version", "other"), ("verus", "commit", "other"),
            ("verus", "toolchain", "other"), ("verification-results", "errors", 2),
            ("verification-results", "verified", self.profile["module_verified"]),
            ("verification-results", "success", True),
            ("verification-results", "encountered-vir-error", True),
            ("verification-results", "is-verifying-entire-crate", True),
        ):
            with self.subTest(field=key):
                changed = copy.deepcopy(report)
                changed[group][key] = value
                self.assertFalse(refused(diagnostic + summary, changed))


if __name__ == "__main__":
    unittest.main()
