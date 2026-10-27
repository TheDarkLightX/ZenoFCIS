"""Optional factory-event hook for ZenoFCIS tools.

Tools call `emit(part, event, subject, body)` at meaningful moments, for example a refused UI law or a failed
acceptance scenario. When ZENO_FCIS_COMMONS is unset this does nothing. When it is set but the store cannot be
written, the tool continues and a one-line warning goes to stderr: reporting must never change a tool's result.
"""

from __future__ import annotations

import os
import sys
from pathlib import Path


def emit(part: str, event: str, subject: str, body: dict | None = None) -> int | None:
    directory = os.environ.get("ZENO_FCIS_COMMONS")
    if not directory:
        return None
    try:
        sys.path.insert(0, str(Path(__file__).resolve().parent))
        from commons import Commons

        store = Commons(directory, os.environ.get("ZENO_FCIS_COMMONS_ACTOR", f"tool:{part}"))
        try:
            return store.emit(part, event, subject, body or {})
        finally:
            store.close()
    except Exception as error:  # noqa: BLE001 - reporting is advisory by design
        print(f"commons: event not recorded ({type(error).__name__}: {str(error)[:200]})", file=sys.stderr)
        return None
