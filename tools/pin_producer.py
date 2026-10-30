"""Shared rules for the reviewed source-pin producers.

A producer recomputes one binding's pins from the current bytes with the
consuming gate's digest. It never runs that gate. Its closure is declared by
the base revision (default HEAD): the bound path set, every non-pin field and
every coupled artifact must equal the base. A difference there, an
inadmissible or missing bound path, or a binding outside the producer's JSON
layout is refused, and nothing is written.

Exit codes: 0 current (--check) or written (--write); 1 stale pins (--check);
2 refused.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
from typing import NamedTuple

ROOT = Path(__file__).resolve().parents[1]


class Refusal(ValueError):
    """A change outside the declared closure or an inadmissible binding."""


class Change(NamedTuple):
    binding: str
    bound: str
    old: str
    new: str


def bound_path(root: Path, name: str) -> Path:
    """An ordinary file under root, named relatively, with no symlink on the way."""
    path = Path(name)
    if not name or path.is_absolute() or ".." in path.parts or str(path) != name:
        raise Refusal("inadmissible bound path: " + name)
    current = root / path
    while current != root:
        if current.is_symlink():
            raise Refusal("symlinked bound path: " + name)
        current = current.parent
    if not (root / path).is_file():
        raise Refusal("missing bound file: " + name)
    return root / path


def read(root: Path, name: str) -> bytes:
    return bound_path(root, name).read_bytes()


def render(value) -> bytes:
    """The layout every binding file already has: two-space JSON in its key order."""
    return (json.dumps(value, indent=2) + "\n").encode()


def load(root: Path, name: str):
    data = read(root, name)
    value = json.loads(data)
    if render(value) != data:
        raise Refusal(name + ": not in the producer layout; a rewrite would move unchanged pins")
    return value


def _git(root: Path, *argv: str) -> subprocess.CompletedProcess:
    return subprocess.run(["git", *argv], cwd=root, capture_output=True)


def at_base(root: Path, base: str, name: str) -> bytes | None:
    """The bytes of name at base, or None when base does not have it."""
    if _git(root, "rev-parse", "--verify", "--quiet", base + "^{commit}").returncode:
        raise Refusal("unknown base revision: " + base)
    shown = _git(root, "show", f"{base}:{name}")
    return shown.stdout if shown.returncode == 0 else None


def load_at_base(root: Path, base: str, name: str):
    data = at_base(root, base, name)
    if data is None:
        raise Refusal(f"{name}: not declared at {base}; commit a reviewed binding first")
    return json.loads(data)


def changed_since(root: Path, base: str, paths) -> list[str]:
    """Tracked differences from base and untracked files under paths."""
    paths = list(paths)
    diff = _git(root, "diff", "--name-only", "--no-renames", base, "--", *paths)
    others = _git(root, "ls-files", "--others", "--exclude-standard", "--", *paths)
    if diff.returncode or others.returncode:
        raise Refusal("cannot compare with base " + base)
    return sorted(set(diff.stdout.decode().splitlines()) | set(others.stdout.decode().splitlines()))


def require_same(name: str, current, declared, what: str) -> None:
    if current != declared:
        raise Refusal(f"{name}: change outside the declared closure ({what})")


def write(root: Path, files: dict[str, bytes]) -> None:
    for name, data in files.items():
        path = bound_path(root, name)
        with tempfile.NamedTemporaryFile(dir=path.parent, prefix=path.name, delete=False) as handle:
            handle.write(data)
        os.chmod(handle.name, path.stat().st_mode & 0o777)
        os.replace(handle.name, path)


def main(plan, description: str, argv=None, root: Path = ROOT) -> int:
    parser = argparse.ArgumentParser(description=description)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true", help="list stale pins; exit 1 when any")
    mode.add_argument("--write", action="store_true", help="rewrite only the stale pins")
    parser.add_argument("--base", default="HEAD", help="the revision that declares the closure")
    args = parser.parse_args(argv)
    try:
        changes, files = plan(root, args.base)
    except (Refusal, OSError, ValueError) as error:
        print("refused:", error)
        return 2
    for change in changes:
        print(f"stale: {change.binding}: {change.bound} {change.old} -> {change.new}")
    if args.write:
        write(root, files)
        print(f"wrote {len(changes)} pins in {len(files)} files")
        return 0
    print(f"{len(changes)} stale pins")
    return 1 if changes else 0
