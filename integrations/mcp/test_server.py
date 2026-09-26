"""Real MCP client checks; run with the pinned SDK and ZENO_FCIS_CLI set."""

import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest

from mcp import Client

from zeno_fcis_synthesis import mcp


ROOT = Path(__file__).resolve().parents[2]
ORDER = ROOT / "crates/zeno-fcis-cli/templates/order-fulfillment"


class SynthesisToolsTest(unittest.IsolatedAsyncioTestCase):
    async def test_core_evidence_and_tamper_refusal(self):
        if not os.environ.get("ZENO_FCIS_CLI"):
            self.skipTest("set ZENO_FCIS_CLI to the built CLI")
        async with Client(mcp) as client:
            tools = {tool.name for tool in (await client.list_tools()).tools}
            self.assertEqual(tools, {"assess_finite_problem", "synthesize_finite_core",
                                     "verify_finite_core"})
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


if __name__ == "__main__":
    unittest.main()
