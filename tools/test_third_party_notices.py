"""Offline inventory, licence-text and missing-crate controls."""

from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import json

import third_party_notices as notices


class ThirdPartyNoticesTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.generated = notices.generate()

    def test_committed_notices_match_the_locked_release_graph(self):
        notices.check_notices(notices.ROOT / "THIRD_PARTY_NOTICES", self.generated)

    def test_deleted_crate_fails_on_an_assertion(self):
        start = self.generated.index("=" * 72)
        end = self.generated.index("=" * 72, start + 72)
        with tempfile.TemporaryDirectory() as raw:
            path = Path(raw) / "THIRD_PARTY_NOTICES"
            path.write_text(self.generated[:start] + self.generated[end:])
            with self.assertRaisesRegex(AssertionError, "differs from the locked"):
                notices.check_notices(path, self.generated)

    def test_missing_licence_text_is_refused(self):
        with tempfile.TemporaryDirectory() as raw:
            with self.assertRaisesRegex(ValueError, "no local licence text"):
                notices.license_files({"name": "example", "manifest_path": str(Path(raw) / "Cargo.toml")})

    def test_full_texts_are_sorted_and_preserved(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            (root / "LICENSE-MIT").write_bytes(b"All licence terms.\r\n")
            (root / "COPYRIGHT").write_text("Copyright attribution.\n")
            (root / "README.md").write_text("Not a licence file.\n")
            package = {"name": "example", "manifest_path": str(root / "Cargo.toml")}
            self.assertEqual(notices.license_files(package), [
                ("COPYRIGHT", "Copyright attribution.\n"), ("LICENSE-MIT", "All licence terms.\r\n")])
            (root / "LICENSE-MIT").write_text("")
            with self.assertRaisesRegex(ValueError, "empty licence file"):
                notices.license_files(package)

    def test_declared_file_cannot_escape_package(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            with self.assertRaisesRegex(ValueError, "unsafe or missing"):
                notices.license_files({"name": "example", "manifest_path": str(root / "Cargo.toml"),
                                       "license_file": "../outside"})

    def test_runtime_selection_uses_declared_binaries_and_both_targets(self):
        workspace = {"name": "example-cli", "version": "1.0.0", "id": "workspace", "source": None}
        runtime = {"name": "runtime", "version": "1.0.0", "id": "runtime", "source": "registry+example"}
        build_only = {"name": "build-only", "version": "1.0.0", "id": "build", "source": "registry+example"}
        metadata = {"packages": [build_only, runtime, workspace], "workspace_members": ["workspace"]}
        def cargo(root, command, *args):
            if command == "metadata":
                return json.dumps(metadata)
            self.assertIn("normal,no-proc-macro", args)
            self.assertIn("example-cli", args)
            return "example-cli v1.0.0\nruntime v1.0.0\nruntime v1.0.0 (*)\n"
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            (root / "release").mkdir()
            (root / "release/package-set.toml").write_text('[[binaries]]\npackage = "example-cli"\n')
            with patch.object(notices, "cargo", side_effect=cargo) as run:
                self.assertEqual(notices.selected_packages(root), [runtime])
                for target in notices.RELEASE_TARGETS:
                    self.assertTrue(any(target in call.args for call in run.call_args_list))
                self.assertEqual(run.call_count, 3)


if __name__ == "__main__":
    unittest.main()
