#!/usr/bin/env python3
"""Reference relay: deliver a store's pending entries to an external system.

The relay uses only the Python standard library. It talks to the store
through two commands of the application that owns it:

- the export command prints the store's pending deliveries, one line of JSON
  per delivery in commit order (`zeno-fcis/relay-export/1`, see
  `zeno_fcis_shell_sqlite::v2::relay`), and writes nothing;
- the acknowledgment command, run with a delivery ID and the SHA-256 of the
  payload the relay sent appended, marks that delivery acknowledged. The
  shell refuses an unknown ID, a payload hash other than the stored
  payload's, and a delivery already acknowledged, each by name and with no
  change to any delivery.

For each exported delivery, in order, the relay performs one outbound call:
an HTTP POST to `--http URL`, or an appended line in the local file
`--queue FILE`. The body is the same bytes on every attempt, and the HTTP
call carries the delivery ID as its `Idempotency-Key`. Only a 2xx answer to
the POST itself counts as sent: the relay follows no redirect, and a 3xx
answer is a refusal, because following one would send a request without the
body. `--timeout` limits the whole call, from connecting to the last byte
of the answer, not each socket operation; a call still unfinished at that
point is cut off and counts as timed out. The relay connects directly to
the URL's host and does not read proxy settings from the environment. A failed or timed-out call is
retried with bounded exponential backoff; after the last attempt the
delivery stays pending, the relay stops (later deliveries keep their order)
and exits 3. After a successful call the relay acknowledges the delivery.

Strength: transport is at least once. The relay keeps no state of its own:
the store is the durable stream, and a relay that stops at any point, before
its call, after it or before the acknowledgment, sends the delivery again
on its next run, with the same ID and the same body. A receiver that honours
the idempotency key sees each effect once; one that does not may act twice.
Lines appended to a queue file may repeat in the same way. A short append
is cut back to the file's size before it, and an append to a file that does
not end in a newline (left by a process stopped part-way through a write)
starts with one, so every complete line stands alone; a consumer must keep
the first line per `delivery_id` and skip any line that is not complete
JSON. An
acknowledgment records that the relay's call succeeded, not that the
receiver acted on it.

`--crash-at POINT[:N]` ends the process abruptly the Nth time it reaches
POINT (`after-export`, `after-send` or `after-acknowledge`), for restart
tests. Exit codes: 0 every exported delivery acknowledged, 2 usage or a
malformed export, 3 a delivery left pending after its last attempt or a
receiver refused it, 4 the export command failed or the store refused an
acknowledgment (a store command still running after `--command-timeout`
seconds is killed and counts as failed), 75 an injected crash.
"""

from __future__ import annotations

import argparse
import hashlib
import http.client
import json
import math
import os
import re
import shlex
import socket
import subprocess
import sys
import threading
import time
import urllib.parse
from pathlib import Path

EXPORT_SCHEMA = "zeno-fcis/relay-export/1"
BODY_SCHEMA = "zeno-fcis/relay-delivery/1"
CRASH_POINTS = ("after-export", "after-send", "after-acknowledge")
CRASH_EXIT = 75
HASH = re.compile(r"[0-9a-f]{64}")
HEX = re.compile(r"(?:[0-9a-f]{2})*")
FIELDS = {"schema": str, "commit": int, "lane": int, "ordinal": int, "delivery_id": str,
          "channel": int, "destination_root": int, "payload_root": int, "destination": str,
          "payload": str, "payload_sha256": str, "entry_hash": str}


class ExportError(ValueError):
    """The export command's output is not a valid relay export."""


def parse_export(text: str) -> list[dict]:
    """The deliveries of an export, in order, each checked field by field;
    a payload whose SHA-256 differs from the line's `payload_sha256` is refused."""
    deliveries = []
    seen = set()
    for number, line in enumerate(text.splitlines(), 1):
        if not line.strip():
            continue
        try:
            record = json.loads(line)
        except json.JSONDecodeError as error:
            raise ExportError(f"line {number} is not JSON: {error}") from error
        if not isinstance(record, dict) or set(record) != set(FIELDS):
            raise ExportError(f"line {number} does not have exactly the export fields")
        for name, kind in FIELDS.items():
            if type(record[name]) is not kind:
                raise ExportError(f"line {number}: {name} is not a {kind.__name__}")
        if record["schema"] != EXPORT_SCHEMA:
            raise ExportError(f"line {number}: schema {record['schema']!r}, expected {EXPORT_SCHEMA}")
        for name in ("delivery_id", "payload_sha256", "entry_hash"):
            if not HASH.fullmatch(record[name]):
                raise ExportError(f"line {number}: {name} is not 64 lowercase hexadecimal digits")
        for name in ("destination", "payload"):
            if not HEX.fullmatch(record[name]):
                raise ExportError(f"line {number}: {name} is not lowercase hexadecimal")
        if hashlib.sha256(bytes.fromhex(record["payload"])).hexdigest() != record["payload_sha256"]:
            raise ExportError(f"line {number}: the payload's SHA-256 is not payload_sha256")
        if record["delivery_id"] in seen:
            raise ExportError(f"line {number}: delivery {record['delivery_id']} appears twice")
        seen.add(record["delivery_id"])
        deliveries.append(record)
    order = [(d["commit"], d["lane"], d["ordinal"]) for d in deliveries]
    if order != sorted(order):
        raise ExportError("the export is not in commit order")
    return deliveries


def body(delivery: dict) -> bytes:
    """The bytes every attempt for this delivery sends: a function of the
    exported line alone."""
    document = {key: delivery[key] for key in FIELDS if key != "schema"}
    document["schema"] = BODY_SCHEMA
    return (json.dumps(document, sort_keys=True, separators=(",", ":")) + "\n").encode()


def sent_payload_sha256(sent: bytes) -> str:
    """The SHA-256 of the payload inside a sent body, which the acknowledgment carries."""
    return hashlib.sha256(bytes.fromhex(json.loads(sent)["payload"])).hexdigest()


class Crashes:
    """`--crash-at POINT[:N]`: end the process the Nth time POINT is reached."""

    def __init__(self, spec: str | None):
        self.point, self.remaining = None, 0
        if spec:
            point, _, count = spec.partition(":")
            if point not in CRASH_POINTS or (count and not count.isdigit()) or count == "0":
                raise argparse.ArgumentTypeError(f"--crash-at {spec!r}: expected POINT[:N], "
                                                 f"POINT one of {', '.join(CRASH_POINTS)}")
            self.point, self.remaining = point, int(count or 1)

    def reach(self, point: str) -> None:
        if point == self.point:
            self.remaining -= 1
            if self.remaining == 0:
                sys.stderr.write(f"relay: injected crash at {point}\n")
                sys.stderr.flush()
                os._exit(CRASH_EXIT)


class Refused(Exception):
    """The receiver refused the delivery for good; retrying cannot help."""


class Deadline:
    """A limit on a whole HTTP call, not on each socket operation.

    A socket timeout bounds one blocking connect, send or receive, so a
    receiver that answers a byte at a time could hold a call open for any
    length of time. When the deadline passes, a timer shuts the
    connection's socket down, which ends whatever operation is blocked on
    it; the call then counts as timed out even if it would have succeeded."""

    def __init__(self, seconds: float):
        self.sock: socket.socket | None = None
        self.passed = threading.Event()
        self.timer = threading.Timer(seconds, self.expire)
        self.timer.daemon = True

    def attach(self, sock: socket.socket) -> None:
        """Watch `sock`, kept here because the connection hands its socket
        to the response, and drops its own reference, once the answer's
        headers say the connection closes after it."""
        self.sock = sock
        self.check()  # The timer may have fired before the socket existed.

    def expire(self) -> None:
        self.passed.set()
        if self.sock is not None:
            try:
                self.sock.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass

    def check(self) -> None:
        if self.passed.is_set():
            raise TimeoutError("timed out: the call exceeded --timeout")


def post(url: str, delivery: dict, data: bytes, timeout: float) -> None:
    """One POST of `data`, bounded as a whole by `timeout` seconds. The
    relay follows no redirect and uses no proxy: a 3xx answer is a refusal,
    since following it would send a request without the body."""
    parts = urllib.parse.urlsplit(url)
    connection_class = (http.client.HTTPSConnection if parts.scheme == "https"
                        else http.client.HTTPConnection)
    connection = connection_class(parts.hostname, parts.port, timeout=timeout)
    deadline = Deadline(timeout)
    deadline.timer.start()
    try:
        try:
            connection.connect()
            deadline.attach(connection.sock)
            connection.request("POST", urllib.parse.urlunsplit(("", "", parts.path or "/",
                                                                 parts.query, "")),
                               body=data, headers={
                "Content-Type": "application/json",
                "Idempotency-Key": delivery["delivery_id"],
                "X-Zeno-Payload-SHA256": delivery["payload_sha256"],
            })
            response = connection.getresponse()
            answer = response.read()
        except (OSError, http.client.HTTPException) as error:
            deadline.check()
            raise error
        # An answer that arrived as the deadline passed still counts as timed
        # out: the delivery stays pending and is sent again, which at least
        # once allows.
        deadline.check()
    finally:
        deadline.timer.cancel()
        connection.close()
    status = response.status
    if 200 <= status < 300:
        return
    # 408, 429 and 5xx are transient; any other answer, a redirect included,
    # is a final refusal.
    if status in (408, 429) or status >= 500:
        raise OSError(f"HTTP {status}")
    raise Refused(f"HTTP {status}: {answer[:200]!r}")


def http_url(text: str) -> str:
    parts = urllib.parse.urlsplit(text)
    if parts.scheme not in ("http", "https") or not parts.hostname:
        raise argparse.ArgumentTypeError(f"--http {text!r}: expected an http:// or https:// URL")
    try:
        parts.port
    except ValueError as error:
        raise argparse.ArgumentTypeError(f"--http {text!r}: {error}") from error
    return text


def append(queue: Path, data: bytes) -> None:
    """Append `data` as whole lines: a short write is cut back to the size
    before it, and a file whose last byte is not a newline (a write stopped
    part-way) gets one first, so no line runs into another."""
    descriptor = os.open(queue, os.O_RDWR | os.O_APPEND | os.O_CREAT, 0o644)
    try:
        size = os.fstat(descriptor).st_size
        if size and os.pread(descriptor, 1, size - 1) != b"\n":
            data = b"\n" + data
        written = os.write(descriptor, data)
        if written != len(data):
            os.ftruncate(descriptor, size)
            raise OSError(f"short append to {queue}: {written} of {len(data)} bytes")
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def send(args: argparse.Namespace, delivery: dict, data: bytes, log: dict) -> str | None:
    """One delivery with bounded retries: None once sent, or why it stays
    pending. A final refusal by the receiver raises `Refused`."""
    reason = "no attempt"
    for attempt in range(args.attempts):
        if attempt:
            time.sleep(min(args.max_backoff, args.backoff * 2 ** (attempt - 1)))
        log["attempts"][delivery["delivery_id"]] = log["attempts"].get(delivery["delivery_id"], 0) + 1
        try:
            if args.http:
                post(args.http, delivery, data, args.timeout)
            else:
                append(args.queue, data)
            return None
        except (OSError, http.client.HTTPException) as error:
            reason = f"{type(error).__name__}: {error}"
    return reason


def run(command: list[str], timeout: float, *extra: str) -> subprocess.CompletedProcess[str]:
    """A store command; one still running after `timeout` seconds is killed
    and reported as failed, so a hung store cannot hold the relay forever."""
    try:
        return subprocess.run([*command, *extra], check=False, text=True, timeout=timeout,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    except subprocess.TimeoutExpired:
        return subprocess.CompletedProcess([*command, *extra], -1, "",
                                           f"timed out: the command ran longer than {timeout} s")


def relay(args: argparse.Namespace) -> tuple[int, dict]:
    crashes = Crashes(args.crash_at)
    log: dict = {"schema": "zeno-fcis/relay-run/1", "status": "drained", "exported": [],
                 "acknowledged": [], "attempts": {}}
    exported = run(args.export_command, args.command_timeout)
    if exported.returncode != 0:
        log.update(status="export-failed", error=exported.stderr.strip())
        return 4, log
    try:
        deliveries = parse_export(exported.stdout)
    except ExportError as error:
        log.update(status="malformed-export", error=str(error))
        return 2, log
    log["exported"] = [d["delivery_id"] for d in deliveries]
    crashes.reach("after-export")
    for delivery in deliveries:
        data = body(delivery)
        try:
            pending = send(args, delivery, data, log)
        except Refused as error:
            log.update(status="refused", delivery_id=delivery["delivery_id"], error=str(error))
            return 3, log
        if pending is not None:
            log.update(status="pending", delivery_id=delivery["delivery_id"], error=pending)
            return 3, log
        crashes.reach("after-send")
        acknowledged = run(args.ack_command, args.command_timeout, delivery["delivery_id"],
                           sent_payload_sha256(data))
        if acknowledged.returncode != 0:
            # Another relay acknowledged it first: it is no longer pending.
            if "AlreadyAcknowledged" not in acknowledged.stderr:
                log.update(status="acknowledgment-refused", delivery_id=delivery["delivery_id"],
                           error=acknowledged.stderr.strip())
                return 4, log
        log["acknowledged"].append(delivery["delivery_id"])
        crashes.reach("after-acknowledge")
    return 0, log


def arguments(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n", 1)[0])
    parser.add_argument("--export-command", required=True, type=shlex.split,
                        help="command that prints the store's relay export")
    parser.add_argument("--ack-command", required=True, type=shlex.split,
                        help="command that acknowledges DELIVERY_ID PAYLOAD_SHA256, appended")
    target = parser.add_mutually_exclusive_group(required=True)
    target.add_argument("--http", metavar="URL", type=http_url,
                        help="POST each delivery to this http:// or https:// URL")
    target.add_argument("--queue", metavar="FILE", type=Path,
                        help="append each delivery as a line to this file")
    parser.add_argument("--attempts", type=int, default=5, help="attempts per delivery (default 5)")
    parser.add_argument("--backoff", type=float, default=0.5,
                        help="seconds before the second attempt, doubled each time (default 0.5)")
    parser.add_argument("--max-backoff", type=float, default=8.0,
                        help="longest wait between attempts, in seconds (default 8)")
    parser.add_argument("--timeout", type=float, default=10.0,
                        help="seconds a whole HTTP call may take, from connecting to the last byte "
                             "of the answer (default 10)")
    parser.add_argument("--command-timeout", type=float, default=120.0,
                        help="seconds the export or an acknowledgment command may run before it "
                             "is killed and counts as failed (default 120)")
    parser.add_argument("--crash-at", metavar="POINT[:N]", help="for tests: end abruptly at POINT")
    args = parser.parse_args(argv)
    if args.attempts < 1 or args.backoff < 0 or args.max_backoff < 0 or args.timeout <= 0 \
            or args.command_timeout <= 0:
        parser.error("--attempts must be at least 1, the waits non-negative, the timeouts positive")
    if not args.export_command or not args.ack_command:
        parser.error("the export and acknowledgment commands must not be empty")
    try:
        Crashes(args.crash_at)
    except argparse.ArgumentTypeError as error:
        parser.error(str(error))
    return args


def destination_settings(path: Path) -> argparse.Namespace:
    """Closed, bounded settings for one generated application's destination."""
    with path.open("rb") as source:
        encoded = source.read(65537)
    if len(encoded) > 65536:
        raise ValueError("relay configuration exceeds 64 KiB")
    def unique(pairs: list[tuple[str, object]]) -> dict:
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError(f"duplicate relay configuration key: {key}")
            result[key] = value
        return result
    config = json.loads(encoded, object_pairs_hook=unique)
    allowed = {"schema", "http", "queue", "attempts", "backoff", "max_backoff", "timeout", "crash_at"}
    if not isinstance(config, dict) or set(config) - allowed \
            or config.get("schema") != "zeno-fcis/relay-destination/1":
        raise ValueError("expected a closed zeno-fcis/relay-destination/1 configuration")
    if ("http" in config) == ("queue" in config):
        raise ValueError("relay configuration needs exactly one of http or queue")
    for name in ("http", "queue"):
        if name in config and (not isinstance(config[name], str) or not config[name]
                               or len(config[name]) > 4096):
            raise ValueError(f"relay {name} must be a nonempty string of at most 4096 characters")
    limits = {"attempts": (1, 8, 5), "backoff": (0, 8, 0.5),
              "max_backoff": (0, 8, 8.0), "timeout": (0.001, 30, 10.0)}
    for name, (minimum, maximum, default) in limits.items():
        value = config.setdefault(name, default)
        if type(value) not in (int, float) or not math.isfinite(value) \
                or not minimum <= value <= maximum:
            raise ValueError(f"relay {name} must be finite and in {minimum}..{maximum}")
    if type(config["attempts"]) is not int:
        raise ValueError("relay attempts must be an integer")
    if "http" in config:
        config["http"] = http_url(config["http"])
    if "queue" in config:
        queue = Path(config["queue"])
        config["queue"] = queue if queue.is_absolute() else path.resolve().parent / queue
    crash = config.get("crash_at")
    if crash not in (None, "after-export", "after-send"):
        raise ValueError("crash_at supports only after-export or after-send")
    return argparse.Namespace(http=config.get("http"), queue=config.get("queue"),
                              crash_at=crash, **{name: config[name] for name in limits})


def send_one(path: Path) -> int:
    """Send one reviewed export from stdin; acknowledgment belongs to Rust."""
    try:
        args = destination_settings(path)
        data = sys.stdin.buffer.read(1048577)
        if len(data) > 1048576:
            raise ExportError("one exported delivery exceeds 1 MiB")
        deliveries = parse_export(data.decode("utf-8"))
        if len(deliveries) != 1:
            raise ExportError("send-one requires exactly one exported delivery")
        crashes = Crashes(args.crash_at)
        crashes.reach("after-export")
        delivery = deliveries[0]
        pending = send(args, delivery, body(delivery), {"attempts": {}})
        if pending is not None:
            raise OSError(pending)
        crashes.reach("after-send")
        return 0
    except (OSError, ValueError, Refused, argparse.ArgumentTypeError) as error:
        print(f"relay destination: {error}", file=sys.stderr)
        return 3


def main(argv: list[str] | None = None) -> int:
    argv = sys.argv[1:] if argv is None else argv
    if argv and argv[0] == "--send-one":
        if len(argv) != 2:
            print("relay --send-one needs one configuration path", file=sys.stderr)
            return 2
        return send_one(Path(argv[1]))
    code, log = relay(arguments(argv))
    print(json.dumps(log, sort_keys=True))
    return code


if __name__ == "__main__":
    sys.exit(main())
