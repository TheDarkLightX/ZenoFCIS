"""Hostile controls for tools/check_release_custody.py on synthetic sources and release sets, a real
packaged lockfile, the repository's ignore rules and the release workflow's evidence uploads."""

from __future__ import annotations

import gzip
import io
import json
import os
import shutil
import subprocess
import tarfile
import tempfile
import time
import unittest
from pathlib import Path

import check_release_custody as custody
import rc_package
import release_fixture as fixture
import workflow_contexts

DATA = Path(__file__).resolve().parent / "test_data" / "release"
# SHA-256 of the zeno-fcis-codec and zeno-fcis-value crate archives of the v2.3.0-rc.1 set.
RC1_CRATES = {"zeno-fcis-codec": "547606dd6a544b18e54f28cb2e37527bd636c67afb4b870392124d21b17eb398",
              "zeno-fcis-value": "f57efbe410726a168f94164ff5cd2038c6b06cee0861d72bec0bf35cb2a75f30"}
# Lockfiles that repository tools read, so each must be committed: build.py copies the certificate
# checker's, the fetch-locked-crates action reads the verification and resolved-purity ones, and the
# site, demo, benchmark and consumer projects build from theirs. The root Cargo.lock has been tracked
# from the start, so the generic ignore rule does not drop it.
COMMITTED_LOCKFILES = (
    "demos/mini-determinator-qemu/Cargo.lock", "docs/benchmarks/withdrawal-queue/Cargo.lock", "site/Cargo.lock",
    "test-projects/external-consumer/Cargo.lock", "tools/resolved-purity/Cargo.lock",
    "tools/symbolic_certificate_checker/Cargo.lock", "verification/Cargo.lock",
)


class CustodyTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="zeno-fcis-custody-test-")
        self.root = Path(self.temporary.name)
        self.source = fixture.Fixture(self.root)
        self.release = self.source.release(self.root / "release")
        self.downloads = self.root / "DOWNLOADS.sha256"
        self.downloads.write_text(rc_package.downloads_lines(self.release))

    def tearDown(self):
        self.temporary.cleanup()

    def renew(self) -> None:
        """A fresh source and release set inside one test."""
        self.tearDown()
        self.setUp()

    def check(self, **overrides) -> dict:
        arguments = {"downloads": self.downloads, "cargo_home": self.source.cargo_home, **overrides}
        if "bindings" not in arguments:
            arguments["bindings"] = self.source.bindings(self.release)
        return custody.check(self.release, self.source.repo, **arguments)

    def failed(self, report: dict) -> set[str]:
        return {name for name, item in report["checks"].items() if not item["passed"]}

    def reindex(self) -> None:
        rc_package.write_release_index(self.release, fixture.VERSION, self.source.commit)
        self.downloads.write_text(rc_package.downloads_lines(self.release))

    def test_a_complete_set_passes_every_check(self):
        report = self.check(tag=f"v{fixture.VERSION}", remote=self.source.commit)
        self.assertEqual(report["status"], "passed", json.dumps(report["checks"], indent=1)[:4000])
        self.assertEqual(len(report["checks"]), 11)
        self.assertEqual(report["checks"]["C7-compiled-inputs"]["counts"]["registry_inputs"], 2)

    def test_a_stat_only_index_rewrite_still_passes(self):
        index = self.source.repo / ".git" / "index"
        before = index.read_bytes()
        later = time.time() + 5
        for path in ("README.md", "crates/demo-core/src/lib.rs"):
            os.utime(self.source.repo / path, (later, later))
        subprocess.run(["git", "update-index", "--really-refresh"], cwd=self.source.repo, check=False,
                       stdout=subprocess.DEVNULL)
        self.assertNotEqual(index.read_bytes(), before)
        self.assertEqual(self.check()["status"], "passed")

    def test_untracked_and_unlisted_ignored_files_fail_source_state(self):
        (self.source.repo / "notes.txt").write_text("untracked\n")
        self.assertIn("C1-source-state", self.failed(self.check()))
        (self.source.repo / "notes.txt").unlink()
        (self.source.repo / "build.log").write_text("ignored, outside the allowlist\n")
        self.assertIn("C1-source-state", self.failed(self.check()))
        (self.source.repo / "build.log").unlink()
        (self.source.repo / "target").mkdir()
        (self.source.repo / "target" / "x").write_text("allowed\n")
        self.assertEqual(self.check()["status"], "passed")

    def test_a_working_tree_edit_or_mode_change_fails_source_state(self):
        (self.source.repo / "README.md").write_text("# Changed\n")
        self.assertIn("C1-source-state", self.failed(self.check()))
        subprocess.run(["git", "checkout", "--", "README.md"], cwd=self.source.repo, check=True)
        (self.source.repo / "crates/demo-cli/scripts/run.sh").chmod(0o644)
        self.assertIn("C1-source-state", self.failed(self.check()))

    def test_a_wrong_tag_or_remote_tag_fails(self):
        fixture.git(self.source.repo, "tag", "v0.0.1")
        self.assertIn("C2-tag-and-version", self.failed(self.check(tag="v0.0.1")))
        self.assertIn("C2-tag-and-version", self.failed(self.check(tag=f"v{fixture.VERSION}", remote="0" * 40)))

    def test_a_record_naming_another_commit_fails(self):
        path = self.release / "hosts" / fixture.MAC / "HOST-BINARY.json"
        record = json.loads(path.read_text())
        record["source_commit"] = "0" * 40
        rc_package.write_json(path, record)
        self.reindex()
        self.assertIn("C2-tag-and-version", self.failed(self.check()))

    def test_a_source_manifest_differing_from_the_blobs_fails(self):
        path = self.release / "SOURCE-MANIFEST.json"
        manifest = json.loads(path.read_text())
        manifest["files"][0]["sha256"] = "0" * 64
        rc_package.write_json(path, manifest)
        self.reindex()
        self.assertIn("C3-source-manifest", self.failed(self.check()))

    def rewrite_source_archive(self, edit) -> None:
        path = self.release / "source" / f"zeno-fcis-{fixture.VERSION}-source.tar.gz"
        tar = gzip.decompress(path.read_bytes())
        source = tarfile.open(fileobj=io.BytesIO(tar), mode="r:")
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w", format=tarfile.PAX_FORMAT,
                          pax_headers=source.pax_headers) as archive:
            for member in source.getmembers():
                data = source.extractfile(member).read() if member.isfile() else None
                member, data = edit(member, data)
                if member is not None:
                    archive.addfile(member, io.BytesIO(data) if data is not None else None)
        with path.open("wb") as raw, gzip.GzipFile(filename="", fileobj=raw, mode="wb", mtime=0) as compressed:
            compressed.write(buffer.getvalue())
        self.reindex()

    def test_a_source_archive_with_an_edited_member_fails(self):
        def edit(member, data):
            if member.name.endswith("README.md"):
                data = b"# Edited\n"
                member.size = len(data)
            return member, data
        self.rewrite_source_archive(edit)
        self.assertIn("C4-source-archive", self.failed(self.check()))

    def test_a_source_archive_with_a_gzip_name_fails(self):
        path = self.release / "source" / f"zeno-fcis-{fixture.VERSION}-source.tar.gz"
        tar = gzip.decompress(path.read_bytes())
        with path.open("wb") as raw, gzip.GzipFile(filename="named.tar", fileobj=raw, mode="wb", mtime=0) as named:
            named.write(tar)
        self.reindex()
        self.assertIn("C4-source-archive", self.failed(self.check()))

    def rebuild_crate(self, name: str, change) -> None:
        """Rebuild every crate in publication order, changing only `name`'s members."""
        crates = {}
        for package in ("demo-core", "demo-cli"):
            path = self.release / "packages" / f"{package}-{fixture.VERSION}.crate"
            members = self.source.crate_members(package, crates)
            if package == name:
                change(members)
            fixture.crate_archive(path, members, f"{package}-{fixture.VERSION}", frozenset({"scripts/run.sh"}))
            crates[package] = fixture.sha256(path.read_bytes())
        document = json.loads((self.release / "PACKAGED-APPLICATION.json").read_text())
        document["archives"] = [{"archive": f"{package}-{fixture.VERSION}.crate", "sha256": digest,
                                 "source_clean": True} for package, digest in sorted(crates.items())]
        rc_package.write_json(self.release / "PACKAGED-APPLICATION.json", document)
        self.reindex()

    def problems(self, report: dict, name: str) -> str:
        return "\n".join(report["checks"][name]["problems"])

    def test_an_edited_extra_or_dirty_crate_member_fails(self):
        controls = {
            "differs from crates/demo-core/src/lib.rs": lambda members: members.update({"src/lib.rs": b"//! edited\n"}),
            "is not a committed file": lambda members: members.update({"src/extra.rs": b"//! extra\n"}),
            ".cargo_vcs_info.json does not name": lambda members: members.update({".cargo_vcs_info.json": json.dumps(
                {"git": {"sha1": self.source.commit, "dirty": True}, "path_in_vcs": "crates/demo-core"}).encode()}),
            "keeps a path or git source": lambda members: members.update({"Cargo.toml": members["Cargo.toml"] +
                                                                         b'\n[dependencies.local]\npath = "../local"\n'}),
        }
        self.assertEqual(self.check()["status"], "passed")
        for expected, change in controls.items():
            with self.subTest(expected):
                self.renew()
                self.rebuild_crate("demo-core", change)
                report = self.check()
                self.assertEqual(self.failed(report), {"C5-crate-archives"}, expected)
                self.assertIn(expected, self.problems(report, "C5-crate-archives"))

    def test_a_crate_lock_not_naming_this_sets_crates_fails(self):
        def change(members):
            members["Cargo.lock"] = members["Cargo.lock"].replace(b'name = "demo-core"', b'name = "demo-core"', 1) \
                .replace(self.source.commit.encode(), b"")
            text = members["Cargo.lock"].decode()
            start = text.index('checksum = "', text.index('name = "demo-core"')) + len('checksum = "')
            members["Cargo.lock"] = (text[:start] + "0" * 64 + text[start + 64:]).encode()
        self.rebuild_crate("demo-cli", change)
        report = self.check()
        self.assertEqual(self.failed(report), {"C5-crate-archives"})
        self.assertIn("Cargo.lock entry demo-core is not this set's crate", self.problems(report, "C5-crate-archives"))

    def replace_host_archive(self, host: str, executable: bytes) -> None:
        binary = self.root / "zeno-fcis"
        binary.write_bytes(executable)
        prefix = f"zeno-fcis-{fixture.VERSION}-{host}"
        rc_package.deterministic_binary_archive(binary, self.release / "binaries" / f"{prefix}.tar.gz", prefix,
                                                root=self.source.repo)
        self.reindex()

    def test_a_tampered_host_archive_fails_its_record_and_binding(self):
        bindings = self.source.bindings(self.release)
        self.replace_host_archive(fixture.MAC, fixture.macho_executable(payload=b"tampered"))
        failed = self.failed(self.check(bindings=bindings))
        self.assertTrue({"C6-binary-archives", "C10-cross-job"} <= failed, failed)

    def test_a_journey_that_did_not_pass_on_its_archive_fails(self):
        def archive(host: str) -> Path:
            return self.release / "binaries" / f"zeno-fcis-{fixture.VERSION}-{host}.tar.gz"

        def without_archive() -> dict:
            report = fixture.journey_report(fixture.MAC, archive(fixture.MAC))
            del report["archive"]
            return report

        controls = {
            "failed": lambda: fixture.journey_report(fixture.MAC, archive(fixture.MAC), "failed"),
            "another archive": lambda: fixture.journey_report(fixture.MAC, archive(fixture.LINUX)),
            "another host": lambda: fixture.journey_report(fixture.LINUX, archive(fixture.MAC)),
            "no archive": without_archive,
        }
        for label, report in controls.items():
            with self.subTest(label):
                self.renew()
                rc_package.write_json(self.release / "hosts" / fixture.MAC / "INSTALLED-JOURNEY.json", report())
                self.reindex()
                # The bindings hash the journey now in the set; only the journey itself is wrong.
                self.assertEqual(self.failed(self.check()), {"C6-binary-archives"})
        self.renew()
        bindings = self.source.bindings(self.release)
        (self.release / "hosts" / fixture.MAC / "INSTALLED-JOURNEY.json").unlink()
        self.reindex()
        self.assertIn("C6-binary-archives", self.failed(self.check(bindings=bindings)))

    def test_executable_header_rules(self):
        self.assertEqual(custody.executable_problems(fixture.elf_executable(), fixture.LINUX), [])
        self.assertEqual(custody.executable_problems(fixture.macho_executable(), fixture.MAC), [])
        controls = {
            "newer glibc": (fixture.elf_executable(needs=(("GLIBC_2.40", False),)), fixture.LINUX),
            "older glibc": (fixture.elf_executable(needs=(("GLIBC_2.34", False),)), fixture.LINUX),
            "interpreter": (fixture.elf_executable(interpreter="/lib/ld-musl-x86_64.so.1"), fixture.LINUX),
            "build path": (fixture.elf_executable(payload=b"/home/runner/work/ZenoFCIS/src"), fixture.LINUX),
            "home path": (fixture.elf_executable(payload=b"/home/" + b"alice/src/x.rs"), fixture.LINUX),
            "macOS 12": (fixture.macho_executable(minimum=(12, 0)), fixture.MAC),
            "unsigned": (fixture.macho_executable(signed=False), fixture.MAC),
            "x86_64 Mach-O": (fixture.macho_executable(cputype=0x01000007), fixture.MAC),
            "universal": (b"\xca\xfe\xba\xbe" + fixture.macho_executable(), fixture.MAC),
            "unknown host": (fixture.elf_executable(), "riscv64gc-unknown-linux-gnu"),
        }
        for label, (data, host) in controls.items():
            with self.subTest(label):
                self.assertTrue(custody.executable_problems(data, host), label)

    def test_a_binary_archive_with_a_drifted_document_fails(self):
        (self.source.repo / "docs/INSTALLATION.md").write_text("drifted\n")
        self.replace_host_archive(fixture.LINUX, fixture.elf_executable())
        subprocess.run(["git", "checkout", "--", "docs/INSTALLATION.md"], cwd=self.source.repo, check=True)
        self.assertIn("C6-binary-archives", self.failed(self.check()))

    def edit_inputs(self, host: str, change) -> None:
        path = self.release / "hosts" / host / f"COMPILED-INPUTS-{host}.json"
        inputs = json.loads(path.read_text())
        change(inputs)
        rc_package.write_json(path, inputs)
        self.reindex()

    def test_compiled_input_controls(self):
        controls = {
            "workspace input outside the commit": lambda d: d["workspace_inputs"].append(
                {"path": "crates/demo-cli/src/generated.rs", "sha256": "0" * 64}),
            "registry input not in its archive": lambda d: d["registry_inputs"].append(
                {"package": "extdep-1.0.0", "path": "src/other.rs", "sha256": "0" * 64}),
            "registry tree drift": lambda d: d["registry_packages"][0].update(tree_sha256="0" * 64),
            "unlocked checksum": lambda d: d["registry_packages"][0].update(checksum="0" * 64),
            "unclassified input": lambda d: d["unclassified"].append("/opt/elsewhere/input.rs"),
            "build tree recorded": lambda d: d["env_deps"].__setitem__(0, {"name": "ZENO_FCIS_BUILD_TREE",
                                                                           "value": "<source>", "private": False}),
            "private environment": lambda d: d["env_deps"].append({"name": "X", "value": "/home/" + "alice/x",
                                                                   "private": True}),
            "a variable recorded as private": lambda d: d["env_deps"].append(
                {"name": "OUT_DIR", "value": "<home>/out", "private": True}),
            "a home path in the first of two values of one variable": lambda d: d["env_deps"].extend([
                {"name": "OUT_DIR", "value": "/home/" + "alice/out", "private": False},
                {"name": "OUT_DIR", "value": "<target>/release/build/x/out", "private": False}]),
        }
        for label, change in controls.items():
            with self.subTest(label):
                self.renew()
                self.edit_inputs(fixture.LINUX, change)
                self.assertIn("C7-compiled-inputs", self.failed(self.check()), label)

    def record_compiled_manifest(self) -> None:
        source = "crates/demo-core/Cargo.toml"
        for host in (fixture.LINUX, fixture.MAC):
            self.edit_inputs(host, lambda d: d["workspace_inputs"].append(
                {"path": source, "sha256": fixture.sha256(self.source.blob(source))}))

    def test_compiled_manifest_uses_original_archive_member(self):
        self.record_compiled_manifest()
        report = self.check()
        self.assertEqual(report["status"], "passed", self.problems(report, "C7-compiled-inputs"))

    def test_compiled_manifest_refuses_missing_or_changed_original(self):
        controls = {
            "missing original": lambda members: members.pop("Cargo.toml.orig"),
            "edited original": lambda members: members.update({"Cargo.toml.orig": b"# edited\n"}),
            "normalized manifest as original": lambda members: members.update(
                {"Cargo.toml.orig": members["Cargo.toml"]}),
        }
        for label, change in controls.items():
            with self.subTest(label):
                self.renew()
                self.record_compiled_manifest()
                self.rebuild_crate("demo-core", change)
                failed = self.failed(self.check())
                self.assertTrue({"C5-crate-archives", "C7-compiled-inputs"} <= failed, label)

    def test_a_cached_registry_archive_that_differs_fails(self):
        cached = next(self.source.cargo_home.glob("registry/cache/*/extdep-1.0.0.crate"))
        cached.write_bytes(cached.read_bytes() + b"\0")
        self.assertIn("C7-compiled-inputs", self.failed(self.check()))

    def test_packaged_evidence_must_name_the_commit_and_crates(self):
        path = self.release / "PACKAGED-APPLICATION.json"
        document = json.loads(path.read_text())
        document["checker_source_clean"] = False
        rc_package.write_json(path, document)
        self.reindex()
        self.assertIn("C8-packaged-evidence", self.failed(self.check()))

    def test_a_stale_index_or_bundle_fails(self):
        (self.release / "SBOM.cdx.json").write_text(json.dumps({"metadata": {}}) + "\n")
        self.assertIn("C9-release-index", self.failed(self.check()))
        self.renew()
        bundle = self.release / f"zeno-fcis-{fixture.VERSION}-rc-bundle.zip"
        bundle.write_bytes(bundle.read_bytes()[:-10])
        self.assertIn("C9-release-index", self.failed(self.check()))

    def test_host_targets_must_equal_the_archive_hosts(self):
        path = self.release / "PROVENANCE-INPUTS.json"
        provenance = json.loads(path.read_text())
        provenance["host_targets"] = [fixture.LINUX]
        rc_package.write_json(path, provenance)
        self.reindex()
        self.assertIn("C9-release-index", self.failed(self.check()))

    def test_bindings_and_rustc_must_agree(self):
        self.assertIn("C10-cross-job", self.failed(self.check(bindings=None)))
        bindings = self.source.bindings(self.release)
        bindings[fixture.MAC]["journey_sha256"] = "0" * 64
        self.assertIn("C10-cross-job", self.failed(self.check(bindings=bindings)))
        self.renew()
        path = self.release / "hosts" / fixture.MAC / "HOST-BINARY.json"
        record = json.loads(path.read_text())
        record["rustc"] = record["rustc"].replace("f1e2d3c4b5a6", "aaaaaaaaaaaa")
        rc_package.write_json(path, record)
        self.reindex()
        self.assertIn("C10-cross-job", self.failed(self.check()))

    def test_bindings_must_name_every_host_and_field(self):
        controls = {
            "a host missing": lambda bindings: bindings.pop(fixture.MAC),
            "a field missing": lambda bindings: bindings[fixture.MAC].pop("journey_sha256"),
            "an empty field": lambda bindings: bindings[fixture.LINUX].update(inputs_sha256=""),
            "an extra host": lambda bindings: bindings.update({"riscv64gc-unknown-linux-gnu": bindings[fixture.LINUX]}),
            "another archive": lambda bindings: bindings[fixture.MAC].update(
                archive=f"zeno-fcis-{fixture.VERSION}-{fixture.LINUX}.tar.gz"),
        }
        for label, change in controls.items():
            with self.subTest(label):
                bindings = self.source.bindings(self.release)
                change(bindings)
                self.assertEqual(self.failed(self.check(bindings=bindings)), {"C10-cross-job"}, label)

    def test_download_list_controls(self):
        self.assertIn("C11-download-list", self.failed(self.check(downloads=None)))
        lines = self.downloads.read_text().splitlines()
        names = [line.split("  ")[1] for line in lines]
        self.assertEqual(names, sorted([f"zeno-fcis-{fixture.VERSION}-{fixture.LINUX}.tar.gz",
                                        f"zeno-fcis-{fixture.VERSION}-{fixture.MAC}.tar.gz",
                                        f"zeno-fcis-rustdoc-{fixture.VERSION}.tar.gz", "SBOM.cdx.json"]))
        archive = f"zeno-fcis-{fixture.VERSION}-{fixture.MAC}.tar.gz"
        key_line = f"{'0' * 64}  RELEASE-SIGNING-KEY.pub"
        for label, text in {"an archive omitted": [line for line in lines if not line.endswith(archive)],
                            "the SBOM omitted": [line for line in lines if not line.endswith("  SBOM.cdx.json")],
                            "an extra asset": [*lines, f"{'0' * 64}  unrelated.tar.gz"],
                            "a wrong hash": [f"{'0' * 64}  {names[0]}", *lines[1:]],
                            "a key without --signing-key": [*lines, key_line]}.items():
            with self.subTest(label):
                self.downloads.write_text("\n".join(text) + "\n")
                self.assertIn("C11-download-list", self.failed(self.check()), label)
        key = self.root / "RELEASE-SIGNING-KEY.pub"
        key.write_text("ssh-ed25519 AAAA\n")
        signed = rc_package.downloads_lines(self.release, key)
        self.downloads.write_text(signed)
        self.assertEqual(self.check(signing_key=key)["status"], "passed")
        renamed = self.root / "owner.pub"
        renamed.write_text("ssh-ed25519 AAAA\n")
        for label, (text, given) in {
                "a key with another hash": (signed.replace(fixture.sha256(key.read_bytes()), "0" * 64), key),
                "the key omitted": ("".join(line + "\n" for line in lines), key),
                "a key file named otherwise": (signed, renamed)}.items():
            with self.subTest(label):
                self.downloads.write_text(text)
                self.assertIn("C11-download-list", self.failed(self.check(signing_key=given)), label)


class PackagedRegressionInputsTest(unittest.TestCase):
    def test_manifest_matches_every_current_input(self):
        for row in rc_package.load_packaged_test_inputs():
            with self.subTest(package=row["package"], target=row["target"], path=row["path"]):
                self.assertEqual(rc_package.sha256(rc_package.ROOT / row["path"]), row["sha256"])

    def test_public_arithmetic_inputs_stage_only_at_the_reviewed_digest(self):
        entries = [row for row in rc_package.load_packaged_test_inputs()
                   if row["path"].startswith(("examples/arithmetic-", "tools/test-data/scalar-arithmetic/"))]
        self.assertEqual(len(entries), 7)
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            staged = rc_package.stage_repository_inputs(
                entries, target, read_blob=lambda name: (rc_package.ROOT / name).read_bytes())
            self.assertEqual(len(staged), 7)
            for row in staged:
                self.assertEqual(rc_package.sha256(target / row["path"]), row["sha256"])
            with self.assertRaises(rc_package.RcError):
                rc_package.stage_repository_inputs(entries, target, read_blob=lambda name: b"changed")

    def test_other_examples_and_traversal_do_not_enter_the_fixture_scope(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "inputs.json"
            for name in ("examples/unreviewed/project.zeno", "tools/private/input.txt",
                         "examples/arithmetic-fee/../private/input.txt"):
                row = {"package": "example", "target": "test:input", "path": name, "sha256": "0" * 64}
                path.write_text(json.dumps({"format": rc_package.PACKAGED_TEST_INPUTS_FORMAT, "inputs": [row]}))
                with self.assertRaises(rc_package.RcError):
                    rc_package.load_packaged_test_inputs(path)


class PackagedLockTest(unittest.TestCase):
    """The Cargo.lock that cargo +1.97.1 packaged into zeno-fcis-project-2.3.0-rc.1.crate of the v2.3.0-rc.1
    set, byte for byte, checked against the entries of that commit's workspace lock it names."""

    def setUp(self):
        self.lock = (DATA / "zeno-fcis-project-2.3.0-rc.1.Cargo.lock").read_text()
        self.workspace = custody.lock_entries((DATA / "v2.3.0-rc.1-workspace-lock-excerpt.toml").read_text())

    def problems(self, text: str) -> list[str]:
        return custody.packaged_lock_problems(text, "zeno-fcis-project", "2.3.0-rc.1",
                                              {"zeno-fcis-project", *RC1_CRATES}, self.workspace, RC1_CRATES,
                                              "zeno-fcis-project-2.3.0-rc.1.crate")

    def test_the_real_packaged_lockfile_passes(self):
        self.assertEqual(self.problems(self.lock), [])

    def test_lockfile_controls(self):
        own = '[[package]]\nname = "zeno-fcis-project"\nversion = "2.3.0-rc.1"\n'
        registry = f'source = "{custody.REGISTRY_SOURCE}"\n'
        value = f'{registry}checksum = "{RC1_CRATES["zeno-fcis-value"]}"\n'
        controls = {
            "its own entry with a source and checksum": self.lock.replace(own, own + registry + f'checksum = "{"0" * 64}"\n'),
            "its own entry twice": self.lock + "\n" + own,
            "no entry of its own": self.lock.replace(own, own.replace("2.3.0-rc.1", "2.3.0-rc.0")),
            "a dependency from another set": self.lock.replace(RC1_CRATES["zeno-fcis-value"], "0" * 64),
            "a path dependency": self.lock.replace(value, ""),
            "an external entry that drifted": self.lock.replace(
                "446ba717509524cb3f22f17ecc096f10f4822d76ab5c0b9822c5f9c284e825f4", "0" * 64),
        }
        for label, text in controls.items():
            with self.subTest(label):
                self.assertNotEqual(text, self.lock, label)
                self.assertTrue(self.problems(text), label)


class LockfileIgnoreTest(unittest.TestCase):
    def test_no_lockfile_a_tool_reads_is_ignored(self):
        """`git add` leaves an ignored file out, and the source-state check refuses one in a checkout."""
        with tempfile.TemporaryDirectory(prefix="zeno-fcis-ignore-test-") as directory:
            repo = Path(directory)
            subprocess.run(["git", "init", "--quiet"], cwd=repo, check=True)
            for lock in COMMITTED_LOCKFILES:
                self.assertTrue((custody.ROOT / lock).is_file(), f"{lock} is missing")
                for folder in (Path(lock).parent, *Path(lock).parent.parents):
                    rules = custody.ROOT / folder / ".gitignore"
                    if rules.is_file():
                        (repo / folder).mkdir(parents=True, exist_ok=True)
                        shutil.copyfile(rules, repo / folder / ".gitignore")
            completed = subprocess.run(["git", "check-ignore", "--no-index", *COMMITTED_LOCKFILES], cwd=repo,
                                       capture_output=True, text=True, check=False)
            self.assertIn(completed.returncode, (0, 1), completed.stderr)
            self.assertEqual(completed.stdout.split(), [])


class WorkflowEvidenceTest(unittest.TestCase):
    def test_journey_and_custody_evidence_is_uploaded_even_when_a_step_fails(self):
        text = (custody.ROOT / ".github/workflows/release-candidate.yml").read_text()
        self.assertEqual(workflow_contexts.check_contexts("release-candidate.yml", text), [])
        steps = text.split("\n      - name: ")[1:]
        for evidence in ("${{ runner.temp }}/host/*.json", "${{ runner.temp }}/evidence\n",
                         "${{ runner.temp }}/INSTALLED-JOURNEY-x86_64-unknown-linux-gnu.json"):
            with self.subTest(evidence):
                uploads = [step for step in steps if "actions/upload-artifact@" in step and evidence in step]
                self.assertTrue(any("\n        if: always()\n" in step for step in uploads), evidence)


if __name__ == "__main__":
    unittest.main(verbosity=2)
