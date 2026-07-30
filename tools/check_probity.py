#!/usr/bin/env python3
"""Exercise the exact pinned Probity configuration with hostile payloads."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXPECTED_NODE = "v22.23.1"
EXPECTED_PROBITY = "1.10.0"
PROBITY = ROOT / "node_modules" / "@nizos" / "probity" / "dist" / "bin.js"


class ProbityCheckError(ValueError):
    """Raised when the pinned deterministic guardrail surface drifts."""


def node_binary() -> str:
    configured = os.environ.get("NODE_BIN")
    candidate = configured or shutil.which("node")
    if candidate is None:
        raise ProbityCheckError("Node is unavailable; install exact Node 22.23.1")
    version = subprocess.run(
        [candidate, "--version"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    if version != EXPECTED_NODE:
        raise ProbityCheckError(
            f"Node version is {version!r}; expected exact {EXPECTED_NODE!r}"
        )
    return candidate


def invoke(
    node: str,
    command: str,
    *,
    transcript: Path | None = None,
) -> str:
    payload = {
        "session_id": "zeno-fcis-probity-self-test",
        "turn_id": "self-test",
        "cwd": str(ROOT),
        "hook_event_name": "PreToolUse",
        "model": "deterministic-self-test",
        "permission_mode": "default",
        "tool_name": "Bash",
        "tool_input": {"command": command},
        "tool_use_id": "self-test-call",
    }
    if transcript is not None:
        payload["transcript_path"] = str(transcript)
    result = subprocess.run(
        [
            node,
            str(PROBITY),
            "--agent",
            "codex",
            "--config",
            "probity.config.ts",
        ],
        cwd=ROOT,
        check=True,
        input=json.dumps(payload),
        capture_output=True,
        text=True,
    )
    return result.stdout


def invoke_raw(node: str, payload: str) -> str:
    """Invoke Probity with malformed input to prove fail-closed admission."""

    result = subprocess.run(
        [
            node,
            str(PROBITY),
            "--agent",
            "codex",
            "--config",
            "probity.config.ts",
        ],
        cwd=ROOT,
        check=True,
        input=payload,
        capture_output=True,
        text=True,
    )
    return result.stdout


def require_block(node: str, command: str, *, transcript: Path | None = None) -> None:
    output = invoke(node, command, transcript=transcript)
    try:
        decoded = json.loads(output)
    except json.JSONDecodeError as error:
        raise ProbityCheckError(f"{command!r} did not return a block response") from error
    if decoded.get("decision") != "block" or not decoded.get("reason"):
        raise ProbityCheckError(f"{command!r} was not blocked: {decoded!r}")


def require_allow(node: str, command: str, *, transcript: Path | None = None) -> None:
    output = invoke(node, command, transcript=transcript)
    if output != "":
        raise ProbityCheckError(f"{command!r} was unexpectedly blocked: {output}")


def command_transcript(path: Path, command: str, *, write_after: bool) -> None:
    entries: list[dict[str, object]] = [
        {
            "type": "response_item",
            "payload": {
                "type": "function_call",
                "name": "exec_command",
                "arguments": json.dumps({"cmd": command, "workdir": str(ROOT)}),
                "call_id": "acceptance-call",
            },
        },
        {
            "type": "response_item",
            "payload": {
                "type": "function_call_output",
                "call_id": "acceptance-call",
                "output": "atdd: complete PASS",
            },
        },
    ]
    if write_after:
        entries.extend(
            [
                {
                    "type": "response_item",
                    "payload": {
                        "type": "custom_tool_call",
                        "name": "apply_patch",
                        "input": "*** Begin Patch\n*** Add File: changed\n+x\n*** End Patch\n",
                        "call_id": "write-call",
                    },
                },
                {
                    "type": "response_item",
                    "payload": {
                        "type": "custom_tool_call_output",
                        "call_id": "write-call",
                        "output": "Success",
                    },
                },
            ]
        )
    path.write_text(
        "".join(json.dumps(entry, sort_keys=True) + "\n" for entry in entries),
        encoding="utf-8",
    )


def main() -> int:
    try:
        if not PROBITY.is_file():
            raise ProbityCheckError("Probity is absent; run `npm ci --ignore-scripts`")
        node = node_binary()
        version = subprocess.run(
            [node, str(PROBITY), "--version"],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        if version != EXPECTED_PROBITY:
            raise ProbityCheckError(
                f"Probity version is {version!r}; expected {EXPECTED_PROBITY!r}"
            )

        hostile = (
            "git reset --hard HEAD",
            "git clean -fd",
            "cargo update",
            "cargo publish -p zeno-fcis",
            "git tag v1.0.0-rc.3",
            "gh pr merge 91",
            "gh release create v1.0.0-rc.3",
            "cargo +1.97.1 test --workspace --locked | cargo test",
            "cargo +1.97.1 test --workspace --locked |& cargo test",
            "cargo test --workspace --locked",
            "cargo +1.97.1 test --workspace",
            "npm install",
        )
        for command in hostile:
            require_block(node, command)

        malformed = invoke_raw(node, "not-json")
        try:
            malformed_decision = json.loads(malformed)
        except json.JSONDecodeError as error:
            raise ProbityCheckError(
                "malformed hook input did not return a block response"
            ) from error
        if (
            malformed_decision.get("decision") != "block"
            or not malformed_decision.get("reason")
        ):
            raise ProbityCheckError(
                f"malformed hook input was not blocked: {malformed_decision!r}"
            )

        permitted = (
            "git status --short",
            "python3 tools/atdd.py check",
            "cargo +1.97.1 test --workspace --locked",
            (
                "cargo +1.97.1 test \\\n"
                "  --workspace \\\n"
                "  --locked"
            ),
        )
        for command in permitted:
            require_allow(node, command)

        with tempfile.TemporaryDirectory(prefix="zeno-fcis-probity-") as directory:
            clean = Path(directory) / "clean.jsonl"
            stale = Path(directory) / "stale.jsonl"
            command_transcript(
                clean,
                "python3 tools/atdd.py run --all",
                write_after=False,
            )
            command_transcript(
                stale,
                "python3 tools/atdd.py run --all",
                write_after=True,
            )
            require_allow(node, "git commit -m acceptance", transcript=clean)
            require_block(node, "git commit -m stale", transcript=stale)
            empty = Path(directory) / "empty.jsonl"
            empty.write_text("", encoding="utf-8")
            require_block(node, "git commit -m unchecked", transcript=empty)

        print(
            "probity: PASS "
            f"(Node {EXPECTED_NODE[1:]}, Probity {EXPECTED_PROBITY}, "
            "15 hostile and 5 permitted actions)"
        )
        return 0
    except (ProbityCheckError, subprocess.CalledProcessError) as error:
        print(f"probity: FAIL: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
