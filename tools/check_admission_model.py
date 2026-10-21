#!/usr/bin/env python3
"""Check the ingress admission rules as a finite contract, then against a live server.

test-data/request-admission states the rules of crates/zeno-fcis-ingress
src/http.rs as a contract over nine request features (129,600 inputs) with
five laws: one framing; one Host, own origin and origin-form target; a
valid credential except for public assets; bodies by method; no state.

1. `contract review` decides every input with the library Authority; no law
   may refuse a decision. The review also compares a decision example for
   every input, written by `reference()` below, so this file's reading of the
   rules is the Authority's on the whole domain.
2. With --app and --server, every input becomes one concrete request to the
   generated application, started on a free 127.0.0.1 port by
   check_http_wire.Server and stopped afterwards. The observed status must be
   the one the decision names: closed, 400, 401, 403 or 413 for a refusal;
   200 for an admitted read or preview, and 409 for an admitted command,
   whose revision is always stale so that nothing commits.

Byte-level parsing stays with check_http_wire.py and the fuzz targets; this
covers the decision rules over the abstraction, not every byte form.
"""
from __future__ import annotations

import argparse
import itertools
import json
from pathlib import Path
import secrets
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
CONTRACT = ROOT / "test-data/request-admission"
sys.path.insert(0, str(ROOT / "tools"))

FIELDS = {  # command field values, in program order
    "method": (150, 151, 152), "route": (155, 156, 157, 158, 159), "target": (160, 161, 162),
    "hosts": (0, 1, 2), "origin": (165, 166, 167, 168), "fetch": (170, 171),
    "credential": (175, 176, 177, 178), "framing": (180, 181, 182, 183, 184), "body": (185, 186, 187, 188, 189, 190),
}
STATUS = {200: "closed", 201: "400", 202: "401", 203: "403", 204: "413", 205: "403"}


def reference(r):
    """The admission decision for one feature tuple: None to admit, else the reason."""
    malformed = r["target"] != 160 or r["hosts"] != 1 or r["origin"] == 168 or r["framing"] == 184
    foreign = r["origin"] == 167 or r["fetch"] == 171
    has_body = r["framing"] in (182, 184) or (r["framing"] == 181 and r["body"] != 185)
    if r["framing"] == 183:
        return 200
    if r["method"] == 150 and r["route"] != 155:
        return 201 if malformed or foreign or has_body or r["route"] != 158 else None
    if r["method"] == 150:
        return 201 if malformed else 203 if foreign else 202 if r["credential"] != 178 else 201 if has_body else None
    if r["method"] != 151 or r["route"] not in (156, 157) or r["target"] != 160:
        return 201
    if malformed:
        return 201
    if foreign:
        return 203
    if r["credential"] != 178:
        return 202
    if r["framing"] == 180:
        return 201
    return {190: 204, 185: 201, 186: 201, 187: 201, 188: 205}.get(r["body"])


def domain():
    names = list(FIELDS)
    for values in itertools.product(*FIELDS.values()):
        yield dict(zip(names, values))


def review(cli: Path, out: Path) -> list[str]:
    """Run contract review with a decision example for every input."""
    with tempfile.TemporaryDirectory(prefix="admission-review-") as directory:
        copy = Path(directory) / "request-admission"
        shutil.copytree(CONTRACT, copy)
        (copy / "tests").mkdir(exist_ok=True)
        lines = []
        for r in domain():
            reason = reference(r)
            decision = "accept - 0" if reason is None else f"reject {reason} 0"
            lines.append(f"0 | {' '.join(map(str, r.values()))} | 195 | {decision} | -")
        (copy / "tests/decision-examples.txt").write_text("\n".join(lines) + "\n")
        result = subprocess.run([str(cli), "contract", "review", str(copy), "--no-kb", "--out", str(out / "admission-review.json")],
                                capture_output=True, text=True, timeout=3600)
    (out / "admission-review.log").write_text(result.stdout + result.stderr)
    text = result.stdout + result.stderr
    expected = f"{len(lines)} examples compared, 0 disagreements"
    failures = [] if result.returncode == 0 and expected in text and "0 law refusals" in text else [
        f"review: exit {result.returncode}; expected '{expected}' and '0 law refusals'"]
    return failures


def concrete(r, server, port, contract, preview, command):
    """One request with these features, for the generated application."""
    host = f"127.0.0.1:{port}"
    path = {155: "/state", 156: "/preview", 157: "/command", 158: "/", 159: "/nope"}[r["route"]]
    target = {160: path, 161: path + "?x=1", 162: f"http://{host}{path}"}[r["target"]]
    method = {150: "GET", 151: "POST", 152: "PUT"}[r["method"]]
    head = [f"{method} {target} HTTP/1.1"] + [f"Host: {host}"] * r["hosts"]
    head += {165: [], 166: [f"Origin: http://{host}"], 167: ["Origin: http://evil.example"],
             168: [f"Origin: http://{host}"] * 2}[r["origin"]]
    head += ["Sec-Fetch-Site: cross-site"] if r["fetch"] == 171 else []
    head += {175: [], 176: ["Authorization: Bearer xyz"], 177: [f"Authorization: Bearer {secrets.token_hex(32)}"],
             178: [f"Authorization: Bearer {server.credential()}"]}[r["credential"]]
    layout = lambda version, digest, revision, data: bytes([version]) + digest + secrets.token_bytes(32) + revision.to_bytes(8, "big") + data
    fits = command if r["route"] == 157 else preview
    body = {185: b"", 186: b"\x01" * 40, 187: layout(2, contract, 0, preview[73:]), 188: layout(1, bytes(32), 0, preview[73:]),
            189: fits, 190: b"\x01" * 65537}[r["body"]]
    chunked = (f"{len(body):x}\r\n".encode() + body + b"\r\n" if body else b"") + b"0\r\n\r\n"
    framing, data = {180: ([], b""), 181: ([f"Content-Length: {len(body)}"], body),
                     182: (["Transfer-Encoding: chunked"], chunked),
                     183: ([f"Content-Length: {len(chunked)}", "Transfer-Encoding: chunked"], chunked),
                     184: (["Transfer-Encoding: gzip, chunked"], chunked)}[r["framing"]]
    return ("\r\n".join(head + framing) + "\r\n\r\n").encode("latin-1") + data


def differential(app: Path, binary: Path, out: Path) -> list[str]:
    from check_http_wire import Server, Suite, exchange
    failures, counts = [], {}
    with tempfile.TemporaryDirectory(prefix="admission-http-") as directory:
        server = Server(app, binary, Path(directory))
        try:
            suite = Suite(server)
            contract, revision = suite.contract, suite.revision
            preview = suite.body(suite.preview_json)
            command = suite.body(suite.command, revision=2**64 - 1)
            for r in domain():
                reason = reference(r)
                expected = STATUS[reason] if reason is not None else "409" if r["route"] == 157 and r["method"] == 151 else "200"
                status, _, _ = exchange(server.port, [concrete(r, server, server.port, contract, preview, command)], limit=15)
                counts[expected] = counts.get(expected, 0) + 1
                if status != expected:
                    failures.append(f"{json.dumps(r)}: expected {expected}, observed {status}")
                    if len(failures) >= 50:
                        break
            after = suite.state()
            if int(after["revision"]) != revision or "panicked" in server.stderr():
                failures.append(f"the server changed state or panicked: revision {revision} -> {after['revision']}")
        finally:
            server.stop()
    (out / "admission-http.json").write_text(json.dumps({"format": "zeno-fcis/admission-differential/1", "expected": counts,
                                                         "failures": failures[:50]}, indent=2) + "\n")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--app", type=Path, help="generated application directory")
    parser.add_argument("--server", type=Path, help="its built app-server binary")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    failures = review(args.cli.resolve(), args.out)
    if args.app and args.server and not failures:
        failures += differential(args.app.resolve(), args.server.resolve(), args.out)
    for line in failures[:50]:
        print(line, file=sys.stderr)
    size = len(list(domain()))
    print(f"admission model: {'PASS' if not failures else 'FAIL'}; {size} inputs reviewed"
          + (", each sent to the live server" if args.app else ""), flush=True)
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
