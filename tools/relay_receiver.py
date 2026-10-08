#!/usr/bin/env python3
"""Test receiver for the reference relay: an HTTP server that honours idempotency keys.

Every POST must carry an `Idempotency-Key` of 64 lowercase hexadecimal
digits. The first request with a key whose body is accepted is one effect;
a repeat with the same body is answered 200 and is no new effect, and a
repeat with a different body is refused with 409. The ledger, every effect
in order with its body and the number of requests per key, is written to
`--record FILE` atomically after each change and read back at start, so a
restarted receiver keeps it. `GET /` returns the ledger.

For failure tests: `--fail-first N` answers the first N requests 503 without
recording them, and `--hang-first N` makes the next N requests wait
`--hang-seconds` before answering, so a relay's call times out. With
`--hang-mode after` (the default) a hanging request records its effect
first, the case of a response lost after the effect; with `before` it
records nothing; with `drip` it records the effect and then sends its
answer a byte at a time, spread over `--hang-seconds`, so that no single
read waits long but the whole answer is slow. The server prints `listening http://HOST:PORT` once it
accepts connections, and stops on SIGTERM or SIGINT.

It uses only the Python standard library and is a test fixture, not a
production receiver.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import signal
import subprocess
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

KEY = re.compile(r"[0-9a-f]{64}")
SCHEMA = "zeno-fcis/relay-receiver-ledger/1"


class Ledger:
    """Effects by idempotency key, persisted atomically to one file."""

    def __init__(self, path: Path):
        self.path = path
        self.lock = threading.Lock()
        self.requests = 0
        if path.exists():
            data = json.loads(path.read_text())
            if data.get("schema") != SCHEMA:
                raise ValueError(f"{path} is not a receiver ledger")
            self.data = data
        else:
            self.data = {"schema": SCHEMA, "effects": [], "requests": {}, "conflicts": 0}
            self.save()

    def save(self) -> None:
        temporary = self.path.with_name(self.path.name + ".tmp")
        with open(temporary, "w") as handle:
            json.dump(self.data, handle, sort_keys=True)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, self.path)

    def count(self, key: str) -> int:
        """Counts a request for `key`; returns its index among all requests since start."""
        with self.lock:
            self.requests += 1
            self.data["requests"][key] = self.data["requests"].get(key, 0) + 1
            self.save()
            return self.requests - 1

    def record(self, key: str, body: bytes) -> int:
        """201 for a new effect, 200 for a repeat with the same body, 409 for another body."""
        digest = hashlib.sha256(body).hexdigest()
        with self.lock:
            for effect in self.data["effects"]:
                if effect["key"] == key:
                    if effect["body_sha256"] == digest:
                        return 200
                    self.data["conflicts"] += 1
                    self.save()
                    return 409
            self.data["effects"].append({"key": key, "body_sha256": digest,
                                         "body": body.decode("utf-8", "replace")})
            self.save()
            return 201


def handler(ledger: Ledger, options: argparse.Namespace) -> type[BaseHTTPRequestHandler]:
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, format: str, *args: object) -> None:  # noqa: A002
            if options.verbose:
                super().log_message(format, *args)

        def reply(self, status: int, document: dict) -> None:
            payload = json.dumps(document, sort_keys=True).encode()
            try:
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
            except (BrokenPipeError, ConnectionResetError):
                pass  # The relay gave up on this request; its ledger entry stands.

        def drip(self, status: int, document: dict) -> None:
            payload = json.dumps(document, sort_keys=True).encode()
            pause = options.hang_seconds / len(payload)
            try:
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.flush()
                for index in range(len(payload)):
                    time.sleep(pause)
                    self.wfile.write(payload[index:index + 1])
                    self.wfile.flush()
            except (BrokenPipeError, ConnectionResetError):
                pass  # The relay cut the call off; its ledger entry stands.

        def do_GET(self) -> None:  # noqa: N802
            with ledger.lock:
                document = json.loads(json.dumps(ledger.data))
            self.reply(200, document)

        def do_POST(self) -> None:  # noqa: N802
            key = self.headers.get("Idempotency-Key", "")
            length = self.headers.get("Content-Length", "")
            if not KEY.fullmatch(key) or not length.isdigit():
                self.reply(400, {"error": "an Idempotency-Key of 64 lowercase hexadecimal "
                                          "digits and a Content-Length are required"})
                return
            body = self.rfile.read(int(length))
            index = ledger.count(key)
            if index < options.fail_first:
                self.reply(503, {"error": "injected failure", "request": index})
                return
            hanging = index < options.fail_first + options.hang_first
            if hanging and options.hang_mode == "before":
                time.sleep(options.hang_seconds)
                self.reply(503, {"error": "injected timeout, nothing recorded"})
                return
            status = ledger.record(key, body)
            if hanging and options.hang_mode == "drip":
                self.drip(status, {"status": "recorded slowly", "key": key})
                return
            if hanging:
                time.sleep(options.hang_seconds)
            outcome = {200: "duplicate", 201: "recorded", 409: "conflict"}[status]
            self.reply(status, {"status": outcome, "key": key})

    return Handler


def arguments(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n", 1)[0])
    parser.add_argument("--record", type=Path, required=True, help="ledger file, kept across restarts")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=0, help="0 picks a free port (default)")
    parser.add_argument("--fail-first", type=int, default=0, metavar="N")
    parser.add_argument("--hang-first", type=int, default=0, metavar="N")
    parser.add_argument("--hang-seconds", type=float, default=5.0)
    parser.add_argument("--hang-mode", choices=("after", "before", "drip"), default="after")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args(argv)
    if args.fail_first < 0 or args.hang_first < 0 or args.hang_seconds < 0:
        parser.error("counts and waits must be non-negative")
    return args


class Running:
    """A receiver in its own process, for tests: `with Running(record, ...) as url:`."""

    def __init__(self, record: Path, *flags: str):
        self.command = [sys.executable, str(Path(__file__).resolve()), "--record", str(record),
                        *flags]
        self.process: subprocess.Popen[str] | None = None

    def __enter__(self) -> str:
        self.process = subprocess.Popen(self.command, text=True, stdout=subprocess.PIPE)
        assert self.process.stdout is not None
        line = self.process.stdout.readline().strip()
        if not line.startswith("listening "):
            self.process.kill()
            raise RuntimeError(f"receiver did not start: {line!r}")
        return line.split(" ", 1)[1] + "/deliveries"

    def __exit__(self, *_: object) -> None:
        assert self.process is not None
        self.process.terminate()
        try:
            self.process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()
        if self.process.stdout is not None:
            self.process.stdout.close()


def ledger(record: Path) -> dict:
    """The ledger a receiver wrote to `record`."""
    return json.loads(record.read_text())


def main(argv: list[str] | None = None) -> int:
    args = arguments(argv)
    ledger = Ledger(args.record)
    server = ThreadingHTTPServer((args.host, args.port), handler(ledger, args))
    server.daemon_threads = True

    def stop(_signal: int, _frame: object) -> None:
        threading.Thread(target=server.shutdown, daemon=True).start()

    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    host, port = server.server_address[:2]
    print(f"listening http://{host}:{port}", flush=True)
    try:
        server.serve_forever(poll_interval=0.1)
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
