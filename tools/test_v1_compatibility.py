"""Negatives for the narrow V1 receipt source and execution qualification."""

import copy
import hashlib
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

import check_generated_application as application
import check_v1_compatibility as compatibility
import rc_package


def passing_output(duration="0.01"):
    return "\n".join([
        *(f"test {name} ... ok" for name in compatibility.TEST_NAMES),
        "test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; "
        f"12 filtered out; finished in {duration}s", "",
    ])


class ReceiptSourceQualificationTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="zeno-fcis-v1-source-")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        source = compatibility.ROOT
        baseline = json.loads((source / compatibility.BASELINE).read_bytes())
        paths = [entry["path"] for entry in baseline["files"]]
        paths += [compatibility.BASELINE, compatibility.QUALIFICATION,
                  compatibility.ORACLE, compatibility.TEST_SOURCE]
        for relative in paths:
            path = self.root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes((source / relative).read_bytes())
        self.patch = mock.patch.object(compatibility, "ROOT", self.root)
        self.patch.start()
        self.addCleanup(self.patch.stop)

    def rewrite(self, relative, before, after):
        path = self.root / relative
        data = path.read_bytes()
        self.assertIn(before, data)
        path.write_bytes(data.replace(before, after, 1))

    def repin(self, field):
        path = self.root / compatibility.QUALIFICATION
        record = json.loads(path.read_bytes())
        item = record[field]
        data = (self.root / item["path"]).read_bytes()
        item.update(bytes=len(data), sha256=hashlib.sha256(data).hexdigest())
        path.write_text(json.dumps(record))

    def packages(self):
        target = self.root / "archives"
        baseline = json.loads((self.root / compatibility.BASELINE).read_bytes())
        packages = {}
        for relative in [*(item["path"] for item in baseline["files"]), compatibility.TEST_SOURCE]:
            path = Path(relative)
            if path.parts[0] != "crates":
                continue
            root = target / path.parts[1]
            packages[path.parts[1]] = root
            dest = root.joinpath(*path.parts[2:])
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes((self.root / path).read_bytes())
        return packages

    def test_source_guard_distinguishes_history_unchanged_files_and_unrun_behavior(self):
        result = compatibility.check()
        self.assertEqual(result["status"], "source-checked")
        self.assertEqual(len(result["baseline"]["files"]), 8)
        self.assertEqual(len(result["unchanged_files"]), 7)
        self.assertEqual(result["receipt_refactor"]["behavior"], "not-run")
        original = next(item for item in result["baseline"]["files"] if item["path"] == compatibility.RECEIPT)
        self.assertEqual(original["sha256"], result["receipt_refactor"]["oracle"]["sha256"])
        self.assertNotEqual(original["sha256"], result["receipt_refactor"]["source"]["sha256"])

    def test_historical_baseline_cannot_be_refreshed(self):
        path = self.root / compatibility.BASELINE
        path.write_bytes(path.read_bytes() + b"\n")
        with self.assertRaisesRegex(RuntimeError, "historical V1"):
            compatibility.check()

    def test_candidate_source_identity_is_exact(self):
        path = self.root / compatibility.RECEIPT
        path.write_bytes(path.read_bytes() + b"\n")
        with self.assertRaisesRegex(RuntimeError, "source changed"):
            compatibility.check()

    def test_oracle_bytes_and_historical_identity_cannot_be_replaced(self):
        path = self.root / compatibility.ORACLE
        path.write_bytes(path.read_bytes() + b"\n")
        with self.assertRaisesRegex(RuntimeError, "source changed"):
            compatibility.check()
        self.repin("oracle")
        with self.assertRaisesRegex(RuntimeError, "historical V1 identity"):
            compatibility.check()

    def test_protected_decoder_change_rejects_even_if_candidate_hash_is_refreshed(self):
        self.rewrite(compatibility.RECEIPT, b"    ensure_consumed(&cursor)?;", b"    // removed consumption check")
        self.repin("source")
        with self.assertRaisesRegex(RuntimeError, "protected source"):
            compatibility.check()

    def test_shared_hash_domain_change_rejects_even_if_candidate_hash_is_refreshed(self):
        self.rewrite(compatibility.RECEIPT, b"Domain::new(domain_name, 1)", b"Domain::new(domain_name, 2)")
        self.repin("source")
        with self.assertRaisesRegex(RuntimeError, "protected source"):
            compatibility.check()

    def test_public_signature_cannot_change_inside_replaced_function(self):
        self.rewrite(compatibility.RECEIPT, b"        reason_code: Option<&str>,",
                     b"        reason_code: Option<&'static str>,")
        self.repin("source")
        with self.assertRaisesRegex(RuntimeError, "public signature"):
            compatibility.check()

    def test_reference_validator_cannot_follow_candidate_change(self):
        self.rewrite(compatibility.TEST_SOURCE, b"if rebuilt != *bundle {", b"if false {")
        self.repin("tests")
        with self.assertRaisesRegex(RuntimeError, "oracle function"):
            compatibility.check()

    def test_test_module_cannot_be_removed(self):
        self.rewrite(compatibility.RECEIPT, b"#[cfg(test)]\nmod validation_tests;\n\n", b"")
        self.repin("source")
        with self.assertRaisesRegex(RuntimeError, "test-module boundary"):
            compatibility.check()

    def test_an_extra_function_cannot_hide_in_removed_helper_region(self):
        self.rewrite(compatibility.RECEIPT, b"fn validate_decision_reason(",
                     b"fn unreviewed() {}\n\nfn validate_decision_reason(")
        self.repin("source")
        with self.assertRaisesRegex(RuntimeError, "helper boundary"):
            compatibility.check()

    def test_missing_qualification_rejects(self):
        (self.root / compatibility.QUALIFICATION).unlink()
        with self.assertRaises(OSError):
            compatibility.check()

    def test_missing_test_source_rejects(self):
        (self.root / compatibility.TEST_SOURCE).unlink()
        with self.assertRaisesRegex(RuntimeError, "missing"):
            compatibility.check()

    def test_qualification_cannot_select_other_paths_or_add_inputs(self):
        path = self.root / compatibility.QUALIFICATION
        record = json.loads(path.read_bytes())
        for changed in ({**record, "skip_tests": True},
                        {**record, "source": {**record["source"], "path": compatibility.ORACLE}}):
            with self.subTest(changed=changed):
                path.write_text(json.dumps(changed))
                with self.assertRaises(RuntimeError):
                    compatibility.check()

    def test_changed_archived_receipt_cannot_borrow_qualified_checkout(self):
        expected = compatibility.check()
        packages = self.packages()
        self.assertEqual(compatibility.check(packages), expected)
        path = packages["zeno-fcis-receipt"] / "src/lib.rs"
        path.write_bytes((self.root / compatibility.ORACLE).read_bytes())
        with self.assertRaisesRegex(RuntimeError, "source changed"):
            compatibility.check(packages)
        self.assertEqual(compatibility.check(), expected)

    def test_archives_cannot_omit_tests_or_receipt_root(self):
        packages = self.packages()
        (packages["zeno-fcis-receipt"] / "src/validation_tests.rs").unlink()
        with self.assertRaisesRegex(RuntimeError, "missing"):
            compatibility.check(packages)
        del packages["zeno-fcis-receipt"]
        with self.assertRaisesRegex(RuntimeError, "missing packaged"):
            compatibility.check(packages)


class ReceiptExecutionQualificationTests(unittest.TestCase):
    def test_zero_missing_ignored_duplicate_and_unexpected_tests_cannot_pass(self):
        valid = passing_output()
        first = f"test {compatibility.TEST_NAMES[0]} ... ok"
        for output in ("", valid.replace(first, ""), valid.replace(first, first.replace("ok", "ignored")),
                       valid + first + "\n", valid + "test validation_tests::extra ... ok\n",
                       valid.replace("3 passed", "0 passed"), valid.replace("0 ignored", "1 ignored"),
                       valid + valid):
            with self.subTest(output=output):
                with self.assertRaisesRegex(RuntimeError, "missing, skipped"):
                    application.checked_receipt_test_output(output)

    def test_runner_executes_selected_package_and_exports_no_paths_or_timing(self):
        source = {"status": "source-checked", "receipt_refactor": {"behavior": "not-run"}}
        receipts = []
        package = Path("/selected-archive/zeno-fcis-receipt")
        for duration in ("0.01", "9.99"):
            with mock.patch.object(compatibility, "check", side_effect=[copy.deepcopy(source), copy.deepcopy(source)]), \
                    mock.patch.object(application, "run", return_value=passing_output(duration)) as run, \
                    mock.patch("sys.stdout", new_callable=io.StringIO):
                result = application.exercise_receipt_compatibility({"zeno-fcis-receipt": package}, {})
            command, cwd = run.call_args.args
            self.assertEqual(cwd, package)
            self.assertEqual(command[3:5], ["--manifest-path", str(package / "Cargo.toml")])
            self.assertNotIn("--no-run", command)
            self.assertNotIn("--ignored", command)
            self.assertEqual(result["receipt_refactor"]["behavior"]["test_names"], list(compatibility.TEST_NAMES))
            self.assertNotIn(str(package), json.dumps(result))
            self.assertNotIn(duration, json.dumps(result))
            receipts.append(result)
        self.assertEqual(receipts[0], receipts[1])

    def test_changed_inputs_during_execution_cannot_pass(self):
        before = {"receipt_refactor": {"behavior": "not-run"}}
        after = {"receipt_refactor": {"behavior": "not-run", "source": "changed"}}
        with mock.patch.object(compatibility, "check", side_effect=[before, after]), \
                mock.patch.object(application, "run", return_value=passing_output()), \
                mock.patch("sys.stdout", new_callable=io.StringIO):
            with self.assertRaisesRegex(RuntimeError, "inputs changed"):
                application.exercise_receipt_compatibility({"zeno-fcis-receipt": Path("archive")}, {})

    def test_failed_execution_cannot_be_reported_as_passed(self):
        with mock.patch.object(compatibility, "check", return_value={}), \
                mock.patch.object(application, "run", side_effect=subprocess.CalledProcessError(1, ["cargo"])):
            with self.assertRaises(subprocess.CalledProcessError):
                application.exercise_receipt_compatibility({"zeno-fcis-receipt": Path("archive")}, {})

    def test_packaged_receipt_retains_qualification_oracle_and_test_inputs(self):
        inputs = {item["path"] for item in rc_package.packaged_checker_inputs()}
        self.assertTrue({compatibility.BASELINE, compatibility.QUALIFICATION, compatibility.ORACLE,
                         compatibility.TEST_SOURCE, "tools/check_v1_compatibility.py"} <= inputs)


if __name__ == "__main__":
    unittest.main()
