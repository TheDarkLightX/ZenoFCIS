"""Copy the actual registered execution dependencies into proof specimens.

The authority gate independently validates the approved manifest identity.
These helpers provide complete local files, including separately listed test
modules and the generated digest. They do not admit specifications or evidence.
"""
from __future__ import annotations

import json
from pathlib import Path
import re

SOURCE_MANIFEST = Path("verification/verus/authority_v2_sources.json")
GETTER = Path("crates/zeno-fcis-synthesis/src/finite/execution_v2/authority/evaluator.rs")


def execution_sources(root: Path) -> tuple[Path, ...]:
    """Return the complete approved local-source list needed by direct harnesses."""
    manifest = json.loads((root / SOURCE_MANIFEST).read_text())
    names = manifest["paths"]
    tests = json.loads((root / "verification/verus/authority_v2_test_sources.json").read_text())["paths"]
    if manifest.get("schema") != "zeno-fcis/authority-source-closure/1" or not names or names != sorted(set(names)):
        raise ValueError("invalid execution source manifest")
    names = sorted(set(names + tests + [str(GETTER)]))
    paths = []
    for name in names:
        path = Path(name)
        if path.is_absolute() or ".." in path.parts or not re.fullmatch(r"[A-Za-z0-9_./-]+", name):
            raise ValueError("unsafe execution source path")
        source = root / path
        if not source.is_file() or any(parent.is_symlink() for parent in (source, *source.parents)):
            raise ValueError(f"missing or symlink execution source: {name}")
        paths.append(path)
    return tuple(paths)


def adjust_specimen_source_lengths(specimen: Path) -> list[dict]:
    """The fixed evaluator constant has no embedded lengths to repair."""
    return []
