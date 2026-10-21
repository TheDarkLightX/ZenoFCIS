"""Keep workspace artifacts from separate source copies in separate Cargo caches.

Cargo can otherwise reuse a backdated source copy's test executable from a
different checkout. The workspace wrapper's path affects artifact filenames;
this pass-through wrapper changes no compiler arguments or external dependency.
"""
from __future__ import annotations

import hashlib
import os
from pathlib import Path


def source_environment(root: Path, environment: dict[str, str]) -> dict[str, str]:
    if environment.get("RUSTC_WORKSPACE_WRAPPER"):
        raise RuntimeError("source controls require an unwrapped workspace compiler")
    target = Path(environment["CARGO_TARGET_DIR"]).resolve()
    identity = hashlib.sha256(os.fsencode(root.resolve())).hexdigest()
    wrapper = target / "source-scopes" / identity / "rustc"
    wrapper.parent.mkdir(parents=True, exist_ok=True)
    body = '#!/bin/sh\nexec "$@"\n'
    if wrapper.exists():
        if wrapper.read_text() != body:
            raise RuntimeError("unexpected source-scope compiler wrapper")
    else:
        wrapper.write_text(body)
        wrapper.chmod(0o755)
    result = dict(environment)
    result["RUSTC_WORKSPACE_WRAPPER"] = str(wrapper)
    return result
