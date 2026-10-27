"""Small independent witnesses for publication privacy checks."""

import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import struct
import sys
import tarfile
import tempfile
import unittest
import zipfile

from check_release_privacy import PATTERNS, Scan, load_allowlist, match_fingerprint


class PrivacyTests(unittest.TestCase):
    def exception(self, location, data):
        match = PATTERNS["model-api-token"].search(data)
        self.assertIsNotNone(match)
        return (location, hashlib.sha256(data).hexdigest(), match_fingerprint(data, match))

    def test_duplicate_reads_fit_the_unique_byte_cap_and_new_bytes_still_refuse(self):
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name, data in [("one", b"abcd"), ("two", b"efg"), ("copy", b"abcd"), ("over", b"hijk")]:
                (root / name).write_bytes(data)
            with patch("check_release_privacy.MAX_FILE_BYTES", 6), patch("check_release_privacy.MAX_TOTAL_BYTES", 8):
                scan = Scan()
                for name in ("one", "two", "copy"):
                    scan.file(root / name, name)
                self.assertEqual(scan.report()["status"], "passed")
                self.assertEqual(scan.bytes, 7)
                scan.file(root / "over", "over")
                self.assertEqual(scan.report()["status"], "failed")
                self.assertEqual(scan.errors[0]["reason"], "byte-limit")

    def test_reviewed_literal_needs_exact_path_file_and_context(self):
        data = b"a" * 80 + b"sk-" + b"public_literal_" * 5 + b"z" * 80
        entry = self.exception("binaries/program", data)
        scan = Scan(allowlist=frozenset({entry}))
        scan.inspect(entry[0], data)
        self.assertEqual(scan.report()["status"], "passed")
        self.assertEqual(scan.report()["allowlisted_matches"], 1)
        for location, changed in [("other/program", data), (entry[0], b"b" + data[1:]),
                                  (entry[0], data[:79] + b"!" + data[80:]),
                                  (entry[0], data.replace(b"public", b"PUBLIC", 1))]:
            with self.subTest(location=location, same_bytes=changed == data):
                scan = Scan(allowlist=frozenset({entry}))
                scan.inspect(location, changed)
                self.assertEqual(scan.report()["status"], "failed")
        for changed_entry in [(entry[0], "0" * 64, entry[2]), (entry[0], entry[1], "0" * 64)]:
            scan = Scan(allowlist=frozenset({changed_entry}))
            scan.inspect(entry[0], data)
            self.assertEqual(scan.report()["status"], "failed")

    def test_refused_token_reports_only_hashes_and_offsets_for_review(self):
        token = b"sk-" + b"A" * 50
        data = b"prefix\n" + token + b"\nsuffix"
        scan = Scan()
        scan.inspect("binaries/program", data)
        report = scan.report()
        self.assertEqual(report["status"], "failed")
        self.assertEqual(report["allowlisted_matches"], 0)
        self.assertEqual(len(scan.findings), 1)
        finding = scan.findings[0]
        self.assertEqual(finding["file_sha256"], hashlib.sha256(data).hexdigest())
        self.assertEqual(finding["match_sha256"], hashlib.sha256(token).hexdigest())
        match = PATTERNS["model-api-token"].search(data)
        self.assertEqual(finding["fingerprint_sha256"], match_fingerprint(data, match))
        self.assertEqual((finding["offset"], finding["length"], finding["line"]), (7, 53, 2))
        self.assertNotIn(token.decode(), json.dumps(report))
        self.assertNotIn("prefix", json.dumps(report))

    def test_allowlisting_one_match_does_not_hide_another_secret_in_the_same_file(self):
        literal = b"sk-" + b"public_literal_" * 5
        secret = b"identifier_prefix_" + b"sk-" + b"A" * 50
        data = literal + b"\n" + secret
        scan = Scan(allowlist=frozenset({self.exception("release.txt", data)}))
        scan.inspect("release.txt", data)
        self.assertEqual(scan.report()["status"], "failed")
        self.assertEqual(scan.allowlisted_matches, 1)
        self.assertEqual(len(scan.findings), 1)
        self.assertEqual(scan.findings[0]["code"], "model-api-token")
        self.assertNotIn(secret.decode(), json.dumps(scan.report()))

    def test_copied_bytes_and_archives_do_not_inherit_another_paths_exception(self):
        data = b"sk-" + b"public_literal_" * 5
        scan = Scan(allowlist=frozenset({self.exception("allowed", data)}))
        scan.inspect("allowed", data)
        scan.inspect("copy", data)
        self.assertEqual(scan.bytes, len(data))
        self.assertEqual(scan.report()["status"], "failed")
        bundle = io.BytesIO()
        with tarfile.open(fileobj=bundle, mode="w:gz") as archive:
            member = tarfile.TarInfo("program")
            member.size = len(data)
            archive.addfile(member, io.BytesIO(data))
        scan = Scan(allowlist=frozenset({self.exception("allowed.tar.gz!program", data)}))
        scan.inspect("allowed.tar.gz", bundle.getvalue())
        unique = scan.bytes
        scan.inspect("copy.tar.gz", bundle.getvalue())
        self.assertEqual(scan.bytes, unique)
        self.assertEqual(scan.findings[0]["location"], "copy.tar.gz!program")

    def test_allowlist_is_bounded_exact_and_fail_closed(self):
        entry = self.exception("binaries/program", b"sk-" + b"public_literal_" * 5)
        record = dict(zip(("path", "file_sha256", "fingerprint_sha256"), entry), reason="Reviewed public literal.")
        document = {"schema": "zeno-fcis/release-privacy-allowlist/1", "context_bytes": 32, "entries": [record]}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "allowlist.json"
            path.write_text(json.dumps(document))
            self.assertEqual(load_allowlist(path), frozenset({entry}))
            for change in [{"path": "../program"}, {"path": "binaries/*"}, {"path": "/program"},
                           {"fingerprint_sha256": "bad"}, {"reason": ""}, {"extra": "field"}]:
                path.write_text(json.dumps({**document, "entries": [{**record, **change}]}))
                with self.assertRaises(ValueError):
                    load_allowlist(path)
            path.write_text(" " * 65537)
            with self.assertRaises(ValueError):
                load_allowlist(path)
            path.unlink()
            with self.assertRaises(ValueError):
                load_allowlist(path)

    def test_native_end_record_literal_still_refuses_invalid_zip_inspection(self):
        native = b"\x7fELF" + bytes(508)
        literal = struct.pack("<4s4H2LH", b"PK\x05\x06", 0, 0, 1, 1, 46, 0, 0)
        data = native + literal
        self.assertTrue(zipfile.is_zipfile(io.BytesIO(data)))
        with self.assertRaises(zipfile.BadZipFile):
            zipfile.ZipFile(io.BytesIO(data))
        with self.assertRaises(zipfile.BadZipFile):
            Scan().inspect("native-program", data)
        bundle = io.BytesIO()
        with tarfile.open(fileobj=bundle, mode="w:gz", format=tarfile.USTAR_FORMAT) as archive:
            member = tarfile.TarInfo("native-program")
            member.size = len(data)
            archive.addfile(member, io.BytesIO(data))
        with self.assertRaises(zipfile.BadZipFile):
            Scan().inspect("native.tar.gz", bundle.getvalue())

    def test_appended_valid_zip_is_checked_even_in_a_native_program(self):
        bundle = io.BytesIO(b"\x7fELF" + bytes(508))
        bundle.seek(0, io.SEEK_END)
        with zipfile.ZipFile(bundle, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            archive.writestr("private.txt", b"synthetic-private-marker")
        scan = Scan((b"synthetic-private-marker",))
        scan.inspect("native-program", bundle.getvalue())
        self.assertEqual(scan.report()["status"], "failed")
        self.assertTrue(any(finding["source"] == "content" for finding in scan.findings))

    def test_damaged_appended_zip_is_refused_even_in_a_native_program(self):
        bundle = io.BytesIO(b"\x7fELF" + bytes(508))
        bundle.seek(0, io.SEEK_END)
        with zipfile.ZipFile(bundle, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            archive.writestr("private.txt", b"synthetic-private-marker")
        damaged = bundle.getvalue().replace(b"PK\x01\x02", b"BROK", 1)
        self.assertNotIn(b"synthetic-private-marker", damaged)
        self.assertTrue(zipfile.is_zipfile(io.BytesIO(damaged)))
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "native-program"
            path.write_bytes(damaged)
            scan = Scan((b"synthetic-private-marker",))
            scan.file(path, path.name)
            report = scan.report()
            self.assertEqual(report["status"], "failed")
            self.assertEqual(report["errors"][0]["reason"], "BadZipFile")

    def test_malformed_intended_zip_cannot_use_native_literal_fallback(self):
        literal = struct.pack("<4s4H2LH", b"PK\x05\x06", 0, 0, 1, 1, 46, 0, 0)
        native = b"\x7fELF" + bytes(508) + literal
        for name, data in [("invalid.zip", native), ("renamed.bin", b"PK\x03\x04broken"),
                           ("renamed.bin", b"PK\x05\x06broken"),
                           ("renamed.bin", b"ordinary bytes" + literal)]:
            with self.subTest(name=name, prefix=data[:4]):
                with self.assertRaises((ValueError, zipfile.BadZipFile)):
                    Scan().inspect(name, data)

    def test_error_categories_are_bounded_and_do_not_echo_private_messages(self):
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "record.bin"
            path.write_bytes(b"safe")
            for error, reason in [(ValueError("byte-limit"), "byte-limit"),
                                  (zipfile.BadZipFile("synthetic-private-marker"), "BadZipFile")]:
                scan = Scan()
                with patch.object(scan, "inspect", side_effect=error):
                    scan.file(path, path.name)
                report = scan.report()
                self.assertEqual(report["errors"][0]["reason"], reason)
                self.assertNotIn("synthetic-private-marker", json.dumps(report))

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

    def test_file_uris_do_not_masquerade_as_windows_drive_paths(self):
        for uri in [b"file:///Users/runner/work/project", b"file:/Users/runner/work/project",
                    b"file:///home/runner/work/project"]:
            for value in [uri, json.dumps({"argv": [uri.decode()]}).encode()]:
                with self.subTest(value=value):
                    scan = Scan()
                    scan.inspect("journey.json", value)
                    self.assertEqual(scan.report()["status"], "passed")
        private = b"private-" + b"owner"
        for value in [b"file:///Users/" + private + b"/project",
                      b"C:/Users/" + private + b"/project",
                      b"c:/Users/" + private,
                      b"file:///C:/Users/" + private + b"/project",
                      b"C:" + bytes([92]) + b"Users" + bytes([92]) + private]:
            for encoded in [value, json.dumps({"path": value.decode()}).encode()]:
                with self.subTest(value=encoded):
                    scan = Scan()
                    scan.inspect("journey.json", encoded)
                    self.assertEqual(scan.report()["status"], "failed")

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
