"""Admission/refusal controls; fixtures are not new mathematical proof results."""
import copy
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest import mock

import check_ordering256 as gate


class OrderingAdmission(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "source"
        for path in gate.SOURCES:
            target = self.root / path
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(gate.ROOT / path, target)
        self.profile = json.loads((self.root / gate.PROFILE).read_text())

    def change(self, path, source, *, review_hash=False):
        (self.root / path).write_text(source)
        if review_hash:
            self.profile["source_hashes"][str(path)] = gate.verifier.digest(self.root / path)

    def test_current_shared_sources_are_admitted(self):
        admitted = gate.configuration(self.root, self.profile)
        self.assertEqual(sum(map(len, admitted.values())), 5)

    def test_source_hash_drift_refuses(self):
        self.change(gate.SUBJECT, (self.root / gate.SUBJECT).read_text() + "\n")
        with self.assertRaisesRegex(ValueError, "source changed"):
            gate.configuration(self.root, self.profile)

    def test_native_only_early_return_refuses_even_with_updated_hash(self):
        source = (self.root / gate.SUBJECT).read_text().replace(
            "    let mut i = N;", "    #[cfg(not(verus_keep_ghost))]\n    return Ordering::Equal;\n    let mut i = N;")
        self.change(gate.SUBJECT, source, review_hash=True)
        with self.assertRaises(gate.verifier.ConfigurationRefused):
            gate.configuration(self.root, self.profile)

    def test_cfg_expression_refuses_even_with_updated_hash(self):
        source = (self.root / gate.SUBJECT).read_text().replace(
            "    let mut i = N;", "    if cfg!(test) { return Ordering::Equal; }\n    let mut i = N;")
        self.change(gate.SUBJECT, source, review_hash=True)
        with self.assertRaises(gate.verifier.ConfigurationRefused):
            gate.configuration(self.root, self.profile)

    def test_assumptions_and_new_input_premises_refuse(self):
        for token in ("assume", "admit", "external_body", "requires"):
            with self.subTest(token=token):
                source = (gate.ROOT / gate.MODEL).read_text() + "\n" + token + "\n"
                self.change(gate.MODEL, source, review_hash=True)
                with self.assertRaisesRegex(ValueError, "unconditional comparison proof"):
                    gate.configuration(self.root, self.profile)

    def test_refusal_precedes_tool_preparation(self):
        self.change(gate.SUBJECT, (self.root / gate.SUBJECT).read_text() + "\n")
        with mock.patch.object(gate.verifier, "prepare_tools") as prepare:
            with self.assertRaisesRegex(ValueError, "source changed"):
                gate.check(Path("unused-cache"), False, Path(self.temporary.name), self.root)
            prepare.assert_not_called()

    def test_missing_or_relocated_configuration_exception_refuses(self):
        for changed in ("missing", "relocated"):
            profile = copy.deepcopy(self.profile)
            entries = profile["native_configuration_exceptions"][str(gate.ARITHMETIC)]
            if changed == "missing":
                entries.pop()
            else:
                entries[0]["following"] = "mod some_other_module;"
            with self.subTest(changed=changed), self.assertRaises((ValueError, RuntimeError)):
                gate.configuration(self.root, profile)

    def test_symbolic_link_source_refuses(self):
        path = self.root / gate.SUBJECT
        path.unlink()
        path.symlink_to(gate.ROOT / gate.SUBJECT)
        with self.assertRaisesRegex(ValueError, "symbolic-link"):
            gate.configuration(self.root, self.profile)

    def test_all_three_mutants_have_unique_actual_body_changes(self):
        controls = gate.control_sources(self.root, self.profile)
        self.assertEqual({name for name, _ in controls}, gate.CONTROL_NAMES)
        self.assertEqual(len({source for _, source in controls}), 3)
        self.assertTrue(all(source != (self.root / gate.SUBJECT).read_text()
                            for _, source in controls))

    def test_missing_duplicate_and_ambiguous_mutants_refuse(self):
        for mode in ("missing", "duplicate", "ambiguous"):
            profile = copy.deepcopy(self.profile)
            if mode == "missing":
                profile["mutants"].pop()
            elif mode == "duplicate":
                profile["mutants"][1] = profile["mutants"][0]
            else:
                profile["mutants"][0]["old"] = "not an actual source anchor"
            with self.subTest(mode=mode), self.assertRaises(ValueError):
                gate.control_sources(self.root, profile)


class ProofAdmission(unittest.TestCase):
    def setUp(self):
        self.profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        self.pin = {"version": "fixture-version", "commit": "fixture-commit", "rust_toolchain": "fixture-toolchain"}
        self.report = {"verus": {"version": self.pin["version"], "commit": self.pin["commit"],
                                 "toolchain": self.pin["rust_toolchain"]},
                       "verification-results": {"success": True, "verified": 2, "errors": 0,
                            "encountered-error": False, "encountered-vir-error": False,
                            "is-verifying-entire-crate": True},
                       "func-details": {name: {} for name in self.profile["local_functions"]}}
        self.proc = subprocess.CompletedProcess([], 0, "", "")

    def test_complete_unfiltered_report_shape(self):
        self.assertTrue(gate.positive_accepted(self.proc, self.report, self.pin, self.profile))

    def test_filtered_missing_or_extra_body_and_wrong_count_refuse(self):
        for mode in ("filtered", "missing", "extra", "count", "error", "wrong-pin", "nonzero-exit"):
            report = copy.deepcopy(self.report)
            proc = self.proc
            if mode == "filtered":
                report["verification-results"]["is-verifying-entire-crate"] = False
            elif mode == "missing":
                del report["func-details"][gate.TARGET]
            elif mode == "extra":
                report["func-details"]["ordering256_harness::other_body"] = {}
            elif mode == "count":
                report["verification-results"]["verified"] = 1
            elif mode == "error":
                report["verification-results"]["encountered-error"] = True
            elif mode == "wrong-pin":
                report["verus"]["commit"] = "different"
            else:
                proc = subprocess.CompletedProcess([], 1, "", "")
            with self.subTest(mode=mode):
                self.assertFalse(gate.positive_accepted(proc, report, self.pin, self.profile))

    def test_smt_entry_and_loop_must_belong_to_actual_shared_body(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            def body(line, path=gate.SUBJECT, function=gate.TARGET):
                return f";; Function-Def {function}\n;; {root / path}:{line}:1: end\n (check-sat)\n"
            log = root / "comparison.smt2"
            valid = body(13) + body(21)
            log.write_text(valid)
            self.assertEqual(gate.smt_coverage(root, root, self.profile)["solver_queries"], 2)
            for changed in (body(13), body(13) * 2, valid + body(21),
                            body(13, Path("copied.rs")) + body(21),
                            body(13, function="other::compare") + body(21),
                            valid + ";; Function-Def malformed\n"):
                with self.subTest(changed=changed):
                    log.write_text(changed)
                    with self.assertRaises(ValueError):
                        gate.smt_coverage(root, root, self.profile)


if __name__ == "__main__":
    unittest.main()
