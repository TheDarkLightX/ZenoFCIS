import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from behavior import Refusal
from mcp import Bridge
from server import Workspace
from shared import SharedWorkspace
from terminal import ReviewClient, human_text
from test_behavior import SOURCE
from workflow import checker_identity, examples_digest
from workspace import export, verify


class ReviewedExportTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.path = Path(self.temp.name) / "workspace.json"
        self.destination = Path(self.temp.name) / "declarations"
        self.store = SharedWorkspace(self.path)
        self.store.initialize(SOURCE)
        self.client = ReviewClient(self.store)

    def tearDown(self):
        self.temp.cleanup()

    def accept_current(self):
        self.client.present(self.client.execute("show"), lambda text: None)
        self.client.execute("accept-current")
        return self.store.load().model.revision

    def test_initial_review_requires_completed_exact_display(self):
        before = self.path.read_bytes()
        with self.assertRaisesRegex(Refusal, "displayed"):
            self.client.execute("accept-current")
        result = self.client.execute("show")
        with self.assertRaises(OSError):
            self.client.present(result, lambda text: (_ for _ in ()).throw(OSError("Output failed")))
        with self.assertRaises(Refusal):
            self.client.execute("accept-current")
        result = self.client.execute("show")
        result["english"] = "Changed display"
        with self.assertRaises(Refusal):
            self.client.present(result, lambda text: None)
        self.assertEqual(self.path.read_bytes(), before)
        revision = self.accept_current()
        event = self.store.load().events[-1]
        self.assertEqual(event["data"]["candidate_revision"], revision)
        self.assertEqual(event["data"]["checker"], checker_identity())
        with self.assertRaises(Refusal):
            self.client.execute("accept-current")

    def test_intervening_event_during_display_invalidates_initial_review(self):
        original_use = self.store.use
        interleave = True
        def changed_after_unlock(action):
            nonlocal interleave
            result = original_use(action)
            if interleave:
                interleave = False
                original_use(lambda s: s.select("state:pending", s.model.revision))
            return result
        with patch.object(self.store, "use", side_effect=changed_after_unlock):
            result = self.client.execute("show")
        self.client.present(result, lambda text: None)
        before = self.path.read_bytes()
        with self.assertRaises(Refusal):
            self.client.execute("accept-current")
        self.assertEqual(self.path.read_bytes(), before)

    def test_read_only_help_preserves_binding_but_proposal_and_cancel_do_not(self):
        self.client.present(self.client.execute("show"), lambda text: None)
        self.client.execute("help")
        self.client.execute("translate symbolic")
        self.client.execute("accept-current")
        self.client.present(self.client.execute("show"), lambda text: None)
        self.store.use(lambda s: s.propose_human(SOURCE.replace("not_enabled", "not_allowed"),
                                               "symbolic", s.model.revision))
        with self.assertRaises(Refusal):
            self.client.execute("accept-current")
        self.client.execute("cancel")
        with self.assertRaises(Refusal):
            self.client.execute("accept-current")

    def test_unreviewed_wrong_revision_and_pending_proposal_refuse_without_export(self):
        revision = self.store.load().model.revision
        with self.assertRaises(Refusal):
            export(self.path, revision, self.destination)
        self.accept_current()
        for requested in ["stale", revision]:
            if requested == revision:
                self.store.use(lambda s: s.propose_human(SOURCE.replace("not_enabled", "not_allowed"),
                                                       "symbolic", s.model.revision))
            before = self.path.read_bytes()
            with self.assertRaises(Refusal):
                export(self.path, requested, self.destination)
            self.assertEqual(self.path.read_bytes(), before)
            self.assertFalse(self.destination.exists())

    def test_export_binds_all_declarations_and_receipt_and_does_not_change_session(self):
        revision = self.accept_current()
        before = self.path.read_bytes()
        receipt = export(self.path, revision, self.destination)
        self.assertEqual(receipt["status"], "exported-reviewed-declarations")
        self.assertEqual(receipt["checker"], checker_identity())
        self.assertEqual(receipt["factory_qualification"], "not-run")
        self.assertEqual(receipt["authority"], "none")
        self.assertEqual(verify(self.path, revision, self.destination)["status"], "verified-reviewed-declarations")
        self.assertEqual(self.path.read_bytes(), before)
        for path in sorted(p for p in self.destination.rglob("*") if p.is_file()):
            with self.subTest(file=path.name):
                original = path.read_bytes()
                path.write_bytes(original + b" ")
                with self.assertRaises(Refusal):
                    verify(self.path, revision, self.destination)
                path.write_bytes(original)
                path.unlink()
                with self.assertRaises(Refusal):
                    verify(self.path, revision, self.destination)
                path.write_bytes(original)
        extra = self.destination / "unexpected.txt"
        extra.write_text("extra")
        with self.assertRaises(Refusal):
            verify(self.path, revision, self.destination)
        extra.unlink()
        extra.mkdir()
        with self.assertRaises(Refusal):
            verify(self.path, revision, self.destination)
        extra.rmdir()
        link = self.destination / "link"
        link.symlink_to(self.path)
        with self.assertRaises(Refusal):
            verify(self.path, revision, self.destination)

    def test_legacy_or_stale_checker_approval_requires_explicit_review_again(self):
        revision = self.accept_current()
        for binding in [None, "old-checker"]:
            def change(session):
                if binding is None:
                    session.events[-1]["data"].pop("checker", None)
                else:
                    session.events[-1]["data"]["checker"] = binding
            self.store.use(change)
            before = self.path.read_bytes()
            with self.assertRaisesRegex(Refusal, "binding"):
                export(self.path, revision, self.destination)
            self.assertFalse(self.destination.exists())
            self.assertEqual(self.path.read_bytes(), before)
            self.accept_current()
        export(self.path, revision, self.destination)
        self.assertEqual(verify(self.path, revision, self.destination)["revision"], revision)

    def test_changed_candidate_approval_exports_only_new_meaning(self):
        old = self.accept_current()
        self.store.use(lambda s: s.propose_human(SOURCE.replace("not_enabled", "not_allowed"),
                                               "symbolic", s.model.revision))
        self.client.present(self.client.execute("candidate"), lambda text: None)
        self.client.execute("accept")
        new = self.store.load().model.revision
        self.assertNotEqual(new, old)
        with self.assertRaises(Refusal):
            export(self.path, old, self.destination)
        receipt = export(self.path, new, self.destination)
        self.assertEqual(receipt["revision"], new)
        self.assertIn("not_allowed", (self.destination / "project.zeno").read_text())

    def test_failed_checks_block_review_and_unresolved_meaning_blocks_export(self):
        for suffix, blocked in [("invariant never_cancelled: !(state == cancelled);\n", True),
                                ("unresolved fairness: pending requests eventually resolve;\n", False)]:
            with self.subTest(suffix=suffix), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "workspace.json"
                destination = Path(directory) / "declarations"
                store = SharedWorkspace(path)
                store.initialize(SOURCE + suffix)
                client = ReviewClient(store)
                client.present(client.execute("show"), lambda text: None)
                if blocked:
                    before = path.read_bytes()
                    with self.assertRaises(Refusal):
                        client.execute("accept-current")
                    self.assertEqual(path.read_bytes(), before)
                else:
                    client.execute("accept-current")
                    self.assertEqual(store.load().agreement, "human-accepted")
                with self.assertRaises(Refusal):
                    export(path, store.load().model.revision, destination)
                self.assertFalse(destination.exists())

    def supply_simulated_label(self, client=None, target="approved", provenance="simulated owner input fixture"):
        return (client or self.client).execute(
            f'label simulated_allow pending approve authorized=yes expect Accept {target} - "{provenance}"')

    def test_simulated_owner_author_explain_trace_change_and_recovery_journey(self):
        self.supply_simulated_label()
        self.client.execute('label simulated_deny pending approve authorized=no expect Reject pending not_enabled "simulated denial fixture"')
        original = self.store.load().owner_examples
        digest = examples_digest(original)
        explanation = self.client.execute("why pending approve authorized=yes")
        self.assertEqual(explanation["supplied_assumptions"]["observations"], {"authorized": True})
        self.assertEqual(explanation["authenticated_facts"], [])
        self.assertIn("Authenticated facts: none", human_text(explanation))
        trace = self.client.execute("trace submit authorized=no then approve authorized=yes")
        self.assertEqual([row["supplied_assumptions"]["observations"]["authorized"] for row in trace], [False, True])
        self.assertTrue(all(row["authenticated_facts"] == [] for row in trace))
        bridge = Bridge(self.store)
        revision = self.store.load().model.revision
        weakened = SOURCE.replace("[authorized]", "[!authorized]")
        comparison = bridge.call("zal_compare", {"revision": revision, "syntax": "symbolic", "surface": weakened})["owner_examples"]
        self.assertTrue(all(case["current"]["status"] == "matched" and case["candidate"]["status"] == "conflict" for case in comparison["cases"]))
        self.assertEqual([case["expected"] for case in comparison["cases"]], [case["expected"] for case in original])
        self.store.use(lambda s: s.propose_human(weakened, "symbolic", revision))
        self.client.present(self.client.execute("candidate"), lambda text: self.assertIn("candidate: conflict", text))
        self.client.execute("reject")
        self.assertEqual(self.store.load().owner_examples, original)
        equivalent = SOURCE.replace("[authorized]", "[authorized & true]")
        self.store.use(lambda s: s.propose_human(equivalent, "symbolic", revision))
        self.client.present(self.client.execute("candidate"), lambda text: self.assertIn("simulated owner input fixture", text))
        self.client.execute("accept")  # Simulated terminal gesture, not evidence of a real person.
        reloaded = self.store.load()
        self.assertEqual(reloaded.owner_examples, original)
        self.assertEqual(reloaded.events[-1]["data"]["examples_digest"], digest)
        self.assertEqual(reloaded.freshness()["review"]["status"], "current")
        receipt = export(self.path, reloaded.model.revision, self.destination)
        self.assertEqual(receipt["examples_digest"], digest)
        self.assertEqual(receipt["owner_examples"]["cases"], reloaded.example_comparison()["cases"])
        self.assertEqual(receipt["factory_qualification"], "not-run")
        self.assertEqual(verify(self.path, reloaded.model.revision, self.destination)["authority"], "none")
        read = bridge.call("zal_read", {})
        self.assertEqual(read["authenticated_facts"], [])
        self.assertEqual(read["supplied_assumptions"]["owner_examples"], original)

    def test_retained_owner_case_is_out_of_domain_without_relabeling(self):
        self.supply_simulated_label()
        original = self.store.load().owner_examples
        self.store.use(lambda s: s.propose_human(SOURCE.replace("pending", "queued"), "symbolic", s.model.revision))
        view = self.store.load().view()
        case = view["proposal"]["example_comparison"]["cases"][0]
        self.assertEqual(case["current"]["status"], "matched")
        self.assertEqual(case["candidate"]["status"], "out-of-domain")
        self.client.present(self.client.execute("candidate"), lambda text: self.assertIn("out-of-domain", text))
        self.client.execute("reject")
        self.assertEqual(self.store.load().owner_examples, original)
        view["owner_examples"][0]["expected"]["state"] = "cancelled"
        self.assertEqual(self.store.load().owner_examples, original)

    def test_owner_examples_are_bounded_explicit_and_do_not_mutate_on_refusal(self):
        for line in [
            'label simulated_allow pending approve authorized=1 expect Accept approved - "simulated"',
            'label simulated_allow pending approve authorized=yes expect Accept approved "simulated"',
            'label simulated_allow pending approve authorized=yes expect Accept approved inferred "simulated"',
            'label simulated_allow pending approve authorized=yes expect Reject cancelled refused "simulated"',
            'label simulated_allow pending approve authorized=yes expect Accept approved - ""',
            'label simulated_allow pending approve authorized=yes expect Accept approved - "' + 'x' * 1025 + '"',
        ]:
            before = self.path.read_bytes()
            with self.subTest(line=line[:100]), self.assertRaises(Refusal):
                self.client.execute(line)
            self.assertEqual(self.path.read_bytes(), before)
        for number in range(64):
            self.client.execute(f'label simulated_{number} pending approve authorized=yes expect Accept approved - "simulated bounded fixture"')
        before = self.path.read_bytes()
        with self.assertRaisesRegex(Refusal, "at most 64"):
            self.client.execute('label simulated_overflow pending approve authorized=yes expect Accept approved - "simulated"')
        self.assertEqual(self.path.read_bytes(), before)

    def test_interleaved_label_change_requires_a_new_current_display(self):
        self.supply_simulated_label()
        self.client.present(self.client.execute("show"), lambda text: self.assertIn("simulated_allow", text))
        other = ReviewClient(self.store)
        self.supply_simulated_label(other, provenance="simulated corrected provenance")
        before = self.path.read_bytes()
        with self.assertRaisesRegex(Refusal, "changed"):
            self.client.execute("accept-current")
        self.assertEqual(self.path.read_bytes(), before)
        self.assertFalse(any(event["act"] == "agree" for event in self.store.load().events))
        self.accept_current()
        self.assertEqual(self.store.load().freshness()["review"]["status"], "current")
        self.supply_simulated_label(other, target="cancelled", provenance="simulated changed expectation")
        before = self.path.read_bytes()
        self.assertEqual(self.store.load().freshness()["review"]["status"], "stale")
        with self.assertRaisesRegex(Refusal, "corpus binding"):
            export(self.path, self.store.load().model.revision, self.destination)
        self.assertEqual(self.path.read_bytes(), before)

    def test_stale_browser_corpus_cannot_accept_an_identical_restaged_candidate(self):
        browser = Workspace(SOURCE, self.path.parent, self.path)
        candidate = SOURCE.replace("not_enabled", "not_allowed")
        self.store.use(lambda s: s.propose_human(candidate, "symbolic", s.model.revision))
        old = browser.view()
        self.supply_simulated_label()
        self.store.use(lambda s: s.propose_human(candidate, "symbolic", s.model.revision))
        before = self.path.read_bytes()
        with self.assertRaisesRegex(Refusal, "Displayed owner-example corpus changed"):
            browser.action("accept", {"base_revision": old["model"]["revision"],
                "candidate_revision": old["proposal"]["revision"], "examples_digest": old["examples_digest"]})
        self.assertEqual(self.path.read_bytes(), before)
        fresh = browser.view()
        browser.action("accept", {"base_revision": fresh["model"]["revision"],
            "candidate_revision": fresh["proposal"]["revision"], "examples_digest": fresh["examples_digest"]})
        self.assertEqual(self.store.load().owner_examples[0]["expected"]["state"], "approved")

    def test_changed_corpus_blocks_pending_accept_before_any_agreement_write(self):
        self.supply_simulated_label()
        self.store.use(lambda s: s.propose_human(SOURCE.replace("not_enabled", "not_allowed"), "symbolic", s.model.revision))
        self.supply_simulated_label(provenance="simulated newer label provenance")
        preview = self.store.load().view()
        self.assertEqual(preview["freshness"]["proposal"]["corpus"], "stale")
        self.client.present(self.client.execute("candidate"), lambda text: self.assertIn("owner corpus: stale", text))
        before = self.path.read_bytes()
        with self.assertRaisesRegex(Refusal, "Owner examples changed"):
            self.client.execute("accept")
        self.assertEqual(self.path.read_bytes(), before)

    def test_old_empty_workspace_loads_but_does_not_reuse_old_checker_review(self):
        self.accept_current()
        saved = json.loads(self.path.read_text())
        data = saved["session"]
        for key in ["owner_examples", "examples_digest", "example_comparison", "freshness", "supplied_assumptions", "authenticated_facts"]:
            data.pop(key, None)
        data["evidence"]["checker"] = "simulated-legacy-checker"
        data["events"][-1]["data"].pop("examples_digest")
        data["events"][-1]["data"]["checker"] = "simulated-legacy-checker"
        self.path.write_text(json.dumps(saved))
        before = self.path.read_bytes()
        with patch("workflow.check", side_effect=AssertionError("Read must preserve stored checks")):
            view = Bridge(self.store).call("zal_read", {})
            self.assertEqual(view["owner_examples"], [])
            self.assertEqual(view["evidence"], data["evidence"])
            self.assertEqual(view["freshness"]["checks"]["status"], "stale")
            self.assertEqual(view["freshness"]["review"]["status"], "stale")
        self.assertEqual(self.path.read_bytes(), before)
        with self.assertRaisesRegex(Refusal, "binding"):
            export(self.path, view["model"]["revision"], self.destination)
        self.assertEqual(self.path.read_bytes(), before)
        self.accept_current()
        self.assertEqual(self.store.load().evidence["checker"], checker_identity())
        self.assertEqual(self.store.load().freshness()["checks"]["status"], "current")

    def test_process_source_drift_preview_is_read_only_and_requires_restart(self):
        before = self.path.read_bytes()
        with patch("workflow._disk_checker_identity", return_value="simulated-changed-source"), \
                patch("workflow.check", side_effect=AssertionError("Preview must not refresh checks")):
            view = Bridge(self.store).call("zal_read", {})
            self.assertEqual(view["freshness"]["checks"]["status"], "unavailable")
            self.assertIn("Restart", view["freshness"]["checks"]["guidance"])
            text = human_text(self.client.execute("show"))
            self.assertIn("Checks freshness: unavailable", text)
            with self.assertRaisesRegex(Refusal, "restart"):
                self.client.execute("accept-current")
        self.assertEqual(self.path.read_bytes(), before)

    def test_missing_displayed_corpus_digest_refuses_even_for_empty_labels(self):
        before = self.path.read_bytes()
        with self.assertRaisesRegex(Refusal, "Displayed owner-example corpus"):
            self.store.use(lambda s: s.accept_current_human(s.model.revision))
        self.assertEqual(self.path.read_bytes(), before)
        self.store.use(lambda s: s.propose_human(SOURCE.replace("not_enabled", "not_allowed"), "symbolic", s.model.revision))
        preview = self.store.load().view()
        browser = Workspace(SOURCE, self.path.parent, self.path)
        before = self.path.read_bytes()
        with self.assertRaisesRegex(Refusal, "Displayed owner-example corpus"):
            browser.action("accept", {"base_revision": preview["model"]["revision"],
                "candidate_revision": preview["proposal"]["revision"]})
        self.assertEqual(self.path.read_bytes(), before)


if __name__ == "__main__":
    unittest.main()
