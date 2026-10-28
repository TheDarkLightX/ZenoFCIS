"""Full bridge controls, including real SMT; use unittest Pure for pure selection.

Pure transport fixtures replace solver identity/checking, never ISA semantics.
Their equivalent verdicts are fixture data and establish no machine correctness.
"""
from __future__ import annotations
import ast
import argparse
import contextlib
import copy
from dataclasses import replace
from dataclasses import FrozenInstanceError
import io
import importlib.util
import json
from pathlib import Path
import sys
import shutil
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "integrations/zal"))
import word_kernel as w

SOURCE = (ROOT / "test-data/zal-word-kernel/add64-x86.zal").read_text("ascii")
ENGLISH = (ROOT / "test-data/zal-word-kernel/add64-x86.english.zal").read_text("ascii")
GOOD = "488d0437c3"
WRONG = "4889f84831f0c3"
DEMO_OUT = None


def proposal(path, code):
    path.write_text(json.dumps({"schema": "zeno-fcis/machine-proposal/1", "code_hex": code,
        "provenance": {"kind": "human", "name": "bridge-control", "note": "test data; no authority"}}), "ascii")
    return path


def fixture_identity():
    return {"fixture": "PURE transport only; no solver qualification", "sources": w.source_identity()}


def fixture_check(problem, code, timeout_ms):
    # Deliberately supplied outcomes, not a second machine interpreter.
    cost = {"bytes": len(code), "instructions": 1 if code.hex() == GOOD else 2}
    if code.hex() == GOOD:
        return {"status": "equivalent", "cost": cost, "scope": "PURE FIXTURE ONLY"}
    return {"status": "counterexample", "reason": "fixture-witness", "cost": cost,
            "inputs": {"x": "1", "y": "1"}, "initial_registers": {k: str(i + 3) for i, k in enumerate(w.mv._SCRATCH[problem["target"]])},
            "expected": "2", "actual": "0", "scope": "PURE FIXTURE ONLY"}


class Pure(unittest.TestCase):
    def setUp(self):
        self.doc = w.parse(SOURCE)
        self.lowered = w.lower(self.doc)
        self.rev = self.lowered["binding"]["revision"]

    def test_english_symbolic_exact_meaning_roundtrip(self):
        eng = w.parse(ENGLISH, "english")
        self.assertEqual(self.doc.kernel, eng.kernel)
        self.assertEqual(self.rev, w.lower(eng)["binding"]["revision"])
        self.assertNotEqual(self.lowered["binding"]["source_sha256"], w.lower(eng)["binding"]["source_sha256"])
        for syntax in ("symbolic", "english"):
            self.assertEqual(self.doc.kernel, w.parse(w.render(self.doc, syntax), syntax).kernel)
        self.assertEqual(set(self.lowered["problem"]), {"schema", "target", "inputs", "expression", "grammar", "max_instructions", "objective"})
        self.assertEqual(self.lowered["problem"]["expression"], ["add", ["input", "x"], ["input", "y"]])

    def test_all_expression_operators_roundtrip(self):
        expressions = ["x", "y", "0", "18446744073709551615", "(not x)", "(shl x 0)", "(lshr y 63)"]
        expressions += [f"({op} x (not y))" for op in ("add", "sub", "mul", "and", "or", "xor")]
        for expr in expressions:
            with self.subTest(expr=expr):
                doc = w.parse(SOURCE.replace("(add x y)", expr))
                for syntax in ("symbolic", "english"):
                    self.assertEqual(doc.kernel, w.parse(w.render(doc, syntax), syntax).kernel)

    def test_rv_profile_reuses_registered_target(self):
        src = SOURCE.replace("x86_64-sysv-register/1", "rv64i-register/1")
        rows = src.splitlines()
        rows[5] = 'palette [["add","a0","a0","a1"],["nop"]];'
        doc = w.parse("\n".join(rows) + "\n")
        self.assertEqual(w.problem(doc.kernel)["target"], "rv64i-register/1")
        self.assertIn("valid ra/link", w.explain(doc, w.lower(doc)["binding"]["revision"], "memory")["answer"])

    def test_exact_source_map_and_immutable_model(self):
        maps = {r["id"]: r for r in self.lowered["binding"]["source_map"]}
        for key, expected in (("expression", "(add x y)"), ("expression/1", "x"), ("expression/2", "y")):
            row = maps[key]
            self.assertEqual(SOURCE[row["start"]:row["end"]], expected)
            self.assertEqual(row["line"], 5)
        changed = w.lower(self.doc)
        changed["problem"]["expression"][0] = "sub"
        self.assertEqual(w.lower(self.doc)["problem"]["expression"][0], "add")
        with self.assertRaises(FrozenInstanceError): self.doc.kernel.name = "other"
        forged = replace(self.doc, kernel=replace(self.doc.kernel, name="other"))
        with self.assertRaisesRegex(ValueError, "exact source parsing"): w.lower(forged)
        crlf = w.lower(w.parse(SOURCE.replace("\n", "\r\n")))
        self.assertEqual(next(r for r in crlf["binding"]["source_map"] if r["id"] == "expression")["line"], 5)

    def test_show_questions_and_max_wrap_are_source_bound(self):
        self.assertEqual(w.question(self.doc, self.rev, "What if x is 18446744073709551615 and y is 1?")["expected"], "0")
        self.assertEqual(w.show(self.doc, self.rev)["problem"], self.lowered["problem"])
        self.assertIn("RET reads", w.question(self.doc, self.rev, "Does it access memory?")["answer"])
        self.assertEqual(len(w.question(self.doc, self.rev, "What can be clobbered?")["answer"]["entry_scratch_universally_quantified"]), 7)
        for text in ("What happens on overflow?", "What are the limits?", "How is a candidate checked?"):
            self.assertEqual(w.question(self.doc, self.rev, text)["revision"], self.rev)
        with self.assertRaises(ValueError): w.question(self.doc, self.rev, "Is this approved?")
        with self.assertRaises(ValueError): w.what_if(self.doc, "0" * 64, "1", "2")
        self.assertEqual(w.question(self.doc, self.rev, "What is observed?")["answer"]["contract"], self.lowered["binding"]["observation_contract"])

    def test_forged_numeric_types_and_mutable_aliases_refuse(self):
        doc = w.parse(SOURCE.replace("candidates=1000", "candidates=1"))
        forged = [replace(doc, kernel=replace(doc.kernel, limits=(value,) + doc.kernel.limits[1:]))
                  for value in (True, 1.0)]
        forged += [replace(doc, kernel=replace(doc.kernel, limits=list(doc.kernel.limits))),
                   replace(doc, kernel=replace(doc.kernel, palette=list(doc.kernel.palette))),
                   replace(doc, spans=list(doc.spans)),
                   replace(doc, spans=((doc.spans[0][0], False, doc.spans[0][2]),) + doc.spans[1:])]
        for value in forged:
            with self.subTest(document=value):
                with self.assertRaisesRegex(ValueError, "exact source parsing"):
                    w.lower(value)
                with self.assertRaisesRegex(ValueError, "exact source parsing"):
                    w.render(value, "english")

    def test_persisted_binding_numeric_alias_refuses(self):
        source = SOURCE.replace("candidates=1000", "candidates=1")
        rev = w.lower(w.parse(source))["binding"]["revision"]
        with tempfile.TemporaryDirectory(prefix="zal-word-binding-types-") as tmp, patch.object(w.ms, "identity", fixture_identity):
            session = Path(tmp) / "session"
            w.open_session(source, "symbolic", session, rev)
            binding = w.read_json(session / "binding.json")
            binding["loop_limits"]["max_candidates"] = True
            (session / "binding.json").write_bytes(w.canon(binding) + b"\n")
            with self.assertRaisesRegex(ValueError, "binding changed"):
                w.load_bound(session, rev)

    def test_invalid_directory_inspection_is_bounded(self):
        for artifact, cap in ((False, 4), (True, 5)):
            visited = []
            class Directory:
                def iterdir(self):
                    for index in range(1000):
                        visited.append(index)
                        yield Path("junk-" + str(index))
            with self.subTest(artifact=artifact), patch.object(w, "ordinary", return_value=Directory()):
                with self.assertRaisesRegex(ValueError, "unexpected files"):
                    w.load_bound(Path("unused"), self.rev, artifact=artifact)
            self.assertEqual(len(visited), cap)

    def test_unsupported_semantics_and_malformed_source_refuse(self):
        edits = [("modulo 2^64", "checked"), ("x:u64", "x:u256"), ("inputs x:u64,y:u64;", "inputs x:u64,y:u64,z:u64;"),
                 ("(add x y)", "(load x)"), ("(add x y)", "(add x 18446744073709551616)"),
                 ("(add x y)", "(shl x 64)"), ("(add x y)", "(add x 01)"),
                 ("(add x y)", "(add x y) x"), ("(add x y)", "("),
                 ("checks=16", "checks=129"), ("max-body 3", "max-body 17"), ("kernel add64", "kernel x;\neffect memory"),
                 ('["xor","rax","rsi"]', '["mov","rax","rsp"]'), ('["xor","rax","rsi"]', '["nop",true]'),
                 ('["xor","rax","rsi"]', '["lea","rax","rdi","rsi",1.0]'),
                 ('["xor","rax","rsi"]', '["add","rax","rsi"]')]
        for before, after in edits:
            with self.subTest(after=after), self.assertRaises(ValueError): w.parse(SOURCE.replace(before, after))
        for source in (SOURCE + "effect memory;\n", SOURCE + "within 10 steps;\n", SOURCE + "next state;\n", SOURCE + " " * 65536, SOURCE.replace("add64", "aé"), SOURCE.replace("\n", "\r"), SOURCE + "\x00"):
            with self.assertRaises(ValueError): w.parse(source)
        with self.assertRaises(ValueError): w.parse(ENGLISH.replace("(x plus y)", "("), "english")

    def test_expression_depth_and_node_caps(self):
        expr = "x"
        for _ in range(17): expr = "(not " + expr + ")"
        with self.assertRaises(ValueError): w.parse(SOURCE.replace("(add x y)", expr))
        expr = "x"
        for _ in range(6): expr = "(add " + expr + " " + expr + ")"
        with self.assertRaises(ValueError): w.parse(SOURCE.replace("(add x y)", expr))

    def test_revision_changes_with_meaning_abi_palette_limits_name(self):
        for before, after in (("(add x y)", "(sub x y)"), ("add64", "other"), ("checks=16", "checks=15"),
                              ("max-body 3", "max-body 2"), ("objective bytes", "objective instructions")):
            self.assertNotEqual(w.lower(w.parse(SOURCE.replace(before, after)))["binding"]["revision"], self.rev)

    def test_question_counterexample_completeness_and_consistency(self):
        veri = fixture_check(self.lowered["problem"], bytes.fromhex(WRONG), 1)
        reply = w.feedback(self.doc, self.lowered, {"status": "counterexample", "verification": veri})
        self.assertEqual(reply["human_question"]["inputs"], {"x": "1", "y": "1"})
        self.assertEqual(len(reply["human_question"]["entry_scratch"]), 7)
        for mutate in (lambda x: x["initial_registers"].pop("rax"), lambda x: x.update(expected="3"), lambda x: x.update(actual="2")):
            bad = copy.deepcopy(veri); mutate(bad)
            with self.assertRaises(ValueError): w.feedback(self.doc, self.lowered, {"status": "counterexample", "verification": bad})

    def test_pure_transport_wrong_correct_packet_emit_replay(self):
        with tempfile.TemporaryDirectory(prefix="zal-word-pure-") as tmp, patch.object(w.ms, "identity", fixture_identity), patch.object(w.mv, "check", fixture_check):
            base = Path(tmp); session = base / "session"; artifact = base / "artifact"
            self.assertEqual(w.open_session(SOURCE, "symbolic", session, self.rev)["status"], "opened")
            bad = w.candidate(session, self.rev, proposal(base / "bad.json", WRONG))
            self.assertEqual(bad["status"], "counterexample")
            self.assertEqual(w.packet(session, self.rev)["human_feedback"][0]["human_question"], bad["human_question"])
            self.assertEqual(w.candidate(session, self.rev, proposal(base / "good.json", GOOD))["status"], "equivalent")
            self.assertEqual(w.emit(session, self.rev, artifact)["status"], "emitted")
            self.assertEqual(w.replay(artifact, self.rev)["status"], "replayed")
            self.assertEqual({p.name for p in (artifact / "machine").iterdir()}, {"kernel.bin", "kernel.o", "kernel.S", "problem.json", "receipt.json", "manifest.json"})
            with self.assertRaises(ValueError): w.open_session(SOURCE, "symbolic", session, self.rev)

    def test_edited_meaning_rehashed_wrapper_cannot_reuse_machine_artifact(self):
        with tempfile.TemporaryDirectory(prefix="zal-word-attack-") as tmp, patch.object(w.ms, "identity", fixture_identity), patch.object(w.mv, "check", fixture_check):
            base = Path(tmp); session = base / "session"; artifact = base / "artifact"
            w.open_session(SOURCE, "symbolic", session, self.rev)
            w.candidate(session, self.rev, proposal(base / "good.json", GOOD)); w.emit(session, self.rev, artifact)
            # The attacker updates every wrapper hash/revision after changing the human meaning.
            changed = SOURCE.replace("(add x y)", "(sub x y)")
            bound = w.lower(w.parse(changed))["binding"]
            (artifact / "source.zal").write_text(changed, "ascii")
            (artifact / "binding.json").write_bytes(w.canon(bound) + b"\n")
            manifest = w.read_json(artifact / "manifest.json")
            manifest.update(revision=bound["revision"], source_sha256=bound["source_sha256"], problem_sha256=bound["problem_sha256"])
            manifest["files"] = {name: {"sha256": w.digest(w.read_bytes(artifact / name)), "bytes": (artifact / name).stat().st_size} for name in ("source.zal", "binding.json")}
            (artifact / "manifest.json").write_bytes(w.canon(manifest) + b"\n")
            with self.assertRaisesRegex(ValueError, "exact ZAL lowering"): w.replay(artifact, bound["revision"])

    def test_stale_source_bindings_extra_files_and_symlinks_refuse(self):
        with tempfile.TemporaryDirectory(prefix="zal-word-bound-") as tmp, patch.object(w.ms, "identity", fixture_identity):
            base = Path(tmp); session = base / "session"
            with self.assertRaises(ValueError): w.open_session(SOURCE, "symbolic", session, "0" * 64)
            self.assertFalse(session.exists())
            w.open_session(SOURCE, "symbolic", session, self.rev)
            (session / "extra").write_text("x")
            with self.assertRaises(ValueError): w.packet(session, self.rev)
            (session / "extra").unlink()
            data = w.read_json(session / "binding.json"); data["bridge_sha256"] = "0" * 64
            (session / "binding.json").write_bytes(w.canon(data))
            with self.assertRaises(ValueError): w.packet(session, self.rev)
            (base / "alias").symlink_to(session, target_is_directory=True)
            with self.assertRaises(ValueError): w.load_bound(base / "alias", self.rev)
            with self.assertRaises(ValueError): w.new_directory(ROOT / "source-write-refused")
            with self.assertRaises(ValueError): w.new_directory(base / "alias" / "new")

    def test_search_exact_declared_budget_and_unknown_preserved(self):
        with tempfile.TemporaryDirectory(prefix="zal-word-search-") as tmp, patch.object(w.ms, "identity", fixture_identity):
            session = Path(tmp) / "session"; w.open_session(SOURCE, "symbolic", session, self.rev)
            with patch.object(w.ms, "search", return_value={"status": "no-checked-candidate", "stop_reason": "deadline"}) as operator:
                self.assertEqual(w.search(session, self.rev)["status"], "no-checked-candidate")
                operator.assert_called_once_with(session / "machine", max_candidates=1000, max_checks=16, deadline_ms=1000, timeout_ms=200)

    def test_recorded_counterexample_and_unknown_survive_unchanged_incumbent(self):
        with tempfile.TemporaryDirectory(prefix="zal-word-observe-") as tmp, patch.object(w.ms, "identity", fixture_identity), patch.object(w.mv, "check", fixture_check):
            base = Path(tmp); session = base / "session"
            w.open_session(SOURCE, "symbolic", session, self.rev)
            w.candidate(session, self.rev, proposal(base / "good.json", GOOD))
            w.candidate(session, self.rev, proposal(base / "bad.json", WRONG))
            unknown = {"status": "inconclusive", "reason": "fixture solver deadline", "scope": "PURE FIXTURE ONLY"}
            with patch.object(w.mv, "check", return_value=unknown):
                self.assertEqual(w.candidate(session, self.rev, proposal(base / "unknown.json", GOOD))["status"], "inconclusive")
            reply = w.packet(session, self.rev)
            self.assertEqual([x["status"] for x in reply["human_feedback"]], ["equivalent", "counterexample", "inconclusive"])
            self.assertEqual(reply["human_feedback"][2]["machine"]["verification"], unknown)
            self.assertEqual(len((session / "machine/events.jsonl").read_text().splitlines()), 3)

    def test_cli_read_show_and_stale_refusal_without_solver(self):
        source = ROOT / "test-data/zal-word-kernel/add64-x86.zal"
        for command, rest in (("read", []), ("show", ["--revision", self.rev]), ("lower", ["--revision", self.rev]),
                              ("what-if", ["--revision", self.rev, "--x", "18446744073709551615", "--y", "1"])):
            with self.subTest(command=command), contextlib.redirect_stdout(io.StringIO()) as output:
                self.assertEqual(w.main([command, "--source", str(source)] + rest), 0)
                self.assertEqual(json.loads(output.getvalue())["authority"], "none")
        with contextlib.redirect_stdout(io.StringIO()) as output:
            self.assertEqual(w.main(["show", "--source", str(source), "--revision", "0" * 64]), 65)
        self.assertEqual(json.loads(output.getvalue())["status"], "refused")

    def test_cli_early_dispatch_preserves_existing_functions(self):
        candidate = ast.parse((ROOT / "integrations/zal/run.py").read_text())
        guards = [n for n in candidate.body if isinstance(n, ast.If)]
        self.assertIn("word-kernel", ast.unparse(guards[0]))
        self.assertLess(candidate.body.index(guards[0]), next(i for i, n in enumerate(candidate.body) if isinstance(n, ast.Import) and any(a.name == "behavior" for a in n.names)))

    def test_cli_discovery_and_imported_main_dispatch(self):
        spec = importlib.util.spec_from_file_location("zal_word_frontdoor", ROOT / "integrations/zal/run.py")
        entry = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(entry)
        for argv in (["--help"], ["word-kernel", "--help"]):
            with self.subTest(argv=argv), contextlib.redirect_stdout(io.StringIO()) as output, self.assertRaises(SystemExit) as stopped:
                entry.main(argv)
            self.assertEqual(stopped.exception.code, 0)
            self.assertIn("word-kernel", output.getvalue())
        with contextlib.redirect_stdout(io.StringIO()) as output:
            self.assertEqual(entry.main(["word-kernel", "read", "--source", str(ROOT / "test-data/zal-word-kernel/add64-x86.zal")]), 0)
        self.assertEqual(json.loads(output.getvalue())["binding"]["revision"], self.rev)
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as stopped:
            entry.main(["word-kernel", "--unsupported"])
        self.assertEqual(stopped.exception.code, 2)


class WithSMT(unittest.TestCase):
    """Explicit gate; no skip-as-pass when Z3 or its bound native inputs are absent."""
    def test_actual_x86_source_cex_repair_emit_replay(self):
        doc = w.parse(SOURCE); rev = w.lower(doc)["binding"]["revision"]
        context = tempfile.TemporaryDirectory(prefix="zal-word-smt-") if DEMO_OUT is None else contextlib.nullcontext(str(w.new_directory(DEMO_OUT)))
        with context as tmp:
            base = Path(tmp); session = base / "session"; artifact = base / "artifact"
            def record(name, result):
                w.write_json(base / (name + ".json"), result)
                return result
            record("01-read", w.lower(doc))
            record("02-show", w.show(doc, rev))
            record("03-overflow-question", w.question(doc, rev, "What happens on overflow?"))
            record("04-max-plus-one", w.what_if(doc, rev, "18446744073709551615", "1"))
            record("05-open", w.open_session(SOURCE, "symbolic", session, rev))
            bad = w.candidate(session, rev, proposal(base / "bad.json", WRONG))
            record("06-counterexample", bad)
            self.assertEqual(bad["status"], "counterexample")
            self.assertEqual(set(bad["human_question"]["entry_scratch"]), set(w.mv._SCRATCH[doc.kernel.target]))
            record("07-packet", w.packet(session, rev))
            self.assertEqual(record("08-search", w.search(session, rev))["status"], "checked-candidate")
            self.assertEqual(record("09-correct-candidate", w.candidate(session, rev, proposal(base / "good.json", GOOD)))["status"], "equivalent")
            self.assertEqual(record("10-emit", w.emit(session, rev, artifact))["status"], "emitted")
            self.assertEqual(record("11-replay", w.replay(artifact, rev))["status"], "replayed")
            # Change human meaning AND rehash all wrapper/nested problem/receipt metadata.
            # Only a fresh semantic check prevents the old sum bytes claiming subtraction.
            attack = base / "artifact-meaning-attack"; shutil.copytree(artifact, attack)
            changed = SOURCE.replace("(add x y)", "(sub x y)")
            lowered = w.lower(w.parse(changed)); binding = lowered["binding"]
            (attack / "source.zal").write_text(changed, "ascii")
            (attack / "binding.json").write_bytes(w.canon(binding) + b"\n")
            (attack / "machine/problem.json").write_bytes(w.canon(lowered["problem"]) + b"\n")
            receipt = w.read_json(attack / "machine/receipt.json"); receipt["problem_sha256"] = binding["problem_sha256"]
            (attack / "machine/receipt.json").write_bytes(w.canon(receipt) + b"\n")
            machine = w.read_json(attack / "machine/manifest.json"); machine["problem_sha256"] = binding["problem_sha256"]
            for name in ("problem.json", "receipt.json"):
                path = attack / "machine" / name
                machine["files"][name] = {"sha256": w.digest(path.read_bytes()), "bytes": path.stat().st_size}
            (attack / "machine/manifest.json").write_bytes(w.canon(machine) + b"\n")
            outer = w.read_json(attack / "manifest.json")
            outer.update(revision=binding["revision"], source_sha256=binding["source_sha256"], problem_sha256=binding["problem_sha256"], machine_manifest_sha256=w.digest((attack / "machine/manifest.json").read_bytes()))
            for name in ("source.zal", "binding.json"):
                outer["files"][name] = {"sha256": w.digest((attack / name).read_bytes()), "bytes": (attack / name).stat().st_size}
            (attack / "manifest.json").write_bytes(w.canon(outer) + b"\n")
            refused = record("12-rehashed-meaning-attack", w.replay(attack, binding["revision"]))
            self.assertEqual(refused["status"], "inconclusive")
            self.assertEqual(refused["machine"]["verification"]["status"], "counterexample")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--demo-out", type=Path)
    args, remaining = parser.parse_known_args()
    DEMO_OUT = args.demo_out
    sys.argv = [sys.argv[0], *remaining]
    unittest.main(verbosity=2)
