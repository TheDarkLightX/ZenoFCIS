"""Read-only, explicitly configured knowledge tools. No writes or raw command recipes."""
from __future__ import annotations
import json
import os
from pathlib import Path
import shutil
import subprocess


class KnowledgeToolError(ValueError):
    pass


def _invoke(operation, paths, task, **fields):
    if operation not in {"advise", "plan", "explain"}:
        raise KnowledgeToolError("kb-operation-refused")
    if type(paths) is not list or len(paths) > 7 or any(type(p) is not str or not p or len(p) > 4096 for p in paths):
        raise KnowledgeToolError("kb-paths")
    if type(task) is not str or len(task.encode()) > 16384:
        raise KnowledgeToolError("kb-task")
    command = os.environ.get("ZENO_FCIS_CLI") or shutil.which("zeno-fcis")
    if not command:
        raise KnowledgeToolError("kb-cli-unavailable")
    arguments = [str(Path(command).resolve()), "kb", operation]
    if operation == "explain":
        query = fields.get("query")
        if type(query) is not str or not query or len(query.encode()) > 4096:
            raise KnowledgeToolError("kb-query")
        arguments.append(query)
    arguments += ["--task", task]
    for p in paths:
        arguments += ["--kb", p]
    if operation == "plan":
        for key in ("have", "want"):
            value = fields.get(key)
            if type(value) is not str or not value or len(value.encode()) > 4096:
                raise KnowledgeToolError("kb-types")
            arguments += ["--" + key, value]
    environment = dict(os.environ)
    environment.pop("ZENO_FCIS_KB", None)  # Only explicit paths are disclosed to an MCP caller.
    try:
        result = subprocess.run(arguments, stdin=subprocess.DEVNULL, capture_output=True,
                                timeout=10, check=False, env=environment)
        if len(result.stdout) > 2_000_000:
            raise KnowledgeToolError("kb-output-bound")
        report = json.loads(result.stdout)
        if type(report) is not dict or report.get("authority") != "none":
            raise KnowledgeToolError("kb-output-authority")
    except (OSError, subprocess.TimeoutExpired, ValueError, TypeError) as error:
        # No stderr, source substrings, task facts or malformed output in diagnostics.
        raise KnowledgeToolError("kb-call-unavailable") from None
    return {"exit_code": result.returncode, "report": report, "authority": "none"}


def kb_advise(paths: list[str], task: str) -> dict:
    """Return private advice to this caller only; pending/stale records abstain."""
    return _invoke("advise", paths, task)


def kb_plan(paths: list[str], task: str, have: str, want: str) -> dict:
    """Propose a constrained route; execute one step then ask with observed artifacts."""
    return _invoke("plan", paths, task, have=have, want=want)


def kb_explain(paths: list[str], task: str, query: str) -> dict:
    """Return one source-bound derivation per answer to this explicitly private call."""
    return _invoke("explain", paths, task, query=query)


TOOLS = (kb_advise, kb_plan, kb_explain)
ANNOTATIONS = {"readOnlyHint": True, "destructiveHint": False, "openWorldHint": False}
