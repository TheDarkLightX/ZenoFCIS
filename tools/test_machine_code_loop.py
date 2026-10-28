"""Integration controls for machine-code proposals, search and replay."""

from contextlib import redirect_stdout
import copy
import io
import json
from pathlib import Path
import struct
import tempfile
import unittest
from unittest.mock import patch

from machine_code import model, session, targets
from machine_code.run import main

ROOT = Path(__file__).resolve().parents[1]


class MachineLoopTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="zeno-machine-loop-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.directory = self.root / "session"
        self.problem_path = ROOT / "test-data/machine-code/add64-x86.json"

    def opened(self, problem=None):
        if problem is not None:
            self.problem_path = self.root / "problem.json"
            self.problem_path.write_bytes(model.canonical(problem))
        session.open_session(self.problem_path, self.directory)

    def proposal(self, instructions, target=model.TARGETS[0]):
        code = targets.encode_program(target, instructions)
        path = self.root / "proposal.json"
        path.write_bytes(model.canonical({"schema": "zeno-fcis/machine-proposal/1", "code_hex": code.hex(),
                         "provenance": {"kind": "human", "name": "integration-test", "note": "explicit test proposal"}}))
        return path

    def accepted(self):
        return session.submit(self.directory, self.proposal([["lea", "rax", "rdi", "rsi", 1]]))

    def test_real_counterexample_then_repaired_raw_machine_proposal_replays(self):
        self.opened()
        wrong = session.submit(self.directory, self.proposal([["mov", "rax", "rdi"]]))
        self.assertEqual(wrong["status"], "counterexample", wrong)
        self.assertNotEqual(wrong["verification"]["actual"], wrong["verification"]["expected"])
        self.assertEqual(self.accepted()["status"], "equivalent")
        packet = session.packet(self.directory)
        self.assertEqual([row["verification"]["status"] for row in packet["feedback"]],
                         ["counterexample", "equivalent"])
        replayed = session.resume(self.directory)
        self.assertEqual(replayed["status"], "replayed")
        self.assertEqual(replayed["verification"]["cost"], {"bytes": 5, "instructions": 1})
        self.assertFalse(replayed["global_optimality"])

    def test_search_selects_a_checked_machine_program_without_an_llm_dependency(self):
        self.opened()
        result = session.search(self.directory, max_candidates=156, deadline_ms=8000)
        self.assertEqual(result["status"], "checked-candidate", result)
        self.assertEqual(result["stop_reason"], "grammar-exhausted", result)
        self.assertEqual(result["proposed"], 156)
        self.assertEqual(result["cost"], {"bytes": 5, "instructions": 1})
        self.assertGreater(result["concrete_refutations"], 0)
        self.assertEqual(result["solver_checks"], 1)
        self.assertGreater(result["objective_pruned"], 0)
        self.assertEqual(session.resume(self.directory)["status"], "replayed")

    def test_stored_incumbent_rechecks_once_within_one_checker_budget(self):
        from machine_code import verify
        self.opened(); self.accepted()
        with patch.object(verify, "check", wraps=verify.check) as checker:
            result = session.search(self.directory, max_candidates=1000, max_checks=1, deadline_ms=5000)
        self.assertEqual(result["status"], "checked-candidate", result)
        self.assertEqual(result["solver_checks"], 1)
        self.assertEqual(checker.call_count, 1)
        self.assertEqual(result["cost"], {"bytes": 5, "instructions": 1})

    def test_checked_cost_bound_does_not_prune_a_cheaper_implementation(self):
        self.opened()
        long = session.submit(self.directory, self.proposal([["mov", "rax", "rdi"], ["add", "rax", "rsi"]]))
        self.assertEqual(long["verification"]["cost"]["bytes"], 7)
        result = session.search(self.directory, max_candidates=156, max_checks=8, deadline_ms=8000)
        self.assertEqual(result["status"], "checked-candidate", result)
        self.assertEqual(result["cost"], {"bytes": 5, "instructions": 1})
        self.assertEqual(result["solver_checks"], 2)

    def test_exhausted_budget_is_not_unrealizability(self):
        self.opened()
        result = session.search(self.directory, max_candidates=1)
        self.assertEqual(result["status"], "no-checked-candidate")
        self.assertEqual(result["stop_reason"], "budget-exhausted")
        self.assertIsNone(result["incumbent"])

    def test_emitted_object_contains_the_exact_checked_text_and_replays(self):
        self.opened(); self.accepted()
        output = self.root / "artifact"
        emitted = session.emit(self.directory, output)
        self.assertEqual(emitted["status"], "emitted")
        obj = (output / "kernel.o").read_bytes()
        self.assertEqual(obj[:7], b"\x7fELF\x02\x01\x01")
        self.assertEqual(struct.unpack_from("<H", obj, 18)[0], 62)
        section = struct.unpack_from("<Q", obj, 40)[0] + 64
        offset, size = struct.unpack_from("<QQ", obj, section + 24)
        self.assertEqual(obj[offset:offset + size], (output / "kernel.bin").read_bytes())
        self.assertEqual(session.replay_artifact(output)["status"], "replayed")

    def test_rv64i_has_actual_machine_bytes_and_a_riscv_object(self):
        problem = model.read_json(ROOT / "test-data/machine-code/add64-rv64i.json")
        self.opened(problem)
        result = session.submit(self.directory, self.proposal([["add", "a0", "a0", "a1"]], model.TARGETS[1]))
        self.assertEqual(result["status"], "equivalent", result)
        out = self.root / "rv"
        session.emit(self.directory, out)
        self.assertEqual((out / "kernel.bin").read_bytes().hex(), "3305b50067800000")
        self.assertEqual(struct.unpack_from("<H", (out / "kernel.o").read_bytes(), 18)[0], 243)
        self.assertEqual(session.replay_artifact(out)["status"], "replayed")

    def test_problem_bytecode_event_and_dependency_drift_refuse(self):
        self.opened(); self.accepted()
        code_path = self.directory / "0001.bin"
        original = code_path.read_bytes()
        code_path.write_bytes(original[:-1] + b"\x90")
        with self.assertRaisesRegex(ValueError, "artifact identity"):
            session.resume(self.directory)
        code_path.write_bytes(original)
        current = session.identity(); changed = copy.deepcopy(current)
        changed["sources"]["verify.py"] = "0" * 64
        with patch.object(session, "identity", return_value=changed):
            with self.assertRaisesRegex(ValueError, "binding drifted"):
                session.resume(self.directory)
        with (self.directory / "events.jsonl").open("ab") as stream:
            stream.write(b'{"incomplete":')
        with self.assertRaisesRegex(ValueError, "incomplete session event"):
            session.resume(self.directory)

    def test_hash_consistent_forged_cost_is_still_rechecked(self):
        self.opened(); self.accepted()
        path = self.directory / "0001.receipt.json"
        receipt = model.read_json(path)
        receipt["verification"]["cost"]["bytes"] = 1
        path.write_bytes(model.canonical(receipt) + b"\n")
        ledger = self.directory / "events.jsonl"
        event = json.loads(ledger.read_bytes())
        event["receipt_sha256"] = session.file_hash(path)
        event.pop("hash")
        event["hash"] = model.digest(model.canonical(event))
        ledger.write_bytes(model.canonical(event) + b"\n")
        with self.assertRaisesRegex(ValueError, "fresh byte interpretation"):
            session.resume(self.directory)

    def test_search_cannot_promote_a_resealed_false_stored_status(self):
        self.opened()
        session.submit(self.directory, self.proposal([["mov", "rax", "rdi"]]))
        path = self.directory / "0001.receipt.json"
        receipt = model.read_json(path); receipt["verification"]["status"] = "equivalent"
        path.write_bytes(model.canonical(receipt) + b"\n")
        ledger = self.directory / "events.jsonl"; event = json.loads(ledger.read_bytes())
        event["status"] = "equivalent"; event["receipt_sha256"] = session.file_hash(path)
        event.pop("hash"); event["hash"] = model.digest(model.canonical(event))
        ledger.write_bytes(model.canonical(event) + b"\n")
        result = session.search(self.directory, max_candidates=1, max_checks=1, deadline_ms=1)
        self.assertEqual(result["status"], "inconclusive", result)

    def test_resealed_unknown_receipt_schema_and_assurance_refuse(self):
        self.opened(); self.accepted()
        path = self.directory / "0001.receipt.json"; original = model.read_json(path)
        ledger = self.directory / "events.jsonl"; initial_event = json.loads(ledger.read_bytes())
        for key, value in [("schema", "unknown-format"), ("authority", "production"),
                           ("assurance", "FULL_ISA_AND_NATIVE_EXECUTION_VERIFIED")]:
            receipt = copy.deepcopy(original); receipt[key] = value
            path.write_bytes(model.canonical(receipt) + b"\n")
            event = copy.deepcopy(initial_event); event["receipt_sha256"] = session.file_hash(path)
            event.pop("hash"); event["hash"] = model.digest(model.canonical(event))
            ledger.write_bytes(model.canonical(event) + b"\n")
            with self.subTest(field=key), self.assertRaisesRegex(ValueError, "unregistered receipt"):
                session.emit(self.directory, self.root / key)

    def test_artifact_tampering_and_cross_target_relabeling_refuse(self):
        self.opened(); self.accepted()
        out = self.root / "artifact"; session.emit(self.directory, out)
        manifest_path = out / "manifest.json"
        manifest = model.read_json(manifest_path)
        manifest["target"] = model.TARGETS[1]
        manifest_path.write_bytes(model.canonical(manifest))
        with self.assertRaisesRegex(ValueError, "target/object"):
            session.replay_artifact(out)
        manifest["target"] = model.TARGETS[0]
        manifest_path.write_bytes(model.canonical(manifest))
        with (out / "kernel.bin").open("ab") as stream:
            stream.write(b"\x90")
        with self.assertRaisesRegex(ValueError, "artifact bytes"):
            session.replay_artifact(out)

    def test_unsupported_machine_code_is_preserved_as_a_refusal(self):
        self.opened()
        path = self.proposal([["nop"]])
        proposal = model.read_json(path); proposal["code_hex"] = "0f05c3"
        path.write_bytes(model.canonical(proposal))
        self.assertEqual(session.submit(self.directory, path)["status"], "refused")
        self.assertIsNone(session.resume(self.directory)["incumbent"])

    def test_resealed_assembly_substitution_is_not_machine_code_correspondence(self):
        self.opened(); self.accepted()
        out = self.root / "artifact"; session.emit(self.directory, out)
        path = out / "kernel.S"
        path.write_text(".text\n.globl zeno_kernel\nzeno_kernel: ret\n")
        manifest = model.read_json(out / "manifest.json")
        manifest["files"]["kernel.S"] = {"sha256": session.file_hash(path), "bytes": path.stat().st_size}
        (out / "manifest.json").write_bytes(model.canonical(manifest))
        with self.assertRaisesRegex(ValueError, "assembly source correspondence"):
            session.replay_artifact(out)

    def test_artifact_metadata_rejects_boolean_and_float_integer_aliases(self):
        self.opened(); self.accepted()
        out = self.root / "artifact"; session.emit(self.directory, out)
        path = out / "manifest.json"; original = model.read_json(path)
        for value in (True, 1.0):
            manifest = copy.deepcopy(original); manifest["cost"]["instructions"] = value
            path.write_bytes(model.canonical(manifest))
            with self.subTest(value=value), self.assertRaisesRegex(ValueError, "must be an integer"):
                session.replay_artifact(out)
        manifest = copy.deepcopy(original)
        manifest["files"]["kernel.bin"]["bytes"] = float(manifest["files"]["kernel.bin"]["bytes"])
        path.write_bytes(model.canonical(manifest))
        with self.assertRaisesRegex(ValueError, "must be an integer"):
            session.replay_artifact(out)

    def test_open_session_has_no_incumbent_and_refuses_overwrite(self):
        self.opened()
        self.assertEqual(session.resume(self.directory)["status"], "open")
        with self.assertRaises(FileExistsError):
            session.open_session(self.problem_path, self.directory)

    def test_lock_and_uncommitted_artifacts_refuse_without_deleting_them(self):
        self.opened()
        lock = self.directory / "lock"; lock.write_text("retained")
        with self.assertRaises(FileExistsError):
            self.accepted()
        self.assertEqual(lock.read_text(), "retained")
        lock.unlink()
        orphan = self.directory / "0001.bin"; orphan.write_bytes(b"\xc3")
        with self.assertRaisesRegex(ValueError, "uncommitted"):
            session.resume(self.directory)
        self.assertEqual(orphan.read_bytes(), b"\xc3")

    def test_duplicate_json_and_noncanonical_word_and_bool_limits_refuse(self):
        path = self.root / "bad.json"; path.write_text('{"x":1,"x":2}')
        with self.assertRaisesRegex(ValueError, "duplicate"):
            model.read_json(path)
        for word in ("01", "-1", str(1 << 64), True, 0):
            with self.subTest(word=word), self.assertRaises(ValueError):
                model.word(word)
        problem = model.read_json(self.problem_path); problem["max_instructions"] = True
        with self.assertRaises(ValueError):
            model.validate_problem(problem)

    def test_width_reference_covers_carry_and_signed_boundary(self):
        expression = ["add", ["input", "x"], ["input", "y"]]
        self.assertEqual(model.evaluate(expression, model.MASK, 1), 0)
        self.assertEqual(model.evaluate(expression, 1 << 63, 1 << 63), 0)
        self.assertEqual(model.evaluate(["lshr", ["input", "x"], 63], 1 << 63, 0), 1)

    def test_a_file_growing_after_metadata_checks_cannot_force_unbounded_reads(self):
        path = self.root / "growing.json"; path.write_bytes(b"0")
        received = []
        class GrowingStream(io.BytesIO):
            def read(self, size=-1):
                data = super().read(size); received.append(len(data)); return data
        stream = GrowingStream(b" " * (2 << 20))
        with patch.object(Path, "open", return_value=stream):
            with self.assertRaisesRegex(ValueError, "grew beyond"):
                model.read_json(path)
        self.assertLessEqual(sum(received), model.MAX_JSON_BYTES + 1)

    def test_cli_reports_unknown_budget_separately_from_success(self):
        with redirect_stdout(io.StringIO()) as output:
            result = main(["open", "--problem", str(self.problem_path), "--session", str(self.directory)])
        self.assertEqual(result, 0)
        self.assertEqual(json.loads(output.getvalue())["status"], "opened")
        with redirect_stdout(io.StringIO()) as output:
            result = main(["search", "--session", str(self.directory), "--max-candidates", "1"])
        self.assertEqual(result, 2)
        self.assertEqual(json.loads(output.getvalue())["status"], "no-checked-candidate")


if __name__ == "__main__":
    unittest.main()
