"""Reject GitHub Actions expressions that use a context where GitHub forbids it.

GitHub refuses the whole workflow file, and runs none of its jobs, when an
expression names a context that its key does not allow; a job-level `env`
reading `runner.temp` is the classic case. The table below is GitHub's
"Context availability" table
(https://docs.github.com/en/actions/reference/workflows-and-actions/contexts).
A key absent from the table admits no expression at all, so an unknown key
fails closed. The walker reads the block YAML style used in this repository
without a YAML dependency.
"""

from __future__ import annotations

import re

_STEP = (
    "github", "needs", "strategy", "matrix", "job", "runner", "env", "vars",
    "secrets", "steps", "inputs",
)
_STEP_IF = tuple(context for context in _STEP if context != "secrets")
_JOB_STATIC = ("github", "needs", "strategy", "matrix", "vars", "inputs")

# Workflow key -> (allowed contexts, allowed special functions).
CONTEXT_AVAILABILITY: dict[str, tuple[frozenset[str], frozenset[str]]] = {
    key: (frozenset(contexts), frozenset(functions))
    for key, contexts, functions in (
        ("run-name", ("github", "inputs", "vars"), ()),
        ("concurrency", ("github", "inputs", "vars"), ()),
        ("env", ("github", "secrets", "inputs", "vars"), ()),
        ("jobs.<job_id>.concurrency", _JOB_STATIC, ()),
        ("jobs.<job_id>.container", _JOB_STATIC, ()),
        ("jobs.<job_id>.container.credentials", _JOB_STATIC + ("env", "secrets"), ()),
        ("jobs.<job_id>.container.env.<env_id>",
         _JOB_STATIC + ("job", "runner", "env", "secrets"), ()),
        ("jobs.<job_id>.container.image", _JOB_STATIC, ()),
        ("jobs.<job_id>.continue-on-error", _JOB_STATIC, ()),
        ("jobs.<job_id>.defaults.run", _JOB_STATIC + ("env",), ()),
        ("jobs.<job_id>.env", _JOB_STATIC + ("secrets",), ()),
        ("jobs.<job_id>.environment", _JOB_STATIC, ()),
        ("jobs.<job_id>.environment.url",
         _JOB_STATIC + ("job", "runner", "env", "steps"), ()),
        ("jobs.<job_id>.if", ("github", "needs", "vars", "inputs"),
         ("always", "cancelled", "success", "failure")),
        ("jobs.<job_id>.name", _JOB_STATIC, ()),
        ("jobs.<job_id>.outputs.<output_id>", _STEP, ()),
        ("jobs.<job_id>.runs-on", _JOB_STATIC, ()),
        ("jobs.<job_id>.secrets.<secrets_id>", _JOB_STATIC + ("secrets",), ()),
        ("jobs.<job_id>.services", _JOB_STATIC, ()),
        ("jobs.<job_id>.services.<service_id>.credentials",
         _JOB_STATIC + ("env", "secrets"), ()),
        ("jobs.<job_id>.services.<service_id>.env.<env_id>",
         _JOB_STATIC + ("job", "runner", "env", "secrets"), ()),
        ("jobs.<job_id>.steps.continue-on-error", _STEP, ("hashFiles",)),
        ("jobs.<job_id>.steps.env", _STEP, ("hashFiles",)),
        ("jobs.<job_id>.steps.if", _STEP_IF,
         ("always", "cancelled", "success", "failure", "hashFiles")),
        ("jobs.<job_id>.steps.name", _STEP, ("hashFiles",)),
        ("jobs.<job_id>.steps.run", _STEP, ("hashFiles",)),
        ("jobs.<job_id>.steps.timeout-minutes", _STEP, ("hashFiles",)),
        ("jobs.<job_id>.steps.with", _STEP, ("hashFiles",)),
        ("jobs.<job_id>.steps.working-directory", _STEP, ("hashFiles",)),
        ("jobs.<job_id>.strategy", ("github", "needs", "vars", "inputs"), ()),
        ("jobs.<job_id>.timeout-minutes", _JOB_STATIC, ()),
        ("jobs.<job_id>.with.<with_id>", _JOB_STATIC, ()),
        ("on.workflow_call.inputs.<inputs_id>.default", ("github", "inputs", "vars"), ()),
        ("on.workflow_call.outputs.<output_id>.value", ("github", "jobs", "vars", "inputs"), ()),
    )
}

# Functions GitHub allows in every expression.
GENERAL_FUNCTIONS = frozenset(
    {"contains", "startsWith", "endsWith", "format", "join", "toJSON", "fromJSON"}
)
LITERALS = frozenset({"true", "false", "null"})

_KEY_LINE = re.compile(r"^(?P<dash>-\s+)?(?P<key>[A-Za-z0-9_][A-Za-z0-9_.-]*|\"[^\"]+\"|'[^']+'):(?:\s+(?P<value>.*))?$")
_EXPRESSION = re.compile(r"\$\{\{(.*?)\}\}", re.DOTALL)
_STRING = re.compile(r"'(?:[^']|'')*'")
_NAME = re.compile(r"[A-Za-z_][A-Za-z0-9_-]*")


def key_path(keys: list[str]) -> str | None:
    """Map a raw key stack to its row in GitHub's table, or None when no row exists."""
    if not keys:
        return None
    top = keys[0]
    if top in ("run-name", "concurrency", "env"):
        return top
    if top == "on":
        if len(keys) >= 5 and keys[1] == "workflow_call":
            if keys[2] == "inputs" and keys[4] == "default":
                return "on.workflow_call.inputs.<inputs_id>.default"
            if keys[2] == "outputs" and keys[4] == "value":
                return "on.workflow_call.outputs.<output_id>.value"
        return None
    if top != "jobs" or len(keys) < 3:
        return None
    job, rest = "jobs.<job_id>", keys[2:]
    first = rest[0]
    if first == "steps":
        if len(rest) < 2:
            return None
        return f"{job}.steps.{rest[1]}"
    if first in ("env", "strategy", "services", "container", "environment", "concurrency") and len(rest) == 1:
        return f"{job}.{first}"
    if first in ("env", "strategy", "concurrency"):
        return f"{job}.{first}"
    if first == "container":
        if rest[1] == "env":
            return f"{job}.container.env.<env_id>"
        if rest[1] in ("credentials", "image"):
            return f"{job}.container.{rest[1]}"
        return f"{job}.container"
    if first == "services":
        if len(rest) >= 3 and rest[2] == "env":
            return f"{job}.services.<service_id>.env.<env_id>"
        if len(rest) >= 3 and rest[2] == "credentials":
            return f"{job}.services.<service_id>.credentials"
        return f"{job}.services"
    if first == "environment":
        return f"{job}.environment.url" if rest[1] == "url" else f"{job}.environment"
    if first == "defaults":
        return f"{job}.defaults.run" if len(rest) >= 2 and rest[1] == "run" else None
    if first == "outputs":
        return f"{job}.outputs.<output_id>"
    if first == "runs-on":
        return f"{job}.runs-on"
    if first == "with":
        return f"{job}.with.<with_id>"
    if first == "secrets":
        return f"{job}.secrets.<secrets_id>"
    if len(rest) == 1:
        return f"{job}.{first}"
    return None


def expression_names(expression: str) -> tuple[set[str], set[str]]:
    """Return (context roots, called functions) named by one expression body."""
    text = _STRING.sub("''", expression)
    contexts: set[str] = set()
    functions: set[str] = set()
    for match in _NAME.finditer(text):
        before = text[: match.start()].rstrip()
        if before.endswith("."):
            continue
        name = match.group(0)
        if text[match.end():].lstrip().startswith("("):
            functions.add(name)
        elif name not in LITERALS:
            contexts.add(name)
    return contexts, functions


def _expressions(raw_key: str, value: str) -> list[str]:
    found = [match.group(1) for match in _EXPRESSION.finditer(value)]
    if not found and raw_key == "if" and value.strip():
        # `if:` values are expressions even without the ${{ }} wrapper.
        bare = re.sub(r"\s+#.*$", "", value.strip())
        if bare and bare not in ("|", ">", "|-", ">-"):
            found.append(bare)
    return found


def _check(path: str, line: int, keys: list[str], value: str) -> list[str]:
    raw_key = keys[-1] if keys else ""
    expressions = _expressions(raw_key, value)
    if not expressions:
        return []
    row = key_path(keys)
    location = f"{path}:{line}: {'.'.join(keys)}"
    if row is None or row not in CONTEXT_AVAILABILITY:
        return [f"{location}: GitHub allows no expression at this key"]
    contexts, functions = CONTEXT_AVAILABILITY[row]
    failures: list[str] = []
    for expression in expressions:
        used, called = expression_names(expression)
        for context in sorted(used - contexts):
            failures.append(f"{location}: context `{context}` is unavailable at {row}")
        for function in sorted(called - functions - GENERAL_FUNCTIONS):
            failures.append(f"{location}: function `{function}()` is unavailable at {row}")
    return failures


def check_contexts(path: str, text: str) -> list[str]:
    """Check every expression in one workflow file against GitHub's availability table."""
    failures: list[str] = []
    # Stack of (key column, key, inline value) for the open mapping keys.
    stack: list[tuple[int, str, str]] = []
    pending: list[tuple[int, list[str], list[str]]] = []  # block or continued values

    def flush() -> None:
        while pending:
            start, keys, parts = pending.pop()
            failures.extend(_check(path, start, keys, "\n".join(parts)))

    for number, raw in enumerate(text.splitlines(), start=1):
        stripped = raw.strip()
        if not stripped:
            continue
        indent = len(raw) - len(raw.lstrip(" "))
        if stack and stack[-1][2] and indent > stack[-1][0] and pending:
            # Continuation of the innermost key's block scalar or flow value,
            # including `#` lines: GitHub expands expressions inside them too.
            pending[-1][2].append(raw)
            continue
        if stripped.startswith("#"):
            continue
        flush()
        match = _KEY_LINE.match(stripped)
        is_item = stripped.startswith("- ") or stripped == "-"
        column = indent + (len(stripped) - len(stripped[2:].lstrip()) if is_item else 0)
        while stack and (stack[-1][0] > indent if is_item else stack[-1][0] >= column):
            stack.pop()
        if match is None:
            # A scalar sequence item or flow mapping item belongs to its parent key.
            keys = [entry[1] for entry in stack]
            value = stripped[2:] if is_item else stripped
            pending.append((number, keys, [value]))
            if stack:
                stack[-1] = (stack[-1][0], stack[-1][1], stack[-1][2] or value)
            continue
        key = match.group("key").strip("\"'")
        value = match.group("value") or ""
        stack.append((column, key, value))
        if value:
            pending.append((number, [entry[1] for entry in stack], [value]))
    flush()
    return failures
