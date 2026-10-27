#!/usr/bin/env python3
"""Mandatory real-tool qualification of checked scalar certificates.

Set ZENO_FCIS_CVC5 and ZENO_FCIS_CERTIFICATE_CHECKER (manifest path).
Missing tools, ignored/missing tests or incomplete results are failures.
"""
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def main():
    missing = [name for name in ("ZENO_FCIS_CVC5", "ZENO_FCIS_CERTIFICATE_CHECKER")
               if not os.environ.get(name)]
    if missing:
        print("certificate qualification requires " + ", ".join(missing), file=sys.stderr)
        return 1
    command = ["cargo", "+1.97.1", "test", "--locked", "--offline", "-j", "1",
               "-p", "zeno-fcis-cli", "--test", "symbolic_certificates",
               "pinned_certificate_", "--", "--ignored", "--test-threads=1", "--nocapture"]
    completed = subprocess.run(command, cwd=ROOT, stdout=subprocess.PIPE, text=True, check=False)
    sys.stdout.write(completed.stdout)
    if completed.returncode:
        return completed.returncode
    summary = re.findall(r"^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;", completed.stdout, re.MULTILINE)
    if summary != [("1", "0", "0")]:
        print("certificate qualification did not run its complete real-tool journey", file=sys.stderr)
        return 1
    print("certificate qualification PASS: full-i128 identity and non-identical check/replay, actual refusal controls")
    return 0


if __name__ == "__main__":
    try:
        from telemetry import run_cli
    except ImportError:
        # Reporting is optional, including for a standalone copy of this tool.
        run_cli = lambda function: function()
    def _entrypoint():
        raise SystemExit(main())
    run_cli(_entrypoint)
