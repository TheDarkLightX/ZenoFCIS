"""Controls for the pure parts of `check_installed_cli.py` and the documented install blocks it runs."""

from __future__ import annotations

import gzip
import io
import json
import os
import shutil
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path

import check_installed_cli as installed
import rc_package

ROOT = installed.ROOT
LINUX, MAC = "x86_64-unknown-linux-gnu", "aarch64-apple-darwin"
URL = "https://github.com/TheDarkLightX/ZenoFCIS/releases/download"


def block(lines: str) -> str:
    return f"<!-- installed-journey: {LINUX} -->\n```bash\n{lines}```\n"


def fake_archive(path: Path, prefix: str, version: str) -> None:
    """A normalized archive whose `zeno-fcis` prints a describe record and nothing else."""
    script = f"#!/bin/sh\nprintf '%s\\n' '{json.dumps({'cli_version': version})}'\n".encode()
    with path.open("wb") as raw, gzip.GzipFile(filename="", fileobj=raw, mode="wb", mtime=0) as compressed, \
            tarfile.open(fileobj=compressed, mode="w", format=tarfile.USTAR_FORMAT) as archive:
        info = rc_package.normalized_tar_info(tarfile.TarInfo(f"{prefix}/zeno-fcis"))
        info.size, info.mode = len(script), 0o755
        archive.addfile(info, io.BytesIO(script))


class BlockTest(unittest.TestCase):
    def test_hosts(self):
        self.assertEqual(installed.host_triple("Linux", "x86_64"), LINUX)
        self.assertEqual(installed.host_triple("Darwin", "arm64"), MAC)
        for system, machine in (("Darwin", "x86_64"), ("Linux", "aarch64"), ("Windows", "AMD64")):
            with self.assertRaises(installed.JourneyError):
                installed.host_triple(system, machine)

    def test_the_installation_guide_marks_one_runnable_block_per_host(self):
        markdown = (ROOT / "docs/INSTALLATION.md").read_text()
        version = installed.workspace_version()
        for host in (LINUX, MAC):
            with self.subTest(host):
                text = installed.install_block(markdown, host)
                script, environment = installed.prepare_block(text, version, "file:///downloads")
                self.assertNotIn(URL, script)
                # Both blocks take the release from VERSION, so a version bump needs no edit there. The
                # journey sets VERSION to this checkout's version; DocumentedInstallTest runs that.
                self.assertEqual(environment, {"VERSION": version})
                self.assertIn(f'ARCHIVE="zeno-fcis-$VERSION-{host}"', script)
                self.assertTrue(script.rstrip().endswith("zeno-fcis describe"))
        self.assertIn("sha256sum --ignore-missing --check DOWNLOADS.sha256", installed.install_block(markdown, LINUX))
        self.assertIn("shasum -a 256 -c -", installed.install_block(markdown, MAC))

    def test_block_controls(self):
        literal = f"curl --output a {URL}/v1.2.3/a\n"
        self.assertEqual(installed.prepare_block(literal, "1.2.3", "file:///d")[0], "curl --output a file:///d/a\n")
        refused = {
            "two markers": lambda: installed.install_block(block(literal) * 2, LINUX),
            "no marker": lambda: installed.install_block(block(literal), MAC),
            "no bash fence": lambda: installed.install_block(f"<!-- installed-journey: {LINUX} -->\n```sh\nx\n```\n",
                                                             LINUX),
            "another version": lambda: installed.prepare_block(literal, "1.2.4", "file:///d"),
            "mixed versions": lambda: installed.prepare_block(literal + f"curl {URL}/v$VERSION/b\n", "1.2.3", "file:///d"),
            "assigned VERSION": lambda: installed.prepare_block(f"VERSION=9.9.9\ncurl {URL}/v$VERSION/b\n", "1.2.3",
                                                                "file:///d"),
            "exported VERSION": lambda: installed.prepare_block(f"export VERSION=9\ncurl {URL}/v$VERSION/b\n", "1",
                                                                "file:///d"),
            "no download": lambda: installed.prepare_block("echo hello\n", "1.2.3", "file:///d"),
        }
        for label, attempt in refused.items():
            with self.subTest(label), self.assertRaises(installed.JourneyError):
                attempt()

    def test_sh_block(self):
        readme = (ROOT / "crates/zeno-fcis-cli/templates/durable-counter/README.md").read_text()
        self.assertEqual(installed.sh_block(readme, "cargo +1.97.1 test --offline"), [
            ["zeno-fcis", "check", "project.zeno"], ["cargo", "+1.97.1", "test", "--offline"],
            ["cargo", "+1.97.1", "run", "--offline", "--", "new-counter.sqlite"]])
        with self.assertRaises(installed.JourneyError):
            installed.sh_block("```sh\nmake\n```\n```sh\nmake\n```\n", "make")
        with self.assertRaises(installed.JourneyError):
            installed.sh_block(readme, "cargo build")


class DownloadsTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="zeno-fcis-installed-test-")
        self.root = Path(self.temporary.name)

    def tearDown(self):
        self.temporary.cleanup()

    def test_a_host_folder_lists_its_archive_and_a_release_set_lists_its_assets(self):
        host = self.root / "host"
        (host / "binaries").mkdir(parents=True)
        archive = host / "binaries" / f"zeno-fcis-9.9.9-{MAC}.tar.gz"
        archive.write_bytes(b"archive")
        self.assertEqual(installed.downloads_text(host, MAC, "9.9.9"), f"{rc_package.sha256(archive)}  {archive.name}\n")
        release = self.root / "release"
        for name in (f"binaries/zeno-fcis-9.9.9-{LINUX}.tar.gz", "docs/zeno-fcis-rustdoc-9.9.9.tar.gz",
                     "SBOM.cdx.json"):
            (release / name).parent.mkdir(parents=True, exist_ok=True)
            (release / name).write_bytes(name.encode())
        (release / "SHA256SUMS").write_text("".join(f"{rc_package.sha256(release / name)}  {name}\n" for name in (
            f"binaries/zeno-fcis-9.9.9-{LINUX}.tar.gz", "docs/zeno-fcis-rustdoc-9.9.9.tar.gz", "SBOM.cdx.json")))
        listed = installed.downloads_text(release, LINUX, "9.9.9")
        self.assertEqual(listed, rc_package.downloads_lines(release))
        self.assertEqual([line.split("  ")[1] for line in listed.splitlines()],
                         ["SBOM.cdx.json", f"zeno-fcis-9.9.9-{LINUX}.tar.gz", "zeno-fcis-rustdoc-9.9.9.tar.gz"])


INSTALL_TOOLS = ("bash", "curl", "sha256sum", "shasum", "grep", "tar")


class DocumentedInstallTest(unittest.TestCase):
    """Both documented blocks, run as the journey runs them, against a local release folder."""

    def setUp(self):
        missing = [tool for tool in INSTALL_TOOLS if shutil.which(tool) is None]
        if missing:
            # Every CI image that runs these tests has all of them, so a gap there is a failure.
            if os.environ.get("CI") == "true":
                self.fail(f"the documented blocks need {', '.join(missing)}, which this CI runner lacks")
            self.skipTest(f"the documented blocks need {', '.join(missing)}, which this machine lacks")
        self.temporary = tempfile.TemporaryDirectory(prefix="zeno-fcis-installed-test-")
        self.root = Path(self.temporary.name)
        self.version = installed.workspace_version()
        self.markdown = (ROOT / "docs/INSTALLATION.md").read_text()

    def tearDown(self):
        self.temporary.cleanup()

    def install(self, host: str, change=None) -> subprocess.CompletedProcess:
        work = Path(tempfile.mkdtemp(dir=self.root))
        downloads, directory = work / "downloads", work / "install"
        downloads.mkdir()
        directory.mkdir()
        archive = downloads / f"zeno-fcis-{self.version}-{host}.tar.gz"
        fake_archive(archive, archive.name.removesuffix(".tar.gz"), self.version)
        (downloads / "DOWNLOADS.sha256").write_text(
            f"{'0' * 64}  zeno-fcis-rustdoc-{self.version}.tar.gz\n{rc_package.sha256(archive)}  {archive.name}\n")
        if change:
            change(downloads, archive)
        script, variables = installed.prepare_block(installed.install_block(self.markdown, host), self.version,
                                                    downloads.as_uri())
        environment = {key: value for key, value in os.environ.items() if key != "VERSION"} | variables
        return subprocess.run(["bash", "-euo", "pipefail", "-c", script], cwd=directory, env=environment,
                              capture_output=True, check=False, timeout=60)

    def test_each_block_installs_and_describes_the_archive(self):
        for host in (LINUX, MAC):
            with self.subTest(host):
                completed = self.install(host)
                self.assertEqual(completed.returncode, 0, completed.stderr.decode(errors="replace"))
                self.assertTrue(completed.stdout.endswith(
                    (json.dumps({"cli_version": self.version}) + "\n").encode()))

    def test_each_block_stops_on_a_wrong_or_missing_checksum(self):
        controls = {
            "tampered archive": lambda downloads, archive: archive.write_bytes(archive.read_bytes() + b"\0"),
            "missing line": lambda downloads, archive: (downloads / "DOWNLOADS.sha256").write_text(
                f"{'0' * 64}  zeno-fcis-rustdoc-{self.version}.tar.gz\n"),
        }
        for host in (LINUX, MAC):
            for label, change in controls.items():
                with self.subTest(host=host, control=label):
                    completed = self.install(host, change)
                    self.assertNotEqual(completed.returncode, 0)
                    self.assertNotIn(b"cli_version", completed.stdout)

    def test_each_block_needs_version(self):
        environment = {key: value for key, value in os.environ.items() if key != "VERSION"}
        for host in (LINUX, MAC):
            with self.subTest(host):
                script, _ = installed.prepare_block(installed.install_block(self.markdown, host), self.version,
                                                    "file:///x")
                completed = subprocess.run(["bash", "-euo", "pipefail", "-c", script], cwd=self.root,
                                           env=environment, capture_output=True, check=False, timeout=60)
                self.assertNotEqual(completed.returncode, 0)
                self.assertIn(b"set VERSION", completed.stderr)
                self.assertEqual(list(self.root.iterdir()), [])


if __name__ == "__main__":
    unittest.main(verbosity=2)
