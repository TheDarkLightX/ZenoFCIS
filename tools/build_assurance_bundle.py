#!/usr/bin/env python3
"""Build a deterministic ZIP containing a clean source manifest and checked evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
import tempfile
import zipfile
from dataclasses import dataclass
from pathlib import Path


LABEL = re.compile(r"[a-z0-9][a-z0-9._-]*")
ZIP_TIME = (1980, 1, 1, 0, 0, 0)


@dataclass(frozen=True)
class Artifact:
    label: str
    source: Path
    archive_path: str
    content: bytes


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-manifest", type=Path, required=True)
    parser.add_argument(
        "--artifact",
        action="append",
        default=[],
        metavar="LABEL=PATH",
        help="add one checked evidence artifact under a stable label",
    )
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def parse_artifact(value: str) -> tuple[str, Path]:
    label, separator, raw_path = value.partition("=")
    if not separator or LABEL.fullmatch(label) is None or not raw_path:
        raise ValueError(f"invalid artifact specification: {value!r}")
    return label, Path(raw_path)


def read_source_manifest(path: Path) -> tuple[dict[str, object], bytes]:
    content = path.read_bytes()
    try:
        document = json.loads(content)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValueError(f"source manifest is not valid JSON: {error}") from error
    if not isinstance(document, dict):
        raise ValueError("source manifest must be a JSON object")
    if document.get("format") != "zeno-fcis/source-manifest/1":
        raise ValueError("source manifest has an unsupported format")
    if document.get("clean") is not True:
        raise ValueError("assurance bundles require a clean source manifest")
    commit = document.get("commit")
    if not isinstance(commit, str) or re.fullmatch(r"[0-9a-f]{40}", commit) is None:
        raise ValueError("source manifest commit is invalid")
    return document, content


def zip_info(path: str) -> zipfile.ZipInfo:
    info = zipfile.ZipInfo(path, date_time=ZIP_TIME)
    info.create_system = 3
    info.external_attr = 0o100644 << 16
    info.compress_type = zipfile.ZIP_STORED
    return info


def manifest_entry(artifact: Artifact) -> dict[str, object]:
    return {
        "label": artifact.label,
        "path": artifact.archive_path,
        "sha256": hashlib.sha256(artifact.content).hexdigest(),
        "size": len(artifact.content),
    }


def main() -> int:
    args = parse_args()
    try:
        source_document, source_content = read_source_manifest(args.source_manifest)
        requested = [parse_artifact(item) for item in args.artifact]
        labels = [label for label, _ in requested]
        if len(labels) != len(set(labels)):
            raise ValueError("artifact labels must be unique")

        artifacts = [
            Artifact(
                label="source-manifest",
                source=args.source_manifest,
                archive_path="evidence/source-manifest.json",
                content=source_content,
            )
        ]
        for label, path in sorted(requested):
            if not path.is_file():
                raise ValueError(f"artifact is not a regular file: {path}")
            artifacts.append(
                Artifact(
                    label=label,
                    source=path,
                    archive_path=f"evidence/{label}",
                    content=path.read_bytes(),
                )
            )

        bundle_manifest = {
            "artifacts": [manifest_entry(artifact) for artifact in artifacts],
            "format": "zeno-fcis/assurance-bundle/1",
            "source_commit": source_document["commit"],
        }
        manifest_bytes = (
            json.dumps(bundle_manifest, ensure_ascii=True, separators=(",", ":"), sort_keys=True)
            + "\n"
        ).encode("utf-8")

        args.output.parent.mkdir(parents=True, exist_ok=True)
        descriptor, temporary_name = tempfile.mkstemp(
            prefix=f".{args.output.name}.", dir=args.output.parent
        )
        os.close(descriptor)
        temporary = Path(temporary_name)
        try:
            with zipfile.ZipFile(temporary, mode="w") as archive:
                archive.writestr(zip_info("BUNDLE-MANIFEST.json"), manifest_bytes)
                for artifact in artifacts:
                    archive.writestr(zip_info(artifact.archive_path), artifact.content)
            os.replace(temporary, args.output)
        finally:
            temporary.unlink(missing_ok=True)
        print(f"assurance-bundle: wrote {args.output} ({len(artifacts)} evidence artifact(s))")
        return 0
    except (OSError, ValueError, zipfile.BadZipFile) as error:
        print(f"assurance-bundle: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
