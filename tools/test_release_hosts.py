"""Hostile controls for `rc_package.py merge-hosts` and `downloads-list` on synthetic release sets, and for
the compiled-inputs record on real rustc dep-info."""

from __future__ import annotations

import gzip
import io
import json
import tarfile
import tempfile
import unittest
from pathlib import Path

import check_release_custody as custody
import rc_package
import release_fixture as fixture

DATA = Path(__file__).resolve().parent / "test_data" / "release"


def archive_with(path: Path, prefix: str, members: dict[str, tuple[bytes, int]]) -> None:
    """A normalized binary archive with exactly the given members."""
    with path.open("wb") as raw, gzip.GzipFile(filename="", fileobj=raw, mode="wb", mtime=0) as compressed, \
            tarfile.open(fileobj=compressed, mode="w", format=tarfile.USTAR_FORMAT) as archive:
        for name, (content, mode) in members.items():
            info = rc_package.normalized_tar_info(tarfile.TarInfo(f"{prefix}/{name}"))
            info.size, info.mode = len(content), mode
            archive.addfile(info, io.BytesIO(content))


class MergeHostsTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="zeno-fcis-hosts-test-")
        self.root = Path(self.temporary.name)
        self.source = fixture.Fixture(self.root)
        self.release = self.source.release(self.root / "release", hosts=(fixture.LINUX,))
        # The Linux set carries no journey until the release-set job adds one.
        (self.release / "hosts" / fixture.LINUX / "INSTALLED-JOURNEY.json").unlink()
        rc_package.write_release_index(self.release, fixture.VERSION, self.source.commit)
        self.journey = self.root / "linux-journey.json"
        self.linux_archive = self.release / "binaries" / f"zeno-fcis-{fixture.VERSION}-{fixture.LINUX}.tar.gz"
        rc_package.write_json(self.journey, fixture.journey_report(fixture.LINUX, self.linux_archive))
        self.mac = self.source.host_dir(self.root / "mac-host", fixture.MAC, fixture.macho_executable())

    def tearDown(self):
        self.temporary.cleanup()

    def renew(self) -> None:
        """A fresh source, release set and host folder inside one test."""
        self.tearDown()
        self.setUp()

    def merge(self, hosts=None, journeys=None) -> dict:
        return rc_package.merge_hosts(self.release, [self.mac] if hosts is None else hosts,
                                      {fixture.LINUX: self.journey} if journeys is None else journeys,
                                      repo=self.source.repo)

    def edit_record(self, change) -> None:
        path = self.mac / "HOST-BINARY.json"
        record = json.loads(path.read_text())
        change(record)
        rc_package.write_json(path, record)

    def refused(self, fragment: str, **arguments) -> None:
        with self.assertRaises(rc_package.RcError) as raised:
            self.merge(**arguments)
        self.assertIn(fragment, str(raised.exception))

    def test_merging_the_macos_host_makes_a_set_that_passes_custody(self):
        summary = self.merge()
        self.assertEqual(summary["hosts"], [fixture.MAC, fixture.LINUX])
        provenance = json.loads((self.release / "PROVENANCE-INPUTS.json").read_text())
        self.assertEqual(provenance["host_targets"], [fixture.MAC, fixture.LINUX])
        self.assertEqual(set(provenance["hosts"]), {fixture.MAC, fixture.LINUX})
        rc_package.verify_release_index(self.release, fixture.VERSION)
        for name in ("HOST-BINARY.json", f"COMPILED-INPUTS-{fixture.MAC}.json", "INSTALLED-JOURNEY.json"):
            self.assertEqual((self.release / "hosts" / fixture.MAC / name).read_bytes(), (self.mac / name).read_bytes())
        downloads = self.root / "DOWNLOADS.sha256"
        downloads.write_text(rc_package.downloads_lines(self.release))
        report = custody.check(self.release, self.source.repo, bindings=self.source.bindings(self.release),
                               downloads=downloads, cargo_home=self.source.cargo_home)
        self.assertEqual(report["status"], "passed", json.dumps(report["checks"], indent=1)[:3000])

    def test_commit_version_and_rustc_mismatches_are_refused(self):
        self.edit_record(lambda record: record.update(source_commit="0" * 40))
        self.refused("built from another commit or version")
        self.renew()
        self.edit_record(lambda record: record.update(version="0.0.0"))
        self.refused("built from another commit or version")
        self.renew()
        self.edit_record(lambda record: record.update(rustc=record["rustc"].replace("f1e2d3c4b5a6", "b" * 12)))
        self.refused("built with another rustc")

    def test_duplicate_hosts_are_refused(self):
        linux = self.source.host_dir(self.root / "linux-again", fixture.LINUX, fixture.elf_executable())
        self.refused(f"duplicate host {fixture.LINUX}", hosts=[linux])
        twin = self.source.host_dir(self.root / "mac-twin", fixture.MAC, fixture.macho_executable())
        self.refused(f"duplicate host {fixture.MAC}", hosts=[self.mac, twin])

    def test_a_renamed_or_unexpected_archive_is_refused(self):
        archive = self.mac / "binaries" / f"zeno-fcis-{fixture.VERSION}-{fixture.MAC}.tar.gz"
        archive.rename(archive.with_name("zeno-fcis-renamed.tar.gz"))
        self.refused("must hold exactly the archive")
        self.renew()
        (self.mac / "binaries" / "extra.tar.gz").write_bytes(b"x")
        self.refused("must hold exactly the archive")

    def rewrite_archive(self, members_change) -> None:
        prefix = f"zeno-fcis-{fixture.VERSION}-{fixture.MAC}"
        path = self.mac / "binaries" / f"{prefix}.tar.gz"
        executable = fixture.macho_executable()
        members = {"zeno-fcis": (executable, 0o755), "README.md": (self.source.blob("README.md"), 0o644),
                   "LICENSE-APACHE": (self.source.blob("LICENSE-APACHE"), 0o644),
                   "LICENSE-MIT": (self.source.blob("LICENSE-MIT"), 0o644),
                   "NOTICE": (self.source.blob("NOTICE"), 0o644),
                   "THIRD_PARTY_NOTICES": (self.source.blob("THIRD_PARTY_NOTICES"), 0o644),
                   "CITATION.cff": (self.source.blob("CITATION.cff"), 0o644),
                   "INSTALLATION.md": (self.source.blob("docs/INSTALLATION.md"), 0o644)}
        members_change(members)
        archive_with(path, prefix, members)
        self.edit_record(lambda record: record["archive"].update(sha256=fixture.sha256(path.read_bytes())))

    def test_member_controls_are_refused(self):
        controls = {
            "extra member": lambda members: members.update({"extra.txt": (b"x", 0o644)}),
            "missing member": lambda members: members.pop("INSTALLATION.md"),
            "drifted document": lambda members: members.update({"README.md": (b"# Drifted\n", 0o644)}),
            "executable mode": lambda members: members.update({"zeno-fcis": (members["zeno-fcis"][0], 0o644)}),
        }
        for name in ("NOTICE", "THIRD_PARTY_NOTICES", "CITATION.cff"):
            controls[f"missing {name}"] = lambda members, name=name: members.pop(name)
            controls[f"drifted {name}"] = lambda members, name=name: members.update({name: (b"drift\n", 0o644)})
        for label, change in controls.items():
            with self.subTest(label):
                self.renew()
                self.rewrite_archive(change)
                with self.assertRaises(rc_package.RcError, msg=label):
                    self.merge()

    def test_a_wrong_archive_hash_is_refused(self):
        self.edit_record(lambda record: record["archive"].update(sha256="0" * 64))
        self.refused("the archive differs from its record")

    def test_an_executable_that_is_not_the_recorded_one_is_refused(self):
        self.edit_record(lambda record: record["executable"].update(sha256="0" * 64))
        self.refused("executable differs from its host record")

    def test_a_stale_index_is_refused_before_anything_is_copied(self):
        (self.release / "SBOM.cdx.json").write_text("{}\n")
        self.refused("changed after indexing")
        self.assertFalse((self.release / "hosts" / fixture.MAC).exists())

    def test_a_set_already_holding_the_host_is_refused_before_anything_is_copied(self):
        # An empty folder is not a file, so the index stays current.
        (self.release / "hosts" / fixture.MAC).mkdir()
        self.refused(f"the set already holds binaries/zeno-fcis-{fixture.VERSION}-{fixture.MAC}.tar.gz "
                     f"or hosts/{fixture.MAC}")
        self.assertFalse((self.release / "hosts" / fixture.LINUX / "INSTALLED-JOURNEY.json").exists())
        rc_package.verify_release_index(self.release, fixture.VERSION)

    def test_private_data_in_a_joining_file_is_refused_before_anything_is_copied(self):
        archive = self.mac / "binaries" / f"zeno-fcis-{fixture.VERSION}-{fixture.MAC}.tar.gz"
        rc_package.write_json(self.mac / "INSTALLED-JOURNEY.json", {
            **fixture.journey_report(fixture.MAC, archive), "steps": [{"argv": ["/home/" + "alice/bin/zeno-fcis"]}]})
        self.refused("INSTALLED-JOURNEY.json: personal-home-path")
        self.assertFalse((self.release / "hosts" / fixture.MAC).exists())
        self.assertFalse((self.release / "hosts" / fixture.LINUX / "INSTALLED-JOURNEY.json").exists())

    def test_missing_or_failed_journeys_are_refused(self):
        (self.mac / "INSTALLED-JOURNEY.json").unlink()
        self.refused("no passed installed-binary journey")
        self.renew()
        self.journey.write_text(json.dumps({"status": "failed"}) + "\n")
        self.refused("journey report is malformed")
        mac_archive = self.mac / "binaries" / f"zeno-fcis-{fixture.VERSION}-{fixture.MAC}.tar.gz"
        controls = {
            "failed run": (fixture.LINUX, lambda: fixture.journey_report(fixture.LINUX, self.linux_archive, "failed")),
            "another host's report": (fixture.LINUX, lambda: fixture.journey_report(fixture.MAC, self.linux_archive)),
            "another archive": (fixture.LINUX, lambda: fixture.journey_report(fixture.LINUX, mac_archive)),
            "another version": (fixture.MAC, lambda: {**fixture.journey_report(fixture.MAC, mac_archive),
                                                      "version": "0.0.0"}),
            "another schema": (fixture.MAC, lambda: {**fixture.journey_report(fixture.MAC, mac_archive),
                                                     "schema": "zeno-fcis/other/1"}),
        }
        for label, (host, report) in controls.items():
            with self.subTest(label):
                self.renew()
                mac_archive = self.mac / "binaries" / mac_archive.name
                target = self.journey if host == fixture.LINUX else self.mac / "INSTALLED-JOURNEY.json"
                rc_package.write_json(target, report())
                self.refused("is not a passed run on this archive")
                self.assertFalse((self.release / "hosts" / fixture.MAC).exists())
        self.renew()
        self.refused("needs a passed installed-binary journey", journeys={})
        self.renew()
        self.refused("journey for unknown host", journeys={fixture.LINUX: self.journey, "riscv64": self.journey})

    def test_downloads_list(self):
        self.merge()
        key = self.root / "RELEASE-SIGNING-KEY.pub"
        key.write_text("ssh-ed25519 AAAA\n")
        lines = rc_package.downloads_lines(self.release, key).splitlines()
        names = [line.split("  ")[1] for line in lines]
        self.assertEqual(names, sorted([f"zeno-fcis-{fixture.VERSION}-{fixture.MAC}.tar.gz",
                                        f"zeno-fcis-{fixture.VERSION}-{fixture.LINUX}.tar.gz",
                                        f"zeno-fcis-rustdoc-{fixture.VERSION}.tar.gz", "SBOM.cdx.json",
                                        "RELEASE-SIGNING-KEY.pub"]))
        self.assertIn(f"{fixture.sha256(key.read_bytes())}  RELEASE-SIGNING-KEY.pub", lines)
        self.assertEqual([line.split("  ")[1] for line in rc_package.downloads_lines(self.release).splitlines()],
                         [name for name in names if name != "RELEASE-SIGNING-KEY.pub"])
        renamed = self.root / "owner.pub"
        renamed.write_text("ssh-ed25519 AAAA\n")
        with self.assertRaises(rc_package.RcError):
            rc_package.downloads_lines(self.release, renamed)
        (self.release / "docs" / f"zeno-fcis-rustdoc-{fixture.VERSION}.tar.gz").write_bytes(b"changed")
        with self.assertRaises(rc_package.RcError):
            rc_package.downloads_lines(self.release)


class CompiledInputsTest(unittest.TestCase):
    """Real rustc 1.97.1 dep-info from a release build of the CLI, placed as GitHub's runners place a build.

    `zeno_fcis-f8ccee4f169b37a2.d` is the CLI crate's file with its 366 prerequisites cut to two, and
    `libsqlite3_sys-2d8a76213bdd63ab.d` is whole. Only the checkout, target and registry folders were
    replaced, by @SOURCE@, @TARGET@ and @REGISTRY@, which each test fills in. rustc recorded
    CARGO_MANIFEST_DIR although the release build sets ZENO_FCIS_BUILD_TREE empty: `binding.rs` expands
    `env!("CARGO_MANIFEST_DIR")` in a branch the build does not take.
    """

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="zeno-fcis-inputs-test-")
        self.root = Path(self.temporary.name).resolve()

    def tearDown(self):
        self.temporary.cleanup()

    def record(self, home: Path, target: Path, extra: str = "") -> dict:
        """The compiled-inputs record of a checkout at ~/work/ZenoFCIS/ZenoFCIS built into `target`."""
        source = home / "work" / "ZenoFCIS" / "ZenoFCIS"
        cargo_home = home / ".cargo"
        registry = cargo_home / "registry" / "src" / "index.crates.io-1949cf8c6b5b557f"
        package = registry / "libsqlite3-sys-0.38.1"
        for path, text in ((source / "crates/zeno-fcis-cli/src/main.rs", "fn main() {}\n"),
                           (source / "crates/zeno-fcis-cli/src/binding.rs", "// binding\n"),
                           (package / "Cargo.toml", '[package]\nname = "libsqlite3-sys"\nversion = "0.38.1"\n'),
                           (package / "src/lib.rs", "// lib\n"), (package / "src/error.rs", "// error\n"),
                           (target / "release/build/libsqlite3-sys-af1c9d641bdcbcf6/out/bindgen.rs", "// bindgen\n")):
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
        deps = target / "release" / "deps"
        deps.mkdir(parents=True)
        for name in ("zeno_fcis-f8ccee4f169b37a2.d", "libsqlite3_sys-2d8a76213bdd63ab.d"):
            text = (DATA / name).read_text() + (extra if name.startswith("zeno_fcis") else "")
            (deps / name).write_text(text.replace("@SOURCE@", str(source)).replace("@TARGET@", str(target))
                                     .replace("@REGISTRY@", str(registry)))
        executable = target / "release" / "zeno-fcis"
        executable.write_bytes(b"executable")
        return rc_package.record_compiled_inputs(target, "host", executable, "0" * 40, "2.3.0-rc.1", root=source,
                                                 home=home, cargo_home=cargo_home)

    def test_the_build_location_variables_of_a_runner_build_are_not_private(self):
        layouts = {
            "Linux: the checkout under home, the target in /tmp": (
                self.root / "home" / "runner", self.root / "tmp" / "zeno-fcis-rc" / ".cargo-target"),
            "macOS: the checkout and the target under home": (
                self.root / "Users" / "runner", self.root / "Users" / "runner" / "work" / "_temp" / "host" / ".cargo-target"),
        }
        for label, (home, target) in layouts.items():
            with self.subTest(label):
                inputs = self.record(home, target)
                self.assertEqual(rc_package.compiled_inputs_problems(inputs), [])
                self.assertEqual({(entry["name"], entry["value"]) for entry in inputs["env_deps"]}, {
                    ("CARGO_MANIFEST_DIR", "<source>/crates/zeno-fcis-cli"), ("CARGO_PKG_RUST_VERSION", "1.97.1"),
                    ("CARGO_PKG_VERSION", "2.3.0-rc.1"), ("ZENO_FCIS_BUILD_TREE", ""),
                    ("OUT_DIR", "<target>/release/build/libsqlite3-sys-af1c9d641bdcbcf6/out")})
                self.assertEqual([item["path"] for item in inputs["workspace_inputs"]],
                                 ["crates/zeno-fcis-cli/src/binding.rs", "crates/zeno-fcis-cli/src/main.rs"])
                self.assertEqual([item["path"] for item in inputs["build_outputs"]],
                                 ["release/build/libsqlite3-sys-af1c9d641bdcbcf6/out/bindgen.rs"])
                self.assertEqual([item["name"] for item in inputs["registry_packages"]], ["libsqlite3-sys"])
                self.assertEqual(inputs["unclassified"], [])

    def test_a_variable_naming_a_private_folder_refuses_the_build(self):
        home = self.root / "home" / "runner"
        source = home / "work" / "ZenoFCIS" / "ZenoFCIS"
        crate = home / ".cargo" / "registry" / "src" / "index.crates.io-1949cf8c6b5b557f" / "other-1.0.0"
        controls = {
            "a registry crate's own folder": ("CARGO_MANIFEST_DIR", str(crate)),
            "an output folder outside the target": ("OUT_DIR", f"{home}/out"),
            "a folder beside the checkout": ("CARGO_MANIFEST_DIR", f"{source}-other/crates/x"),
            "a path that leaves the checkout": ("CARGO_MANIFEST_DIR", f"{source}/../../outside"),
            "another variable naming the checkout": ("ZENO_FCIS_BUILD_TREE", str(source)),
            "another variable naming the target": ("BUILD_CACHE", "{target}/cache"),
            "the home folder": ("HOME", str(home)),
        }
        for number, (label, (name, value)) in enumerate(controls.items()):
            with self.subTest(label):
                target = self.root / f"target-{number}" / ".cargo-target"
                inputs = self.record(home, target, f"# env-dep:{name}={value.format(target=target)}\n")
                self.assertIn(f"{name} names a private path", rc_package.compiled_inputs_problems(inputs), label)


if __name__ == "__main__":
    unittest.main(verbosity=2)
