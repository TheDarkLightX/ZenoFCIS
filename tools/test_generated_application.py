"""Dependency provenance regressions for generated and packaged consumers."""

import copy
import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import tomllib
import unittest
from unittest import mock

import check_generated_application as application
import check_v1_compatibility
import rc_package


class ReproducibleApplicationReceiptTests(unittest.TestCase):
    def test_variable_test_logs_are_checked_but_never_exported(self):
        report = {"schema": "zeno-fcis/completion-result/1", "status": "verified",
                  "authority": "none", "assurance": "complete-finite", "states_checked": 4,
                  "commands_per_state": 27, "maximum_exit_steps": 1,
                  "problem": "a" * 64, "plan_sha256": "b" * 64}
        result = {"status": "passed", "demonstration": json.dumps(
            {"status": "passed", "count": 0, "bundles": 2, "deliveries": 2,
             "pending": 0, "publication_bytes": 1893})}
        receipts = []
        for duration in ("0.10", "9.99"):
            log = f"completion_problem={'a' * 64}\ntest result: ok; finished in {duration}s\n"
            with mock.patch.object(application, "exercise_rust_application", return_value=(result, log)), \
                    mock.patch.object(application.check_synthesis, "exercise_counter", return_value={}), \
                    mock.patch.object(application, "run", side_effect=[json.dumps(report), json.dumps(report),
                                      json.dumps({**report, "replay": "matched"})]):
                ordinary = application.exercise_application(Path("app"), Path("work"), {}, "1.1.0", {}, ["cli"])
                prepared = application.exercise_prepared_application(Path("app"), Path("work"), {}, "1.1.0", {}, ["cli"])
                receipts.append((ordinary, prepared))
                self.assertNotIn(duration, json.dumps(receipts[-1]))
        self.assertEqual(receipts[0], receipts[1])


class ExampleApplicationTests(unittest.TestCase):
    def test_example_summaries_are_compared_key_by_key(self):
        for template, expected in application.EXAMPLE_TEMPLATES.items():
            with self.subTest(template=template):
                passed = {"status": "passed", "demonstration": json.dumps({**expected, "extra": 1})}
                synthesis = {"status": "passed"}
                with mock.patch.object(application, "exercise_rust_application", return_value=(passed, "")), \
                        mock.patch.dict(application.SYNTHESIZED_EXAMPLES,
                                        {name: mock.Mock(return_value=synthesis)
                                         for name in application.SYNTHESIZED_EXAMPLES}):
                    result = application.exercise_example_application(
                        template, Path("app"), Path("work"), {}, "1.1.0", {}, ["cli"])
                self.assertEqual(result["template"], template)
                self.assertEqual(result.get("synthesis"),
                                 synthesis if template in application.SYNTHESIZED_EXAMPLES else None)
                for key in expected:
                    changed = {"status": "passed",
                               "demonstration": json.dumps({**expected, key: "different"})}
                    with mock.patch.object(application, "exercise_rust_application",
                                           return_value=(changed, "")), \
                            mock.patch.dict(application.SYNTHESIZED_EXAMPLES,
                                            {name: mock.Mock(return_value={})
                                             for name in application.SYNTHESIZED_EXAMPLES}):
                        with self.assertRaisesRegex(RuntimeError, "demonstration differs"):
                            application.exercise_example_application(
                                template, Path("app"), Path("work"), {}, "1.1.0", {}, ["cli"])


class V1PackagedSourceTests(unittest.TestCase):
    def test_retained_consumer_manifest_parses_and_copies_without_a_checkout_dependency(self):
        retained = application.ROOT / "test-projects/external-consumer"
        manifest_bytes = (retained / "Cargo.toml").read_bytes()
        source_bytes = (retained / "src/main.rs").read_bytes()
        manifest = tomllib.loads(manifest_bytes.decode())
        version = tomllib.loads((application.ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
        dependency = manifest["dependencies"]["zeno-fcis"]
        self.assertEqual(dependency, {
            "version": f"={version}", "path": "../../crates/zeno-fcis", "default-features": False,
            "features": ["std", "bootstrap", "composed-program", "backend", "authenticated-authority", "authoring"],
        })
        packages = {tomllib.loads(path.read_text())["package"]["name"]: path.parent
                    for path in sorted((application.ROOT / "crates").glob("*/Cargo.toml"))}
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-v1-consumer-manifest-") as directory:
            with mock.patch.object(application, "resolve_reviewed_graph", return_value={}), \
                    mock.patch.object(application, "run", return_value=""):
                report = application.exercise_v1_consumer(Path(directory), packages, version, {})
            consumer = Path(directory) / "v1-consumer"
            copied = tomllib.loads((consumer / "Cargo.toml").read_text())
            self.assertEqual(copied["dependencies"]["zeno-fcis"], {
                key: value for key, value in dependency.items() if key != "path"
            })
            for key in ("package", "workspace", "lints"):
                self.assertEqual(copied[key], manifest[key])
            self.assertEqual((consumer / "src/main.rs").read_bytes(), source_bytes)
            self.assertEqual(report["status"], "passed")
        self.assertEqual((retained / "Cargo.toml").read_bytes(), manifest_bytes)
        self.assertEqual((retained / "src/main.rs").read_bytes(), source_bytes)

    def test_changed_archive_source_cannot_borrow_the_checkout_baseline(self):
        baseline = check_v1_compatibility.check()
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-v1-packaged-source-") as directory:
            packages = {}
            for entry in baseline["files"]:
                relative = Path(entry["path"])
                if relative.parts[0] != "crates":
                    continue
                root = Path(directory) / relative.parts[1]
                packages[relative.parts[1]] = root
                target = root.joinpath(*relative.parts[2:])
                target.parent.mkdir(parents=True)
                target.write_bytes((application.ROOT / relative).read_bytes())
            self.assertEqual(check_v1_compatibility.check(packages), baseline)
            codec = packages["zeno-fcis-codec"] / "src/lib.rs"
            codec.write_bytes(codec.read_bytes() + b"\n// Altered archive source.\n")
            with self.assertRaisesRegex(RuntimeError, "V1 compatibility baseline changed"):
                check_v1_compatibility.check(packages)
            # The independently retained checkout still matches. It cannot
            # substitute for the altered archive in packaged qualification.
            self.assertEqual(check_v1_compatibility.check(), baseline)


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
            ("zeno-fcis-core", '"=1.0.0-rc.2"'),
            ("zeno-fcis-core", '{ version = "=1.0.0-rc.3", path = "/checkout/crates/zeno-fcis-core" }'),
            ("core-alias", '{ package = "zeno-fcis-core", version = "=1.0.0-rc.2" }'),
            ("core-alias", '{ package = "zeno-fcis-core", version = "=1.0.0-rc.3", path = "/checkout/core" }'),
        ]
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-emitted-pins-") as directory:
            root = Path(directory)
            package = root / "core"
            package.mkdir()
            (package / "Cargo.toml").write_text('[package]\nname = "zeno-fcis-core"\nversion = "1.0.0-rc.3"\n')
            manifest = root / "Cargo.toml"
            for name, declaration in declarations:
                with self.subTest(declaration=declaration):
                    original = f'[dependencies]\n{name} = {declaration}\n'
                    manifest.write_text(original)
                    with self.assertRaisesRegex(RuntimeError, "generated.*dependency"):
                        application.bind_generated_dependencies(manifest, {"zeno-fcis-core": package}, "1.0.0-rc.3")
                    self.assertEqual(manifest.read_text(), original)

    def test_renamed_internal_dependencies_bind_the_actual_package_closure(self):
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-renamed-dependencies-") as directory:
            root = Path(directory)
            packages = {}
            for name in ("zeno-fcis-core", "zeno-fcis-value"):
                packages[name] = root / name
                packages[name].mkdir()
                (packages[name] / "Cargo.toml").write_text(
                    f'[package]\nname = "{name}"\nversion = "1.0.0-rc.3"\n',
                )
            with (packages["zeno-fcis-core"] / "Cargo.toml").open("a") as manifest:
                manifest.write('[dependencies]\nvalues = { package = "zeno-fcis-value", version = "=1.0.0-rc.3" }\n')
            manifest = root / "Cargo.toml"
            manifest.write_text('[dependencies]\ncore = { package = "zeno-fcis-core", version = "=1.0.0-rc.3" }\n')
            application.bind_generated_dependencies(manifest, packages, "1.0.0-rc.3")
            patched = rc_package.load_toml(manifest)["patch"]["crates-io"]
            self.assertEqual(patched, {name: {"path": str(path)} for name, path in packages.items()})


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

    def test_standalone_invalid_packages_leave_output_available_for_retry(self):
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-verify-retry-") as directory:
            root = Path(directory)
            packages = root / "packages"
            packages.mkdir()
            output = root / "result"
            marker = root / "preserve"
            marker.write_bytes(b"existing output")
            for attempt in range(2):
                with self.subTest(attempt=attempt):
                    with self.assertRaisesRegex(rc_package.RcError, "exact declared crate set"):
                        rc_package.verify_packaged(packages, output, "1.0.0")
                    self.assertFalse(output.exists())
                    self.assertEqual(marker.read_bytes(), b"existing output")

    def test_standalone_receipt_write_failure_removes_partial_output(self):
        def fail_write(path, result):
            path.write_text("partial receipt")
            raise OSError("deliberate receipt write failure")

        with tempfile.TemporaryDirectory(prefix="zeno-fcis-verify-write-") as directory:
            output = Path(directory) / "result"
            with mock.patch.object(rc_package, "check_packaged_workspace", return_value={}), \
                    mock.patch.object(rc_package, "write_json", side_effect=fail_write):
                with self.assertRaisesRegex(OSError, "deliberate receipt write failure"):
                    rc_package.verify_packaged(Path(directory) / "packages", output, "1.0.0")
            self.assertFalse(output.exists())

    def test_standalone_success_retains_receipt_and_rejects_existing_output(self):
        result = {"format": "zeno-fcis/packaged-application/1", "status": "passed"}
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-verify-success-") as directory:
            output = Path(directory) / "result"
            with mock.patch.object(rc_package, "check_packaged_workspace", return_value=result) as check:
                rc_package.verify_packaged(Path(directory) / "packages", output, "1.0.0")
                receipt = output / "PACKAGED-APPLICATION.json"
                self.assertEqual(list(output.iterdir()), [receipt])
                self.assertEqual(rc_package.load_json_object(receipt), result)
                before = receipt.read_bytes()
                with self.assertRaises(FileExistsError):
                    rc_package.verify_packaged(Path(directory) / "packages", output, "1.0.0")
                self.assertEqual(receipt.read_bytes(), before)
                check.assert_called_once()

    def test_standalone_interruption_removes_owned_output(self):
        def interrupt_check(packages, version, staging, environment):
            (staging / "partial-build").write_bytes(b"temporary")
            raise KeyboardInterrupt()

        with tempfile.TemporaryDirectory(prefix="zeno-fcis-verify-interrupt-") as directory:
            output = Path(directory) / "result"
            with mock.patch.object(rc_package, "check_packaged_workspace", side_effect=interrupt_check):
                with self.assertRaises(KeyboardInterrupt):
                    rc_package.verify_packaged(Path(directory) / "packages", output, "1.0.0")
            self.assertFalse(output.exists())


class CompilerFlagTests(unittest.TestCase):
    def test_spaced_paths_compile_and_documentation_warnings_remain_errors(self):
        with tempfile.TemporaryDirectory(prefix="zeno fcis compiler flags ") as directory:
            staging = Path(directory).resolve()
            root = staging / "probe app"
            root.mkdir()
            (root / "Cargo.toml").write_text(
                '[package]\nname = "compiler_flag_probe"\nversion = "0.0.0"\n'
                'edition = "2024"\n[workspace]\n[dependencies]\n'
                'probe-dep = { path = "../probe dep" }\n', encoding="utf-8",
            )
            dependency = staging / "probe dep"
            (dependency / "src").mkdir(parents=True)
            (dependency / "Cargo.toml").write_text(
                '[package]\nname = "probe-dep"\nversion = "0.0.0"\nedition = "2024"\n[workspace]\n',
                encoding="utf-8",
            )
            (dependency / "src/lib.rs").write_text(
                "pub fn origin() -> &'static str { file!() }\n", encoding="utf-8",
            )
            (root / "src").mkdir()
            (root / "src/main.rs").write_text(
                'fn main() { println!("{}", probe_dep::origin()); }\n', encoding="utf-8",
            )
            source = root / "src/lib.rs"
            source.write_text('/// A documented probe.\npub fn probe() {}\n', encoding="utf-8")
            inherited = {**os.environ, "CARGO_TARGET_DIR": str(root / "build target"),
                         "RUSTFLAGS": "--invalid-inherited-flag",
                         "RUSTDOCFLAGS": "--invalid-inherited-flag",
                         "CARGO_ENCODED_RUSTFLAGS": "--invalid-inherited-flag",
                         "CARGO_ENCODED_RUSTDOCFLAGS": "-A\x1fwarnings"}
            for name in ("RUSTC", "RUSTDOC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER",
                         "CARGO_BUILD_RUSTC", "CARGO_BUILD_RUSTDOC", "CARGO_BUILD_RUSTC_WRAPPER",
                         "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER", "CARGO_BUILD_TARGET"):
                inherited[name] = str(staging / "missing-tool")
            environment = rc_package.remapped_compiler_environment(
                inherited, staging, "/zeno-fcis-compiler-probe",
            )

            def cargo(*arguments):
                return subprocess.run(
                    ["cargo", "+1.97.1", *arguments], cwd=root, env=environment,
                    text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                    timeout=60,
                )

            for arguments in [("generate-lockfile", "--offline"),
                              ("check", "--locked", "--offline"),
                              ("doc", "--locked", "--offline", "--no-deps")]:
                result = cargo(*arguments)
                self.assertEqual(result.returncode, 0, result.stdout)
            result = cargo("run", "--quiet", "--locked", "--offline")
            self.assertEqual(result.returncode, 0, result.stdout)
            self.assertEqual(result.stdout.strip().replace("\\", "/"),
                             "/zeno-fcis-compiler-probe/probe dep/src/lib.rs")
            self.assertTrue((root / "build target/doc/compiler_flag_probe/fn.probe.html").is_file())
            source.write_text('/// See [MissingProbeType].\npub fn probe() {}\n', encoding="utf-8")
            result = cargo("doc", "--locked", "--offline", "--no-deps")
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertIn("unresolved link", result.stdout)
            self.assertIn("MissingProbeType", result.stdout)


if __name__ == "__main__":
    unittest.main()
