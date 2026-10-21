#!/usr/bin/env python3
"""Bounded source and package guard for the sole public knowledge example."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re

ROOT = Path(__file__).resolve().parents[1]
EXAMPLE = "crates/zeno-fcis-loop-doors/examples/strategy-example.kb"
INCLUDES = "release/knowledge-base-includes.json"
PUBLIC_SHA256 = "010ccbc712c1fcb08a9fb5b2b200c7ce263350173b4fa561afd494d0db5480e1"
MAX_ENTRIES = 100_000


def is_knowledge(location: str) -> bool:
    return location.replace("\\", "/").rstrip("/").rsplit("!", 1)[-1].lower().endswith(".kb")


def listed_location(location: str) -> bool:
    """Only a canonical source example or its crate/source-archive member."""
    member = location.rsplit("!", 1)[-1]
    if "\\" in member:
        return False
    path = PurePosixPath(member)
    if path.is_absolute() or ".." in path.parts or member != str(path):
        return False
    if member == EXAMPLE:
        return True
    parts = path.parts
    if len(parts) == len(PurePosixPath(EXAMPLE).parts) + 1:
        return "/".join(parts[1:]) == EXAMPLE
    return (len(parts) == 3
            and re.fullmatch(r"zeno-fcis-loop-doors-[0-9][A-Za-z0-9.+-]*", parts[0]) is not None
            and parts[1:] == ("examples", "strategy-example.kb"))


def public_content(data: bytes) -> bool:
    return len(data) <= 65536 and hashlib.sha256(data).hexdigest() == PUBLIC_SHA256


def check(root: Path) -> list[str]:
    """Inspect names including ignored/untracked files; unknown KB bytes are unread."""
    failures = []
    try:
        manifest = root / INCLUDES
        if manifest.is_symlink() or manifest.stat().st_size > 4096:
            return ["kb-includes-nonregular-or-bound"]
        policy = json.loads(manifest.read_text(encoding="utf-8"))
        if policy != {"schema": "zeno-fcis/knowledge-base-includes/1", "files": {EXAMPLE: PUBLIC_SHA256}}:
            failures.append("kb-includes-mismatch")
        entries = 0
        found_example = False

        def failed_walk(error):
            raise error

        for directory, dirs, files in os.walk(root, followlinks=False, onerror=failed_walk):
            parent = Path(directory)
            if parent == root and ".git" in dirs:
                dirs.remove(".git")  # Git internals are not the source tree.
            entries += len(dirs) + len(files)
            if entries > MAX_ENTRIES:
                return failures + ["kb-tree-inventory-budget"]
            for name in dirs:
                p = parent / name
                if p.is_symlink():
                    failures.append("kb-tree-symlink-directory")
                if is_knowledge(name):
                    failures.append("kb-unlisted-directory")
            for name in files:
                if not is_knowledge(name):
                    continue
                p = parent / name
                relative = p.relative_to(root).as_posix()
                if relative != EXAMPLE:
                    failures.append("kb-unlisted-file")
                    continue
                found_example = True
                if p.is_symlink() or not p.is_file():
                    failures.append("kb-example-nonregular")
                    continue
                with p.open("rb") as stream:
                    data = stream.read(65537)
                if not public_content(data):
                    failures.append("kb-example-content-mismatch")
        if not found_example:
            failures.append("kb-example-missing")
    except (OSError, ValueError, UnicodeError):
        failures.append("kb-inventory-or-includes-failed")
    return sorted(set(failures))


def main() -> int:
    failures = check(ROOT)
    print(json.dumps({"schema": "zeno-fcis/knowledge-file-guard/1",
                      "status": "failed" if failures else "passed", "failures": failures},
                     sort_keys=True))
    return int(bool(failures))


if __name__ == "__main__":
    try:
        from telemetry import run_cli
    except ImportError:
        # Reporting is optional, including for a standalone copy of this tool.
        run_cli = lambda function: function()
    def _entrypoint():
        raise SystemExit(main())
    run_cli(_entrypoint)
