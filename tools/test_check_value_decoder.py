#!/usr/bin/env python3
"""Gate refusal and evidence-retention tests; synthetic reports are not proofs."""
import copy
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import check_value_decoder as gate
import check_verus as verifier


class DecoderGateTests(unittest.TestCase):
    def setUp(self):
        self.pin = json.loads((gate.ROOT / verifier.PIN).read_text())
        self.profile = {"expected_verified": 7,
                        "target_functions": ["value_decoder::codec::decode_value"]}
        self.report = {
            "verus": {"version": self.pin["version"], "commit": self.pin["commit"],
                      "toolchain": self.pin["rust_toolchain"]},
            "verification-results": {"success": True, "errors": 0, "verified": 7,
                "encountered-error": False, "encountered-vir-error": False,
                "is-verifying-entire-crate": True},
            "func-details": dict.fromkeys(self.profile["target_functions"], {}),
        }

    def accepts(self, report=None, code=0, stderr=""):
        report = self.report if report is None else report
        proc = subprocess.CompletedProcess([], code, json.dumps(report), stderr)
        return gate.accepted(proc, report, self.pin, self.profile)

    def test_exact_success_is_required(self):
        self.assertTrue(self.accepts())
        for key, value in [("success", False), ("errors", 1), ("errors", False),
                           ("verified", 6), ("verified", 8), ("verified", True),
                           ("encountered-error", True), ("encountered-vir-error", True),
                           ("is-verifying-entire-crate", False)]:
            report = copy.deepcopy(self.report)
            report["verification-results"][key] = value
            self.assertFalse(self.accepts(report), (key, value))
        for key in ("version", "commit", "toolchain"):
            report = copy.deepcopy(self.report)
            report["verus"][key] = "wrong"
            self.assertFalse(self.accepts(report), key)
        report = copy.deepcopy(self.report)
        report["func-details"] = {}
        self.assertFalse(self.accepts(report))
        report = copy.deepcopy(self.report)
        report["func-details"]["value_decoder::unexpected"] = {}
        self.assertFalse(self.accepts(report))
        for code in (1, 2, -9, 137, None):
            self.assertFalse(self.accepts(code=code))
        for stderr in ("error: failed", "resource limit exceeded", "solver returned unknown",
                       "timed out", "internal compiler error", "panic"):
            self.assertFalse(self.accepts(stderr=stderr))

    def test_unqualified_or_boolean_expected_count_is_refused(self):
        for count in (0, -1, True, "7"):
            self.profile["expected_verified"] = count
            self.assertFalse(self.accepts())

    def test_malformed_report_is_refused(self):
        for value in (None, [], {}, {"verification-results": []}, {"verus": 0}):
            self.assertFalse(self.accepts(value if value is not None else []))
        for text in ("", "not json", "[]", "null"):
            proc = subprocess.CompletedProcess([], 1, text, "")
            self.assertEqual(gate.parse_report(proc), {})

    def test_nine_unique_mutations_preserve_original_source(self):
        original = (gate.ROOT / gate.SUBJECT).read_bytes()
        source = original.decode()
        mutants = gate.mutation_sources(source)
        self.assertEqual(set(mutants), set(gate.SEMANTIC_TARGETS) | set(gate.NATIVE_TARGETS))
        self.assertEqual(len(mutants), 9)
        for name, changed in mutants.items():
            self.assertNotEqual(source, changed, name)
            if name in gate.SEMANTIC_TARGETS:
                start, end = verifier.function_lines(changed, gate.SEMANTIC_TARGETS[name])
                self.assertGreaterEqual(start, 1)
                self.assertGreater(end, start)
        self.assertEqual((gate.ROOT / gate.SUBJECT).read_bytes(), original)
        for source in ("no anchor", "same same"):
            with self.assertRaises(ValueError):
                gate.replace_once(source, "same", "different")

    def classify(self, name, *, code=1, extra="", unrelated=False, report=None):
        source = gate.mutation_sources((gate.ROOT / gate.SUBJECT).read_text())[name]
        region = verifier.function_lines(source, gate.SEMANTIC_TARGETS[name])
        specimen = Path("/specimen")
        path = specimen / gate.SUBJECT
        if unrelated:
            path = Path("/unrelated.rs")
        stderr = f"error: postcondition not satisfied\n --> {path}:{region[0]}:1\n"
        stderr += "error: aborting due to 1 previous error\n" + extra
        if report is None:
            report = copy.deepcopy(self.report)
            report["verification-results"].update({"success": False, "errors": 1,
                                                   "encountered-error": True})
        proc = subprocess.CompletedProcess([], code, json.dumps(report), stderr)
        return gate.classify_mutation(proc, report, name, source, specimen, self.pin, self.profile)

    def test_controls_require_named_semantic_obligation(self):
        for name in gate.SEMANTIC_TARGETS:
            self.assertTrue(self.classify(name), name)
            self.assertFalse(self.classify(name, unrelated=True), name)
            for code in (0, 2, -9, 137, None):
                self.assertFalse(self.classify(name, code=code), (name, code))
            for text in ("SMT result UNKNOWN", "resource limit exceeded", "rlimit exceeded",
                         "timed out", "out of memory", "panic", "transport error",
                         "error[E0308]: mismatched types\n"):
                self.assertFalse(self.classify(name, extra=text), (name, text))
            self.assertFalse(self.classify(name, report=self.report), name)

    def test_unreviewed_application_or_standard_trust_is_refused(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for path in gate.UNIT_SOURCES:
                target = root / path
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes((gate.ROOT / path).read_bytes())
            profile = {"std_contracts_sha256": verifier.digest(root / gate.STD_CONTRACTS)}
            gate.audit_trust(root, profile)
            subject = root / gate.SUBJECT
            original = subject.read_text()
            for escape in ("assume(false);", "admit();", "#[verifier::external_body]",
                           "#[verifier::external]", "pub assume_specification[f]();",
                           "#[verifier::exec_allows_no_decreases_clause]"):
                subject.write_text(original + "\n" + escape)
                with self.assertRaisesRegex(RuntimeError, "forbidden trust escape"):
                    gate.audit_trust(root, profile)
            subject.write_text(original)
            contracts = root / gate.STD_CONTRACTS
            contracts.write_text(contracts.read_text() + "\npub assume_specification[other]();\n")
            profile["std_contracts_sha256"] = verifier.digest(contracts)
            with self.assertRaisesRegex(RuntimeError, "standard-library contracts"):
                gate.audit_trust(root, profile)

    def test_timeout_retains_partial_streams_and_command(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            error = subprocess.TimeoutExpired(["verus"], 1, output=b"partial", stderr=b"diagnostic")
            with patch.object(verifier, "run", side_effect=error):
                result = gate.retained_run(["verus"], directory, {}, timeout=1)
            self.assertIsNone(result.returncode)
            self.assertEqual((directory / "stdout.log").read_text(), "partial")
            self.assertEqual((directory / "stderr.log").read_text(), "diagnostic")
            record = json.loads((directory / "command.json").read_text())
            self.assertTrue(record["timed_out"])
            self.assertEqual(record["command"], ["verus"])
            self.assertIsNone(record["exit_code"])


class NativeBindingTests(unittest.TestCase):
    def audit(self, source, names=("reader",), extra_sources=None):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            sources = {gate.SUBJECT: source, **(extra_sources or {})}
            for path, text in sources.items():
                target = root / path
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(text)
            report = {"func-details": dict.fromkeys("value_decoder::codec::" + name for name in names)}
            with patch.object(gate, "UNIT_SOURCES", tuple(sources)):
                return gate.audit_native_binding(root, report)

    def refused(self, source, line, names=("reader",), function="reader", **kwargs):
        with self.assertRaises(gate.NativeBindingRefused) as caught:
            self.audit(source, names, **kwargs)
        error = caught.exception
        self.assertEqual(error.path, gate.SUBJECT)
        self.assertEqual(error.line, line)
        self.assertEqual(error.function, "value_decoder::codec::" + function)
        self.assertIn(f"{gate.SUBJECT}:{line}:", str(error))

    def test_native_cfg_inside_nested_body_refuses_exact_line(self):
        self.refused("fn reader() {\n    if true {\n        #[cfg(not(verus_keep_ghost))]\n"
                     "        { return; }\n    }\n}\nfn outside() {}", 3)

    def test_native_cfg_outside_body_passes(self):
        source = ("#[cfg(not(verus_keep_ghost))]\nfn outside() { return; }\n"
                  "fn reader() { if true { } }\n#[cfg(not(verus_keep_ghost))]\nfn after() {}")
        bindings = self.audit(source)
        self.assertEqual([(b["start_line"], b["end_line"]) for b in bindings], [(3, 3)])

    def test_native_attribute_on_verified_function_refuses(self):
        self.refused("#[cfg(not(verus_keep_ghost))]\nfn reader() {}", 1)
        self.refused("#[cfg_attr(not(verus_keep_ghost), inline)]\nfn reader() {}", 1)

    def test_cfg_algebra_and_other_native_configurations_refuse(self):
        for predicate in ("not(verus_keep_ghost)", "any(not(verus_keep_ghost), feature = \"std\")",
                          "all(not(verus_keep_ghost), target_os = \"linux\")", "feature = \"std\"",
                          "not(all(verus_keep_ghost, unix))", "all()", "unix"):
            with self.subTest(predicate=predicate):
                self.refused(f"fn reader() {{\n#[cfg({predicate})]\nreturn;\n}}", 2)
        for predicate in ("verus_keep_ghost", "all(verus_keep_ghost, unix)",
                          "not(not(verus_keep_ghost))", "any()"):
            with self.subTest(predicate=predicate):
                self.audit(f"fn reader() {{ #[cfg({predicate})] proof! {{ }} }}")

    def test_cfg_attr_and_cfg_expression_refuse(self):
        for syntax in ("#[cfg_attr(not(verus_keep_ghost), allow(unused))] let x = 1;",
                       "#[cfg_attr(verus_keep_ghost, cfg(any()))] return;",
                       "#[cfg_attr(verus_keep_ghost, cfg_attr(all(), cfg(any())))] return;",
                       "#![cfg(not(verus_keep_ghost))]",
                       "let x = cfg!(verus_keep_ghost);", "let x = cfg!(feature = \"std\");"):
            with self.subTest(syntax=syntax):
                self.refused("fn reader() {\n" + syntax + "\n}", 2)

    def test_comments_literals_proof_annotations_and_body_end(self):
        source = '''#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result,))]
fn reader<'a>(value: &'a str) -> bool {
    // } #[cfg(not(verus_keep_ghost))] fn decoy() {
    /* nested /* } */ #[cfg(not(verus_keep_ghost))] */
    let s = r###"} fn decoy() { #[cfg(not(verus_keep_ghost))]"###;
    let c = '}';
    let s = "\\\" } #[cfg(not(verus_keep_ghost))]";
    #[cfg(verus_keep_ghost)] proof! { assert(true); }
    true
}
#[cfg(not(verus_keep_ghost))]
fn outside() {}
'''
        bindings = self.audit(source)
        self.assertEqual(bindings[0]["start_line"], 2)
        self.assertEqual(bindings[0]["end_line"], 10)

    def test_native_alternative_method_and_inherited_cfg_refuse(self):
        source = ("struct Cursor;\nimpl Cursor { fn reader() {} }\n"
                  "#[cfg(not(verus_keep_ghost))]\nimpl Cursor { fn reader() {} }")
        self.refused(source, 3, names=("Cursor::reader",), function="Cursor::reader")
        self.refused("#[cfg(not(verus_keep_ghost))]\nmod nested { fn reader() {} }",
                     1, names=("nested::reader",), function="nested::reader")

    def test_external_module_attribute_is_inherited(self):
        path = Path("crates/zeno-fcis-codec/src/decoder_bytes.rs")
        self.refused("#[cfg(not(verus_keep_ghost))]\nmod decoder_bytes;", 1,
                     names=("decoder_bytes::reader",), function="decoder_bytes::reader",
                     extra_sources={path: "fn reader() {}"})
        self.refused('#[cfg_attr(verus_keep_ghost, path = "other.rs")]\nmod decoder_bytes;', 1,
                     names=("decoder_bytes::reader",), function="decoder_bytes::reader",
                     extra_sources={path: "fn reader() {}"})

    def test_verus_contract_braces_do_not_truncate_body(self):
        source = ("proof fn reader()\n    ensures if true { 1 } else { 2 } == 1,\n"
                  "        forall|x: int| x > 0 ==> { x + 1 > 0 },\n{\n"
                  "    #[cfg(not(verus_keep_ghost))]\n    return;\n}\nfn outside() {}")
        self.refused(source, 5)

    def test_native_alternative_types_refuse_but_derives_pass(self):
        self.audit("#[cfg_attr(not(verus_keep_ghost), derive(Clone, Debug))]\n"
                   "struct Cursor;\nimpl Cursor { fn reader() {} }", ("Cursor::reader",))
        self.refused("#[cfg(verus_keep_ghost)]\nstruct Cursor;\n"
                     "#[cfg(not(verus_keep_ghost))]\nstruct Cursor;\nfn reader() {}", 3,
                     function="Cursor")
        self.refused("struct Cursor;\n#[cfg_attr(verus_keep_ghost, cfg(any()))]\n"
                     "struct Cursor;\nfn reader() {}", 2, function="Cursor")
        self.refused("#[cfg(verus_keep_ghost)]\ntype Cursor = u8;\n"
                     "#[cfg(not(verus_keep_ghost))]\ntype Cursor = fn() -> u8;\nfn reader() {}",
                     3, function="Cursor")

    def test_function_pointer_types_do_not_change_declaration_kind(self):
        items = gate.source_items("type Callback = fn(u8) -> u8;\n"
                                  "const CALLBACK: fn(u8) -> u8 = reader;\n"
                                  "const fn reader(callback: fn(u8) -> u8) -> u8 { 1 }", "root")
        self.assertEqual([(item.name, item.kind) for item in items],
                         [("root::Callback", "type"), ("root::CALLBACK", "const"), ("root::reader", "fn")])

    def test_native_import_alternatives_refuse(self):
        self.refused("fn reader() {}\n#[cfg(not(verus_keep_ghost))]\n"
                     "use other::replacement as reader;", 2)
        self.refused("struct Cursor;\nfn reader() {}\n#[cfg(not(verus_keep_ghost))]\n"
                     "use other::{Replacement as Cursor};", 3, function="Cursor")
        self.audit("fn reader() {}\n#[cfg(not(verus_keep_ghost))]\n"
                   "pub use other::{Unrelated, Another};")

    def test_generated_clone_constant_and_projection_map_to_declarations(self):
        bindings = self.audit("#[cfg_attr(verus_keep_ghost, derive(Clone))]\nstruct Cursor;\n"
                              "impl Cursor { const LIMIT: u32 = 4; }\nenum Model { Unit }",
                              ("Cursor::clone", "impl&%0::LIMIT", "Model::arrow_0"))
        self.assertEqual({b["kind"] for b in bindings}, {"struct", "const", "enum"})

    def test_missing_mapping_and_bad_delimiters_fail_closed(self):
        for source in ("fn other() {}", "fn reader() {", "/* unterminated"):
            with self.subTest(source=source), self.assertRaises((ValueError, RuntimeError)):
                self.audit(source)

    def test_real_production_inventory_and_three_native_controls(self):
        profile = json.loads((gate.ROOT / gate.PROFILE).read_text())
        report = {"func-details": dict.fromkeys(profile["target_functions"])}
        normal = gate.audit_native_binding(gate.ROOT, report)
        self.assertGreater(len(normal), 200)
        original = (gate.ROOT / gate.SUBJECT).read_text()
        for name, function in gate.NATIVE_TARGETS.items():
            changed = gate.mutation_sources(original)[name]
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                for path in gate.UNIT_SOURCES:
                    target = root / path
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_text(changed if path == gate.SUBJECT else (gate.ROOT / path).read_text())
                with self.assertRaises(gate.NativeBindingRefused) as caught:
                    gate.audit_native_binding(root, report)
                self.assertEqual(caught.exception.function, function)
                self.assertIn("cfg", changed.splitlines()[caught.exception.line - 1])
        self.assertEqual((gate.ROOT / gate.SUBJECT).read_text(), original)


if __name__ == "__main__":
    unittest.main()
