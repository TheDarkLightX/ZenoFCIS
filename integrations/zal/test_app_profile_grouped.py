"""Opt-in grouped presentation preserves complete app bindings and decisions."""
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import app_profile
import behavior2
from test_app_profile import MAX_SHAPE, SOURCE, binding_for, encode


LEGACY_APP_HASHES = {
    "project.zeno": "b6a1fc01945cb8f791befaf666bc6a859065604922d727e5e69113574fbcbaf9",
    "tests/decision-examples.txt": "e1ca8bbb4b7a1badb34018a7c72afc64e499de04d7c8e5621c4e3045428474fb",
    "tests/zal-authority-examples.txt": "efe3178d3f3f4950353ef9ad46605ec64c29c062c8cebb87cdf747a7098769b6",
    "tests/zal_correspondence.rs": "7f79bdeb77c923fec222814db8314d3fec38239e5641275817c5e23978cd43f0",
    "ui.json": "6c4b37dc5a33c229033523efaddc4e6693e103eede90b7dd680616bbb52d9353",
    "v2/policy.json": "204b4ae19b400ce76f22af716f363a0f6040e40d3571640cbd93a2a9c5a6583e",
    "zal-app-binding.json": "76b7d1ab106d2ef993afb480aee9cbb1ee272981785a120b84d40ee3ba4ac918",
    "zal-original.zal": "b06b4c74aee0da89b23206555ac8ad903f0d4a2273f776867852826b57b69efa",
    "zal-source-map.json": "a3ff10bcf577d7839c57041cb09e2171fe4bdc887ef6c3f775fc6c58ee2c8e80",
}
PRESENTATION = {"format": "zal/app-presentation/1", "commands": "grouped"}


def grouped(binding):
    result = copy.deepcopy(binding)
    result["ui"]["presentation"] = dict(PRESENTATION)
    return result


class GroupedAppProfileTests(unittest.TestCase):
    def setUp(self):
        self.model = behavior2.parse(SOURCE)
        self.binding = binding_for(self.model)

    def lower(self, source, binding):
        with patch("subprocess.run", side_effect=AssertionError("pure export must not launch a process")):
            return app_profile.lower(source, binding)

    def test_absent_intent_keeps_complete_original_app_bytes(self):
        files = self.lower(SOURCE, encode(self.binding))
        self.assertEqual({n: hashlib.sha256(t.encode()).hexdigest() for n, t in files.items()}, LEGACY_APP_HASHES)
        self.assertEqual(json.loads(files["ui.json"])["format"], "zenoui/metadata/1")

    def test_explicit_intent_emits_closed_conservative_requirements(self):
        files = self.lower(SOURCE, encode(grouped(self.binding)))
        ui = json.loads(files["ui.json"])
        self.assertEqual(ui["format"], "zenoui/metadata/2")
        self.assertEqual(ui["requirements"], {
            "format": "zenoui/requirements/2", "primaryFields": ["command.101.164"],
            "detailFields": [], "fixedFields": [], "contextFields": ["context.102.160"],
            "groups": [], "groupCommands": True, "collapseExpert": True, "compact": True,
            "maxVisibleForms": 1, "maxExpandedExpertPanels": 0, "maxColumns": 2,
            "projections": [], "quoteOverrides": {}})
        self.assertEqual(set(ui), {"format", "title", "action", "fields", "variants", "commands", "reasons", "requirements"})
        self.assertTrue(all(c["consequential"] is True for c in ui["commands"].values()))

    def test_grouping_changes_no_executable_reference_or_correspondence_bytes(self):
        legacy = self.lower(SOURCE, encode(self.binding))
        new = self.lower(SOURCE, encode(grouped(self.binding)))
        self.assertEqual(set(legacy), set(new))
        changed = {n for n in new if new[n] != legacy[n]}
        self.assertEqual(changed, {"ui.json", "zal-app-binding.json", "zal-source-map.json"})
        before, after = (json.loads(f["zal-source-map.json"]) for f in (legacy, new))
        self.assertEqual(after["revision"], self.model.revision)
        for key in ("binding_sha256", "canonical_binding_sha256", "ui_sha256"):
            self.assertNotEqual(before.pop(key), after.pop(key))
        self.assertEqual(before, after)
        before_ui, after_ui = (json.loads(f["ui.json"]) for f in (legacy, new))
        after_ui.pop("requirements"); after_ui["format"] = "zenoui/metadata/1"
        self.assertEqual(before_ui, after_ui)

    def test_all_bound_inputs_are_retained_by_root_and_stable_field_order(self):
        model = behavior2.parse(MAX_SHAPE)
        origins = {"a": "command", "b": "server-context", "c": "command", "d": "server-context",
                   "left": "server-context", "right": "command"}
        binding = grouped(binding_for(model, origins))
        binding["observations"].reverse()  # Authored array order cannot reorder the ABI or omit an input.
        files = self.lower(MAX_SHAPE, encode(binding))
        req = json.loads(files["ui.json"])["requirements"]
        self.assertEqual(req["primaryFields"], ["command.101.160", "command.101.162", "command.101.165"])
        self.assertEqual(req["contextFields"], ["context.102.161", "context.102.163", "context.102.164"])
        mapping = json.loads(files["zal-source-map.json"])
        self.assertEqual(set(req["primaryFields"] + req["contextFields"]), {v["path"] for v in mapping["bindings"].values()})

    def test_unit_context_is_retained_and_grouping_does_not_infer_authentication(self):
        model = behavior2.parse(MAX_SHAPE)
        origins = {name: "command" for name in (*model.facts, *[x.name for x in model.inputs])}
        files = self.lower(MAX_SHAPE, encode(grouped(binding_for(model, origins))))
        req = json.loads(files["ui.json"])["requirements"]
        self.assertEqual(req["contextFields"], ["context.102.159"])
        self.assertEqual(req["primaryFields"], [f"command.101.{n}" for n in range(160, 166)])
        self.assertEqual(req["fixedFields"], [])
        mapping = json.loads(files["zal-source-map.json"])
        self.assertIn("host responsibility", mapping["authentication"])
        declared = grouped(binding_for(self.model, {"authorized": "command"}))
        files = self.lower(SOURCE, encode(declared))
        self.assertIn("command.101.160", json.loads(files["ui.json"])["requirements"]["primaryFields"])

    def test_malformed_unknown_or_extra_intent_refuses_without_fallback(self):
        invalid = [None, True, 1, [], {}, {"format": "zal/app-presentation/1"},
                   {"commands": "grouped"}, {"format": "unknown", "commands": "grouped"}]
        invalid.extend({"format": "zal/app-presentation/1", "commands": v} for v in (True, 1, "separate", None, ["grouped"]))
        invalid.extend([{**PRESENTATION, "hide": ["quantity"]}, {**PRESENTATION, "maxVisibleForms": 0}])
        for intent in invalid:
            binding = grouped(self.binding); binding["ui"]["presentation"] = intent
            with self.subTest(intent=intent), self.assertRaises(behavior2.Refusal):
                self.lower(SOURCE, encode(binding))
        text = encode(grouped(self.binding)).replace('"commands": "grouped"', '"commands": "grouped", "commands": "grouped"')
        with self.assertRaisesRegex(behavior2.Refusal, "duplicate JSON key"):
            self.lower(SOURCE, text)

    def test_cli_preserves_complete_utf8_crlf_bundle_and_actual_hashes(self):
        source = SOURCE.replace("\n", "\r\n")
        binding = grouped(self.binding); binding["ui"]["title"] = "Réparation — 雪"
        raw_binding = (json.dumps(binding, ensure_ascii=False, indent=2) + "\n").replace("\n", "\r\n")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); src = root / "input.zal"; given = root / "binding.json"
            src.write_bytes(source.encode()); given.write_bytes(raw_binding.encode())
            command = [sys.executable, "-B", str(Path(app_profile.__file__).resolve()),
                       "--source", str(src), "--binding", str(given), "--out", str(root / "contract")]
            result = subprocess.run(command, capture_output=True, text=True, timeout=15, check=False)
            self.assertEqual(result.returncode, 0, result.stderr)
            files = self.lower(source, raw_binding)
            for name, data in files.items():
                self.assertEqual((root / "contract" / name).read_bytes(), data.encode())
            self.assertEqual((root / "contract/zal-original.zal").read_bytes(), src.read_bytes())
            self.assertEqual((root / "contract/zal-app-binding.json").read_bytes(), given.read_bytes())
            mapping = json.loads((root / "contract/zal-source-map.json").read_bytes())
            for key, name in [("source_sha256", "zal-original.zal"), ("binding_sha256", "zal-app-binding.json"),
                              ("project_sha256", "project.zeno"), ("policy_sha256", "v2/policy.json"),
                              ("ui_sha256", "ui.json"), ("reference_sha256", "tests/zal-authority-examples.txt")]:
                self.assertEqual(mapping[key], hashlib.sha256((root / "contract" / name).read_bytes()).hexdigest())
            self.assertEqual(mapping["canonical_binding_sha256"], hashlib.sha256(json.dumps(binding, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode("ascii")).hexdigest())

    def test_cli_invalid_intent_refuses_before_output_and_keeps_existing_bundle(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); src = root / "input.zal"; given = root / "binding.json"
            src.write_text(SOURCE); given.write_text(encode(grouped(self.binding)))
            command = [sys.executable, "-B", str(Path(app_profile.__file__).resolve()),
                       "--source", str(src), "--binding", str(given), "--out", str(root / "existing")]
            first = subprocess.run(command, capture_output=True, text=True, timeout=15, check=False)
            self.assertEqual(first.returncode, 0, first.stderr)
            before = {str(p.relative_to(root / "existing")): p.read_bytes() for p in (root / "existing").rglob("*") if p.is_file()}
            bad = grouped(self.binding); bad["ui"]["presentation"]["commands"] = "unknown"
            given.write_text(encode(bad))
            for destination in [root / "invalid", root / "existing"]:
                command[-1] = str(destination)
                refused = subprocess.run(command, capture_output=True, text=True, timeout=15, check=False)
                self.assertEqual(refused.returncode, 2)
                self.assertEqual(json.loads(refused.stderr)["status"], "refused")
            self.assertFalse((root / "invalid").exists())
            self.assertEqual(before, {str(p.relative_to(root / "existing")): p.read_bytes() for p in (root / "existing").rglob("*") if p.is_file()})


if __name__ == "__main__":
    unittest.main(verbosity=2)
