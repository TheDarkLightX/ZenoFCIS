#!/usr/bin/env python3
"""Adversarial raw-socket HTTP checks against a live generated application.

The suite starts the generated `app-server` on a free 127.0.0.1 port with its
own store, credentials and test clock, sends each malformed or hostile request
over a raw socket, and stops the server afterwards. Every case names the
outcomes it permits: an HTTP status, or `closed` when the server closes the
connection without a response. After each case the server must still be
running, must not have panicked, and must answer `/state` with the same
revision and pending count. Only the final positive controls may commit.

It uses only the Python standard library and never connects anywhere but the
server it started.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import secrets
import select
import socket
import subprocess
import sys
import tempfile
import time

DEADLINE = 10.0  # The ingress deadline the generated server configures.
REFUSED = {"closed", "400"}


class Server:
    """The generated app-server, started on loopback with its own files."""

    def __init__(self, app: Path, binary: Path, root: Path):
        self.root = root
        self.app = app
        fixture = json.loads((app / "app-checks/journey.json").read_text())
        self.step = fixture["steps"][0]
        environment = {"PATH": os.environ.get("PATH", "/usr/bin:/bin")}
        self.credentials = {}
        if isinstance(fixture.get("parties"), list):
            parties = []
            for index, party in enumerate(fixture["parties"]):
                name = party["name"]
                self.credentials[name] = secrets.token_hex(32)
                (root / f"{name}.credential").write_text(self.credentials[name] + "\n")
                (root / f"{name}.context.json").write_text(json.dumps(party["context"]))
                entry = {"name": name, "credential": f"{name}.credential",
                         "principal": f"{index + 0x51:02x}" * 32, "context": f"{name}.context.json"}
                if party.get("view") is not None:
                    entry["view"] = party["view"]
                parties.append(entry)
            (root / "parties.json").write_text(json.dumps({"schema": "zeno-fcis/app-parties/1", "parties": parties}))
            environment["ZENO_APP_PARTIES"] = str(root / "parties.json")
            self.party = self.step["party"]
        else:
            self.party = "default"
            self.credentials["default"] = secrets.token_hex(32)
            (root / "credential").write_text(self.credentials["default"] + "\n")
            (root / "context.json").write_text(json.dumps(self.step["context"]))
            environment["ZENO_APP_SECRET_FILE"] = str(root / "credential")
            environment["ZENO_APP_CONTEXT"] = str(root / "context.json")
        (root / "clock").write_text(self.step.get("clock", "0"))
        environment["ZENO_APP_CLOCK_FILE"] = str(root / "clock")
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            self.port = probe.getsockname()[1]
        environment.update(ZENO_APP_STORE=str(root / "app.sqlite"), ZENO_APP_RECEIVER=str(root / "received"),
                           ZENO_APP_ADDRESS=f"127.0.0.1:{self.port}")
        self.log = (root / "server.log").open("w+")
        self.process = subprocess.Popen([str(binary)], cwd=root, env=environment, stdin=subprocess.DEVNULL,
                                        stdout=subprocess.PIPE, stderr=self.log, text=True)
        line = self.process.stdout.readline()
        if "Application available" not in line:
            self.stop()
            raise RuntimeError(f"server did not start: {line!r}; {self.stderr()[-2000:]}")

    def credential(self) -> str:
        return self.credentials[self.party]

    def stderr(self) -> str:
        self.log.flush()
        self.log.seek(0)
        return self.log.read()

    def stop(self) -> None:
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()


def exchange(port: int, parts, *, pause: float = 0.0, half_close: bool = True, limit: float = DEADLINE + 5):
    """Send each part, read until the server closes, and classify the reply."""
    started = time.monotonic()
    reply = b""
    with socket.create_connection(("127.0.0.1", port), timeout=limit) as stream:
        try:
            for index, part in enumerate(parts):
                # A slow client stops sending once the server has closed.
                if index and pause and select.select([stream], [], [], pause)[0]:
                    break
                stream.sendall(part)
            if half_close:
                stream.shutdown(socket.SHUT_WR)
        except OSError:
            pass  # The server closed first: a refusal before it read everything.
        try:
            while True:
                chunk = stream.recv(65536)
                if not chunk:
                    break
                reply += chunk
        except OSError as error:  # a reset ends the reply; a timeout is a hang
            if isinstance(error, socket.timeout):
                return "hang", reply, time.monotonic() - started
    elapsed = time.monotonic() - started
    # An interim 100 Continue precedes the final response.
    while reply.startswith(b"HTTP/1.1 100 ") and b"\r\n\r\n" in reply:
        reply = reply.split(b"\r\n\r\n", 1)[1]
    if not reply:
        return "closed", reply, elapsed
    status = reply.split(b"\r\n", 1)[0].split(b" ")
    return (status[1].decode("ascii", "replace") if len(status) > 1 else "garbled"), reply, elapsed


def payload(reply: bytes) -> bytes:
    return reply.split(b"\r\n\r\n", 1)[1] if b"\r\n\r\n" in reply else b""


def responses(reply: bytes) -> int:
    return reply.count(b"HTTP/1.1 ")


class Suite:
    def __init__(self, server: Server):
        self.server = server
        self.port = server.port
        self.host = f"127.0.0.1:{self.port}"
        self.rows = []
        state = self.state()
        self.contract = bytes.fromhex(state["contract"])
        self.revision = int(state["revision"])
        self.inputs = state["values"] + self.server.step["command"] + state["context"]
        self.preview_json = json.dumps(self.inputs).encode()
        status, reply, _ = exchange(self.port, [self.request("POST", "/preview", self.body(self.preview_json))])
        if status != "200":
            raise RuntimeError(f"baseline preview refused: {status} {payload(reply)[:200]!r}")
        preview = json.loads(payload(reply))
        if preview["reply"]["class"] != "Accept":
            raise RuntimeError("the fixture's first step must be accepted")
        self.command = bytes.fromhex(preview["command"])

    def request(self, method, target, body=b"", *, headers=None, length=True, credential=None):
        lines = [f"{method} {target} HTTP/1.1", f"Host: {self.host}",
                 f"Authorization: Bearer {credential or self.server.credential()}"]
        if length and (body or method == "POST"):
            lines.append(f"Content-Length: {len(body)}")
        lines += headers or []
        return ("\r\n".join(lines) + "\r\n\r\n").encode("latin-1") + body

    def body(self, payload_bytes, *, key=None, revision=None, contract=None, version=1):
        return (bytes([version]) + (self.contract if contract is None else contract) + (key or secrets.token_bytes(32))
                + (self.revision if revision is None else revision).to_bytes(8, "big") + payload_bytes)

    def state(self):
        status, reply, _ = exchange(self.port, [self.request("GET", "/state")])
        if status != "200":
            raise RuntimeError(f"/state refused: {status}")
        return json.loads(payload(reply))

    def case(self, name, expected, parts, *, pause=0.0, half_close=True, commits=0, one_response=False, slow=False):
        before = self.state()
        status, reply, elapsed = exchange(self.port, parts, pause=pause, half_close=half_close)
        alive = self.server.process.poll() is None
        after = self.state() if alive else None
        panicked = "panicked" in self.server.stderr()
        problems = []
        if status not in expected:
            problems.append(f"status {status} not in {sorted(expected)}")
        if status == "hang" or (not slow and elapsed > 3) or elapsed > DEADLINE + 3:
            problems.append(f"took {elapsed:.1f} s")
        if not alive or panicked:
            problems.append("server stopped or panicked")
        elif int(after["revision"]) != int(before["revision"]) + commits or (commits == 0 and after["pending"] != before["pending"]):
            problems.append(f"revision {before['revision']} -> {after['revision']}")
        if one_response and responses(reply) != 1:
            problems.append(f"{responses(reply)} responses")
        row = {"case": name, "expected": sorted(expected), "observed": status, "seconds": round(elapsed, 2),
               "revision": [before["revision"], after["revision"] if after else None],
               "body": payload(reply)[:40].decode("latin-1"), "result": "FAIL: " + "; ".join(problems) if problems else "pass"}
        self.rows.append(row)
        print(f"{row['result'][:4]:4}  {name:34} expected {'|'.join(row['expected']):24} observed {status:6} {elapsed:5.2f} s", flush=True)
        if not alive:
            raise RuntimeError("server stopped")
        return reply

    def run(self):
        r, b, c = self.request, self.body, self.case
        preview = b(self.preview_json)
        command = b(self.command)
        n = len(preview)
        head = f"POST /preview HTTP/1.1\r\nHost: {self.host}\r\nAuthorization: Bearer {self.server.credential()}\r\n"

        def raw(*lines, body=b""):
            return (head + "".join(line + "\r\n" for line in lines) + "\r\n").encode("latin-1") + body

        def chunked(data, size=None, extension=""):
            size = len(data) if size is None else size
            return f"{size:x}{extension}\r\n".encode() + data + b"\r\n0\r\n\r\n"

        # Message framing: Content-Length and Transfer-Encoding.
        c("length_and_chunked", REFUSED, [raw(f"Content-Length: {n}", "Transfer-Encoding: chunked", body=chunked(preview))])
        c("chunked_and_length", REFUSED, [raw("Transfer-Encoding: chunked", f"Content-Length: {n}", body=chunked(preview))])
        c("duplicate_equal_lengths", REFUSED, [raw(f"Content-Length: {n}", f"Content-Length: {n}", body=preview)])
        c("conflicting_lengths", REFUSED, [raw(f"Content-Length: {n}", f"Content-Length: {n + 1}", body=preview + b"x")])
        c("length_list_equal", REFUSED, [raw(f"Content-Length: {n}, {n}", body=preview)])
        c("length_plus_sign", REFUSED, [raw(f"Content-Length: +{n}", body=preview)])
        c("length_negative", REFUSED, [raw("Content-Length: -1", body=preview)])
        c("length_hexadecimal", REFUSED, [raw(f"Content-Length: 0x{n:x}", body=preview)])
        c("length_overflow", REFUSED, [raw("Content-Length: 99999999999999999999999", body=preview)])
        c("length_over_limit", {"413"}, [raw("Content-Length: 65537", body=b"\x01" * 65537)])
        c("length_longer_than_body", REFUSED, [raw(f"Content-Length: {n + 10}", body=preview)])
        c("transfer_gzip_chunked", REFUSED, [raw("Transfer-Encoding: gzip, chunked", body=chunked(preview))])
        c("transfer_identity", REFUSED, [raw("Transfer-Encoding: identity", body=preview)])
        c("transfer_chunked_twice", REFUSED, [raw("Transfer-Encoding: chunked", "Transfer-Encoding: chunked", body=chunked(preview))])
        c("transfer_chunked_list", REFUSED, [raw("Transfer-Encoding: chunked, chunked", body=chunked(chunked(preview)))])
        c("chunked_plain", {"200"}, [raw("Transfer-Encoding: chunked", body=chunked(preview))])
        c("chunk_extension_ignored", {"200"}, [raw("Transfer-Encoding: chunked", body=chunked(preview, extension=";name=value"))])
        c("chunk_size_overflow", REFUSED, [raw("Transfer-Encoding: chunked", body=b"fffffffffffffffffffff\r\n" + preview)])
        c("chunk_over_limit", {"413"}, [raw("Transfer-Encoding: chunked", body=chunked(b"\x01" * 65537))])
        c("chunked_truncated", REFUSED, [raw("Transfer-Encoding: chunked", body=b"40\r\n" + preview[:10])])
        c("chunked_unterminated", REFUSED, [raw("Transfer-Encoding: chunked", body=f"{n:x}\r\n".encode() + preview + b"\r\n")])
        c("chunked_large_trailer", REFUSED, [raw("Transfer-Encoding: chunked", body=f"{n:x}\r\n".encode() + preview + b"\r\n0\r\n" + b"X-T: " + b"a" * 8192 + b"\r\n\r\n")])
        c("chunked_one_byte_chunks", {"200"}, [raw("Transfer-Encoding: chunked", body=b"".join(b"1\r\n" + bytes([v]) + b"\r\n" for v in preview) + b"0\r\n\r\n")])
        # Header limits and header syntax.
        c("header_oversized", REFUSED | {"431"}, [raw("X-Large: " + "a" * 5000, f"Content-Length: {n}", body=preview)])
        c("headers_too_many", REFUSED | {"431"}, [raw(*[f"X-H{i}: {i}" for i in range(30)], f"Content-Length: {n}", body=preview)])
        c("header_obs_fold", REFUSED, [raw("X-Fold: a", " b", f"Content-Length: {n}", body=preview)])
        c("header_space_before_colon", REFUSED, [raw("X-Space : a", f"Content-Length: {n}", body=preview)])
        c("header_nul_byte", REFUSED, [raw("X-Nul: a\x00b", f"Content-Length: {n}", body=preview)])
        c("header_non_ascii_value", REFUSED, [raw("X-Note: caf\xe9", f"Content-Length: {n}", body=preview)])
        c("header_non_ascii_name", REFUSED, [raw("X-N\xe9: a", f"Content-Length: {n}", body=preview)])
        c("expect_continue", {"200"}, [raw(f"Content-Length: {n}", "Expect: 100-continue", body=preview)])
        c("expect_unknown", REFUSED | {"417", "200"}, [raw(f"Content-Length: {n}", "Expect: something", body=preview)])
        c("upgrade_h2c_ignored", {"200"}, [raw(f"Content-Length: {n}", "Connection: Upgrade, HTTP2-Settings", "Upgrade: h2c", "HTTP2-Settings: AAMAAABkAAQAAP__", body=preview)])
        c("upgrade_websocket_ignored", {"200"}, [raw(f"Content-Length: {n}", "Connection: Upgrade", "Upgrade: websocket", body=preview)])
        c("header_bare_lf", REFUSED, [(head.replace("\r\n", "\n") + f"Content-Length: {n}\n\n").encode() + preview])
        c("authorization_duplicate", {"400"}, [raw(f"Authorization: Bearer {self.server.credential()}", f"Content-Length: {n}", body=preview)])
        c("authorization_lowercase_scheme", {"401"}, [r("POST", "/preview", preview, credential=None).replace(b"Bearer ", b"bearer ")])
        c("authorization_uppercase_hex", {"401"}, [r("POST", "/preview", preview, credential=self.server.credential().upper())])
        c("authorization_double_space", {"401"}, [r("POST", "/preview", preview, credential=" " + self.server.credential())])
        c("authorization_non_ascii", {"400"}, [r("POST", "/preview", preview).replace(b"Bearer ", b"Bearer \xe9")])
        c("authorization_missing", {"401"}, [r("POST", "/preview", preview).replace(b"Authorization: Bearer", b"X-Authorization: Bearer")])
        c("origin_duplicate", {"400"}, [r("POST", "/preview", preview, headers=[f"Origin: http://{self.host}"] * 2)])
        c("origin_other", {"403"}, [r("POST", "/preview", preview, headers=["Origin: http://evil.example"])])
        c("origin_null", {"403"}, [r("POST", "/preview", preview, headers=["Origin: null"])])
        c("origin_trailing_slash", {"403"}, [r("POST", "/preview", preview, headers=[f"Origin: http://{self.host}/"])])
        c("fetch_site_cross_site", {"403"}, [r("POST", "/preview", preview, headers=["Sec-Fetch-Site: cross-site"])])
        c("host_missing", {"400"}, [r("POST", "/preview", preview).replace(f"Host: {self.host}\r\n".encode(), b"")])
        c("host_duplicate", {"400"}, [r("POST", "/preview", preview, headers=["Host: other"])])
        c("http10_without_host", REFUSED, [r("POST", "/preview", preview).replace(b"HTTP/1.1", b"HTTP/1.0").replace(f"Host: {self.host}\r\n".encode(), b"")])
        # Request targets and methods.
        c("absolute_form_target", {"400"}, [r("POST", f"http://{self.host}/preview", preview)])
        c("absolute_form_state", {"400"}, [r("GET", f"http://{self.host}/state")])
        c("authority_form_connect", REFUSED, [r("CONNECT", self.host)])
        c("asterisk_options", REFUSED, [r("OPTIONS", "*")])
        c("options_command", {"400"}, [r("OPTIONS", "/command")])
        c("head_state", REFUSED, [r("HEAD", "/state")])
        c("head_root", REFUSED, [r("HEAD", "/")])
        c("put_command", {"400"}, [r("PUT", "/command", command)])
        c("delete_state", {"400"}, [r("DELETE", "/state")])
        c("trace_root", {"400"}, [r("TRACE", "/")])
        c("get_preview", {"400"}, [r("GET", "/preview")])
        c("lowercase_method", REFUSED, [r("post", "/preview", preview)])
        c("query_on_state", {"400"}, [r("GET", "/state?x=1")])
        c("query_on_command", {"400"}, [r("POST", "/command?x=1", command)])
        c("dot_segment_asset", {"400"}, [r("GET", "/zenoui/../index.html")])
        c("percent_encoded_route", {"400"}, [r("GET", "/%73tate")])
        c("double_slash_route", {"400"}, [r("GET", "//state")])
        c("trailing_slash_route", {"400"}, [r("POST", "/preview/", preview)])
        c("long_request_line", REFUSED | {"414", "431"}, [r("GET", "/" + "a" * 8000)])
        c("http2_preface", REFUSED | {"505"}, [b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n"])
        c("http09_request", REFUSED | {"505"}, [b"GET /state\r\n\r\n"])
        c("state_with_body", {"400"}, [r("GET", "/state", b"x")])
        c("state_with_chunked", {"400"}, [r("GET", "/state", headers=["Transfer-Encoding: chunked"]) + b"0\r\n\r\n"])
        c("state_with_zero_padded_length", {"400"}, [r("GET", "/state", headers=["Content-Length: 00"])])
        # Pipelining: one request per connection; later bytes are never served.
        c("pipelined_state_then_command", {"200"}, [r("GET", "/state") + r("POST", "/command", command)], one_response=True)
        c("pipelined_preview_then_command", {"200"}, [r("POST", "/preview", preview) + r("POST", "/command", command)], one_response=True)
        c("length_shorter_than_body", {"200"}, [raw(f"Content-Length: {n}", body=preview + r("POST", "/command", command))], one_response=True)
        # Slow clients are cut off at the deadline.
        c("idle_connection", {"closed", "408"}, [b""], half_close=False, slow=True)
        c("slow_headers", {"closed", "408"}, [bytes([v]) for v in raw(f"Content-Length: {n}")[:14]], pause=1.0, half_close=False, slow=True)
        c("slow_body", {"closed", "408", "400"}, [raw(f"Content-Length: {n}")] + [bytes([v]) for v in preview[:14]], pause=1.0, half_close=False, slow=True)
        # Body layout: version, contract, key, revision, payload.
        c("body_empty", {"400"}, [r("POST", "/preview", b"")])
        c("body_without_payload", {"400"}, [r("POST", "/preview", b(b""))])
        c("body_version_zero", {"400"}, [r("POST", "/preview", b(self.preview_json, version=0))])
        c("body_version_two", {"400"}, [r("POST", "/preview", b(self.preview_json, version=2))])
        c("body_contract_truncated", {"400"}, [r("POST", "/preview", b"\x01" + self.contract[:20])])
        c("body_contract_other", {"403"}, [r("POST", "/preview", b(self.preview_json, contract=bytes(32)))])
        c("revision_max_preview", {"409"}, [r("POST", "/preview", b(self.preview_json, revision=2**64 - 1))])
        c("revision_max_command", {"409"}, [r("POST", "/command", b(self.command, revision=2**64 - 1))])
        c("revision_high_bit_command", {"409"}, [r("POST", "/command", b(self.command, revision=2**63))])
        c("preview_trailing_garbage", {"400"}, [r("POST", "/preview", b(self.preview_json + b"x"))])
        c("preview_trailing_json_space", {"200"}, [r("POST", "/preview", b(self.preview_json + b" \n"))])
        c("preview_object_payload", {"400"}, [r("POST", "/preview", b(b'{"0":"1"}'))])
        c("preview_deep_nesting", {"400"}, [r("POST", "/preview", b(b"[" * 5000 + b"]" * 5000))])
        c("preview_wrong_count", {"400"}, [r("POST", "/preview", b(json.dumps(self.inputs[:-1]).encode()))])
        c("preview_json_numbers", {"400"}, [r("POST", "/preview", b(json.dumps([int(v) for v in self.inputs]).encode()))])
        # A command field, an integer one when there is one, and a value just
        # outside its declared domain.
        fields = json.loads((self.server.app / "www/app.json").read_text())["description"]["fields"]
        commands = [f for f in fields if f["source"] == "command"]
        target = next((f for f in commands if f["domain"]["kind"] == "int"), commands[-1])
        at = fields.index(target)
        beyond = (int(target["domain"]["max"]) + 1 if target["domain"]["kind"] == "int"
                  else max(int(o["value"]) for o in target["options"]) + 1)
        for label, text in [("exponent", "1e0"), ("plus_sign", "+1"), ("leading_zero", "01"), ("negative_zero", "-0"),
                            ("space", " 1"), ("separator", "1,0"), ("decimal", "1.0"), ("arabic_digits", "١"),
                            ("fullwidth_digits", "１"), ("forty_one_digits", "1" * 41), ("i128_overflow", str(2**127)),
                            ("empty", "")]:
            changed = list(self.inputs)
            changed[at] = text
            c(f"preview_number_{label}", {"400"}, [r("POST", "/preview", b(json.dumps(changed).encode()))])
        # Another party's context or a different state is refused, never decided.
        status = self.state()
        changed = list(self.inputs)
        changed[0] = str(int(changed[0]) + 1)
        c("preview_other_state", {"403"}, [r("POST", "/preview", b(json.dumps(changed).encode()))])
        changed = list(self.inputs)
        changed[-len(status["context"])] = str(int(changed[-len(status["context"])]) + 1)
        c("preview_other_context", {"403"}, [r("POST", "/preview", b(json.dumps(changed).encode()))])
        # Command payloads are exact framed bytes.
        c("command_trailing_byte", {"400", "422"}, [r("POST", "/command", b(self.command + b"\x00"))])
        c("command_truncated", {"400", "422"}, [r("POST", "/command", b(self.command[:-1]))])
        c("command_preview_json", {"400", "422"}, [r("POST", "/command", b(self.preview_json))])
        outside = list(self.inputs)
        outside[at] = str(beyond)
        reply = self.case("preview_outside_domain", {"200"}, [r("POST", "/preview", b(json.dumps(outside).encode()))])
        decided = json.loads(payload(reply))
        if decided["reply"]["class"] != "Refused":
            self.rows[-1]["result"] = "FAIL: an input outside its domain was decided"
        c("command_outside_domain", {"422"}, [r("POST", "/command", b(bytes.fromhex(decided["command"])))])
        # Positive controls: the oracle sees a commit, and an exact replay returns it.
        key = secrets.token_bytes(32)
        first = c("command_accepted_control", {"200"}, [r("POST", "/command", b(self.command, key=key))], commits=1)
        again = c("command_replay_control", {"200"}, [r("POST", "/command", b(self.command, key=key))])
        if payload(first) != payload(again):
            self.rows[-1]["result"] = "FAIL: replay changed the packet"
        c("command_replay_changed", {"409"}, [r("POST", "/command", b(self.command + b"\x00", key=key))])
        return self.rows


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=Path, required=True, help="generated application directory")
    parser.add_argument("--server", type=Path, required=True, help="its built app-server binary")
    parser.add_argument("--out", type=Path, help="write the case table as JSON")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="http-wire-") as directory:
        server = Server(args.app.resolve(), args.server.resolve(), Path(directory))
        try:
            rows = Suite(server).run()
        finally:
            server.stop()
            log = server.stderr()
    failed = [row for row in rows if row["result"] != "pass"]
    if "panicked" in log:
        failed.append({"case": "server log", "result": "FAIL: panicked"})
    if args.out:
        args.out.write_text(json.dumps({"format": "zeno-fcis/http-wire-check/1", "cases": rows, "failed": len(failed)}, indent=2) + "\n")
    for row in failed:
        print(f"{row['case']}: {row['result']}", file=sys.stderr)
    print(f"http wire: {'PASS' if not failed else 'FAIL'}; {len(rows)} cases, {len(failed)} failed", flush=True)
    return 1 if failed else 0


if __name__ == "__main__":
    from telemetry import run_cli
    def _entrypoint():
        raise SystemExit(main())
    run_cli(_entrypoint)
