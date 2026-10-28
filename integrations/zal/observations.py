#!/usr/bin/env python3
"""Check or replay source-bound ZAL observation contracts. Standard library only."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

import behavior
import behavior2
import observation_game
import relational_observation

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
SCHEMA = "zal/observation-bundle/1"
MAX_RESULT = 64 * 1024 * 1024
SOURCES = tuple("integrations/zal/" + name for name in (
    "observations.py", "observation_game.py", "relational_observation.py",
    "behavior.py", "behavior2.py", "diagnostics.py", "symmetry2.py")) + tuple(
    "tools/trace_game/" + name for name in ("knowledge.py", "arena_spec.py", "arena_check.py", "game_spec.py"))


def need(condition, message):
    if not condition:
        raise ValueError(message)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def read(path, maximum):
    need(not path.is_symlink() and path.is_file(), "regular input file required")
    with path.open("rb") as stream:
        raw = stream.read(maximum + 1)
    need(len(raw) <= maximum, "input byte bound")
    return raw


def unique(items):
    result = {}
    for k, v in items:
        need(k not in result, "duplicate JSON key")
        result[k] = v
    return result


def no_number(_):
    raise ValueError("JSON requires integers, not floating-point or nonfinite numbers")


def decode(raw):
    return json.loads(raw, object_pairs_hook=unique, parse_float=no_number, parse_constant=no_number)


def encode(value):
    return (json.dumps(value, sort_keys=True, indent=2, ensure_ascii=True) + "\n").encode("ascii")


def identities():
    return {name: sha(read(ROOT / name, 1024 * 1024)) for name in SOURCES}


def prepare(mode, model_bytes, profile_bytes):
    value = decode(profile_bytes)
    if mode == "game":
        model = behavior.parse(model_bytes.decode("utf-8"))
        return model, observation_game.parse_profile(value, model)
    need(mode == "relational", "observation mode")
    model = behavior2.parse(model_bytes.decode("utf-8"))
    return model, relational_observation.parse_profile(value, model)


def evaluate(mode, model, profile, limits):
    if mode == "game":
        return observation_game.check(model, profile, **limits)
    return relational_observation.check(model, profile, **limits)


def safe_output(directory):
    need(not any(p.is_symlink() for p in (directory, *directory.absolute().parents)),
         "symbolic-link output refused")
    need(not directory.resolve().is_relative_to(ROOT), "evidence output must be outside source")
    need(not directory.exists(), "fresh evidence directory required")


def write_bundle(directory, mode, model_bytes, profile_bytes, report, sources, limits):
    safe_output(directory)
    result_bytes = encode(report)
    need(len(result_bytes) <= MAX_RESULT, "result byte bound")
    artifacts = {"model.zal": model_bytes, "profile.json": profile_bytes, "result.json": result_bytes}
    binding = dict(schema=SCHEMA, mode=mode, authority="none", sources=sources,
                   python=list(sys.version_info[:3]), limits=limits,
                   artifacts={name: sha(raw) for name, raw in artifacts.items()})
    directory.mkdir(parents=True, exist_ok=False)
    for name, raw in {**artifacts, "binding.json": encode(binding)}.items():
        with (directory / name).open("xb") as stream:
            stream.write(raw)


def verify(directory, model_path, profile_path):
    """Verify exact expected input bytes and current checker sources; never run saved commands."""
    need(not directory.is_symlink(), "symbolic-link bundle refused")
    before = identities()
    binding = decode(read(directory / "binding.json", 64 * 1024))
    need(type(binding) is dict and set(binding) == {
        "schema", "mode", "authority", "sources", "python", "limits", "artifacts"}, "bundle fields")
    need(binding["schema"] == SCHEMA and binding["authority"] == "none", "bundle identity")
    need(binding["mode"] in ("game", "relational"), "bundle mode")
    need(encode(binding["sources"]) == encode(before), "checker source drift")
    need(encode(binding["python"]) == encode(list(sys.version_info[:3])), "Python version drift")
    need(type(binding["limits"]) is dict and set(binding["limits"]) == {"max_states", "max_edges", "max_work"},
         "bundle limits")
    need(type(binding["artifacts"]) is dict and set(binding["artifacts"]) == {
        "model.zal", "profile.json", "result.json"}, "bundle artifact coverage")
    files = {name: read(directory / name, MAX_RESULT if name == "result.json" else 64 * 1024)
             for name in binding["artifacts"]}
    need(all(binding["artifacts"][name] == sha(raw) for name, raw in files.items()), "bundle artifact drift")
    need(files["model.zal"] == read(model_path, 64 * 1024) and
         files["profile.json"] == read(profile_path, 64 * 1024), "expected observation subject mismatch")
    model, profile = prepare(binding["mode"], files["model.zal"], files["profile.json"])
    report = decode(files["result.json"])
    need(type(report) is dict and report.get("status") != "UNKNOWN", "UNKNOWN is not completion evidence")
    if binding["mode"] == "game":
        observation_game._limits(**binding["limits"])
        need(encode(report.get("limits")) == encode(binding["limits"]), "game limits mismatch")
        observation_game.verify(model, profile, report)
    else:
        replay = evaluate("relational", model, profile, binding["limits"])
        need(encode(replay) == encode(report), "relational replay mismatch")
    need(before == identities(), "checker source drift during replay")
    return report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, epilog=(
        "Exit 0: checked claim; 1: checked counterexample or unrealizability; "
        "2: UNKNOWN/refusal. Game mode is sure safety with optional bounded service "
        "and abstract cost monitors. Relational mode checks all finite synchronized "
        "histories of the admitted finite model. Neither route qualifies native execution."))
    parser.add_argument("mode", choices=("game", "relational", "verify"))
    parser.add_argument("model", type=Path)
    parser.add_argument("profile", type=Path)
    parser.add_argument("--out", type=Path, required=True, help="fresh evidence directory; existing for verify")
    parser.add_argument("--max-states", type=int, default=4096)
    parser.add_argument("--max-edges", type=int, default=100000)
    parser.add_argument("--max-work", type=int, default=1000000)
    args = parser.parse_args(argv)
    try:
        if args.mode == "verify":
            report = verify(args.out, args.model, args.profile)
        else:
            safe_output(args.out)
            before = identities()
            raw_model, raw_profile = read(args.model, 64 * 1024), read(args.profile, 64 * 1024)
            model, profile = prepare(args.mode, raw_model, raw_profile)
            limits = dict(max_states=args.max_states, max_edges=args.max_edges, max_work=args.max_work)
            report = evaluate(args.mode, model, profile, limits)
            need(before == identities(), "checker source drift during evaluation")
            write_bundle(args.out, args.mode, raw_model, raw_profile, report, before, limits)
        status = report["status"]
        print(json.dumps(dict(status=status, authority="none", model_revision=report["model_revision"],
                              profile_revision=report["profile_revision"],
                              reason=report.get("reason", report.get("stop")), scope=report["scope"]), sort_keys=True))
        return {"CHECKED": 0, "UNREALIZABLE": 1, "COUNTEREXAMPLE": 1, "UNKNOWN": 2}[status]
    except (OSError, ValueError, UnicodeError, KeyError, TypeError, RecursionError) as error:
        print(json.dumps(dict(status="UNKNOWN", authority="none", reason=str(error))))
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
