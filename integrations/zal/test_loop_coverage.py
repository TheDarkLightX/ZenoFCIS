"""Coverage-loop authority, refusal and native regression checks."""

import dataclasses
import copy
import hashlib
import json
import os
import re
import shlex
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import behavior2
import factory
import loop_coverage as coverage

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]


class CoverageVerdictTests(unittest.TestCase):
    def setUp(self):
        self.before_packet = json.loads((HERE / "test-data/coverage/before.json").read_text())
        self.after_packet = json.loads((HERE / "test-data/coverage/after.json").read_text())
        self.project = (ROOT / "examples/guide-counter/project.zeno").read_text()
        self.policy = json.loads((ROOT / "examples/guide-counter/v2/policy.json").read_text())
        self.before = coverage.snapshot(self.before_packet, self.project, self.policy)

    def assess(self, packet, code=0):
        return coverage.assess(packet, code, self.before, self.project, self.policy)

    def test_native_progress_is_required_for_kept(self):
        verdict, detail = self.assess(self.after_packet)
        self.assertEqual(verdict, "kept")
        self.assertEqual((detail["gaps_closed"], detail["mutants_rejected"]), (4, 6))
        verdict, _ = self.assess(self.before_packet)
        self.assertEqual(verdict, "rejected", "a law closing nothing was kept")

    def test_planted_disagreement_overrides_apparent_progress(self):
        planted = copy.deepcopy(self.after_packet)
        planted["gaps"]["status"] = "proposal-false-alarm"
        planted["gaps"]["false_alarm"] = {"law": 9000, "input": [0, 150, 1], "contract": {"class": "Accept"}}
        verdict, detail = self.assess(planted, 1)
        self.assertEqual(verdict, "disagreeing", "binary disagreement promoted to kept")
        self.assertEqual(detail["witness"], planted["gaps"]["false_alarm"])

    def test_unclassified_witnesses_survive(self):
        with patch.object(coverage, "DIMENSION_RULES", ()):
            rows = coverage.witnesses(coverage.measurement(self.before_packet), self.before_packet,
                                      self.project, self.policy)
        self.assertEqual(len(rows), 12, "unclassified witnesses were dropped")
        self.assertTrue(all(w["dimensions"] == ["unclassified"] for w in rows))

    def test_budget_never_becomes_a_measured_result(self):
        with self.assertRaises(coverage.JudgeStop) as raised:
            self.assess({"gaps": {"status": "inconclusive", "reason": "work-over-limit"}}, 2)
        self.assertEqual(raised.exception.code, 2)

    def test_input_packets_are_not_changed_by_assessment(self):
        original = copy.deepcopy(self.after_packet)
        self.assess(self.after_packet)
        self.assertEqual(original, self.after_packet)

    def test_disagreement_is_present_in_json_and_reading(self):
        candidate = coverage.Candidate("false", "proposed", ("law 9010 bad on any = false;",),
                                       ("authority",), "false")
        def review(judge, laws=()):
            if not laws:
                return 0, self.before_packet, "before.json"
            bad = {"gaps": {"status": "proposal-false-alarm", "false_alarm": {"law": 9010, "input": [0, 150, 1]}}}
            return 1, bad, "bad.json"
        with tempfile.TemporaryDirectory() as tmp:
            options = type("Options", (), {"directory": ROOT / "examples/guide-counter", "out": Path(tmp) / "report",
                                           "proposals": None, "zal": None, "max_work": None, "rounds": 3})()
            binary = Path(tmp) / "binary"
            binary.write_text("fixture")
            with patch.dict(os.environ, {"ZENO_FCIS_BIN": str(binary)}), \
                 patch.object(coverage, "proposals", return_value=[candidate]), \
                 patch.object(coverage, "synthesize", return_value=[]), \
                 patch.object(coverage.Judge, "review", review):
                self.assertEqual(coverage.run(options), 1)
            report = json.loads((options.out / "coverage.json").read_text())
            self.assertEqual(len(report["rounds"][0]["disagreeing"]), 1, "disagreement silently dropped")
            self.assertIn("Disagreement witness:", (options.out / "COVERAGE.md").read_text())

    def test_rounds_recheck_all_prior_kept_laws(self):
        final_packet = copy.deepcopy(self.after_packet)
        view = final_packet["gaps"]["proposals"]["after"]
        view["admissible"]["full"].update(undetermined=0, determined=12, examples=[])
        view["mutation"]["full"]["admitted"] = 0
        for mutant in view["mutation"]["list"]:
            mutant["classification"] = "caught"
        first = coverage.Candidate("one", "synthesized", ("law 9100 one on reject = true;",), ("authority",), "one")
        second = coverage.Candidate("two", "synthesized", ("law 9101 two on reject = true;",), ("failure",), "two")
        calls = []
        def review(judge, laws=()):
            calls.append(laws)
            return 0, [self.before_packet, self.after_packet, final_packet][len(calls) - 1], "native.json"
        with tempfile.TemporaryDirectory() as tmp:
            options = type("Options", (), {"directory": ROOT / "examples/guide-counter", "out": Path(tmp) / "out",
                                           "proposals": None, "zal": None, "max_work": None, "rounds": 3})()
            binary = Path(tmp) / "binary"
            binary.write_text("fixture")
            with patch.dict(os.environ, {"ZENO_FCIS_BIN": str(binary)}), \
                 patch.object(coverage, "synthesize", side_effect=[[first], [second]]), \
                 patch.object(coverage.Judge, "review", review):
                self.assertEqual(coverage.run(options), 0)
            result = json.loads((options.out / "coverage.json").read_text())
            self.assertEqual(len(result["rounds"]), 2)
            self.assertEqual(calls[-1], first.laws + second.laws)

    def test_no_progress_proposal_is_retried_after_the_law_set_changes(self):
        final_packet = copy.deepcopy(self.after_packet)
        view = final_packet["gaps"]["proposals"]["after"]
        view["admissible"]["full"].update(undetermined=0, determined=12, examples=[])
        view["mutation"]["full"]["admitted"] = 0
        for mutant in view["mutation"]["list"]:
            mutant["classification"] = "caught"
        proposed = coverage.Candidate("context_dependent", "proposed", ("law 9100 suggestion on reject = true;",), ("state",), "suggestion")
        synthesized = coverage.Candidate("first_gain", "synthesized", ("law 9101 first_gain on reject = true;",), ("state",), "first gain")
        calls = []
        def review(judge, laws=()):
            calls.append(laws)
            return 0, [self.before_packet, self.before_packet, self.after_packet, final_packet][len(calls) - 1], "native.json"
        with tempfile.TemporaryDirectory() as tmp:
            options = type("Options", (), {"directory": ROOT / "examples/guide-counter", "out": Path(tmp) / "out",
                                           "proposals": None, "zal": None, "max_work": None, "rounds": 3})()
            binary = Path(tmp) / "binary"
            binary.write_text("fixture")
            with patch.dict(os.environ, {"ZENO_FCIS_BIN": str(binary)}), \
                 patch.object(coverage, "proposals", side_effect=lambda *a, **kw: [] if kw.get("idioms") else [proposed]), \
                 patch.object(coverage, "synthesize", return_value=[synthesized]), \
                 patch.object(coverage.Judge, "review", review):
                self.assertEqual(coverage.run(options), 0)
            report = json.loads((options.out / "coverage.json").read_text())
            self.assertEqual(len(calls), 4, "a no-progress law was not retried in its new context")
            self.assertEqual(calls[-1], synthesized.laws + proposed.laws)
            self.assertEqual(report["rounds"][0]["rejected"][0]["id"], proposed.id)
            self.assertEqual(report["rounds"][1]["kept"][0]["id"], proposed.id)


class CoverageDocumentationTests(unittest.TestCase):
    def test_card_and_reference_show_the_same_exact_commands_and_outputs(self):
        card_text = (ROOT / "docs/zal/COVERAGE_CARD.md").read_text()
        card = json.loads(card_text.removeprefix("```json\n").split("\n```", 1)[0])
        reference = (ROOT / "docs/zal/LOOPS.md").read_text().split("## Coverage expansion\n", 1)[1].split("## MCP tools", 1)[0]
        for text in (card_text, reference):
            self.assertEqual(re.findall(r"```sh\n(.*?)\n```", text, re.S), [step["command"] for step in card["steps"]])
            displayed = re.findall(r"Expected stdout \(exit \d\):\n\n```json\n(.*?)\n```", text, re.S)
            self.assertEqual([json.loads(s) for s in displayed], [step["output"] for step in card["steps"]])


@unittest.skipUnless(os.environ.get("COVERAGE_TEST_BIN"), "set COVERAGE_TEST_BIN for native evidence")
class NativeCoverageTests(unittest.TestCase):
    def call(self, args):
        return subprocess.run([sys.executable, "-B", str(HERE / "run.py"), "coverage", *map(str, args)],
                              cwd=ROOT, text=True, capture_output=True,
                              env={**os.environ, "ZENO_FCIS_BIN": os.environ["COVERAGE_TEST_BIN"]})

    def test_card_commands_and_exact_outputs(self):
        text = (ROOT / "docs/zal/COVERAGE_CARD.md").read_text()
        card = json.loads(text.removeprefix("```json\n").split("\n```", 1)[0])
        with tempfile.TemporaryDirectory() as tmp:
            for step in card["steps"]:
                args = shlex.split(step["command"].replace("$WORK", tmp))[3:]
                result = self.call(args)
                self.assertEqual(result.returncode, step["exit"], result.stderr)
                self.assertEqual(json.loads(result.stdout), step["output"])
            report = json.loads((Path(tmp) / "counter/coverage.json").read_text())
            first = report["rounds"][0]["kept"][0]
            self.assertEqual(first["id"], "coverage_rejects_only_when_needed")
            self.assertEqual((first["gaps_closed"], first["mutants_rejected"]), (4, 6))

    def test_zal_proposals_keep_binary_findings_and_plain_readings(self):
        with tempfile.TemporaryDirectory() as tmp:
            work = Path(tmp)
            source = HERE / "examples/context_free2.zal"
            factory.write_new(factory.lower_machine(behavior2.parse(source.read_text())), work / "contract")
            proposal = work / "proposals.txt"
            proposal.write_text("require only_running: accepted => state' == running; # targets: state\n"
                                "law 9400 nothing on any = true; # targets: story\n"
                                "require never_accept: !accepted; # targets: authority\n")
            result = self.call([work / "contract", "--zal", source, "--proposals", proposal, "--out", work / "out"])
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            report = json.loads((work / "out/coverage.json").read_text())
            kept = [c for r in report["rounds"] for c in r["kept"] if c["source"] == "proposed"]
            self.assertTrue(any(c["zal"].startswith("require only_running") for c in kept))
            self.assertTrue(all(c["plain"].startswith("Requirement") for c in kept))
            rejected = [c for r in report["rounds"] for c in r["rejected"]]
            self.assertTrue(any(c["id"] == "proposed-line-2" for c in rejected))
            findings = [c for r in report["rounds"] for c in r["disagreeing"]]
            self.assertEqual(len(findings), 1)
            self.assertIsNotNone(findings[0]["witness"])

    def test_idiom_instance_is_checked_and_kept_as_a_separate_source(self):
        with tempfile.TemporaryDirectory() as tmp:
            work = Path(tmp)
            instance = {"schema": "zeno-fcis/design-idiom-instance/1", "idiom": "required-acceptance",
                        "law": "law 9500 reject_only_at_bounds on reject = context.102.130 == 0 || (command.101.120 == 150 && pre.100.110 == 2) || (command.101.120 == 151 && pre.100.110 == 0);",
                        "dimension": "liveness", "source": "required-acceptance library instance"}
            idioms = work / "idioms.json"
            idioms.write_text(json.dumps([instance]))
            result = self.call([ROOT / "examples/guide-counter", "--idioms", idioms, "--out", work / "out"])
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            report = json.loads((work / "out/coverage.json").read_text())
            first = report["rounds"][0]["kept"][0]
            self.assertEqual((first["source"], first["idiom"], first["source_note"]),
                             ("idiom", instance["idiom"], instance["source"]))
            self.assertEqual((first["gaps_closed"], first["mutants_rejected"]), (4, 6))
            self.assertIn("Idiom: required-acceptance", (work / "out/COVERAGE.md").read_text())

    def test_crlf_subject_hash_matches_the_binary_source_hash(self):
        with tempfile.TemporaryDirectory() as tmp:
            work = Path(tmp)
            for name in ("project.zeno", "v2/policy.json", "tests/decision-examples.txt"):
                target = work / "contract" / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes((ROOT / "examples/guide-counter" / name).read_bytes().replace(b"\n", b"\r\n"))
            result = self.call([work / "contract", "--out", work / "out"])
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            report = json.loads((work / "out/coverage.json").read_text())
            native = json.loads((work / "out/packets/0001.stdout").read_text())
            expected = hashlib.sha256((work / "contract/project.zeno").read_bytes()).hexdigest()
            self.assertEqual(report["sources"]["project_sha256"], expected, "report normalized CRLF source bytes")
            self.assertEqual(native["sources"]["project_sha256"], expected)


class ProposalLoweringTests(unittest.TestCase):
    def test_export_and_proposals_share_expression_lowering(self):
        model = behavior2.parse((HERE / "examples/context_free2.zal").read_text())
        kind, formula = model.invariants[0]
        proposed = factory.profile2_proposal(model, "invariant", kind, formula, 501)
        self.assertIn(proposed[0], factory.lower_machine(model)["project.zeno"])

    def test_false_proposal_is_lowered_without_a_python_verdict(self):
        model = behavior2.parse((HERE / "examples/context_free2.zal").read_text())
        with patch.object(behavior2, "check", side_effect=AssertionError("Python verdict forbidden")):
            laws = factory.profile2_proposal(model, "require", "false_proposal", behavior2.Expr("false"), 9000)
        self.assertEqual(len(laws), 2)
        self.assertTrue(all("= false;" in law for law in laws))


if __name__ == "__main__":
    unittest.main()
