"""Local declaration authoring and finite synthesis tools using the actual CLI."""

import json
import os
from pathlib import Path
import shutil
import subprocess
from typing import Any

import functools

from mcp.server import MCPServer
from mcp.server.mcpserver.exceptions import ToolError

from zeno_fcis_transform import TOOLS as TRANSFORM_TOOLS, TransformToolError


mcp = MCPServer("ZenoFCIS synthesis")
PROFILE = "zeno-fcis/finite-i64/1"
TARGETS = {"rust", "python", "javascript"}
PROGRAM_TEMPLATES = {"durable-counter", "account-lockout", "order-fulfillment",
                     "inventory-reservation", "withdrawal-queue", "agent-treasury-guard",
                     "prepared-counter", "compliance-gateway"}


def _absolute(path: str) -> Path:
    if not path:
        raise ToolError("A nonempty path is required")
    return Path(path).expanduser().resolve()


def _domain_size(field: dict) -> int:
    domain = field["type"]
    if domain["kind"] == "bool":
        return 2
    if domain["kind"] == "int":
        low, high = domain["min"], domain["max"]
        if type(low) is not int or type(high) is not int or low > high:
            raise ValueError("invalid integer bounds")
        return high - low + 1
    raise ValueError("unsupported domain kind")


def _count(fields: list) -> int:
    product = 1
    for field in fields:
        product *= _domain_size(field)
    return product


@mcp.tool()
def assess_finite_problem(problem_path: str) -> dict[str, Any]:
    """Assess finite-domain size before trying synthesis; this is no proof of the app."""
    path = _absolute(problem_path)
    try:
        problem = json.loads(path.read_text(encoding="utf-8"))
        if problem["profile"] != PROFILE:
            raise ValueError("unsupported profile")
        inputs = _count(problem["inputs"])
        outputs = _count(problem["outputs"])
    except (OSError, ValueError, KeyError, TypeError) as error:
        raise ToolError(f"cannot assess finite problem: {error}") from error
    return {"profile": PROFILE, "input_tuples": inputs, "output_tuples": outputs,
            "within_tuple_limits": inputs <= 65_536 and outputs <= 4_096,
            "limits": {"inputs": 65_536, "outputs": 4_096,
                       "graph_nodes": 256, "evaluated_nodes": 100_000_000},
            "next": "Run the CLI for grammar and work-budget validation; review the complete core contract and adapter separately."}


def _cli() -> str:
    command = os.environ.get("ZENO_FCIS_CLI") or shutil.which("zeno-fcis")
    if not command:
        raise ToolError("Install zeno-fcis CLI or set ZENO_FCIS_CLI to its absolute path")
    path = _absolute(command)
    if not path.is_file():
        raise ToolError(f"CLI does not exist: {path}")
    return str(path)


def _program_command(arguments: list[str], json_report: bool) -> dict[str, Any]:
    try:
        result = subprocess.run([_cli(), *arguments], capture_output=True, text=True,
                                timeout=180, check=False)
    except (OSError, subprocess.TimeoutExpired) as error:
        raise ToolError(f"authoring process did not complete: {error}") from error
    if json_report:
        try:
            report = json.loads(result.stdout)
        except ValueError as error:
            raise ToolError(f"CLI returned invalid JSON (exit {result.returncode}): {result.stderr[:1000]}") from error
        if report.get("authority") not in {"none", "diagnostic-only"}:
            raise ToolError("Unexpected authority claim from CLI")
    else:
        report = {"status": "created" if result.returncode == 0 else "refused",
                  "stdout": result.stdout, "authority": "none",
                  "runtime_admission": "not-run"}
    return {"exit_code": result.returncode, "report": report,
            "stderr": result.stderr[:1000], "authority": "none",
            "note": "Authoring declarations only. Review complete original schema, policy, branches, footprints and laws; bind and execute through the checked library before a Publication can exist."}


@mcp.tool()
def discover_program_authoring() -> dict[str, Any]:
    """Discover the actual CLI and normal checked Program declaration workflow."""
    return _program_command(["describe"], True)


@mcp.tool()
def create_program_project(output_path: str, template: str = "durable-counter") -> dict[str, Any]:
    """Copy a supported complete declaration example; refuse a nonempty destination."""
    if template not in PROGRAM_TEMPLATES:
        raise ToolError(f"Unsupported checked program template: {template}")
    return _program_command(["new", str(_absolute(output_path)), "--template", template], False)


@mcp.tool()
def check_project_spec(project_path: str, require_substantive: bool = False) -> dict[str, Any]:
    """Check .zeno declarations and diagnostics; this does not admit or publish a Program."""
    arguments = ["check", str(_absolute(project_path)), "--format", "json"]
    if require_substantive:
        arguments.append("--require-substantive")
    return _program_command(arguments, True)


def _invoke(operation: str, problem_path: str, output_path: str, target: str, check: bool) -> dict[str, Any]:
    if target not in TARGETS:
        raise ToolError(f"Unsupported target: {target}")
    problem, output = _absolute(problem_path), _absolute(output_path)
    if not problem.is_file():
        raise ToolError(f"Problem does not exist: {problem}")
    if check and operation != "run":
        raise ToolError("--check applies only to synth run")
    args = [_cli(), "synth", operation, str(problem), "--target", target,
            "--out", str(output)]
    if check:
        args.append("--check")
    try:
        result = subprocess.run(args, capture_output=True, text=True, timeout=180,
                                check=False)
    except (OSError, subprocess.TimeoutExpired) as error:
        raise ToolError(f"synthesis process did not complete: {error}") from error
    try:
        report = json.loads(result.stdout)
    except ValueError as error:
        raise ToolError(f"CLI returned invalid JSON (exit {result.returncode}): {result.stderr[:1000]}") from error
    if report.get("authority") != "none":
        raise ToolError("Unexpected authority claim from CLI")
    return {"exit_code": result.returncode, "report": report,
            "stderr": result.stderr[:1000], "problem": str(problem), "output": str(output),
            "note": "Finite core evidence only; the application adapter and laws need independent checks."}


@mcp.tool()
def synthesize_finite_core(problem_path: str, output_path: str, target: str = "rust",
                           check_existing: bool = False) -> dict[str, Any]:
    """Run bounded synthesis. check_existing refuses artifact drift without overwriting."""
    return _invoke("run", problem_path, output_path, target, check_existing)


@mcp.tool()
def verify_finite_core(problem_path: str, output_path: str, target: str = "rust") -> dict[str, Any]:
    """Rebuild and execute the exact generated target on every admitted input."""
    return _invoke("verify", problem_path, output_path, target, False)


def _transform_tool(handler):
    """Register a transform-loop handler; its refusals become MCP tool errors."""
    @functools.wraps(handler)
    def tool(*arguments, **keywords):
        try:
            return handler(*arguments, **keywords)
        except TransformToolError as error:
            raise ToolError(str(error)) from error
    return mcp.tool()(tool)


# transform_request, transform_candidate and transform_replay drive the bounded
# optimization loop (`zeno-fcis loop`); see docs/LLM_SYNTHESIS.md.
for _handler in TRANSFORM_TOOLS:
    _transform_tool(_handler)


if __name__ == "__main__":
    mcp.run()
