"""The ZAL loop workspace shell: channels, transcript chain, view, owner terminal and MCP tools.

The door commands are replaced by recorded zeno-fcis reports here, so these tests need no CLI build; the
real doors run in check_loop_workspace.py and the loop-doors acceptance scenarios.
"""
from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from behavior import Refusal
import loop_doors
import loop_judges as J
import loop_language as Z
import loop_workspace as LW
import mcp

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
SHA = "ab" * 32
# bounded-counter at C = 8, as `contract export-program` writes it (fixture vocabulary).
PROGRAM = {
    "inputs": [{"kind": "Int", "min": "0", "max": "8"}, {"kind": "Int", "min": "150", "max": "151"}, {"kind": "Bool"}],
    "outputs": [{"kind": "Int", "min": "0", "max": "4"},
                {"kind": "Int", "min": "-9223372036854775808", "max": "9223372036854775807"},
                {"kind": "Int", "min": "-9223372036854775808", "max": "9223372036854775807"}],
    "nodes": [["Input", 2], ["Input", 1], ["Input", 0], ["Int", "150"], ["Eq", 1, 3], ["Int", "8"], ["Int", "0"],
              ["Select", 4, 5, 6], ["Eq", 2, 7], ["Int", "149"], ["Int", "147"], ["Select", 8, 9, 10],
              ["Sub", 1, 11], ["Select", 0, 12, 6], ["Int", "1"], ["Add", 2, 14], ["Sub", 2, 14]],
    "roots": [13, 15, 16]}


def witness_report(door: dict) -> dict:
    witness = {"input": ["7", "150", True], "ordinal": 29,
               "observations": {"original": {"ok": ["3", "8", "6"]}, "candidate": {"ok": ["1", "8", "6"]}}}
    return {"status": "different", "authority": "none",
            "detail": {"outcome": {"outcome": "different", "witness": witness}, "door": door,
                       "incumbent": {"kind": "original-admitted"}}}


REPAIR = {"label": "checked repair", "program": {"nodes": 17},
          "check": {"verdict": "equivalent", "replay": "matched"}}
REFUTED = witness_report({"door": "expressions", "lowering": "lowered", "explanation": {
    "region": {"tuples": 2, "of": 36, "where": "value == 7 && action == Inc && authorized"},
    "sites": [{"output": "out0", "subexpression": "7", "replacement": "8"}], "repair": REPAIR}})
COMPLETED = {"status": "checked-improvement", "authority": "none", "detail": {
    "outcome": {"outcome": "checked-improvement", "cost": {"nodes": 17, "bytes": 1198}},
    "incumbent": {"kind": "checked-replacement", "attempt": 1, "cost": {"nodes": 17, "bytes": 1198}, "sha256": SHA},
    "incumbent_program": PROGRAM,
    "door": {"door": "sketch", "status": "completed", "check": {"verdict": "equivalent", "replay": "matched"},
             "holes": {"0": {"fill": "8"}, "1": {"fill": "149"}, "2": {"fill": "147"}}}}}


ACCEPTED = {"status": "equivalent-not-smaller", "authority": "none", "detail": {
    "outcome": {"outcome": "equivalent-not-smaller", "cost": {"nodes": 24, "bytes": 1556}},
    "incumbent": {"kind": "original-admitted"},
    "door": {"door": "expressions", "lowering": "lowered", "check": {"verdict": "equivalent", "replay": "matched"}}}}


QUESTION = {"case": 0, "tuples": 18, "where": "!authorized",
            "example": {"value": 0, "action": "Inc", "authorized": False},
            "options": [{"class": "Reject", "reason": "not_authorized", "current": True},
                        {"class": "Reject", "reason": "at_capacity", "current": False},
                        {"class": "Reject", "reason": "at_zero", "current": False}]}


class Fake:
    """Stands in for the zeno-fcis CLI: recorded reports, no checking here."""

    def __init__(self):
        self.reports = []
        self.calls = []
        self.keys = {}          # outputs -> meaning key; "core" means the original's

    def next(self, *arguments):
        self.calls.append(arguments)
        return 0, self.reports.pop(0)

    def meaning(self, path):
        outputs = tuple(json.loads(Path(path).read_text())["outputs"])
        key = self.keys.get(outputs, LW.Z.digest({"outputs": list(outputs)})[:64].ljust(64, "0"))
        return 0, {"status": "keyed", "door": "meaning",
                   "detail": {"key": "c" * 64 if key == "core" else key, "same_as_original": key == "core"}}


def subject_contract(directory: Path) -> Path:
    sys.path.insert(0, str(ROOT / "tools"))
    try:
        import instantiate_core
    finally:
        sys.path.pop(0)
    contract = directory / "contract"
    instantiate_core.instantiate("bounded-counter", {"C": 8}, contract, include_proof=False)
    return contract


class LoopWorkspaceTests(unittest.TestCase):
    def test_later_owned_read_feedback_names_the_field_and_suggested_order(self):
        self.fake.reports.append({"status": "unrealizable", "detail": {"verdict": "unrealizable", "structure": [{
            "what": "reads-later", "kind": "reads-later", "component": "lifecycle", "field": "value",
            "owner": "ballot", "suggested_order": ["ballot", "lifecycle"], "scope": "submitted-component-order"}]}})
        events = self.submit("component lifecycle of d: owns value; reads action; decides at_zero;\n"
                             "decompose d: lifecycle;\n")
        facts = [e["symbolic"] for e in events if e["act"]["kind"] == "fact"]
        self.assertTrue(any("reads value owned later by ballot" in fact and "ballot, lifecycle" in fact for fact in facts))
        self.assertIn("fact d: unrealizable in the submitted component order;", facts)

    def freedom_report(self, question=QUESTION, **stats):
        values = {"tuples": 36, "reference_refused": 0, "determined": 0, "determined_by_laws": 0,
                  "determined_with_reasons": 0, "regions": 5, "inconsistent": 0, "determined_rate": 0.0}
        values.update(stats)
        return {"status": "open" if question else "determined", "detail": {"stats": values, "question": question}}

    def test_workspace_fact_distinguishes_laws_from_recorded_reasons(self):
        self.fake.reports.append(self.freedom_report(None, determined=36, determined_by_laws=16,
                                                     determined_with_reasons=36, regions=0, determined_rate=1.0))
        events = self.submit("probe f: freedom;\n")
        fact = events[-1]
        self.assertIn("36 of 36 decisions are determined with recorded reasons", fact["symbolic"])
        self.assertIn("16 of 36 determined by laws", fact["symbolic"])
        self.assertEqual(fact["detail"]["determined_by_laws"], 16)
        self.assertEqual(fact["detail"]["determined_with_reasons"], 36)

    def test_two_probes_without_an_answer_emit_one_ask_and_return_its_id(self):
        self.fake.reports += [self.freedom_report(), self.freedom_report()]
        self.submit("probe f1: freedom;\n")
        events = self.submit("probe f2: freedom;\n")
        state = self.workspace.load()
        asks = [event for event in state["events"] if event["act"]["kind"] == "ask"]
        self.assertEqual([event["act"]["id"] for event in asks], ["q1"])
        self.assertEqual(list(state["questions"]), ["q1"])
        self.assertEqual(events[-1]["detail"]["question_id"], "q1")
        self.assertEqual(state["questions"]["q1"]["status"], "open")

    def test_changed_question_supersedes_the_old_question_and_refuses_its_answer(self):
        different = dict(QUESTION, tuples=17)
        self.fake.reports += [self.freedom_report(), self.freedom_report(different)]
        self.submit("probe f1: freedom;\n")
        self.submit("probe f2: freedom;\n")
        state = self.workspace.load()
        self.assertEqual(state["questions"]["q1"]["status"], "superseded")
        self.assertEqual(state["questions"]["q1"]["superseded_by"], "q2")
        self.assertEqual([q["id"] for q in self.workspace.view()["open_questions"]], ["q2"])
        before = self.path.read_bytes()
        with self.assertRaisesRegex(Refusal, "no open question q1"):
            self.workspace.answer("q1", "1")
        self.assertEqual(before, self.path.read_bytes())

    def test_a_determined_probe_supersedes_an_old_open_question(self):
        self.fake.reports += [self.freedom_report(), self.freedom_report(None, determined=36, regions=0)]
        self.submit("probe f1: freedom;\n")
        self.submit("probe f2: freedom;\n")
        self.assertEqual(self.workspace.load()["questions"]["q1"]["status"], "superseded")
        self.assertEqual(self.workspace.view()["open_questions"], [])

    def test_legacy_open_judge_questions_are_retired_on_the_next_probe(self):
        self.fake.reports += [self.freedom_report(), self.freedom_report()]
        self.submit("probe f1: freedom;\n")
        state = self.workspace.load()
        del state["questions"]["q1"]["freedom_key"]
        self.workspace.save(state)
        self.submit("probe f2: freedom;\n")
        state = self.workspace.load()
        self.assertEqual(state["questions"]["q1"]["status"], "superseded")
        self.assertEqual(state["questions"]["q2"]["status"], "open")
        with self.assertRaisesRegex(Refusal, "no open question q1"):
            self.workspace.answer("q1", "1")

    def test_owner_terminal_next_starts_and_reuses_a_question_without_a_model(self):
        import contextlib
        import io
        self.fake.reports += [self.freedom_report(), self.freedom_report()]
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(LW.terminal(["next", "--workspace", str(self.path)]), 0)
            self.assertEqual(LW.terminal(["next", "--workspace", str(self.path)]), 0)
        state = self.workspace.load()
        self.assertEqual([e["author"] for e in state["events"] if e["act"]["kind"] == "probe"], ["owner", "owner"])
        self.assertEqual([e["act"]["id"] for e in state["events"] if e["act"]["kind"] == "ask"], ["q1"])
        self.assertFalse(state.get("confirmed"))

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.dir = Path(self.temp.name)
        self.fake = Fake()
        self.lemma_reads = []
        self.patches = [
            patch.object(loop_doors, "resolve_cli", lambda given=None: Path("/fake/zeno-fcis")),
            patch.object(loop_doors, "export_program",
                         lambda cli, contract, out: (out.write_bytes(b"program"),
                                                     {"application": "bounded-counter-c8"})[1]),
            patch.object(loop_doors, "open_session", lambda cli, original, session: (
                session.mkdir(), {"detail": {"view": {"original": {"nodes": 24, "bytes": 1556, "program": PROGRAM}},
                                             "request": {"original": {"sha256": SHA}}}})[1]),
            patch.object(loop_doors, "candidate", lambda cli, session, path: self.fake.next("candidate", path)),
            patch.object(loop_doors, "sketch", lambda cli, session, path: self.fake.next("sketch", path)),
            patch.object(loop_doors, "meaning", lambda cli, original, path: self.fake.meaning(path)),
            patch.object(loop_doors, "gaps", lambda cli, contract, laws: self.fake.next("gaps", laws)),
            patch.object(loop_doors, "question", lambda cli, contract, answers: self.fake.next("question", answers)),
            patch.object(loop_doors, "lemma", lambda cli, contract, formula: (
                self.lemma_reads.append((contract, (contract / "v2" / "policy.json").read_text())),
                self.fake.next("lemma", formula))[1]),
            patch.object(loop_doors, "teach_back",
                         lambda cli, contract, rules, answers: self.fake.next("teach-back", rules)),
            patch.object(loop_doors, "probe", lambda cli, contract, q, args: self.fake.next("probe", q, args)),
            patch.object(loop_doors, "scenario", lambda cli, contract, path: self.fake.next("scenario", path)),
            patch.object(loop_doors, "decompose", lambda cli, contract, path: self.fake.next("decompose", path)),
        ]
        for p in self.patches:
            p.start()
        self.path = self.dir / "loop.json"
        self.workspace = LW.LoopWorkspace(self.path)
        self.contract = subject_contract(self.dir)
        self.workspace.init(contract=self.contract)
        state = self.workspace.load()
        self.head = f"zal fcis loop 1;\nsubject bounded-counter-c8 at {state['subject']['revision']};\n"

    def tearDown(self):
        for p in self.patches:
            p.stop()
        self.temp.cleanup()

    def printed(self, text: str) -> str:
        """The expression as the workspace writes it into a door document."""
        return Z.print_symbolic(Z.read(text, Z.Context(self.workspace.load()["subject"]), False))

    def submit(self, body: str, author: str = "model"):
        return self.workspace.submit(self.head + f"author {author};\n" + body, author)

    # BEGIN capture-channel regressions (additive)
    def test_capture_claims_refuse_on_the_ordinary_model_channel_without_recording(self):
        before = self.workspace.load()["events"]
        for body in ("captured-exact p1: 8 tuples, replay matched;\n",
                     "examples-equivalent p1: 3 examples;\n",
                     "capture-result p1: refused;\n",
                     "capture p1 out0 = 0;\n"):
            with self.subTest(body=body), patch.object(self.workspace, "_record", side_effect=AssertionError("recorded")):
                with self.assertRaises(Refusal):
                    self.submit(body)
            self.assertEqual(self.workspace.load()["events"], before)
        # Direct shell callers also cannot bypass the guard with capture-tagged
        # contexts or preconstructed acts; this reproduces the review probe.
        import capture_workspace as CW
        ctx = CW.context({"inputs": [], "outputs": [{"kind": "Bool"}]}, "capture", "a" * 64)
        for act in (Z.act("captured-exact", "p1", tuples=8),
                    Z.act("examples-equivalent", "p1", examples=3),
                    Z.act("capture-example", "p1", input=(), output=(1,)),
                    Z.act("capture-result", "p1", status="refused")):
            with patch.object(self.workspace, "_record", side_effect=AssertionError("recorded")):
                with self.assertRaises(Refusal):
                    self.workspace._submit({"events": []}, ctx, Z.Document(ctx.subject_ref, "model", (act,)))
    # END capture-channel regressions

    def test_a_refuted_program_gets_its_region_sites_and_repair_and_is_remembered(self):
        self.fake.reports.append(REFUTED)
        wrong = "authorized ? action - (value == (action == Inc ? 7 : 0) ? 149 : 147) : 0"
        events = self.submit(f"program p1 out0 = {wrong};\n")
        kinds = [e["act"]["kind"] for e in events]
        self.assertEqual(kinds, ["program", "refuted", "region", "site", "fact"])
        self.assertEqual(events[1]["symbolic"],
                         "refuted p1 at value = 7, action = Inc, authorized = true: out0 expected 3 got 1;")
        self.assertIn("checked repair", events[-1]["english"])
        document = json.loads(Path(self.fake.calls[0][1]).read_text())
        self.assertEqual(document["schema"], "zeno-fcis/loop-expressions/1")
        self.assertEqual(document["outputs"][1:], ["keep", "keep"])
        again = self.submit(f"program p1 out0 = {wrong};\n")
        self.assertIn("already refuted", again[-1]["english"])
        self.assertEqual(len(self.fake.calls), 1, "a refuted proposal is answered from memory")

    def test_a_refuted_program_written_another_way_is_answered_from_memory(self):
        wrong = "authorized ? action - (value == (action == Inc ? 7 : 0) ? 149 : 147) : 0"
        respelled = "!authorized ? 0 : action - ((action == Inc ? 7 : 0) == value ? 149 : 147)"
        for text in (wrong, respelled):
            self.fake.keys[(self.printed(text), "keep", "keep")] = "d" * 64
        self.fake.reports.append(REFUTED)
        first = self.submit(f"program p1 out0 = {wrong};\n")
        self.assertEqual(first[1]["act"]["kind"], "refuted")
        again = self.submit(f"program p2 out0 = {respelled};\n")
        self.assertEqual([e["act"]["kind"] for e in again], ["program", "same"])
        self.assertEqual(again[1]["symbolic"], "same p2 as p1: 36 inputs, event 1, refuted;")
        self.assertEqual(again[1]["english"],
                         "Same p2: it means exactly what p1 means on all 36 inputs, judged refuted at event 1.")
        self.assertEqual(len(self.fake.calls), 1, "a repeat in other words spends no attempt")
        # a program with another meaning is judged as usual
        other = wrong.replace("? 7 :", "? 6 :")
        self.fake.reports.append(REFUTED)
        self.assertEqual(self.submit(f"program p3 out0 = {other};\n")[1]["act"]["kind"], "refuted")
        self.assertEqual(len(self.fake.calls), 2)

    def test_correct_programs_are_never_answered_from_memory(self):
        right = "authorized ? action - (value == (action == Inc ? 8 : 0) ? 149 : 147) : 0"
        respelled = "!authorized ? 0 : action - (value == (action == Inc ? 8 : 0) ? 149 : 147)"
        for text in (right, respelled):
            self.fake.keys[(self.printed(text), "keep", "keep")] = "core"
        for pid, text in (("p1", right), ("p2", respelled)):
            self.fake.reports.append(ACCEPTED)
            events = self.submit(f"program {pid} out0 = {text};\n")
            self.assertEqual(events[1]["act"]["kind"], "accepted", events)
        self.assertEqual(len(self.fake.calls), 2, "each correct program is judged for its cost")

    def test_an_invariant_cycle_returns_its_counterexample_then_accepts_the_revision(self):
        cti = {"state": [7], "input": [150, 1], "successor": [8], "count": 1}
        self.fake.reports += [
            {"lemmas": [{"verdict": "false", "holds_sha256": "01" * 32, "counterexample_to_induction": cti,
                         "trace_from_genesis": [[150, 1]] * 8,
                         "states": {"declared": 9, "admitted": 8, "reachable": 9}}]},
            {"result": {"authority": {"class": "Accept", "reason": None}}},
            {"lemmas": [{"verdict": "inductive", "holds_sha256": "02" * 32, "exact": True,
                         "states": {"declared": 9, "admitted": 9, "reachable": 9}}]},
            {"lemmas": [{"verdict": "inductive", "holds_sha256": "02" * 32, "exact": True,
                         "states": {"declared": 9, "admitted": 9, "reachable": 9}}]},
        ]
        weak = self.submit("invariant i1: value <= 7;\n")
        self.assertEqual(weak[-1]["symbolic"], "fact i1: false: a reachable state steps outside it: from value = 7 on "
                                               "action = Inc, authorized = true the core decides accept and reaches "
                                               "value = 8;")
        self.assertEqual(self.fake.calls[1], ("probe", "explain", ["--input", "7 150 1"]))
        revised = self.submit("invariant i2: value <= 8;\n")
        self.assertTrue(revised[-1]["symbolic"].startswith("fact i2: inductive"))
        again = self.submit("invariant i3: !(value > 8);\n")
        self.assertEqual(again[-1]["symbolic"], "same i3 as i2: 9 states, event 3, inductive;")
        rounds = self.workspace.load()["invariants"]
        self.assertEqual(rounds["i1"]["counterexample_to_induction"]["decision"], "accept")
        self.assertEqual(rounds["i2"]["scope"]["exhaustive"], True)

    def test_a_conflicting_answer_to_a_judge_question_changes_nothing(self):
        self.fake.reports += [
            {"status": "open", "detail": {"stats": {"tuples": 36, "reference_refused": 0, "determined": 0,
                                                    "regions": 5, "inconsistent": 0, "determined_rate": 0.0},
                                          "question": QUESTION}},
            {"status": "conflict", "detail": {"answer": 0, "reason": "answer-conflicts-with-earlier-answers"}},
        ]
        self.submit("probe f: freedom;\n")
        before = self.workspace.load()
        events = self.workspace.answer("q1", "2")
        self.assertEqual(events[-1]["symbolic"], "fact q1: refused: answer-conflicts-with-confirmed-requirements;")
        after = self.workspace.load()
        self.assertEqual(after["questions"]["q1"], before["questions"]["q1"])
        self.assertEqual(after.get("confirmed", []), before.get("confirmed", []))

    def test_judge_questions_skip_used_ids_and_never_overwrite_an_answer(self):
        # a model asks q1, the owner answers it, then a freedom probe posts the judge's question
        self.submit("ask q1 at value = 3, action = Inc, authorized = false: reject not_authorized | "
                    "reject at_zero;\n")
        self.workspace.answer("q1", "2")
        answered = self.workspace.load()["questions"]["q1"]
        self.fake.reports.append(
            {"status": "open", "detail": {"stats": {"tuples": 36, "reference_refused": 0, "determined": 0,
                                                    "regions": 5, "inconsistent": 0, "determined_rate": 0.0},
                                          "question": QUESTION}})
        events = self.submit("probe f: freedom;\n")
        self.assertTrue(events[-1]["symbolic"].startswith("ask q2 where !context.authorized"), events[-1])
        questions = self.workspace.load()["questions"]
        self.assertEqual(questions["q1"], answered)
        self.assertEqual(questions["q1"]["status"], "answered")
        self.assertEqual(questions["q2"]["asked_by"], "judge")

    def test_a_number_in_a_truth_position_keeps_the_loop_meaning_or_is_not_routed(self):
        subject = self.workspace.load()["subject"]
        ctx = Z.Context(subject)

        def plain(text: str) -> str:
            return J.plain_formula(Z.read(text, ctx, False), subject)

        # the loop language reads `!value` as `value == 0`, so the routed law must too
        self.assertEqual(plain("!value"), "pre.100.110 == 0")
        self.assertEqual(plain("!(value + 1)"), "pre.100.110 + 1 == 0")
        self.assertEqual(plain("value && authorized"), "(pre.100.110 == 1) && (context.102.130 == 1)")
        self.assertEqual(plain("!authorized"), "!(context.102.130 == 1)")
        # a Boolean field in a sum is 0 or 1, as in the contract's own laws
        self.assertEqual(plain("authorized + 1 > 1"), "context.102.130 + 1 > 1")
        for text in ("value == authorized", "value || authorized", "!(value || authorized)"):
            with self.assertRaises(J.Unrouted, msg=text):
                plain(text)
        self.fake.reports.append({"result": {"answer": 4, "of": 36}})
        events = self.submit("probe p1: count !value;\n")
        self.assertEqual(self.fake.calls[-1], ("probe", "count", ["--where", "pre.100.110 == 0"]))
        self.assertEqual(events[-1]["symbolic"], "fact p1: 4 of 36 inputs satisfy it;")

    def test_decomposition_steps_one_to_three_are_reported_as_the_doors_own_evaluation(self):
        self.fake.reports.append({"status": "meets-requirements", "detail": {
            "verdict": "meets-requirements", "decided_by": "door-simulation", "domain": 36,
            "local_domain_total": "40", "structure": [], "interface": [], "projection": {}}})
        events = self.submit("component all of d1: owns value; reads action, authorized; decides not_authorized, "
                             "at_capacity, at_zero;\ncase all 0: when true -> reject not_authorized;\n"
                             "decompose d1: all;\n")
        self.assertEqual(events[-1]["symbolic"], "fact d1: by the door's own evaluation, every component meets its "
                                                 "required behaviour on all 36 inputs; local domains total 40;")

    def test_a_rule_the_requirements_forbid_is_never_called_required(self):
        self.fake.reports.append({"detail": {"rules": [{
            "verdict": "exact", "meaning": "ef" * 32, "domain": 36, "applies": 18, "exception_groups": [],
            "missing_region": None,
            "required": {"open": 0, "forced": 0, "forbidden": 18, "of": 18, "example": None, "alternatives": [],
                         "forbidden_example": {"input": {"value": 0, "action": "Inc", "authorized": False},
                                               "admitted": [{"class": "Reject", "reason": "at_capacity"}]}}}]}})
        events = self.submit("rule t1: where !authorized -> reject not_authorized;\n")
        texts = [e["symbolic"] for e in events if e["author"] == "judge"]
        self.assertFalse(any(" required: " in t for t in texts), texts)
        self.assertEqual(texts[-1], "fact t1: forbidden: the confirmed requirements rule this out on 18 of 18 inputs "
                                    "where it holds, for example value = 0, action = Inc, authorized = false, where "
                                    "they admit only Reject at_capacity;")

    def test_judges_read_the_sessions_own_contract_copy_and_memory_is_bound_to_the_subject(self):
        inductive = {"lemmas": [{"verdict": "inductive", "holds_sha256": "03" * 32, "exact": True,
                                 "states": {"declared": 9, "admitted": 1, "reachable": 1}}]}
        self.fake.reports += [inductive, inductive]
        policy = (self.contract / "v2" / "policy.json").read_text()
        first = self.submit("invariant original: value == 0;\n")
        self.assertTrue(first[-1]["symbolic"].startswith("fact original: inductive"), first[-1])
        # the source contract changes after initialization: the judges still read the bound copy
        (self.contract / "v2" / "policy.json").write_text(policy.replace('"Reject"', '"Accept"', 1))
        again = self.submit("invariant respelled: !(value != 0);\n")
        self.assertEqual(again[-1]["symbolic"], "same respelled as original: 9 states, event 1, inductive;")
        state = self.workspace.load()
        copy = self.workspace.data / "subject"
        self.assertEqual([read for read in self.lemma_reads], [(copy, policy), (copy, policy)])
        # the memory key binds the revision and the policy, not the admitted states alone
        key = next(k for k in state["meanings"] if k.startswith("invariant:"))
        self.assertNotIn("03" * 32, key)
        self.assertEqual(key, "invariant:" + self.workspace._subject_key(state, "03" * 32))
        # a changed copy is refused before any judge reads it
        target = copy / "v2" / "policy.json"
        target.chmod(0o644)
        target.write_text(policy.replace('"Reject"', '"Accept"', 1))
        refused = self.submit("invariant third: value <= 8;\n")
        self.assertIn("contract copy changed", refused[-1]["symbolic"])
        self.assertEqual(len(self.lemma_reads), 2)

    def test_a_completed_sketch_becomes_the_incumbent(self):
        self.fake.reports.append(COMPLETED)
        events = self.submit("program p2 out0 = authorized ? action - (value == (action == Inc ? ?c : 0) ? ?c : ?c)"
                             " : 0;\n")
        self.assertEqual([e["act"]["kind"] for e in events], ["program", "completed", "accepted", "fact"])
        self.assertEqual(events[1]["symbolic"], "completed p2: ?0 = 8; ?1 = 149; ?2 = 147;")
        self.assertEqual(self.workspace.load()["incumbent"]["nodes"], 17)
        self.assertEqual(self.fake.calls[0][0], "sketch")

    def test_channels_carry_authority_by_convention(self):
        with self.assertRaises(Refusal):
            self.submit("answer k1: any;\n")                          # a model cannot answer
        with self.assertRaises(Refusal):
            self.submit("fact f1: hello;\n", author="judge")          # only the judge speaks for the judge
        with self.assertRaises(Refusal):
            self.submit("program p1 out0 = 0;\n", author="owner")     # the owner channel: answers and rules
        with self.assertRaises(Refusal):
            self.workspace.submit(self.head + "author owner;\nanswer k1: any;\n", "model")
        with self.assertRaises(Refusal):
            self.submit("accepted p1: 36 tuples, 1 nodes;\n")          # judge statements are not submitted
        with self.assertRaises(Refusal):
            self.workspace.say("program p1 out0 = 0;")

    def test_the_owner_answers_through_the_terminal_and_acts_route_to_the_contract_judges(self):
        self.fake.reports += [
            {"gaps": {"status": "measured", "proposals": {"laws": [{"applies_on": 16}], "now_caught": ["m1"]}}},
            {"status": "open", "detail": {"stats": {"tuples": 36, "reference_refused": 0, "determined": 0,
                                                    "regions": 5, "inconsistent": 0, "determined_rate": 0.0},
                                          "question": QUESTION}},
            {"lemmas": [{"verdict": "inductive", "holds_sha256": "ab" * 32, "exact": True,
                         "states": {"declared": 9, "admitted": 9, "reachable": 9}}]},
        ]
        events = self.submit("ask k1 at value = 3, action = Inc, authorized = false: reject not_authorized | "
                             "reject at_zero;\nlaw l1 on accept: post.value <= 8;\nprobe f: freedom;\n"
                             "invariant i1: value <= 8;\n")
        self.assertEqual(events[1]["english"], "Fact k1: question passed to the owner.")
        judged = [e["symbolic"] for e in events if e["author"] == "judge"][1:]
        self.assertEqual(judged[0], "fact l1: holds: applies on 16 inputs and rules out 1 surviving mutants; pending "
                                    "the owner's review;")
        self.assertTrue(judged[1].startswith("fact f: 0 of 36 decisions are determined"), judged)
        self.assertEqual(judged[2], "ask q1 where !context.authorized at value = 0, action = Inc, authorized = false: "
                                    "reject not_authorized | reject at_capacity | reject at_zero;")
        self.assertTrue(judged[3].startswith("fact i1: inductive: holds on all 9 reachable states"), judged)
        laws = self.fake.calls[0][1].read_text()
        self.assertEqual(laws, "law 8000 proposed_l1 on accept = post.100.110 <= 8;\n")
        self.assertEqual(self.fake.calls[2], ("lemma", "post.100.110 <= 8"))
        view = json.loads(self.workspace.view_path.read_text())
        self.assertEqual([q["id"] for q in view["open_questions"]], ["k1", "q1"])
        self.assertEqual(view["open_questions"][0]["options"][1]["symbolic"], "reject at_zero")
        # an answer that is not an offered option changes nothing (the source's owner-confirmation gate)
        before = self.workspace.load()["questions"]["k1"]
        refused = self.submit("answer k1: accept;\n", "owner")
        self.assertEqual(refused[-1]["english"], "Fact k1: refused: the answer is not an offered option.")
        self.assertEqual(self.workspace.load()["questions"]["k1"], before)
        done = subprocess.run([sys.executable, str(HERE / "terminal.py"), "loop", "answer", "k1", "2", "--workspace",
                               str(self.path)], capture_output=True, text=True, timeout=120)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertIn("answer recorded: reject at_zero", done.stdout)
        state = self.workspace.load()
        self.assertEqual(state["questions"]["k1"]["status"], "answered")
        self.assertEqual(state["events"][-2]["author"], "owner")
        with self.assertRaises(Refusal):
            self.workspace.answer("k1", "1")
        # asking again with the same ID cannot overwrite the owner's answer
        answered = self.workspace.load()["questions"]["k1"]
        again = self.submit("ask k1 at value = 3, action = Inc, authorized = false: reject at_zero | accept;\n")
        self.assertEqual(again[-1]["english"], "Fact k1: refused: question k1 already exists.")
        self.assertEqual(self.workspace.load()["questions"]["k1"], answered)
        self.fake.reports.append({"detail": {"rules": [{
            "verdict": "wrong", "meaning": "cd" * 32, "domain": 36, "applies": 4,
            "exception_groups": [{"count": 2, "where": "!authorized && value == 0",
                                  "input": {"value": 0, "action": "Inc", "authorized": False},
                                  "actual": {"class": "Reject", "reason": "not_authorized"}}],
            "missing_region": None}]}})
        rule = self.workspace.say("Rule t1: where value is 0, the core will reject because at_zero.")
        self.assertEqual(rule[0]["author"], "owner")
        self.assertEqual(rule[-1]["symbolic"], "exception t1: 2 of 4 where !authorized && value == 0 at value = 0, "
                                               "action = Inc, authorized = false: reject not_authorized;")

    def test_every_event_reparses_to_its_act_in_both_surfaces(self):
        self.fake.reports += [REFUTED, COMPLETED]
        self.submit("program p1 out0 = authorized ? action - (value == (action == Inc ? 7 : 0) ? 149 : 147) : 0;\n"
                    "ask k1 at value = 3, action = Inc, authorized = false: reject not_authorized | reject at_zero;\n")
        self.submit("program p2 out0 = authorized ? action - (value == (action == Inc ? ?c : 0) ? ?c : ?c) : 0;\n")
        self.workspace.answer("k1", "any")
        state = self.workspace.load()
        ctx = Z.Context(state["subject"])
        self.assertGreater(len(state["events"]), 10)
        for event in state["events"]:
            for english in (False, True):
                parsed = Z.parse_statement(event["english" if english else "symbolic"], ctx, english)
                self.assertEqual(parsed.data(), event["act"], event)

    def test_the_transcript_chain_is_verified_on_every_load(self):
        self.submit("ask k1 at value = 3, action = Inc, authorized = false: reject not_authorized | reject at_zero;\n")
        data = json.loads(self.path.read_text())
        data["loop"]["events"][0]["english"] = "Question k1: something else?"
        self.path.write_text(json.dumps(data))
        with self.assertRaises(Refusal):
            self.workspace.load()

    def test_the_packet_names_the_header_lines_and_the_vocabulary(self):
        packet = self.workspace.packet()
        lines = packet.splitlines()
        self.assertEqual(lines[0], "zal fcis loop 1;")
        self.assertTrue(lines[1].startswith("subject bounded-counter-c8 at "))
        self.assertEqual(lines[2], "author model;")
        self.assertIn("#   action: command, 150..151 (Inc=150, Dec=151)", lines)
        self.assertIn("# program incumbent out1 = value + 1;", lines)


# --- the routed translation against the loop language's meaning ------------------------------------------

SWEEP_SUBJECT = {"paths": {"pre.100.110": "x", "context.102.130": "f"}, "names": ["x", "f"],
                 "kinds": ["Int", "Bool"], "variants": {}}
SWEEP_ENVS = [{"x": x, "f": f} for x in range(4) for f in (0, 1)]
SWEEP_PATHS = {"pre.100.110": "x", "context.102.130": "f"}


def arithmetic_reference(name, args):
    """Independent test oracle only; production arithmetic is evaluated in Rust."""
    import math
    if not name.startswith("div_") and any(v < 0 for v in args):
        raise Refusal("arithmetic")
    if name == "products_ge":
        middle = len(args) // 2
        return int(math.prod(args[:middle]) >= math.prod(args[middle:]))
    numerator = args[0] if name.startswith("div_") else args[0] * args[1]
    denominator = args[-1]
    if denominator == 0:
        raise Refusal("arithmetic")
    q, r = divmod(numerator, denominator)
    value = q + int(name.endswith("ceil") and r != 0)
    if not -(1 << 63) <= value < (1 << 63):
        raise Refusal("arithmetic")
    return value


def loop_value(node, env):
    """Test-only eager source semantics for the bounded translation corpus."""
    tag = node[0]
    if tag == "int":
        return node[1]
    if tag == "var":
        return env[node[1]]
    if tag == "call":
        return arithmetic_reference(node[1], [loop_value(a, env) for a in node[2]])
    if tag == "not":
        return int(loop_value(node[1], env) == 0)
    if tag == "if":
        c, a, b = (loop_value(n, env) for n in node[1:])
        return a if c == 1 else b
    op, a, b = node[1], loop_value(node[2], env), loop_value(node[3], env)
    return {"+": lambda: a + b, "-": lambda: a - b, "*": lambda: a * b,
            "==": lambda: int(a == b), "!=": lambda: int(a != b), "<": lambda: int(a < b),
            "<=": lambda: int(a <= b), ">": lambda: int(a > b), ">=": lambda: int(a >= b),
            "&&": lambda: int(a == 1 and b == 1), "||": lambda: a if a == 1 else b,
            "->": lambda: b if a == 1 else 1}[op]()


def truth_observation(reader, code, env, source=False):
    try:
        value = reader(code, env)
        return ("value", value == 1 if source else bool(value))
    except Refusal:
        return ("arithmetic-refusal",)


def law_value(text: str, env: dict):
    """A law formula's value on one input: a small reader for the law text the writers emit (paths, integer
    literals, `true`/`false`, `!`, `+ -`, comparisons, `&& || ->`, parentheses)."""
    import re
    tokens = re.findall(r"mul_div_floor|mul_div_ceil|div_floor|div_ceil|products_ge|pre\.\d+\.\d+|context\.\d+\.\d+|\d+|true|false|==|!=|<=|>=|&&|\|\||->|[-+*<>!(),]", text)
    assert "".join(tokens) == re.sub(r"\s+", "", text), text
    at = [0]

    def peek():
        return tokens[at[0]] if at[0] < len(tokens) else None

    def take():
        at[0] += 1
        return tokens[at[0] - 1]

    def atom():
        tok = take()
        if tok == "(":
            inner = level(0)
            assert take() == ")"
            return inner
        if tok == "!":
            return not atom()
        if tok in ("true", "false"):
            return tok == "true"
        if tok in SWEEP_PATHS:
            return env[SWEEP_PATHS[tok]]
        if tok in Z.FUNCTIONS:
            assert take() == "("
            arguments = [level(0)]
            while peek() == ",":
                take()
                arguments.append(level(0))
            assert take() == ")"
            return arithmetic_reference(tok, arguments)
        return int(tok)

    levels = [("->",), ("||",), ("&&",), ("==", "!=", "<", "<=", ">", ">="), ("+", "-"), ("*",)]

    def level(i):
        if i == len(levels):
            return atom()
        left = level(i + 1)
        while peek() in levels[i]:
            op = take()
            right = level(i + 1)
            left = {"->": lambda x, y: (not x) or y, "||": lambda x, y: x or y, "&&": lambda x, y: x and y,
                    "==": lambda x, y: x == y, "!=": lambda x, y: x != y, "<": lambda x, y: x < y,
                    "<=": lambda x, y: x <= y, ">": lambda x, y: x > y, ">=": lambda x, y: x >= y,
                    "+": lambda x, y: x + y, "-": lambda x, y: x - y, "*": lambda x, y: x * y}[op](left, right)
        return left

    out = level(0)
    assert at[0] == len(tokens), text
    return out


class TranslationSweep(unittest.TestCase):
    """Every expression of a bounded depth-2 family over one Boolean and one integer field: its routed law text
    holds on exactly the inputs where the loop language reads it as true, or it is not routed."""

    def test_routed_laws_keep_the_loop_meaning_or_are_refused(self):
        atoms = [("var", "x"), ("var", "f"), ("int", 0), ("int", 1), ("int", 2)]
        ops = ["+", "-", "*", "&&", "||", "->", "==", "!=", "<"]
        depth1 = list(atoms) + [("not", a) for a in atoms] + \
            [("bin", op, a, b) for op in ops for a in atoms for b in atoms] + \
            [("if", c, a, b) for c in atoms[:2] for a in atoms for b in atoms]
        x, zero, one, two = atoms[0], atoms[2], atoms[3], atoms[4]
        depth1 += [("call", name, args) for name in ("mul_div_floor", "mul_div_ceil")
                   for args in ((x, two, one), (x, two, two), (x, one, two), (x, two, zero))]
        depth1 += [("call", name, (x, denominator)) for name in ("div_floor", "div_ceil")
                   for denominator in (zero, one, two)]
        depth1 += [("call", "products_ge", args) for args in ((x, two, one, two),
                   (x, two, zero, one), (x, two, one, one, two, two), (x, two, zero, one, two, two))]
        ifs = [n for n in depth1 if n[0] == "if"]
        depth2 = [("not", n) for n in depth1] + \
            [("bin", op, a, b) for op in ops for n in depth1 for m in atoms for a, b in ((n, m), (m, n))] + \
            [("if", c, a, b) for c in atoms[:2] for n in ifs for m in atoms for a, b in ((n, m), (m, n))]
        translated = refused = 0
        wrong = []
        for node in dict.fromkeys(depth1 + depth2):
            try:
                text = J.plain_formula(node, SWEEP_SUBJECT)
            except J.Unrouted:
                refused += 1
                continue
            translated += 1
            meant = [truth_observation(loop_value, node, env, source=True) for env in SWEEP_ENVS]
            if [truth_observation(law_value, text, env) for env in SWEEP_ENVS] != meant:
                wrong.append((node, text))
        self.assertEqual(wrong[:3], [])
        self.assertGreater(translated, 5000)
        self.assertGreater(refused, 0)
        print(f"depth-2 translation: {translated} routed, {refused} refused, {len(wrong)} mismatches")

    def test_a_conditional_mixing_a_truth_value_and_a_number_is_refused(self):
        x, f = ("var", "x"), ("var", "f")
        for node in (("not", ("if", x, f, x)), ("bin", "==", f, ("if", f, f, x)), ("if", f, x, f)):
            with self.assertRaises(J.Unrouted, msg=node) as caught:
                J.plain_formula(node, SWEEP_SUBJECT)
            self.assertIn("law-conditional-mixes-a-truth-value-and-a-number", str(caught.exception))


class LoopMcpTools(unittest.TestCase):
    ORIGINAL = ["zal_read", "zal_check", "zal_explain", "zal_help", "zal_propose", "zal_compare", "zal_trace"]

    def test_no_answer_or_say_tool_and_the_existing_tools_come_first_unchanged(self):
        names = [t["name"] for t in mcp.TOOLS]
        self.assertEqual(names[:7], self.ORIGINAL)
        self.assertEqual(names[7:12], ["zal_loop_read", "zal_loop_packet", "zal_loop_submit", "zal_loop_translate",
                                     "zal_loop_legend"])
        self.assertEqual(names[12:], ["zal_capture_read", "zal_capture_packet", "zal_capture_submit",
                                      "zal_capture_translate", "zal_capture_legend"])
        for tool in mcp.TOOLS:
            self.assertFalse(any(word in tool["name"] for word in ("answer", "say", "accept")), tool["name"])
        for tool in mcp.TOOLS[7:12]:
            self.assertIn("not the owner's agreement", tool["description"])
        for tool in mcp.TOOLS[12:]:
            self.assertIn("Tool permission is not owner agreement.", tool["description"])

    def test_a_model_document_with_an_answer_is_refused_through_mcp(self):
        tests = LoopWorkspaceTests("test_channels_carry_authority_by_convention")
        tests.setUp()
        try:
            bridge = mcp.Bridge(None)
            with patch.dict("os.environ", {"ZAL_LOOP_WORKSPACE": str(tests.path)}):
                legend = bridge.call("zal_loop_legend", {})
                self.assertIn("answer ID: O;", legend["legend"])
                reply = bridge.handle({"jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": {
                    "name": "zal_loop_submit", "arguments": {"source": tests.head + "author model;\nanswer k1: any;\n"}}})
                self.assertTrue(reply["isError"])
                self.assertIn("cannot answer", reply["content"][0]["text"])
                before = tests.path.read_bytes()
                reply = bridge.handle({"jsonrpc": "2.0", "id": 8, "method": "tools/call", "params": {
                    "name": "zal_loop_answer", "arguments": {}}})
                self.assertTrue(reply["isError"])
                self.assertEqual(tests.path.read_bytes(), before)
                read = bridge.call("zal_loop_read", {})
                self.assertEqual(read["authority"], "none")
        finally:
            tests.tearDown()

    def test_deep_nesting_is_refused_and_the_server_keeps_answering(self):
        tests = LoopWorkspaceTests("test_channels_carry_authority_by_convention")
        tests.setUp()
        try:
            ctx = Z.Context(tests.workspace.load()["subject"])
            # each form nests far past the bound but stays within the line, node and document limits
            for text, english in (("(" * 400 + "value" + ")" * 400 + " == 0", False),
                                  ("!" * 1500 + "authorized", False),
                                  (" -> ".join(["authorized"] * 500), False),
                                  ("-" * 2000 + "value == 0", False),
                                  (" + ".join(["1"] * 1500) + " == value", False),
                                  ("(" * 400 + "value" + ")" * 400 + " is 0", True),
                                  ("not " * 1500 + "authorized", True)):
                with self.assertRaises(Refusal, msg=text[:20]) as caught:
                    Z.read(text, ctx, english)
                self.assertIn("too-deep", str(caught.exception))
            # a shallow expression still reads and round-trips
            shallow = Z.read("((value)) == 0 && !authorized", ctx, False)
            self.assertEqual(Z.read(Z.print_symbolic(shallow), ctx, False), shallow)
            deep = tests.head + "author model;\ninvariant i1: " + "(" * 400 + "value" + ")" * 400 + " == 0;\n"
            stream = [{"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                       "params": {"name": "zal_loop_translate", "arguments": {"source": deep, "to": "english"}}},
                      {"jsonrpc": "2.0", "id": 2, "method": "ping"}]
            done = subprocess.run([sys.executable, "-B", str(HERE / "mcp.py"), "--workspace", str(tests.dir / "zal")],
                                  input="".join(json.dumps(r) + "\n" for r in stream), capture_output=True,
                                  text=True, timeout=120,
                                  env={**__import__("os").environ, "ZAL_LOOP_WORKSPACE": str(tests.path)})
            replies = [json.loads(line) for line in done.stdout.splitlines()]
            self.assertEqual([r["id"] for r in replies], [1, 2], done.stderr[-2000:])
            self.assertTrue(replies[0]["result"]["isError"], replies[0])
            self.assertIn("nested", replies[0]["result"]["content"][0]["text"])
            self.assertEqual(replies[1]["result"], {})
        finally:
            tests.tearDown()


if __name__ == "__main__":
    unittest.main()
