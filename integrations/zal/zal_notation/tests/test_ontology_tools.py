"""Integration contracts for source-bound ontology tools, CLI, and packet replay."""
from copy import deepcopy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from zal_notation import core
from zal_notation.ontology import ontology_example
from zal_notation.ontology_cnl import render_ontology_cnl
from zal_notation.ontology_demo import evidence_boundary, generate, replay as replay_packet
from zal_notation.ontology_tools import classify, explain, query, replay
from zal_notation.sharing import dispatch_tool, toolkit

ROOT = Path(__file__).resolve().parents[2]


class OntologyIntegrationTests(unittest.TestCase):
    def test_query_explanation_and_tools_use_fresh_classification(self):
        document = ontology_example()
        result = query(document, individual="trainingRun", class_name="FeedbackGuidedConstruction")
        self.assertEqual(len(result["matches"]), 1)
        explanation = explain(document, result["matches"][0]["id"])
        self.assertEqual(explanation["status"], "explained")
        self.assertGreater(len(explanation["proofs"]), 1)
        self.assertTrue(all(p["fact"] in {f["id"] for f in explanation["facts"]} for p in explanation["proofs"]))
        self.assertEqual(query(document, individual="not-declared")["matches"], [])
        self.assertEqual(explain(document, "not-established")["status"], "not-established")
        names = {entry["name"] for entry in toolkit()["tools"]}
        self.assertIn("zal_ontology_explain", names)
        self.assertIn("zal_lens_verify", names)
        dispatched = dispatch_tool("zal_ontology_query", {"ontology": document, "individual": "trainingRun"})
        self.assertEqual(dispatched, query(document, individual="trainingRun"))
        with self.assertRaises(core.NotationError):
            dispatch_tool("zal_ontology_classify", {"ontology": document, "exec": "ignored?"})

    def test_whole_run_replay_rejects_context_proof_and_source_forgery(self):
        document = ontology_example()
        run = classify(document)
        self.assertTrue(replay(document, run)["matches_replay"])
        for field in ("source", "fact", "status", "request"):
            forged = deepcopy(run)
            if field == "source":
                forged["implementation"]["files"]["ontology.py"] = "0" * 64
            elif field == "fact":
                forged["result"]["facts"].pop()
            elif field == "status":
                forged["result"]["status"] = "equivalent"
            else:
                forged["request"]["max_work"] -= 1
            self.assertFalse(replay(document, forged)["matches_replay"], field)
        changed = deepcopy(document)
        changed["version"] += 1
        self.assertFalse(replay(changed, run)["matches_replay"])
        stopped = classify(document, max_work=0)
        self.assertTrue(replay(document, stopped)["matches_replay"])
        self.assertEqual(stopped["result"]["status"], "incomplete")

    def test_classification_shape_and_behavior_are_separate(self):
        control = evidence_boundary()
        self.assertEqual(control["classification"]["types"]["record"], ["ClaimRecord"])
        self.assertFalse(control["missing_shape"]["conforms"])
        self.assertEqual({row["path"] for row in control["missing_shape"]["results"]}, {"receipt"})
        self.assertTrue(control["forged_shape"]["conforms"])
        self.assertEqual(control["fresh_result"]["status"], "refuted")
        self.assertTrue(control["genuine_matches_replay"])
        self.assertFalse(control["forged_matches_replay"])

    def test_packet_replay_recomputes_browser_and_does_not_overwrite(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "packet"
            self.assertEqual(generate(root)["status"], "classified")
            self.assertTrue(replay_packet(root)["matches_replay"])
            with self.assertRaises(core.NotationError):
                generate(root)
            html = root / "index.html"
            original = html.read_bytes()
            html.write_bytes(original.replace(b"shared category", b"proven equivalent"))
            with self.assertRaises(core.NotationError):
                replay_packet(root)
            html.write_bytes(original)
            result_path = root / "classification.json"
            original_result = result_path.read_bytes()
            result = json.loads(original_result)
            result["result"]["proofs"][0]["assertion"] = "forged"
            result_path.write_text(json.dumps(result), encoding="utf-8")
            with self.assertRaises(core.NotationError):
                replay_packet(root)
            result_path.write_bytes(original_result)
            source = root / "ontology.json"
            saved = source.read_bytes()
            source.unlink()
            elsewhere = Path(tmp) / "outside.json"
            elsewhere.write_bytes(saved)
            source.symlink_to(elsewhere)
            with self.assertRaises(core.NotationError):
                replay_packet(root)

    def test_browser_escapes_hostile_names(self):
        from zal_notation.ontology_demo import artifacts
        document = {"schema": "zal.ontology/1", "id": "escape", "version": 1,
                    "classes": ["</script><script>alert(1)</script>"], "properties": [],
                    "individuals": ["<img src=x onerror=alert(2)>"], "axioms": [], "facts": []}
        packet, files = artifacts(document)
        html = files["index.html"].decode()
        self.assertNotIn(document["classes"][0], html)
        embedded = html.split('<script type="application/json" id="ontology-data">', 1)[1].split('</script>', 1)[0]
        self.assertEqual(json.loads(embedded), packet)

    def test_cli_cnl_classification_stdio_and_budget_exit(self):
        def cli(*args, input=None):
            return subprocess.run([sys.executable, "-m", "zal_notation", *args], cwd=ROOT,
                                  input=input, text=True, capture_output=True, timeout=30)
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "ontology.txt"
            source.write_text(render_ontology_cnl(ontology_example()), encoding="utf-8")
            parsed = cli("ontology-parse", "--source", str(source))
            self.assertEqual(parsed.returncode, 0, parsed.stderr)
            self.assertEqual(json.loads(parsed.stdout), ontology_example())
            result = cli("ontology-classify", "--source", str(source))
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout)["result"]["status"], "classified")
            exhausted = cli("ontology-classify", "--source", str(source), "--max-work", "0")
            self.assertEqual(exhausted.returncode, 2)
            self.assertEqual(json.loads(exhausted.stdout)["result"]["status"], "incomplete")
            request = {"name": "zal_ontology_query", "arguments": {"ontology": ontology_example(), "class_name": "FeedbackGuidedConstruction"}}
            stdio = cli("tools-stdio", input=json.dumps(request) + "\n")
            self.assertEqual(stdio.returncode, 0, stdio.stderr)
            self.assertEqual(len(json.loads(stdio.stdout)["matches"]), 2)


if __name__ == "__main__":
    unittest.main()
