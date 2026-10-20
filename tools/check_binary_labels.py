"""Narrow release-artifact adjudication of pooled public diagnostic strings.

The ordinary privacy scanner is unchanged. Only an exact recorded executable
can use this reviewed eight-label policy; every overlapping token start must
be explained. The raw scanner result remains visible in the returned receipt.
"""
from __future__ import annotations

from dataclasses import dataclass, asdict
import hashlib
import json
import os
from pathlib import Path
import re
import tarfile

import check_release_privacy as privacy


# These complete public literals, source files and contexts were independently
# reviewed. Changing a source hash requires another review, not auto-discovery.
POLICY = (
    ("crates/zeno-fcis-cli/src/loop_doors_command.rs",
     "e35dcd53b8365657ccf24583d6fabe975b44685717faa0799d5bc7cfe5f671fc",
     ("ask-when-it-matters",)),
    ("crates/zeno-fcis-cli/src/effects.rs",
     "28e18bef084c4574df7197ec4f938463894d083fad0bc32366fa0e02b1a89d3f",
     ("strategy-task-manifest", "task-original-programs")),
    ("crates/zeno-fcis-loop-doors/src/strategy_knowledge/reasoning.rs",
     "cb924b3c9d3124e8a7d7f5920be752f1762bf4603602e399b5a35efba213b729",
     ("kb-task-subject-required", "kb-task-key", "kb-task-value", "kb-task-facts-only",
      "kb-task-conflict")),
)
MAX_STARTS = 1024


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def require(condition: bool, code: str) -> None:
    if not condition:
        raise ValueError(code)


def read(path: Path) -> bytes:
    require(not path.is_symlink() and path.is_file(), "binding-not-regular")
    with path.open("rb") as stream:
        data = stream.read(privacy.MAX_FILE_BYTES + 1)
    require(len(data) <= privacy.MAX_FILE_BYTES, "binding-byte-limit")
    return data


def anchors(root: Path) -> list[dict]:
    result = []
    pattern = privacy.PATTERNS["model-api-token"]
    for name, expected, labels in POLICY:
        path = root / name
        require(not path.is_symlink(), "source-anchor-symlink")
        source = read(path)
        require(sha(source) == expected, "source-anchor-changed")
        for label in labels:
            literal = label.encode("ascii")
            require(pattern.search(literal) is None, "label-is-token")
            occurrences = list(re.finditer(re.escape(b'"' + literal + b'"'), source))
            require(bool(occurrences), "source-literal-missing")
            for occurrence in occurrences:
                result.append({"source_path": name, "source_sha256": expected,
                               "source_offset": occurrence.start() + 1,
                               "literal": literal})
    return result


def classify(data: bytes, source_anchors: list[dict]) -> list[dict]:
    pattern = privacy.PATTERNS["model-api-token"]
    # A lookahead sees starts hidden inside another greedy match.
    starts = re.finditer(b"(?=" + pattern.pattern + b")", data, pattern.flags)
    matches = []
    for start in starts:
        require(len(matches) < MAX_STARTS, "token-start-limit")
        match = pattern.match(data, start.start())
        require(match is not None, "token-enumeration-inconsistent")
        found = []
        for anchor in source_anchors:
            literal = anchor["literal"]
            for prefix in re.finditer(b"sk-", literal):
                offset = match.start() - prefix.start()
                if (0 < prefix.start() and offset >= 0
                        and data[offset:offset + len(literal)] == literal
                        and match.end() > offset + len(literal)):
                    found.append({k: v for k, v in anchor.items() if k != "literal"} | {
                        "literal_sha256": sha(literal), "literal_length": len(literal),
                        "literal_binary_start": offset, "prefix_offset": prefix.start()})
        require(bool(found), "unclassified-token-start")
        matches.append({"offset": match.start(), "length": len(match[0]),
                        "sha256": sha(match[0]), "source_anchors": found})
    require(bool(matches), "missing-token-start")
    return matches


@dataclass(frozen=True)
class ExecutableBinding:
    source_commit: str
    host: str
    member: str
    executable_sha256: str
    archive_sha256: str
    host_record_sha256: str
    compiled_inputs_sha256: str


def binding(record_path: Path, archive_path: Path, commit: str) -> ExecutableBinding:
    record_bytes = read(record_path)
    record = json.loads(record_bytes)
    require(record.get("format") == "zeno-fcis/host-binary/1"
            and record.get("source_commit") == commit, "host-record-identity")
    host = record["host"]
    require(host in ("x86_64-unknown-linux-gnu", "aarch64-apple-darwin"), "host-profile")
    require(archive_path.name == record["archive"]["name"]
            and sha(read(archive_path)) == record["archive"]["sha256"], "archive-identity")
    member_name = record["archive"]["prefix"] + "/zeno-fcis"
    with tarfile.open(archive_path, "r:gz") as archive:
        members = [m for m in archive.getmembers() if m.name == member_name]
        require(len(members) == 1 and members[0].isfile(), "executable-member")
        require(members[0].size <= privacy.MAX_FILE_BYTES, "executable-size")
        stream = archive.extractfile(members[0])
        require(stream is not None, "executable-read")
        with stream:
            executable = stream.read(privacy.MAX_FILE_BYTES + 1)
    require(sha(executable) == record["executable"]["sha256"]
            and len(executable) == record["executable"]["size"], "executable-identity")
    magic = b"\x7fELF" if host == "x86_64-unknown-linux-gnu" else b"\xcf\xfa\xed\xfe"
    require(executable.startswith(magic), "executable-format")
    inputs_bytes = read(record_path.parent / f"COMPILED-INPUTS-{host}.json")
    inputs = json.loads(inputs_bytes)
    require(inputs.get("format") == "zeno-fcis/compiled-inputs/1"
            and inputs.get("source_commit") == commit and inputs.get("host") == host
            and inputs.get("executable", {}).get("sha256") == sha(executable)
            and inputs.get("unclassified") == [], "compiled-inputs-identity")
    return ExecutableBinding(commit, host, member_name, sha(executable),
                             sha(read(archive_path)), sha(record_bytes), sha(inputs_bytes))


class ArtifactScan(privacy.Scan):
    def __init__(self, root: Path, bindings: list[ExecutableBinding], private_markers=()):
        super().__init__(private_markers)
        self.root = root
        self.bindings = bindings
        self.adjudicated: dict[int, dict] = {}

    def match(self, location: str, data: bytes, source: str) -> None:
        first = len(self.findings)
        super().match(location, data, source)
        if source != "content":
            return
        for index in range(first, len(self.findings)):
            if self.findings[index]["code"] != "model-api-token":
                continue
            candidates = [b for b in self.bindings if location.rsplit("!", 1)[-1] == b.member
                          and b.executable_sha256 == sha(data)]
            if len(candidates) != 1:
                continue
            try:
                matches = classify(data, anchors(self.root))
            except (OSError, ValueError):
                continue
            self.adjudicated[index] = {"finding_index": index,
                                      "binding": asdict(candidates[0]), "matches": matches}

    def disposition(self) -> dict:
        raw = super().report()
        remaining = [f for index, f in enumerate(self.findings) if index not in self.adjudicated]
        accepted = not remaining and not self.errors
        return {"schema": "zeno-fcis/binary-label-disposition/1", "raw_report": raw,
                "effective_status": ("accepted-public-labels" if self.adjudicated else "passed")
                if accepted else "failed", "remaining_findings": remaining,
                "adjudications": list(self.adjudicated.values()),
                "scanner_sha256": sha(Path(privacy.__file__).read_bytes()),
                "policy_and_verifier_sha256": sha(Path(__file__).read_bytes()),
                "qualification": "Reviewed public-label policy for exact artifacts; not proof of secret absence."}


def check(paths: list[Path], root: Path, commit: str,
          records: list[tuple[Path, Path]]) -> dict:
    """Scan all inputs with existing archive/error/marker checks and explicit bindings."""
    markers = ()
    if name := os.environ.get("ZENO_FCIS_PRIVATE_MARKERS_FILE"):
        path = Path(name)
        require(not path.is_symlink() and path.is_file(), "private-markers-file")
        with path.open("rb") as stream:
            data = stream.read(65537)
        markers = tuple(line for line in data.splitlines() if line)
        require(len(data) <= 65536 and bool(markers), "private-markers-file")
    scan = ArtifactScan(root, [binding(r, a, commit) for r, a in records], markers)
    for path in paths:
        if path.is_dir() and not path.is_symlink():
            for child in sorted(path.rglob("*")):
                if scan.files > privacy.MAX_FILES:
                    break
                if not child.is_dir() or child.is_symlink():
                    scan.file(child, path.name + "/" + child.relative_to(path).as_posix())
        else:
            scan.file(path, path.name)
    return scan.disposition()
