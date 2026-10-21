#!/usr/bin/env python3
"""Keep embedded cards exact; execute every Steps and Worked example contract."""
import argparse
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
SECTIONS = ["Use when", "Not for", "Inputs", "Steps", "Verdicts", "Cost", "Coordinate", "Pitfalls", "Worked example", "See also"]
FIELDS = ["synopsis", "engine", "inputs", "outputs", "exit_codes", "verdicts", "limits", "refusals", "cost", "use_when", "not_for", "example", "sources"]


def cards():
    for path in sorted((ROOT / "docs/loops").glob("*.md")):
        text = path.read_text()
        metadata = json.loads(text.removeprefix("```json\n").split("\n```", 1)[0])
        assert metadata["name"] == path.stem, path
        assert all(field in metadata for field in FIELDS), path
        assert re.findall(r"^## (.*)$", text, re.M) == SECTIONS, path
        yield path, text, metadata


def selected(report, path):
    value = report
    for part in path.split("."):
        value = value[int(part)] if isinstance(value, list) else value[part]
    return value


def check_copies(write):
    target = ROOT / "crates/zeno-fcis-cli/guide-cards"
    target.mkdir(exist_ok=True)
    expected = set()
    for path, text, metadata in cards():
        copy = target / (path.stem + ".txt")
        if write:
            copy.write_text(text)
        assert copy.read_bytes() == path.read_bytes(), f"embedded card differs: {path.stem}"
        expected.add(copy.name)
    assert {path.name for path in target.iterdir()} == expected


def run_examples(cli, destination, only=()):
    results = []
    destination.mkdir(parents=True, exist_ok=True)
    description = subprocess.run([str(cli), "describe"], capture_output=True, text=True, check=True)
    (destination / "describe.json").write_text(description.stdout)
    with tempfile.TemporaryDirectory(prefix="guide-path-") as bin_directory:
        (Path(bin_directory) / "zeno-fcis").symlink_to(cli.resolve())
        env = {**os.environ, "PATH": bin_directory + os.pathsep + os.environ["PATH"],
            "ZENO_FCIS_CLI": str(cli.resolve()), "ZENO_FCIS_BUILD_TREE": str(ROOT),
            "CARGO_PROFILE_DEV_DEBUG": "0", "CARGO_PROFILE_TEST_DEBUG": "0"}
        env.pop("ZENO_FCIS_KB", None)
        for path, text, card in cards():
            if only and card["name"] not in only:
                continue
            work = destination / card["name"]
            work.mkdir()
            env["WORK"] = str(work)
            # Check the displayed commands, not just metadata unused by readers.
            displayed = []
            for section in ("Steps", "Worked example"):
                body = text.split("## " + section + "\n", 1)[1].split("\n## ", 1)[0]
                displayed.extend(re.findall(r"```sh\n(.*?)\n```", body, re.S))
            steps = card["steps"] + [card["example"]]
            assert displayed == [step["command"] for step in steps], path
            for index, step in enumerate(steps):
                start = time.monotonic()
                result = subprocess.run(["nice", "-n", "19", "ionice", "-c3", "sh", "-c", step["command"]],
                    cwd=ROOT, env=env, text=True, capture_output=True, timeout=1200, check=False)
                (work / f"{index}.stdout").write_text(result.stdout)
                (work / f"{index}.stderr").write_text(result.stderr)
                assert result.returncode == step["exit"], (card["name"], step["command"], result.returncode, result.stderr[-1000:])
                if step.get("expected"):
                    report = json.loads(result.stdout)
                    for key, value in step["expected"].items():
                        assert selected(report, key) == value, (card["name"], key, value, selected(report, key))
                for excerpt in step.get("contains", []):
                    assert excerpt in result.stdout + result.stderr, (card["name"], excerpt)
                results.append({"card": card["name"], "command": step["command"], "exit_code": result.returncode,
                    "duration_ms": round((time.monotonic() - start) * 1000)})
            for command in card["commands"]:
                help_result = subprocess.run([str(cli), *shlex.split(command), "--help"], env=env,
                    text=True, capture_output=True, check=False)
                (work / ("help-" + command.replace(" ", "-") + ".txt")).write_text(help_result.stdout)
                assert help_result.returncode == 0, command
                assert card["example"]["command"] in help_result.stdout, (command, "worked example absent from help")
            for index, command in enumerate(card.get("help_commands", [])):
                help_result = subprocess.run(["nice", "-n", "19", "ionice", "-c3", "sh", "-c", command],
                    cwd=ROOT, env=env, text=True, capture_output=True, check=False)
                (work / f"python-help-{index}.txt").write_text(help_result.stdout)
                assert help_result.returncode == 0, command
                assert card["example"]["command"] in help_result.stdout, (command, "worked example absent from help")
            print(card["name"], "examples and help passed", flush=True)
    (destination / "results.json").write_text(json.dumps(results, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true")
    parser.add_argument("--run", action="store_true")
    parser.add_argument("--card", action="append", default=[])
    parser.add_argument("--cli", type=Path)
    parser.add_argument("--evidence", type=Path)
    args = parser.parse_args()
    check_copies(args.write)
    if args.run:
        assert args.cli and args.evidence, "--run requires --cli and --evidence"
        run_examples(args.cli, args.evidence, args.card)
    print("guide cards: PASS")


if __name__ == "__main__":
    try:
        from telemetry import run_cli
    except ImportError:
        # Reporting is optional, including for a standalone copy of this tool.
        run_cli = lambda function: function()
    def _entrypoint():
        main()
    run_cli(_entrypoint)
