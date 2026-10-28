"""CLI: python3 -B -m tools.machine_code.run --help (from the repository root)."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

from . import session


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    opened = commands.add_parser("open", help="create a source/solver-bound proposal session")
    opened.add_argument("--problem", required=True, type=Path)
    opened.add_argument("--session", required=True, type=Path)
    for command in ("packet", "candidate", "search", "resume", "emit"):
        sub = commands.add_parser(command)
        sub.add_argument("--session", required=True, type=Path)
        if command in ("candidate", "resume", "emit", "search"):
            sub.add_argument("--solver-ms", type=int, default=1000)
        if command == "candidate":
            sub.add_argument("--proposal", required=True, type=Path)
        if command == "emit":
            sub.add_argument("--out", required=True, type=Path)
        if command == "search":
            sub.add_argument("--max-candidates", type=int, default=10_000)
            sub.add_argument("--max-checks", type=int, default=64)
            sub.add_argument("--deadline-ms", type=int, default=5000)
    replay = commands.add_parser("replay", help="recheck exact emitted machine bytes and bindings")
    replay.add_argument("--artifact", required=True, type=Path)
    replay.add_argument("--solver-ms", type=int, default=1000)
    args = parser.parse_args(argv)
    try:
        if args.command == "open":
            result = session.open_session(args.problem, args.session)
        elif args.command == "packet":
            result = session.packet(args.session)
        elif args.command == "candidate":
            result = session.submit(args.session, args.proposal, args.solver_ms)
        elif args.command == "search":
            result = session.search(args.session, args.max_candidates, args.max_checks,
                                    args.deadline_ms, args.solver_ms)
        elif args.command == "resume":
            result = session.resume(args.session, args.solver_ms)
        elif args.command == "emit":
            result = session.emit(args.session, args.out, args.solver_ms)
        else:
            result = session.replay_artifact(args.artifact, args.solver_ms)
        result.setdefault("authority", "none")
        print(json.dumps(result, sort_keys=True, allow_nan=False))
        status = result["status"] if "status" in result else "packet"
        return 2 if status in ("inconclusive", "no-checked-candidate") else 1 if status in ("counterexample", "refused") else 0
    except ImportError as error:
        print(json.dumps({"status": "inconclusive", "reason": str(error), "authority": "none"}))
        return 2
    except (OSError, ValueError, KeyError, TypeError, RecursionError) as error:
        print(json.dumps({"status": "refused", "reason": str(error), "authority": "none"}))
        return 65


if __name__ == "__main__":
    sys.exit(main())
