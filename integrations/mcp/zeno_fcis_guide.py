"""Read-only lookup of the CLI's embedded public command cards."""
import json
import os
import re
import shutil
import subprocess

ANNOTATIONS = {"readOnlyHint": True, "destructiveHint": False, "openWorldHint": False}


class GuideToolError(ValueError):
    """A refused guide lookup."""


def _lookup(arguments):
    executable = os.environ.get("ZENO_FCIS_CLI") or shutil.which("zeno-fcis")
    if not executable:
        raise GuideToolError("Install the CLI or set ZENO_FCIS_CLI.")
    try:
        result = subprocess.run([executable, "--no-kb", "guide", *arguments],
            capture_output=True, text=True, timeout=15, check=False)
    except (OSError, subprocess.TimeoutExpired) as error:
        raise GuideToolError("Guide lookup did not complete.") from error
    if result.returncode:
        raise GuideToolError("Unknown card or unavailable guide; run guide list.")
    if arguments[0] == "show":
        text = result.stdout
        try:
            metadata = json.loads(text.split("```json\n", 1)[1].split("\n```", 1)[0])
            if metadata["name"] != arguments[1]:
                raise ValueError("card name differs")
        except (ValueError, KeyError, IndexError) as error:
            raise GuideToolError("Invalid card response.") from error
        return {"schema": "zeno-fcis/guide/1", "authority": "none", "card": text}
    try:
        report = json.loads(result.stdout)
        if report.get("schema") != "zeno-fcis/guide/1" or report.get("authority") != "none":
            raise ValueError("unexpected guide contract")
    except ValueError as error:
        raise GuideToolError("Invalid guide response.") from error
    return report


def guide_list():
    """List embedded public cards; reads no project, private KB or application."""
    return _lookup(["list"])


def guide_show(name: str):
    """Read one card with exact command contracts and executable examples."""
    if not isinstance(name, str) or not re.fullmatch(r"[a-z][a-z0-9-]{0,79}", name):
        raise GuideToolError("Use an exact card name from guide_list.")
    return _lookup(["show", name])


def guide_find(task: str):
    """Rank declared task phrases deterministically; retrieval grants no authority."""
    if not isinstance(task, str) or not task.strip() or len(task) > 4096:
        raise GuideToolError("Supply 1-4096 characters of task words.")
    return _lookup(["find", "--", task])


TOOLS = (guide_list, guide_show, guide_find)
