"""Dependency provenance regressions for generated and packaged consumers."""

import copy
import io
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest import mock

import check_generated_application as application
import rc_package


class DependencyAdmissionTests(unittest.TestCase):
    def setUp(self):
        self.external = {
            "name": "external-library", "version": "2.3.4",
            "source": "registry+https://github.com/rust-lang/crates.io-index",
            "checksum": "a" * 64,
        }
        self.internal = {"name": "zeno-fcis-core", "version": "1.0.0-rc.3"}
        self.reference = {"package": [self.external, self.internal]}
        self.lock = copy.deepcopy(self.reference)
        self.manifest = Path("/unpacked/zeno-fcis-core-1.0.0-rc.3/Cargo.toml")
        self.allowed = {("zeno-fcis-core", "1.0.0-rc.3"): self.manifest}
        self.metadata = {"packages": [
            {**self.internal, "source": None, "manifest_path": str(self.manifest)},
            {**self.external, "manifest_path": "/cargo-cache/external-library/Cargo.toml"},
        ]}

    def validate(self):
        application.validate_resolved_graph(
            self.reference, self.lock, self.metadata, self.allowed,
        )

    def test_reviewed_external_graph_and_exact_package_source_are_accepted(self):
        self.validate()

    def test_external_identity_drift_is_rejected_even_when_cached_offline(self):
        for field, value in [("version", "2.3.5"), ("checksum", "b" * 64),
                             ("source", "registry+https://another.invalid/index")]:
            with self.subTest(field=field):
                self.lock = copy.deepcopy(self.reference)
                self.lock["package"][0][field] = value
                with self.assertRaisesRegex(RuntimeError, "external dependency"):
                    self.validate()

    def test_checkout_fallback_is_rejected(self):
        self.metadata["packages"][0]["manifest_path"] = "/checkout/crates/zeno-fcis-core/Cargo.toml"
        with self.assertRaisesRegex(RuntimeError, "local manifest"):
            self.validate()

    def test_internal_version_drift_is_rejected(self):
        self.lock["package"][1]["version"] = "1.0.0-rc.2"
        self.metadata["packages"][0]["version"] = "1.0.0-rc.2"
        with self.assertRaisesRegex(RuntimeError, "local package"):
            self.validate()

    def test_external_library_cannot_be_replaced_by_a_local_checkout(self):
        self.lock["package"][0].pop("source")
        self.metadata["packages"][1]["source"] = None
        with self.assertRaisesRegex(RuntimeError, "local package"):
            self.validate()

    def test_metadata_must_agree_with_the_admitted_lock(self):
        self.metadata["packages"][1]["version"] = "2.3.5"
        with self.assertRaisesRegex(RuntimeError, "metadata.*lock"):
            self.validate()

    def test_emitted_dependencies_reject_stale_pins_and_source_overrides_before_editing(self):
        declarations = [
            '"=1.0.0-rc.2"',
            '{ version = "=1.0.0-rc.3", path = "/checkout/crates/zeno-fcis-core" }',
        ]
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-emitted-pins-") as directory:
            root = Path(directory)
            package = root / "core"
            package.mkdir()
            (package / "Cargo.toml").write_text('[package]\nname = "zeno-fcis-core"\nversion = "1.0.0-rc.3"\n')
            manifest = root / "Cargo.toml"
            for declaration in declarations:
                with self.subTest(declaration=declaration):
                    original = f'[dependencies]\nzeno-fcis-core = {declaration}\n'
                    manifest.write_text(original)
                    with self.assertRaisesRegex(RuntimeError, "generated.*dependency"):
                        application.bind_generated_dependencies(manifest, {"zeno-fcis-core": package}, "1.0.0-rc.3")
                    self.assertEqual(manifest.read_text(), original)


class PackagedStagingTests(unittest.TestCase):
    def archive(self, root: Path, members: list[str]) -> Path:
        path = root / "example-1.0.0.crate"
        with tarfile.open(path, "w:gz") as archive:
            for name in members:
                info = tarfile.TarInfo(name)
                info.size = 1
                archive.addfile(info, io.BytesIO(b"x"))
        return path

    def test_single_declared_root_extracts(self):
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-crate-layout-") as directory:
            root = Path(directory)
            archive = self.archive(root, ["example-1.0.0/Cargo.toml"])
            rc_package.extract_checked_crate(archive, root / "sources")
            self.assertEqual((root / "sources/example-1.0.0/Cargo.toml").read_bytes(), b"x")

    def test_ambiguous_package_layout_is_rejected_before_extraction(self):
        cases = [["unexpected-root/Cargo.toml"],
                 ["example-1.0.0/Cargo.toml", "example-1.0.0/./Cargo.toml"]]
        for members in cases:
            with self.subTest(members=members), tempfile.TemporaryDirectory(prefix="zeno-fcis-crate-layout-") as directory:
                root = Path(directory)
                archive = self.archive(root, members)
                with self.assertRaises(rc_package.RcError):
                    rc_package.extract_checked_crate(archive, root / "sources")
                self.assertFalse((root / "sources").exists())

    def test_failure_removes_only_the_new_staging_directory(self):
        def fail_check(packages, version, staging, environment):
            (staging / "partial-build").write_bytes(b"temporary")
            raise RuntimeError("deliberate check failure")

        with tempfile.TemporaryDirectory(prefix="zeno-fcis-stage-cleanup-") as directory:
            root = Path(directory)
            marker = root / "preserve"
            marker.write_bytes(b"existing output")
            with mock.patch.object(rc_package, "check_packaged_workspace", side_effect=fail_check):
                with self.assertRaisesRegex(RuntimeError, "deliberate check failure"):
                    rc_package.verify_packaged_workspace(root / "packages", "1.0.0", root, {})
            self.assertEqual(list(root.iterdir()), [marker])
            self.assertEqual(marker.read_bytes(), b"existing output")


if __name__ == "__main__":
    unittest.main()
