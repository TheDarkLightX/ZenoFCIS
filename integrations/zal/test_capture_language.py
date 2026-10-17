"""Pure paired-surface and shell tests; reference execution stays deferred."""
import json
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from behavior import Refusal
import loop_language as Z
import capture_workspace as W
import mcp


class CaptureLanguageTests(unittest.TestCase):
    def setUp(self):
        self.abi = {"inputs": [{"kind": "Int", "min": "0", "max": "3"}, {"kind": "Bool"}],
                    "outputs": [{"kind": "Bool"}], "tuples": 8,
                    "order": "ordered-product-last-input-fastest-v1"}
        self.ctx = W.context(self.abi, "capture", "a" * 64)

    def test_every_capture_act_round_trips_both_surfaces(self):
        docs = [Z.Document(self.ctx.subject_ref, "model", (Z.act("capture", "p1", output="out0",
                           expr=Z.read("x1 && x0 < 2", self.ctx, False)),)),
                Z.Document(self.ctx.subject_ref, "judge", (
                    Z.act("examples-equivalent", "p1", examples=0),
                    Z.act("capture-example", "p1", input=(0, 0), output=(0,)),
                    Z.act("captured-exact", "p1", tuples=8),
                    Z.act("capture-result", "p1", status="inconclusive")))]
        for doc in docs:
            for english in (False, True):
                self.assertEqual(Z.parse(Z.render(doc, self.ctx, english), self.ctx), doc)

    def test_capture_hooks_leave_other_family_prefixes_and_legacy_acts_to_their_handlers(self):
        document = Z.Document(self.ctx.subject_ref, "owner", (Z.act("answer", "q1", choice="any"),))
        with patch("capture_language.render", side_effect=AssertionError("capture intercepted an unrelated act")), \
                patch("capture_language.parse", side_effect=AssertionError("capture intercepted an unrelated prefix")):
            for english in (False, True):
                self.assertEqual(Z.parse(Z.render(document, self.ctx, english), self.ctx), document)
            for text, english in (("versions go;", False), ("Version space go.", True),
                                  ("learn go;", False), ("Learn go.", True),
                                  ("pipeline go;", False), ("Pipeline go.", True)):
                try:
                    Z.parse_statement(text, self.ctx, english)
                except Refusal:
                    pass

    def test_capture_refuses_reference_dependent_or_incomplete_expressions(self):
        for expression in ("keep", "out0", "?c", "$C", "unknown", "(" * 400 + "x0" + ")" * 400):
            source = Z.render(Z.Document(self.ctx.subject_ref, "model", ()), self.ctx) + \
                f"capture p1 out0 = {expression};\n"
            with self.assertRaises(Refusal, msg=expression[:40]):
                Z.parse(source, self.ctx)

    def test_mcp_has_no_owner_reference_or_answer_tool(self):
        names = {t["name"] for t in mcp.TOOLS if t["name"].startswith("zal_capture_")}
        self.assertEqual(names, {"zal_capture_read", "zal_capture_packet", "zal_capture_submit",
                                 "zal_capture_translate", "zal_capture_legend"})
        self.assertFalse(any("answer" in name or "open" in name or "status" in name for name in names))

    def test_capture_terminal_refusal_uses_exit_one(self):
        output = io.StringIO()
        with patch.object(W.CaptureWorkspace, "public_packet", side_effect=Refusal("unavailable")), \
                patch("sys.stdout", output):
            code = W.terminal(["show", "--workspace", "unused.json"])
        self.assertEqual(code, 1)
        self.assertEqual(json.loads(output.getvalue())["status"], "refused")

    def test_public_packet_never_contains_owner_session_paths_or_table(self):
        with tempfile.TemporaryDirectory() as folder:
            workspace = W.CaptureWorkspace(Path(folder) / "capture.json", cli="unused")
            saved = {"schema": W.SCHEMA, "session": "SECRET-PRIVATE-SESSION", "tag": "capture", "revision": "a" * 64,
                     "abi": self.abi, "events": []}
            workspace.save(saved)
            packet = {"profile": Z.PROFILE, "abi": self.abi, "round": 0, "examples": []}
            with patch.object(workspace, "packet_data", return_value=packet):
                rendered = json.dumps(workspace.public_packet())
            self.assertNotIn("SECRET", rendered)
            self.assertNotIn("table", rendered)
            self.assertEqual(set(json.loads(rendered)["packet"]), {"profile", "abi", "round", "examples"})

    def test_model_cannot_submit_owner_answer_or_judge_capture_claim(self):
        with tempfile.TemporaryDirectory() as folder:
            workspace = W.CaptureWorkspace(Path(folder) / "capture.json", cli="unused")
            workspace.save({"schema": W.SCHEMA, "session": "private", "tag": "capture", "revision": "a" * 64,
                            "abi": self.abi, "events": []})
            for author, act in (("owner", Z.act("answer", "q1", choice="any")),
                                ("model", Z.act("answer", "q1", choice="any")),
                                ("model", Z.act("captured-exact", "p1", tuples=8)),
                                ("judge", Z.act("captured-exact", "p1", tuples=8))):
                source = Z.render(Z.Document(self.ctx.subject_ref, author, (act,)), self.ctx)
                with patch.object(workspace, "cli", side_effect=AssertionError("must not route")):
                    with self.assertRaises(Refusal):
                        workspace.submit(source)

    def test_mocked_cli_feedback_cycle_retains_both_surfaces_without_private_evidence(self):
        with tempfile.TemporaryDirectory() as folder:
            workspace = W.CaptureWorkspace(Path(folder) / "capture.json", cli="unused")
            workspace.save({"schema": W.SCHEMA, "session": "PRIVATE-SESSION", "tag": "capture", "revision": "a" * 64,
                            "abi": self.abi, "events": []})
            before = {"profile": Z.PROFILE, "abi": self.abi, "round": 0, "examples": []}
            after = {**before, "round": 1, "examples": [{"input": [0, 1], "output": [1]}]}
            source = Z.render(Z.Document(self.ctx.subject_ref, "model", (Z.act("capture", "p1", output="out0",
                                          expr=Z.read("false", self.ctx, False)),)), self.ctx)
            reply = {"status": "refuted", "detail": {"precheck": "examples-equivalent", "SECRET-SOURCE": "private"}}
            with patch.object(workspace, "cli", return_value=Path("unused")), \
                    patch.object(workspace, "packet_data", side_effect=[before, after]), \
                    patch.object(W.D, "run", return_value=(1, reply)):
                events = workspace.submit(source)
            self.assertEqual(len(events), 2)
            self.assertNotIn("SECRET", json.dumps(events))
            for event in events:
                for surface in ("symbolic", "english"):
                    Z.parse(event[surface], self.ctx)
            self.assertEqual(len(workspace.load()["events"]), 2)
            forged = {"status": "captured-exact", "detail": {"checked": 8,
                       "check": {"verdict": "equivalent", "replay": "differs"}}}
            with patch.object(workspace, "cli", return_value=Path("unused")), \
                    patch.object(workspace, "packet_data", return_value=after), \
                    patch.object(W.D, "run", return_value=(0, forged)):
                with self.assertRaises(Refusal):
                    workspace.submit(source)
            self.assertEqual(len(workspace.load()["events"]), 2)


if __name__ == "__main__":
    unittest.main()
