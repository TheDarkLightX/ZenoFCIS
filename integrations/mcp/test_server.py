"""Real MCP client checks; run with the pinned SDK and ZENO_FCIS_CLI set."""

import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest

from mcp import Client

from zeno_fcis_synthesis import PROGRAM_TEMPLATES, mcp


ROOT = Path(__file__).resolve().parents[2]
ORDER = ROOT / "crates/zeno-fcis-cli/templates/order-fulfillment"


class SynthesisToolsTest(unittest.IsolatedAsyncioTestCase):
    async def test_core_evidence_and_tamper_refusal(self):
        if not os.environ.get("ZENO_FCIS_CLI"):
            self.skipTest("set ZENO_FCIS_CLI to the built CLI")
        async with Client(mcp) as client:
            tools = {tool.name for tool in (await client.list_tools()).tools}
            self.assertEqual(tools, {"assess_finite_problem", "synthesize_finite_core",
                                     "verify_finite_core", "discover_program_authoring",
                                     "create_program_project", "check_project_spec",
                                     "transform_request", "transform_candidate",
                                     "transform_replay"})
            problem = str(ORDER / "synthesis.json")
            output = str(ORDER / "synthesized")
            assessed = (await client.call_tool("assess_finite_problem",
                                               {"problem_path": problem})).structured_content
            self.assertEqual(assessed["input_tuples"], 1728)
            self.assertTrue(assessed["within_tuple_limits"])
            current = (await client.call_tool("synthesize_finite_core",
                                              {"problem_path": problem, "output_path": output,
                                               "check_existing": True})).structured_content
            self.assertEqual((current["exit_code"], current["report"]["status"]), (0, "current"))
            verified = (await client.call_tool("verify_finite_core",
                                               {"problem_path": problem, "output_path": output})).structured_content
            self.assertEqual(verified["report"]["inputs_checked"], 1728)
            self.assertEqual(verified["report"]["status"], "passed")
            with tempfile.TemporaryDirectory(prefix="zenofcis-mcp-test-") as temporary:
                copied = Path(temporary) / "synthesized"
                shutil.copytree(ORDER / "synthesized", copied)
                with (copied / "transition.rs").open("a") as source:
                    source.write("\n")
                drift = (await client.call_tool("verify_finite_core",
                                                {"problem_path": problem,
                                                 "output_path": str(copied)})).structured_content
                self.assertEqual((drift["exit_code"], drift["report"]["status"]),
                                 (1, "artifact-drift"))
                oversized = json.loads((ORDER / "synthesis.json").read_text())
                oversized["inputs"][0]["type"]["max"] = 65_536
                oversized_path = Path(temporary) / "oversized.json"
                oversized_path.write_text(json.dumps(oversized))
                too_big = (await client.call_tool("assess_finite_problem",
                                                  {"problem_path": str(oversized_path)})).structured_content
                self.assertFalse(too_big["within_tuple_limits"])

    async def test_normal_program_authoring_and_refusals(self):
        if not os.environ.get("ZENO_FCIS_CLI"):
            self.skipTest("set ZENO_FCIS_CLI to the built CLI")
        async with Client(mcp) as client:
            described = (await client.call_tool("discover_program_authoring", {})).structured_content
            self.assertEqual(described["exit_code"], 0)
            self.assertEqual(described["authority"], "none")
            workflow = described["report"]["normal_program"]
            self.assertEqual(set(workflow["templates"]), PROGRAM_TEMPLATES)
            self.assertEqual(workflow["runtime_admission"], "not-run")
            with tempfile.TemporaryDirectory(prefix="zenofcis-mcp-program-") as temporary:
                for template in sorted(PROGRAM_TEMPLATES):
                    project = Path(temporary) / template
                    created = (await client.call_tool("create_program_project", {
                        "output_path": str(project), "template": template})).structured_content
                    self.assertEqual((created["exit_code"], created["report"]["status"]), (0, "created"))
                    self.assertEqual(created["report"]["runtime_admission"], "not-run")
                    original = (project / "src/v2_contract.rs").read_bytes()
                    self.assertIn(b"catalog::bind_original(", original)
                    self.assertIn(b"authority::bind(&catalog)", original)
                    checked = (await client.call_tool("check_project_spec", {
                        "project_path": str(project / "project.zeno")})).structured_content
                    self.assertEqual((checked["exit_code"], checked["report"]["status"]), (0, "valid"))
                    self.assertEqual(checked["report"]["authority"], "none")
                    self.assertEqual(checked["report"]["runtime_admission"], "not-run")
                    refused = (await client.call_tool("create_program_project", {
                        "output_path": str(project), "template": template})).structured_content
                    self.assertEqual((refused["exit_code"], refused["report"]["status"]), (1, "refused"))
                    self.assertEqual((project / "src/v2_contract.rs").read_bytes(), original)
                unsupported = await client.call_tool("create_program_project", {
                    "output_path": str(Path(temporary) / "unsupported"), "template": "native-callback"})
                self.assertTrue(unsupported.is_error)
                self.assertFalse((Path(temporary) / "unsupported").exists())
                invalid = Path(temporary) / "invalid.zeno"
                invalid.write_text("zeno 1; project", encoding="utf-8")
                diagnostic = (await client.call_tool("check_project_spec", {
                    "project_path": str(invalid)})).structured_content
                self.assertEqual(diagnostic["exit_code"], 1)
                self.assertEqual(diagnostic["authority"], "none")


if __name__ == "__main__":
    unittest.main()
