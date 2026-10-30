"""Research pilot acceptance, refusal, and negative-control tests."""

from __future__ import annotations

import copy
import json
from pathlib import Path
import tempfile
import unittest

import research

ROOT = Path(__file__).resolve().parent


class ContractGuidedResearchTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.corpus = research.read_json(ROOT / "corpus.json")

    def test_corpus_and_reference_baselines(self):
        problems = research.validate_corpus(self.corpus)
        self.assertEqual(len(problems), 6)
        for problem in problems:
            self.assertEqual(research.verify(problem, problem["baseline"])["cases_checked"], 256)

    def test_independent_oracle_finds_mutated_candidate(self):
        problem = self.corpus["problems"][0]
        with self.assertRaisesRegex(research.Refusal, "semantic counterexample"):
            research.verify(problem, ["input", "x"])
        # A normal semantic mismatch, not a stale hash, syntax failure or timeout.
        with self.assertRaisesRegex(research.Refusal, "semantic counterexample"):
            research.verify(problem, ["and", ["input", "x"], ["const", 0]])

    def test_full_pipeline_and_replay(self):
        artifact = research.run(self.corpus, 10000)
        self.assertEqual(len(artifact["experiments"]), 12)
        self.assertEqual(research.replay(self.corpus, artifact), len(artifact["entries"]))
        self.assertGreaterEqual(len(artifact["entries"]), 6)
        for item in artifact["entries"]:
            self.assertEqual(item["verification"]["cases_checked"], 256)
            self.assertEqual(item["cost"]["metric"], "abstract-tree-nodes")
        self.assertTrue(any(x["status"] == "none-within-grammar"
                            for x in artifact["experiments"]))
        self.assertFalse(any(x["status"].startswith("inconclusive")
                             for x in artifact["experiments"]))

    def test_tampered_candidate_is_not_promoted(self):
        artifact = research.run(self.corpus, 10000)
        altered = copy.deepcopy(artifact)
        altered["entries"][0]["expression"] = ["input", "x"]
        with self.assertRaisesRegex(research.Refusal, "hash mismatch"):
            research.replay(self.corpus, altered)
        altered["entries"][0]["expression_sha256"] = research.digest(["input", "x"])
        with self.assertRaisesRegex(research.Refusal, "semantic counterexample"):
            research.replay(self.corpus, altered)

    def test_cost_forgery_is_refused(self):
        altered = research.run(self.corpus, 10000)
        altered["entries"][0]["cost"]["candidate"] += 1
        with self.assertRaisesRegex(research.Refusal, "candidate outside palette size bound|cost evidence drift"):
            research.replay(self.corpus, altered)

    def test_budget_exhaustion_is_inconclusive(self):
        p = self.corpus["problems"][0]
        result = research.search(p, p["palettes"][0], 1)
        self.assertEqual(result["status"], "inconclusive-budget")
        self.assertIsNone(result["candidate"])

    def test_large_grammar_is_inconclusive_without_materialization(self):
        p = self.corpus["problems"][0]
        large = {"name": "large", "operators": list(research.OPS),
                 "constants": list(range(8)), "shifts": list(range(8)), "max_nodes": 7}
        result = research.search(p, large, 1)
        self.assertEqual(result["status"], "inconclusive-grammar-size")
        self.assertEqual(result["candidates_inspected"], 0)

    def test_missing_catalog_experiment_is_refused(self):
        altered = research.run(self.corpus, 10000)
        altered["experiments"].pop()
        with self.assertRaisesRegex(research.Refusal, "missing/duplicate"):
            research.replay(self.corpus, altered)

    def test_wrong_profile_duplicate_key_and_unknown_operator_refused(self):
        bad = copy.deepcopy(self.corpus)
        bad["problems"][0]["profile"] = "signed-i64/1"
        with self.assertRaisesRegex(research.Refusal, "unsupported"):
            research.validate_corpus(bad)
        with self.assertRaisesRegex(research.Refusal, "unsupported operation"):
            research.validate_expr(["injection", ["input", "x"]])
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/"bad.json"
            path.write_text('{"schema":1,"schema":2}', encoding="utf-8")
            with self.assertRaisesRegex(research.Refusal, "duplicate JSON key"):
                research.read_json(path)

    def test_immutable_artifact_and_cli(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "result.json"
            self.assertEqual(research.main(["run", "--corpus", str(ROOT/"corpus.json"),
                                            "--out", str(target)]), 0)
            self.assertTrue(target.is_file())
            self.assertEqual(research.main(["replay", "--corpus", str(ROOT/"corpus.json"),
                                            "--catalog", str(target)]), 0)
            self.assertEqual(research.main(["run", "--corpus", str(ROOT/"corpus.json"),
                                            "--out", str(target)]), 1)

    def test_reproducible_catalog(self):
        self.assertEqual(research.canonical(research.run(self.corpus, 10000)),
                         research.canonical(research.run(self.corpus, 10000)))


if __name__ == "__main__":
    unittest.main()
