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

    def test_right_to_left_impossibility_before_search(self):
        """Chapter 2: a missing right-shift prevents higher-to-lower flow."""
        experiments = research.run(self.corpus, 10000)["experiments"]
        found = {(e["problem"], e["palette_name"]): e for e in experiments}
        for problem, palette in (
            ("divide-by-eight-u8", "control-no-shift"),
            ("gray-code-u8", "control-no-shift"),
            ("rotate-left-one-u8", "control-no-right-shift"),
        ):
            report = found[(problem, palette)]
            self.assertEqual(report["status"], "infeasible-right-to-left-palette")
            self.assertEqual(report["candidates_inspected"], 0)
            witness = report["preflight"]
            self.assertEqual(witness["rule"], "right-to-left-prefix-u8/1")
            mask = (1 << witness["bits"]) - 1
            self.assertEqual(witness["input_a"] & mask, witness["input_b"] & mask)
            ref = next(p for p in self.corpus["problems"] if p["id"] == problem)["reference"]
            self.assertNotEqual(research.reference.result(ref, witness["input_a"]) & mask,
                                research.reference.result(ref, witness["input_b"]) & mask)
        # A palette that includes a real right shift must not receive this refusal.
        prob = next(p for p in self.corpus["problems"] if p["id"] == "gray-code-u8")
        self.assertIsNone(research.structural_preflight(prob, prob["palettes"][0]))
        # A shift by 0 is still prefix preserving, so it cannot defeat the refutation.
        bogus = {"name":"shift0", "operators":["xor", "lshr"],
                 "constants":[], "shifts":[0], "max_nodes":4}
        self.assertIsNotNone(research.structural_preflight(prob, bogus))

    def test_right_to_left_witness_tampering_is_detected(self):
        artifact = research.run(self.corpus, 10000)
        altered = copy.deepcopy(artifact)
        target = next(e for e in altered["experiments"]
                      if e["status"] == "infeasible-right-to-left-palette")
        target["preflight"]["input_b"] ^= 1
        with self.assertRaisesRegex(research.Refusal, "experiment replay mismatch"):
            research.replay(self.corpus, altered)

    def test_right_to_left_positive_closure_for_supported_atoms(self):
        """Check a complete 4-bit atom cross-product; closure follows by induction."""
        for bits in range(1, 4):
            mask = (1 << bits) - 1
            for a in range(16):
                for b in range(16):
                    a2, b2 = a ^ (1 << bits), b ^ (1 << bits)
                    if a2 > 15 or b2 > 15:
                        continue
                    pairs = [(a & b, a2 & b2), (a | b, a2 | b2),
                             (a ^ b, a2 ^ b2), ((a+b)&15, (a2+b2)&15),
                             ((a-b)&15, (a2-b2)&15),
                             ((~a)&15, (~a2)&15),
                             ((a<<1)&15, (a2<<1)&15)]
                    self.assertTrue(all((left & mask) == (right & mask)
                                        for left, right in pairs))

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
