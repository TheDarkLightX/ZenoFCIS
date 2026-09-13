"""Small independent witnesses for publication privacy checks."""

import gzip
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
import zipfile

from check_release_privacy import Scan


class PrivacyTests(unittest.TestCase):
    def test_overlapping_identifiers_do_not_leave_email_fragments_in_locations(self):
        username = b"owner"
        email = b"owner+private@example.invalid"
        for markers in [(username, email), (email, username)]:
            scan = Scan(markers)
            scan.inspect("archive!" + email.decode(), b"safe")
            self.assertEqual(scan.report()["status"], "failed")
            self.assertNotIn("example.invalid", json.dumps(scan.report()))

    def test_home_directory_roots_are_private_without_a_trailing_separator(self):
        for home in [b"/home/" + b"private-owner", b"/Users/" + b"private-owner",
                     b"C:\\Users\\" + b"private-owner"]:
            for value in [home, b'{"home":"' + home + b'"}', home + b" other value"]:
                scan = Scan()
                scan.inspect("record", value)
                self.assertEqual(scan.report()["status"], "failed")
        for home in [b"/home/runner", b"/home/user", b"/Users/example"]:
            for value in [home, home + b"/project", b'{"home":"' + home + b'"}']:
                scan = Scan()
                scan.inspect("record", value)
                self.assertEqual(scan.report()["status"], "passed")

    def test_owner_identifiers_are_literal_private_and_scoped_to_each_scan(self):
        marker = b"owner+private@example.invalid"
        scan = Scan((marker,))
        scan.inspect("record!" + marker.decode(), marker)
        report = scan.report()
        self.assertEqual(report["status"], "failed")
        self.assertEqual(report["private_marker_count"], 1)
        self.assertNotIn(marker.decode(), json.dumps(report))
        unrelated = Scan()
        unrelated.inspect("record", marker)
        self.assertEqual(unrelated.report()["status"], "passed")
        literal = Scan((marker,))
        literal.inspect("record", b"ownerprivate@exampleXinvalid")
        self.assertEqual(literal.report()["status"], "passed")

    def test_private_marker_file_cli_and_environment_fail_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            marker_file = root / "private-identifiers"
            record = root / "record.json"
            marker = b"owner+private@example.invalid"
            marker_file.write_bytes(marker + b"\n")
            record.write_bytes(marker)
            command = [sys.executable, str(Path(__file__).with_name("check_release_privacy.py")), "check", str(record)]
            env = dict(os.environ, ZENO_FCIS_PRIVATE_MARKERS_FILE=str(marker_file))
            for options in [[], ["--private-markers-file", str(marker_file)]]:
                result = subprocess.run(command + options, env=env, capture_output=True, check=False)
                self.assertEqual(result.returncode, 1)
                self.assertNotIn(marker, result.stdout + result.stderr)
                self.assertEqual(json.loads(result.stdout)["findings"][0]["code"], "private-marker")
            record.write_bytes(b"safe")
            for invalid in [b"", b"abc\n", b"x" * 65537]:
                marker_file.write_bytes(invalid)
                result = subprocess.run(command, env=env, capture_output=True, check=False)
                self.assertEqual(result.returncode, 1)
                self.assertEqual(json.loads(result.stdout)["errors"][0]["code"], "invalid-private-markers")
            marker_file.unlink()
            result = subprocess.run(command, env=env, capture_output=True, check=False)
            self.assertEqual(result.returncode, 1)
            self.assertEqual(json.loads(result.stdout)["errors"][0]["code"], "invalid-private-markers")

    def test_private_markers_are_found_without_echoing_them(self):
        markers = [
            b"/home/" + b"private-owner/project/file",
            b"/Users/" + b"private-owner/project/file",
            b"C:\\Users\\" + b"private-owner\\project",
            b"ghp_" + b"a" * 40,
            b"sk-ant-" + b"a" * 50,
            b"AKIA" + b"A" * 16,
            b"-----BEGIN " + b"OPENSSH PRIVATE KEY-----",
            b"-----BEGIN " + b"ENCRYPTED PRIVATE KEY-----",
            b"Authorization: Bearer " + b"a" * 40,
        ]
        for marker in markers:
            with self.subTest(marker_number=markers.index(marker)):
                scan = Scan()
                scan.inspect("record.json", marker)
                self.assertEqual(scan.report()["status"], "failed")
                self.assertNotIn(marker.decode(), json.dumps(scan.report()))

    def test_public_placeholders_hashes_and_key_are_allowed(self):
        scan = Scan()
        scan.inspect("record", b"<toolchain>/bin/rustc /home/runner/work/project /home/user/example ssh-rsa AAAAB3NzaC1yc2E= " + b"a" * 64)
        self.assertEqual(scan.report()["status"], "passed")

    def test_nested_archives_and_compressed_logs_are_checked(self):
        marker = b"/home/" + b"private-owner/tool"
        tar_bytes = io.BytesIO()
        with tarfile.open(fileobj=tar_bytes, mode="w:gz") as archive:
            member = tarfile.TarInfo("metadata.json")
            member.size = len(marker)
            archive.addfile(member, io.BytesIO(marker))
        bundle = io.BytesIO()
        with zipfile.ZipFile(bundle, "w") as archive:
            archive.writestr("package.crate", tar_bytes.getvalue())
            archive.writestr("log.gz", gzip.compress(marker + b"\n"))
        scan = Scan()
        scan.inspect("bundle.zip", bundle.getvalue())
        self.assertEqual(len(scan.findings), 2)
        self.assertFalse(scan.errors)

    def test_member_names_are_checked_even_for_duplicate_bytes(self):
        scan = Scan()
        scan.inspect("safe", b"same")
        scan.inspect("archive!/home/" + "private-owner/record", b"same")
        self.assertEqual(scan.report()["status"], "failed")
        self.assertNotIn("private-owner", json.dumps(scan.report()))

    def test_empty_archive_directories_are_checked(self):
        bundle = io.BytesIO()
        with zipfile.ZipFile(bundle, "w") as archive:
            archive.writestr("/home/" + "private-owner/empty/", b"")
        scan = Scan()
        scan.inspect("bundle.zip", bundle.getvalue())
        self.assertEqual(scan.report()["status"], "failed")
        self.assertNotIn("private-owner", json.dumps(scan.report()))

    def test_compressed_tar_owner_and_extended_metadata_are_checked(self):
        for metadata in ["owner", "group", "member-comment", "global-comment"]:
            with self.subTest(metadata=metadata):
                bundle = io.BytesIO()
                marker = "private-\u00f6wner"
                headers = {"comment": marker} if metadata == "global-comment" else {}
                with tarfile.open(fileobj=bundle, mode="w:gz", pax_headers=headers) as archive:
                    if metadata != "global-comment":
                        member = tarfile.TarInfo("public-record.txt")
                        if metadata == "owner":
                            member.uname = marker
                        elif metadata == "group":
                            member.gname = marker
                        else:
                            member.pax_headers = {"comment": marker}
                        archive.addfile(member, io.BytesIO())
                scan = Scan((marker.encode(),))
                scan.inspect("bundle.tar.gz", bundle.getvalue())
                self.assertEqual(scan.report()["status"], "failed")
                self.assertNotIn(marker, json.dumps(scan.report(), ensure_ascii=False))

    def test_symlinks_are_not_followed(self):
        with tempfile.TemporaryDirectory() as directory:
            link = Path(directory) / "link"
            link.symlink_to("missing-private-file")
            scan = Scan()
            scan.file(link, "link")
            self.assertEqual(scan.errors[0]["code"], "not-a-regular-file")

    def test_private_file_names_are_rejected_even_without_recognizable_contents(self):
        scan = Scan()
        scan.inspect("archive!.env", b"opaque configuration")
        self.assertEqual(scan.findings[0]["code"], "private-file-name")

    def test_archive_depth_is_bounded(self):
        data = b"leaf"
        for level in range(6):
            bundle = io.BytesIO()
            with zipfile.ZipFile(bundle, "w") as archive:
                archive.writestr("leaf.txt" if level == 0 else "nested.zip", data)
            data = bundle.getvalue()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "nested.zip"
            path.write_bytes(data)
            scan = Scan()
            scan.file(path, path.name)
            self.assertEqual(scan.report()["status"], "failed")
            self.assertTrue(scan.errors)

    def test_oversized_compressed_input_fails_closed(self):
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "large.gz"
            path.write_bytes(gzip.compress(b"x" * 1024))
            with patch("check_release_privacy.MAX_FILE_BYTES", 128):
                scan = Scan()
                scan.file(path, path.name)
            self.assertEqual(scan.report()["status"], "failed")
            self.assertTrue(scan.errors)

    def test_invalid_or_unsupported_archives_fail_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            for name in ["broken.zip", "broken.crate", "broken.tar", "unsupported.xz"]:
                with self.subTest(name=name):
                    path = Path(directory) / name
                    path.write_bytes(b"uninspected compressed payload")
                    scan = Scan()
                    scan.file(path, name)
                    self.assertEqual(scan.report()["status"], "failed")
                    self.assertTrue(scan.errors)

    def test_archive_admission_is_not_skipped_for_repeated_bytes(self):
        scan = Scan()
        scan.inspect("record.txt", b"same bytes")
        with self.assertRaises(ValueError):
            scan.inspect("unsupported.xz", b"same bytes")

    def test_renaming_an_unsupported_archive_does_not_hide_it(self):
        import lzma
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "renamed.bin"
            path.write_bytes(lzma.compress(b"/home/" + b"private-owner/file"))
            scan = Scan()
            scan.file(path, path.name)
            self.assertEqual(scan.report()["status"], "failed")
            self.assertTrue(scan.errors)


if __name__ == "__main__":
    unittest.main()
