#!/usr/bin/env python3
"""Emit a deterministic SHA-256 manifest for the exact tracked source tree."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--require-clean",
        action="store_true",
        help="refuse to describe a source tree with tracked modifications",
    )
    return parser.parse_args()


def git(*arguments: str) -> bytes:
    process = subprocess.run(
        ["git", *arguments],
        cwd=ROOT,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if process.returncode != 0:
        detail = process.stderr.decode("utf-8", errors="replace").strip()
        raise RuntimeError(f"git {' '.join(arguments)} failed: {detail}")
    return process.stdout


def tracked_paths() -> tuple[str, ...]:
    raw = git("ls-files", "--cached", "-z")
    paths = tuple(
        item.decode("utf-8", errors="surrogateescape")
        for item in raw.split(b"\0")
        if item
    )
    if paths != tuple(sorted(paths, key=lambda item: item.encode("utf-8", errors="surrogateescape"))):
        raise RuntimeError("git returned non-canonical tracked-file order")
    return paths


def source_entry(relative: str) -> dict[str, object]:
    path = ROOT / relative
    if path.is_symlink():
        content = os.readlink(path).encode("utf-8", errors="surrogateescape")
        kind = "symlink"
    elif path.is_file():
        content = path.read_bytes()
        kind = "file"
    else:
        raise RuntimeError(f"tracked path is missing or unsupported: {relative}")
    return {
        "kind": kind,
        "path": relative,
        "sha256": hashlib.sha256(content).hexdigest(),
        "size": len(content),
    }


def main() -> int:
    args = parse_args()
    try:
        status = git("status", "--porcelain=v1", "--untracked-files=no")
        clean = not status
        if args.require_clean and not clean:
            print("release-manifest: tracked source tree is dirty", file=sys.stderr)
            return 1

        commit = git("rev-parse", "HEAD").decode("ascii").strip()
        if len(commit) != 40:
            raise RuntimeError("HEAD is not a full 40-character commit")
        toolchain_path = ROOT / "rust-toolchain.toml"
        toolchain = toolchain_path.read_text(encoding="utf-8")
        entries = tuple(source_entry(path) for path in tracked_paths())
        total_size = sum(int(entry["size"]) for entry in entries)
        manifest = {
            "clean": clean,
            "commit": commit,
            "file_count": len(entries),
            "files": entries,
            "format": "zeno-fcis/source-manifest/1",
            "rust_toolchain_sha256": hashlib.sha256(toolchain.encode("utf-8")).hexdigest(),
            "total_size": total_size,
        }
        encoded = json.dumps(
            manifest,
            ensure_ascii=True,
            separators=(",", ":"),
            sort_keys=True,
        )
        sys.stdout.write(encoded)
        sys.stdout.write("\n")
        return 0
    except (OSError, RuntimeError, UnicodeError) as error:
        print(f"release-manifest: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
