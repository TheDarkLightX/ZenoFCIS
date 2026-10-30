"""The 2.7.1 design documents stay precise: required sections, real repository references, scoped claims."""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]
DESIGNS = {
    "docs/PURITY_GENERATED_APPLICATIONS_27.md": {
        "sections": ["The journey that fails today", "Trust rule", "Behaviour", "Acceptance", "Scope and what stays open"],
        "mentions": ["[patch.crates-io]", "manifest-patch-refused", "ZENO_FCIS_BUILD_TREE", "check_resolved_paths"],
    },
    "docs/APPLICATION_BACKUP_RESTORE_27.md": {
        "sections": ["What exists and what is missing", "Commands", "Verification", "Scope"],
        "mentions": ["zeno-fcis/app-backup/1", "delivery_id", "--resume-deliveries", "DATABASE.submissions"],
    },
    "docs/zal/APPLICATION_BRIDGE.md": {
        "sections": ["The user story", "Assessment", "Design: one qualified bridge for finite models that send messages",
                     "Route status for the rest of ZAL", "Next steps and required evidence", "Scope", "Lineage"],
        "mentions": ["zal/app-effects/1", "zal/effect-plan/1", "CHANNEL@ORDINAL:IDEMPOTENCY", "zal/typed-fsm/1"],
    },
}
FORBIDDEN = re.compile(r"correct by construction|/(?:home|tmp)/|Claude|Codex|GPT|Opus|Sonnet|Astra|Luna|Sol 6", re.I)


class DesignDocuments(unittest.TestCase):
    def test_required_sections_and_terms_are_present(self):
        for relative, expected in DESIGNS.items():
            text = (ROOT / relative).read_text()
            headings = re.findall(r"^## (.+)$", text, re.M)
            with self.subTest(document=relative):
                self.assertEqual(headings, expected["sections"])
                self.assertIn("Status: design only", text)
                for term in expected["mentions"]:
                    self.assertIn(term, text)

    def test_repository_references_exist(self):
        for relative in DESIGNS:
            text = (ROOT / relative).read_text()
            references = set(re.findall(r"`((?:crates|tools|docs|release)/[A-Za-z0-9_./-]+)`", text))
            self.assertTrue(references, relative)
            for reference in sorted(references):
                with self.subTest(document=relative, reference=reference):
                    self.assertTrue((ROOT / reference).exists(), reference)

    def test_claims_stay_scoped_and_public(self):
        for relative in DESIGNS:
            text = (ROOT / relative).read_text()
            with self.subTest(document=relative):
                self.assertIsNone(FORBIDDEN.search(text), FORBIDDEN.search(text))
                self.assertRegex(text, r"(?m)^## Scope")
                self.assertIn("not a proof", text, "state what the design does not establish")

    def test_designs_are_indexed_and_in_the_reviewed_inventory(self):
        index = (ROOT / "docs/INDEX.md").read_text()
        inventory = (ROOT / "release/public-documents.txt").read_text().splitlines()
        for relative in DESIGNS:
            with self.subTest(document=relative):
                self.assertIn(Path(relative).name, index)
                self.assertIn(relative, inventory)

    def test_gap_register_names_both_items(self):
        register = (ROOT / "docs/FACTORY_GAPS_27.md").read_text()
        self.assertIn("| F27-28 |", register)
        self.assertIn("| F27-30 |", register)


if __name__ == "__main__":
    unittest.main()
