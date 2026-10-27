"""Keep workspace artifacts from separate source copies in separate Cargo caches.

Cargo can otherwise reuse a backdated source copy's test executable from a
different checkout. The workspace wrapper's path affects artifact filenames;
this pass-through wrapper changes no compiler arguments or external dependency.
"""
from __future__ import annotations

import hashlib
import os
import sys
from pathlib import Path
import shutil


def require_build_space(work: Path, target: Path, minimum: int = 8 * 1024**3) -> None:
    """Check the actual work/target mounts, including not-yet-created paths."""
    for purpose, requested in (("work", work), ("target", target)):
        existing = requested.resolve()
        while not existing.exists():
            existing = existing.parent
        if shutil.disk_usage(existing).free < minimum:
            raise RuntimeError(f"{purpose} free space below {minimum} bytes at {existing}")


def source_environment(root: Path, environment: dict[str, str]) -> dict[str, str]:
    if environment.get("RUSTC_WORKSPACE_WRAPPER"):
        raise RuntimeError("source controls require an unwrapped workspace compiler")
    target = Path(environment["CARGO_TARGET_DIR"]).resolve()
    identity = hashlib.sha256(b"zeno-fcis/source-compiler/2\0" + os.fsencode(root.resolve())).hexdigest()
    wrapper = target / "source-scopes" / identity / "rustc"
    wrapper.parent.mkdir(parents=True, exist_ok=True)
    body = f'#!{sys.executable}\nimport os, sys\nos.execv(sys.argv[1], sys.argv[1:])\n'
    if wrapper.exists():
        if wrapper.read_text() != body:
            raise RuntimeError("unexpected source-scope compiler wrapper")
    else:
        wrapper.write_text(body)
        wrapper.chmod(0o755)
    result = dict(environment)
    result["RUSTC_WORKSPACE_WRAPPER"] = str(wrapper)
    return result
