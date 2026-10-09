"""Planted false-success controls for the live operational journey gate."""

import copy
import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import check_contract_upgrade as upgrades
import check_operational_journey as journey


def write_app(directory: Path, name: str = "spend-approval") -> Path:
    app = directory / "app"
    app.mkdir()
    (app / "Cargo.toml").write_text(f'[package]\nname = "{name}"\nversion = "0.1.0"\n')
    return app


def artifact(app: Path, executable, *, name: str = "spend-approval",
             kind: tuple = ("bin",), manifest: Path | None = None) -> dict:
    return {"reason": "compiler-artifact", "manifest_path": str(manifest or app / "Cargo.toml"),
            "target": {"kind": list(kind), "name": name}, "executable": executable}


class BuildAppFindsCargoBinary(unittest.TestCase):
    def fake_run(self, records: list[dict]) -> mock.Mock:
        return mock.Mock(return_value="\n".join(json.dumps(record) for record in records))

    def test_absent_target_env_uses_reported_artifact(self):
        with tempfile.TemporaryDirectory() as directory, \
                mock.patch.dict(os.environ):
            os.environ.pop("CARGO_TARGET_DIR", None)
            app = write_app(Path(directory))
            built = app / "target/debug/spend-approval"
            built.parent.mkdir(parents=True)
            built.write_bytes(b"binary")
            records = [
                artifact(app, "target/debug/build/dep-1/build-script",
                         name="dep", kind=("custom-build",), manifest=Path(directory) / "dep/Cargo.toml"),
                artifact(app, None, name="dep", kind=("lib",), manifest=Path(directory) / "dep/Cargo.toml"),
                artifact(app, "target/debug/spend-approval"),
                {"reason": "build-finished", "success": True},
            ]
            run = self.fake_run(records)
            with mock.patch.object(upgrades.applications, "run", run):
                result = upgrades.build_app(app)
            self.assertEqual(result, app / "spend-approval.bin")
            self.assertEqual(result.read_bytes(), b"binary")
            command, cwd = run.call_args.args[:2]
            self.assertEqual(command, ["cargo", "+1.97.1", "build", "--locked", "--offline",
                                       "--message-format=json"])
            self.assertEqual(cwd, app)
            self.assertTrue(run.call_args.kwargs["capture"])

    def test_dependency_and_foreign_manifest_artifacts_are_ignored(self):
        with tempfile.TemporaryDirectory() as directory:
            app = write_app(Path(directory))
            foreign = Path(directory) / "foreign"
            foreign.mkdir()
            (foreign / "Cargo.toml").write_text('[package]\nname = "spend-approval"\n')
            for misleading in ("foreign-bin", "other-target"):
                (foreign / misleading).write_bytes(b"not ours")
            real = Path(directory) / "elsewhere/tool"
            real.parent.mkdir()
            real.write_bytes(b"ours")
            records = [
                artifact(app, str(foreign / "foreign-bin"), manifest=foreign / "Cargo.toml"),
                artifact(app, str(foreign / "other-target"), name="other"),
                artifact(app, str(real)),
                {"reason": "build-finished", "success": True},
            ]
            with mock.patch.object(upgrades.applications, "run", self.fake_run(records)):
                result = upgrades.build_app(app)
            self.assertEqual(result.read_bytes(), b"ours")

    def test_missing_matching_artifact_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            app = write_app(Path(directory))
            records = [artifact(app, "libdep.rlib", name="dep", kind=("lib",)),
                       {"reason": "build-finished", "success": True}]
            with mock.patch.object(upgrades.applications, "run", self.fake_run(records)), \
                    self.assertRaisesRegex(RuntimeError, "found 0"):
                upgrades.build_app(app)

    def test_duplicate_matching_artifacts_are_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            app = write_app(Path(directory))
            records = [artifact(app, "a"), artifact(app, "b"),
                       {"reason": "build-finished", "success": True}]
            with mock.patch.object(upgrades.applications, "run", self.fake_run(records)), \
                    self.assertRaisesRegex(RuntimeError, "found 2"):
                upgrades.build_app(app)


class PlantedControls(unittest.TestCase):
    def test_a_silently_skipped_step_fails(self):
        journey.check_steps(list(journey.STEPS))
        for skipped in journey.STEPS:
            with self.subTest(skipped=skipped), self.assertRaisesRegex(RuntimeError, "skipped"):
                journey.check_steps([step for step in journey.STEPS if step != skipped])

    def test_duplicate_effect_fails_even_under_the_same_delivery_id(self):
        record = {"effects": [{"key": "payment"}], "conflicts": 0}
        journey.check_effects(record, ["payment"])
        record["effects"].append({"key": "payment"})
        with self.assertRaisesRegex(RuntimeError, "duplicated"):
            journey.check_effects(record, ["payment"])

    def test_a_migration_changing_any_observation_fails(self):
        original = {"class": "Accept", "reason": None, "state": {"tier": 2},
                    "deliveries": [{"channel": 300, "payload": {"payment_tier": 2}}]}
        journey.check_observations(original, copy.deepcopy(original))
        for key, wrong in (("class", "Reject"), ("reason", 200),
                           ("state", {"tier": 0}), ("deliveries", [])):
            mutant = {**original, key: wrong}
            with self.subTest(key=key), self.assertRaisesRegex(RuntimeError, "changed a decision"):
                journey.check_observations(original, mutant)

    def test_changed_payload_under_the_same_key_fails(self):
        body = {"payload": "original", "destination": "bank"}
        record = {"effects": [{"key": "payment", "body": json.dumps(body)}], "conflicts": 0}
        journey.check_effects(record, ["payment"], {"payment": body})
        record["effects"][0]["body"] = json.dumps({**body, "payload": "wrong"})
        with self.assertRaisesRegex(RuntimeError, "changed destination or payload"):
            journey.check_effects(record, ["payment"], {"payment": body})


if __name__ == "__main__":
    unittest.main()
