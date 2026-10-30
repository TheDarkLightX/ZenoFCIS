"""The binding register lists every tracked source binding, and its check catches what it does not."""
from pathlib import Path
import json
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import binding_register as register  # noqa: E402

ROOT = register.ROOT
FIVE = ("record_arithmetic_proof_sources", "record_category_capsule", "record_core_app_references",
        "record_v1_baseline", "record_finite_library_components")


def git(root, *argv):
    subprocess.run(["git", "-C", str(root), *argv], check=True, capture_output=True)


class Planted:
    """A small tracked tree with one registered binding and a register that lists only it."""

    def __init__(self, test):
        temporary = tempfile.TemporaryDirectory()
        test.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        (self.root / "tools").mkdir()
        (self.root / "tools/record_thing.py").write_text("# producer\n")
        (self.root / "src").mkdir()
        (self.root / "src/thing.rs").write_text("fn main() {}\n")
        self.write("pins/thing.json", {"files": {"src/thing.rs": "0" * 64}})
        git(self.root, "init", "-q")
        git(self.root, "add", "-A")
        self.register = {"pins/thing.json": ("python3 tools/record_thing.py --write", "the planted binding")}

    def write(self, name, document):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(document, indent=2) + "\n")
        return path

    def check(self):
        return register.check(self.root, self.register)


class CurrentTree(unittest.TestCase):
    def test_every_tracked_binding_is_registered(self):
        self.assertEqual(register.check(), [])

    def test_the_five_named_producers_are_registered_with_their_bindings(self):
        producers = {row[0].split()[1] for row in register.REGISTER.values() if row[0] and row[0].startswith("python3 ")}
        for name in FIVE:
            with self.subTest(producer=name):
                self.assertIn(f"tools/{name}.py", producers)
        found = register.detect()
        for name in ("verification/verus/arithmetic-typed-proof.json", "test-data/v1-compatibility/baseline.json",
                     "integrations/zal/finite_library/UPSTREAM.json",
                     "integrations/zal/category_profile/provider/SOURCE_MANIFEST.json",
                     "integrations/zal/category_profile/DEPENDENCIES.json"):
            with self.subTest(binding=name):
                self.assertIn(name, found)
                self.assertIsNotNone(register.entry(name))

    def test_every_entry_has_a_producer_or_a_reason_that_names_why(self):
        for pattern, (producer, reason) in register.REGISTER.items():
            with self.subTest(pattern=pattern):
                self.assertGreater(len(reason), 20)
                if producer is None:
                    self.assertRegex(reason, r"record|reviewed|edited|history|template|re-integration",
                                     "a binding without a producer states how it changes")

    def test_fixture_and_example_trees_are_not_bindings(self):
        for name in register.detect():
            self.assertIsNone(register.NOT_BINDINGS.search(name), name)

    def test_cli_check_passes_and_list_prints_every_binding(self):
        check = subprocess.run([sys.executable, "-B", "tools/binding_register.py", "check"], cwd=ROOT, capture_output=True, text=True)
        self.assertEqual(check.returncode, 0, check.stdout)
        self.assertIn("binding-register: PASS", check.stdout)
        listed = subprocess.run([sys.executable, "-B", "tools/binding_register.py", "list"], cwd=ROOT, capture_output=True, text=True)
        rows = [json.loads(line) for line in listed.stdout.splitlines()]
        self.assertEqual({row["binding"] for row in rows}, set(register.detect()))
        self.assertNotIn("UNREGISTERED", {row["reason"] for row in rows})


class PlantedDefects(unittest.TestCase):
    def test_planted_tree_passes_before_any_defect(self):
        planted = Planted(self)
        self.assertEqual(planted.check(), [])

    def test_unregistered_binding_fails_the_check(self):
        planted = Planted(self)
        planted.write("pins/other.json", {"entries": [{"path": "src/thing.rs", "sha256": "1" * 64}]})
        git(planted.root, "add", "-A")
        problems = planted.check()
        self.assertEqual(len(problems), 1, problems)
        self.assertIn("unregistered binding: pins/other.json pins 1 tracked file(s)", problems[0])

    def test_untracked_binding_is_not_reported_and_pins_of_absent_files_are_not_bindings(self):
        planted = Planted(self)
        planted.write("pins/untracked.json", {"files": {"src/thing.rs": "2" * 64}})
        planted.write("pins/external.json", {"files": {"../elsewhere.rs": "3" * 64, "no/such/file": "4" * 64}})
        git(planted.root, "add", "pins/external.json")
        self.assertEqual(planted.check(), [])

    def test_stale_entry_and_missing_producer_fail_the_check(self):
        planted = Planted(self)
        planted.register["pins/gone.json"] = ("python3 tools/record_gone.py --write", "a binding that was removed")
        problems = planted.check()
        self.assertEqual(sorted(problems), sorted([
            "stale register entry: pins/gone.json matches no tracked binding",
            "producer missing: pins/gone.json names tools/record_gone.py",
        ]))

    def test_ancestor_relative_pins_are_resolved(self):
        planted = Planted(self)
        planted.write("pins/deep/nested.json", {"files": {"thing.json": "5" * 64}})
        git(planted.root, "add", "-A")
        found = register.detect(planted.root)
        self.assertIn("pins/deep/nested.json", found, "pins resolve against the binding's ancestors too")

    def test_sha256_beside_a_path_key_is_a_pin(self):
        planted = Planted(self)
        planted.write("pins/entry.json", {"certificate": {"file": "src/thing.rs", "source_sha256": "6" * 64}})
        git(planted.root, "add", "-A")
        self.assertEqual(register.detect(planted.root)["pins/entry.json"], 1)

    def test_fixture_directories_are_skipped(self):
        planted = Planted(self)
        planted.write("tests/fixtures/pins.json", {"files": {"src/thing.rs": "7" * 64}})
        planted.write("examples/pins.json", {"files": {"src/thing.rs": "8" * 64}})
        git(planted.root, "add", "-A")
        self.assertEqual(planted.check(), [])


if __name__ == "__main__":
    unittest.main()
