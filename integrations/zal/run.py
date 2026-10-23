"""Experimental ZAL authoring CLI; the factory remains the execution target."""

import argparse
import json
import os
from pathlib import Path
import sys

import behavior
from behavior import MAX_SOURCE
import behavior2
from factory import lower, qualify, write_new
from workflow import checker_identity


def verdict_exit(result):
    """Map core verdict fields to exits; successful query results stay successful."""
    if "comparison" in result:
        return {"same-functional-observations-on-declared-domain": 0, "different": 1}.get(result["comparison"], 2)
    if result.get("status") in {"counterexample", "fail", "none-in-scope"}:
        return 1
    if result.get("status") in {"unfinished", "unsupported", "UNKNOWN", "inconclusive"}:
        return 2
    return 0


def main():
    sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools"))
    from guide_help import example_help
    exit_help = "Exit 0 positive/answered, 1 counterexample/different/none-in-scope, 2 UNKNOWN/unfinished/unsupported/refused."
    parser = argparse.ArgumentParser(description=__doc__, epilog=exit_help + example_help("zal"), formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    import loop_coverage
    loop_coverage.add_command(sub)
    for command in ["legend", "check", "render", "export", "export-game", "qualify", "diff", "ui", "story", "why-not", "what-if",
                    "lower", "refine-check", "explain", "guide"]:
        p = sub.add_parser(command, epilog=exit_help + example_help("zal"), formatter_class=argparse.RawDescriptionHelpFormatter)
        p.add_argument("source", type=Path, nargs="?" if command in {"legend", "ui"} else None,
                       default=Path(__file__).parent / "examples/order.zal")
        p.add_argument("--syntax", choices=["symbolic", "english"], default="symbolic")
        if command == "check":
            p.add_argument("--no-symmetry", action="store_true", help="Check the full graph after validating symmetry obligations")
        if command == "render":
            p.add_argument("--to", choices=["symbolic", "english"], required=True)
        if command in {"export", "export-game", "lower"}:
            p.add_argument("--out", type=Path, required=command in {"export", "export-game"})
        if command == "export-game":
            p.add_argument("--property", required=True)
        if command == "refine-check":
            p.add_argument("contract", type=Path)
            p.add_argument("--cli", type=Path, required=True)
            p.add_argument("--max-checks", type=int, default=262144)
        if command == "qualify":
            p.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
            p.add_argument("--cli", type=Path)
            p.add_argument("--out", type=Path)
        if command == "diff":
            p.add_argument("candidate", type=Path)
            p.add_argument("--candidate-syntax", choices=["symbolic", "english"], default="symbolic")
        if command == "story":
            p.add_argument("--trace", type=Path, help="JSON array of {event, context} inputs to tell instead of the check counterexample")
        if command in {"why-not", "what-if"}:
            p.add_argument("--input", required=True, help="JSON object: state, event, context and, in profile 2, variables")
        if command == "why-not":
            p.add_argument("--want", required=True, help="JSON object naming the wanted outcome")
            p.add_argument("--scope", choices=["context", "input"], default="context")
        if command == "what-if":
            p.add_argument("--change", required=True, help="JSON object of named input changes")
        if command in {"explain", "guide"}:
            p.add_argument("--topic", default="", help="port:NAME or type:NAME to explain; a construct or object ID for guide")
            p.add_argument("--human", action="store_true", help="Print plain text instead of JSON")
        if command == "explain":
            p.add_argument("--input", help="JSON object of state, variables, event and context")
        if command == "ui":
            p.add_argument("--port", type=int, default=8765)
            p.add_argument("--workspace", type=Path, help="Use an existing terminal/MCP shared workspace")
    options = parser.parse_args()
    if options.command == "coverage":
        return loop_coverage.main(options)
    source = ""
    try:
        with options.source.open(encoding="ascii") as stream:
            source = stream.read(MAX_SOURCE + 1)
        first = next((line.strip() for line in source.splitlines() if line.strip() and not line.lstrip().startswith("#")), "")
        if first in {"zal effects 1;", "ZAL effects version 1.", "zal typed 1;", "ZAL typed version 1."}:
            return typed_or_effects(options, source, first)
        if options.command == "guide":
            raise behavior.Refusal("The guide command supports zal/effects/1 and zal/typed-fsm/1 sources")
        model = behavior2.parse_any(source, options.syntax)
        profile = behavior2 if isinstance(model, behavior2.Machine) else behavior
        if options.command == "render":
            print(model.render(options.to == "english"), end="")
            return 0
        if options.command == "check":
            result = (profile.check(model, checker_identity(), no_symmetry=options.no_symmetry)
                      if profile is behavior2 else profile.check(model, checker_identity()))
        elif options.command == "explain":
            if profile is not behavior2:
                raise behavior.Refusal("The explain command supports zal/effects/1 and zal/typed-fsm/1 sources")
            if options.input is None:
                raise behavior.Refusal("Profile-2 explain needs --input with state, variables, event and context")
            given = json.loads(options.input)
            keys = {"state", "variables", "event", "context"}
            if not isinstance(given, dict) or set(given) != keys:
                raise behavior.Refusal("Profile-2 explain input names exactly state, variables, event and context")
            result = profile.explain(model, given["state"], given["variables"], given["event"],
                                    given["context"], checker_identity())
        elif options.command == "legend":
            result = profile.legend(model)
        elif options.command == "diff":
            candidate = behavior2.parse_any(options.candidate.read_text(encoding="ascii"), options.candidate_syntax)
            if type(candidate) is not type(model):
                raise behavior.Refusal("Comparison needs two models of the same profile")
            result = profile.semantic_diff(model, candidate)
        elif options.command == "story":
            import explain
            if options.trace:
                result = explain.narrate_trace(model, json.loads(options.trace.read_text(encoding="ascii")), checker_identity())
            else:
                result = explain.narrate_check(model, profile.check(model, checker_identity()), checker_identity())
        elif options.command in {"why-not", "what-if"}:
            import explain
            import explain2
            given = json.loads(options.input)
            keys = {"state", "event", "context"} | ({"variables"} if profile is behavior2 else set())
            if not isinstance(given, dict) or set(given) != keys:
                raise behavior.Refusal("The input names exactly " + ", ".join(sorted(keys)))
            module = explain2 if profile is behavior2 else explain
            head = [given["state"], *([given["variables"]] if profile is behavior2 else []), given["event"], given["context"]]
            if options.command == "why-not":
                result = module.why_not(model, *head, json.loads(options.want), options.scope, checker_identity())
            else:
                result = module.what_if(model, *head, json.loads(options.change), checker_identity())
        elif options.command in {"export", "lower"}:
            if options.out is None:
                raise behavior.Refusal("Finite-FSM lowering needs --out with an absent or empty directory")
            write_new(lower(model), options.out)
            result = {"status": "lowered" if options.command == "lower" else "exported",
                      "revision": model.revision, "authority": "none"}
        elif options.command == "export-game":
            return write_game_export(model, options, source)
        elif options.command == "refine-check":
            from factory import refine_check
            result, code = refine_check(model, options.contract, options.cli, options.max_checks)
            print(json.dumps(result, indent=2))
            return code
        elif options.command == "qualify":
            root = options.root.resolve()
            target = Path(os.environ.get("CARGO_TARGET_DIR", str(root / "target")))
            if not target.is_absolute():
                target = root / target
            cli = options.cli.resolve() if options.cli else target / "debug/zeno-fcis"
            result = qualify(model, root, cli, options.out)
        else:
            if profile is behavior2:
                raise behavior.Refusal("The browser workspace supports profile zal/finite-fsm/1 only")
            from server import serve
            serve(source if options.syntax == "symbolic" else model.render(), Path.cwd(), options.port, options.workspace)
            return 0
        print(json.dumps(result, indent=2))
        return verdict_exit(result)
    except (ValueError, OSError) as error:
        from diagnostics import describe
        print(json.dumps({"status": "refused", "diagnostic": str(error),
                          "source_diagnostic": describe(error, source, options.source), "authority": "none"}), file=sys.stderr)
        return 2


def write_game_export(model, options, source):
    """Bind paths and emit the immutable core result without interpreting status."""
    decision = behavior2.export_game_decision(model, options.property, str(options.out),
                                               source, str(options.source))
    if decision.text is not None:
        options.out.parent.mkdir(parents=True, exist_ok=True)
        options.out.write_text(decision.text, encoding="ascii")
    print(decision.status_json, file=sys.stderr if decision.to_stderr else sys.stdout)
    return decision.exit_code


def typed_or_effects(options, source, first):
    """Render, check, lower, explain and guide for effect signatures and typed machines."""
    import effects
    import effects_guide
    import typed2
    module = effects if first in {"zal effects 1;", "ZAL effects version 1."} else typed2
    model = module.parse(source, options.syntax)
    if options.command == "export-game":
        return write_game_export(model, options, source)
    signatures = isinstance(model, effects.Signatures)
    if options.command == "render":
        print(model.render(options.to == "english"), end="")
        return 0
    if options.command == "check":
        result = (effects if signatures else typed2).check(model, checker_identity())
    elif options.command == "legend":
        result = effects_guide.legend(model)
    elif options.command == "lower" and signatures:
        record = effects.lower(model)
        result = {"status": "lowered", "revision": model.revision, "schema": model.schema, "bytes": len(record),
                  "identity": effects.identity(model), "canonical": record.hex(), "authority": "none"}
    elif options.command == "guide":
        result = effects_guide.guide(model, options.topic, checker_identity())
    elif options.command == "explain" and signatures:
        result = effects_guide.explain(model, options.topic)
    elif options.command == "explain" and options.input:
        given = json.loads(options.input)
        if not isinstance(given, dict) or set(given) != {"state", "variables", "event", "context"}:
            raise behavior.Refusal("The input names exactly context, event, state, variables")
        result = typed2.explain(model, given["state"], given["variables"], given["event"], given["context"],
                                checker_identity())
    else:
        raise behavior.Refusal(f"The {options.command} command does not support {model.data()['profile']} sources")
    if getattr(options, "human", False) and result.get("kind") in {"language-help", "type-explanation", "effect-explanation"}:
        print(effects_guide.human_text(result))
    else:
        print(json.dumps(result, indent=2))
    return verdict_exit(result)


if __name__ == "__main__":
    raise SystemExit(main())
