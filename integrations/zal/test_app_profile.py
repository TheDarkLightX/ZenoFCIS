"""Explicit app bindings retain the finite model's complete original decisions."""
import copy
import hashlib
import itertools
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import app_profile
import behavior2
import factory

SOURCE = """zal 2;
machine repair_request;
states draft, submitted;
events request, reset;
context authorized;
input quantity in 1..=2;
var units in 0..=2 initially 0;
initial draft;
step atomic; frame control_and_variables;
default reject not_enabled unchanged;
rule request_ok: draft + request [authorized] -> accept submitted, units = quantity;
rule request_refused: draft + request [!authorized] -> reject denied unchanged;
rule reset_ok: submitted + reset [authorized] -> accept draft, units = 0;
require approve_only_if_authorized: (event == request & accepted) => authorized;
"""

MAX_SHAPE = """zal 2;
machine max_shape;
states idle, ready;
events engage, reset;
context a, b, c, d;
input left in 0..=0;
input right in 0..=0;
var alpha in 0..=0 initially 0;
var beta in 0..=0 initially 0;
var gamma in 0..=0 initially 0;
var omega in 0..=0 initially 0;
initial idle;
step atomic; frame control_and_variables;
default reject not_enabled unchanged;
rule run: idle + engage [a & !b & c & !d] -> accept ready;
rule reset: ready + reset [true] -> accept idle;
"""

LEGACY_HASHES = {
    "project.zeno": "0e5ffff4b18798a0b83fb09f60415808e85ce9878ac876071add051a43fe16c5",
    "v2/policy.json": "faf0cd869ae40b1fa41bcb5024f2f1ab97d0ee2c7c98869844e8c3835428dd77",
    "tests/decision-examples.txt": "1e68cb9d709e1660f47179a556c2783714a043d31d24988fbeb37e827b32cba6",
    "tests/zal-authority-examples.txt": "1a119d61be58f067179b373b7d3c535c376129ca4c722cb90ea59c9321869b2c",
    "tests/zal_correspondence.rs": "dc9b3268b4aff4e044da732c87f84f3d3a8964853db71fa0c5e44a290e23c2cf",
    "zal-source-map.json": "6cd8a56537789370978ba65c3fc43b96e7e587fc2f8f3db475d0354a58cd3e1e",
}


def binding_for(model, origins=None):
    origins = origins or {}
    observations = []
    for kind, names in (("fact", model.facts), ("input", [item.name for item in model.inputs])):
        for name in names:
            source = origins.get(name, "server-context" if kind == "fact" else "command")
            role = "intent" if source == "command" else "authorization" if name == "authorized" else "observation"
            observations.append({"name": name, "kind": kind, "source": source, "role": role,
                                 "label": name.title(), "unit": "unitless" if kind == "fact" else "items"})
    return {"format": "zal/app-binding/1", "model_revision": model.revision, "observations": observations,
            "ui": {"title": "Repair request", "action": {"label": "Action", "unit": "unitless"},
                   "control": {"label": "Request state", "unit": "unitless"},
                   "variables": {v.name: {"label": v.name.title(), "unit": "items"} for v in model.variables},
                   "states": {state: state.title() for state in model.states},
                   "events": {event: {"label": event.title(), "consequential": True} for event in model.events},
                   "reasons": {name: name.replace("_", " ").title() for name in
                               {model.default, *[rule.reason for rule in model.rules if rule.reason]}}}}


def encode(value):
    return json.dumps(value, sort_keys=True, indent=2) + "\n"


class AppProfileTests(unittest.TestCase):
    def setUp(self):
        self.model = behavior2.parse(SOURCE)
        self.binding = binding_for(self.model)

    def test_binding_size_refuses_before_encoding_and_keeps_byte_boundary(self):
        calls = []

        def observe(frame, event, function):
            if (event == "c_call" and frame.f_code is app_profile._parse_binding.__code__
                    and getattr(function, "__name__", "") == "encode"):
                calls.append(function.__name__)

        previous = sys.getprofile()
        try:
            sys.setprofile(observe)
            with self.assertRaisesRegex(behavior2.Refusal, "exceeds 64 KiB"):
                app_profile._parse_binding(self.model, " " * (app_profile.MAX_BINDING_BYTES + 1))
        finally:
            sys.setprofile(previous)
        self.assertEqual(calls, [], "over-budget text must be refused before UTF-8 allocation")

        binding = copy.deepcopy(self.binding)
        binding["ui"]["title"] = "Réparation"
        text = json.dumps(binding, ensure_ascii=False, sort_keys=True)
        text += " " * (app_profile.MAX_BINDING_BYTES - len(text.encode("utf-8")))
        parsed, _, raw = app_profile._parse_binding(self.model, text)
        self.assertEqual(parsed, binding)
        self.assertEqual(raw, text.encode("utf-8"))
        self.assertEqual(len(raw), app_profile.MAX_BINDING_BYTES)

        # Fewer characters can still exceed the byte budget in UTF-8.
        with self.assertRaisesRegex(behavior2.Refusal, "exceeds 64 KiB"):
            app_profile._parse_binding(self.model, "é" * 32769)
        with self.assertRaisesRegex(behavior2.Refusal, "invalid Unicode"):
            app_profile._parse_binding(self.model, chr(0xd800))

    def test_source_size_refuses_before_parser_encoding_and_keeps_boundary(self):
        calls = []

        def observe(frame, event, function):
            if (event == "c_call" and frame.f_code is behavior2.parse.__code__
                    and getattr(function, "__name__", "") == "encode"):
                calls.append(function.__name__)

        previous = sys.getprofile()
        try:
            sys.setprofile(observe)
            with self.assertRaisesRegex(behavior2.Refusal, "source exceeds 64 KiB"):
                app_profile.lower(" " * (2 * 1024 * 1024), encode(self.binding))
        finally:
            sys.setprofile(previous)
        self.assertEqual(calls, [], "over-budget source must refuse before the parser's UTF-8 allocation")
        source = SOURCE.ljust(behavior2.MAX_SOURCE)
        outputs = app_profile.lower(source, encode(self.binding))
        self.assertEqual(outputs["zal-original.zal"], source)
        self.assertEqual(len(outputs["zal-original.zal"].encode()), behavior2.MAX_SOURCE)
        with self.assertRaisesRegex(behavior2.Refusal, "source exceeds 64 KiB"):
            app_profile.lower(source + " ", encode(self.binding))
        with self.assertRaisesRegex(behavior2.Refusal, "invalid source Unicode"):
            app_profile.lower(chr(0xd800), encode(self.binding))
        with self.assertRaisesRegex(behavior2.Refusal, "Source must be ASCII"):
            app_profile.lower("é" * 32769, encode(self.binding))

    def test_legacy_factory_bytes_are_unchanged(self):
        outputs = factory.lower(self.model)
        self.assertEqual({name: hashlib.sha256(value.encode()).hexdigest() for name, value in outputs.items()}, LEGACY_HASHES)
        self.assertIn("field 164 102 input_quantity 111;", outputs["project.zeno"])
        self.assertIn("variant 120 101 event_request none;", outputs["project.zeno"])

    def test_complete_interpreter_table_and_explicit_app_fields(self):
        outputs = app_profile.lower(SOURCE, encode(self.binding))
        project = outputs["project.zeno"]
        self.assertIn("type 103 data EventAction;", project)
        self.assertIn("field 119 101 action 103;", project)
        self.assertIn("field 164 101 input_quantity 111;", project)
        self.assertIn("field 160 102 observed_authorized 106;", project)
        self.assertIn("command.101.119 == 120", project)
        metadata = json.loads(outputs["ui.json"])
        self.assertEqual(metadata["action"], "command.101.119")
        self.assertEqual(metadata["fields"]["command.101.164"], {"label": "Quantity", "unit": "items"})
        self.assertEqual(metadata["fields"]["context.102.160"], {"label": "Authorized", "unit": "unitless"})
        source_map = json.loads(outputs["zal-source-map.json"])
        self.assertEqual(source_map["input_order"], ["pre.100.110", "pre.100.170", "command.101.119",
                                                      "command.101.164", "context.102.160"])
        self.assertEqual(source_map["bindings"]["authorized"]["role"], "authorization")
        self.assertEqual(source_map["bindings"]["quantity"]["role"], "intent")
        rows = outputs["tests/zal-authority-examples.txt"].splitlines()
        self.assertEqual(len(rows), 48)
        actual = {tuple(int(x) for x in row.split(" | ")[0].split()): row for row in rows}
        self.assertEqual(len(actual), 48)
        for state, units, event, quantity, authorized in itertools.product((150, 151), range(3), (120, 121), (1, 2), (0, 1)):
            frame = (state, units, event, quantity, authorized)
            if state == 150 and event == 120 and authorized:
                result = f"accept - 151 {quantity}"
            elif state == 150 and event == 120:
                result = f"reject 200 {state} {units}"
            elif state == 151 and event == 121 and authorized:
                result = "accept - 150 0"
            else:
                result = f"reject 201 {state} {units}"
            self.assertEqual(actual[frame], " ".join(map(str, frame)) + " | " + result + " | -")

    def test_bad_and_security_crossed_bindings_refuse(self):
        changes = []
        missing = copy.deepcopy(self.binding); missing["observations"].pop(); changes.append(missing)
        duplicate = copy.deepcopy(self.binding); duplicate["observations"][1] = copy.deepcopy(duplicate["observations"][0]); changes.append(duplicate)
        unknown = copy.deepcopy(self.binding); unknown["observations"][0]["name"] = "unknown"; changes.append(unknown)
        kind = copy.deepcopy(self.binding); kind["observations"][0]["kind"] = "input"; changes.append(kind)
        crossed = copy.deepcopy(self.binding); crossed["observations"][0]["source"] = "command"; changes.append(crossed)
        identity = copy.deepcopy(self.binding); identity["observations"][1]["role"] = "identity"; changes.append(identity)
        wrong_type = copy.deepcopy(self.binding); wrong_type["observations"][0]["role"] = []; changes.append(wrong_type)
        stale = copy.deepcopy(self.binding); stale["model_revision"] = "0" * 64; changes.append(stale)
        extra = copy.deepcopy(self.binding); extra["ui"]["events"]["request"]["untrusted"] = True; changes.append(extra)
        missing_label = copy.deepcopy(self.binding); del missing_label["ui"]["reasons"]["denied"]; changes.append(missing_label)
        bad_text = copy.deepcopy(self.binding); bad_text["ui"]["title"] = "hidden\u202etext"; changes.append(bad_text)
        wide_text = copy.deepcopy(self.binding); wide_text["ui"]["title"] = "😀" * 151; changes.append(wide_text)
        not_bool = copy.deepcopy(self.binding); not_bool["ui"]["events"]["request"]["consequential"] = 1; changes.append(not_bool)
        for changed in changes:
            with self.subTest(changed=changed), self.assertRaises(behavior2.Refusal):
                app_profile.lower(SOURCE, encode(changed))
        duplicated_json_key = encode(self.binding).replace('"format":', '"format":"duplicate", "format":', 1)
        with self.assertRaisesRegex(behavior2.Refusal, "duplicate JSON key"):
            app_profile.lower(SOURCE, duplicated_json_key)

    def test_exact_source_binding_and_owned_outputs(self):
        raw = SOURCE.replace("\n", "\r\n")
        encoded = encode(self.binding)
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "source.zal"; source.write_bytes(raw.encode())
            binding = Path(tmp) / "binding.json"; binding.write_bytes(encoded.encode())
            outputs = app_profile.lower_files(source, binding)
        mapping = json.loads(outputs["zal-source-map.json"])
        self.assertEqual(outputs["zal-original.zal"], raw)
        self.assertEqual(outputs["zal-app-binding.json"], encoded)
        self.assertEqual(mapping["source_sha256"], hashlib.sha256(raw.encode()).hexdigest())
        self.assertEqual(mapping["binding_sha256"], hashlib.sha256(encoded.encode()).hexdigest())
        self.assertEqual(mapping["revision"], self.model.revision)
        before = copy.deepcopy(self.binding)
        outputs["project.zeno"] = "changed"
        self.assertEqual(self.binding, before)
        self.assertNotEqual(app_profile.lower(SOURCE, encoded)["project.zeno"], "changed")

    def test_cli_emits_exact_files_and_refuses_invalid_or_existing_output(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source, binding = root / "source.zal", root / "binding.json"
            source.write_text(SOURCE)
            binding.write_text(encode(self.binding))
            command = [sys.executable, "-B", str(Path(app_profile.__file__).resolve()),
                       "--source", str(source), "--binding", str(binding), "--out", str(root / "contract")]
            result = subprocess.run(command, capture_output=True, text=True, timeout=10, check=False)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout)["status"], "lowered")
            expected = app_profile.lower(SOURCE, encode(self.binding))
            for name, contents in expected.items():
                self.assertEqual((root / "contract" / name).read_text(), contents)
            refused = subprocess.run(command, capture_output=True, text=True, timeout=10, check=False)
            self.assertEqual(refused.returncode, 2)
            self.assertEqual(json.loads(refused.stderr)["status"], "refused")
            self.assertEqual((root / "contract/project.zeno").read_text(), expected["project.zeno"])
            malformed = dict(self.binding, model_revision="0" * 64)
            binding.write_text(encode(malformed))
            command[-1] = str(root / "invalid")
            refused = subprocess.run(command, capture_output=True, text=True, timeout=10, check=False)
            self.assertEqual(refused.returncode, 2)
            self.assertFalse((root / "invalid").exists())

    def test_cli_preserves_literal_utf8_and_crlf_and_refuses_invalid_bytes(self):
        source_text = SOURCE.replace("\n", "\r\n")
        binding = copy.deepcopy(self.binding)
        binding["ui"]["title"] = "Réparation — 雪"
        binding_text = (json.dumps(binding, ensure_ascii=False, sort_keys=True, indent=2) + "\n").replace("\n", "\r\n")
        expected = app_profile.lower(source_text, binding_text)
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source, binding_path = root / "source.zal", root / "binding.json"
            source.write_bytes(source_text.encode("utf-8"))
            binding_path.write_bytes(binding_text.encode("utf-8"))
            command = [sys.executable, "-B", str(Path(app_profile.__file__).resolve()),
                       "--source", str(source), "--binding", str(binding_path), "--out", str(root / "contract")]
            result = subprocess.run(command, capture_output=True, text=True, timeout=10, check=False)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout)["status"], "lowered")
            for name, contents in expected.items():
                self.assertEqual((root / "contract" / name).read_bytes(), contents.encode("utf-8"))
            mapping = json.loads((root / "contract/zal-source-map.json").read_bytes())
            for key, name in [("source_sha256", "zal-original.zal"), ("binding_sha256", "zal-app-binding.json"),
                              ("project_sha256", "project.zeno"), ("policy_sha256", "v2/policy.json"),
                              ("ui_sha256", "ui.json"), ("reference_sha256", "tests/zal-authority-examples.txt")]:
                self.assertEqual(mapping[key], hashlib.sha256((root / "contract" / name).read_bytes()).hexdigest())
            for invalid in (binding_path, source):
                with self.subTest(invalid=invalid.name):
                    original = invalid.read_bytes()
                    invalid.write_bytes(b"\xff")
                    command[-1] = str(root / ("invalid-" + invalid.name))
                    refused = subprocess.run(command, capture_output=True, text=True, timeout=10, check=False)
                    self.assertEqual(refused.returncode, 2, refused.stderr)
                    self.assertEqual(json.loads(refused.stderr)["status"], "refused")
                    self.assertFalse(Path(command[-1]).exists())
                    invalid.write_bytes(original)

    def test_writer_preflights_all_encodings_before_creating_any_output(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for exact_utf8, invalid in [(False, "é"), (True, chr(0xd800))]:
                for exists in (False, True):
                    with self.subTest(exact_utf8=exact_utf8, exists=exists):
                        destination = root / f"contract-{exact_utf8}-{exists}"
                        if exists:
                            destination.mkdir()
                        with self.assertRaises(UnicodeEncodeError):
                            factory.write_new({"first.txt": "valid\n", "nested/last.txt": invalid},
                                              destination, exact_utf8=exact_utf8)
                        self.assertEqual(destination.exists(), exists)
                        if exists:
                            self.assertEqual(list(destination.iterdir()), [])

    def test_writer_default_keeps_platform_newlines_and_utf8_mode_keeps_exact_bytes(self):
        contents = "first\nsecond\r\n"
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            factory.write_new({"text.txt": contents}, root / "default")
            self.assertEqual((root / "default/text.txt").read_bytes(), contents.replace("\n", os.linesep).encode("ascii"))

            def windows_text(path, text, *, encoding):
                with path.open("w", encoding=encoding, newline="\r\n") as handle:
                    return handle.write(text)

            with patch.object(Path, "write_text", windows_text):
                factory.write_new({"text.txt": contents}, root / "windows-default")
                factory.write_new({"text.txt": "Réparation\r\n"}, root / "windows-utf8", exact_utf8=True)
            self.assertEqual((root / "windows-default/text.txt").read_bytes(), contents.replace("\n", "\r\n").encode("ascii"))
            self.assertEqual((root / "windows-utf8/text.txt").read_bytes(), "Réparation\r\n".encode())

    def test_maximum_declarations_keep_disjoint_fields_and_abi_order(self):
        model = behavior2.parse(MAX_SHAPE)
        origins = {"a": "command", "b": "server-context", "c": "command", "d": "server-context",
                   "left": "server-context", "right": "command"}
        outputs = app_profile.lower(MAX_SHAPE, encode(binding_for(model, origins)))
        project = outputs["project.zeno"]
        for line in ("type 103 data EventAction;", "type 110 int Variable3 in 0..=0;",
                     "type 111 int Input0 in 0..=0;", "type 112 int Input1 in 0..=0;",
                     "field 119 101 action 103;", "field 160 101 observed_a 106;",
                     "field 161 102 observed_b 106;", "field 162 101 observed_c 106;",
                     "field 163 102 observed_d 106;", "field 164 102 input_left 111;",
                     "field 165 101 input_right 112;"):
            self.assertIn(line, project)
        mapping = json.loads(outputs["zal-source-map.json"])
        self.assertEqual(mapping["input_order"], ["pre.100.110", "pre.100.170", "pre.100.171", "pre.100.172", "pre.100.173",
                                                  "command.101.119", "command.101.160", "command.101.162", "command.101.165",
                                                  "context.102.161", "context.102.163", "context.102.164"])
        self.assertEqual(len(outputs["tests/zal-authority-examples.txt"].splitlines()), 64)

    def test_all_command_inputs_get_a_distinct_unit_context(self):
        model = behavior2.parse(MAX_SHAPE)
        origins = {name: "command" for name in (*model.facts, *[item.name for item in model.inputs])}
        outputs = app_profile.lower(MAX_SHAPE, encode(binding_for(model, origins)))
        self.assertIn("type 104 int UnitContext in 0..=0;", outputs["project.zeno"])
        self.assertIn("field 159 102 unit 104;", outputs["project.zeno"])
        source_map = json.loads(outputs["zal-source-map.json"])
        self.assertEqual(source_map["unit_context_field"], 159)
        self.assertEqual(source_map["input_order"][-1], "context.102.159")
        self.assertEqual(json.loads(outputs["ui.json"])["fields"]["context.102.159"]["unit"], "unitless")


if __name__ == "__main__":
    unittest.main(verbosity=2)
