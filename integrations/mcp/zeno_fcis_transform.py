"""Transform-loop tools: an agent drives `zeno-fcis loop` one checked candidate at a time.

`transform_request` admits an original program and opens a session directory,
`transform_candidate` spends one attempt on a candidate the agent supplies, and
`transform_replay` re-admits the session and replays its incumbent (NSM-001).
Every verdict comes from the library-owned checker through the CLI; a candidate
is data, never code, and no argument lets a caller assert that it passed
(NSM-002). Provider identifiers or explanations an agent attaches are untrusted
provenance (NSM-003). The handlers depend only on the standard library so they
can be tested without the MCP SDK; `zeno_fcis_synthesis.py` registers them.
"""

import json
import os
from pathlib import Path
import shutil
import subprocess
from typing import Any

LOOP_SCHEMA = "zeno-fcis/transform-loop/1"
PROFILES = ("functional-bool-v1", "checked-i64-v1")
NOTE = ("Checker-derived feedback over the declared input domain under eager semantics. "
        "No optimality, convergence or application authority is implied; a receipt never publishes.")


class TransformToolError(ValueError):
    """A refused tool call; the server maps it to the MCP ToolError."""


def _absolute(path: str) -> Path:
    if not isinstance(path, str) or not path:
        raise TransformToolError("A nonempty path is required")
    return Path(path).expanduser().resolve()


def _new_path(path: str) -> Path:
    """Resolve only the parent; the CLI must see and refuse an existing final link."""
    if not isinstance(path, str) or not path:
        raise TransformToolError("A nonempty path is required")
    expanded = Path(path).expanduser()
    return expanded.parent.resolve() / expanded.name


def _cli() -> str:
    command = os.environ.get("ZENO_FCIS_CLI") or shutil.which("zeno-fcis")
    if not command:
        raise TransformToolError("Install zeno-fcis CLI or set ZENO_FCIS_CLI to its absolute path")
    path = _absolute(command)
    if not path.is_file():
        raise TransformToolError(f"CLI does not exist: {path}")
    return str(path)


def _bounded(name: str, value: Any, low: int, high: int) -> str:
    if type(value) is not int or not low <= value <= high:
        raise TransformToolError(f"{name} must be an integer in {low}..{high}")
    return str(value)


def _loop(arguments: list[str]) -> dict[str, Any]:
    """Runs one fixed `zeno-fcis loop` subcommand; no shell, no free-form arguments."""
    try:
        result = subprocess.run([_cli(), "loop", *arguments], capture_output=True, text=True,
                                timeout=120, check=False)
    except (OSError, subprocess.TimeoutExpired) as error:
        raise TransformToolError(f"loop process did not complete: {error}") from error
    try:
        report = json.loads(result.stdout)
    except ValueError as error:
        raise TransformToolError(
            f"CLI returned invalid JSON (exit {result.returncode}): {result.stderr[:1000]}") from error
    if report.get("schema") != LOOP_SCHEMA or report.get("authority") != "none":
        raise TransformToolError("Unexpected schema or authority claim from CLI")
    return {"exit_code": result.returncode, "report": report, "stderr": result.stderr[:1000],
            "authority": "none", "note": NOTE}


def transform_request(original_path: str, session_path: str, profile: str = "functional-bool-v1",
                      attempts: int = 8, checks: int = 8, deadline_ms: int = 20_000) -> dict[str, Any]:
    """Admit an original program and open a loop session; hosted providers stay disabled."""
    if profile not in PROFILES:
        raise TransformToolError(f"Unsupported profile: {profile}")
    original = _absolute(original_path)
    if not original.is_file():
        raise TransformToolError(f"Original does not exist: {original}")
    return _loop(["open", "--original", str(original), "--session", str(_absolute(session_path)),
                  "--profile", profile,
                  "--attempts", _bounded("attempts", attempts, 1, 8),
                  "--checks", _bounded("checks", checks, 1, 8),
                  "--deadline-ms", _bounded("deadline_ms", deadline_ms, 1, 20_000)])


def transform_candidate(session_path: str, candidate_path: str = "",
                        candidate_json_path: str = "") -> dict[str, Any]:
    """Spend one attempt on a candidate (canonical bytes or fixture JSON) and return checker feedback."""
    if bool(candidate_path) == bool(candidate_json_path):
        raise TransformToolError("Give exactly one of candidate_path or candidate_json_path")
    if candidate_path:
        candidate = _absolute(candidate_path)
        selector = "--candidate"
    else:
        candidate = _absolute(candidate_json_path)
        selector = "--candidate-json"
    if not candidate.is_file():
        raise TransformToolError(f"Candidate does not exist: {candidate}")
    return _loop(["candidate", "--session", str(_absolute(session_path)), selector, str(candidate)])


def transform_replay(session_path: str) -> dict[str, Any]:
    """Re-admit the session, verify its ledger and replay the incumbent before reporting it."""
    return _loop(["resume", "--session", str(_absolute(session_path))])


DRAFT_SCHEMA = "zeno-fcis/contract-draft/1"
DRAFT_NOTE = ("Supplied intent labels are assumptions, not authenticated owner approval. "
              "F2 is advisory; finalization writes a draft, never adopts or publishes it.")


def _draft(operation: str, arguments: list[str]) -> dict[str, Any]:
    """Fixed CLI operation; all parsing, evaluation and state transitions stay in Rust."""
    try:
        result = subprocess.run([_cli(), "contract", "draft", operation, *arguments],
                                capture_output=True, text=True, timeout=120, check=False)
    except (OSError, subprocess.TimeoutExpired) as error:
        raise TransformToolError(f"draft process did not complete: {error}") from error
    try:
        report = json.loads(result.stdout)
    except ValueError as error:
        raise TransformToolError(f"CLI returned invalid draft JSON (exit {result.returncode})") from error
    if (not isinstance(report, dict) or report.get("schema") != DRAFT_SCHEMA
            or report.get("authority") != "none" or report.get("hosted_model") != "off"):
        raise TransformToolError("Unexpected draft schema, authority or hosted-model claim")
    return {"exit_code": result.returncode, "report": report, "stderr": result.stderr[:1000],
            "authority": "none", "note": DRAFT_NOTE}


def _supplied(name: str, value: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise TransformToolError(f"{name} must be nonempty")
    return value


def contract_draft_start(session_path: str, intent_path: str, project_path: str,
                         provenance: str, examples_path: str = "", rounds: int = 4,
                         max_tuples: int = 4096) -> dict[str, Any]:
    """Start from intent and optional supplied examples. No hosted model or authentication."""
    arguments = ["--session", str(_new_path(session_path)), "--intent", str(_absolute(intent_path)),
                 "--project", str(_absolute(project_path)), "--provenance", _supplied("provenance", provenance),
                 "--rounds", _bounded("rounds", rounds, 1, 8),
                 "--max-tuples", _bounded("max_tuples", max_tuples, 1, 4096)]
    if examples_path:
        arguments.extend(["--examples", str(_absolute(examples_path))])
    return _draft("start", arguments)


def contract_draft_propose(session_path: str, revision: str, rules_path: str,
                           provenance: str) -> dict[str, Any]:
    """Spend a bounded attempt on a complete supplied rules file; retain invalid proposals."""
    return _draft("propose", ["--session", str(_absolute(session_path)),
                             "--revision", _supplied("revision", revision),
                             "--rules", str(_absolute(rules_path)),
                             "--provenance", _supplied("provenance", provenance)])


def contract_draft_questions(session_path: str) -> dict[str, Any]:
    """Return the library's F2 table and current distinguishing questions; suggestions are not labels."""
    return _draft("questions", ["--session", str(_absolute(session_path))])


def contract_draft_label(session_path: str, revision: str, examples_path: str,
                         provenance: str) -> dict[str, Any]:
    """Record explicitly supplied expected decisions for this exact revision's questions."""
    return _draft("label", ["--session", str(_absolute(session_path)),
                           "--revision", _supplied("revision", revision),
                           "--examples", str(_absolute(examples_path)),
                           "--provenance", _supplied("provenance", provenance)])


def contract_draft_check(session_path: str) -> dict[str, Any]:
    """Rebind and compare all expected observations; no label means no success."""
    return _draft("check", ["--session", str(_absolute(session_path))])


def contract_draft_finalize(session_path: str, revision: str, out_path: str) -> dict[str, Any]:
    """Write a new labeled draft only when the current proposal passes; never adopt it."""
    return _draft("finalize", ["--session", str(_absolute(session_path)),
                              "--revision", _supplied("revision", revision),
                              "--out", str(_new_path(out_path))])


TOOLS = (transform_request, transform_candidate, transform_replay,
         contract_draft_start, contract_draft_propose, contract_draft_questions,
         contract_draft_label, contract_draft_check, contract_draft_finalize)
