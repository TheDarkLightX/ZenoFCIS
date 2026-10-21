"""Render command help examples from public guide cards for Python entrypoints."""
import json
from pathlib import Path


def example_help(name):
    try:
        text = (Path(__file__).resolve().parents[1] / "docs/loops" / f"{name}.md").read_text()
        card = json.loads(text.removeprefix("```json\n").split("\n```", 1)[0])
        commands = [step["command"] for step in card["steps"]] + [card["example"]["command"]]
        return "\nEngine: " + card["engine"] + "\nExamples (repository root; WORK is fresh scratch):\n" + "\n".join(commands)
    except (OSError, ValueError, KeyError):
        return "Read the public command guide for examples."
