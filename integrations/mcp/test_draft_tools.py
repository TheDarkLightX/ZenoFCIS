"""Wrapper controls plus an explicit native CLI path regression.

Run DraftToolsTest alone for lightweight mocked checks. NativeDraftPathTest
requires the built CLI and is part of the existing G7 acceptance command.
"""
import json
import os
from pathlib import Path
import tempfile
import subprocess
import unittest
from unittest.mock import patch

import zeno_fcis_transform as tools


class DraftToolsTest(unittest.TestCase):
    def invoke(self, function, *args, **kwargs):
        report = {"schema": tools.DRAFT_SCHEMA, "authority": "none", "hosted_model": "off"}
        completed = subprocess.CompletedProcess([], 1, json.dumps(report), "refused")
        with patch.object(tools, "_cli", return_value="/pinned/zeno-fcis"), patch.object(
                tools.subprocess, "run", return_value=completed) as run:
            result = function(*args, **kwargs)
            self.assertEqual(result["exit_code"], 1)  # A refusal must not become success.
            self.assertEqual(result["authority"], "none")
            self.assertNotIn("shell", run.call_args.kwargs)
            self.assertEqual(run.call_args.kwargs["timeout"], 120)
            return run.call_args.args[0]

    def test_all_six_operations_use_fixed_cli_routes(self):
        cases = [
            (tools.contract_draft_start, ("/s", "/intent", "/project", "supplied fixture"), "start"),
            (tools.contract_draft_propose, ("/s", "abc", "/rules", "manual"), "propose"),
            (tools.contract_draft_questions, ("/s",), "questions"),
            (tools.contract_draft_label, ("/s", "abc", "/labels", "assumed owner"), "label"),
            (tools.contract_draft_check, ("/s",), "check"),
            (tools.contract_draft_finalize, ("/s", "abc", "/out"), "finalize"),
        ]
        for function, arguments, operation in cases:
            with self.subTest(operation=operation):
                command = self.invoke(function, *arguments)
                self.assertEqual(command[:4], ["/pinned/zeno-fcis", "contract", "draft", operation])
                self.assertIn(function, tools.TOOLS)

    def test_new_paths_preserve_real_dangling_final_links(self):
        with tempfile.TemporaryDirectory(prefix="zeno-draft-wrapper-path-") as directory:
            root = Path(directory)
            destination = root / "absent"
            link = root / "requested"
            link.symlink_to(destination, target_is_directory=True)
            command = self.invoke(tools.contract_draft_finalize, "/s", "abc", str(link))
            self.assertEqual(command[command.index("--out") + 1], str(link))
            command = self.invoke(tools.contract_draft_start, str(link), "/i", "/p", "fixture")
            self.assertEqual(command[command.index("--session") + 1], str(link))
            self.assertTrue(link.is_symlink())
            self.assertFalse(destination.exists())

    def test_budgets_and_provenance_fail_before_process(self):
        with patch.object(tools.subprocess, "run") as run:
            for rounds in [0, 9, True]:
                with self.assertRaises(tools.TransformToolError):
                    tools.contract_draft_start("/s", "/i", "/p", "fixture", rounds=rounds)
            with self.assertRaises(tools.TransformToolError):
                tools.contract_draft_label("/s", "old", "/labels", " ")
            run.assert_not_called()

    def test_unexpected_authority_hosted_or_schema_is_refused(self):
        for report in [{}, {"schema": tools.DRAFT_SCHEMA, "authority": "publication", "hosted_model": "off"},
                       {"schema": tools.DRAFT_SCHEMA, "authority": "none", "hosted_model": "on"}, []]:
            with patch.object(tools, "_cli", return_value="/cli"), patch.object(tools.subprocess, "run",
                    return_value=subprocess.CompletedProcess([], 0, json.dumps(report), "")):
                with self.assertRaises(tools.TransformToolError):
                    tools.contract_draft_check("/s")


class NativeDraftPathTest(unittest.TestCase):
    """Real wrapper-to-CLI existing-path refusals; never run in source-only checks."""

    def test_dangling_session_output_and_existing_output_are_preserved(self):
        from test_transform_tools import locate_cli
        executable = locate_cli()
        template = Path(__file__).resolve().parents[2] / "crates/zeno-fcis-cli/templates/durable-counter"
        with tempfile.TemporaryDirectory(prefix="zeno-draft-native-path-") as directory, patch.dict(
                os.environ, {"ZENO_FCIS_CLI": executable}):
            root = Path(directory)
            missing_session = root / "absent-session"
            linked_session = root / "requested-session"
            linked_session.symlink_to(missing_session, target_is_directory=True)
            refused = tools.contract_draft_start(str(linked_session), str(template / "README.md"),
                str(template / "project.zeno"), "path refusal fixture")
            self.assertNotEqual(refused["exit_code"], 0)
            self.assertTrue(linked_session.is_symlink())
            self.assertFalse(missing_session.exists())

            session = root / "session"
            opened = tools.contract_draft_start(str(session), str(template / "README.md"),
                str(template / "project.zeno"), "retained template fixture; no live owner",
                examples_path=str(template / "tests/decision-examples.txt"))
            self.assertEqual(opened["exit_code"], 0)
            current = tools.contract_draft_propose(str(session), opened["report"]["revision"],
                str(template / "v2/policy.json"), "unchanged original template for path regression")
            self.assertEqual(current["exit_code"], 0)
            # This filesystem control uses only the unchanged retained baseline.
            # Its proposed answers are explicitly simulated test labels, never
            # represented as human review or used as an oracle for a wrong candidate.
            for _ in range(8):
                detail = current["report"]["detail"]
                if not detail["unlabeled_inputs"]:
                    break
                examples = [question["proposed_example"] for question in detail["questions"]
                            if not question["label_supplied"]]
                self.assertTrue(examples)
                self.assertTrue(all(isinstance(example, str) for example in examples))
                supplied = root / "simulated-labels.txt"
                supplied.write_text("\n".join(examples), encoding="utf-8")
                current = tools.contract_draft_label(str(session), current["report"]["revision"],
                    str(supplied), "SIMULATED baseline-library labels solely for filesystem regression")
                self.assertEqual(current["exit_code"], 0)
            checked = tools.contract_draft_check(str(session))
            self.assertEqual(checked["exit_code"], 0)
            self.assertTrue(checked["report"]["detail"]["ready"])
            revision = checked["report"]["revision"]
            missing_output = root / "absent-output"
            linked_output = root / "requested-output"
            linked_output.symlink_to(missing_output, target_is_directory=True)
            direct = subprocess.run([executable, "contract", "draft", "finalize", "--session", str(session),
                "--revision", revision, "--out", str(linked_output)], capture_output=True, text=True,
                timeout=120, check=False)
            self.assertNotEqual(direct.returncode, 0)
            self.assertTrue(linked_output.is_symlink())
            self.assertFalse(missing_output.exists())
            wrapped = tools.contract_draft_finalize(str(session), revision, str(linked_output))
            self.assertNotEqual(wrapped["exit_code"], 0)
            self.assertTrue(linked_output.is_symlink())
            self.assertFalse(missing_output.exists())

            output = root / "new-output"
            finalized = tools.contract_draft_finalize(str(session), revision, str(output))
            self.assertEqual(finalized["exit_code"], 0)
            retained = (output / "draft-finalized.json").read_bytes()
            self.assertNotEqual(tools.contract_draft_finalize(str(session), revision, str(output))["exit_code"], 0)
            self.assertEqual((output / "draft-finalized.json").read_bytes(), retained)


if __name__ == "__main__":
    unittest.main()
