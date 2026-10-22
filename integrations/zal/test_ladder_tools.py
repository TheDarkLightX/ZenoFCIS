"""Revision-bound ladder tools and the explanation tools through the MCP bridge and terminal."""

import copy
import hashlib
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from behavior import Refusal, check, parse
import explain as E
import ladder as l
import ladder_tools as T
from mcp import Bridge, TOOLS
from shared import SharedWorkspace
from terminal import ReviewClient, command, human_text


EXAMPLES = Path(__file__).with_name("examples")
SOURCE = (EXAMPLES / "order.zal").read_text()
NEW_TOOLS = ["zal_why_not", "zal_what_if", "zal_narrate", "zal_guide", "zal_ladder_read", "zal_ladder_render",
             "zal_ladder_parse", "zal_ladder_same", "zal_ladder_lower", "zal_ladder_explain"]
REFUSALS = (Refusal, l.Refusal)
VAULT = "example:temporal/vault_drain"
DEX = "example:ladders/dex_k"


def revisions():
    return {item["id"]: item["revision"] for item in T.read_catalogue()["statements"]}


class LadderToolTests(unittest.TestCase):
    def setUp(self):
        self.bridge = Bridge(None)
        self.revisions = revisions()

    def call(self, name, **arguments):
        return self.bridge.call(name, arguments)

    def test_catalogue_lists_every_example_with_exact_revisions(self):
        read = self.call("zal_ladder_read")
        self.assertEqual((read["schema"], read["authority"], read["project_directory"]), ("zal/ladder-tools/1", "none", "unset"))
        statements = {item["id"]: item for item in read["statements"]}
        expected = sorted("example:" + path.parent.name + "/" + path.stem for folder in ("ladders", "temporal")
                          for path in (EXAMPLES / folder).glob("*.json"))
        self.assertEqual(sorted(statements), expected)
        for sid, item in statements.items():
            folder, stem = sid.removeprefix("example:").split("/")
            data = json.loads((EXAMPLES / folder / (stem + ".json")).read_text())
            canonical = json.dumps(data, ensure_ascii=True, sort_keys=True, separators=(",", ":"))
            self.assertEqual(item["revision"], hashlib.sha256(b"zal/ladder-statement/1\0" + canonical.encode()).hexdigest())
            self.assertEqual(item["status"], "admitted")
            self.assertEqual(item["profile"], l.TEMPORAL_PROFILE if folder == "temporal" else l.PROFILE)
            self.assertEqual(item["scope_identity"] is not None, folder == "temporal")

    def test_render_is_the_canonical_ladder_for_every_example_and_rung(self):
        rendered = 0
        for sid, revision in self.revisions.items():
            s = T.load(sid, revision)
            full = self.call("zal_ladder_render", statement=sid, revision=revision)
            self.assertEqual(full["text"], l.ladder_text(s.tree, s.ctx))
            self.assertEqual(list(full["rungs"]), list(l.RUNGS))
            self.assertEqual(full["authority"], "none")
            for rung in l.RUNGS:
                one = self.call("zal_ladder_render", statement=sid, revision=revision, rung=rung)
                self.assertEqual((one["rungs"], one["text"]), ({rung: l.render(s.tree, rung, s.ctx)}, l.ladder_text(s.tree, s.ctx, rung)))
                self.assertEqual(one["scope"], full["scope"])
                rendered += 1
        vault = self.call("zal_ladder_render", statement=VAULT, revision=self.revisions[VAULT])
        self.assertEqual(vault["rungs"]["plain"], "No admitted operation sequence drains the vault.")
        self.assertEqual(vault["rungs"]["formula"], "∀σ ∈ Traces : ∀a ∈ Assets : □(¬Loss(a))")
        self.assertEqual(vault["evidence"]["evidence"], "obligation")
        self.assertEqual([item["field"] for item in vault["scope"]][:6],
                         ["source_revision", "event_schema", "order", "projection", "mode", "horizon"])
        dex = self.call("zal_ladder_render", statement=DEX, revision=self.revisions[DEX])
        self.assertEqual(dex["rungs"]["plain"], "No accepted swap decreases the reserve product.")
        self.assertEqual(dex["evidence"]["evidence"], "fixture tables only")
        print(f"LADDER TOOLS: {len(self.revisions)} statements; {rendered} single-rung renderings equal the canonical ladder")

    def test_parse_gives_one_normalized_tree_or_a_named_refusal(self):
        parsed = 0
        for sid, revision in self.revisions.items():
            s = T.load(sid, revision)
            for rung in l.RUNGS:
                text = l.render(s.tree, rung, s.ctx)
                result = self.call("zal_ladder_parse", statement=sid, revision=revision, rung=rung, text=text)
                self.assertTrue(result["same_as_statement"], (sid, rung))
                self.assertEqual(result["normalized"], l.plain_data(l.normal(s.tree, s.ctx)))
                self.assertTrue(result["uniqueness"]["unique"])
                if rung != "question":
                    self.assertEqual(result["uniqueness"]["counts"]["normalized_meanings"], 1)
                parsed += 1
        result = self.call("zal_ladder_parse", statement=DEX, revision=self.revisions[DEX], rung="typed",
                           text="forall t in T: S(t) => x'(t)*y'(t) >= x(t)*y(t)")
        self.assertEqual(result["uniqueness"]["sentence"], "The typed grammar gave 1 complete derivation, 1 distinct syntax tree "
                         "and 1 well-typed formula, with exactly one expanded normalized meaning.")
        refusals = {("typed", "forall t in T: S(t) =>"): "syntax at token",
                    ("precise", "No accepted swap decreases the reserve product."): "syntax at token",
                    ("typed", "forall t in T: Q(t)"): "unknown-symbol:Q"}
        for (rung, text), name in refusals.items():
            with self.assertRaises(REFUSALS) as caught:
                self.call("zal_ladder_parse", statement=DEX, revision=self.revisions[DEX], rung=rung, text=text)
            self.assertIn(name, str(caught.exception))
        with self.assertRaises(REFUSALS) as caught:
            self.call("zal_ladder_parse", statement=VAULT, revision=self.revisions[VAULT], rung="typed",
                      text="forall sigma in Traces: forall a in Assets: within(4, not Loss(a))")
        self.assertIn("temporal:within-horizon", str(caught.exception))
        print(f"LADDER PARSE: {parsed} rung texts parse back to one normalized meaning; named refusals kept")

    def test_same_reports_meaning_with_its_evidence(self):
        vault, dex = self.revisions[VAULT], self.revisions[DEX]
        same = self.call("zal_ladder_same", statement=VAULT, revision=vault, rung="plain",
                         text="No admitted operation sequence drains the vault.")
        self.assertEqual((same["status"], same["same"]), ("same", True))
        self.assertEqual(same["evidence"]["scope_identity"]["left"], same["evidence"]["scope_identity"]["right"])
        different = self.call("zal_ladder_same", statement=VAULT, revision=vault, rung="typed",
                              text="forall sigma in Traces: forall a in Assets: always(Loss(a))")
        self.assertEqual(different["status"], "different-normal-forms")
        self.assertEqual(different["evidence"]["first_difference"]["path"], "root/2/2/1")
        self.assertIn("does not show the meanings differ", different["summary"])
        crossed = self.call("zal_ladder_same", statement=VAULT, revision=vault, other_statement=DEX, other_revision=dex)
        self.assertEqual((crossed["status"], crossed["same"]), ("not-compared", None))
        with tempfile.TemporaryDirectory() as directory:
            data = json.loads((EXAMPLES / "temporal/vault_drain.json").read_text())
            data["trace"]["horizon"] = 5
            (Path(directory) / "longer.json").write_text(json.dumps(data))
            with patch.dict(os.environ, {"ZAL_LADDER_DIR": directory}):
                longer = revisions()["project:longer"]
                result = self.call("zal_ladder_same", statement=VAULT, revision=vault, other_statement="project:longer",
                                   other_revision=longer)
                self.assertEqual(result["status"], "same-formula-different-scope")
                self.assertIsNone(result["evidence"]["first_difference"])
        for arguments in [{}, {"rung": "typed"}, {"other_statement": DEX}, {"rung": "typed", "text": "x", "other_statement": DEX,
                                                                            "other_revision": dex}]:
            with self.assertRaises(REFUSALS):
                self.call("zal_ladder_same", statement=VAULT, revision=vault, **arguments)

    def test_lower_is_inert_with_obligations(self):
        result = self.call("zal_ladder_lower", statement=VAULT, revision=self.revisions[VAULT])
        self.assertEqual((result["status"], result["authority"]), ("inert-claim", "none"))
        self.assertTrue(result["source"].startswith("zeno 1; project 1 ladder_temporal;"))
        self.assertIn("execution initialization, transition membership and post/pre continuity", result["obligations"])
        with self.assertRaises(REFUSALS) as caught:
            self.call("zal_ladder_lower", statement=DEX, revision=self.revisions[DEX])
        self.assertIn("lowering:unsupported-theory", str(caught.exception))

    def test_explain_names_the_rung_uniqueness_and_scope(self):
        vault = self.revisions[VAULT]
        result = self.call("zal_ladder_explain", statement=VAULT, revision=vault, text="No admitted operation sequence drains the vault.")
        self.assertEqual(result["status"], "admitted")
        self.assertEqual(result["summary"][:3], [
            "Rung used: plain (intuitive + leftmost-outermost definition folding).",
            "Why the parse is unique: The plain grammar gave 1 complete derivation, 1 distinct syntax tree and 1 well-typed formula, "
            "with exactly one expanded normalized meaning.",
            "It has the same normalized meaning as the statement's formula."])
        self.assertEqual([a["rung"] for a in result["attempts"] if a["admitted"]], ["plain"])
        self.assertTrue(all(a.get("refusal") for a in result["attempts"] if not a["admitted"]))
        self.assertIn("Scope identity: a hash of every item above plus observation and definition versions; any change gives a "
                      "new identity, and a hash is an identity, not a proof.", result["summary"])
        own = self.call("zal_ladder_explain", statement=DEX, revision=self.revisions[DEX])
        self.assertEqual((own["own_formula"], own["attempts"][0]["rung"]), (True, "typed"))
        refused = self.call("zal_ladder_explain", statement=DEX, revision=self.revisions[DEX], rung="precise", text="Nothing here.")
        self.assertEqual(refused["status"], "refused")
        self.assertTrue(refused["summary"][1].startswith("At precise: syntax at token"))

    def test_scope_meanings_cover_every_scope_field(self):
        fields = set()
        for sid, revision in self.revisions.items():
            for item in T.scope_strip(T.load(sid, revision).ctx):
                self.assertTrue(item["meaning"])
                fields.add(item["field"])
        trace_keys = set(json.loads((EXAMPLES / "temporal/vault_drain.json").read_text())["trace"])
        self.assertLessEqual(trace_keys, set(T.SCOPE_MEANINGS))
        self.assertEqual(set(T.SCOPE_MEANINGS), fields - {"scope"})
        self.assertEqual(set(T.RUNG_MEANINGS), set(l.RUNGS))

    def test_stale_ladder_revision_is_refused(self):
        stale = "0" * 64
        vault = self.revisions[VAULT]
        calls = [("zal_ladder_render", {}), ("zal_ladder_parse", {"rung": "typed", "text": "forall sigma in Traces: always(not Loss())"}),
                 ("zal_ladder_same", {"rung": "typed", "text": "forall sigma in Traces: always(not Loss())"}),
                 ("zal_ladder_lower", {}), ("zal_ladder_explain", {})]
        for name, extra in calls:
            with self.subTest(tool=name):
                with self.assertRaises(REFUSALS) as caught:
                    self.call(name, statement=VAULT, revision=stale, **extra)
                self.assertEqual(str(caught.exception), T.STALE)
        with self.assertRaises(REFUSALS) as caught:
            self.call("zal_ladder_same", statement=VAULT, revision=vault, other_statement=VAULT, other_revision=stale)
        self.assertEqual(str(caught.exception), T.STALE)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "edited.json"
            data = json.loads((EXAMPLES / "temporal/vault_drain.json").read_text())
            path.write_text(json.dumps(data))
            with patch.dict(os.environ, {"ZAL_LADDER_DIR": directory}):
                before = revisions()["project:edited"]
                self.assertEqual(before, vault)
                data["readings"][0]["plain"] = "nothing drains the vault"
                path.write_text(json.dumps(data))
                with self.assertRaises(REFUSALS) as caught:
                    self.call("zal_ladder_render", statement="project:edited", revision=before)
                self.assertEqual(str(caught.exception), T.STALE)
                after = revisions()["project:edited"]
                self.assertNotEqual(after, before)
                self.assertEqual(self.call("zal_ladder_render", statement="project:edited", revision=after)["rungs"]["plain"],
                                 "Nothing drains the vault.")
        reply = self.bridge.handle({"jsonrpc": "2.0", "id": 9, "method": "tools/call",
                                    "params": {"name": "zal_ladder_render", "arguments": {"statement": VAULT, "revision": stale}}})
        self.assertTrue(reply["isError"])
        self.assertEqual(reply["content"][0]["text"], T.STALE)

    def test_project_directory_is_bounded_and_read_only(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            (folder / "broken.json").write_text("{")
            (folder / "Upper.json").write_text("{}")
            os.symlink(EXAMPLES / "temporal/vault_drain.json", folder / "linked.json")
            before = {p.name: p.read_bytes() for p in folder.iterdir() if not p.is_symlink()}
            with patch.dict(os.environ, {"ZAL_LADDER_DIR": directory}):
                read = {item["id"]: item for item in T.read_catalogue()["statements"]}
                self.assertEqual(read["project:broken"]["status"], "refused")
                self.assertEqual(read["project:linked"]["status"], "refused")
                self.assertNotIn("project:Upper", read)
            self.assertEqual({p.name: p.read_bytes() for p in folder.iterdir() if not p.is_symlink()}, before)
            with patch.dict(os.environ, {"ZAL_LADDER_DIR": str(folder / "missing")}):
                with self.assertRaises(REFUSALS):
                    T.read_catalogue()


class ExplainBridgeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.path = Path(self.temp.name) / "workspace.json"
        self.store = SharedWorkspace(self.path)
        self.store.initialize(SOURCE)
        self.bridge = Bridge(self.store)
        self.revision = self.bridge.call("zal_read", {})["model"]["revision"]

    def tearDown(self):
        self.temp.cleanup()

    def test_tool_list_appends_new_tools_without_authority(self):
        names = [tool["name"] for tool in TOOLS]
        self.assertEqual(names[17:], NEW_TOOLS)
        for tool in TOOLS[17:]:
            self.assertFalse(any(word in tool["name"] for word in ("answer", "say", "accept")))
            self.assertTrue("no agreement" in tool["description"] or "Authority: none" in tool["description"], tool["name"])

    def test_explanation_tools_are_revision_bound_and_read_only(self):
        before = self.path.read_bytes()
        base = {"revision": self.revision, "state": "pending", "event": "approve", "context": {"authorized": False}}
        why = self.bridge.call("zal_why_not", {**base, "want": {"class": "Accept"}})
        self.assertEqual(why["minimal_changes"][0]["rule"], "approve_order")
        what = self.bridge.call("zal_what_if", {**base, "change": {"context": {"authorized": True}}})
        self.assertEqual(what["after"]["outcome"]["state"], "approved")
        story = self.bridge.call("zal_narrate", {"revision": self.revision, "source": "trace",
                                                 "inputs": [{"event": "submit", "context": {"authorized": False}}]})
        self.assertEqual(story["english"][-1], "After 1 step the state is pending.")
        self.assertEqual(self.bridge.call("zal_narrate", {"revision": self.revision, "source": "check"})["status"], "no-counterexample")
        self.assertEqual(self.bridge.call("zal_guide", {"revision": self.revision, "topic": "rule"})["schema"], "zal/help/2")
        for name, arguments in [("zal_why_not", {**base, "want": {"class": "Accept"}}),
                                ("zal_what_if", {**base, "change": {"context": {"authorized": True}}}),
                                ("zal_narrate", {"revision": self.revision, "source": "check"}),
                                ("zal_guide", {"revision": self.revision, "topic": ""})]:
            with self.assertRaises(REFUSALS) as caught:
                self.bridge.call(name, {**arguments, "revision": "stale"})
            self.assertIn("Stale revision", str(caught.exception))
        with self.assertRaises(REFUSALS):
            self.bridge.call("zal_narrate", {"revision": self.revision, "source": "candidate"})
        with self.assertRaises(REFUSALS):
            self.bridge.call("zal_why_not", {**base, "want": {"class": "Accept", "extra": "x"}})
        self.assertEqual(self.path.read_bytes(), before)

    def test_candidate_counterexample_is_narrated_for_review(self):
        surface = SOURCE + "rule another: pending + approve [true] -> accept cancelled;\n"
        self.bridge.call("zal_propose", {"base_revision": self.revision, "object_ids": ["rule:approve_order"],
                                         "message": "Test overlap", "syntax": "symbolic", "surface": surface})
        story = self.bridge.call("zal_narrate", {"revision": self.revision, "source": "candidate"})
        self.assertEqual((story["obligation"], story["candidate_revision"]), ("determinism", parse(surface).revision))
        self.assertEqual(story["english"][-1], "2 rules are enabled together, so the next state is not determined; "
                                               "this profile gives no precedence.")
        text = self.store.use(lambda session: human_text(command(session, "story candidate")))
        self.assertIn("4. 2 rules are enabled together", text)

    def test_terminal_commands_match_the_tools(self):
        def run(line):
            return self.store.use(lambda session: command(session, line))
        self.assertEqual(run("whynot pending approve authorized=no want Accept")["summary"],
                         self.bridge.call("zal_why_not", {"revision": self.revision, "state": "pending", "event": "approve",
                                                          "context": {"authorized": False}, "want": {"class": "Accept"}})["summary"])
        self.assertEqual(run("whynot draft approve authorized=no want Accept approved scope input")["distance"], 2)
        self.assertEqual(run("whynot pending approve authorized=no want rule approve_order")["status"], "found")
        self.assertEqual(run("whatif pending approve authorized=no change authorized=yes event:approve")["changes"][0],
                         {"input": "event", "from": "approve", "to": "approve", "effective": False})
        self.assertEqual(run("story trace submit authorized=no")["english"][-1], "After 1 step the state is pending.")
        self.assertEqual(run("guide not")["construct"], "!")
        self.assertEqual(run("ladder list")["kind"], "ladder-catalogue")
        parsed = run("ladder parse example:ladders/dex_k typed forall t in T: S(t) => x'(t)*y'(t) >= x(t)*y(t)")
        self.assertTrue(parsed["same_as_statement"])
        self.assertEqual(run("ladder same example:temporal/vault_drain example:temporal/vault_drain")["status"], "same")
        self.assertEqual(run("ladder explain example:temporal/vault_drain No admitted operation sequence drains the vault.")["status"],
                         "admitted")
        self.assertEqual(run("ladder render example:temporal/vault_drain plain")["rung"], "plain")
        self.assertEqual(run("ladder lower example:temporal/vault_drain")["status"], "inert-claim")
        for line in ["whynot pending approve authorized=no want Maybe", "whatif pending approve authorized=no change nope=yes",
                     "ladder render example:none", "ladder parse example:ladders/dex_k", "story sideways"]:
            with self.assertRaises(REFUSALS, msg=line):
                run(line)
        client = ReviewClient(self.store)
        text = human_text(client.execute("ladder parse example:ladders/dex_k typed forall t in T: S(t) => x'(t)*y'(t) >= x(t)*y(t)"))
        self.assertIn("Same meaning as the statement's formula.", text)


if __name__ == "__main__":
    unittest.main()
