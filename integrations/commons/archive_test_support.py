"""Shared exact-message detection for unavailable Rust archive I/O."""

from __future__ import annotations

import json

ARCHIVE_REFUSAL = (
    "archives need the reference tool: python3 -I integrations/commons/cli.py "
    "(this build has no xz decoder)"
)


def is_archive_refusal(result) -> bool:
    """Return true only for the precise archive capability refusal."""
    try:
        response = json.loads(result.stderr)
    except (json.JSONDecodeError, TypeError):
        return False
    return isinstance(response, dict) and response.get("refused") == ARCHIVE_REFUSAL
