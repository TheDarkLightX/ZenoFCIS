"""Local file and encoder boundary for the checked film plan."""
import argparse
import os
from pathlib import Path
import subprocess
from tempfile import TemporaryDirectory

from . import core
from .library_lens_browser import read_artifact
from .search_film import compile_film


def read_plan(source):
    source = Path(source).absolute()
    text = read_artifact(source.parent, source.name).decode()
    doc = core.loads(text)
    evidence = replay = None
    if "evidence" in doc:
        ref = doc["evidence"]
        evidence = read_artifact(source.parent, ref).decode()
        replay_ref = doc.get("replay")
        if doc.get("format") == "essay-film/1" and "replay" not in doc:
            replay_ref = str(Path(ref).with_name("replay.json"))
        replay = read_artifact(source.parent, replay_ref).decode()
    return compile_film(text, evidence, replay)


def summary(plan):
    walk = plan["walk"]
    byte = plan["bytes"]
    return {"modelDigest": walk["modelDigest"], "frames": plan["frames"], "fps": plan["fps"],
            "routes": len(walk["routes"]), "endpoints": len(walk["groups"]),
            "solutions": len(walk["solutions"]), "minimumTurns": walk["minTurns"],
            "optimalRoutes": walk["optima"], "byteChecks": len(byte["candidates"]) * 256 if byte else 0,
            "exactPrograms": [[c["l"], c["h"]] for c in byte["candidates"] if not c["misses"]] if byte else []}


def render(source, destination):
    plan = read_plan(source)
    output = Path(destination).absolute()
    core.need(not os.path.lexists(output), "film output must be a new directory")
    output.parent.mkdir(parents=True, exist_ok=True)
    stem = Path(source).stem
    renderer = Path(__file__).with_name("film_renderer") / "render.mjs"
    with TemporaryDirectory(prefix=".zal-film-", dir=output.parent) as temporary:
        stage = Path(temporary)
        (stage / (stem + ".plan.json")).write_text(plan + "\n", encoding="utf-8")
        result = subprocess.run(["node", str(renderer), str(stage), stem])
        core.need(result.returncode == 0, "film renderer failed; see encoder diagnostics")
        # Reserve the name exclusively after encoding. Concurrent renders cannot mix files.
        output.mkdir()
        try:
            for completed in stage.iterdir():
                completed.rename(output / completed.name)
        except OSError:
            # Recover files we moved; a failed build must not masquerade as complete.
            for name in (stem + suffix for suffix in (".plan.json", "-light.mp4", "-dark.mp4", ".vtt", ".svg")):
                if not (stage / name).exists() and (output / name).is_file():
                    (output / name).rename(stage / name)
            output.rmdir()
            raise
    return {**summary(core.loads(plan)), "directory": str(output)}


def main(argv=None):
    from .__main__ import emit
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("film-check", "film-render"):
        command = commands.add_parser(name)
        command.add_argument("source", help="path to the authored JSON storyboard")
        command.add_argument("--output", required=name == "film-render",
                             help="new plan file for check, or new directory for render")
    args = parser.parse_args(argv)
    try:
        if args.command == "film-render":
            emit(render(args.source, args.output))
        else:
            plan = core.loads(read_plan(args.source))
            if args.output:
                emit(plan, args.output)
            emit(summary(plan))
        return 0
    except (ValueError, TypeError, KeyError, OSError, RecursionError) as error:
        emit({"status": "refused", "diagnostic": str(error)[:2000], "authority": "none"})
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
