"""Direct handler checks for the transform-loop tools, standard library only.

The pinned MCP SDK is not required: these tests call the handler functions the
server registers, playing the agent (a fake client) that drives one session
from request to candidates to replay. Set ZENO_FCIS_CLI to the built CLI, or
build it first so target/debug/zeno-fcis exists.
"""

import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest

import zeno_fcis_transform as tools


ROOT = Path(__file__).resolve().parents[2]
ARTIFACTS = ROOT / "docs/benchmarks/withdrawal-queue/artifacts"
SWAPPED_KERNEL = {
    "inputs": [{"kind": "Bool"}] * 4,
    "outputs": [{"kind": "Bool"}] * 3,
    "nodes": [["Input", 0], ["Input", 1], ["Input", 2], ["Input", 3], ["Not", 0], ["Not", 2],
              ["And", 4, 5], ["Not", 6], ["Not", 1], ["Not", 3], ["And", 8, 9], ["Not", 10],
              ["And", 6, 10], ["Not", 12]],
    "roots": [11, 7, 13],
}


def locate_cli() -> str:
    """The CLI under test: ZENO_FCIS_CLI, else the debug build of this checkout."""
    configured = os.environ.get("ZENO_FCIS_CLI")
    candidates = [configured] if configured else []
    target = os.environ.get("CARGO_TARGET_DIR")
    if target:
        candidates.append(str(Path(target) / "debug/zeno-fcis"))
    candidates.append(str(ROOT / "target/debug/zeno-fcis"))
    for candidate in candidates:
        if candidate and Path(candidate).is_file():
            return str(Path(candidate).resolve())
    raise AssertionError("no zeno-fcis CLI: set ZENO_FCIS_CLI or build crates/zeno-fcis-cli first")


class TransformToolsTest(unittest.TestCase):
    def setUp(self):
        os.environ["ZENO_FCIS_CLI"] = locate_cli()
        self.temporary = tempfile.mkdtemp(prefix="zenofcis-transform-tools-")
        self.session = str(Path(self.temporary) / "session")

    def tearDown(self):
        shutil.rmtree(self.temporary, ignore_errors=True)

    def test_an_agent_improves_the_kernel_through_the_checker(self):
        opened = tools.transform_request(str(ARTIFACTS / "boolean-kernel-original.zcve"), self.session)
        self.assertEqual(opened["exit_code"], 0)
        self.assertEqual(opened["authority"], "none")
        report = opened["report"]
        self.assertEqual(report["status"], "opened")
        request_id = report["detail"]["request_id"]
        self.assertEqual(len(request_id), 64)
        self.assertEqual(report["detail"]["request"]["policy"]["hosted"], "disabled")
        view = report["detail"]["view"]
        self.assertEqual(view["original"]["nodes"], 16)
        self.assertEqual(len(view["original"]["program"]["nodes"]), 16)

        # A plausible but wrong candidate: a replayed counterexample.
        swapped = Path(self.temporary) / "swapped.json"
        swapped.write_text(json.dumps(SWAPPED_KERNEL), encoding="utf-8")
        wrong = tools.transform_candidate(self.session, candidate_json_path=str(swapped))
        self.assertEqual(wrong["exit_code"], 1)
        self.assertEqual(wrong["report"]["status"], "different")
        feedback = wrong["report"]["detail"]["feedback"]
        self.assertEqual(feedback["kind"], "verified-difference")
        self.assertEqual(feedback["witness"]["ordinal"], 1)
        self.assertEqual(feedback["witness"]["request_id"], request_id)
        self.assertEqual(wrong["report"]["detail"]["status"], "no-checked-improvement")

        # The model-found kernel candidate: a checked improvement, 16 -> 7 nodes.
        good = tools.transform_candidate(self.session,
                                         candidate_path=str(ARTIFACTS / "boolean-kernel-candidate.zcve"))
        self.assertEqual(good["exit_code"], 0)
        self.assertEqual(good["report"]["status"], "checked-improvement")
        detail = good["report"]["detail"]
        self.assertEqual(detail["outcome"]["cost"], {"nodes": 7, "bytes": 763})
        self.assertEqual(detail["incumbent"]["kind"], "checked-replacement")
        self.assertEqual(detail["status"], "best-checked-so-far")
        self.assertEqual(len(detail["incumbent_program"]["nodes"]), 7)
        self.assertEqual(len(detail["all_feedback"]), 2)
        self.assertEqual(detail["accounting"]["model_calls"], 0)

        # Resubmission is a duplicate; a larger equivalent is no improvement.
        again = tools.transform_candidate(self.session,
                                          candidate_path=str(ARTIFACTS / "boolean-kernel-candidate.zcve"))
        self.assertEqual((again["exit_code"], again["report"]["status"]), (1, "duplicate-of-attempt:1"))
        padded = tools.transform_candidate(self.session,
                                           candidate_path=str(ARTIFACTS / "boolean-kernel-padded.zcve"))
        self.assertEqual(padded["exit_code"], 0)
        self.assertEqual(padded["report"]["status"], "equivalent-without-improvement:original-cost-guard")

        # Replay re-admits and re-checks the incumbent before reporting it.
        replayed = tools.transform_replay(self.session)
        self.assertEqual(replayed["exit_code"], 0)
        loop_report = replayed["report"]["detail"]["report"]
        self.assertEqual(loop_report["status"], "best-checked-so-far")
        self.assertEqual(loop_report["incumbent"]["cost"], {"nodes": 7, "bytes": 763})
        self.assertEqual(loop_report["accounting"]["attempts"], 4)
        self.assertGreaterEqual(loop_report["accounting"]["replays"], 1)
        self.assertIn("global optimality", loop_report["claims"]["not_claimed"])

        # A tampered receipt leaves no trusted incumbent.
        receipt = Path(self.session) / "receipts/1.json"
        original = receipt.read_bytes()
        tampered = bytearray(original)
        tampered[20] ^= 1
        receipt.write_bytes(bytes(tampered))
        refused = tools.transform_replay(self.session)
        self.assertEqual(refused["exit_code"], 2)
        self.assertEqual(refused["report"]["status"], "resume-refused")
        self.assertEqual(refused["report"]["detail"]["refusal"]["reason"], "replacement-digest")
        self.assertIsNone(refused["report"]["detail"]["trusted_incumbent"])
        receipt.write_bytes(original)
        self.assertEqual(tools.transform_replay(self.session)["exit_code"], 0)

    def test_refusals_and_closed_arguments(self):
        original = str(ARTIFACTS / "boolean-kernel-original.zcve")
        with self.assertRaises(tools.TransformToolError):
            tools.transform_request(original, self.session, profile="FunctionalBoolV1")
        with self.assertRaises(tools.TransformToolError):
            tools.transform_request(original, self.session, attempts=9)
        with self.assertRaises(tools.TransformToolError):
            tools.transform_request(original, self.session, deadline_ms="20000")
        with self.assertRaises(tools.TransformToolError):
            tools.transform_request(str(Path(self.temporary) / "missing.zcve"), self.session)
        self.assertFalse(Path(self.session).exists())
        # No argument can assert a verdict: the handler has no such parameter.
        with self.assertRaises(TypeError):
            tools.transform_candidate(self.session, candidate_path=original, passed=True)
        with self.assertRaises(tools.TransformToolError):
            tools.transform_candidate(self.session)
        with self.assertRaises(tools.TransformToolError):
            tools.transform_candidate(self.session, candidate_path=original, candidate_json_path=original)
        # A session that does not exist is an I/O refusal from the CLI, not a verdict.
        missing = tools.transform_replay(str(Path(self.temporary) / "absent"))
        self.assertEqual((missing["exit_code"], missing["report"]["status"]), (3, "io-error"))
        # The controller is outside the Boolean profile and inside checked-i64.
        controller = str(ARTIFACTS / "retained-controller-original.zcve")
        outside = tools.transform_request(controller, self.session)
        self.assertEqual((outside["exit_code"], outside["report"]["status"]), (1, "refused"))
        self.assertEqual(outside["report"]["detail"]["reason"], "profile-inputs")
        inside = tools.transform_request(controller, self.session, profile="checked-i64-v1", attempts=2)
        self.assertEqual(inside["exit_code"], 0)
        self.assertEqual(inside["report"]["detail"]["request"]["domain"]["size"], 384)
        improved = tools.transform_candidate(
            self.session, candidate_path=str(ARTIFACTS / "retained-controller-candidate.zcve"))
        self.assertEqual(improved["report"]["status"], "checked-improvement")
        self.assertEqual(improved["report"]["detail"]["outcome"]["cost"]["nodes"], 60)


if __name__ == "__main__":
    unittest.main()
