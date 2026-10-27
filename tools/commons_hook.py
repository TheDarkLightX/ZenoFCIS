"""Report a factory event to Zeno Commons when ZENO_FCIS_COMMONS is set; otherwise do nothing.

Reporting never changes a tool's result: an unavailable store only prints a one-line warning.
"""

from __future__ import annotations

import os
import sys
from pathlib import Path


def report(part: str, event: str, subject: str, body: dict | None = None) -> int | None:
    if not os.environ.get("ZENO_FCIS_COMMONS"):
        return None
    hook = Path(__file__).resolve().parent.parent / "integrations" / "commons"
    if str(hook) not in sys.path:
        sys.path.insert(0, str(hook))
    try:
        from emit import emit
    except Exception as error:  # noqa: BLE001 - reporting is advisory
        print(f"commons: hook unavailable ({type(error).__name__})", file=sys.stderr)
        return None
    return emit(part, event, subject or "unnamed", body or {})


def failure(error: BaseException) -> dict:
    """A short, safe description of why a tool stopped."""
    return {"error": type(error).__name__, "detail": str(error)[:500]}
