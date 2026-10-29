"""Ontology commands for the included ZAL library; no model provider required."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

from . import core
from .library_lens_browser import read_artifact
from .ontology_cnl import parse_ontology_cnl
from .ontology_tools import classify, dispatch, tool_definitions


def read_text(path):
    if path == "-":
        data = sys.stdin.buffer.read(core.MAX_BYTES + 1)
        core.need(len(data) <= core.MAX_BYTES, "input byte budget exceeded")
    else:
        source = Path(path).absolute()
        data = read_artifact(source.parent, source.name)
    return data.decode("utf-8")


def emit(value, output=None):
    core.canonical(value)
    text = json.dumps(value, ensure_ascii=False, indent=2, allow_nan=False) + "\n"
    core.need(len(text.encode("utf-8")) <= core.MAX_BYTES, "output byte budget exceeded")
    if output:
        with Path(output).open("x", encoding="utf-8") as stream:
            stream.write(text)
    else:
        sys.stdout.write(text)


def request(document):
    core.keys(document, {"name", "arguments"}, where="ontology tool request")
    return dispatch(document["name"], document["arguments"])


def stdio():
    """Bounded JSON requests for the ontology tools; this is not MCP transport."""
    while True:
        data = sys.stdin.buffer.readline(core.MAX_BYTES + 1)
        if not data:
            return 0
        core.need(len(data) <= core.MAX_BYTES and data.endswith(b"\n"),
                  "ontology request exceeds its byte bound or is incomplete")
        try:
            result = request(core.loads(data.decode("utf-8")))
        except (ValueError, TypeError, KeyError, RecursionError) as error:
            result = {"status": "refused", "diagnostic": str(error)[:2000], "authority": "none"}
        encoded = core.canonical(result)
        sys.stdout.write(encoded + "\n")
        sys.stdout.flush()


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    parse = commands.add_parser("ontology-parse")
    parse.add_argument("--source", required=True)
    parse.add_argument("--output")
    classify_command = commands.add_parser("ontology-classify")
    source = classify_command.add_mutually_exclusive_group(required=True)
    source.add_argument("--ontology")
    source.add_argument("--source")
    classify_command.add_argument("--max-facts", type=int, default=1024)
    classify_command.add_argument("--max-work", type=int, default=1_000_000)
    classify_command.add_argument("--output")
    handoff_command = commands.add_parser("arithmetic-handoff")
    source = handoff_command.add_mutually_exclusive_group(required=True)
    source.add_argument("--ontology")
    source.add_argument("--source")
    handoff_command.add_argument("--bindings", required=True)
    handoff_command.add_argument("--max-facts", type=int, default=1024)
    handoff_command.add_argument("--max-work", type=int, default=1_000_000)
    handoff_command.add_argument("--output")
    for name in ("ontology-demo", "ontology-browse"):
        command = commands.add_parser(name)
        command.add_argument("--output", required=True)
        if name == "ontology-browse":
            source = command.add_mutually_exclusive_group(required=True)
            source.add_argument("--ontology")
            source.add_argument("--source")
    replay = commands.add_parser("ontology-replay")
    replay.add_argument("--directory", required=True)
    replay.add_argument("--output")
    toolkit = commands.add_parser("toolkit")
    toolkit.add_argument("--output")
    discuss = commands.add_parser("discuss")
    discuss.add_argument("--request", required=True)
    discuss.add_argument("--output")
    commands.add_parser("tools-stdio")
    args = parser.parse_args(argv)
    try:
        if args.command == "tools-stdio":
            return stdio()
        if args.command == "toolkit":
            emit({"schema": "zal.ontology-toolkit/1", "tools": tool_definitions(),
                  "scope": "Ontology operations; the general library retains its Python APIs.",
                  "authority": "none"}, args.output)
        elif args.command == "discuss":
            emit(request(core.loads(read_text(args.request))), args.output)
        elif args.command == "ontology-parse":
            emit(parse_ontology_cnl(read_text(args.source)), args.output)
        elif args.command == "ontology-classify":
            document = (core.loads(read_text(args.ontology)) if args.ontology
                        else parse_ontology_cnl(read_text(args.source)))
            run = classify(document, max_facts=args.max_facts, max_work=args.max_work)
            emit(run, args.output)
            return 0 if run["result"]["status"] == "classified" else 2
        elif args.command == "arithmetic-handoff":
            from arithmetic_handoff_tools import handoff
            document = (core.loads(read_text(args.ontology)) if args.ontology
                        else parse_ontology_cnl(read_text(args.source)))
            bindings = core.loads(read_text(args.bindings))
            core.keys(bindings, {"bindings"}, where="binding document")
            result = handoff(document, bindings["bindings"], max_facts=args.max_facts, max_work=args.max_work)
            emit(result, args.output)
            return 0 if result["status"] == "ready" else 2
        elif args.command in ("ontology-demo", "ontology-browse"):
            from .ontology_demo import generate
            document = None
            if args.command == "ontology-browse":
                document = (core.loads(read_text(args.ontology)) if args.ontology
                            else parse_ontology_cnl(read_text(args.source)))
            result = generate(Path(args.output), ontology=document)
            emit(result)
            return 0 if result["status"] == "classified" else 2
        else:
            from .ontology_demo import replay
            emit(replay(Path(args.directory)), args.output)
        return 0
    except (ValueError, TypeError, KeyError, OSError, RecursionError) as error:
        emit({"status": "refused", "diagnostic": str(error)[:2000], "authority": "none"})
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
