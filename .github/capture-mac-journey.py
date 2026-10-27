#!/usr/bin/env python3
"""Capture only the failed synthesis result while preserving journey checks."""
import json
import os
from pathlib import Path
import subprocess
import sys

sys.path.insert(0, str(Path.cwd() / "tools"))
import check_installed_cli

original = subprocess.run


def capture(args, *positional, **keywords):
    completed = original(args, *positional, **keywords)
    if len(args) >= 3 and str(args[1]) == "synth" and str(args[2]) == "verify":
        folder = Path(os.environ["ZF26_DIAGNOSTICS"])
        folder.mkdir(mode=0o700, exist_ok=True)
        (folder / "synth-verify.stdout").write_bytes(completed.stdout or b"")
        (folder / "synth-verify.stderr").write_bytes(completed.stderr or b"")
        try:
            result = json.loads(completed.stdout)
            summary = {"exit": completed.returncode, "status": result.get("status"),
                       "detail_code": result.get("detail", {}).get("code")}
        except (TypeError, ValueError, AttributeError):
            summary = {"exit": completed.returncode, "status": "non-json"}
        print("synth-verify diagnostic:", json.dumps(summary), flush=True)
    return completed


subprocess.run = capture
raise SystemExit(check_installed_cli.main())
