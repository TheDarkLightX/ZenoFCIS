"""Loopback companion shell. Client events own acceptance, not model responses."""

from __future__ import annotations

import argparse
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import secrets
import threading
from urllib.parse import urlsplit

from behavior import Refusal, canonical, explain, help_topic, legend, parse, replay
from codex_transport import AppServer, TransportError
from factory import lower
from shared import SharedWorkspace
from workflow import PROPOSAL_SCHEMA, Session, checker_identity


class Workspace:
    def __init__(self, source: str, cwd: Path, shared_path: Path | None = None):
        self.session = Session(parse(source))
        self.store = SharedWorkspace(shared_path) if shared_path is not None else None
        self.cwd = cwd
        self.lock = threading.RLock()
        self.transport: AppServer | None = None
        self.mode = "deterministic replay"
        self.live_model = None
        self.running: str | None = None
        self.turn_base: str | None = None
        self.messages: dict[str, str] = {}
        self.questions = []
        self.error: str | None = None
        self.version = 0
        self.connection_generation = 0
        self.operation = 0
        self.connecting = False
        self.start_pending = False
        self.cancel_pending = False
        self.buffered = []

    @contextmanager
    def domain(self):
        """Hold one fresh domain transaction; never hold it during inference."""
        with self.lock:
            if self.store is None:
                yield
                return
            with self.store.locked():
                current = self.store.load()
                before = canonical(current.view())
                if canonical(self.session.view()) != before:
                    self.version += 1
                self.session = current
                yield
                if canonical(self.session.view()) != before:
                    self.store.save(self.session)

    def view(self):
        with self.domain():
            return {**self.session.view(), "mode": self.mode, "live_model": self.live_model, "running": self.running,
                    "questions": json.loads(canonical(self.questions)), "error": self.error, "version": self.version,
                    "workspace_mode": "shared terminal/MCP workspace" if self.store else "isolated browser session",
                    "legend": legend(self.session.model)}

    def notify(self, event, generation=None):
        with self.domain():
            if generation is not None and generation != self.connection_generation:
                return
            method, params = event.get("method"), event.get("params", {})
            if method == "zal/transportError":
                self.error = params.get("message", "Disconnected")
                self.running = None
                self.operation += 1
                self.start_pending = False
                self.buffered.clear()
                self.questions.clear()
                self.version += 1
                return
            if not self.transport or params.get("threadId") != self.transport.thread_id:
                return
            if self.start_pending:
                if len(self.buffered) >= 2048:
                    self.cancel_pending = True
                    self.error = "Pending turn event bound exceeded; import revoked"
                else:
                    self.buffered.append(json.loads(canonical(event)))
                return
            turn_id = params.get("turnId") or params.get("turn", {}).get("id")
            if not self.running or self.running in {"interrupting", "cancelling-start"} or turn_id != self.running:
                return
            if method == "item/tool/requestUserInput":
                self.questions.append(json.loads(canonical(event)))
            elif method == "item/completed" and params.get("item", {}).get("type") == "agentMessage":
                turn = params.get("turnId")
                if turn == self.running:
                    self.messages[turn] = params["item"].get("text", "")[:65536]
            elif method == "turn/completed":
                turn = params.get("turn", {})
                if turn.get("id") != self.running:
                    return
                try:
                    if turn.get("status") == "completed" and self.turn_base == self.session.model.revision:
                        if turn["id"] not in self.messages:
                            raise Refusal("Completed turn has no final structured message")
                        self.session.agent_act(json.loads(self.messages[turn["id"]]))
                    else:
                        self.error = "Turn interrupted, failed or stale; no proposal applied"
                except (ValueError, TypeError) as error:
                    self.error = str(error)
                finally:
                    self.messages.pop(turn.get("id"), None)
                    self.questions.clear()
                    self.running = None
            else:
                return  # Streaming deltas do not invalidate the user's focus or draft.
            self.version += 1

    def connect(self):
        with self.lock:
            if self.running or self.connecting:
                raise Refusal("Interrupt the current turn before reconnecting")
            self.connecting = True
            self.connection_generation += 1
            generation = self.connection_generation
            previous, self.transport = self.transport, None
        if previous:
            previous.close()
        candidate = None
        try:
            candidate = AppServer(lambda event: self.notify(event, generation))
            candidate.connect()
            available = candidate.request("model/list", {}).get("data", [])
            chosen = next((m["model"] for m in available if m.get("isDefault") and not m.get("hidden")), None)
            if not chosen:
                raise Refusal("Codex has no available default model; use replay")
            candidate.start_thread(str(self.cwd), chosen)
        except Exception:
            if candidate:
                candidate.close()
            with self.lock:
                self.connecting = False
            raise
        with self.lock:
            self.transport = candidate
            self.live_model = chosen
            self.mode = "live Codex stdio"
            self.questions = []
            self.error = None
            self.connecting = False
            self.version += 1

    def discuss(self, message, revision, demo=False, inputs=None, cursor=None):
        with self.lock:
            with self.domain():
                if revision != self.session.model.revision:
                    raise Refusal("Question uses a stale revision")
                if self.running or self.connecting:
                    raise Refusal("A turn is running; steer or interrupt it")
                if not demo and not self.transport:
                    raise Refusal("Connect Codex first, or use the labeled replay")
                exploration = None
                if inputs is not None:
                    steps = replay(self.session.model, inputs)
                    if type(cursor) is not int or not 0 <= cursor <= len(steps):
                        raise Refusal("Exploration cursor is outside the replayed trace")
                    exploration = {"revision": revision, "cursor": cursor, "trace": steps[:cursor],
                                   "state": steps[cursor - 1]["outcome"]["state"] if cursor else self.session.model.initial,
                                   "observations": "assumed inputs; independently replayed by the client"}
                text = self.session.context(message, exploration)
                self.error = None
                if demo:
                    self.mode = "deterministic replay"
                    model = self.session.model
                    extra = "rule cancel_approved: approved + cancel [true] -> accept cancelled;\n"
                    if all(name in model.states for name in ["approved", "cancelled"]) and "cancel" in model.events and not any(r.id == "cancel_approved" for r in model.rules):
                        self.session.agent_act({"act": "propose", "base_revision": revision,
                            "object_ids": [self.session.selected], "message": "Replay example: allow cancellation after approval. The witness shows a previously rejected cancellation now accepted. Review the exact paraphrase before choosing.",
                            "syntax": "symbolic", "surface": model.render() + extra})
                    else:
                        self.session.agent_act({"act": "explain", "base_revision": revision,
                            "object_ids": [self.session.selected], "message": "Replay: the selected behavior is in the synchronized views. All unmatched inputs use the visible default refusal. This response uses no live model.",
                            "syntax": "symbolic", "surface": None})
                    self.version += 1
                    return
            transport = self.transport
            self.operation += 1
            operation = self.operation
            self.turn_base = revision
            self.running = "starting"
            self.start_pending = True
            self.cancel_pending = False
            self.buffered.clear()
            self.messages.clear()
            self.questions.clear()
            self.mode = "live Codex stdio"
            self.version += 1

        def run():
            try:
                turn = transport.start_turn(text, PROPOSAL_SCHEMA)
                with self.lock:
                    if operation != self.operation or transport is not self.transport:
                        return
                    cancelled = self.cancel_pending
                    self.start_pending = False
                    self.running = "interrupting" if cancelled else turn
                    buffered, self.buffered = self.buffered, []
                    if not cancelled:
                        for event in buffered:
                            self.notify(event)
                if cancelled:
                    transport.interrupt()
                    with self.lock:
                        if operation == self.operation:
                            self.running = None
            except (ValueError, OSError) as error:
                with self.lock:
                    if operation == self.operation:
                        self.error = str(error)
                        self.running = None
                        self.start_pending = False
                        self.buffered.clear()
            finally:
                with self.lock:
                    self.version += 1
        threading.Thread(target=run, daemon=True).start()

    def action(self, route, data):
        if route == "connect":
            self.connect()
            return self.view()
        if route == "discuss":
            self.discuss(data.get("message"), data.get("revision"), data.get("demo") is True,
                         data.get("trace_inputs"), data.get("cursor"))
            return self.view()
        if route == "interrupt":
            with self.lock:
                transport = self.transport
                if not self.running:
                    raise Refusal("There is no active turn to interrupt")
                self.cancel_pending = True
                self.running = "cancelling-start" if self.start_pending else "interrupting"
                self.messages.clear()
                self.questions.clear()
                self.error = "Turn interrupted; no partial output accepted"
                self.version += 1
                pending = self.start_pending
            if transport and not pending:
                try:
                    transport.interrupt()
                finally:
                    with self.lock:
                        self.running = None
            return self.view()
        if route == "steer":
            with self.domain():
                if not self.transport or self.start_pending or self.cancel_pending or not self.running or data.get("turn_id") != self.running:
                    raise Refusal("Steering refers to a stale turn")
                text = self.session.context(data.get("message"))
                transport = self.transport
            transport.steer(text, data["turn_id"])
            return self.view()
        if route == "answer":
            with self.lock:
                if not self.transport or not any(q["id"] == data["request_id"] for q in self.questions):
                    raise Refusal("There is no live question")
                self.transport.answer(data["request_id"], data["answers"])
                self.questions = [q for q in self.questions if q["id"] != data["request_id"]]
                self.version += 1
            return self.view()
        with self.domain():
            if route == "select":
                self.session.select(data["object_id"], data["revision"])
            elif route == "propose":
                self.session.propose_human(data["surface"], data["syntax"], data["revision"])
            elif route == "accept":
                if self.running:
                    raise Refusal("Interrupt the live turn before accepting a revision")
                self.session.accept_human(data["base_revision"], data["candidate_revision"])
            elif route == "reject":
                self.session.reject_human(data["base_revision"], data["candidate_revision"])
            elif route == "trace":
                if data["revision"] != self.session.model.revision:
                    raise Refusal("Trace uses a stale revision")
                return {"revision": self.session.model.revision, "trace": replay(self.session.model, data["inputs"])}
            elif route == "decision":
                if data["revision"] != self.session.model.revision:
                    raise Refusal("Decision uses a stale revision")
                model = self.session.model
                result = model.decide(data["state"], data["event"], data["context"])
                return {"revision": model.revision, "outcome": result,
                        "guards": [{"rule": r.id, "state_event_match": r.source == data["state"] and r.event == data["event"],
                                    "predicate": r.guard.render(True), "value": r.guard.value(data["state"], data["context"])} for r in model.rules]}
            elif route == "explain":
                if data["revision"] != self.session.model.revision:
                    raise Refusal("Explanation uses a stale revision")
                return explain(self.session.model, data["state"], data["event"], data["context"], checker_identity())
            elif route == "help":
                if data["revision"] != self.session.model.revision:
                    raise Refusal("Help uses a stale revision")
                return help_topic(self.session.model, data["topic"], checker_identity())
            elif route == "export":
                if self.session.agreement != "human-accepted":
                    raise Refusal("Explicitly accept a reviewed candidate before factory export")
                return {"revision": self.session.model.revision, "files": lower(self.session.model), "authority": "none"}
            else:
                raise Refusal("Unknown workspace action")
            self.error = None
            self.version += 1
        return self.view()


def serve(source: str, cwd: Path, port=8765, shared_path: Path | None = None):
    workspace = Workspace(source, cwd, shared_path)
    workspace.view()  # Refuse missing/invalid shared stores before listening.
    token = secrets.token_urlsafe(32)
    html = (Path(__file__).parent / "workspace.html").read_text().replace("__ZAL_CLIENT_TOKEN__", token)
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def reply(self, status, body, mime="application/json"):
            data = body.encode() if isinstance(body, str) else canonical(body).encode()
            self.send_response(status)
            self.send_header("Content-Type", mime + "; charset=utf-8")
            self.send_header("Content-Length", str(len(data)))
            self.send_header("Cache-Control", "no-store")
            self.send_header("X-Content-Type-Options", "nosniff")
            self.send_header("Content-Security-Policy", "default-src 'self'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; img-src 'self' data:; frame-ancestors 'none'")
            self.end_headers()
            self.wfile.write(data)

        def authorized(self):
            expected = f"127.0.0.1:{self.server.server_port}"
            return self.headers.get("Host") == expected and secrets.compare_digest(self.headers.get("X-ZAL-Client", ""), token) and self.headers.get("Origin", f"http://{expected}") == f"http://{expected}"

        def do_GET(self):
            if self.path == "/":
                self.reply(200, html, "text/html")
            elif self.path == "/favicon.ico":
                self.reply(204, "", "image/x-icon")
            elif self.path == "/api/state" and self.authorized():
                self.reply(200, workspace.view())
            else:
                self.reply(404, {"error": "Unknown or unauthorized route"})

        def do_POST(self):
            if not self.authorized():
                self.reply(403, {"error": "Local workspace capability required"})
                return
            try:
                length = int(self.headers.get("Content-Length", "0"))
                if not 0 < length <= 96 * 1024:
                    raise Refusal("Request exceeds the 96 KiB workspace bound")
                data = json.loads(self.rfile.read(length))
                if not isinstance(data, dict):
                    raise Refusal("Expected a workspace event object")
                route = urlsplit(self.path).path.removeprefix("/api/")
                self.reply(200, workspace.action(route, data))
            except (ValueError, KeyError, TypeError, OSError) as error:
                self.reply(400, {"error": str(error)[:2000]})

    httpd = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    print(f"ZAL companion: http://127.0.0.1:{httpd.server_port} (replay mode; Codex connects only on request)", flush=True)
    try:
        httpd.serve_forever()
    finally:
        httpd.server_close()
        if workspace.transport:
            workspace.transport.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Experimental ZAL behavior dialogue companion")
    parser.add_argument("--source", type=Path, default=Path(__file__).parent / "examples/order.zal")
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument("--workspace", type=Path, help="Use an existing terminal/MCP shared workspace")
    options = parser.parse_args()
    serve(options.source.read_text(), Path.cwd(), options.port, options.workspace)
