#!/usr/bin/env python3
"""Pure-Python tests of the released-2.6.0 check: the executable pin and the
scenario's inputs. Nothing here builds or runs a CLI."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import check_contract_upgrade_26 as released


def write_manifest(root: Path, **fields) -> None:
    manifest = {"schema": "zeno-fcis/released-26-test-inputs/1", "release": released.RELEASE,
                "subject": released.SUBJECT, "released_cli_sha256": "0" * 64, **fields}
    data = root / released.SHELL_DATA
    data.mkdir(parents=True)
    (data / "manifest.json").write_text(json.dumps(manifest))


def executable(directory: Path, content: bytes) -> tuple[Path, str]:
    path = directory / "zeno-fcis"
    path.write_bytes(content)
    return path, hashlib.sha256(content).hexdigest()


def namespace(**given) -> argparse.Namespace:
    fields = {"released_cli": None, "released_target": None, "released_source": None}
    return argparse.Namespace(**{**fields, **given})


class PinnedExecutable(unittest.TestCase):
    def test_the_retained_manifest_pins_one_executable(self):
        pinned = released.pinned_cli_sha256(released.applications.ROOT)
        manifest = json.loads((released.applications.ROOT / released.SHELL_DATA / "manifest.json").read_text())
        self.assertEqual(pinned, manifest["released_cli_sha256"])
        self.assertRegex(pinned, r"^[0-9a-f]{64}$")

    def test_the_pinned_executable_passes_and_any_other_is_refused_by_name(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cli, actual = executable(root, b"released cli bytes")
            write_manifest(root, released_cli_sha256=actual)
            self.assertEqual(released.check_pinned_cli(cli, root), actual)
            (root / "other").mkdir()
            other, other_digest = executable(root / "other", b"another build")
            with self.assertRaises(RuntimeError) as refused:
                released.check_pinned_cli(other, root)
            message = str(refused.exception)
            self.assertIn(other_digest, message)
            self.assertIn(actual, message)
            self.assertIn("--write-test-data", message)

    def test_a_manifest_for_another_release_or_without_a_pin_is_refused(self):
        for fields in ({"release": "2.5.0"}, {"subject": "f" * 40}, {"released_cli_sha256": "short"}):
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                write_manifest(root, **fields)
                with self.assertRaises(RuntimeError):
                    released.pinned_cli_sha256(root)

    def test_a_wrong_executable_is_refused_before_any_store_is_made(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cli, _ = executable(root, b"not the pinned build")
            write_manifest(root)
            def never(*_args, **_kwargs):
                raise AssertionError("the released stores must not be made")
            with patch.object(released.applications, "ROOT", root), \
                    patch.object(released, "released", never):
                with self.assertRaises(RuntimeError) as refused:
                    released.check(root / "work", cli, root / "source", root / "current", False)
            self.assertIn("--write-test-data", str(refused.exception))

    def test_the_producer_run_skips_the_pin_and_records_none(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cli, _ = executable(root, b"a new published build")
            write_manifest(root)
            calls = []
            with patch.object(released.applications, "ROOT", root), \
                    patch.object(released, "released", lambda *a: calls.append("released") or {"r": 1}), \
                    patch.object(released, "export_test_data", lambda *a: calls.append("export") or {"e": 1}), \
                    patch.object(released, "current", lambda *a: calls.append("current") or {"c": 1}):
                result = released.check(root / "work", cli, root / "source", root / "current", True)
            self.assertEqual(calls, ["released", "export", "current"])
            self.assertIsNone(result["pinned_cli_sha256"])
            self.assertEqual(result["exported"], {"e": 1})

    def test_the_check_run_records_the_pin_it_verified(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cli, actual = executable(root, b"the pinned build")
            write_manifest(root, released_cli_sha256=actual)
            with patch.object(released.applications, "ROOT", root), \
                    patch.object(released, "released", lambda *a: {}), \
                    patch.object(released, "current", lambda *a: {}):
                result = released.check(root / "work", cli, root / "source", root / "current", False)
            self.assertEqual(result["pinned_cli_sha256"], actual)
            self.assertIsNone(result["exported"])


class ScenarioInputs(unittest.TestCase):
    def test_options_win_over_the_variables(self):
        args = namespace(released_cli=Path("/opt/cli"), released_source=Path("/opt/src"))
        environment = {released.RELEASED_CLI_ENV: "/env/cli", released.RELEASED_SOURCE_ENV: "/env/src"}
        self.assertEqual(released.resolve_inputs(args, environment), (Path("/opt/cli"), Path("/opt/src")))

    def test_the_variables_supply_both_inputs(self):
        environment = {released.RELEASED_CLI_ENV: "/env/cli", released.RELEASED_SOURCE_ENV: "/env/src"}
        self.assertEqual(released.resolve_inputs(namespace(), environment), (Path("/env/cli"), Path("/env/src")))

    def test_a_build_target_needs_no_executable(self):
        args = namespace(released_target=Path("/tmp/target"), released_source=Path("/opt/src"))
        self.assertEqual(released.resolve_inputs(args, {}), (None, Path("/opt/src")))

    def test_a_missing_or_empty_input_names_its_variable(self):
        cases = (
            ({}, released.RELEASED_CLI_ENV),
            ({released.RELEASED_CLI_ENV: ""}, released.RELEASED_CLI_ENV),
            ({released.RELEASED_CLI_ENV: "/env/cli"}, released.RELEASED_SOURCE_ENV),
            ({released.RELEASED_CLI_ENV: "/env/cli", released.RELEASED_SOURCE_ENV: ""},
             released.RELEASED_SOURCE_ENV),
        )
        for environment, variable in cases:
            with self.assertRaises(SystemExit) as stop:
                released.resolve_inputs(namespace(), environment)
            self.assertIn(variable, str(stop.exception))


if __name__ == "__main__":
    unittest.main()
