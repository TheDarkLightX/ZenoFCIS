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
    for command in ("packet", "candidate", "search", "split-search", "resume", "emit"):
        sub = commands.add_parser(command)
        sub.add_argument("--session", required=True, type=Path)
        if command in ("candidate", "resume", "emit", "search", "split-search"):
            sub.add_argument("--solver-ms", type=int, default=1000)
        if command == "candidate":
            sub.add_argument("--proposal", required=True, type=Path)
        if command == "emit":
            sub.add_argument("--out", required=True, type=Path)
        if command in ("search", "split-search"):
            sub.add_argument("--max-candidates", type=int, default=10_000)
            sub.add_argument("--max-checks", type=int, default=64)
            sub.add_argument("--deadline-ms", type=int, default=5000)
        if command == "search":
            sub.add_argument("--no-prune", action="store_true",
                             help="enumerate every sequence (the exact footprint pruning is on by default)")
    bench = commands.add_parser("bench", help="paired timing of a replayed x86 artifact against gcc -O2 for the same expression")
    bench.add_argument("--artifact", required=True, type=Path)
    bench.add_argument("--out", required=True, type=Path)
    bench.add_argument("--iterations", type=int, default=1_000_000)
    bench.add_argument("--repeats", type=int, default=21)
    bench.add_argument("--solver-ms", type=int, default=1000)
    replay = commands.add_parser("replay", help="recheck exact emitted machine bytes and bindings")
    replay.add_argument("--artifact", required=True, type=Path)
    replay.add_argument("--solver-ms", type=int, default=1000)
    library = commands.add_parser("library", help="recheck, run, time or add to a checked kernel library")
    actions = library.add_subparsers(dest="action", required=True)
    checked = actions.add_parser("check", help="recheck every entry; exit 0 only if all are equivalent")
    checked.add_argument("--native", action="store_true",
                         help="also run every x86 entry on this processor against the integer reference")
    timed = actions.add_parser("bench", help="recheck, then time every x86 entry against gcc -O2 in one table")
    timed.add_argument("--out", required=True, type=Path)
    timed.add_argument("--iterations", type=int, default=1_000_000)
    timed.add_argument("--repeats", type=int, default=21)
    added = actions.add_parser("add", help="add an emitted artifact that replays here; never overwrites")
    for sub in (checked, timed, added):
        sub.add_argument("--dir", required=True, type=Path)
        sub.add_argument("--solver-ms", type=int, default=1000)
    added.add_argument("--artifact", required=True, type=Path)
    added.add_argument("--name", required=True)
    added.add_argument("--summary", required=True)
    added.add_argument("--route", required=True, choices=("proposal", "search"))
    realized = commands.add_parser("realize", help="ask whether any admitted program with cost below a bound exists")
    source = realized.add_mutually_exclusive_group(required=True)
    source.add_argument("--problem", type=Path)
    source.add_argument("--session", type=Path, help="ask below the session's rechecked best candidate")
    realized.add_argument("--below", type=int, help="cost bound on the problem's objective; omitted: no bound")
    realized.add_argument("--replay", type=Path, help="rebuild and re-solve a record's unrealizable queries")
    realized.add_argument("--out", type=Path, help="also write the record; never overwrites")
    realized.add_argument("--solver-ms", type=int, default=2000)
    realized.add_argument("--max-rounds", type=int, default=64)
    realized.add_argument("--deadline-ms", type=int, default=60_000)
    realized.add_argument("--groups", type=int, default=1,
                          help="split each slot's palette into this many groups and decide every subspace alone")
    realized.add_argument("--workers", type=int, default=1, help="processes deciding subspaces (at most 4)")
    realized.add_argument("--split-ms", type=int,
                          help="halve any subspace whose query does not answer within this many milliseconds")
    contract = commands.add_parser("contract", help="lower an arithmetic contract to profile 2, or check bytes against it")
    contract_actions = contract.add_subparsers(dest="action", required=True)
    lowered = contract_actions.add_parser("lower", help="print the lowering record; refuse with a named missing bridge")
    lowered.add_argument("--problem-out", type=Path, help="also write the lowered problem; never overwrites")
    against = contract_actions.add_parser("check", help="check exact bytes against the lowered problem")
    against.add_argument("--code", required=True, type=Path)
    against.add_argument("--solver-ms", type=int, default=1000)
    for sub in (lowered, against):
        sub.add_argument("--contract", required=True, type=Path)
    special = commands.add_parser("specialize", help="fix some inputs of a profile-2 problem with a Z3 proof; "
                                  "optionally rewrite a checked kernel and check it; writes nothing on failure")
    special.add_argument("--problem", type=Path, help="profile-2 problem (or use --kernel)")
    special.add_argument("--code", type=Path, help="checked kernel bytes for --problem")
    special.add_argument("--kernel", type=Path, help="checked-library entry; its problem and bytes are used")
    special.add_argument("--fix", action="append", required=True, metavar="NAME=VALUE",
                         help="input name and canonical unsigned decimal word; repeatable")
    special.add_argument("--out", required=True, type=Path, help="new directory; never overwrites")
    special.add_argument("--solver-ms", type=int, default=1000)
    sliced = commands.add_parser("slice", help="per-result backward slices of a checked profile-2 kernel, each "
                                 "checked as a one-result kernel; writes nothing unless all are equivalent")
    sliced.add_argument("--problem", type=Path, help="profile-2 problem (or use --kernel)")
    sliced.add_argument("--code", type=Path, help="kernel bytes for --problem")
    sliced.add_argument("--kernel", type=Path, help="checked-library entry; its problem and bytes are used")
    sliced.add_argument("--out", required=True, type=Path, help="new directory; never overwrites")
    sliced.add_argument("--solver-ms", type=int, default=1000)
    replayed = commands.add_parser("specialize-replay", help="recompute a written specialization and recheck it")
    replayed.add_argument("--dir", required=True, type=Path)
    replayed.add_argument("--solver-ms", type=int, default=1000)
    confirmed = commands.add_parser("second-solver",
                                    help="recheck every library entry and confirm each equivalent verdict with the pinned cvc5")
    confirmed.add_argument("--dir", required=True, type=Path)
    confirmed.add_argument("--cvc5", required=True, type=Path)
    confirmed.add_argument("--solver-ms", type=int, default=10_000)
    args = parser.parse_args(argv)
    try:
        if args.command == "second-solver":
            from . import second_solver
            from .library import _validate
            from .model import read_json
            rows = []
            for path in sorted(args.dir.glob("*.json")):
                problem, code = _validate(read_json(path))
                verdict = second_solver.confirm(problem, code, args.cvc5, args.solver_ms)
                rows.append({"entry": path.name, "status": verdict["status"], "reason": verdict.get("reason"),
                             "second_solver": verdict["second_solver"]})
            agreed = bool(rows) and all(row["status"] == "equivalent" for row in rows)
            result = {"status": "confirmed" if agreed else "inconclusive", "entries": rows, "authority": "none"}
        elif args.command == "slice":
            from . import slicing
            from .model import read_json
            if (args.kernel is None) == (args.problem is None) or (args.kernel is None) == (args.code is None):
                raise ValueError("give exactly one of --problem with --code, or --kernel")
            if args.kernel is not None:
                from .library import _validate
                problem, code = _validate(read_json(args.kernel))
            else:
                problem, code = read_json(args.problem), session._code(args.code)
            result = slicing.slices(problem, code, args.solver_ms)
            if result["status"] == "sliced":
                result["record"] = slicing.write(result, args.out, problem, code)
            for row in result.get("slices", []):
                row.pop("problem", None)
        elif args.command in ("specialize", "specialize-replay"):
            from . import specialize
            if args.command == "specialize-replay":
                result = specialize.replay(args.dir, args.solver_ms)
            else:
                from .model import read_json
                if (args.kernel is None) == (args.problem is None) or (args.kernel is not None and args.code):
                    raise ValueError("give exactly one of --problem [--code] or --kernel")
                if args.kernel is not None:
                    from .library import _validate
                    problem, code = _validate(read_json(args.kernel))
                else:
                    problem = read_json(args.problem)
                    code = session._code(args.code) if args.code is not None else None
                result = specialize.specialize(problem, args.fix, args.out, code, args.solver_ms)
                result.pop("problem", None)
        elif args.command == "contract":
            from . import contracts
            from .model import read_json
            if args.action == "lower":
                result = contracts.lower(read_json(args.contract))
                if args.problem_out is not None and result["status"] == "lowered":
                    session._json(args.problem_out, result["problem"])
            else:
                result = contracts.check(read_json(args.contract), session._code(args.code), args.solver_ms)
        elif args.command == "realize":
            from . import realize
            from .model import read_json
            if args.session is not None:
                if args.below is not None or args.replay is not None:
                    raise ValueError("--session takes its bound from the session; --below and --replay need --problem")
                result = realize.realize_session(args.session, args.solver_ms, args.max_rounds, args.deadline_ms,
                                                 args.groups, args.workers, args.split_ms)
            elif args.replay is not None:
                result = realize.replay(read_json(args.problem), read_json(args.replay), args.solver_ms)
            else:
                result = realize.realize(read_json(args.problem), args.below, args.solver_ms, args.max_rounds,
                                         args.deadline_ms, groups=args.groups, workers=args.workers,
                                         split_ms=args.split_ms)
            if args.out is not None:
                session._json(args.out, result)
        elif args.command == "open":
            result = session.open_session(args.problem, args.session)
        elif args.command == "packet":
            result = session.packet(args.session)
        elif args.command == "candidate":
            result = session.submit(args.session, args.proposal, args.solver_ms)
        elif args.command == "search":
            result = session.search(args.session, args.max_candidates, args.max_checks,
                                    args.deadline_ms, args.solver_ms, prune=not args.no_prune)
        elif args.command == "split-search":
            result = session.split_search(args.session, args.max_candidates, args.max_checks,
                                          args.deadline_ms, args.solver_ms)
        elif args.command == "resume":
            result = session.resume(args.session, args.solver_ms)
        elif args.command == "emit":
            result = session.emit(args.session, args.out, args.solver_ms)
        elif args.command == "bench":
            from .bench import bench
            result = bench(args.artifact, args.out, args.iterations, args.repeats, args.solver_ms)
        elif args.command == "library":
            from . import library
            if args.action == "check":
                result = library.check(args.dir, args.solver_ms, args.native)
            elif args.action == "bench":
                result = library.bench(args.dir, args.out, args.iterations, args.repeats, args.solver_ms)
            else:
                result = library.add(args.artifact, args.dir, args.name, args.summary, args.route, args.solver_ms)
        else:
            result = session.replay_artifact(args.artifact, args.solver_ms)
        result.setdefault("authority", "none")
        print(json.dumps(result, sort_keys=True, allow_nan=False))
        status = result["status"] if "status" in result else "packet"
        return 2 if status in ("inconclusive", "no-checked-candidate") else 1 if status in ("counterexample", "refused", "failed", "not-applicable") else 0
    except ImportError as error:
        print(json.dumps({"status": "inconclusive", "reason": str(error), "authority": "none"}))
        return 2
    except (OSError, ValueError, KeyError, TypeError, RecursionError) as error:
        print(json.dumps({"status": "refused", "reason": str(error), "authority": "none"}))
        return 65


if __name__ == "__main__":
    sys.exit(main())
