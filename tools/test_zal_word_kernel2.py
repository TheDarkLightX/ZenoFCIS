"""ZAL word-kernel/2 controls: exact syntax, refusals, library-equal lowering and real profile-2 checking.

`Pure` needs no solver. `WithSMT` runs the real profile-2 checker; a missing Z3 fails it rather than skipping.
"""
from __future__ import annotations

import contextlib
import copy
from dataclasses import FrozenInstanceError, replace
import io
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "integrations/zal"))
import word_kernel as w
import word_kernel2 as w2

DATA = ROOT / "test-data/zal-word-kernel"
LIBRARY = ROOT / "test-data/machine-code/kernels"
EXAMPLES = ("add128-x86", "umin-x86")
ADD128 = (DATA / "add128-x86.zal").read_text("ascii")
UMIN = (DATA / "umin-x86.zal").read_text("ascii")
GOOD_ADD128 = "4889f84801d04889f24811cac3"
MAX = str((1 << 64) - 1)


def source(name, syntax):
    return (DATA / (name + (".zal" if syntax == "symbolic" else ".english.zal"))).read_text("ascii")


def proposal(path, code):
    path.write_text(json.dumps({"schema": "zeno-fcis/machine-proposal/1", "code_hex": code,
        "provenance": {"kind": "human", "name": "bridge-control", "note": "test data; no authority"}}), "ascii")
    return path


def revision(text, syntax="symbolic"):
    return w.lower(w.parse(text, syntax))["binding"]["revision"]


class Pure(unittest.TestCase):
    def test_examples_round_trip_exactly_between_syntaxes(self):
        for name in EXAMPLES:
            with self.subTest(kernel=name):
                symbolic, english = w.parse(source(name, "symbolic")), w.parse(source(name, "english"), "english")
                self.assertIs(type(symbolic.kernel), w2.Kernel2)
                self.assertEqual(symbolic.kernel, english.kernel)
                self.assertEqual(w.render(symbolic, "english"), source(name, "english"))
                self.assertEqual(w.render(english, "symbolic"), source(name, "symbolic"))
                for document in (symbolic, english):
                    for syntax in ("symbolic", "english"):
                        self.assertEqual(w.parse(w.render(document, syntax), syntax).kernel, document.kernel)
                low, eng = w.lower(symbolic)["binding"], w.lower(english)["binding"]
                self.assertEqual(low["revision"], eng["revision"])
                self.assertNotEqual(low["source_sha256"], eng["source_sha256"])
                self.assertEqual((low["profile"], low["schema"]), ("zal/word-kernel/2", "zal/word-kernel-binding/1"))
                self.assertEqual(low["profile_syntax_sha256"], w.digest(w.read_bytes(w2.__file__, 131072)))

    def test_lowering_equals_the_checked_library_problem(self):
        for name in EXAMPLES:
            with self.subTest(kernel=name):
                library = json.loads((LIBRARY / (name + ".json")).read_text())["problem"]
                for syntax in ("symbolic", "english"):
                    self.assertEqual(w.lower(w.parse(source(name, syntax), syntax))["problem"], library)

    def test_every_profile2_constructor_round_trips(self):
        expressions = ["x", "0", MAX, "(not x)", "(shl x 0)", "(lshr y 63)", "(ashr x 7)", "(ite x y 0)",
                       "(ite (ule x y) (ashr x 63) (not y))"]
        expressions += [f"({op} x (not y))" for op in ("add", "sub", "mul", "and", "or", "xor", "ult", "ule", "slt", "sle", "eq")]
        for expression in expressions:
            with self.subTest(expression=expression):
                document = w.parse(UMIN.replace("(ite (ule x y) x y)", expression))
                english = w.render(document, "english")
                self.assertEqual(w.parse(english, "english").kernel, document.kernel)
                self.assertEqual(w.render(w.parse(english, "english"), "symbolic"), w.render(document, "symbolic"))
        english = w.render(w.parse(UMIN.replace("(ite (ule x y) x y)", "(ite (slt x y) (ashr x 3) (eq x y))")), "english")
        self.assertIn("(if (x is below y (signed)) then (x shifted right arithmetically by 3) else (x equals y))", english)

    def test_one_and_six_inputs_and_rv64i_target(self):
        one = UMIN.replace("inputs x:u64,y:u64;", "inputs x:u64;").replace("(ite (ule x y) x y)", "(ite (slt x 0) (sub 0 x) x)")
        document = w.parse(one)
        self.assertIn("Input x is an unsigned 64-bit word.", w.render(document, "english"))
        self.assertEqual(w.parse(w.render(document, "english"), "english").kernel, document.kernel)
        six = UMIN.replace("inputs x:u64,y:u64;", "inputs a:u64,b:u64,c:u64,d:u64,e:u64,f:u64;").replace(
            "(ite (ule x y) x y)", "(add e f)")
        self.assertIn("Inputs a, b, c, d, e and f are", w.render(w.parse(six), "english"))
        rv = (ADD128.replace("x86_64-sysv-register/2", "rv64i-register/2")
              .replace(ADD128.splitlines()[6], 'palette [["add","a0","a0","a2"],["sltu","t0","a0","a2"],'
                       '["add","a1","a1","a3"],["add","a1","a1","t0"],["nop"]];'))
        lowered = w.lower(w.parse(rv))
        self.assertEqual(lowered["problem"], json.loads((LIBRARY / "add128-rv64i.json").read_text())["problem"])

    def test_source_map_covers_every_expression_node(self):
        document = w.parse(ADD128)
        spans = {row["id"]: row for row in w.lower(document)["binding"]["source_map"]}
        for key, text in (("result/2/expression", "(add (add a_hi b_hi) (ult (add a_lo b_lo) a_lo))"),
                          ("result/2/expression/2", "(ult (add a_lo b_lo) a_lo)"),
                          ("result/2/expression/2/1/2", "b_lo"), ("result/1", "result 1 (add a_lo b_lo) modulo 2^64;")):
            self.assertEqual(ADD128[spans[key]["start"]:spans[key]["end"]], text)
        self.assertEqual(spans["result/2/expression"]["line"], 6)
        nodes = [key for key in spans if "/expression" in key]
        self.assertEqual(len(nodes), 3 + 9)
        english = source("add128-x86", "english")
        row = next(r for r in w.lower(w.parse(english, "english"))["binding"]["source_map"]
                   if r["id"] == "result/2/expression/2")
        self.assertEqual(english[row["start"]:row["end"]], "((a_lo plus b_lo) is below a_lo (unsigned))")

    def test_refusals_name_the_declaration_and_reason(self):
        cases = [
            (ADD128.replace("(add a_lo b_lo) modulo", "(add a_lo c_lo) modulo"), r"^result/1: unknown input c_lo$"),
            (ADD128.replace("a_hi:u64,b_lo", "a_lo:u64,b_lo"), r"^inputs: duplicate input name a_lo$"),
            (ADD128.replace("b_hi:u64;", "b_hi:u64,c:u64,d:u64,e:u64;"), r"^inputs: .* 1 to 6 inputs; found 7$"),
            (ADD128.replace("\npalette", "\nresult 3 a_lo modulo 2^64;\npalette"), r"^result: .* one or two results; found 3$"),
            (ADD128.replace("max-body 4;", "result 3 a_lo modulo 2^64;\nmax-body 4;"), r"requires the declarations"),
            (ADD128.replace("(add a_lo b_lo) modulo", "(shl a_lo (ult a_lo b_lo)) modulo"),
             r"^result/1: shift count must be a literal 0 to 63; an expression, such as a comparison result"),
            (ADD128.replace("x86_64-sysv-register/2", "x86_64-sysv-register/1"),
             r"^target: profile zal/word-kernel/2 requires x86_64-sysv-register/2 or rv64i-register/2; found x86_64-sysv-register/1$"),
            (ADD128.replace("result 1 (add", "result 2 (add"), r"^result/1: declaration syntax is unsupported"),
            (UMIN.replace("inputs x:u64,y:u64;", "inputs if:u64,y:u64;").replace("(ule x y) x", "(ule if y) if"),
             r"^inputs: input name 'if' is a reserved expression word$"),
            (UMIN.replace("inputs x:u64,y:u64;", "inputs x:u256,y:u64;"), r"^inputs: inputs are declared as NAME:u64"),
            (UMIN.replace("(ite (ule x y) x y)", "(ite (ule x y) x)"), r"^result/1: "),
            (UMIN.replace("(ite (ule x y) x y)", "(ite (ule x y) x y) x"), r"^result/1: trailing expression$"),
            (UMIN.replace("(ite (ule x y) x y)", "(udiv x y)"), r"^result/1: unsupported word operation udiv$"),
            (UMIN.replace('["nop"]', '["setp","rax"]'), r"^palette: InvalidDeclaredInstruction$"),
            (UMIN + "effect memory;\n", r"profile zal/word-kernel/2 requires the declarations"),
            (UMIN.replace("modulo 2^64", "checked"), r"^result/1: declaration syntax is unsupported"),
        ]
        for text, reason in cases:
            with self.subTest(reason=reason), self.assertRaisesRegex(ValueError, reason):
                w.parse(text)
        english = source("umin-x86", "english")
        for text, reason in ((english.replace("Inputs x and y are", "Inputs x are"), r"^inputs: write 'Input NAME is"),
                             (english.replace("is at most y (unsigned)", "is at most y"), r"^result/1: "),
                             (english.replace("x is at most", "z is at most"), r"^result/1: unknown input z$")):
            with self.subTest(reason=reason), self.assertRaisesRegex(ValueError, reason):
                w.parse(text, "english")

    def test_profile_line_and_declarations_must_agree(self):
        # A profile-1 line keeps the profile-1 parser and its existing refusals.
        with self.assertRaisesRegex(ValueError, "unsupported word operation"):
            w.parse(UMIN.replace("zal/word-kernel/2", "zal/word-kernel/1"))
        with self.assertRaisesRegex(ValueError, "exactly nine declarations"):
            w.parse(ADD128.replace("zal/word-kernel/2", "zal/word-kernel/1"))
        with self.assertRaises(ValueError):
            w.parse(source("umin-x86", "english"), "symbolic")
        with self.assertRaises(ValueError):
            w.parse(UMIN, "english")
        profile1 = w.lower(w.parse((DATA / "add64-x86.zal").read_text("ascii")))
        self.assertEqual(profile1["binding"]["profile"], "zal/word-kernel/1")
        self.assertNotIn("profile_syntax_sha256", profile1["binding"])
        self.assertIs(type(w.parse((DATA / "add64-x86.zal").read_text("ascii")).kernel), w.Kernel)

    def test_forged_documents_refuse(self):
        document = w.parse(ADD128)
        with self.assertRaises(FrozenInstanceError):
            document.kernel.name = "other"
        profile1 = w.parse((DATA / "add64-x86.zal").read_text("ascii"))
        for forged in (replace(document, kernel=replace(document.kernel, name="other")),
                       replace(document, kernel=replace(document.kernel, inputs=list(document.kernel.inputs))),
                       replace(document, kernel=profile1.kernel), replace(profile1, kernel=document.kernel),
                       replace(document, spans=document.spans[:-1])):
            with self.subTest(forged=forged.kernel.name), self.assertRaisesRegex(ValueError, "exact source parsing"):
                w.lower(forged)

    def test_what_if_questions_and_answers(self):
        document = w.parse(ADD128)
        rev = revision(ADD128)
        values = {"a_lo": MAX, "a_hi": "0", "b_lo": "1", "b_hi": "0"}
        answer = w.what_if_inputs(document, rev, values)
        self.assertEqual((answer["expected"], answer["result_registers"]), (["0", "1"], ["rax", "rdx"]))
        asked = w.question(document, rev, f"What if a_lo is {MAX}, a_hi is 0, b_lo is 1 and b_hi is 0?")
        self.assertEqual(asked["expected"], ["0", "1"])
        self.assertEqual(w.question(w.parse(UMIN), revision(UMIN), "What if x is 7 and y is 3?")["expected"], ["3"])
        for bad in ({"a_lo": "1"}, {**values, "c": "1"}, {**values, "a_lo": "01"}):
            with self.subTest(values=bad), self.assertRaises(ValueError):
                w.what_if_inputs(document, rev, bad)
        with self.assertRaisesRegex(ValueError, "takes --input"):
            w.what_if(document, rev, "1", "2")
        with self.assertRaises(ValueError):
            w.what_if_inputs(document, "0" * 64, values)
        clobbers = w.question(document, rev, "What can be clobbered?")["answer"]
        self.assertEqual(clobbers["input_registers"], {"a_lo": "rdi", "a_hi": "rsi", "b_lo": "rdx", "b_hi": "rcx"})
        self.assertEqual(clobbers["result_registers"], ["rax", "rdx"])
        self.assertEqual(clobbers["entry_flags_universally_quantified"], ["cf", "zf", "sf", "of"])
        self.assertEqual(clobbers["entry_scratch_universally_quantified"], ["rax", "r8", "r9", "r10", "r11"])
        for text in ("What happens on overflow?", "Does it access memory?", "What are the limits?",
                     "How is a candidate checked?", "What is observed?"):
            self.assertEqual(w.question(document, rev, text)["revision"], rev)
        with self.assertRaises(ValueError):
            w.question(document, rev, "What if x is 1 and y is 2?")

    def test_cli_read_show_render_what_if_and_argument_refusals(self):
        path = DATA / "add128-x86.zal"
        rev = revision(ADD128)
        inputs = ["--input", f"a_lo={MAX}", "--input", "a_hi=0", "--input", "b_lo=1", "--input", "b_hi=0"]
        for command, rest, code in (("read", [], 0), ("show", ["--revision", rev], 0), ("lower", ["--revision", rev], 0),
                                    ("what-if", ["--revision", rev] + inputs, 0),
                                    ("what-if", ["--revision", rev, "--x", "1", "--y", "2"], 65),
                                    ("what-if", ["--revision", rev] + inputs[:6], 65),
                                    ("what-if", ["--revision", rev] + inputs + ["--input", "a_lo=1"], 65)):
            with self.subTest(command=command, rest=rest), contextlib.redirect_stdout(io.StringIO()) as output:
                self.assertEqual(w.main([command, "--source", str(path)] + rest), code)
                self.assertEqual(json.loads(output.getvalue())["authority"], "none")
        with contextlib.redirect_stdout(io.StringIO()) as output:
            self.assertEqual(w.main(["render", "--source", str(path), "--revision", rev, "--to", "english"]), 0)
        self.assertEqual(output.getvalue(), source("add128-x86", "english"))
        with contextlib.redirect_stderr(io.StringIO()) as error, self.assertRaises(SystemExit) as stopped:
            w.main(["what-if", "--source", str(path), "--revision", rev])
        self.assertEqual(stopped.exception.code, 2)
        self.assertIn("the following arguments are required: --x, --y", error.getvalue())
        profile1 = DATA / "add64-x86.zal"
        rev1 = revision(profile1.read_text("ascii"))
        with contextlib.redirect_stdout(io.StringIO()) as output:
            self.assertEqual(w.main(["what-if", "--source", str(profile1), "--revision", rev1, "--input", "x=1"]), 65)
        self.assertIn("takes --x and --y", json.loads(output.getvalue())["reason"])

    def test_counterexample_feedback_is_checked_against_source_meaning(self):
        document = w.parse(UMIN)
        lowered = w.lower(document)
        verification = {"status": "counterexample", "inputs": {"x": "5", "y": "3"},
                        "initial_registers": {r: "0" for r in ("rax", "rcx", "rdx", "r8", "r9", "r10", "r11")},
                        "initial_flags": {"cf": 0, "zf": 1, "sf": 0, "of": 0}, "expected": ["3"], "actual": ["5"]}
        reply = w.feedback(document, lowered, {"status": "counterexample", "verification": verification})
        self.assertEqual(reply["human_question"]["expected"], ["3"])
        self.assertEqual(reply["human_question"]["entry_flags"], verification["initial_flags"])
        self.assertIn("At x=5, y=3", reply["human_question"]["text"])
        for mutate in (lambda v: v["initial_registers"].pop("rax"), lambda v: v.update(expected=["5"]),
                       lambda v: v.update(actual=["3"]), lambda v: v["initial_flags"].pop("of"),
                       lambda v: v.update(inputs={"x": "5"}), lambda v: v["initial_flags"].update(cf=2),
                       lambda v: v.update(expected="3")):
            bad = copy.deepcopy(verification); mutate(bad)
            with self.subTest(verification=bad), self.assertRaises(ValueError):
                w.feedback(document, lowered, {"status": "counterexample", "verification": bad})


class WithSMT(unittest.TestCase):
    """Real profile-2 checker; no skip-as-pass when Z3 or its bound native inputs are absent."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="zal-word2-smt-")
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)

    def test_umin_search_emit_and_replay_from_english(self):
        english = source("umin-x86", "english")
        rev = revision(english, "english")
        session, artifact = self.base / "session", self.base / "artifact"
        self.assertEqual(w.open_session(english, "english", session, rev)["status"], "opened")
        found = w.search(session, rev)
        self.assertEqual(found["status"], "checked-candidate", found)
        self.assertEqual(found["machine"]["cost"], {"bytes": 11, "instructions": 3})
        self.assertEqual(w.emit(session, rev, artifact)["status"], "emitted")
        self.assertEqual(w.replay(artifact, rev)["status"], "replayed")
        manifest = w.read_json(artifact / "manifest.json")
        self.assertEqual((manifest["profile"], manifest["assurance"]), ("zal/word-kernel/2", w2.ASSURANCE))
        library = json.loads((LIBRARY / "umin-x86.json").read_text())
        self.assertEqual((artifact / "machine/kernel.bin").read_bytes().hex(), library["code_hex"])

    def test_add128_candidate_packet_emit_replay(self):
        rev = revision(ADD128)
        session, artifact = self.base / "session", self.base / "artifact"
        w.open_session(ADD128, "symbolic", session, rev)
        accepted = w.candidate(session, rev, proposal(self.base / "good.json", GOOD_ADD128))
        self.assertEqual(accepted["status"], "equivalent", accepted)
        packet = w.packet(session, rev)
        self.assertEqual(packet["machine_packet"]["problem"]["schema"], "zeno-fcis/machine-problem/2")
        self.assertEqual(w.emit(session, rev, artifact)["status"], "emitted")
        self.assertEqual(w.replay(artifact, rev)["status"], "replayed")

    def test_planted_wrong_expression_gives_a_source_level_counterexample(self):
        wrong = ADD128.replace("result 2 (add (add a_hi b_hi) (ult (add a_lo b_lo) a_lo)) modulo 2^64;",
                               "result 2 (add a_hi b_hi) modulo 2^64;")
        rev = revision(wrong)
        session = self.base / "session"
        w.open_session(wrong, "symbolic", session, rev)
        reply = w.candidate(session, rev, proposal(self.base / "good.json", GOOD_ADD128))
        self.assertEqual(reply["status"], "counterexample", reply)
        question = reply["human_question"]
        self.assertEqual(set(question["inputs"]), {"a_lo", "a_hi", "b_lo", "b_hi"})
        self.assertEqual(set(question["entry_flags"]), {"cf", "zf", "sf", "of"})
        self.assertEqual(question["expected"][0], question["reported_actual"][0])
        self.assertEqual((int(question["reported_actual"][1]) - int(question["expected"][1])) % (1 << 64), 1)
        self.assertEqual(w.packet(session, rev)["human_feedback"][0]["human_question"], question)

    def test_rehashed_meaning_change_cannot_reuse_the_machine_artifact(self):
        rev = revision(ADD128)
        session, artifact = self.base / "session", self.base / "artifact"
        w.open_session(ADD128, "symbolic", session, rev)
        w.candidate(session, rev, proposal(self.base / "good.json", GOOD_ADD128))
        w.emit(session, rev, artifact)
        attack = self.base / "attack"; shutil.copytree(artifact, attack)
        changed = ADD128.replace("(add a_lo b_lo) modulo", "(sub a_lo b_lo) modulo")
        bound = w.lower(w.parse(changed))["binding"]
        (attack / "source.zal").write_text(changed, "ascii")
        (attack / "binding.json").write_bytes(w.canon(bound) + b"\n")
        manifest = w.read_json(attack / "manifest.json")
        manifest.update(revision=bound["revision"], source_sha256=bound["source_sha256"], problem_sha256=bound["problem_sha256"])
        manifest["files"] = {name: {"sha256": w.digest(w.read_bytes(attack / name)), "bytes": (attack / name).stat().st_size}
                             for name in ("source.zal", "binding.json")}
        (attack / "manifest.json").write_bytes(w.canon(manifest) + b"\n")
        with self.assertRaisesRegex(ValueError, "exact ZAL lowering"):
            w.replay(attack, bound["revision"])


if __name__ == "__main__":
    unittest.main()
