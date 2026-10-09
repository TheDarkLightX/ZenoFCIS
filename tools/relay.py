#!/usr/bin/env python3
"""Repository entrypoint for the relay shipped in contract applications.

Execute the one canonical module in this namespace so imports, patched globals,
``__file__`` and direct script invocations retain their repository behaviour.
"""
from pathlib import Path as _Path

_source = _Path(__file__).resolve().parents[1] / "crates/zeno-fcis-cli/contract-app/tools/relay.py"
exec(compile(_source.read_bytes(), str(_source), "exec"), globals())
