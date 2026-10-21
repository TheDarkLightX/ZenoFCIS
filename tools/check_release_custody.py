#!/usr/bin/env python3
"""Bind every artifact of an assembled release set to the exact source commit, by content.

Usage:
  python3 tools/check_release_custody.py --release DIR --report FILE [--repo DIR]
      [--bindings FILE] [--downloads FILE [--signing-key RELEASE-SIGNING-KEY.pub]]
      [--tag vVERSION [--remote-tag-commit SHA]] [--cargo-home DIR]

The checks, each with counts; C1-C9 always run and missing inputs fail.
C10 and C11 run only when their signing-stage inputs are supplied. A check that
does not run is listed in `not_run` and is never counted as passed: the status
is `passed` (exit 0) only when all eleven checks ran and passed, `failed`
(exit 1) when any check that ran failed, and otherwise `incomplete` (exit 2).

C1  Source state. HEAD is commit C with tree T; the index's stage-0 entries equal T
    (path, mode, object), with no unmerged stage; every working-tree file's
    recomputed blob ID and executable bit match; there are no untracked files, and
    ignored files lie only under target/, node_modules/ or a __pycache__ folder.
    `.git/index` itself is never hashed, so a stat-only index rewrite still passes.
C2  Versions and names. Cargo.toml and release/package-set.toml in T declare the set's
    version; every record names C. With --tag, the tag is v<version> and peels to C
    (and equals --remote-tag-commit when given). This checks consistency; it does not
    authenticate the tag, which on a tag run the checkout itself creates at the run's
    commit.
C3  SOURCE-MANIFEST.json equals a recomputation from T's blobs.
C4  The source archive: gzip without a name and with mtime 0; exactly the prefix and
    every path of T, with the blob bytes and executable bits; pax comment C.
C5  Every publish_order .crate: safe members; Cargo.toml.orig and every other member
    equal T's blobs; .cargo_vcs_info.json names C, the package path and no dirty
    state; the normalized manifest pins internal dependencies to =<version> and has
    no path; a packaged Cargo.lock has exactly one entry for the crate itself, as cargo
    writes it (no source, no checksum), and every other entry is one of the set's
    crates or equals the workspace lock.
C6  Every binary archive: exactly the executable and the four committed documents,
    ustar, normalized headers; the executable equals its host record, holds no
    build path, and its headers state the documented platform floor (ELF:
    interpreter and highest non-weak GLIBC version; Mach-O: arm64 only, macOS 11.0,
    a code signature). Each host's installed-binary journey passed on that archive.
C7  Each executable's recorded compiled inputs are blobs of T inside published
    crates (equal to the crate members), members of locked registry packages whose
    cached .crate matches the lock checksum, or build-script outputs; the build
    recorded an empty ZENO_FCIS_BUILD_TREE, and no variable it read is marked private
    or holds a personal home path.
C8  PACKAGED-APPLICATION.json names C, a clean checker source, the set's crates and
    the committed checker and test inputs.
C9  SHA256SUMS, RC-MANIFEST.json, the bundle and PROVENANCE-INPUTS.json equal a
    recomputation; the SBOM names the version and C.
C10 With --bindings, the bindings name exactly the set's hosts, each with its archive
    name and four hashes; each host record, compiled-inputs record, archive and journey
    equals the hash the producing job reported; rustc's commit-hash is the same on
    every host.
C11 With --downloads, the download list names each published asset (the host
    archives, the rustdoc archive and SBOM.cdx.json) once with its SHA256SUMS hash and,
    given --signing-key, RELEASE-SIGNING-KEY.pub with that file's hash; nothing else.

Nonclaims: nothing is rebuilt, so this is not a reproducible-build proof; it does not
authenticate the runner, compiler or linker; signature trust stays with signing.
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
import io
import json
import os
import re
import struct
import subprocess
import sys
import tarfile
import zipfile
from pathlib import Path, PurePosixPath

import tomllib

import check_release_privacy

ROOT = Path(__file__).resolve().parents[1]
FORMAT = "zeno-fcis/release-custody/2"
JOURNEY_SCHEMA = "zeno-fcis/installed-cli-journey/1"
GLIBC_FLOOR = (2, 39)
MACOS_MINIMUM = (11, 0)
ELF_INTERPRETER = "/lib64/ld-linux-x86-64.so.2"
BUILD_PATH_MARKERS = (b"/home/runner/work/", b"/Users/runner/work/", b"/private/var/folders/",
                      b"/tmp/zeno-fcis-rc/", b".cargo-target/")
HOME_PATH = check_release_privacy.PATTERNS["personal-home-path"]
DOCUMENT_MEMBERS = (("README.md", "README.md"), ("LICENSE-APACHE", "LICENSE-APACHE"),
                    ("LICENSE-MIT", "LICENSE-MIT"), ("INSTALLATION.md", "docs/INSTALLATION.md"))
CARGO_FILES = {"Cargo.toml", "Cargo.toml.orig", ".cargo_vcs_info.json", "Cargo.lock"}
DEPENDENCY_TABLES = ("dependencies", "dev-dependencies", "build-dependencies")
EXCLUDED_FROM_INDEX = {"RC-MANIFEST.json", "SHA256SUMS"}
FIXED_ZIP_TIME = (1980, 1, 1, 0, 0, 0)
SIGNING_KEY_NAME = "RELEASE-SIGNING-KEY.pub"
REGISTRY_SOURCE = "registry+https://github.com/rust-lang/crates.io-index"
BINDING_FIELDS = ("archive", "archive_sha256", "record_sha256", "inputs_sha256", "journey_sha256")


def published_asset(name: str) -> bool:
    """Whether a SHA256SUMS entry is a download users check: a host archive, the rustdoc archive or the SBOM."""
    return name.startswith("binaries/") or (name.startswith("docs/") and name.endswith(".tar.gz")) \
        or name == "SBOM.cdx.json"


class CustodyError(ValueError):
    """A custody input that cannot be read at all."""


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def load_json(path: Path) -> dict:
    document = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(document, dict):
        raise CustodyError(f"{path.name} is not a JSON object")
    return document


# ---------------------------------------------------------------- git, by content


class Git:
    """Read-only git with optional locks off, so no command rewrites the index."""

    def __init__(self, repo: Path):
        self.repo = repo

    def run(self, *arguments: str, data: bytes | None = None) -> bytes:
        result = subprocess.run(["git", "--no-optional-locks", *arguments], cwd=self.repo, input=data,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)
        if result.returncode != 0:
            raise CustodyError(f"git {' '.join(arguments[:3])} failed: "
                               f"{result.stderr.decode(errors='replace').strip()[:300]}")
        return result.stdout

    def text(self, *arguments: str) -> str:
        return self.run(*arguments).decode("utf-8", errors="surrogateescape").strip()

    def records(self, *arguments: str) -> list[str]:
        raw = self.run(*arguments)
        return [item.decode("utf-8", errors="surrogateescape") for item in raw.split(b"\0") if item]

    def blobs(self, objects: list[str]) -> dict[str, bytes]:
        if not objects:
            return {}
        output = self.run("cat-file", "--batch", data="".join(f"{name}\n" for name in objects).encode())
        found, offset = {}, 0
        for name in objects:
            end = output.index(b"\n", offset)
            header = output[offset:end].decode().split()
            if len(header) != 3 or header[1] != "blob":
                raise CustodyError(f"object {name} is not a blob")
            size = int(header[2])
            found[name] = output[end + 1:end + 1 + size]
            offset = end + 1 + size + 1
        return found


def tree_digest(files: dict[str, bytes]) -> str:
    """One digest over an unpacked package: its sorted paths and their SHA-256."""
    return sha256_bytes("".join(f"{sha256_bytes(files[name])}  {name}\n" for name in sorted(files)).encode())


def blob_id(data: bytes, algorithm: str) -> str:
    return hashlib.new(algorithm, b"blob %d\0" % len(data) + data).hexdigest()


def object_algorithm(commit: str) -> str:
    return {40: "sha1", 64: "sha256"}[len(commit)]


class Tree:
    """Tree T of commit C: every path's mode and blob bytes."""

    def __init__(self, git: Git, commit: str):
        self.commit = commit
        self.tree = git.text("rev-parse", f"{commit}^{{tree}}")
        self.entries: dict[str, tuple[str, str]] = {}
        self.unsupported: list[str] = []
        for record in git.records("ls-tree", "-r", "-z", "--full-tree", self.tree):
            meta, path = record.split("\t", 1)
            mode, kind, name = meta.split()
            if kind != "blob":
                self.unsupported.append(f"{path} is a {kind}")
                continue
            self.entries[path] = (mode, name)
        contents = git.blobs(sorted({name for _, name in self.entries.values()}))
        self.files = {path: (mode, contents[name]) for path, (mode, name) in self.entries.items()}

    def blob(self, path: str) -> bytes:
        if path not in self.files:
            raise CustodyError(f"{path} is not in the source tree")
        return self.files[path][1]

    def executable(self, path: str) -> bool:
        return self.files[path][0] == "100755"


def ignored_allowed(path: str) -> bool:
    """Cargo target folders, npm's node_modules and Python's __pycache__, wherever they are."""
    return any(part in ("target", "node_modules", "__pycache__") for part in PurePosixPath(path.rstrip("/")).parts)


def source_state(repo: Path) -> dict:
    """C1, evaluated at the start and end of every assembly stage."""
    return source_state_and_tree(repo)[0]


def source_state_and_tree(repo: Path) -> tuple[dict, Tree]:
    git = Git(repo)
    commit = git.text("rev-parse", "HEAD")
    problems: list[str] = []
    if re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", commit) is None:
        raise CustodyError("HEAD is not a full lowercase commit")
    algorithm = object_algorithm(commit)
    tree = Tree(git, commit)
    problems.extend(tree.unsupported)
    index: dict[str, tuple[str, str]] = {}
    for record in git.records("ls-files", "-s", "-z"):
        meta, path = record.split("\t", 1)
        mode, name, stage = meta.split()
        if stage != "0":
            problems.append(f"unmerged index entry: {path}")
            continue
        index[path] = (mode, name)
    if index != tree.entries:
        changed = sorted(set(index.items()) ^ set(tree.entries.items()))
        problems.append(f"{len(changed)} index entries differ from the commit, first {changed[0][0]}")
    for path, (mode, name) in sorted(tree.entries.items()):
        local = repo / path
        if mode == "120000":
            ok = local.is_symlink() and blob_id(os.readlink(local).encode("utf-8", "surrogateescape"),
                                                algorithm) == name
        else:
            ok = (local.is_file() and not local.is_symlink() and blob_id(local.read_bytes(), algorithm) == name
                  and bool(local.stat().st_mode & 0o100) == (mode == "100755"))
        if not ok:
            problems.append(f"working tree differs from the commit: {path}")
    untracked = git.records("ls-files", "--others", "--exclude-standard", "-z")
    problems.extend(f"untracked file: {path}" for path in untracked)
    ignored = git.records("ls-files", "--others", "--ignored", "--exclude-standard", "--directory", "-z")
    problems.extend(f"ignored file outside the allowlist: {path}" for path in ignored if not ignored_allowed(path))
    return {"passed": not problems, "commit": commit, "tree": tree.tree, "counts": {
        "tracked": len(tree.entries), "untracked": len(untracked), "ignored": len(ignored)},
        "ignored": sorted(ignored), "problems": problems}, tree


# ---------------------------------------------------------------- executable headers


def elf_facts(data: bytes) -> dict:
    """Interpreter and GNU version needs of a 64-bit little-endian ELF executable."""
    if data[:4] != b"\x7fELF" or data[4] != 2 or data[5] != 1:
        raise CustodyError("not a 64-bit little-endian ELF file")
    machine, = struct.unpack_from("<H", data, 18)
    phoff, shoff = struct.unpack_from("<QQ", data, 0x20)
    phentsize, phnum, shentsize, shnum, shstrndx = struct.unpack_from("<HHHHH", data, 0x36)
    interpreter = None
    for index in range(phnum):
        p_type, _, p_offset, _, _, p_filesz = struct.unpack_from("<IIQQQQ", data, phoff + index * phentsize)
        if p_type == 3:
            interpreter = data[p_offset:p_offset + p_filesz].rstrip(b"\0").decode()
    sections = []
    for index in range(shnum):
        name, kind, _, _, offset, size, link, info = struct.unpack_from("<IIQQQQII", data, shoff + index * shentsize)
        sections.append({"name": name, "offset": offset, "size": size, "info": info})

    def string(offset: int) -> str:
        return data[offset:data.index(b"\0", offset)].decode()

    names = {string(sections[shstrndx]["offset"] + section["name"]): section for section in sections}
    needs = []
    if ".gnu.version_r" in names and ".dynstr" in names:
        dynstr, offset = names[".dynstr"]["offset"], names[".gnu.version_r"]["offset"]
        for _ in range(names[".gnu.version_r"]["info"]):
            _, count, library, auxiliary, following = struct.unpack_from("<HHIII", data, offset)
            entry = offset + auxiliary
            for _ in range(count):
                _, flags, _, name, step = struct.unpack_from("<IHHII", data, entry)
                needs.append({"library": string(dynstr + library), "version": string(dynstr + name),
                              "weak": bool(flags & 2)})
                entry += step
            offset += following
    glibc = sorted(tuple(int(part) for part in need["version"].removeprefix("GLIBC_").split("."))
                   for need in needs if need["version"].startswith("GLIBC_") and not need["weak"])
    return {"machine": machine, "interpreter": interpreter, "glibc_floor": list(glibc[-1]) if glibc else None,
            "version_needs": len(needs)}


def macho_facts(data: bytes) -> dict:
    """Architecture, platform, minimum OS and code-signature presence of a thin Mach-O executable."""
    if data[:4] in (b"\xca\xfe\xba\xbe", b"\xbe\xba\xfe\xca"):
        raise CustodyError("a universal Mach-O file is not a single-architecture executable")
    if data[:4] != b"\xcf\xfa\xed\xfe":
        raise CustodyError("not a 64-bit little-endian Mach-O file")
    cputype, _, filetype, ncmds, _ = struct.unpack_from("<IIIII", data, 4)
    offset, platform, minimum, signed = 32, None, None, False
    for _ in range(ncmds):
        command, size = struct.unpack_from("<II", data, offset)
        if command == 0x32:  # LC_BUILD_VERSION
            platform, version = struct.unpack_from("<II", data, offset + 8)
            minimum = (version >> 16, (version >> 8) & 0xFF)
        elif command == 0x24:  # LC_VERSION_MIN_MACOSX
            version, = struct.unpack_from("<I", data, offset + 8)
            platform, minimum = 1, (version >> 16, (version >> 8) & 0xFF)
        elif command == 0x1D:  # LC_CODE_SIGNATURE
            signed = True
        offset += size
    return {"arm64": cputype == 0x0100000C, "executable": filetype == 2, "macos": platform == 1,
            "minimum_os": list(minimum) if minimum else None, "code_signature": signed}


def executable_problems(data: bytes, host: str) -> list[str]:
    problems = [f"executable holds a build path ({marker.decode()})" for marker in BUILD_PATH_MARKERS
                if marker in data]
    if HOME_PATH.search(data):
        problems.append("executable holds a personal home path")
    try:
        if host == "x86_64-unknown-linux-gnu":
            facts = elf_facts(data)
            if facts["machine"] != 62 or facts["interpreter"] != ELF_INTERPRETER:
                problems.append(f"ELF machine/interpreter: {facts['machine']}, {facts['interpreter']}")
            if facts["glibc_floor"] != list(GLIBC_FLOOR):
                problems.append(f"highest non-weak glibc requirement {facts['glibc_floor']}, documented "
                                f"{list(GLIBC_FLOOR)}")
        elif host == "aarch64-apple-darwin":
            facts = macho_facts(data)
            if not (facts["arm64"] and facts["executable"] and facts["macos"] and facts["code_signature"]):
                problems.append(f"Mach-O facts: {facts}")
            if facts["minimum_os"] != list(MACOS_MINIMUM):
                problems.append(f"minimum macOS {facts['minimum_os']}, expected {list(MACOS_MINIMUM)}")
        else:
            problems.append(f"no header rule for host {host}")
    except (CustodyError, struct.error, ValueError, IndexError) as error:
        problems.append(f"executable headers unreadable: {error}")
    return problems


def journey_problems(report: object, host: str, version: str, archive_sha256: object) -> list[str]:
    """An installed-binary journey report that passed on exactly this host's archive."""
    if not isinstance(report, dict) or not isinstance(report.get("archive"), dict):
        return [f"{host}: the installed-binary journey report is malformed"]
    found = (report.get("schema"), report.get("status"), report.get("host"), report.get("version"),
             report["archive"].get("sha256"))
    if found != (JOURNEY_SCHEMA, "passed", host, version, archive_sha256):
        return [f"{host}: the installed-binary journey is not a passed run on this archive: {found}"]
    return []


# ---------------------------------------------------------------- archives


def gzip_problems(data: bytes, label: str) -> list[str]:
    if len(data) < 10 or data[:3] != b"\x1f\x8b\x08":
        return [f"{label}: not a deflate gzip stream"]
    flags, mtime = data[3], struct.unpack_from("<I", data, 4)[0]
    problems = []
    if flags & 0x1C:
        problems.append(f"{label}: gzip header carries a name, comment or extra field")
    if mtime != 0:
        problems.append(f"{label}: gzip mtime {mtime}")
    return problems


def tar_members(data: bytes) -> tarfile.TarFile:
    return tarfile.open(fileobj=io.BytesIO(gzip.decompress(data)), mode="r:")


def plain_ustar(data: bytes) -> bool:
    """Every member has one POSIX ustar header and no pax or GNU extended header."""
    raw = gzip.decompress(data)
    with tarfile.open(fileobj=io.BytesIO(raw), mode="r:") as archive:
        return all(raw[member.offset + 257:member.offset + 265] == b"ustar\x0000"
                   and member.offset_data == member.offset + 512 for member in archive.getmembers())


def binary_archive_problems(path: Path, version: str, host: str, tree: Tree, record: dict | None) -> tuple[list, dict]:
    problems = gzip_problems(path.read_bytes(), path.name)
    prefix = f"zeno-fcis-{version}-{host}"
    if path.name != f"{prefix}.tar.gz":
        problems.append(f"{path.name}: archive name is not {prefix}.tar.gz")
    expected = {f"{prefix}/zeno-fcis": 0o755, **{f"{prefix}/{name}": 0o644 for name, _ in DOCUMENT_MEMBERS}}
    executable = b""
    with tar_members(path.read_bytes()) as archive:
        members = archive.getmembers()
        if not plain_ustar(path.read_bytes()):
            problems.append(f"{path.name}: not a plain ustar archive")
        names = [member.name for member in members]
        if sorted(names) != sorted(expected) or len(names) != len(set(names)):
            problems.append(f"{path.name}: members {sorted(names)}")
        for member in members:
            if not member.isfile() or (member.mtime, member.uid, member.gid, member.uname, member.gname) != (0, 0, 0, "", ""):
                problems.append(f"{path.name}:{member.name}: not a normalized regular file")
                continue
            if expected.get(member.name) != member.mode:
                problems.append(f"{path.name}:{member.name}: mode {oct(member.mode)}")
            content = archive.extractfile(member).read()
            short = member.name.removeprefix(prefix + "/")
            if short == "zeno-fcis":
                executable = content
            else:
                committed = dict(DOCUMENT_MEMBERS).get(short)
                if committed is None or content != tree.blob(committed):
                    problems.append(f"{path.name}:{short} differs from the committed {committed}")
    if not executable:
        problems.append(f"{path.name}: no executable")
    elif record is None:
        problems.append(f"{path.name}: no host record")
    elif sha256_bytes(executable) != record.get("executable", {}).get("sha256"):
        problems.append(f"{path.name}: executable differs from its host record")
    if executable:
        problems.extend(f"{path.name}: {problem}" for problem in executable_problems(executable, host))
    return problems, {"executable_sha256": sha256_bytes(executable) if executable else None}


def source_archive_problems(path: Path, version: str, tree: Tree) -> list[str]:
    data = path.read_bytes()
    problems = gzip_problems(data, path.name)
    prefix = f"zeno-fcis-{version}"
    expected_files = {f"{prefix}/{name}": name for name in tree.files}
    expected_dirs = {prefix} | {f"{prefix}/{parent}" for name in tree.files
                                for parent in map(str, PurePosixPath(name).parents) if parent != "."}
    seen_files, seen_dirs = set(), set()
    with tar_members(data) as archive:
        if archive.pax_headers.get("comment") != tree.commit:
            problems.append(f"{path.name}: pax comment is not the source commit")
        for member in archive.getmembers():
            name = member.name.rstrip("/")
            if member.isdir():
                seen_dirs.add(name)
                continue
            source = expected_files.get(name)
            if source is None:
                problems.append(f"{path.name}: unexpected member {name}")
                continue
            seen_files.add(name)
            mode, blob = tree.files[source]
            if mode == "120000":
                if not member.issym() or member.linkname.encode("utf-8", "surrogateescape") != blob:
                    problems.append(f"{path.name}: symlink {source} differs")
            elif not member.isfile() or archive.extractfile(member).read() != blob:
                problems.append(f"{path.name}: {source} differs from its blob")
            elif bool(member.mode & 0o111) != (mode == "100755") or member.mode & 0o022 not in (0, 0o020):
                problems.append(f"{path.name}: {source} mode {oct(member.mode)}")
    missing = sorted(set(expected_files) - seen_files)
    if missing:
        problems.append(f"{path.name}: {len(missing)} paths of the commit missing, first {missing[0]}")
    if seen_dirs != expected_dirs:
        problems.append(f"{path.name}: directory entries differ from the commit's folders")
    return problems


def package_directories(tree: Tree) -> dict[str, str]:
    workspace = tomllib.loads(tree.blob("Cargo.toml").decode())
    found = {}
    for member in workspace["workspace"]["members"]:
        manifest = tomllib.loads(tree.blob(f"{member}/Cargo.toml").decode())
        found[manifest["package"]["name"]] = member
    return found


def lock_entries(text: str) -> dict[tuple[str, str], dict]:
    return {(entry["name"], entry["version"]): entry for entry in tomllib.loads(text).get("package", [])}


def dependency_problems(manifest: dict, internal: set[str], version: str, label: str) -> list[str]:
    problems = []
    tables = [manifest.get(name, {}) for name in DEPENDENCY_TABLES]
    for target in manifest.get("target", {}).values():
        tables.extend(target.get(name, {}) for name in DEPENDENCY_TABLES)
    for table in tables:
        for key, value in table.items():
            spec = value if isinstance(value, dict) else {"version": value}
            if "path" in spec or "git" in spec:
                problems.append(f"{label}: dependency {key} keeps a path or git source")
            if spec.get("package", key) in internal and spec.get("version") != f"={version}":
                problems.append(f"{label}: internal dependency {key} is not pinned to ={version}")
    return problems


def packaged_lock_problems(text: str, name: str, version: str, internal: set[str], workspace_lock: dict,
                           crate_hashes: dict[str, str], label: str) -> list[str]:
    """The Cargo.lock that `cargo package` wrote into the crate `name`.

    Cargo writes the crate's own entry with no source and no checksum. Every other internal entry
    must be one of this set's archives from the registry, and every external entry must equal
    the workspace lock."""
    problems, seen, own = [], set(), 0
    for entry in tomllib.loads(text).get("package", []):
        package, package_version = entry.get("name"), entry.get("version")
        if (package, package_version) in seen:
            problems.append(f"{label}: Cargo.lock repeats {package} {package_version}")
        seen.add((package, package_version))
        if (package, package_version) == (name, version) and "source" not in entry and "checksum" not in entry:
            own += 1
        elif package in internal:
            if package_version != version or entry.get("source") != REGISTRY_SOURCE \
                    or crate_hashes.get(package) is None or entry.get("checksum") != crate_hashes[package]:
                problems.append(f"{label}: Cargo.lock entry {package} is not this set's crate")
        elif workspace_lock.get((package, package_version)) is None or any(
                entry.get(key) != workspace_lock[(package, package_version)].get(key) for key in ("source", "checksum")):
            problems.append(f"{label}: Cargo.lock entry {package} {package_version} differs from the workspace lock")
    if own != 1:
        problems.append(f"{label}: Cargo.lock has {own} entries for {name} {version} itself, not one")
    return problems


def readme_source(original: dict, directory: str, tree: Tree) -> str | None:
    readme = original.get("package", {}).get("readme")
    if isinstance(readme, dict) and readme.get("workspace"):
        readme = tomllib.loads(tree.blob("Cargo.toml").decode())["workspace"]["package"].get("readme")
        return readme if isinstance(readme, str) else None
    if isinstance(readme, str):
        return str(PurePosixPath(os.path.normpath(f"{directory}/{readme}")))
    return None


def crate_problems(path: Path, name: str, version: str, directory: str, tree: Tree, internal: set[str],
                   workspace_lock: dict, crate_hashes: dict[str, str]) -> tuple[list, dict]:
    problems = []
    top = f"{name}-{version}"
    members: dict[str, tuple[bytes, int]] = {}
    with tarfile.open(path, mode="r:gz") as archive:
        for member in archive.getmembers():
            relative = PurePosixPath(member.name)
            if (relative.is_absolute() or ".." in relative.parts or not relative.parts or relative.parts[0] != top
                    or member.issym() or member.islnk() or not (member.isdir() or member.isfile())):
                problems.append(f"{path.name}: unsafe member {member.name}")
                continue
            if member.isfile():
                inner = PurePosixPath(*relative.parts[1:]).as_posix()
                if inner in members:
                    problems.append(f"{path.name}: duplicate member {inner}")
                members[inner] = (archive.extractfile(member).read(), member.mode)
    original_text = tree.blob(f"{directory}/Cargo.toml")
    if members.get("Cargo.toml.orig", (None,))[0] != original_text:
        problems.append(f"{path.name}: Cargo.toml.orig differs from {directory}/Cargo.toml")
    try:
        vcs = json.loads(members[".cargo_vcs_info.json"][0])
        git_info = vcs.get("git", {})
        if git_info.get("sha1") != tree.commit or git_info.get("dirty", False) is not False \
                or vcs.get("path_in_vcs") != directory:
            problems.append(f"{path.name}: .cargo_vcs_info.json does not name the clean commit and {directory}")
    except (KeyError, ValueError):
        problems.append(f"{path.name}: unreadable .cargo_vcs_info.json")
    try:
        manifest = tomllib.loads(members["Cargo.toml"][0].decode())
        if (manifest["package"]["name"], manifest["package"]["version"]) != (name, version):
            problems.append(f"{path.name}: normalized manifest names another package")
        problems.extend(dependency_problems(manifest, internal, version, path.name))
    except (KeyError, ValueError, UnicodeError):
        problems.append(f"{path.name}: unreadable normalized Cargo.toml")
    if "Cargo.lock" in members:
        problems.extend(packaged_lock_problems(members["Cargo.lock"][0].decode(), name, version, internal,
                                               workspace_lock, crate_hashes, path.name))
    readme = readme_source(tomllib.loads(original_text.decode()), directory, tree)
    checked = 0
    for inner, (content, mode) in sorted(members.items()):
        if inner in CARGO_FILES:
            continue
        source = f"{directory}/{inner}"
        if source not in tree.files and readme is not None and inner == PurePosixPath(readme).name:
            source = readme
        if source not in tree.files:
            problems.append(f"{path.name}: {inner} is not a committed file of {directory}")
            continue
        checked += 1
        if content != tree.blob(source) or bool(mode & 0o111) != tree.executable(source):
            problems.append(f"{path.name}: {inner} differs from {source}")
    tracked = {name[len(directory) + 1:] for name in tree.files if name.startswith(directory + "/")}
    unpackaged = sorted(tracked - set(members))
    return problems, {"members": len(members), "compared": checked, "unpackaged_tracked_files": unpackaged}


# ---------------------------------------------------------------- the checks


class Custody:
    def __init__(self, release: Path, repo: Path, cargo_home: Path):
        self.release, self.repo, self.cargo_home = release, repo, cargo_home
        self.checks: dict[str, dict] = {}
        self.manifest = load_json(release / "RC-MANIFEST.json")
        self.version = self.manifest.get("version")
        self.hosts = {path.parent.name: load_json(path) for path in sorted(release.glob("hosts/*/HOST-BINARY.json"))}

    def record(self, name: str, problems: list[str], **counts) -> None:
        self.checks[name] = {"passed": not problems, "counts": counts, "problems": problems}

    def c1(self) -> None:
        state, self.tree = source_state_and_tree(self.repo)
        self.commit = state["commit"]
        self.record("C1-source-state", state["problems"], **state["counts"])

    def c2(self, tag: str | None, remote: str | None) -> None:
        problems = []
        declared = tomllib.loads(self.tree.blob("Cargo.toml").decode())["workspace"]["package"]["version"]
        package_set = tomllib.loads(self.tree.blob("release/package-set.toml").decode())["version"]
        if not (declared == package_set == self.version):
            problems.append(f"versions differ: Cargo.toml {declared}, package set {package_set}, set {self.version}")
        named = {"RC-MANIFEST.json": self.manifest.get("source_commit"),
                 "PROVENANCE-INPUTS.json": load_json(self.release / "PROVENANCE-INPUTS.json").get("source_commit"),
                 "SOURCE-MANIFEST.json": load_json(self.release / "SOURCE-MANIFEST.json").get("commit"),
                 "PACKAGED-APPLICATION.json": load_json(self.release / "PACKAGED-APPLICATION.json").get("source_commit")}
        for host, record in self.hosts.items():
            named[f"hosts/{host}/HOST-BINARY.json"] = record.get("source_commit")
        problems.extend(f"{name} names {value}, not {self.commit}" for name, value in named.items()
                        if value != self.commit)
        tag_state = "not-a-tag-run"
        if tag is not None:
            peeled = Git(self.repo).text("rev-parse", f"refs/tags/{tag}^{{commit}}")
            if tag != f"v{self.version}" or peeled != self.commit:
                problems.append(f"tag {tag} peels to {peeled}, expected v{self.version} at {self.commit}")
            if remote is not None and remote != self.commit:
                problems.append(f"the remote tag names {remote}")
            tag_state = tag
        self.record("C2-tag-and-version", problems, records=len(named), tag=tag_state)

    def c3(self) -> None:
        algorithm_files = []
        for path in sorted(self.tree.files, key=lambda item: item.encode("utf-8", "surrogateescape")):
            mode, blob = self.tree.files[path]
            algorithm_files.append({"kind": "symlink" if mode == "120000" else "file", "path": path,
                                    "sha256": sha256_bytes(blob), "size": len(blob)})
        expected = {"clean": True, "commit": self.commit, "file_count": len(algorithm_files), "files": algorithm_files,
                    "format": "zeno-fcis/source-manifest/1",
                    "rust_toolchain_sha256": sha256_bytes(self.tree.blob("rust-toolchain.toml")),
                    "total_size": sum(item["size"] for item in algorithm_files)}
        actual = load_json(self.release / "SOURCE-MANIFEST.json")
        problems = [f"SOURCE-MANIFEST.json {key} differs from the commit's blobs" for key in sorted(set(expected) | set(actual))
                    if expected.get(key) != actual.get(key)]
        self.record("C3-source-manifest", problems, files=len(algorithm_files))

    def c4(self) -> None:
        path = self.release / "source" / f"zeno-fcis-{self.version}-source.tar.gz"
        problems = source_archive_problems(path, self.version, self.tree) if path.is_file() else [f"missing {path.name}"]
        self.record("C4-source-archive", problems, paths=len(self.tree.files))

    def c5(self) -> None:
        package_set = tomllib.loads(self.tree.blob("release/package-set.toml").decode())
        order = package_set["publish_order"]
        directories = package_directories(self.tree)
        internal = set(directories)
        workspace_lock = lock_entries(self.tree.blob("Cargo.lock").decode())
        expected = {f"{name}-{self.version}.crate" for name in order}
        actual = {path.name for path in (self.release / "packages").glob("*.crate")}
        problems = [f"crate set differs: missing {sorted(expected - actual)}, extra {sorted(actual - expected)}"] \
            if actual != expected else []
        crate_hashes = {name: sha256_file(self.release / "packages" / f"{name}-{self.version}.crate")
                        for name in order if f"{name}-{self.version}.crate" in actual}
        compared = 0
        for name in order:
            path = self.release / "packages" / f"{name}-{self.version}.crate"
            if not path.is_file() or name not in directories:
                problems.append(f"no archive or package folder for {name}")
                continue
            found, counts = crate_problems(path, name, self.version, directories[name], self.tree, internal,
                                           workspace_lock, crate_hashes)
            problems.extend(found)
            compared += counts["compared"]
        self.directories = directories
        self.record("C5-crate-archives", problems, crates=len(actual), members_compared=compared)

    def c6(self) -> None:
        problems, archives = [], sorted((self.release / "binaries").glob("*.tar.gz"))
        prefix = f"zeno-fcis-{self.version}-"
        hosts = set()
        for path in archives:
            host = path.name.removeprefix(prefix).removesuffix(".tar.gz")
            hosts.add(host)
            found, _ = binary_archive_problems(path, self.version, host, self.tree, self.hosts.get(host))
            problems.extend(found)
            record = self.hosts.get(host, {})
            archive = record.get("archive", {})
            if archive.get("name") != path.name or archive.get("sha256") != sha256_file(path):
                problems.append(f"{path.name}: differs from hosts/{host}/HOST-BINARY.json")
            journey = self.release / "hosts" / host / "INSTALLED-JOURNEY.json"
            problems.extend(journey_problems(load_json(journey), host, self.version, sha256_file(path))
                            if journey.is_file() else [f"{host}: no installed-binary journey"])
        if hosts != set(self.hosts):
            problems.append(f"archive hosts {sorted(hosts)} differ from host records {sorted(self.hosts)}")
        self.archive_hosts = hosts
        self.record("C6-binary-archives", problems, archives=len(archives))

    def registry_crate(self, name: str, version: str) -> tuple[str | None, dict[str, bytes]]:
        candidates = sorted(self.cargo_home.glob(f"registry/cache/*/{name}-{version}.crate"))
        if not candidates:
            return None, {}
        data = candidates[0].read_bytes()
        files = {}
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
            for member in archive.getmembers():
                if member.isfile():
                    files[PurePosixPath(*PurePosixPath(member.name).parts[1:]).as_posix()] = \
                        archive.extractfile(member).read()
        return sha256_bytes(data), files

    def c7(self) -> None:
        problems, counted = [], {"workspace": 0, "registry_packages": 0, "registry_inputs": 0, "build_outputs": 0}
        workspace_lock = lock_entries(self.tree.blob("Cargo.lock").decode())
        crates: dict[str, dict[str, bytes]] = {}
        for name, directory in self.directories.items():
            path = self.release / "packages" / f"{name}-{self.version}.crate"
            if path.is_file():
                with tarfile.open(path, mode="r:gz") as archive:
                    crates[directory] = {PurePosixPath(*PurePosixPath(member.name).parts[1:]).as_posix():
                                         archive.extractfile(member).read()
                                         for member in archive.getmembers() if member.isfile()}
        for host, record in self.hosts.items():
            path = self.release / "hosts" / host / f"COMPILED-INPUTS-{host}.json"
            if not path.is_file():
                problems.append(f"missing {path.name}")
                continue
            inputs = load_json(path)
            if (inputs.get("host"), inputs.get("source_commit"), inputs.get("version")) != (host, self.commit, self.version) \
                    or inputs.get("executable", {}).get("sha256") != record.get("executable", {}).get("sha256"):
                problems.append(f"{path.name}: host, commit, version or executable differs from its host record")
            if inputs.get("unclassified"):
                problems.append(f"{path.name}: {len(inputs['unclassified'])} unclassified inputs")
            for item in inputs.get("workspace_inputs", []):
                counted["workspace"] += 1
                source = item["path"]
                directory = next((d for d in crates if source.startswith(d + "/")), None)
                member = source[len(directory) + 1:] if directory is not None else None
                # Cargo normalizes the published manifest; its .orig member
                # preserves the source manifest used by the workspace build.
                if member == "Cargo.toml":
                    member = "Cargo.toml.orig"
                if source not in self.tree.files or sha256_bytes(self.tree.blob(source)) != item["sha256"]:
                    problems.append(f"{path.name}: {source} is not this commit's blob")
                elif directory is None or crates[directory].get(member) != self.tree.blob(source):
                    problems.append(f"{path.name}: {source} is not a member of a published crate")
            contents: dict[str, dict[str, bytes]] = {}
            for package in inputs.get("registry_packages", []):
                counted["registry_packages"] += 1
                locked = workspace_lock.get((package["name"], package["version"]))
                digest, files = self.registry_crate(package["name"], package["version"])
                if locked is None or locked.get("checksum") != package.get("checksum") or digest != package.get("checksum"):
                    problems.append(f"{path.name}: registry package {package['name']} {package['version']} does not "
                                    "match the lock and its cached archive")
                    continue
                if tree_digest(files) != package.get("tree_sha256"):
                    problems.append(f"{path.name}: unpacked tree of {package['name']} differs from its archive")
                contents[f"{package['name']}-{package['version']}"] = files
            for item in inputs.get("registry_inputs", []):
                counted["registry_inputs"] += 1
                files = contents.get(item["package"], {})
                if item["path"] not in files or sha256_bytes(files[item["path"]]) != item["sha256"]:
                    problems.append(f"{path.name}: registry input {item['package']}/{item['path']} is not its archive member")
            counted["build_outputs"] += len(inputs.get("build_outputs", []))
            # Every entry: one variable, such as OUT_DIR, can be read once per crate with different values.
            for entry in inputs.get("env_deps", []):
                name, value = entry.get("name"), entry.get("value")
                if name == "ZENO_FCIS_BUILD_TREE" and value not in ("", None):
                    problems.append(f"{path.name}: ZENO_FCIS_BUILD_TREE was not empty")
                if entry.get("private") is not False:
                    problems.append(f"{path.name}: build environment {name} is not recorded as non-private")
                if value and HOME_PATH.search(value.encode("utf-8", "surrogateescape")):
                    problems.append(f"{path.name}: build environment {name} holds a private path")
        self.record("C7-compiled-inputs", problems, **counted)

    def c8(self) -> None:
        document = load_json(self.release / "PACKAGED-APPLICATION.json")
        problems = []
        if document.get("source_commit") != self.commit or document.get("checker_source_clean") is not True:
            problems.append("PACKAGED-APPLICATION.json does not name the clean commit")
        archives = {item.get("archive"): item.get("sha256") for item in document.get("archives", [])}
        actual = {path.name: sha256_file(path) for path in (self.release / "packages").glob("*.crate")}
        if archives != actual:
            problems.append("PACKAGED-APPLICATION.json archives differ from packages/")
        tests = document.get("packaged_tests", {})
        if tests.get("test_inputs_manifest_sha256") != sha256_bytes(self.tree.blob("release/packaged-test-inputs.json")):
            problems.append("the packaged test-input manifest is not the committed one")
        reviewed = {item["path"]: item["sha256"] for item in json.loads(self.tree.blob("release/packaged-test-inputs.json"))["inputs"]}
        for item in document.get("checker_inputs", []):
            if item.get("path") not in self.tree.files or sha256_bytes(self.tree.blob(item["path"])) != item.get("sha256"):
                problems.append(f"checker input {item.get('path')} is not the committed blob")
        for item in tests.get("repository_inputs", []):
            if item.get("path") not in self.tree.files or sha256_bytes(self.tree.blob(item["path"])) != item.get("sha256") \
                    or reviewed.get(item["path"]) != item.get("sha256"):
                problems.append(f"repository input {item.get('path')} differs from its blob or the reviewed manifest")
        self.record("C8-packaged-evidence", problems, archives=len(archives),
                    checker_inputs=len(document.get("checker_inputs", [])))

    def c9(self) -> None:
        problems = []
        bundle_name = f"zeno-fcis-{self.version}-rc-bundle.zip"
        files = {path.relative_to(self.release).as_posix(): path for path in sorted(self.release.rglob("*"))
                 if path.is_file()}
        listed = {}
        for line in (self.release / "SHA256SUMS").read_text().splitlines():
            digest, separator, name = line.partition("  ")
            if separator != "  " or name in listed:
                problems.append(f"SHA256SUMS: malformed or repeated line for {name}")
            listed[name] = digest
        expected = {name for name in files if name not in ("SHA256SUMS", bundle_name)}
        if set(listed) != expected:
            problems.append(f"SHA256SUMS lists {sorted(set(listed) ^ expected)[:3]} wrongly")
        problems.extend(f"SHA256SUMS: {name} hash differs" for name, digest in listed.items()
                        if name in files and sha256_file(files[name]) != digest)
        artifacts = [{"path": name, "sha256": sha256_file(path), "size": path.stat().st_size}
                     for name, path in files.items() if name not in ("RC-MANIFEST.json", "SHA256SUMS", bundle_name)]
        if self.manifest.get("artifacts") != artifacts:
            problems.append("RC-MANIFEST.json artifacts differ from a recomputation")
        bundle = files.get(bundle_name)
        if bundle is None:
            problems.append(f"missing {bundle_name}")
        else:
            with zipfile.ZipFile(bundle) as archive:
                infos = archive.infolist()
                members = {info.filename: info for info in infos}
                if set(members) != set(files) - {bundle_name} or len(infos) != len(members):
                    problems.append("the bundle's members differ from the set")
                for name, info in members.items():
                    if name in files and (archive.read(info) != files[name].read_bytes() or info.date_time != FIXED_ZIP_TIME
                                          or info.external_attr >> 16 != 0o100644 or info.compress_type != zipfile.ZIP_STORED):
                        problems.append(f"bundle member {name} differs")
        provenance = load_json(self.release / "PROVENANCE-INPUTS.json")
        if set(provenance.get("host_targets", [])) != getattr(self, "archive_hosts", set()):
            problems.append("PROVENANCE-INPUTS.json host_targets differ from the archive hosts")
        for key, path in (("source_manifest_sha256", "SOURCE-MANIFEST.json"), ("sbom_sha256", "SBOM.cdx.json")):
            if provenance.get(key) != sha256_file(self.release / path):
                problems.append(f"PROVENANCE-INPUTS.json {key} differs")
        if provenance.get("packaged_test_inputs_sha256") != sha256_bytes(self.tree.blob("release/packaged-test-inputs.json")):
            problems.append("PROVENANCE-INPUTS.json packaged_test_inputs_sha256 differs")
        sbom = load_json(self.release / "SBOM.cdx.json")
        properties = {item.get("name"): item.get("value") for item in sbom.get("metadata", {}).get("properties", [])}
        if sbom.get("metadata", {}).get("component", {}).get("version") != self.version \
                or properties.get("zeno-fcis:source-commit") != self.commit:
            problems.append("the SBOM does not name the version and commit")
        self.record("C9-release-index", problems, files=len(files), listed=len(listed))

    def c10(self, bindings: dict | None) -> None:
        problems = []
        if bindings is None:
            self.record("C10-cross-job", ["no job-output bindings were given"], hosts=0)
            return
        if set(bindings) != set(self.hosts):
            problems.append(f"the bindings name hosts {sorted(bindings)}, the set holds {sorted(self.hosts)}")
        for host, expected in sorted(bindings.items()):
            missing = [field for field in BINDING_FIELDS if not isinstance(expected.get(field), str)
                       or not expected[field]]
            if missing:
                problems.append(f"{host}: the bindings lack {', '.join(missing)}")
                continue
            recorded = self.hosts.get(host, {}).get("archive", {}).get("name")
            if expected["archive"] != recorded:
                problems.append(f"{host}: the bound archive {expected['archive']} is not the host record's {recorded}")
                continue
            folder = self.release / "hosts" / host
            pairs = {"archive_sha256": self.release / "binaries" / recorded,
                     "record_sha256": folder / "HOST-BINARY.json",
                     "inputs_sha256": folder / f"COMPILED-INPUTS-{host}.json",
                     "journey_sha256": folder / "INSTALLED-JOURNEY.json"}
            for key, path in pairs.items():
                if not path.is_file() or sha256_file(path) != expected[key]:
                    problems.append(f"{host}: {path.name} differs from the producing job's {key}")
        commit_hashes = {host: re.findall(r"^commit-hash: (\S+)$", record.get("rustc", ""), re.M)
                         for host, record in self.hosts.items()}
        if len({tuple(found) for found in commit_hashes.values()}) != 1 or not all(commit_hashes.values()):
            problems.append(f"rustc commit-hash differs across hosts: {commit_hashes}")
        self.record("C10-cross-job", problems, hosts=len(bindings))

    def c11(self, downloads: Path | None, signing_key: Path | None) -> None:
        if downloads is None:
            self.record("C11-download-list", ["no download list was given"], assets=0)
            return
        sums = {}
        for line in (self.release / "SHA256SUMS").read_text().splitlines():
            digest, _, name = line.partition("  ")
            sums[name] = digest
        published = {PurePosixPath(name).name: digest for name, digest in sums.items() if published_asset(name)}
        problems, seen = [], {}
        if signing_key is not None:
            if signing_key.name != SIGNING_KEY_NAME:
                problems.append(f"the signing key file is not named {SIGNING_KEY_NAME}")
            published[SIGNING_KEY_NAME] = sha256_file(signing_key)
        for line in downloads.read_text().splitlines():
            digest, separator, name = line.partition("  ")
            if separator != "  " or "/" in name or name in seen:
                problems.append(f"download list: malformed or repeated line for {name}")
            seen[name] = digest
        for name, digest in seen.items():
            if name not in published:
                unexplained = " without --signing-key" if name == SIGNING_KEY_NAME else ""
                problems.append(f"download list: {name} is not a published asset{unexplained}")
            elif published[name] != digest:
                source = "the signing key file" if name == SIGNING_KEY_NAME else "SHA256SUMS"
                problems.append(f"download list: {name} hash differs from {source}")
        problems.extend(f"download list omits {name}" for name in sorted(set(published) - set(seen)))
        self.record("C11-download-list", problems, assets=len(seen))


def check(release: Path, repo: Path, *, bindings: dict | None = None, downloads: Path | None = None,
          signing_key: Path | None = None, tag: str | None = None, remote: str | None = None,
          cargo_home: Path | None = None) -> dict:
    custody = Custody(release, repo, cargo_home or Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo")))
    custody.c1()
    steps = [("C2-tag-and-version", lambda: custody.c2(tag, remote)), ("C3-source-manifest", custody.c3),
             ("C4-source-archive", custody.c4), ("C5-crate-archives", custody.c5),
             ("C6-binary-archives", custody.c6), ("C7-compiled-inputs", custody.c7),
             ("C8-packaged-evidence", custody.c8), ("C9-release-index", custody.c9)]
    not_run = []
    if bindings is not None:
        steps.append(("C10-cross-job", lambda: custody.c10(bindings)))
    else:
        not_run.append("C10-cross-job")
    if downloads is not None or signing_key is not None:
        steps.append(("C11-download-list", lambda: custody.c11(downloads, signing_key)))
    else:
        not_run.append("C11-download-list")
    for name, step in steps:
        try:
            step()
        except (CustodyError, OSError, KeyError, ValueError, AttributeError, tarfile.TarError,
                zipfile.BadZipFile, struct.error) as error:
            custody.record(name, [f"unreadable input: {error}"])
    ran = all(item["passed"] for item in custody.checks.values()) and len(custody.checks) == len(steps) + 1
    status = "failed" if not ran else "incomplete" if not_run else "passed"
    return {"format": FORMAT, "status": status, "version": custody.version,
            "source_commit": custody.commit, "tree": custody.tree.tree, "checks": custody.checks,
            "bindings": bindings, "not_run": not_run, "nonclaims": [
                "nothing is rebuilt: not a reproducible-build proof",
                "does not authenticate the runner, compiler or linker",
                "signature trust remains with the signing step"]}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--release", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--repo", type=Path, default=ROOT)
    parser.add_argument("--bindings", type=Path)
    parser.add_argument("--downloads", type=Path)
    parser.add_argument("--signing-key", type=Path, help=f"the {SIGNING_KEY_NAME} the download list names")
    parser.add_argument("--tag")
    parser.add_argument("--remote-tag-commit")
    parser.add_argument("--cargo-home", type=Path)
    options = parser.parse_args(argv)
    try:
        bindings = load_json(options.bindings) if options.bindings else None
        report = check(options.release.resolve(), options.repo.resolve(), bindings=bindings,
                       downloads=options.downloads, signing_key=options.signing_key, tag=options.tag,
                       remote=options.remote_tag_commit, cargo_home=options.cargo_home)
    except (CustodyError, OSError, ValueError, KeyError) as error:
        print(f"release-custody: FAIL: {error}", file=sys.stderr)
        return 1
    options.report.parent.mkdir(parents=True, exist_ok=True)
    options.report.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    for name, item in sorted(report["checks"].items()):
        print(f"release-custody: {'PASS' if item['passed'] else 'FAIL'} {name} {item['counts']}")
        for problem in item["problems"][:20]:
            print(f"  - {problem}")
    for name in report["not_run"]:
        print(f"release-custody: NOT RUN {name} (its signing-stage input was not supplied)")
    print(f"release-custody: {report['status'].upper()}")
    return {"passed": 0, "incomplete": 2}.get(report["status"], 1)


if __name__ == "__main__":
    try:
        from telemetry import run_cli
    except ImportError:
        # Reporting is optional, including for a standalone copy of this tool.
        run_cli = lambda function: function()
    def _entrypoint():
        raise SystemExit(main())
    run_cli(_entrypoint)
