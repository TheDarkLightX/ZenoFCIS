"""Synthetic sources and release sets for the custody and host-merge self-tests.

Nothing here builds Rust: a small git repository stands in for the source, and the
executables are minimal ELF and Mach-O files whose headers carry exactly the facts
the custody check reads (interpreter, GNU version needs, architecture, platform,
minimum OS, code-signature command).
"""

from __future__ import annotations

import gzip
import hashlib
import io
import json
import os
import struct
import subprocess
import tarfile
from pathlib import Path

import check_release_custody as custody
import rc_package

VERSION = "9.9.9"
LINUX = "x86_64-unknown-linux-gnu"
MAC = "aarch64-apple-darwin"
RUSTC = {LINUX: "rustc 1.97.1 (f1e2d3c4b 2026-08-01)\nbinary: rustc\ncommit-hash: f1e2d3c4b5a6\nhost: {host}\n",
         MAC: "rustc 1.97.1 (f1e2d3c4b 2026-08-01)\nbinary: rustc\ncommit-hash: f1e2d3c4b5a6\nhost: {host}\n"}
REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def journey_report(host: str, archive: Path, status: str = "passed") -> dict:
    """The fields of an installed-binary journey report that merging and custody read."""
    return {"schema": custody.JOURNEY_SCHEMA, "status": status, "host": host, "version": VERSION,
            "archive": {"name": archive.name, "sha256": sha256(archive.read_bytes())}}


def elf_executable(needs=(("GLIBC_2.2.5", False), ("GLIBC_2.39", False), ("GLIBC_2.41", True)),
                   interpreter: str = "/lib64/ld-linux-x86-64.so.2", payload: bytes = b"zeno-fcis linux") -> bytes:
    """A 64-bit little-endian ELF with PT_INTERP and one libc.so.6 version-needs entry."""
    shstrtab = b"\0.shstrtab\0.dynstr\0.gnu.version_r\0"
    dynstr = b"\0libc.so.6\0" + b"".join(name.encode() + b"\0" for name, _ in needs)
    names, offset = [], len(b"\0libc.so.6\0")
    for name, _ in needs:
        names.append(offset)
        offset += len(name) + 1
    verneed = struct.pack("<HHIII", 1, len(needs), 1, 16, 0)
    for index, ((name, weak), name_offset) in enumerate(zip(needs, names)):
        step = 16 if index + 1 < len(needs) else 0
        verneed += struct.pack("<IHHII", 0, 2 if weak else 0, index + 2, name_offset, step)
    interp = interpreter.encode() + b"\0"
    body = interp + shstrtab + dynstr + verneed + payload
    interp_offset = 64 + 56
    shstr_offset = interp_offset + len(interp)
    dynstr_offset = shstr_offset + len(shstrtab)
    verneed_offset = dynstr_offset + len(dynstr)
    shoff = (64 + 56 + len(body) + 7) // 8 * 8
    header = struct.pack("<16sHHIQQQIHHHHHH", b"\x7fELF\x02\x01\x01" + b"\0" * 9, 3, 62, 1, 0, 64, shoff, 0,
                         64, 56, 1, 64, 4, 1)
    program = struct.pack("<IIQQQQQQ", 3, 4, interp_offset, 0, 0, len(interp), len(interp), 1)
    sections = struct.pack("<IIQQQQIIQQ", 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)
    sections += struct.pack("<IIQQQQIIQQ", 1, 3, 0, 0, shstr_offset, len(shstrtab), 0, 0, 1, 0)
    sections += struct.pack("<IIQQQQIIQQ", 11, 3, 0, 0, dynstr_offset, len(dynstr), 0, 0, 1, 0)
    sections += struct.pack("<IIQQQQIIQQ", 19, 0x6FFFFFFE, 0, 0, verneed_offset, len(verneed), 2, 1, 8, 0)
    data = header + program + body
    return data + b"\0" * (shoff - len(data)) + sections


def macho_executable(minimum=(11, 0), signed: bool = True, cputype: int = 0x0100000C,
                     payload: bytes = b"zeno-fcis mac") -> bytes:
    """A thin 64-bit Mach-O executable with LC_BUILD_VERSION and, optionally, LC_CODE_SIGNATURE."""
    commands = struct.pack("<IIIIII", 0x32, 24, 1, (minimum[0] << 16) | (minimum[1] << 8), 15 << 16, 0)
    if signed:
        commands += struct.pack("<IIII", 0x1D, 16, 0, 0)
    header = struct.pack("<IIIIIIII", 0xFEEDFACF, cputype, 0, 2, 2 if signed else 1, len(commands), 0, 0)
    return header + commands + payload


def git(repo: Path, *arguments: str) -> str:
    environment = {**os.environ, "GIT_AUTHOR_NAME": "fixture", "GIT_AUTHOR_EMAIL": "fixture@example.invalid",
                   "GIT_COMMITTER_NAME": "fixture", "GIT_COMMITTER_EMAIL": "fixture@example.invalid",
                   "GIT_AUTHOR_DATE": "2026-01-01T00:00:00Z", "GIT_COMMITTER_DATE": "2026-01-01T00:00:00Z"}
    result = subprocess.run(["git", *arguments], cwd=repo, env=environment, check=True,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    return result.stdout.decode().strip()


def write(path: Path, text: str | bytes, executable: bool = False) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(text.encode() if isinstance(text, str) else text)
    if executable:
        path.chmod(0o755)


def crate_archive(path: Path, members: dict[str, bytes], top: str, executables: frozenset = frozenset()) -> None:
    """A .crate as cargo writes one: regular files only, 0644 or 0755, cargo's fixed mtime."""
    with path.open("wb") as raw, gzip.GzipFile(fileobj=raw, mode="wb", mtime=0) as compressed, \
            tarfile.open(fileobj=compressed, mode="w", format=tarfile.USTAR_FORMAT) as archive:
        for name in sorted(members):
            info = tarfile.TarInfo(f"{top}/{name}")
            info.size, info.mtime = len(members[name]), 1153704088
            info.mode = 0o755 if name in executables else 0o644
            archive.addfile(info, io.BytesIO(members[name]))


class Fixture:
    """A committed synthetic repository, a fake Cargo home and builders for release sets."""

    def __init__(self, root: Path):
        self.root = root
        self.repo = root / "repo"
        self.cargo_home = root / "cargo-home"
        self.extdep = self.registry_crate()
        self.make_repo()

    def registry_crate(self) -> dict:
        files = {"Cargo.toml": b'[package]\nname = "extdep"\nversion = "1.0.0"\n', "src/lib.rs": b"pub fn f() {}\n",
                 "native/extdep.c": b"int extdep(void) { return 1; }\n"}
        cache = self.cargo_home / "registry/cache/index.crates.io-0000"
        cache.mkdir(parents=True)
        crate_archive(cache / "extdep-1.0.0.crate", files, "extdep-1.0.0")
        return {"files": files, "checksum": sha256((cache / "extdep-1.0.0.crate").read_bytes())}

    def make_repo(self) -> None:
        repo = self.repo
        repo.mkdir()
        # BEGIN neutral knowledge include fixture; public bytes only.
        source = Path(__file__).resolve().parents[1]
        for relative in (rc_package.knowledge_files.EXAMPLE, rc_package.knowledge_files.INCLUDES):
            write(repo / relative, (source / relative).read_text())
        # END neutral knowledge fixture.
        write(repo / "Cargo.toml", '[workspace]\nresolver = "3"\nmembers = ["crates/demo-core", "crates/demo-cli"]\n\n'
              f'[workspace.package]\nversion = "{VERSION}"\nreadme = "README.md"\n')
        for name, extra in (("demo-core", ""), ("demo-cli", f'\n[dependencies]\ndemo-core = {{ path = "../demo-core", version = "{VERSION}" }}\nextdep = "1.0.0"\n')):
            write(repo / f"crates/{name}/Cargo.toml", f'[package]\nname = "{name}"\nversion.workspace = true\n'
                  f'readme.workspace = true\n{extra}')
            write(repo / f"crates/{name}/src/lib.rs", f"//! {name}\n")
        write(repo / "crates/demo-cli/scripts/run.sh", "#!/bin/sh\necho run\n", executable=True)
        write(repo / "Cargo.lock", 'version = 4\n\n[[package]]\nname = "demo-cli"\nversion = "9.9.9"\n\n'
              '[[package]]\nname = "demo-core"\nversion = "9.9.9"\n\n'
              f'[[package]]\nname = "extdep"\nversion = "1.0.0"\nsource = "{REGISTRY}"\n'
              f'checksum = "{self.extdep["checksum"]}"\n')
        write(repo / "release/package-set.toml", f'format = "zeno-fcis/package-set/1"\nversion = "{VERSION}"\n'
              'rust_toolchain = "1.97.1"\npublish_order = ["demo-core", "demo-cli"]\nprivate_packages = []\n\n'
              '[[binaries]]\npackage = "demo-cli"\ntarget = "zeno-fcis"\n')
        write(repo / "release/packaged-test-inputs.json", json.dumps({"format": "zeno-fcis/packaged-test-inputs/1",
                                                                     "inputs": []}) + "\n")
        write(repo / "rust-toolchain.toml", '[toolchain]\nchannel = "1.97.1"\n')
        write(repo / "README.md", "# Demo\n")
        write(repo / "LICENSE-APACHE", "Apache\n")
        write(repo / "LICENSE-MIT", "MIT\n")
        write(repo / "docs/INSTALLATION.md", "Requires glibc 2.39 or later.\n")
        write(repo / ".gitignore", "target/\n__pycache__/\n*.log\n")
        os.symlink("README.md", repo / "docs/README-link.md")
        git(repo, "init", "--quiet", "--initial-branch=main")
        git(repo, "add", "-A")
        git(repo, "commit", "--quiet", "-m", "fixture")
        git(repo, "tag", f"v{VERSION}")
        self.commit = git(repo, "rev-parse", "HEAD")

    def blob(self, path: str) -> bytes:
        return subprocess.run(["git", "show", f"HEAD:{path}"], cwd=self.repo, check=True,
                              stdout=subprocess.PIPE).stdout

    def crate_members(self, name: str, crates: dict[str, str]) -> dict[str, bytes]:
        directory = f"crates/{name}"
        members = {"Cargo.toml.orig": self.blob(f"{directory}/Cargo.toml"),
                   ".cargo_vcs_info.json": json.dumps({"git": {"sha1": self.commit},
                                                       "path_in_vcs": directory}).encode(),
                   "src/lib.rs": self.blob(f"{directory}/src/lib.rs"), "README.md": self.blob("README.md")}
        normalized = f'[package]\nname = "{name}"\nversion = "{VERSION}"\nreadme = "README.md"\n'
        # As `cargo package` writes it: every crate's lock has its own entry, with no source or checksum.
        lock = '# This file is automatically @generated by Cargo.\n# It is not intended for manual editing.\nversion = 4\n'
        if name == "demo-cli":
            normalized += f'\n[dependencies.demo-core]\nversion = "={VERSION}"\n\n[dependencies.extdep]\nversion = "1.0.0"\n'
            lock += (f'\n[[package]]\nname = "demo-cli"\nversion = "{VERSION}"\ndependencies = [\n "demo-core",\n'
                     f' "extdep",\n]\n\n[[package]]\nname = "demo-core"\nversion = "{VERSION}"\nsource = "{REGISTRY}"\n'
                     f'checksum = "{crates["demo-core"]}"\n\n[[package]]\nname = "extdep"\nversion = "1.0.0"\n'
                     f'source = "{REGISTRY}"\nchecksum = "{self.extdep["checksum"]}"\n')
            members["scripts/run.sh"] = self.blob(f"{directory}/scripts/run.sh")
        else:
            lock += f'\n[[package]]\nname = "{name}"\nversion = "{VERSION}"\n'
        members["Cargo.lock"] = lock.encode()
        members["Cargo.toml"] = normalized.encode()
        return members

    def host_dir(self, directory: Path, host: str, executable: bytes, journey: bool = True) -> Path:
        """A build-host-binary output folder for `host`."""
        binaries = directory / "binaries"
        binaries.mkdir(parents=True)
        binary = directory / "zeno-fcis"
        binary.write_bytes(executable)
        prefix = f"zeno-fcis-{VERSION}-{host}"
        archive = binaries / f"{prefix}.tar.gz"
        rc_package.deterministic_binary_archive(binary, archive, prefix, root=self.repo)
        record = rc_package.host_binary_record(
            version=VERSION, commit=self.commit, host=host, rustc=RUSTC[host].format(host=host), archive=archive,
            prefix=prefix, executable=binary, compiler_flags={}, commands=[])
        binary.unlink()
        rc_package.write_json(directory / "HOST-BINARY.json", record)
        rc_package.write_json(directory / f"COMPILED-INPUTS-{host}.json", self.compiled_inputs(host, executable))
        if journey:
            rc_package.write_json(directory / "INSTALLED-JOURNEY.json", journey_report(host, archive))
        return directory

    def compiled_inputs(self, host: str, executable: bytes) -> dict:
        files = self.extdep["files"]
        return {"format": "zeno-fcis/compiled-inputs/1", "host": host, "version": VERSION,
                "source_commit": self.commit, "executable": {"name": "zeno-fcis", "sha256": sha256(executable)},
                "workspace_inputs": [{"path": "crates/demo-cli/src/lib.rs",
                                      "sha256": sha256(self.blob("crates/demo-cli/src/lib.rs"))}],
                "registry_packages": [{"name": "extdep", "version": "1.0.0", "checksum": self.extdep["checksum"],
                                       "tree_sha256": custody.tree_digest(files), "files": len(files)}],
                "registry_inputs": [{"package": "extdep-1.0.0", "path": "src/lib.rs",
                                     "sha256": sha256(files["src/lib.rs"])}],
                "build_outputs": [], "env_deps": [{"name": "ZENO_FCIS_BUILD_TREE", "value": "", "private": False}],
                "unclassified": []}

    def release(self, directory: Path, hosts=(LINUX, MAC), executables: dict | None = None) -> Path:
        """A complete, indexed release set for `hosts`, as build plus merge-hosts would leave it."""
        executables = {LINUX: elf_executable(), MAC: macho_executable(), **(executables or {})}
        for folder in ("packages", "binaries", "docs", "source"):
            (directory / folder).mkdir(parents=True)
        crates: dict[str, str] = {}
        for name in ("demo-core", "demo-cli"):
            path = directory / "packages" / f"{name}-{VERSION}.crate"
            crate_archive(path, self.crate_members(name, crates), f"{name}-{VERSION}",
                          frozenset({"scripts/run.sh"}))
            crates[name] = sha256(path.read_bytes())
        (directory / "docs" / f"zeno-fcis-rustdoc-{VERSION}.tar.gz").write_bytes(b"rustdoc")
        tar = subprocess.run(["git", "archive", "--format=tar", f"--prefix=zeno-fcis-{VERSION}/", "HEAD"],
                             cwd=self.repo, check=True, stdout=subprocess.PIPE).stdout
        with (directory / "source" / f"zeno-fcis-{VERSION}-source.tar.gz").open("wb") as raw, \
                gzip.GzipFile(filename="", fileobj=raw, mode="wb", mtime=0) as compressed:
            compressed.write(tar)
        tree = custody.Tree(custody.Git(self.repo), self.commit)
        files = [{"kind": "symlink" if mode == "120000" else "file", "path": path, "sha256": sha256(blob),
                  "size": len(blob)} for path, (mode, blob) in sorted(tree.files.items())]
        rc_package.write_json(directory / "SOURCE-MANIFEST.json", {
            "clean": True, "commit": self.commit, "file_count": len(files), "files": files,
            "format": "zeno-fcis/source-manifest/1", "rust_toolchain_sha256": sha256(self.blob("rust-toolchain.toml")),
            "total_size": sum(item["size"] for item in files)})
        rc_package.write_json(directory / "PACKAGED-APPLICATION.json", {
            "source_commit": self.commit, "checker_source_clean": True,
            "archives": [{"archive": f"{name}-{VERSION}.crate", "sha256": digest, "source_clean": True}
                         for name, digest in sorted(crates.items())],
            "checker_inputs": [{"path": "Cargo.lock", "sha256": sha256(self.blob("Cargo.lock"))}],
            "packaged_tests": {"test_inputs_manifest_sha256": sha256(self.blob("release/packaged-test-inputs.json")),
                               "repository_inputs": []}})
        rc_package.write_json(directory / "SBOM.cdx.json", {"metadata": {
            "component": {"version": VERSION}, "properties": [{"name": "zeno-fcis:source-commit", "value": self.commit}]}})
        for host in hosts:
            folder = self.host_dir(self.root / f"build-{host}-{directory.name}", host, executables[host])
            (directory / "hosts" / host).mkdir(parents=True)
            archive = folder / "binaries" / f"zeno-fcis-{VERSION}-{host}.tar.gz"
            (directory / "binaries" / archive.name).write_bytes(archive.read_bytes())
            for name in ("HOST-BINARY.json", f"COMPILED-INPUTS-{host}.json", "INSTALLED-JOURNEY.json"):
                (directory / "hosts" / host / name).write_bytes((folder / name).read_bytes())
        rc_package.write_json(directory / "PROVENANCE-INPUTS.json", {
            "format": "zeno-fcis/provenance-inputs/1", "version": VERSION, "source_commit": self.commit,
            "rust_toolchain": "1.97.1", "rustc": RUSTC[LINUX].format(host=LINUX), "host_targets": sorted(hosts),
            "source_manifest_sha256": sha256((directory / "SOURCE-MANIFEST.json").read_bytes()),
            "packaged_test_inputs_sha256": sha256(self.blob("release/packaged-test-inputs.json")),
            "sbom_sha256": sha256((directory / "SBOM.cdx.json").read_bytes())})
        rc_package.write_release_index(directory, VERSION, self.commit)
        return directory

    def bindings(self, release: Path) -> dict:
        found = {}
        for record in sorted(release.glob("hosts/*/HOST-BINARY.json")):
            host = record.parent.name
            archive = f"zeno-fcis-{VERSION}-{host}.tar.gz"
            found[host] = {"archive": archive, "archive_sha256": sha256((release / "binaries" / archive).read_bytes()),
                           "record_sha256": sha256(record.read_bytes()),
                           "inputs_sha256": sha256((record.parent / f"COMPILED-INPUTS-{host}.json").read_bytes()),
                           "journey_sha256": sha256((record.parent / "INSTALLED-JOURNEY.json").read_bytes())}
        return found
