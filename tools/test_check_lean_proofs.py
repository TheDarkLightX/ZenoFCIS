"""Proof gate controls: textual refusal AND independent kernel-module rejection."""

import contextlib
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

import check_lean_proofs as check


class ProofReports(unittest.TestCase):
    expected = {"N.example_proof": []}
    good = "'N.example_proof' does not depend on any axioms"

    def test_accepts_permitted_axioms_and_axiom_free_reports(self):
        self.assertEqual(check.ALLOWED, frozenset({"propext", "Quot.sound"}))
        check.check_output(self.good, self.expected)
        check.check_output("'N.example_proof' depends on axioms: [propext, Quot.sound]",
                           {"N.example_proof": ["propext", "Quot.sound"]})

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
        for token in ("sorry", "admit", "axiom", "opaque", "native_decide", "implemented_by",
                      "extern", "unsafe", "@[csimp]", "attribute [csimp]",
                      "set_option debug.skipKernelTC true in", "run_cmd pure ()", "run_elab pure ()",
                      "run_meta pure ()", "addDecl", "#eval 1"):
            with self.subTest(token=token), self.assertRaises(ValueError):
                check.check_source(token)

    def test_refuses_imports_other_than_std_and_proof_modules(self):
        check.check_source("import Std\nimport FixedPoints\n", frozenset({"FixedPoints"}))
        for source in ("import Lean", "import Lean.Elab.Command", "import all Lean", "import FixedPoints"):
            with self.subTest(source=source), self.assertRaisesRegex(ValueError, "forbidden proof-source import"):
                check.check_source(source)

    def test_constructive_reports_refuse_classical_choice(self):
        with self.assertRaisesRegex(ValueError, "unexpected axioms"):
            check.check_output("'N.example_proof' depends on axioms: [Classical.choice]",
                               {"N.example_proof": ["Classical.choice"]}, check.CONSTRUCTIVE)


class ProofTree(unittest.TestCase):
    def test_success_text_describes_kernel_replay_and_axiom_audit(self):
        with tempfile.TemporaryDirectory(prefix="lean-tree-test-") as tmp:
            proofs = Path(tmp)
            (proofs / "Good.lean").write_text("import Std\ntheorem checked : True := by trivial\n")
            (proofs / "AXIOMS.json").write_text("{}\n")
            output = io.StringIO()
            with mock.patch.object(check, "PROOFS", proofs), \
                    mock.patch.object(sys, "argv", ["check_lean_proofs.py"]), \
                    contextlib.redirect_stdout(output):
                check.main()
            success = output.getvalue().splitlines()[-1]
            self.assertTrue("kernel-replayed" in success and "axioms audited" in success,
                            "L1_PASS_must_say_kernel_replayed_and_axioms_audited")

    def test_symlinked_directory_is_refused(self):
        with tempfile.TemporaryDirectory(prefix="lean-tree-test-") as tmp:
            root = Path(tmp)
            proofs, outside = root / "proofs", root / "outside"
            proofs.mkdir()
            outside.mkdir()
            (proofs / "Good.lean").write_text("import Std\ntheorem checked : True := by trivial\n")
            (proofs / "AXIOMS.json").write_text("{}\n")
            bad = outside / "Bad.lean"
            bad.write_text("import Std\naxiom injected : False\ntheorem false_proved : False := injected\n")
            # Establish that the skipped source compiles and replays, but its
            # extra axiom makes it ineligible for the proof gate.
            for version in check.VERSIONS:
                build = root / ("build-" + version)
                build.mkdir()
                module, _ = check.compile_module(version, bad, outside, build)
                check.replay_module(version, module, build)
                code, packet = check.audit_module(version, module, build)
                self.assertEqual(code, 1, packet)
                self.assertFalse(packet["accepted"])
                self.assertTrue(any("injected" in r["forbidden"] for r in packet["constants"]))
            with mock.patch.object(check, "PROOFS", proofs), \
                    mock.patch.object(sys, "argv", ["check_lean_proofs.py"]), \
                    contextlib.redirect_stdout(io.StringIO()):
                check.main()  # The same regular tree is eligible.
                (proofs / "linked").symlink_to(outside, target_is_directory=True)
                with self.assertRaisesRegex(ValueError, "symlink in proof tree",
                                            msg="symlinked_directory_must_not_escape_audit"):
                    check.main()

    def test_refuses_file_broken_non_source_and_root_symlinks(self):
        with tempfile.TemporaryDirectory(prefix="lean-tree-test-") as tmp:
            root = Path(tmp)
            proofs = root / "proofs"
            proofs.mkdir()
            target = root / "target"
            target.write_text("data")
            for name, destination in (("Alias.lean", target), ("broken", root / "missing"),
                                      ("data-link", target)):
                with self.subTest(name=name):
                    link = proofs / name
                    link.symlink_to(destination)
                    with self.assertRaisesRegex(ValueError, "symlink in proof tree"):
                        check.proof_files(proofs)
                    link.unlink()
            alias = root / "alias"
            alias.symlink_to(proofs, target_is_directory=True)
            with self.assertRaisesRegex(ValueError, "symlink in proof tree"):
                check.proof_files(alias)


class ModuleCoverage(unittest.TestCase):
    """Each negative fixture compiles, then fails on its real axiom dependencies.

    The ordinary gate first refuses its text. The independent compile+audit
    calls below deliberately exercise the semantic layer without that guard.
    A normal variant of each declaration form must pass the complete gate.
    """
    prefix = "import Std\nnamespace GateGap\ntheorem covered : True := by trivial\n#print axioms covered\n"
    named = {"GateGap.covered": []}

    def test_classical_choice_is_refused(self):
        normal = "import Std\ntheorem selected {α : Type} (h : Nonempty α) : Nonempty α := h\n"
        candidate = normal.replace(":= h", ":= ⟨Classical.choice h⟩")
        for version in check.VERSIONS:
            with self.subTest(version=version), tempfile.TemporaryDirectory(prefix="lean-choice-test-") as tmp:
                root = Path(tmp)
                build = root / "build"
                build.mkdir()
                path = root / "Choice.lean"
                path.write_text(normal)
                with contextlib.redirect_stdout(io.StringIO()):
                    packet, _ = check.check_module(version, path, root, build, {})
                self.assertTrue(packet["accepted"])
                path.write_text(candidate)
                check.check_source(candidate)  # The semantic rule must decide.
                module, _ = check.compile_module(version, path, root, build)
                check.replay_module(version, module, build)
                code, rejected = check.audit_module(version, module, build)
                self.assertEqual(code, 1, "lean_axiom_ceiling_must_reject_classical_choice")
                self.assertFalse(rejected["accepted"])
                target = next(r for r in rejected["constants"] if r["name"] == "selected")
                self.assertIn("Classical.choice", target["axioms"])
                self.assertIn("Classical.choice", target["forbidden"])
                # Model an auditor claiming success despite the real dependency.
                # The Python acceptance rule independently retains the ceiling.
                claimed = dict(rejected, accepted=True, rejected_count=0,
                               constants=[dict(r, forbidden=[]) for r in rejected["constants"]])
                with self.assertRaisesRegex(ValueError, "forbidden declaration",
                                            msg="python_axiom_ceiling_must_reject_classical_choice"):
                    check.require_audit(0, claimed)
                with self.assertRaisesRegex(ValueError, "unexpected axioms",
                                            msg="named_report_must_reject_classical_choice"):
                    check.check_output("'selected' depends on axioms: [Classical.choice]",
                                       {"selected": ["Classical.choice"]})
                with contextlib.redirect_stdout(io.StringIO()), \
                        self.assertRaisesRegex(ValueError, "module axiom audit rejected"):
                    check.check_module(version, path, root, build, {})
                print("GATE_CONTROL " + json.dumps({"name": "classical_choice", "version": version,
                    "normal": 0, "mutant_compile": 0, "mutant_replay": 0, "mutant_audit": code,
                    "python_acceptance_refused": True, "witness": target}), flush=True)

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

    def test_classical_dependency_refused_in_new_module(self):
        for version in check.VERSIONS:
            with self.subTest(version=version), tempfile.TemporaryDirectory(prefix="lean-classical-control-") as tmp:
                root = Path(tmp)
                build = root / "build"
                build.mkdir()
                path = root / "NewCollectionsProof.lean"
                path.write_text("import Std\ntheorem unreported (p : Prop) : p ∨ ¬p := Classical.em p\n")
                module, _ = check.compile_module(version, path, root, build)
                code, packet = check.audit_module(version, module, build)
                self.assertEqual(code, 1, packet)
                self.assertFalse(packet["accepted"])
                target = next(r for r in packet["constants"] if r["name"] == "unreported")
                self.assertIn("Classical.choice", target["forbidden"])
                with self.assertRaisesRegex(ValueError, "module axiom audit rejected"):
                    check.require_audit(code, packet)


class KernelReplay(unittest.TestCase):
    """A declaration added with the kernel check switched off compiles, and the
    axiom audit sees no axioms in it; only the kernel replay rejects it."""
    bypass = """import Lean
open Lean

set_option debug.skipKernelTC true in
run_cmd Lean.Elab.Command.liftCoreM <| addDecl <| .thmDecl {
  name := `Bypass.unchecked, levelParams := [], type := mkConst ``False, value := mkConst ``True.intro }

theorem Bypass.false_proved : False := Bypass.unchecked
"""

    def test_kernel_bypass_is_rejected_by_replay(self):
        for version in check.VERSIONS:
            with self.subTest(version=version), tempfile.TemporaryDirectory(prefix="lean-gate-test-") as tmp:
                root = Path(tmp)
                build = root / "build"
                build.mkdir()
                path = root / "Bypass.lean"
                path.write_text(self.bypass)
                with self.assertRaisesRegex(ValueError, "forbidden proof-source"):
                    check.check_module(version, path, root, build, {})
                module, _ = check.compile_module(version, path, root, build)
                code, packet = check.audit_module(version, module, build)
                self.assertEqual(code, 0, packet)
                self.assertTrue(packet["accepted"])
                with self.assertRaisesRegex(ValueError, "Lean kernel replay rejected Bypass"):
                    check.replay_module(version, module, build)
                # The complete module check replays even when the textual guard is bypassed.
                with mock.patch.object(check, "check_source"), contextlib.redirect_stdout(io.StringIO()), \
                        self.assertRaisesRegex(ValueError, "Lean kernel replay rejected Bypass"):
                    check.check_module(version, path, root, build, {})
                print("GATE_CONTROL " + json.dumps({"name": "kernel_bypass", "version": version,
                    "compile": 0, "audit": code, "audit_accepted": True, "replay_rejected": True}), flush=True)


if __name__ == "__main__":
    unittest.main()
