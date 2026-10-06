"""Experimental ZAL authoring CLI; the factory remains the execution target."""

import argparse
import json
import os
from pathlib import Path
import sys

from behavior import MAX_SOURCE, check, legend, parse, semantic_diff
from factory import lower, qualify, write_new
from workflow import checker_identity


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    for command in ["legend", "check", "render", "export", "qualify", "diff", "ui"]:
        p = sub.add_parser(command)
        p.add_argument("source", type=Path, nargs="?" if command in {"legend", "ui"} else None,
                       default=Path(__file__).parent / "examples/order.zal")
        p.add_argument("--syntax", choices=["symbolic", "english"], default="symbolic")
        if command == "render":
            p.add_argument("--to", choices=["symbolic", "english"], required=True)
        if command == "export":
            p.add_argument("--out", type=Path, required=True)
        if command == "qualify":
            p.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
            p.add_argument("--cli", type=Path)
            p.add_argument("--out", type=Path)
        if command == "diff":
            p.add_argument("candidate", type=Path)
            p.add_argument("--candidate-syntax", choices=["symbolic", "english"], default="symbolic")
        if command == "ui":
            p.add_argument("--port", type=int, default=8765)
            p.add_argument("--workspace", type=Path, help="Use an existing terminal/MCP shared workspace")
    options = parser.parse_args()
    try:
        with options.source.open(encoding="ascii") as stream:
            source = stream.read(MAX_SOURCE + 1)
        model = parse(source, options.syntax)
        if options.command == "render":
            print(model.render(options.to == "english"), end="")
            return 0
        if options.command == "check":
            result = check(model, checker_identity())
        elif options.command == "legend":
            result = legend(model)
        elif options.command == "diff":
            result = semantic_diff(model, parse(options.candidate.read_text(encoding="ascii"), options.candidate_syntax))
        elif options.command == "export":
            write_new(lower(model), options.out)
            result = {"status": "exported", "revision": model.revision, "authority": "none"}
        elif options.command == "qualify":
            root = options.root.resolve()
            target = Path(os.environ.get("CARGO_TARGET_DIR", str(root / "target")))
            if not target.is_absolute():
                target = root / target
            cli = options.cli.resolve() if options.cli else target / "debug/zeno-fcis"
            result = qualify(model, root, cli, options.out)
        else:
            from server import serve
            serve(source if options.syntax == "symbolic" else model.render(), Path.cwd(), options.port, options.workspace)
            return 0
        print(json.dumps(result, indent=2))
        return 1 if result.get("status") in {"counterexample", "unfinished"} else 3 if result.get("status") == "unsupported" else 0
    except (ValueError, OSError) as error:
        print(json.dumps({"status": "refused", "diagnostic": str(error), "authority": "none"}), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
