"""Local preparation shell for a shared, revision-bound terminal/MCP workspace.

Files and advisory locks coordinate cooperating clients; they are not security
against a malicious same-user process and grant no production authority.
"""

from contextlib import contextmanager
import fcntl
import json
import os
from pathlib import Path
import tempfile
import time

from behavior import Refusal, canonical, parse
from workflow import Session


MAX_SNAPSHOT = 32 * 1024 * 1024


class SharedWorkspace:
    def __init__(self, path: Path):
        self.path = path.absolute()

    @contextmanager
    def locked(self):
        descriptor = os.open(str(self.path) + ".lock", os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
        try:
            deadline = time.monotonic() + 2
            while True:
                try:
                    fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    break
                except BlockingIOError:
                    if time.monotonic() >= deadline:
                        raise Refusal("Workspace is busy; retry the exact revision")
                    time.sleep(.01)
            yield
        finally:
            os.close(descriptor)

    def load(self):
        descriptor = os.open(self.path, os.O_RDONLY | os.O_NOFOLLOW)
        with os.fdopen(descriptor, encoding="ascii") as stream:
            raw = stream.read(MAX_SNAPSHOT + 1)
        if len(raw) > MAX_SNAPSHOT:
            raise Refusal("Shared workspace exceeds32MiB")
        saved = json.loads(raw)
        if not isinstance(saved, dict) or set(saved) != {"schema", "session"} or saved["schema"] != "zal/shared-workspace/1":
            raise Refusal("Unknown shared workspace format")
        data = saved["session"]
        session = Session(parse(data["model"]["symbolic"]))
        if session.model.revision != data["model"]["revision"] or data["agreement"] not in {"seed-example", "human-accepted"} or data["realization"] != "not-run":
            raise Refusal("Workspace identity/status mismatch")
        events = data["events"]
        if not isinstance(events, list) or len(events) > 256 or any(not isinstance(event, dict) or
                event.get("sequence") != i or event.get("actor") not in {"human", "agent"} for i, event in enumerate(events)):
            raise Refusal("Invalid bounded workspace event history")
        session.events = events
        session.selected = data["selected"]
        if session.selected not in session.objects():
            raise Refusal("Workspace selection has an unknown typed ID")
        session.agreement = data["agreement"]
        # Preserve the proposal's original source-bound evidence. accept_human
        # reruns current checks and refuses a stale checker closure.
        proposal = data["proposal"]
        if proposal:
            candidate = parse(proposal["model"]["symbolic"])
            if proposal["base_revision"] != session.model.revision or candidate.revision != proposal["revision"]:
                raise Refusal("Workspace proposal identity mismatch")
            session.proposal = {**proposal, "model": candidate}
        return session

    def save(self, session):
        data = canonical({"schema": "zal/shared-workspace/1", "session": session.view()}) + "\n"
        if len(data) > MAX_SNAPSHOT:
            raise Refusal("Shared workspace exceeds32MiB")
        descriptor, temporary = tempfile.mkstemp(prefix=".zal-write-", dir=self.path.parent)
        try:
            with os.fdopen(descriptor, "w", encoding="ascii") as stream:
                stream.write(data)
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(temporary, self.path)
        finally:
            if os.path.exists(temporary):
                os.unlink(temporary)

    def initialize(self, source):
        with self.locked():
            if self.path.exists() or self.path.is_symlink():
                raise Refusal("Workspace already exists; existing edits are preserved")
            self.save(Session(parse(source)))

    def use(self, action):
        with self.locked():
            session = self.load()
            before = canonical(session.view())
            result = action(session)
            if canonical(session.view()) != before:
                self.save(session)
            return result
