#!/usr/bin/env python3
"""Read-only, bounded privacy checks for source and release archives."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import tarfile
from typing import BinaryIO
import zipfile
# BEGIN public knowledge example release guard.
import check_knowledge_files as knowledge_files
# END public knowledge example release guard.

ROOT = Path(__file__).resolve().parents[1]
MAX_FILE_BYTES = 128 * 1024 * 1024
MAX_TOTAL_BYTES = 256 * 1024 * 1024
MAX_FILES = 20_000
MAX_DEPTH = 4
MATCH_CONTEXT_BYTES = 32
ALLOWLIST = ROOT / "release/privacy-allowlist.json"
PRIVATE_FILENAMES = {
    ".env", ".netrc", ".git-credentials", "credentials.toml", "credentials.json",
    "id_rsa", "id_ed25519", "id_ecdsa", "id_dsa",
}
PATTERNS = {
    "personal-home-path": re.compile(
        rb"/(?:home|Users)/(?!(?:runner|build|builder|user|example|USERNAME|YOUR_USER)(?:[/\s\"'<>]|$))[^/\s\"'<>]+(?:/|(?=[\s\"'<>]|$))"
    ),
    "windows-home-path": re.compile(rb"(?<![A-Za-z0-9])[A-Za-z]:[\\/]+Users[\\/]+[^\\/\s\"'<>]+(?:[\\/]|(?=[\s\"'<>]|$))"),
    "private-key": re.compile(rb"-----BEGIN (?:OPENSSH |RSA |EC |DSA |PGP |ENCRYPTED )?PRIVATE KEY(?: BLOCK)?-----"),
    "github-token": re.compile(rb"(?:gh[pousr]_[A-Za-z0-9]{20,255}|github_pat_[A-Za-z0-9_]{30,255})"),
    "model-api-token": re.compile(rb"sk-(?:ant-|proj-|svcacct-)?[A-Za-z0-9_-]{35,255}"),
    "aws-access-key": re.compile(rb"(?:AKIA|ASIA)[0-9A-Z]{16}"),
    "bearer-credential": re.compile(rb"(?i:authorization)[\"\s]*[:=][\"\s]*(?i:bearer) [A-Za-z0-9_.-]{20,255}"),
}


def safe_location(location: str, patterns: dict[str, re.Pattern[bytes]]) -> str:
    encoded = location.encode("utf-8", errors="replace")
    for pattern in patterns.values():
        encoded = pattern.sub(b"<private-data>/", encoded)
    return encoded.decode("utf-8", errors="replace")[:512]


def match_fingerprint(data: bytes, match: re.Match[bytes]) -> str:
    """Bind the match's position, length and up to 32 bytes on each side."""
    context = data[max(0, match.start() - MATCH_CONTEXT_BYTES):match.end() + MATCH_CONTEXT_BYTES]
    framed = (b"zeno-fcis/release-privacy-match/1\0" + match.start().to_bytes(8, "big")
              + (match.end() - match.start()).to_bytes(8, "big") + context)
    return hashlib.sha256(framed).hexdigest()


def load_allowlist(path: Path = ALLOWLIST) -> frozenset[tuple[str, str, str]]:
    """Exact reviewed model-token exceptions; never patterns or directory prefixes."""
    if path.is_symlink() or not path.is_file():
        raise ValueError("invalid-allowlist")
    with path.open("rb") as stream:
        data = stream.read(65537)
    document = json.loads(data)
    if (len(data) > 65536 or not isinstance(document, dict)
            or set(document) != {"schema", "context_bytes", "entries"}
            or document["schema"] != "zeno-fcis/release-privacy-allowlist/1"
            or document["context_bytes"] != MATCH_CONTEXT_BYTES
            or not isinstance(document["entries"], list)):
        raise ValueError("invalid-allowlist")
    entries = set()
    for entry in document["entries"]:
        if not isinstance(entry, dict) or set(entry) != {"path", "file_sha256", "fingerprint_sha256", "reason"}:
            raise ValueError("invalid-allowlist")
        location, file_hash, fingerprint, reason = (entry[key] for key in
                                                  ("path", "file_sha256", "fingerprint_sha256", "reason"))
        if (not all(isinstance(value, str) for value in (location, file_hash, fingerprint, reason))
                or not location or "\\" in location or any(c in location for c in "*?[]")
                or any(not part or PurePosixPath(part).is_absolute()
                       or any(piece in {"", ".", ".."} for piece in part.split("/"))
                       for part in location.split("!"))
                or not re.fullmatch(r"[0-9a-f]{64}", file_hash)
                or not re.fullmatch(r"[0-9a-f]{64}", fingerprint)
                or not reason.strip() or len(reason) > 256 or any(c in reason for c in "\r\n")):
            raise ValueError("invalid-allowlist")
        key = (location, file_hash, fingerprint)
        if key in entries:
            raise ValueError("invalid-allowlist")
        entries.add(key)
    return frozenset(entries)


class Scan:
    def __init__(self, private_markers: tuple[bytes, ...] = (), *,
                 allowlist: frozenset[tuple[str, str, str]] = frozenset()) -> None:
        if len(private_markers) > 128 or sum(map(len, private_markers)) > 65536:
            raise ValueError("private-marker-limit")
        if any(len(marker) < 4 for marker in private_markers):
            raise ValueError("private-marker-too-short")
        self.patterns = dict(PATTERNS)
        if private_markers:
            longest_first = sorted(private_markers, key=len, reverse=True)
            self.patterns["private-marker"] = re.compile(b"|".join(map(re.escape, longest_first)))
        self.private_marker_count = len(private_markers)
        self.allowlist = frozenset(allowlist)
        self.allowlisted_matches = 0
        self.seen: set[bytes] = set()
        self.files = 0
        self.bytes = 0
        self.findings: list[dict[str, object]] = []
        self.errors: list[dict[str, str]] = []

    def error(self, location: str, code: str) -> None:
        self.errors.append({"location": safe_location(location, self.patterns), "code": code})

    def read(self, stream: BinaryIO) -> bytes:
        # Only inspect() knows whether these bytes have already been charged.
        limit = MAX_FILE_BYTES
        data = stream.read(limit + 1)
        if len(data) > limit:
            raise ValueError("byte-limit")
        return data

    def inspect(self, location: str, data: bytes, depth: int = 0) -> None:
        self.files += 1
        if self.files > MAX_FILES or depth > MAX_DEPTH:
            raise ValueError("file-or-depth-limit")
        # Names can contain private information even when file bytes repeat.
        self.match(location, location.encode("utf-8", errors="replace"), "member-name")
        basename = location.replace("\\", "/").rsplit("!", 1)[-1].rsplit("/", 1)[-1]
        if basename.lower() in PRIVATE_FILENAMES:
            self.findings.append({"location": safe_location(location, self.patterns), "code": "private-file-name"})
        # BEGIN public knowledge example member guard (before content deduplication).
        if knowledge_files.is_knowledge(location):
            if not knowledge_files.listed_location(location) or not knowledge_files.public_content(data):
                self.findings.append({"location": "knowledge-file", "code": "unlisted-or-changed-knowledge-file"})
        # END public knowledge example member guard.
        if len(data) > MAX_FILE_BYTES:
            raise ValueError("byte-limit")
        zipped = zipfile.is_zipfile(io.BytesIO(data))
        suffix = basename.lower()
        intended_zip = suffix.endswith(".zip") or data.startswith(
            (b"PK\x03\x04", b"PK\x05\x06", b"PK\x07\x08")
        )
        unsupported = (b"\xfd7zXZ\x00", b"BZh", b"\x28\xb5\x2f\xfd", b"7z\xbc\xaf\x27\x1c", b"Rar!\x1a\x07")
        if suffix.endswith((".xz", ".bz2", ".zst", ".7z", ".rar")) or data.startswith(unsupported):
            raise ValueError("unsupported-archive-format")
        if intended_zip and not zipped:
            raise ValueError("invalid-zip")
        if suffix.endswith((".gz", ".tgz", ".crate")) and not data.startswith(b"\x1f\x8b"):
            raise ValueError("invalid-gzip")
        if suffix.endswith(".tar"):
            with tarfile.open(fileobj=io.BytesIO(data), mode="r:"):
                pass
        digest = hashlib.sha256(data).digest()
        if digest not in self.seen:
            self.seen.add(digest)
            self.bytes += len(data)
            if self.bytes > MAX_TOTAL_BYTES:
                raise ValueError("byte-limit")
        # Check each location, including repeated archive members: an exception
        # for one exact release path must not authorize a copy at another path.
        self.match(location, data, "content")
        if zipped:
            with zipfile.ZipFile(io.BytesIO(data)) as archive:
                for member in archive.infolist():
                    if member.is_dir():
                        self.inspect(location + "!" + member.filename, b"", depth + 1)
                    else:
                        with archive.open(member) as stream:
                            self.inspect(location + "!" + member.filename, self.read(stream), depth + 1)
        elif data.startswith(b"\x1f\x8b"):
            with gzip.GzipFile(fileobj=io.BytesIO(data)) as stream:
                inflated = self.read(stream)
            try:
                archive = tarfile.open(fileobj=io.BytesIO(inflated), mode="r:")
            except tarfile.ReadError:
                self.inspect(location + "!gzip", inflated, depth + 1)
            else:
                with archive:
                    self.inspect_tar(location, archive, depth)
        else:
            try:
                archive = tarfile.open(fileobj=io.BytesIO(data), mode="r:")
            except tarfile.ReadError:
                return
            with archive:
                self.inspect_tar(location, archive, depth)

    def inspect_tar(self, location: str, archive: tarfile.TarFile, depth: int) -> None:
        self.match(location, json.dumps(archive.pax_headers, ensure_ascii=False).encode(), "archive-metadata")
        for member in archive:
            name = location + "!" + member.name
            metadata = [member.uname, member.gname, member.pax_headers]
            self.match(name, json.dumps(metadata, ensure_ascii=False).encode(), "archive-metadata")
            if member.isfile():
                with archive.extractfile(member) as stream:
                    self.inspect(name, self.read(stream), depth + 1)
            else:
                self.inspect(name, member.linkname.encode(), depth + 1)

    def match(self, location: str, data: bytes, source: str) -> None:
        file_hash = None
        for code, pattern in self.patterns.items():
            for match in pattern.finditer(data):
                if code == "model-api-token" and source == "content" and self.allowlist:
                    if file_hash is None:
                        file_hash = hashlib.sha256(data).hexdigest()
                    if (location, file_hash, match_fingerprint(data, match)) in self.allowlist:
                        self.allowlisted_matches += 1
                        continue
                self.findings.append({
                    "location": safe_location(location, self.patterns), "code": code,
                    "source": source, "line": data.count(b"\n", 0, match.start()) + 1,
                })

    def file(self, path: Path, location: str) -> None:
        if self.files > MAX_FILES:
            return
        try:
            # BEGIN refuse unlisted source KB names without reading their bytes.
            if knowledge_files.is_knowledge(location) and not knowledge_files.listed_location(location):
                self.findings.append({"location": "knowledge-file", "code": "unlisted-or-changed-knowledge-file"})
                return
            # END refuse unlisted source KB names.
            if path.is_symlink() or not path.is_file():
                self.error(location, "not-a-regular-file")
                return
            with path.open("rb") as stream:
                self.inspect(location, self.read(stream))
        except (OSError, ValueError, RuntimeError, EOFError, tarfile.TarError, zipfile.BadZipFile) as error:
            self.error(location, "inspection-failed-or-limit-exceeded")
            # Do not echo exception messages: archive names and payloads can
            # contain private data. A bounded category distinguishes parsing
            # failures from resource limits during release qualification.
            reasons = {"byte-limit", "file-or-depth-limit", "unsupported-archive-format",
                       "invalid-zip", "invalid-gzip"}
            reason = str(error) if isinstance(error, ValueError) and str(error) in reasons else type(error).__name__
            self.errors[-1]["reason"] = reason

    def report(self) -> dict[str, object]:
        return {
            "schema": "zeno-fcis/release-privacy-check/1",
            "status": "failed" if self.findings or self.errors else "passed",
            "files_checked": self.files, "unique_bytes_checked": self.bytes,
            "archive_formats": ["zip", "tar", "gzip"],
            "private_marker_count": self.private_marker_count,
            "allowlisted_matches": self.allowlisted_matches,
            "findings": self.findings, "errors": self.errors,
            "qualification": "Bounded pattern checks; matched data is never printed. This does not prove absence of every possible secret.",
        }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["check"])
    parser.add_argument("paths", nargs="*", type=Path)
    parser.add_argument(
        "--private-markers-file", type=Path,
        default=os.environ.get("ZENO_FCIS_PRIVATE_MARKERS_FILE"),
        help="Private file of literal identifiers, one per line; also accepts ZENO_FCIS_PRIVATE_MARKERS_FILE",
    )
    args = parser.parse_args()
    scan = Scan()
    try:
        allowlist = load_allowlist()
        scan = Scan(allowlist=allowlist)
    except (OSError, ValueError, TypeError):
        scan.error("allowlist", "invalid-allowlist")
        print(json.dumps(scan.report(), sort_keys=True))
        return 1
    if args.private_markers_file is not None:
        try:
            path = args.private_markers_file
            if path.is_symlink() or not path.is_file():
                raise ValueError("not-a-regular-file")
            with path.open("rb") as stream:
                data = stream.read(65537)
            markers = tuple(line for line in data.splitlines() if line)
            if len(data) > 65536 or not markers:
                raise ValueError("invalid-private-markers")
            scan = Scan(markers, allowlist=allowlist)
        except (OSError, ValueError):
            scan.error("private-markers", "invalid-private-markers")
            print(json.dumps(scan.report(), sort_keys=True))
            return 1
    try:
        if not args.paths:
            # BEGIN include ignored and untracked knowledge files in release refusal.
            for failure in knowledge_files.check(ROOT):
                scan.error("knowledge-files", failure)
            # END knowledge tree guard.
            tracked = subprocess.check_output(["git", "ls-files", "-z"], cwd=ROOT).decode().split("\0")
            for name in tracked:
                if scan.files > MAX_FILES:
                    break
                if name:
                    scan.file(ROOT / name, name)
        else:
            for path in args.paths:
                if path.is_dir() and not path.is_symlink():
                    for child in sorted(path.rglob("*")):
                        if scan.files > MAX_FILES:
                            break
                        if not child.is_dir() or child.is_symlink():
                            scan.file(child, child.relative_to(path).as_posix())
                else:
                    # BEGIN preserve the canonical explicit example location.
                    location = path.name
                    if knowledge_files.is_knowledge(str(path)):
                        resolved = path.absolute()
                        if resolved.is_relative_to(ROOT):
                            location = resolved.relative_to(ROOT).as_posix()
                    scan.file(path, location)
                    # END explicit knowledge location.
    except (OSError, ValueError, subprocess.CalledProcessError):
        scan.error("input", "inventory-failed")
    report = scan.report()
    print(json.dumps(report, sort_keys=True))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    from telemetry import run_cli
    def _entrypoint():
        raise SystemExit(main())
    run_cli(_entrypoint)
