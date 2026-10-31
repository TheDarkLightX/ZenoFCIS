"""Run one explicitly supplied local command with bounds and pidfd cleanup.

This helper records a command's exit status; it does not interpret its proof or
qualification result. An existing result directory is always refused.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import resource
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
from namespace_process import Owned, wait_owned


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--cwd", type=Path)
    parser.add_argument("--wall-seconds", type=int, default=120)
    parser.add_argument("--cpu-seconds", type=int, default=90)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command or args.cpu_seconds < 1 or args.wall_seconds < 1:
        parser.error("positive bounds and an explicit command are required")
    args.output.mkdir(parents=True, exist_ok=False)
    result = {"schema": "zal/independent-bounded-command/1", "pass": False, "command": command,
              "cpu_limit_seconds": args.cpu_seconds, "wall_limit_seconds": args.wall_seconds,
              "address_space_limit_bytes": 1024**3, "affinity": [min(os.sched_getaffinity(0))]}
    start = time.monotonic()
    owner = None

    def limits():
        resource.setrlimit(resource.RLIMIT_CPU, (args.cpu_seconds, args.cpu_seconds))
        resource.setrlimit(resource.RLIMIT_AS, (1024**3, 1024**3))
        os.sched_setaffinity(0, set(result["affinity"]))

    try:
        with (args.output / "output.log").open("xb") as log:
            process = subprocess.Popen(command, cwd=args.cwd, stdout=log, stderr=subprocess.STDOUT,
                                       start_new_session=True, preexec_fn=limits)
            owner = Owned(process)
            result["exit"] = wait_owned(process, owner, args.wall_seconds)
            result["pass"] = result["exit"] == 0
    except BaseException as error:
        result["failure"] = {"type": type(error).__name__, "message": str(error)}
    finally:
        if owner is not None:
            result["cleanup"] = owner.finish()
            result["pass"] = result["pass"] and not result["cleanup"]["survivors"]
        result["wall_seconds"] = time.monotonic() - start
        (args.output / "RESULT.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps(result))
    return 0 if result["pass"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
