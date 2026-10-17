#!/usr/bin/env python3
"""Loopback test adapter for a generated Spot SQLite application.

Python's http.server is not a production ingress server. This adapter provides
local fake-value integration evidence only.
"""
from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
from http.server import BaseHTTPRequestHandler, HTTPServer
import io
import json
from pathlib import Path
import re
import secrets
import subprocess

from native_check import strict_json
from spot_reference import Command, command_values

MAX_BODY = 8192
FIELDS = frozenset(('action', 'direction', 'a', 'b', 'limit0', 'limit1', 'sqrt', 'recipient'))


@dataclass(frozen=True)
class Identity:
    name: str
    slot: int
    token: str


@dataclass(frozen=True)
class Binding:
    """Owned transport identity configuration; contains no economic state."""
    identities: tuple[Identity, ...]

    def __post_init__(self):
        if (type(self.identities) is not tuple or len(self.identities) != 2
                or any(type(i) is not Identity for i in self.identities)
                or {i.slot for i in self.identities} != {0, 1}
                or len({i.name for i in self.identities}) != 2
                or len({i.token for i in self.identities}) != 2
                or any(type(i.name) is not str or type(i.token) is not str
                       or type(i.slot) is not int or not i.name or not 24 <= len(i.token) <= 249
                       or not i.token.isascii() for i in self.identities)):
            raise ValueError('two owned ordinary identity bindings with distinct credentials required')

    def actor(self, authorization):
        if type(authorization) is not str or not authorization.isascii() or len(authorization) > 256:
            raise PermissionError('authenticated session required')
        for identity in self.identities:
            if secrets.compare_digest(authorization, 'Bearer ' + identity.token):
                return identity
        raise PermissionError('authenticated session required')

    def command(self, identity, body):
        if type(body) is not dict or set(body) - FIELDS or 'action' not in body:
            raise ValueError('closed command required; body-supplied actor and role fields are forbidden')
        recipient = body.get('recipient', identity.name)
        target = next((i for i in self.identities if i.name == recipient), None)
        if target is None:
            raise ValueError('recipient requires an ordinary identity binding')
        numeric = {name: value for name, value in body.items() if name != 'recipient'}
        if any(type(value) is not int for value in numeric.values()):
            raise ValueError('exact integers required')
        return Command(**numeric, actor=identity.slot, recipient=target.slot)


@dataclass(frozen=True)
class Mount:
    binary: Path
    database: Path
    binding: Binding

    def submit(self, authorization, retry, body):
        identity = self.binding.actor(authorization)
        if not re.fullmatch('[0-9a-f]{64}', retry):
            raise ValueError('Idempotency-Key requires 64 lowercase hexadecimal digits')
        command = self.binding.command(identity, body)
        # A principal-scoped stable key. Native stored command bytes detect reuse
        # for a different command, even after later commits and process restarts.
        key = hashlib.sha256((identity.name + '\0' + retry).encode()).hexdigest()
        result = subprocess.run([str(self.binary), 'keyed', str(self.database), key,
                                 ' '.join(map(str, command_values(command))), '1'],
                                capture_output=True, text=True, timeout=30, check=False)
        if result.returncode:
            raise ValueError(result.stderr.strip() or 'native process refused')
        return strict_json(result.stdout)


def handler(mount):
    """The repository's standard-library HTTP surface, backed by native SQLite."""
    class Handler(BaseHTTPRequestHandler):
        def setup(self):
            if hasattr(self.request, 'settimeout'):
                self.request.settimeout(30)
            super().setup()

        def log_message(self, *_args):
            pass

        def reply(self, status, value):
            data = json.dumps(value, sort_keys=True, separators=(',', ':')).encode()
            self.send_response(status)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def do_POST(self):
            try:
                if self.path != '/commands':
                    self.reply(404, {'error': 'unknown route'})
                    return
                for name in ('Authorization', 'Content-Length', 'Idempotency-Key'):
                    if len(self.headers.get_all(name, [])) != 1:
                        raise ValueError('exactly one ' + name + ' required')
                if self.headers.get('Transfer-Encoding') is not None:
                    raise ValueError('transfer coding is unsupported')
                authorization = self.headers['Authorization']
                mount.binding.actor(authorization)
                size = self.headers['Content-Length']
                if not size.isascii() or not size.isdecimal() or not 0 < int(size) <= MAX_BODY:
                    raise ValueError('bounded Content-Length required')
                data = self.rfile.read(int(size))
                if len(data) != int(size):
                    raise ValueError('incomplete body')
                value = mount.submit(authorization, self.headers['Idempotency-Key'], strict_json(data))
                self.reply(200, value)
            except PermissionError as error:
                self.reply(401, {'error': str(error)})
            except (ValueError, UnicodeError, RecursionError) as error:
                self.reply(400, {'error': str(error)})
            except (OSError, subprocess.TimeoutExpired):
                self.reply(503, {'error': 'native command unavailable; retry with the same key'})
    return Handler


def local_http(mount, request):
    """Real HTTP parsing/handler over owned byte streams; no TCP claim."""
    class Connection:
        def __init__(self):
            self.answer = io.BytesIO()
        def makefile(self, _mode, _buffering=-1):
            return io.BytesIO(request)
        def sendall(self, data):
            self.answer.write(data)
    connection = Connection()
    handler(mount)(connection, ('127.0.0.1', 0), object())
    return connection.answer.getvalue()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', type=Path, required=True)
    p.add_argument('--database', type=Path, required=True)
    p.add_argument('--identities', type=Path, required=True)
    p.add_argument('--port', type=int, default=0)
    a = p.parse_args()
    data = strict_json(a.identities.read_text())
    binding = Binding(tuple(Identity(**row) for row in data))
    mount = Mount(a.binary.resolve(), a.database.resolve(), binding)
    with HTTPServer(('127.0.0.1', a.port), handler(mount)) as server:
        server.timeout = 30
        print(json.dumps({'listening': server.server_address}), flush=True)
        server.serve_forever()


if __name__ == '__main__':
    main()
