"""Proof gate controls: textual refusal AND independent kernel-module rejection."""

import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest

import check_lean_proofs as check


class ProofReports(unittest.TestCase):
    expected = {"N.example_proof": []}
    good = "'N.example_proof' does not depend on any axioms"

    def test_accepts_permitted_axioms_and_axiom_free_reports(self):
        check.check_output(self.good, self.expected)
        check.check_output("'N.example_proof' depends on axioms: [propext, Quot.sound, Classical.choice]",
                           {"N.example_proof": sorted(check.ALLOWED)})

    def test_refuses_missing_duplicate_or_wrong_theorem_reports(self):
        for report in ("", self.good + "\n" + self.good, self.good.replace("N.", "Other.")):
            with self.subTest(report=report), self.assertRaises(ValueError):
                check.check_output(report, self.expected)

    def test_refuses_unexpected_axioms_diagnostics_and_manifest_drift(self):
        for report in (
            "'N.example_proof' depends on axioms: [sorryAx]",
            "'N.example_proof' depends on axioms: [unprovedClaim]",
            "'N.example_proof' depends on axioms: [propext]",
            "file:1:1: error: failed", "file:1:1: warning: declaration uses a placeholder",
        ):
            with self.subTest(report=report), self.assertRaises(ValueError):
                check.check_output(report, self.expected)

    def test_refuses_each_textual_escape(self):
        for token in ("sorry", "admit", "axiom", "native_decide", "implemented_by",
                      "extern", "unsafe", "@[csimp]", "attribute [csimp]"):
            with self.subTest(token=token), self.assertRaises(ValueError):
                check.check_source(token)


class ModuleCoverage(unittest.TestCase):
    """Each negative fixture compiles, then fails on its real axiom dependencies.

    The ordinary gate first refuses its text. The independent compile+audit
    calls below deliberately exercise the semantic layer without that guard.
    A normal variant of each declaration form must pass the complete gate.
    """
    prefix = "import Std\nnamespace GateGap\ntheorem covered : True := by trivial\n#print axioms covered\n"
    named = {"GateGap.covered": []}

    def run_control(self, name, declaration, *, forbidden=True):
        for version in check.VERSIONS:
            with self.subTest(version=version), tempfile.TemporaryDirectory(prefix="lean-gate-test-") as tmp:
                root = Path(tmp)
                build = root / "build"
                build.mkdir()
                path = root / "Unreported.lean"
                normal = self.prefix + declaration.replace("native_decide", "decide") + "\nend GateGap\n"
                path.write_text(normal)
                with contextlib.redirect_stdout(io.StringIO()):
                    packet, seen = check.check_module(version, path, root, build, self.named)
                self.assertEqual(seen, set(self.named))
                self.assertTrue(packet["accepted"])
                self.assertGreaterEqual(packet["constant_count"], 2)
                matched = [r for r in packet["constants"] if r["name"].endswith(".unreported")]
                self.assertEqual(len(matched), 1, packet)
                if not forbidden:
                    self.assertTrue(matched[0]["name"].startswith("_private."))
                    print("GATE_CONTROL " + json.dumps({"name": name, "version": version,
                        "normal": 0, "candidate": 0, "counted": matched[0]["name"],
                        "constant_count": packet["constant_count"]}), flush=True)
                    continue
                mutant = self.prefix + declaration + "\nend GateGap\n"
                path.write_text(mutant)
                with self.assertRaisesRegex(ValueError, "forbidden proof-source token: native_decide"):
                    check.check_module(version, path, root, build, self.named)
                # A fresh .olean from the mutant, not the preceding normal build.
                module, _ = check.compile_module(version, path, root, build)
                code, rejected = check.audit_module(version, module, build)
                self.assertEqual(code, 1, rejected)
                self.assertFalse(rejected["accepted"])
                with self.assertRaisesRegex(ValueError, "module axiom audit rejected"):
                    check.require_audit(code, rejected)
                target = next(r for r in rejected["constants"] if r["name"].endswith(".unreported"))
                if version == "4.28.0":
                    self.assertIn("Lean.ofReduceBool", target["forbidden"])
                    self.assertIn("Lean.trustCompiler", target["forbidden"])
                else:
                    self.assertTrue(any("._native.native_decide.ax_" in n
                                        for n in target["forbidden"]), target)
                    self.assertTrue(any(r["kind"] == "axiom" and r["name"] in target["forbidden"]
                                        for r in rejected["constants"]), rejected)
                print("GATE_CONTROL " + json.dumps({"name": name, "version": version,
                    "normal": 0, "mutant_compile": 0, "mutant_audit": code,
                    "text_refused": True, "witness": target,
                    "constant_count": rejected["constant_count"]}), flush=True)

    def test_unreported_private_theorem_fails(self):
        self.run_control("unreported_private_theorem",
                         "private theorem unreported : (1 : Nat) + 1 = 2 := by native_decide")

    def test_protected_theorem_fails(self):
        self.run_control("protected_theorem",
                         "protected theorem unreported : (1 : Nat) + 1 = 2 := by native_decide")

    def test_indented_namespace_theorem_fails(self):
        self.run_control("indented_namespace_theorem",
                         "  theorem unreported : (1 : Nat) + 1 = 2 := by native_decide")

    def test_definition_fails(self):
        self.run_control("definition",
                         "def unreported : {n : Nat // 1 + 1 = n} := ⟨2, by native_decide⟩")

    def test_instance_fails(self):
        self.run_control("instance",
                         "class Witness : Prop where\n  holds : (1 : Nat) + 1 = 2\n"
                         "instance unreported : Witness := ⟨by native_decide⟩")

    def test_allowed_private_theorem_passes_and_is_counted(self):
        self.run_control("allowed_private_theorem",
                         "private theorem unreported : (1 : Nat) + 1 = 2 := by decide", forbidden=False)


if __name__ == "__main__":
    unittest.main()
