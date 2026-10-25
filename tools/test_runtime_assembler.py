#!/usr/bin/env python3
"""Unit refusal controls; mocked build steps are NOT installed-app evidence."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

TOOLS = Path(__file__).resolve().parent


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


assembler = load("runtime_assembler", TOOLS / "runtime_assembler.py")
launcher = load("assembly_launcher", TOOLS / "runtime-assembly/launch.py")
source_identity = assembler.source_identity


class Fixture(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="assembly-unit-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.root / "repository"
        self.source.mkdir()
        self.contract = self.source / "examples/apps/task-status"
        for selected in assembler.CATALOG.values():
            for name in selected["files"]:
                path = self.source / selected["contract"] / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("synthetic unit fixture " + selected["contract"] + "/" + name)
            journey = self.source / selected["journey"]
            journey.parent.mkdir(parents=True, exist_ok=True)
            journey.write_text('{"steps": []}')
        self.cli = self.root / "cli"
        self.cli.write_bytes(b"synthetic cli, never executed")
        self.root_patch = patch.object(assembler, "ROOT", self.source)
        self.root_patch.start()
        self.addCleanup(self.root_patch.stop)
        self.identity_patch = patch.object(assembler, "source_identity", return_value="ab" * 32)
        self.identity = self.identity_patch.start()
        self.addCleanup(self.identity_patch.stop)
        self.document = assembler.prepare(self.cli)
        self.description = self.root / "assembly.json"
        self.write_description()

    def write_description(self):
        self.description.write_bytes(assembler.canonical(self.document))

    def refuse(self, code, function, *args):
        with self.assertRaisesRegex(ValueError, code):
            function(*args)


class AssemblyTests(Fixture):
    def test_closed_profile_accepts_exact_fixture(self):
        assembler.validate(self.document)
        assembler.check_identity(self.document, self.cli)

    def test_inventory_catalog_has_distinct_exact_contract_roles_and_evidence(self):
        name = "inventory-reservation-sqlite-local-simulation/1"
        document = assembler.prepare(self.cli, name)
        assembler.validate(document)
        selected = assembler.profile(name)
        self.assertEqual(selected["roles"], [("operator", "1"), ("observer", "0")])
        self.assertEqual([(p["name"], p["role"]) for p in document["configuration"]["parties"]],
                         [("operator", "1"), ("observer", "0")])
        self.assertEqual(document["evidence"], ["native-contract-review", "zenoui-laws",
                                             "generated-app-journey", "inventory-local-runtime"])
        self.assertNotEqual(document["contract"]["project.zeno"], self.document["contract"]["project.zeno"])
        self.assertNotIn("tests/decision-scenarios.txt", document["contract"])
        self.assertEqual(selected["support"], "inventory_journey.rs")
        self.assertIn("assembly_inventory_authority_and_local_intent", selected["tests"])
        self.assertNotIn("assembly_timeout_is_unknown_and_correlated_observations_use_authority", selected["tests"])

    def test_inventory_cannot_substitute_task_contract_role_or_evidence(self):
        original = assembler.prepare(self.cli, "inventory-reservation-sqlite-local-simulation/1")
        for field, value, code in [
            ("evidence", self.document["evidence"], "AssemblyEvidenceRequirements"),
            ("contract", self.document["contract"], "AssemblyFields"),
            ("configuration", self.document["configuration"], "AssemblyParties"),
        ]:
            with self.subTest(field=field):
                document = copy.deepcopy(original)
                document[field] = value
                self.refuse(code, assembler.validate, document)
        document = copy.deepcopy(original)
        document["contract"]["project.zeno"] = self.document["contract"]["project.zeno"]
        self.refuse("AssemblyContractIdentity", assembler.validate, document)
        document = copy.deepcopy(original)
        document["configuration"]["parties"][0]["role"] = "170"
        self.refuse("AssemblyPartyBinding", assembler.validate, document)
        document = copy.deepcopy(self.document)
        document["profile"] = original["profile"]
        self.refuse("AssemblyEvidenceRequirements", assembler.validate, document)

    def test_optimized_python_cannot_disable_package_acceptance(self):
        checker = TOOLS / "runtime-assembly/check_package.py"
        code = ("import importlib.util; "
                "s=importlib.util.spec_from_file_location('acceptance', " + repr(str(checker)) + "); "
                "m=importlib.util.module_from_spec(s); s.loader.exec_module(m); "
                "m.require(False)")
        result = subprocess.run([sys.executable, "-O", "-c", code], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0, "optimized interpreter waived acceptance")
        self.assertIn("PackageAcceptanceFailed", result.stderr)
        self.assertNotIn("AssertionError", result.stderr)

    def test_decode_duplicate_unknown_nonfinite_and_size(self):
        for raw, code in [(b'{"x":1,"x":2}', "AssemblyDuplicateField"),
                          (b'{"x":NaN}', "AssemblyNonfiniteNumber"),
                          (b" " * (assembler.MAX_JSON + 1), "AssemblyByteLimit")]:
            with self.subTest(code=code):
                self.description.write_bytes(raw)
                self.refuse(code, assembler.decode, self.description)
        self.document["unexpected"] = True
        self.refuse("AssemblyFields", assembler.validate, self.document)

    def test_version_profile_capabilities_evidence_and_hash_refuse(self):
        changes = [("format", "zeno-fcis/app-assembly/2", "AssemblyVersion"),
                   ("profile", "collections/1", "AssemblyUnsupportedProfile"),
                   ("capabilities", assembler.CAPABILITIES + ["network"], "AssemblyCapabilities"),
                   ("capabilities", assembler.CAPABILITIES[:-1], "AssemblyCapabilities"),
                   ("evidence", ["passed"], "AssemblyEvidenceRequirements"),
                   ("evidence", [], "AssemblyEvidenceRequirements"),
                   ("cli_sha256", "UPPERCASE", "AssemblyDigest")]
        for field, value, code in changes:
            with self.subTest(field=field, value=value):
                document = copy.deepcopy(self.document)
                document[field] = value
                self.refuse(code, assembler.validate, document)

    def test_contract_substitution_and_unknown_schema_file(self):
        self.document["contract"]["project.zeno"] = "00" * 32
        self.refuse("AssemblyContractIdentity", assembler.validate, self.document)
        self.document["contract"]["new-schema"] = "00" * 32
        self.refuse("AssemblyFields", assembler.validate, self.document)

    def test_configuration_role_principal_and_capability_substitution(self):
        changes = [("address", "0.0.0.0:8080", "AssemblyConfigurationProfile"),
                   ("data_env", "ARBITRARY", "AssemblyConfigurationProfile")]
        for field, value, code in changes:
            config = copy.deepcopy(self.document["configuration"])
            config[field] = value
            self.refuse(code, assembler.validate_config, config)
        for field, value, code in [("role", "171", "AssemblyPartyBinding"),
                                   ("credential_env", "HOME", "AssemblyPartyBinding"),
                                   ("principal", "0" * 64, "AssemblyPrincipal")]:
            config = copy.deepcopy(self.document["configuration"])
            config["parties"][0][field] = value
            self.refuse(code, assembler.validate_config, config)
        config = copy.deepcopy(self.document["configuration"])
        config["parties"][1]["principal"] = config["parties"][0]["principal"]
        self.refuse("AssemblyPrincipal", assembler.validate_config, config)

    def test_source_and_cli_drift(self):
        self.identity.return_value = "cd" * 32
        self.refuse("AssemblySourceIdentity", assembler.check_identity, self.document, self.cli)
        self.identity.return_value = self.document["source_sha256"]
        self.cli.write_bytes(b"changed")
        self.refuse("AssemblyCliIdentity", assembler.check_identity, self.document, self.cli)

    def test_symlink_inputs_and_ancestor_refuse(self):
        link = self.root / "link"
        link.symlink_to(self.cli)
        self.refuse("AssemblySymlink", assembler.read, link)
        directory = self.root / "linked-directory"
        directory.symlink_to(self.source, target_is_directory=True)
        self.refuse("AssemblySymlink", assembler.plain, directory / "new")

    def test_parent_path_components_cannot_bypass_source_containment(self):
        output = self.root / "placeholder/../repository/new"
        with patch.object(assembler, "run_checked") as child:
            self.refuse("AssemblyOutputOrScratch", assembler.build, self.description,
                        self.cli, output, self.root / "scratch")
            child.assert_not_called()

    def test_existing_output_and_source_output_refuse_before_children(self):
        for output in [self.source / "new-app", self.source]:
            with self.subTest(output=output), patch.object(assembler, "run_checked") as child:
                self.refuse("AssemblyOutputOrScratch", assembler.build, self.description,
                            self.cli, output, self.root / "scratch")
                child.assert_not_called()

    def test_ambient_connector_refuses(self):
        with patch.dict(os.environ, {"ZENO_APP_CONNECTOR": "/tmp/injected"}, clear=True), \
             patch.object(assembler, "run_checked") as child:
            self.refuse("AssemblyAmbientAppConfiguration", assembler.build, self.description,
                        self.cli, self.root / "package", self.root / "scratch")
            child.assert_not_called()

    def test_external_build_flags_and_wrappers_refuse(self):
        for name in ["RUSTFLAGS", "RUSTC_WRAPPER", "CARGO_BUILD_RUSTFLAGS", "CC", "LD_PRELOAD", "PYTHONOPTIMIZE", "PYTHONPATH", "PYTHONHOME"]:
            with self.subTest(name=name), patch.dict(os.environ, {name: "injected"}, clear=True), \
                 patch.object(assembler, "run_checked") as child:
                self.refuse("AssemblyBuildEnvironment", assembler.build, self.description,
                            self.cli, self.root / "package", self.root / "scratch")
                child.assert_not_called()

    def test_ambient_cargo_home_and_parent_config_refuse(self):
        cargo_home = self.root / "cargo-home"
        cargo_home.mkdir()
        config = cargo_home / "config.toml"
        config.write_text('[build]\nrustflags = ["--cfg", "injected"]\n')
        with patch.dict(os.environ, {"CARGO_HOME": str(cargo_home)}, clear=True), \
             patch.object(assembler, "run_checked") as child:
            self.refuse("AssemblyAmbientCargoConfiguration", assembler.build, self.description,
                        self.cli, self.root / "package", self.root / "scratch")
            child.assert_not_called()
        config.unlink()
        parent_config = self.root / ".cargo/config"
        parent_config.parent.mkdir()
        parent_config.write_text("[build]\n")
        with patch.dict(os.environ, {"CARGO_HOME": str(cargo_home)}, clear=True), \
             patch.object(assembler, "run_checked") as child:
            self.refuse("AssemblyAmbientCargoConfiguration", assembler.build, self.description,
                        self.cli, self.root / "package", self.root / "scratch")
            child.assert_not_called()

    def test_failed_factory_leaves_no_published_package(self):
        output = self.root / "package"
        calls = []
        def child(command, log, cwd, environment):
            calls.append(log.name)
            if log.name.endswith("-version.log"):
                log.write_text("mock version; not qualification")
                return {"check": log.stem, "exit": 0}
            self.assertEqual(log.name, "factory.log")
            raise ValueError("AssemblyCheckFailed:factory.log")
        with patch.dict(os.environ, {}, clear=True), \
             patch.object(assembler, "run_checked", side_effect=child):
            self.refuse("AssemblyCheckFailed:factory.log", assembler.build, self.description,
                        self.cli, output, self.root / "scratch")
        self.assertEqual(calls, ["rustc-version.log", "cargo-version.log", "node-version.log", "python-version.log", "factory.log"])
        self.assertFalse(output.exists())
        self.assertEqual(list(self.root.glob(".assembly-*")), [])

    def test_source_inventory_binds_bytes_and_rejects_links(self):
        for name in ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml"]:
            (self.source / name).write_text(name)
        tool = self.source / "tools/check.py"
        tool.parent.mkdir(exist_ok=True)
        tool.write_text("first")
        before = source_identity(self.source)
        tool.write_text("changed")
        self.assertNotEqual(source_identity(self.source), before)
        tool.unlink()
        tool.symlink_to(self.cli)
        self.refuse("AssemblySourceSymlink", source_identity, self.source)

    def test_runtime_failure_also_leaves_no_published_package(self):
        helper = self.source / "tools/runtime-assembly/task_journey.rs"
        helper.parent.mkdir(parents=True, exist_ok=True)
        helper.write_text("// synthetic unit helper")
        calls = []

        def child(command, log, cwd, environment):
            calls.append(command)
            if log.name.endswith("-version.log"):
                log.write_text("mock version; not qualification")
                return {"check": log.stem, "exit": 0}
            if log.name == "factory.log":
                app = Path(command[2])
                (app / "tests").mkdir(parents=True)
                (app / "tests/app_journey.rs").write_text("// mock only")
                (app / "app-manifest.json").write_text("{}")
                (app / "app-checks").mkdir()
                (app / "app-checks/journey.json").write_text("{}")
                log.write_text("mock factory success; not qualification")
                return {"check": "factory", "exit": 0}
            raise ValueError("AssemblyCheckFailed:runtime.log")

        output = self.root / "package"
        with patch.dict(os.environ, {}, clear=True), patch.object(assembler, "run_checked", side_effect=child):
            self.refuse("AssemblyCheckFailed:runtime.log", assembler.build, self.description,
                        self.cli, output, self.root / "scratch")
        self.assertEqual(len(calls), 6)
        self.assertFalse(output.exists())
        self.assertEqual(list(self.root.glob(".assembly-*")), [])

    def test_nonzero_child_is_never_evidence(self):
        result = subprocess.CompletedProcess(["fake"], 7)
        with patch.object(assembler.subprocess, "run", return_value=result):
            self.refuse("AssemblyCheckFailed", assembler.run_checked, ["fake"],
                        self.root / "child.log", self.root, {})


class LaunchTests(Fixture):
    def setUp(self):
        super().setUp()
        self.package = self.root / "package"
        self.package.mkdir()
        for name in ["app-server", "launch.py", "evidence/checks.json"]:
            path = self.package / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"synthetic mocked unit artifact")
        self.environment = {"ZENO_ASSEMBLY_DATA": str(self.root / "data")}
        for index, name in enumerate(["operator", "adapter", "observer"], 1):
            path = self.root / (name + ".secret")
            path.write_text(f"{index:02x}" * 32 + "\n")
            path.chmod(0o600)
            self.environment["ZENO_ASSEMBLY_" + name.upper() + "_SECRET"] = str(path)
        self.rebind()

    def rebind(self):
        """Synthetic owner-approved package fixture; never real build evidence."""
        (self.package / "assembly.json").write_bytes(assembler.canonical(self.document))
        names = ["app-server", "launch.py", "assembly.json", "evidence/checks.json"]
        self.receipt = {"format": "zeno-fcis/assembled-app/1", "authority": "none",
                        "assembly_sha256": assembler.digest(assembler.canonical(self.document)),
                        "files": assembler.files(self.package, names)}
        self.write_receipt()

    def write_receipt(self):
        (self.package / "package.json").write_bytes(assembler.canonical(self.receipt))

    def test_exact_package_validates_and_launches_only_closed_environment(self):
        data, config, parties = launcher.validate(self.package, self.environment)
        self.assertEqual(data, self.root / "data")
        self.assertEqual([p["name"] for p in parties], ["operator", "adapter", "observer"])
        with patch.object(launcher.os, "execve") as execute:
            launcher.launch(self.package, {**self.environment, "UNRELATED_SECRET": "must-not-propagate"})
        child = execute.call_args.args[2]
        self.assertEqual(set(child), {"PATH", "ZENO_APP_ADDRESS", "ZENO_APP_PARTIES", "ZENO_APP_STORE", "ZENO_APP_RECEIVER"})
        self.assertEqual(child["ZENO_APP_RECEIVER"], str(data / "receiver"))
        self.assertNotIn(b"0101010101", (data / "parties.json").read_bytes())

    def test_inventory_launcher_uses_two_exact_parties_not_task_roles(self):
        self.document = assembler.prepare(self.cli, "inventory-reservation-sqlite-local-simulation/1")
        self.rebind()
        _, config, parties = launcher.validate(self.package, self.environment)
        self.assertEqual([(p["name"], p["role"]) for p in config["parties"]],
                         [("operator", "1"), ("observer", "0")])
        self.assertEqual([p["name"] for p in parties], ["operator", "observer"])
        self.document["configuration"]["parties"][0]["role"] = "170"
        self.rebind()
        self.refuse("LaunchPartyBinding", launcher.validate, self.package, self.environment)

    def test_store_cannot_switch_between_catalog_profiles(self):
        with patch.object(launcher.os, "execve"):
            launcher.launch(self.package, self.environment)
        binding = self.root / "data/assembly-binding.json"
        before = binding.read_bytes()
        self.document = assembler.prepare(self.cli, "inventory-reservation-sqlite-local-simulation/1")
        self.rebind()
        with patch.object(launcher.os, "execve") as execute:
            self.refuse("LaunchStoredBindingDrift", launcher.launch, self.package, self.environment)
            execute.assert_not_called()
        self.assertEqual(binding.read_bytes(), before)

    def test_artifact_drift_and_missing_artifact_refuse(self):
        (self.package / "app-server").write_bytes(b"substitution")
        self.refuse("LaunchArtifactDrift", launcher.validate, self.package, self.environment)
        (self.package / "app-server").unlink()
        with self.assertRaises(OSError):
            launcher.validate(self.package, self.environment)

    def test_assembly_binding_and_manifest_paths_refuse(self):
        self.receipt["assembly_sha256"] = "00" * 32
        self.write_receipt()
        self.refuse("LaunchAssemblyBinding", launcher.validate, self.package, self.environment)
        for name in ["../escape", "/absolute", "evidence/../app-server", "./app-server"]:
            with self.subTest(name=name):
                self.receipt["files"][name] = "00" * 32
                self.write_receipt()
                self.refuse("LaunchManifestPath", launcher.validate, self.package, self.environment)
                del self.receipt["files"][name]

    def test_connector_environment_override_refuses(self):
        self.environment["ZENO_APP_CONNECTOR"] = "/tmp/substituted"
        self.refuse("LaunchAmbientAppConfiguration", launcher.validate, self.package, self.environment)

    def test_duplicate_manifest_fields_refuse(self):
        (self.package / "package.json").write_text('{"format":1,"format":2}')
        self.refuse("LaunchDuplicateField", launcher.validate, self.package, self.environment)

    def test_unsupported_profile_and_capability_manifest_refuse(self):
        for field, value, code in [("profile", "remote-payments/1", "LaunchProfile"),
                                   ("capabilities", ["network"], "LaunchCapabilities")]:
            original = self.document[field]
            self.document[field] = value
            self.rebind()
            self.refuse(code, launcher.validate, self.package, self.environment)
            self.document[field] = original

    def test_bad_secret_permissions_format_duplicate_and_symlink(self):
        secret = Path(self.environment["ZENO_ASSEMBLY_OPERATOR_SECRET"])
        secret.chmod(0o644)
        self.refuse("LaunchSecretPermissions", launcher.validate, self.package, self.environment)
        secret.chmod(0o600)
        for value in ["0" * 64, "xyz", "02" * 32]:
            with self.subTest(value=value[:3]):
                secret.write_text(value)
                self.refuse("LaunchSecretFormat", launcher.validate, self.package, self.environment)
        secret.unlink()
        secret.symlink_to(self.environment["ZENO_ASSEMBLY_ADAPTER_SECRET"])
        self.refuse("LaunchSymlink", launcher.validate, self.package, self.environment)

    def test_data_cannot_be_inside_package_or_traverse_into_it(self):
        for path in [self.package / "data", self.root / "placeholder/../package/data"]:
            with self.subTest(path=path):
                self.environment["ZENO_ASSEMBLY_DATA"] = str(path)
                self.refuse("Launch", launcher.validate, self.package, self.environment)

    def test_existing_unbound_database_refuses_without_changes(self):
        data = self.root / "data"
        data.mkdir(mode=0o700)
        database = data / "store.sqlite"
        database.write_bytes(b"old-schema-data")
        with patch.object(launcher.os, "execve") as execute:
            self.refuse("LaunchUnboundExistingData", launcher.launch, self.package, self.environment)
            execute.assert_not_called()
        self.assertEqual(database.read_bytes(), b"old-schema-data")
        self.assertEqual(list(data.iterdir()), [database])

    def test_retained_configuration_or_schema_binding_change_refuses(self):
        with patch.object(launcher.os, "execve"):
            launcher.launch(self.package, self.environment)
        data = self.root / "data"
        before = (data / "assembly-binding.json").read_bytes()
        for mutate in [lambda: self.document["configuration"]["parties"][0].update(principal="b1" * 32),
                       lambda: self.document["contract"].update({"v2/schema.zcve": "00" * 32})]:
            mutate()
            self.rebind()
            with patch.object(launcher.os, "execve") as execute:
                self.refuse("LaunchStoredBindingDrift", launcher.launch, self.package, self.environment)
                execute.assert_not_called()
            self.assertEqual((data / "assembly-binding.json").read_bytes(), before)

    def test_retained_context_drift_refuses(self):
        with patch.object(launcher.os, "execve"):
            launcher.launch(self.package, self.environment)
        (self.root / "data/operator.context.json").write_text('["171"]\n')
        with patch.object(launcher.os, "execve") as execute:
            self.refuse("LaunchContextDrift", launcher.launch, self.package, self.environment)
            execute.assert_not_called()


if __name__ == "__main__":
    unittest.main()
