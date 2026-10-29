"""Actual ontology MCP requests stay separate from behavioral review and authority."""
from copy import deepcopy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from mcp import Bridge, TOOLS
from zal_notation.ontology import ontology_example
from zal_notation.ontology_cnl import render_ontology_cnl
from zal_notation.ontology_tools import classify

ROOT = Path(__file__).resolve().parent


class UnusedWorkspace:
    def use(self, action):
        raise AssertionError("ontology requests must not read or change behavioral review")


class OntologyMCPTests(unittest.TestCase):
    def call(self, name, arguments):
        result = Bridge(UnusedWorkspace()).handle({"jsonrpc": "2.0", "id": 1,
            "method": "tools/call", "params": {"name": name, "arguments": arguments}})
        return result, json.loads(result["content"][0]["text"]) if not result["isError"] else None

    def test_catalog_has_six_unique_operations_without_replacing_existing_tools(self):
        names = [tool["name"] for tool in TOOLS]
        self.assertEqual(len(names), len(set(names)))
        self.assertEqual({name for name in names if name.startswith("zal_ontology_")},
            {"zal_ontology_" + name for name in ("parse", "classify", "query", "explain", "replay", "validate_record")})
        self.assertIn("zal_compare", names)
        self.assertIn("zal_propose", names)
        self.assertNotIn("zal_accept", names)

    def test_parse_query_and_explanation_use_actual_rules(self):
        document = ontology_example()
        result, parsed = self.call("zal_ontology_parse", {"source": render_ontology_cnl(document)})
        self.assertFalse(result["isError"])
        self.assertEqual(parsed["ontology"], document)
        _, query = self.call("zal_ontology_query", {"ontology": document,
            "individual": "trainingRun", "class_name": "FeedbackGuidedConstruction"})
        self.assertEqual(len(query["matches"]), 1)
        _, explanation = self.call("zal_ontology_explain", {"ontology": document,
            "fact": query["matches"][0]["id"]})
        self.assertEqual(explanation["status"], "explained")
        self.assertEqual(explanation["authority"], "none")
        self.assertTrue(any(p["axiom"] == "feedback-construction" for p in explanation["proofs"]))

    def test_exhaustion_and_forged_replay_never_become_classification(self):
        document = ontology_example()
        _, run = self.call("zal_ontology_classify", {"ontology": document, "max_work": 0})
        self.assertEqual(run["result"]["status"], "incomplete")
        _, replay = self.call("zal_ontology_replay", {"ontology": document, "run": run})
        self.assertTrue(replay["matches_replay"])
        self.assertEqual(replay["current_status"], "incomplete")
        forged = deepcopy(run)
        forged["result"]["status"] = "classified"
        _, replay = self.call("zal_ontology_replay", {"ontology": document, "run": forged})
        self.assertFalse(replay["matches_replay"])

    def test_profile_validator_refuses_aliases_extra_fields_and_code(self):
        document = ontology_example()
        for arguments in ({"ontology": document, "max_work": True},
                          {"ontology": document, "max_facts": 1025},
                          {"ontology": document, "exec": "ignored?"},
                          {"ontology": []}):
            with self.subTest(arguments=list(arguments)):
                response, _ = self.call("zal_ontology_classify", arguments)
                self.assertTrue(response["isError"])
        response, _ = self.call("zal_ontology_parse", {"source": 'Import "file:///outside".'})
        self.assertTrue(response["isError"])

    def test_shape_conformance_does_not_validate_a_receipt(self):
        record = {"id": "urn:record", "source": "urn:source", "target": "urn:target",
                  "scope": "0" * 64, "checker": "1" * 64, "receipt": "f" * 64}
        _, result = self.call("zal_ontology_validate_record", {"record": record})
        self.assertTrue(result["conforms"])
        self.assertEqual(result["evidence"], "unchecked")
        self.assertEqual(result["authority"], "none")

    def test_defi_example_shares_width_without_conflating_signedness(self):
        source = (ROOT / "examples/ontology/defi_arithmetic.english.txt").read_text()
        _, parsed = self.call("zal_ontology_parse", {"source": source})
        document = parsed["ontology"]
        _, wide = self.call("zal_ontology_query", {"ontology": document, "class_name": "NeedsWideArithmetic"})
        self.assertEqual({f["individual"] for f in wide["matches"]}, {"swapBalance", "signedPnl"})
        _, unsigned = self.call("zal_ontology_query", {"ontology": document, "class_name": "NeedsUnsignedArithmetic"})
        self.assertEqual([f["individual"] for f in unsigned["matches"]], ["swapBalance"])
        _, signed = self.call("zal_ontology_query", {"ontology": document, "class_name": "NeedsSignedArithmetic"})
        self.assertEqual([f["individual"] for f in signed["matches"]], ["signedPnl"])

    def test_real_stdio_service_serves_all_operations_without_a_workspace(self):
        document = ontology_example()
        run = classify(document)
        fact = next(f["id"] for f in run["result"]["facts"]
                    if f["kind"] == "type" and f["class"] == "FeedbackGuidedConstruction")
        arguments = [
            ("parse", {"source": render_ontology_cnl(document)}),
            ("classify", {"ontology": document}),
            ("query", {"ontology": document, "class_name": "FeedbackGuidedConstruction"}),
            ("explain", {"ontology": document, "fact": fact}),
            ("replay", {"ontology": document, "run": run}),
            ("validate_record", {"record": {"id": "urn:missing"}}),
        ]
        requests = [{"jsonrpc": "2.0", "id": 0, "method": "tools/list"}]
        requests += [{"jsonrpc": "2.0", "id": i + 1, "method": "tools/call",
                      "params": {"name": "zal_ontology_" + name, "arguments": value}}
                     for i, (name, value) in enumerate(arguments)]
        with tempfile.TemporaryDirectory() as temporary:
            workspace = Path(temporary) / "never-created.json"
            command = [sys.executable, *(["-O"] if sys.flags.optimize else []), "-B",
                       str(ROOT / "mcp.py"), "--workspace", str(workspace)]
            process = subprocess.run(command, cwd=ROOT, input="".join(json.dumps(r) + "\n" for r in requests),
                                     capture_output=True, text=True, timeout=20)
            self.assertEqual(process.returncode, 0, process.stderr)
            replies = [json.loads(line) for line in process.stdout.splitlines()]
            self.assertEqual([r["id"] for r in replies], list(range(7)))
            self.assertTrue(all(not r["result"]["isError"] for r in replies[1:]))
            results = [json.loads(r["result"]["content"][0]["text"]) for r in replies[1:]]
            self.assertTrue(all(r["authority"] == "none" for r in results))
            self.assertTrue(results[4]["matches_replay"])
            self.assertFalse(results[5]["conforms"])
            self.assertFalse(workspace.exists())


class OntologyCLIBoundaryTests(unittest.TestCase):
    def cli(self, *arguments, input=None):
        return subprocess.run([sys.executable, *(["-O"] if sys.flags.optimize else []),
            "-B", "-m", "zal_notation", *arguments], cwd=ROOT,
            input=input, capture_output=True, text=True, timeout=20)

    def test_outputs_preserve_existing_files_and_stdio_refuses_other_tool_families(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "ontology.txt"
            source.write_text(render_ontology_cnl(ontology_example()))
            output = root / "keep.json"
            output.write_text("retained user content")
            result = self.cli("ontology-parse", "--source", str(source), "--output", str(output))
            self.assertEqual(result.returncode, 2)
            self.assertEqual(output.read_text(), "retained user content")
            request = {"name": "zal_propose", "arguments": {"accept": True}}
            result = self.cli("tools-stdio", input=json.dumps(request) + "\n")
            self.assertEqual(json.loads(result.stdout)["status"], "refused")
            duplicate = '{"name":"zal_ontology_query","name":"zal_propose","arguments":{}}\n'
            result = self.cli("tools-stdio", input=duplicate)
            self.assertEqual(json.loads(result.stdout)["status"], "refused")

    def test_demo_browse_and_replay_commands_preserve_the_checked_packet(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            demo = root / "demo"
            result = self.cli("ontology-demo", "--output", str(demo))
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            result = self.cli("ontology-replay", "--directory", str(demo))
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertTrue(json.loads(result.stdout)["matches_replay"])
            browse = root / "browse"
            result = self.cli("ontology-browse", "--source", str(demo / "ontology.english.txt"),
                              "--output", str(browse))
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            (browse / "index.html").write_text("forged browser")
            result = self.cli("ontology-replay", "--directory", str(browse))
            self.assertEqual(result.returncode, 2)
            self.assertEqual(json.loads(result.stdout)["status"], "refused")


if __name__ == "__main__":
    unittest.main()
