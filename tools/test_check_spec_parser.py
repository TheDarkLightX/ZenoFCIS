"""Fail-closed admission tests for the parser's proof evidence gate."""
import copy
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from check_spec_parser import (
    accepted_kernel, accepted_native, binding_snapshot, dependency_closure,
    main, mutated_source, native_spec_unchanged, sha, snapshot_tree,
)


class Admission(unittest.TestCase):
    def setUp(self):
        self.pin = {"version": "pin", "commit": "revision", "rust_toolchain": "rust"}
        self.kernel = {"expected_verified": 3, "target_functions": ["runtime::accept"]}
        self.proc = SimpleNamespace(returncode=0)
        self.report = {
            "verus": {"version": "pin", "commit": "revision", "toolchain": "rust"},
            "func-details": {"runtime::accept": {}},
            "verification-results": {
                "success": True, "errors": 0, "verified": 3,
                "encountered-error": False, "encountered-vir-error": False,
                "is-verifying-entire-crate": True,
            },
        }

    def test_exact_complete_observation_is_accepted(self):
        self.assertTrue(accepted_kernel(self.proc, self.report, self.pin, self.kernel))

    def test_proof_errors_partial_runs_and_missing_inventory_are_refused(self):
        for key, value in [("errors", 1), ("verified", 2), ("verified", True),
                           ("success", False), ("is-verifying-entire-crate", False),
                           ("encountered-error", True), ("encountered-vir-error", True)]:
            with self.subTest(key=key, value=value):
                report = copy.deepcopy(self.report)
                report["verification-results"][key] = value
                self.assertFalse(accepted_kernel(self.proc, report, self.pin, self.kernel))
        report = copy.deepcopy(self.report)
        report["func-details"] = {}
        self.assertFalse(accepted_kernel(self.proc, report, self.pin, self.kernel))

    def test_stale_tool_and_nonzero_exit_are_refused(self):
        for key in ["version", "commit", "toolchain"]:
            report = copy.deepcopy(self.report)
            report["verus"][key] = "different"
            self.assertFalse(accepted_kernel(self.proc, report, self.pin, self.kernel))
        self.assertFalse(accepted_kernel(SimpleNamespace(returncode=1), self.report, self.pin, self.kernel))

    def test_native_summary_uses_exact_count_and_no_skipped_tests(self):
        summary = "test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.0s\n"
        self.assertTrue(accepted_native(SimpleNamespace(returncode=0, stdout=summary), 15))
        for changed in [summary.replace("15 passed", "115 passed"),
                        summary.replace("0 ignored", "1 ignored"),
                        summary.replace("0 filtered out", "1 filtered out"),
                        summary + summary, "compiler error"]:
            self.assertFalse(accepted_native(SimpleNamespace(returncode=0, stdout=changed), 15))
        self.assertFalse(accepted_native(SimpleNamespace(returncode=1, stdout=summary), 15))


class SourceAndDependencyBinding(unittest.TestCase):
    def test_directory_binding_detects_new_removed_and_changed_inputs(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "src").mkdir()
            (root / "src/a.rs").write_text("first")
            profile = {"bindings": [], "binding_directories": ["src"], "kernels": []}
            initial = binding_snapshot(profile, root)
            (root / "src/b.rs").write_text("added")
            self.assertNotEqual(initial, binding_snapshot(profile, root))
            (root / "src/b.rs").unlink()
            self.assertEqual(initial, binding_snapshot(profile, root))
            (root / "src/a.rs").write_text("different")
            self.assertNotEqual(initial, binding_snapshot(profile, root))
            (root / "src/a.rs").unlink()
            self.assertNotEqual(initial, binding_snapshot(profile, root))

    def test_source_symlinks_cannot_hide_unbound_inputs(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "source"
            source.mkdir()
            (root / "outside.rs").write_text("external")
            (source / "alias.rs").symlink_to(root / "outside.rs")
            with self.assertRaises(RuntimeError):
                snapshot_tree(source)

    def test_binding_paths_cannot_escape_the_repository(self):
        with tempfile.TemporaryDirectory() as temporary:
            for field in ("bindings", "binding_directories"):
                for escape in ("../outside", "/outside"):
                    profile = {"bindings": [], "binding_directories": [], "kernels": []}
                    profile[field] = [escape]
                    with self.assertRaises(RuntimeError):
                        binding_snapshot(profile, Path(temporary))

    def test_dependency_closure_includes_build_transitivity_but_excludes_dev_only(self):
        def dependency(package, kind):
            return {"pkg": package, "dep_kinds": [{"kind": kind, "target": None}]}

        metadata = {"resolve": {"nodes": [
            {"id": "spec", "deps": [dependency("codec", None), dependency("dev", "dev")]},
            {"id": "codec", "deps": [dependency("generator", "build")]},
            {"id": "generator", "deps": [dependency("support", None)]},
            {"id": "support", "deps": []},
            {"id": "dev", "deps": []},
        ]}}
        self.assertEqual(dependency_closure(metadata, "spec"),
                         {"spec", "codec", "generator", "support"})
        with self.assertRaises(RuntimeError):
            dependency_closure(metadata, "missing")

    def test_native_dependency_freshness_checks_sources_outputs_and_tool_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, output = root / "package", root / "target"
            source.mkdir()
            output.mkdir()
            (source / "Cargo.toml").write_text("manifest")
            (source / "lib.rs").write_text("body")
            (output / "libspec.rlib").write_bytes(b"compiled")
            tool = root / "rustc"
            tool.write_bytes(b"compiler")
            record = {
                "cargo_configuration_sha256": {},
                "packages": {"spec": {"manifest_path": str(source / "Cargo.toml"),
                                      "source_sha256": snapshot_tree(source)}},
                "tools": {"rustc": {"path": str(tool), "sha256": sha(tool)}},
                "target_directory": str(output), "artifact_sha256": snapshot_tree(output),
            }
            with patch("check_spec_parser.cargo_configuration_snapshot", return_value={}):
                self.assertTrue(native_spec_unchanged(record, {}))
                for path, changed in [(source / "lib.rs", b"changed source"),
                                      (output / "libspec.rlib", b"changed object"),
                                      (tool, b"changed compiler")]:
                    original = path.read_bytes()
                    path.write_bytes(changed)
                    self.assertFalse(native_spec_unchanged(record, {}))
                    path.write_bytes(original)
                (output / "extra.rlib").write_bytes(b"unrecorded object")
                self.assertFalse(native_spec_unchanged(record, {}))
            with patch("check_spec_parser.cargo_configuration_snapshot", return_value={"new": "hash"}):
                self.assertFalse(native_spec_unchanged(record, {}))

    def test_failed_replay_removes_the_old_success_receipt_first(self):
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary)
            receipt = out / "receipt.json"
            receipt.write_text(json.dumps({"status": "passed"}))
            with patch("sys.argv", ["check_spec_parser.py", "--out", str(out)]), \
                    patch("check_spec_parser.prepare_tools", side_effect=RuntimeError("tool refusal")):
                with self.assertRaisesRegex(RuntimeError, "tool refusal"):
                    main()
            self.assertFalse(receipt.exists())

    def test_configuration_change_before_source_binding_is_refused(self):
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary)
            pin, profile = out / "pin.json", out / "profile.json"
            pin.write_text(json.dumps({"rust_toolchain": "pinned"}))
            profile.write_text(json.dumps({"kernels": []}))

            def changed_configuration(*_args):
                profile.write_text(json.dumps({"kernels": [], "changed": True}))
                return out

            with patch("sys.argv", ["check_spec_parser.py", "--out", str(out)]), \
                    patch("check_spec_parser.PIN", pin), \
                    patch("check_spec_parser.PROFILE", profile), \
                    patch("check_spec_parser.prepare_tools", side_effect=changed_configuration), \
                    patch("check_spec_parser.execute") as execute:
                with self.assertRaisesRegex(RuntimeError, "configuration changed"):
                    main()
                execute.assert_not_called()
            self.assertFalse((out / "receipt.json").exists())


class MutationBinding(unittest.TestCase):
    def test_method_mutation_does_not_reach_the_following_method(self):
        source = "impl Ops {\n    pub fn first() -> bool { true }\n    pub fn second() -> bool { true }\n}\n"
        mutant = {"function": "first", "old": "true", "new": "false"}
        changed = mutated_source(source, mutant)
        self.assertIn("fn first() -> bool { false }", changed)
        self.assertIn("fn second() -> bool { true }", changed)

    def test_const_and_spec_methods_are_separate_mutation_scopes(self):
        source = ("impl Span {\n"
                  "    pub const fn start(self) -> u32 { self.start }\n"
                  "    pub const fn end(self) -> u32 { self.start }\n"
                  "    pub closed spec fn view(self) -> u32 { self.start }\n}\n")
        changed = mutated_source(source, {"function": "start", "old": "self.start", "new": "self.end"})
        self.assertIn("fn start(self) -> u32 { self.end }", changed)
        self.assertIn("fn end(self) -> u32 { self.start }", changed)
        self.assertIn("fn view(self) -> u32 { self.start }", changed)

    def test_ambiguous_function_or_body_anchor_is_refused(self):
        mutant = {"function": "accept", "old": "true", "new": "false"}
        for source in ["fn accept() { true; true; }", "fn accept() { true }\nfn accept() { true }",
                       "fn accept_more() { true }"]:
            with self.assertRaises(RuntimeError):
                mutated_source(source, mutant)


if __name__ == "__main__":
    unittest.main()
