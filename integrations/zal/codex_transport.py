"""Bounded stdio client for the installed Codex app-server protocol.

This outer adapter grants no domain agreement or factory authority. It does
not change accounts or configuration. Tool/command approval and domain revision
acceptance are different events; this client never approves a tool request.
"""

from __future__ import annotations

import json
from pathlib import Path
import shutil
import subprocess
import threading
import time
from typing import Any, Callable


MAX_MESSAGE = 512 * 1024
MAX_EVENTS = 2048


class TransportError(ValueError):
    """Protocol refusal, timeout, or disconnected child process."""


class AppServer:
    """One owned child process, reader and request table; no shell invocation."""

    def __init__(self, on_event: Callable[[dict], None], executable: str = "codex"):
        resolved = shutil.which(executable)
        if not resolved:
            raise TransportError("Codex CLI is unavailable; use the labeled replay demo")
        self.executable = str(Path(resolved).resolve())
        self.on_event = on_event
        self.process: subprocess.Popen | None = None
        self.condition = threading.Condition()
        self.write_lock = threading.Lock()
        self.responses: dict[int, dict] = {}
        self.pending: set[int] = set()
        self.questions: dict[int, dict] = {}
        self.next_id = 1
        self.closed = False
        self.event_count = 0
        self.thread_id: str | None = None
        self.turn_id: str | None = None

    def connect(self) -> dict:
        self.process = subprocess.Popen(
            [self.executable, "app-server", "--listen", "stdio://"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
        )
        threading.Thread(target=self._read, daemon=True).start()
        result = self.request("initialize", {
            "clientInfo": {"name": "zenofcis_zal", "title": "ZAL dialogue prototype", "version": "0.1.0"},
        })
        self.send({"method": "initialized"})
        return result

    def start_thread(self, cwd: str, model: str | None = None) -> str:
        params = {
            "cwd": str(Path(cwd).resolve()), "sandbox": "read-only", "approvalPolicy": "on-request",
            "ephemeral": True,
            "developerInstructions": (
                "Discuss software behavior through the supplied ZAL objects. "
                "Do not use shell, network, MCP or other tools. Use the final structured response. "
                "You may explain, ask, compare or propose; you cannot accept a domain revision. "
                "Context observations are assumptions. Describe uncertainty explicitly."
            ),
        }
        if model:
            params["model"] = model
        result = self.request("thread/start", params)
        self.thread_id = result["thread"]["id"]
        return self.thread_id

    def start_turn(self, text: str, schema: dict) -> str:
        if not self.thread_id:
            raise TransportError("Connect a discussion thread first")
        result = self.request("turn/start", {
            "threadId": self.thread_id, "input": [{"type": "text", "text": text}],
            "outputSchema": schema,
        })
        self.turn_id = result["turn"]["id"]
        return self.turn_id

    def steer(self, text: str, expected_turn_id: str) -> dict:
        if not self.thread_id or expected_turn_id != self.turn_id:
            raise TransportError("The selected turn is stale")
        return self.request("turn/steer", {
            "threadId": self.thread_id, "expectedTurnId": expected_turn_id,
            "input": [{"type": "text", "text": text}],
        })

    def interrupt(self) -> dict:
        if not self.thread_id or not self.turn_id:
            raise TransportError("There is no active turn to interrupt")
        return self.request("turn/interrupt", {"threadId": self.thread_id, "turnId": self.turn_id})

    def answer(self, request_id: int, answers: dict[str, list[str]]) -> None:
        """Only a human input route calls this; answers cannot accept a revision."""
        with self.condition:
            question = self.questions.get(request_id)
            if not question:
                raise TransportError("The question is stale")
            ids = {q["id"] for q in question["params"]["questions"]}
            if set(answers) != ids or any(not isinstance(v, list) or not v or
                                         any(not isinstance(a, str) or len(a) > 4096 for a in v)
                                         for v in answers.values()):
                raise TransportError("Answer every question with bounded text")
            self.send({"id": request_id, "result": {"answers": {
                key: {"answers": value} for key, value in answers.items()
            }}})
            del self.questions[request_id]

    def request(self, method: str, params: dict, timeout: float = 30) -> dict:
        with self.condition:
            request_id = self.next_id
            self.next_id += 1
            self.pending.add(request_id)
        try:
            self.send({"id": request_id, "method": method, "params": params})
            end = time.monotonic() + timeout
            with self.condition:
                while request_id not in self.responses and not self.closed:
                    remaining = end - time.monotonic()
                    if remaining <= 0:
                        raise TransportError(f"{method} timed out")
                    self.condition.wait(remaining)
                if self.closed:
                    raise TransportError("Codex app-server disconnected")
                response = self.responses.pop(request_id)
            if "error" in response:
                raise TransportError(str(response["error"].get("message", "Protocol error"))[:2000])
            return response["result"]
        finally:
            with self.condition:
                self.pending.discard(request_id)

    def send(self, message: dict) -> None:
        data = json.dumps(message, ensure_ascii=True, separators=(",", ":")).encode() + b"\n"
        if len(data) > MAX_MESSAGE:
            raise TransportError("Protocol message exceeds 512 KiB")
        with self.write_lock:
            if not self.process or not self.process.stdin or self.closed:
                raise TransportError("Codex app-server is disconnected")
            try:
                self.process.stdin.write(data)
                self.process.stdin.flush()
            except (OSError, BrokenPipeError) as error:
                raise TransportError("Codex app-server write failed") from error

    def _dispatch(self, message: Any) -> None:
        if not isinstance(message, dict):
            raise TransportError("Expected a JSON-RPC object")
        if "id" in message and "method" not in message:
            with self.condition:
                if type(message["id"]) is int and message["id"] in self.pending:
                    self.responses[message["id"]] = message
                    self.condition.notify_all()
            return
        if "id" in message:
            if message.get("method") == "item/tool/requestUserInput":
                with self.condition:
                    if len(self.questions) >= 8:
                        raise TransportError("Too many pending human questions")
                    self.questions[message["id"]] = message
            else:
                # Refuse all execution/permission/unknown requests. No automatic approvals.
                self.send({"id": message["id"], "error": {
                    "code": -32601, "message": "This dialogue client does not approve tool execution",
                }})
        self.event_count += 1
        if self.event_count > MAX_EVENTS:
            raise TransportError("Discussion event budget exhausted; reconnect")
        if message.get("method") == "turn/completed":
            with self.condition:
                turn = message.get("params", {}).get("turn", {}).get("id")
                self.questions = {key: q for key, q in self.questions.items()
                                  if q.get("params", {}).get("turnId") != turn}
        self.on_event(message)

    def _read(self) -> None:
        try:
            assert self.process and self.process.stdout
            while True:
                data = self.process.stdout.readline(MAX_MESSAGE + 1)
                if not data:
                    if not self.closed:
                        self.on_event({"method": "zal/transportError", "params": {"message": "Codex app-server disconnected"}})
                    break
                if len(data) > MAX_MESSAGE or not data.endswith(b"\n"):
                    raise TransportError("Overlong or incomplete protocol message")
                self._dispatch(json.loads(data))
        except (ValueError, OSError, AssertionError) as error:
            self.on_event({"method": "zal/transportError", "params": {"message": str(error)[:2000]}})
        finally:
            with self.condition:
                self.closed = True
                self.condition.notify_all()

    def close(self) -> None:
        with self.condition:
            self.closed = True
            self.condition.notify_all()
        if self.process:
            self.process.terminate()
            try:
                self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=3)
            for stream in [self.process.stdin, self.process.stdout]:
                if stream:
                    stream.close()
