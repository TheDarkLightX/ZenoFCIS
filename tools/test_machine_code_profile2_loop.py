"""Integration controls for profile-2 machine-code proposals, search, emission and replay."""

from contextlib import redirect_stdout
import io
import json
from pathlib import Path
import struct
import tempfile
import time
import unittest
from unittest.mock import patch

from machine_code import footprint, model, profile2, session, targets, verify2
from machine_code.run import main

ROOT = Path(__file__).resolve().parents[1]
SKIPPED = ("dead_pruned", "order_pruned", "duplicate_pruned", "lost_pruned", "bound_skipped")
DATA = ROOT / "test-data/machine-code"
X86, RV64 = profile2.TARGETS


class Profile2LoopTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="zeno-machine-loop2-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.directory = self.root / "session"

    def opened(self, name=None, problem=None):
        path = DATA / name if name else self.root / "problem.json"
        if problem is not None:
            path.write_bytes(model.canonical(problem))
        session.open_session(path, self.directory)
        return model.read_json(path)

    def proposal(self, instructions, target=X86):
        code = targets.encode_program(target, instructions)
        path = self.root / "proposal.json"
        path.write_bytes(model.canonical({"schema": "zeno-fcis/machine-proposal/1", "code_hex": code.hex(),
                         "provenance": {"kind": "human", "name": "integration-test", "note": "explicit test proposal"}}))
        return path

    def test_lost_carry_counterexample_then_adc_repair_replays(self):
        self.opened("add128-x86.json")
        wrong = session.submit(self.directory, self.proposal(
            [["mov", "rax", "rdi"], ["add", "rax", "rdx"], ["mov", "rdx", "rsi"], ["add", "rdx", "rcx"]]))
        self.assertEqual(wrong["status"], "counterexample", wrong)
        verification = wrong["verification"]
        self.assertEqual(set(verification["inputs"]), {"a_lo", "a_hi", "b_lo", "b_hi"})
        self.assertEqual(set(verification["initial_flags"]), {"cf", "zf", "sf", "of"})
        self.assertEqual(verification["expected"][0], verification["actual"][0])
        self.assertNotEqual(verification["expected"][1], verification["actual"][1])
        repaired = session.submit(self.directory, self.proposal(
            [["mov", "rax", "rdi"], ["add", "rax", "rdx"], ["mov", "rdx", "rsi"], ["adc", "rdx", "rcx"]]))
        self.assertEqual(repaired["status"], "equivalent", repaired)
        packet = session.packet(self.directory)
        self.assertEqual(packet["problem"]["schema"], profile2.SCHEMA)
        self.assertEqual([row["verification"]["status"] for row in packet["feedback"]], ["counterexample", "equivalent"])
        diagnosis = packet["feedback"][0]["diagnosis"]
        self.assertEqual((diagnosis["available"], diagnosis["authority"]), (True, "none"))
        self.assertEqual(diagnosis["first_missing_subterm"],
                         {"output": 1, "expression": "(ult (add a_lo b_lo) a_lo)", "value": "1"})
        generic = diagnosis["generic_failure"]
        self.assertEqual([row["register"] for row in generic["results"]], ["rax", "rdx"])
        self.assertEqual(generic["results"][0]["expected"], generic["results"][0]["actual"])
        self.assertEqual(int(generic["results"][1]["expected"]) - int(generic["results"][1]["actual"]), 1)
        self.assertEqual([row["instruction"] for row in generic["trace"]],
                         ["mov rax, rdi", "add rax, rdx", "mov rdx, rsi", "add rdx, rcx"])
        self.assertEqual(generic["trace"][1]["flags"]["cf"], 1, "the lost carry is visible in the trace")
        self.assertNotIn("diagnosis", packet["feedback"][1])
        replayed = session.resume(self.directory)
        self.assertEqual((replayed["status"], replayed["code_hex"]), ("replayed", "4889f84801d04889f24811cac3"))
        self.assertFalse(replayed["global_optimality"])

    def test_search_finds_the_carry_chain_and_an_unsigned_minimum(self):
        self.opened("add128-x86.json")
        result = session.search(self.directory, max_candidates=5000, max_checks=8, deadline_ms=20000, prune=False)
        self.assertEqual((result["status"], result["stop_reason"]), ("checked-candidate", "grammar-exhausted"), result)
        self.assertEqual((result["proposed"], result["cost"]), (1555, {"bytes": 13, "instructions": 4}))
        self.assertGreater(result["concrete_refutations"], 1000)
        self.assertEqual(session.resume(self.directory)["code_hex"], "4889f84801d04889f24811cac3")
        self.directory = self.root / "pruned"
        self.opened("add128-x86.json")
        result = session.search(self.directory, max_candidates=5000, max_checks=8, deadline_ms=20000)
        self.assertEqual((result["status"], result["stop_reason"]), ("checked-candidate", "grammar-exhausted"), result)
        self.assertEqual(result["proposed"] + sum(result[key] for key in SKIPPED), 1555)
        self.assertLess(result["proposed"], 1555 // 4)
        self.assertGreater(result["order_pruned"], 0)
        self.assertEqual(result["cost"], {"bytes": 13, "instructions": 4})
        self.assertEqual(session.resume(self.directory)["code_hex"], "4889f84801d04889f24811cac3")
        self.directory = self.root / "umin"
        self.opened("umin-x86.json")
        result = session.search(self.directory, max_candidates=5000, max_checks=8, deadline_ms=20000)
        self.assertEqual(result["status"], "checked-candidate", result)
        self.assertEqual(result["cost"], {"bytes": 11, "instructions": 3})
        self.assertEqual(session.resume(self.directory)["code_hex"], "4839f74889f0480f42c7c3")

    def test_split_search_joins_independent_parts_and_bounds_the_joint_search(self):
        self.opened("minmax-x86.json")
        split = session.split_search(self.directory, max_candidates=5000, max_checks=8, deadline_ms=20000)
        self.assertEqual((split["status"], split["stop_reason"]), ("checked-candidate", "joined"), split)
        self.assertEqual((split["cost"]["instructions"], split["solver_checks"]), (6, 1))
        proposal = json.loads((self.directory / "0001.proposal.json").read_text())
        self.assertEqual(proposal["provenance"]["name"], "split-search/1")
        joint = session.search(self.directory, max_candidates=5000, max_checks=8, deadline_ms=20000)
        self.assertEqual((joint["status"], joint["stop_reason"]), ("checked-candidate", "grammar-exhausted"), joint)
        self.assertEqual(joint["cost"]["instructions"], 5, "the shared comparison beats the joined parts")
        self.assertGreater(joint["bound_skipped"], 0)
        self.assertEqual(session.resume(self.directory)["status"], "replayed")

    def test_pruned_search_and_split_search_keep_their_deadline_with_no_survivors(self):
        # No palette entry writes a result register, so every prefix is skipped and nothing reaches the caller.
        x, y = ["input", "x"], ["input", "y"]
        self.opened(problem={"schema": profile2.SCHEMA, "target": X86, "inputs": ["x", "y"], "outputs": [x, y],
                             "grammar": [["add", "r10", "rdi"], ["sub", "r10", "rsi"], ["xor", "r10", "r8"]],
                             "max_instructions": 16, "objective": "instructions"})
        for run in (session.search, session.split_search):
            began = time.monotonic()
            result = run(self.directory, max_candidates=100_000, max_checks=8, deadline_ms=500)
            self.assertLess(time.monotonic() - began, 5, run.__name__)
            self.assertEqual((result["status"], result["stop_reason"]), ("no-checked-candidate", "budget-exhausted"))

    def test_search_does_not_start_a_check_after_filtering_outlasts_the_deadline(self):
        x = ["input", "x"]
        self.opened(problem={"schema": profile2.SCHEMA, "target": X86, "inputs": ["x"], "outputs": [x],
                             "grammar": [["mov", "rax", "rdi"]], "max_instructions": 1,
                             "objective": "instructions"})
        clock = [1000.0]
        agrees = session._agrees

        def slow(problem, code, witness):  # the real filter, on a clock that passes the deadline meanwhile
            clock[0] += 2.0
            return agrees(problem, code, witness)
        with patch.object(session.time, "monotonic", lambda: clock[0]), patch.object(session, "_agrees", slow):
            result = session.search(self.directory, max_candidates=10, max_checks=8, deadline_ms=1000)
        self.assertEqual((result["solver_checks"], result["stop_reason"]), (0, "budget-exhausted"), result)

    def test_split_search_and_search_reuse_counterexamples_from_earlier_runs(self):
        x, y = ["input", "x"], ["input", "y"]
        self.opened(problem={"schema": profile2.SCHEMA, "target": X86, "inputs": ["x", "y"],
                             "outputs": [["eq", x, ["const", "2"]], y],
                             "grammar": [["xor", "rax", "rax"], ["mov", "rdx", "rsi"]],
                             "max_instructions": 2, "objective": "instructions"})
        first = session.split_search(self.directory, max_checks=1, deadline_ms=20000)
        self.assertEqual(first["checker_verdicts"][0]["status"], "counterexample")
        second = session.split_search(self.directory, max_checks=1, deadline_ms=20000)
        self.assertEqual((second["solver_checks"], second["retained_counterexamples"]), (0, 1),
                         "the retained failure filters the same join without a solver call")
        again = session.search(self.directory, max_checks=1, deadline_ms=20000)
        self.assertEqual(again["solver_checks"], 0)

    def test_split_search_refuses_one_result_problems_and_unsafe_joins(self):
        self.opened("umin-x86.json")
        self.assertEqual(session.split_search(self.directory)["status"], "not-applicable")
        with redirect_stdout(io.StringIO()):
            self.assertEqual(main(["split-search", "--session", str(self.directory)]), 1)
        problem = json.loads((DATA / "minmax-x86.json").read_text())
        prints = footprint.footprints(problem)
        index = {tuple(form): i for i, form in enumerate(problem["grammar"])}
        umin = (index[("mov", "rax", "rdi")], index[("cmp", "rdi", "rsi")], index[("cmova", "rax", "rsi")])
        umax = (index[("mov", "rdx", "rsi")], index[("cmp", "rdi", "rsi")], index[("cmova", "rdx", "rdi")])
        self.assertTrue(session._joinable(prints, umin, umax, "rax"))
        # The second part may not read flags the first wrote, nor write the first part's result.
        self.assertFalse(session._joinable(prints, (index[("cmp", "rdi", "rsi")],), (index[("cmova", "rdx", "rdi")],), "rax"))
        self.assertFalse(session._joinable(prints, umin, (index[("mov", "rax", "rsi")],), "rax"))
        self.assertFalse(session._joinable([None] * len(prints), umin, umax, "rax"))

    def test_search_filters_undefined_flag_reads_without_refusing_the_session(self):
        x, y = ["input", "x"], ["input", "y"]
        self.opened(problem={"schema": profile2.SCHEMA, "target": X86, "inputs": ["x", "y"],
                             "outputs": [["eq", x, y]],
                             "grammar": [["imul", "rax", "rsi"], ["sete", "rax"], ["cmp", "rdi", "rsi"],
                                         ["xor", "rax", "rax"]],
                             "max_instructions": 3, "objective": "instructions"})
        result = session.search(self.directory, max_candidates=100, max_checks=8, deadline_ms=20000)
        self.assertEqual(result["status"], "checked-candidate", result)
        self.assertEqual(result["cost"]["instructions"], 3)
        incumbent = bytes.fromhex(session.resume(self.directory)["code_hex"])
        self.assertEqual(incumbent, targets.encode_program(X86, [["xor", "rax", "rax"], ["cmp", "rdi", "rsi"], ["sete", "rax"]]))

    def test_emitted_two_result_x86_object_contains_the_checked_text_and_replays(self):
        self.opened("add128-x86.json")
        session.submit(self.directory, self.proposal(
            [["mov", "rax", "rdi"], ["add", "rax", "rdx"], ["mov", "rdx", "rsi"], ["adc", "rdx", "rcx"]]))
        out = self.root / "artifact"
        emitted = session.emit(self.directory, out)
        self.assertEqual((emitted["status"], emitted["target"]), ("emitted", X86))
        obj = (out / "kernel.o").read_bytes()
        self.assertEqual(struct.unpack_from("<H", obj, 18)[0], 62)
        section = struct.unpack_from("<Q", obj, 40)[0] + 64
        offset, size = struct.unpack_from("<QQ", obj, section + 24)
        self.assertEqual(obj[offset:offset + size], (out / "kernel.bin").read_bytes())
        self.assertEqual(session.replay_artifact(out)["status"], "replayed")

    def test_rv64i_diagnosis_has_registers_and_results_but_no_flags(self):
        self.opened("add128-rv64i.json")
        session.submit(self.directory, self.proposal(
            [["add", "a0", "a0", "a2"], ["add", "a1", "a1", "a3"]], RV64))
        diagnosis = session.packet(self.directory)["feedback"][0]["diagnosis"]
        self.assertTrue(diagnosis["available"])
        report = diagnosis["generic_failure"]
        self.assertNotIn("flags", report["trace"][0])
        self.assertNotIn("initial_flags", report)
        self.assertEqual(set(report["trace"][0]["registers"]), set(verify2.REGISTERS[RV64]))
        self.assertEqual(diagnosis["first_missing_subterm"]["expression"], "(ult (add a_lo b_lo) a_lo)")

    def test_rv64i_two_result_search_object_and_replay(self):
        self.opened("add128-rv64i.json")
        result = session.search(self.directory, max_candidates=5000, max_checks=8, deadline_ms=20000)
        self.assertEqual(result["status"], "checked-candidate", result)
        out = self.root / "rv"
        session.emit(self.directory, out)
        self.assertEqual((out / "kernel.bin").read_bytes().hex(), "3305c500b332c500b3855500b385d50067800000")
        self.assertEqual(struct.unpack_from("<H", (out / "kernel.o").read_bytes(), 18)[0], 243)
        self.assertEqual(session.replay_artifact(out)["status"], "replayed")

    def test_cross_profile_relabelling_of_problem_or_artifact_refuses(self):
        problem = self.opened("umin-x86.json")
        session.submit(self.directory, self.proposal(
            [["mov", "rax", "rdi"], ["cmp", "rdi", "rsi"], ["cmova", "rax", "rsi"]]))
        out = self.root / "artifact"; session.emit(self.directory, out)
        manifest = model.read_json(out / "manifest.json")
        manifest["target"] = model.TARGETS[0]
        (out / "manifest.json").write_bytes(model.canonical(manifest))
        with self.assertRaisesRegex(ValueError, "target/object"):
            session.replay_artifact(out)
        for index, change in enumerate(({"target": model.TARGETS[0]}, {"schema": "zeno-fcis/machine-problem/1"})):
            path = self.root / f"relabelled{index}.json"
            path.write_bytes(model.canonical({**problem, **change}))
            with self.subTest(change=change), self.assertRaises(ValueError):
                session.open_session(path, self.root / f"relabelled{index}")

    def test_profile_one_sessions_keep_their_own_validator(self):
        self.directory = self.root / "p1"
        session.open_session(DATA / "add64-x86.json", self.directory)
        result = session.submit(self.directory, self.proposal([["adc", "rax", "rsi"]]))
        self.assertEqual(result["status"], "refused", result)
        self.assertEqual(result["verification"]["reason"], "UnsupportedOpcode")

    def test_cli_runs_the_profile2_loop(self):
        with redirect_stdout(io.StringIO()) as output:
            code = main(["open", "--problem", str(DATA / "umin-x86.json"), "--session", str(self.directory)])
        self.assertEqual((code, json.loads(output.getvalue())["status"]), (0, "opened"))
        with redirect_stdout(io.StringIO()) as output:
            code = main(["search", "--session", str(self.directory), "--deadline-ms", "20000"])
        self.assertEqual((code, json.loads(output.getvalue())["status"]), (0, "checked-candidate"))
        out = self.root / "artifact"
        with redirect_stdout(io.StringIO()):
            self.assertEqual(main(["emit", "--session", str(self.directory), "--out", str(out)]), 0)
        with redirect_stdout(io.StringIO()) as output:
            code = main(["replay", "--artifact", str(out)])
        self.assertEqual((code, json.loads(output.getvalue())["status"]), (0, "replayed"))


if __name__ == "__main__":
    unittest.main()
