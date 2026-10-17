"""Pure-Python leak controls; no cargo, native CLI or private knowledge fixtures."""
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile
from contextlib import redirect_stdout
import check_knowledge_files as K
import check_release_privacy as P
from check_release_privacy import Scan
import rc_package


class KnowledgeFiles(unittest.TestCase):
    def fixture(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        root = Path(temp.name)
        for name in (K.EXAMPLE, K.INCLUDES):
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes((K.ROOT / name).read_bytes())
        return root

    def test_full_tree_has_exactly_the_public_example(self):
        self.assertEqual(K.check(K.ROOT), [])
        self.assertEqual(K.check(self.fixture()), [])

    def test_untracked_ignored_hidden_and_case_variant_files_are_refused_unread(self):
        for name in ("other.kb", ".hidden/other.kb", "target/other.kb", "data/OTHER.KB", "data/strategy-example.kb"):
            root = self.fixture()
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("opaque test bytes")
            self.assertIn("kb-unlisted-file", K.check(root))
            scan = Scan()
            with patch.object(Path, "open", side_effect=AssertionError("unlisted bytes read")):
                scan.file(path, name)
            self.assertEqual(scan.report()["status"], "failed")

    def test_only_the_pinned_neutral_contents_can_use_the_example_name(self):
        root = self.fixture()
        (root / K.EXAMPLE).write_text("custom(a).")
        self.assertIn("kb-example-content-mismatch", K.check(root))
        scan = Scan()
        scan.inspect(K.EXAMPLE, b"custom(a).")
        self.assertEqual(scan.report()["status"], "failed")
        root = self.fixture()
        policy = json.loads((root / K.INCLUDES).read_text())
        policy["files"]["other.kb"] = "0" * 64
        (root / K.INCLUDES).write_text(json.dumps(policy))
        self.assertIn("kb-includes-mismatch", K.check(root))
        with patch.object(K, "public_content", return_value=True):
            changed = self.fixture()
            (changed / K.EXAMPLE).write_text("custom(a).")
            self.assertNotIn("kb-example-content-mismatch", K.check(changed))
        self.assertIn("kb-example-content-mismatch", K.check(changed))

    def test_symlink_and_directory_names_are_refused(self):
        root = self.fixture()
        (root / "alias.kb").symlink_to(root / K.EXAMPLE)
        self.assertIn("kb-unlisted-file", K.check(root))
        (root / "dir.kb").mkdir()
        self.assertIn("kb-unlisted-directory", K.check(root))
        example = root / K.EXAMPLE
        example.unlink()
        example.symlink_to(root / K.INCLUDES)
        self.assertIn("kb-example-nonregular", K.check(root))

    def test_archives_check_each_name_before_content_deduplication(self):
        data = (K.ROOT / K.EXAMPLE).read_bytes()
        for name in ("other.kb", "elsewhere/strategy-example.kb", "../" + K.EXAMPLE, "/" + K.EXAMPLE, K.EXAMPLE.upper()):
            scan = Scan()
            scan.inspect(K.EXAMPLE, data)
            scan.inspect("archive!" + name, data)
            self.assertEqual(scan.report()["status"], "failed", name)
        for name in (K.EXAMPLE, "source-2.4.0/" + K.EXAMPLE,
                     "zeno-fcis-loop-doors-2.4.0/examples/strategy-example.kb"):
            scan = Scan()
            scan.inspect("archive!" + name, data)
            self.assertEqual(scan.report()["status"], "passed", name)
        bundle = io.BytesIO()
        with zipfile.ZipFile(bundle, "w") as archive:
            archive.writestr("other.kb", data)
        scan = Scan()
        scan.inspect("bundle.zip", bundle.getvalue())
        self.assertEqual(scan.report()["status"], "failed")

    def test_packaging_guard_fails_before_cargo_and_archive_extraction(self):
        root = self.fixture()
        (root / "other.kb").write_text("opaque test bytes")
        with patch.object(rc_package, "ROOT", root), patch.object(rc_package, "cargo_metadata", side_effect=AssertionError("cargo called")):
            with self.assertRaisesRegex(rc_package.RcError, "include policy"):
                rc_package.validate_package_set()
        for name, data in [("other.kb", b"opaque"), ("examples/strategy-example.kb", b"changed"),
                           ("examples/strategy-example.kb", (K.ROOT / K.EXAMPLE).read_bytes())]:
            temp = self.fixture()
            archive_path = temp / "zeno-fcis-loop-doors-2.4.0.crate"
            with tarfile.open(archive_path, "w:gz") as archive:
                member = tarfile.TarInfo("zeno-fcis-loop-doors-2.4.0/" + name)
                member.size = len(data)
                archive.addfile(member, io.BytesIO(data))
            destination = temp / "extract"
            destination.mkdir()
            if data == (K.ROOT / K.EXAMPLE).read_bytes():
                rc_package.extract_checked_crate(archive_path, destination)
                self.assertEqual(len(list(destination.rglob("*.kb"))), 1)
            else:
                with self.assertRaises(rc_package.RcError):
                    rc_package.extract_checked_crate(archive_path, destination)
                self.assertEqual(list(destination.rglob("*.kb")), [])

    def test_location_and_content_comparison_controls_fail_when_stubbed(self):
        def control():
            self.assertFalse(K.listed_location("other.kb"))
            self.assertFalse(K.public_content(b"changed"))
        control()
        with patch.object(K, "listed_location", return_value=True), self.assertRaises(AssertionError):
            control()
        with patch.object(K, "public_content", return_value=True), self.assertRaises(AssertionError):
            control()

    def test_privacy_entrypoint_includes_untracked_files_and_preserves_explicit_example(self):
        root = self.fixture()
        output = io.StringIO()
        with patch.object(P, "ROOT", root), patch("sys.argv", ["privacy", "check", str(root / K.EXAMPLE)]), redirect_stdout(output):
            self.assertEqual(P.main(), 0)
        self.assertEqual(json.loads(output.getvalue())["status"], "passed")
        (root / ".ignored.kb").write_text("opaque bytes")
        output = io.StringIO()
        with patch.object(P, "ROOT", root), patch("sys.argv", ["privacy", "check"]), \
                patch.object(P.subprocess, "check_output", return_value=K.EXAMPLE.encode() + b"\0"), redirect_stdout(output):
            self.assertEqual(P.main(), 1)
        self.assertEqual(json.loads(output.getvalue())["errors"][0]["code"], "kb-unlisted-file")


if __name__ == "__main__":
    unittest.main()
