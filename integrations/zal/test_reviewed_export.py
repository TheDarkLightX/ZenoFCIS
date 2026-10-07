import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from behavior import Refusal
from shared import SharedWorkspace
from terminal import ReviewClient
from test_behavior import SOURCE
from workflow import checker_identity
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


if __name__ == "__main__":
    unittest.main()
