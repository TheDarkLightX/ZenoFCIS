"""Optional invocation observations; never checker or publication evidence."""
from __future__ import annotations

from datetime import datetime, timezone
import hashlib
import contextlib
import json
import os
from pathlib import Path
import stat
import sys
import time

SCHEMA = "zeno-fcis/telemetry/1"
WORK_KEYS = frozenset(("work", "work_units", "search_work", "check_work", "session_work", "used_work", "reserved_work", "steps"))
CONTAINERS = frozenset(("accounting", "usage", "detail", "report", "result", "session", "totals", "spent"))


class ObservationStream:
    """Forward stdout unchanged and keep only numeric, named work observations."""
    def __init__(self, stream):
        self.stream = stream
        self.pending = ""
        self.work = {}

    def __getattr__(self, name):
        return getattr(self.stream, name)

    def write(self, text):
        count = self.stream.write(text)
        try:
            self.pending += text
            if len(self.pending) > 1024 * 1024:
                self.pending = ""
            while "\n" in self.pending:
                line, self.pending = self.pending.split("\n", 1)
                try:
                    self.observe(json.loads(line))
                except (ValueError, TypeError):
                    pass
        except Exception:
            self.pending = ""
        return count

    def observe(self, value, path=""):
        if not isinstance(value, dict):
            return
        for key, child in value.items():
            name = f"{path}.{key}" if path else key
            if key in WORK_KEYS and type(child) is int and child >= 0:
                self.work[name] = child
            elif key in CONTAINERS:
                self.observe(child, name)


def subject_digest(inputs):
    digests = sorted({item["sha256"] for item in inputs if item.get("sha256")})
    return hashlib.sha256("\n".join(digests).encode()).hexdigest() if digests else None


def input_digests(arguments):
    """Snapshot regular file operands. Paths and source text are never emitted."""
    inputs = []
    for index, argument in enumerate(arguments):
        path = Path(argument)
        try:
            fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK)
            try:
                if not stat.S_ISREG(os.fstat(fd).st_mode):
                    continue
                digest = hashlib.sha256()
                with os.fdopen(fd, "rb", closefd=False) as stream:
                    for chunk in iter(lambda: stream.read(65536), b""):
                        digest.update(chunk)
                inputs.append({"name": f"arg{index}", "sha256": digest.hexdigest()})
            finally:
                os.close(fd)
        except (OSError, ValueError):
            pass
    return inputs


def append(destination, record):
    """Exactly one O_APPEND write per line; any reporting failure is advisory."""
    try:
        line = (json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n").encode()
        fd = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_APPEND | os.O_NONBLOCK, 0o600)
        try:
            if stat.S_ISREG(os.fstat(fd).st_mode):
                os.write(fd, line)
        finally:
            os.close(fd)
    except Exception:
        pass


def run_cli(function, command=None):
    """Run the existing entrypoint unchanged, including SystemExit/exception flow."""
    destination = os.environ.get("ZENO_FCIS_TELEMETRY")
    if not destination:
        return function()
    started = time.monotonic_ns()
    timestamp = datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")
    try:
        inputs = input_digests(sys.argv[1:])
    except Exception:
        inputs = []
    code = 0
    observer = ObservationStream(sys.stdout)
    try:
        with contextlib.redirect_stdout(observer):
            return function()
    except SystemExit as error:
        code = error.code if isinstance(error.code, int) else (0 if error.code is None else 1)
        raise
    except KeyboardInterrupt:
        code = 130
        raise
    except BaseException:
        code = 1
        raise
    finally:
        try:
            append(destination, {"schema": SCHEMA, "timestamp": timestamp,
                "actor": os.environ.get("ZENO_FCIS_COMMONS_ACTOR", "unknown"),
                "command": command or ["python", Path(sys.argv[0]).stem],
                "inputs": inputs, "subject_sha256": subject_digest(inputs),
                "exit_code": code, "verdict": "accepted" if code == 0 else "refused",
                "work_units": observer.work or None, "duration_ms": (time.monotonic_ns() - started) // 1_000_000})
        except Exception:
            pass
