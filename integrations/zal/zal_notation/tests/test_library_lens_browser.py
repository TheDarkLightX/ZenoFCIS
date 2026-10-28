"""Actual generic producer/replay controls; no imported HTML/browser execution."""
from copy import deepcopy
from hashlib import sha256
import json
import io
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

from zal_notation import abstractions as A, core, library_lens_browser as B
from zal_notation.tests.generic_browser_fixture import request


class GenericBrowserTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.workspace = tempfile.TemporaryDirectory(prefix="generic-lens-tests-")
        cls.root = Path(cls.workspace.name)
        cls.request = request()
        cls.result = B.export(cls.request, cls.root / "base")
        cls.pin = cls.result["request_sha256"]
        cls.packet = json.loads((cls.root / "base/data/packet.json").read_text())

    @classmethod
    def tearDownClass(cls):
        cls.workspace.cleanup()

    def subject(self, name):
        return next(s for s in self.packet["subjects"] if s["object"]["id"] == name)

    def view(self, subject, lens):
        return next(v for v in self.subject(subject)["lenses"] if v["id"] == lens)

    def test_five_actual_subjects_and_generic_bounds(self):
        self.assertEqual([s["object"]["id"] for s in self.packet["subjects"]],
                         ["draft", "custom-term", "custom-graph", "counter", "phase"])
        phase = self.view("phase", "state-machine")
        self.assertEqual(phase["status"], "checked")
        self.assertTrue(any("3 declared states; 6 total rows" in line for line in phase["bounds"]))
        self.assertFalse(any("Four declared states" in line for line in phase["bounds"]))
        self.assertEqual(self.view("phase", "unit-group")["operations"]["carrier"].__len__(), 3)

    def test_draft_and_authored_proposal_are_usable_without_finite_lowering(self):
        draft = self.subject("draft")
        self.assertEqual(draft["object"]["meaning"]["status"], "draft")
        self.assertIn("\r\n", draft["object"]["_sourceText"])
        self.assertTrue(all(v["status"] != "checked" for v in draft["lenses"]))
        proposal = self.view("draft", "authored-proposal-0")
        self.assertEqual(proposal["status"], "proposed")
        self.assertEqual(proposal["proposal"]["evidence"], "author-declared; not checked")
        self.assertIn("unproved", proposal["targetEntry"]["story"])

    def test_structure_checked_never_settles_custom_meaning(self):
        term = self.view("custom-term", "inductive-syntax")
        graph = self.view("custom-graph", "relational-graph")
        self.assertEqual((term["status"], term["evidenceScope"]), ("checked", "structure"))
        self.assertEqual((graph["status"], graph["evidenceScope"]), ("checked", "structure"))
        self.assertEqual(self.subject("custom-term")["object"]["meaning"]["status"], "draft")
        self.assertIn("no execution-tree", term["_receipt"]["receipt"]["evidence"]["claim"])
        self.assertEqual(self.view("custom-term", "unit-group")["status"], "unsupported")

    def test_quotient_fibers_and_retained_source_are_distinct(self):
        quotient = self.view("counter", "parity-quotient")
        self.assertEqual(quotient["status"], "checked")
        self.assertNotEqual(quotient["sourceContextDigest"], quotient["targetContextDigest"])
        fibers = {}
        for pair in quotient["mapping"]:
            fibers.setdefault(pair["target"], []).append(pair["source"])
        self.assertEqual(set(fibers["even"]), {"unseen", "two"})
        self.assertEqual(set(fibers["odd"]), {"one", "three"})
        self.assertEqual(self.view("counter", "retained-source")["story"], self.subject("counter")["object"]["_sourceText"])

    def test_actual_effect_table_composition_inverse_and_word_loss(self):
        ops = self.view("counter", "transformation-monoid")["operations"]
        units = self.view("counter", "unit-group")["operations"]
        self.assertEqual(len(ops["elements"]), 8)
        self.assertEqual(len(units["carrier"]), 4)
        inc, reset = (ops["generators"][name] for name in ('["inc","yes"]', '["reset","yes"]'))
        self.assertNotEqual(ops["multiplication"][inc][reset], ops["multiplication"][reset][inc])
        self.assertNotIn(reset, units["carrier"])
        self.assertIsNotNone(units["inverses"][str(inc)])
        self.assertEqual(ops["period_example"]["nonempty_word_steps"], 4)
        self.assertIn("not", ops["scope"])
        self.assertEqual(self.view("counter", "automorphism-group")["status"], "unsupported")

    def test_refuted_and_incomplete_receipts_keep_their_actual_status(self):
        refuted = self.view("counter", "all-effects-group")
        incomplete = self.view("counter", "one-object-category")
        self.assertEqual(refuted["status"], "refuted")
        self.assertEqual(incomplete["status"], "incomplete")
        self.assertEqual(refuted["_receipt"]["receipt"]["status"], "refuted")
        self.assertEqual(incomplete["_receipt"]["receipt"]["status"], "incomplete")
        self.assertIsNone(refuted["operations"])
        self.assertIsNone(incomplete["operations"])

    def test_replay_reconstructs_actual_bytes_deterministically(self):
        report = B.replay(self.root / "base", self.pin)
        self.assertEqual(report["status"], "replayed")
        second = self.root / "second"
        B.export(self.request, second)
        before = {p.relative_to(self.root / "base"): p.read_bytes() for p in (self.root / "base").rglob("*") if p.is_file()}
        after = {p.relative_to(second): p.read_bytes() for p in second.rglob("*") if p.is_file()}
        self.assertEqual(before, after)

    def test_source_type_schema_and_budget_refusals_before_output(self):
        for name, mutate in [
            ("future-schema", lambda x: x.update(schema="zal.library-lens-browser-request/999")),
            ("boolean-budget", lambda x: x["checks"][0].update(check_budget=True)),
            ("numeric-source-id", lambda x: x["entries"].__setitem__(0, 1)),
            ("unknown-field", lambda x: x.update(html="<script>bad()</script>")),
            ("duplicate-entry", lambda x: x["entries"].append(x["entries"][0])),
            ("forged-map-type", lambda x: x["proposals"][0]["correspondence"]["pairs"][0].update(source=True)),
        ]:
            with self.subTest(name=name):
                value = deepcopy(self.request); mutate(value)
                output = self.root / name
                with self.assertRaises((core.NotationError, TypeError, ValueError)):
                    B.export(value, output)
                self.assertFalse(output.exists())

    def test_fractional_presentation_is_allowed_but_semantic_profile_stays_exact(self):
        value = deepcopy(self.request)
        entry = value['library']['entries'][2]
        primitive = entry['discussion']['notation']['glyphs']['state']['primitives'][0]
        primitive['x'] = 8.5
        value['entries'][2] = core.digest(entry)
        A.validate_library(value['library'])
        B.validate_request(value)
        # Parsed presentation floats do not preserve their original decimal lexeme.
        semantic = deepcopy(self.request)
        semantic['library']['entries'][2]['discussion']['context']['semantics']['parameters']['quantity'] = 0.5
        with self.assertRaisesRegex(core.NotationError, 'semantic context numbers'):
            B.validate_request(semantic)

    def test_packet_source_catalog_target_receipt_and_asset_mutations_are_rejected_even_resealed(self):
        base = self.root / "base"
        cases = ["source", "catalog", "target", "receipt", "status-scope", "browser-js", "browser-html"]
        for name in cases:
            with self.subTest(name=name):
                folder = self.root / ("mutated-" + name); shutil.copytree(base, folder)
                packet_path = folder / "data/packet.json"
                packet = json.loads(packet_path.read_text())
                counter = next(s for s in packet["subjects"] if s["object"]["id"] == "counter")
                lens = next(v for v in counter["lenses"] if v["id"] == "parity-quotient")
                if name == "source": counter["object"]["entry"]["story"] += "changed"
                if name == "catalog": counter["catalog"]["lenses"].pop()
                if name == "target": lens["_receipt"]["application"]["payload"]["target_context"]["observation"]["omits"] = []
                if name == "receipt": lens["_receipt"]["receipt"]["status"] = "checked-by-author"
                if name == "status-scope": lens["evidenceScope"] = "structure"
                packet_path.write_text(json.dumps(packet))
                if name == "browser-js": (folder / "app.js").write_text("throw Error('imported executable');")
                if name == "browser-html": (folder / "index.html").write_text("<script>require('fs').writeFileSync('EXECUTED','wrong')</script>")
                manifest = json.loads((folder / "manifest.json").read_text())
                manifest["files"] = {str(p.relative_to(folder)): sha256(p.read_bytes()).hexdigest()
                                     for p in folder.rglob("*") if p.is_file() and p.name != "manifest.json"}
                (folder / "manifest.json").write_text(json.dumps(manifest))
                with self.assertRaisesRegex(core.NotationError, "reconstructed artifact differs"):
                    B.replay(folder, self.pin)
                self.assertFalse((folder / "EXECUTED").exists())

    def test_request_pin_symlinks_inventory_and_fresh_only_exports(self):
        folder = self.root / "mutated-request"; shutil.copytree(self.root / "base", folder)
        value = json.loads((folder / "request.json").read_text())
        value["library"]["entries"][0]["story"] += "different subject"
        (folder / "request.json").write_text(json.dumps(value))
        with self.assertRaisesRegex(core.NotationError, "expected request/source pin"):
            B.replay(folder, self.pin)
        extra = self.root / "extra"; shutil.copytree(self.root / "base", extra)
        (extra / "unreferenced-import.html").write_text("<script>bad()</script>")
        with self.assertRaisesRegex(core.NotationError, "inventory differs"):
            B.replay(extra, self.pin)
        linked = self.root / "linked"; shutil.copytree(self.root / "base", linked)
        (linked / "app.js").unlink(); (linked / "app.js").symlink_to(self.root / "base/app.js")
        with self.assertRaisesRegex(core.NotationError, "symlink"):
            B.replay(linked, self.pin)
        with self.assertRaisesRegex(core.NotationError, "new directory"):
            B.export(self.request, self.root / "base")

    def test_export_cli_bounds_file_read_and_refuses_before_decode(self):
        calls = []
        class BoundedStream(io.BytesIO):
            def read(self, size=-1):
                calls.append(size)
                if size != B.MAX_REQUEST + 1:
                    raise AssertionError('CLI did not use its exact bounded read')
                return super().read(size)
        class InputPath:
            def __init__(self, name):
                self.name = name

            def open(self, mode):
                if mode != 'rb':
                    raise AssertionError('request must be read as bytes')
                return BoundedStream(b'\xff' * (B.MAX_REQUEST + 20))
        with patch.object(B, 'Path', InputPath), patch('sys.argv', ['lens-browser', 'export', 'oversized.json', 'unused-output']):
            with self.assertRaisesRegex(core.NotationError, 'byte budget'):
                B.main()
        self.assertEqual(calls, [B.MAX_REQUEST + 1])


if __name__ == "__main__":
    unittest.main(verbosity=2)
