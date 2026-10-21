"""Zeno Commons: the shared state of a ZenoFCIS software factory.

Factory parts and the agents and people using them coordinate and share what they learn through one local
store. Every write is one SQLite transaction that appends a hash-chained event and updates the projections,
so concurrent writers never lose an update and sequence numbers only increase. Standard library only.

The store records coordination (leases, agent status, numbered messages with acknowledgements) and knowledge
(hypotheses, claims, evidence, refutations, gaps, lessons and decisions, with links). It decides nothing about
programs: every verdict it records comes from the ZenoFCIS checkers or from a person, and is cited, not proven,
here.
"""

from __future__ import annotations

import hashlib
import json
import lzma
import os
import re
import sqlite3
import time
from pathlib import Path

FORMAT = "zeno-fcis/commons/1"
DB_NAME = "commons.sqlite3"
RECORD_TYPES = ("hypothesis", "claim", "evidence", "refutation", "counterexample", "result", "gap",
                "lesson", "decision", "question", "answer", "risk", "note")
RECORD_STATUSES = ("open", "testing", "supported", "refuted", "superseded", "stale", "answered",
                   "resolved", "accepted", "rejected")
AGENT_STATES = ("working", "blocked", "idle", "done")
LINK_RELATIONS = ("supports", "refutes", "answers", "supersedes", "cites", "duplicates", "blocks", "relates")
NAME = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._:@/-]{0,127}$")
MAX_TEXT = 65536
MAX_TAGS = 32
# Negative memory: what advise returns before an action.
ADVISE_TYPES = ("lesson", "refutation", "counterexample", "gap", "risk", "decision")
# Size bounds. The live store keeps the newest events; older ones move to compressed, chain-checked segments.
ARCHIVE_DIR = "archive"
ARCHIVE_FORMAT = "zeno-fcis/commons-archive/1"
BUDGET_BYTES = 64 * 1024 * 1024
MAX_LIVE_EVENTS = 50_000
KEEP_EVENTS = 20_000
GENESIS = "0" * 64


class CommonsError(ValueError):
    """A refused request. The message is safe to show to the caller."""


class _Existing(Exception):
    """Raised inside a write to roll it back when the record already exists (idempotent add)."""


def _now() -> float:
    return time.time()


def _iso(at: float) -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(at))


def _canonical(value) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def _name(value: str, what: str) -> str:
    if not isinstance(value, str) or not NAME.match(value):
        raise CommonsError(f"{what} must be 1-128 characters of letters, digits and ._:@/- and start with a letter or digit")
    return value


def _text(value, what: str, allow_empty: bool = False) -> str:
    if not isinstance(value, str) or (not value.strip() and not allow_empty) or len(value) > MAX_TEXT:
        raise CommonsError(f"{what} must be non-empty text of at most {MAX_TEXT} characters")
    return value


def _tags(values) -> list[str]:
    if values is None:
        return []
    if not isinstance(values, (list, tuple)) or len(values) > MAX_TAGS:
        raise CommonsError(f"tags must be a list of at most {MAX_TAGS} names")
    return sorted({_name(str(v).lower(), "tag") for v in values})


def normalize_resource(resource: str) -> str:
    """Resources are opaque names, except that paths are made absolute so prefixes compare correctly."""
    if not isinstance(resource, str) or not resource or len(resource) > 4096:
        raise CommonsError("resource must be non-empty text of at most 4096 characters")
    if resource.startswith(("/", "~", "./", "../")):
        return os.path.normpath(os.path.abspath(os.path.expanduser(resource)))
    return resource


def _covers(a: str, b: str) -> bool:
    """True when either resource contains the other: removing a parent folder touches a leased child too."""
    if a == b:
        return True
    if a.startswith("/") and b.startswith("/"):
        return b.startswith(a.rstrip("/") + "/") or a.startswith(b.rstrip("/") + "/")
    return False


def _words(text: str) -> set[str]:
    return {w for w in re.findall(r"[a-z0-9]+", text.lower()) if len(w) > 2}


SCHEMA = """
CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS events(
  seq INTEGER PRIMARY KEY AUTOINCREMENT, at REAL NOT NULL, actor TEXT NOT NULL, kind TEXT NOT NULL,
  subject TEXT NOT NULL, body TEXT NOT NULL, prev TEXT NOT NULL, hash TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS leases(
  resource TEXT PRIMARY KEY, owner TEXT NOT NULL, note TEXT NOT NULL, taken_seq INTEGER NOT NULL,
  heartbeat_at REAL NOT NULL, expires_at REAL NOT NULL);
CREATE TABLE IF NOT EXISTS agents(
  name TEXT PRIMARY KEY, state TEXT NOT NULL, step TEXT NOT NULL, question TEXT NOT NULL,
  updated_at REAL NOT NULL, read_through INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS messages(
  seq INTEGER PRIMARY KEY, sender TEXT NOT NULL, recipient TEXT NOT NULL, body TEXT NOT NULL, at REAL NOT NULL);
CREATE TABLE IF NOT EXISTS records(
  id TEXT PRIMARY KEY, type TEXT NOT NULL, status TEXT NOT NULL, title TEXT NOT NULL, body TEXT NOT NULL,
  tags TEXT NOT NULL, refs TEXT NOT NULL, author TEXT NOT NULL, created_seq INTEGER NOT NULL,
  updated_seq INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS identities(
  id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE, kind TEXT NOT NULL, role TEXT NOT NULL,
  registered_by TEXT NOT NULL, registered_seq INTEGER NOT NULL, registered_at REAL NOT NULL);
CREATE TABLE IF NOT EXISTS links(
  src TEXT NOT NULL, rel TEXT NOT NULL, dst TEXT NOT NULL, seq INTEGER NOT NULL, PRIMARY KEY(src, rel, dst));
CREATE TABLE IF NOT EXISTS checkpoints(
  last INTEGER PRIMARY KEY, first INTEGER NOT NULL, prev TEXT NOT NULL, head TEXT NOT NULL, segment TEXT NOT NULL,
  sha256 TEXT NOT NULL, bytes INTEGER NOT NULL, events INTEGER NOT NULL, archived_seq INTEGER NOT NULL);
"""


def _check_chain(rows, prev: str, first: int) -> dict:
    """Recompute a run of events from the hash before it. Sequence numbers must be contiguous from first."""
    count = 0
    for seq, at, actor, kind, subject, body, stored_prev, digest in rows:
        expected = hashlib.sha256((prev + _canonical([at, actor, kind, subject, json.loads(body)])).encode()).hexdigest()
        if seq != first + count:
            return {"ok": False, "broken_at": seq, "checked": count, "reason": f"expected event {first + count}"}
        if stored_prev != prev or digest != expected:
            return {"ok": False, "broken_at": seq, "checked": count, "reason": "hash chain mismatch"}
        prev, count = digest, count + 1
    return {"ok": True, "checked": count, "head": prev}


def _segment_rows(path: Path) -> tuple[dict, list]:
    """The header and event rows of one archive segment."""
    with lzma.open(path, "rt", encoding="utf-8") as stream:
        header = json.loads(stream.readline())
        if header.get("format") != ARCHIVE_FORMAT:
            raise CommonsError(f"{path.name} is not a {ARCHIVE_FORMAT} segment")
        rows = []
        for line in stream:
            e = json.loads(line)
            rows.append((e["seq"], e["at"], e["actor"], e["kind"], e["subject"], e["body"], e["prev"], e["hash"]))
    return header, rows


def _event_line(row) -> str:
    seq, at, actor, kind, subject, body, prev, digest = row
    return _canonical({"seq": seq, "at": at, "actor": actor, "kind": kind, "subject": subject, "body": body,
                       "prev": prev, "hash": digest})


def _sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as stream:
        for block in iter(lambda: stream.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


TABLES = ("meta", "events", "leases", "agents", "messages", "records", "identities", "links", "checkpoints",
          "sqlite_sequence")


def _refuse_links(directory: Path) -> None:
    """Sandboxed agents can write the store directory, so host processes never follow links placed in it."""
    for name in (DB_NAME, DB_NAME + "-wal", DB_NAME + "-shm", DB_NAME + "-journal", ARCHIVE_DIR):
        if (directory / name).is_symlink():
            raise CommonsError(f"{name} in the store is a symbolic link; refusing to open it")


class Commons:
    """One store directory. Safe for many processes: every write is one immediate transaction.

    The directory is shared with sandboxed agents, so opening it checks before writing anything: no symbolic
    links, the opened file is the store's own, and the schema holds only the Commons tables (no triggers or
    views). SQLite runs in defensive mode with untrusted schemas."""

    def __init__(self, directory: str | os.PathLike, actor: str, clock=_now):
        self.directory = Path(directory)
        self.actor = _name(actor, "actor")
        self.clock = clock
        self.directory.mkdir(parents=True, exist_ok=True)
        _refuse_links(self.directory)
        self.db = sqlite3.connect(self.directory / DB_NAME, timeout=30.0, isolation_level=None)
        try:
            self._check_opened()
        except BaseException:
            self.db.close()
            raise
        self.db.execute("PRAGMA journal_mode=WAL")
        self.db.execute("PRAGMA busy_timeout=30000")
        self.db.execute("PRAGMA synchronous=NORMAL")
        self.db.executescript(SCHEMA)
        self.db.execute("INSERT OR IGNORE INTO meta(key, value) VALUES('format', ?)", (FORMAT,))
        found = self.db.execute("SELECT value FROM meta WHERE key='format'").fetchone()[0]
        if found != FORMAT:
            raise CommonsError(f"store format {found} is not {FORMAT}")

    def _check_opened(self) -> None:
        if hasattr(sqlite3, "SQLITE_DBCONFIG_DEFENSIVE"):
            self.db.setconfig(sqlite3.SQLITE_DBCONFIG_DEFENSIVE, True)
        self.db.execute("PRAGMA trusted_schema=OFF")
        self.db.execute("PRAGMA cell_size_check=ON")
        # SQLite reports a linked file under its target's path, so this also catches a file swapped for a link
        # between the check above and the open.
        opened = self.db.execute("PRAGMA database_list").fetchone()[2]
        expected = os.path.join(os.path.realpath(self.directory), DB_NAME)
        if opened and opened != expected:
            raise CommonsError("the opened database is not the store's own file; refusing")
        foreign = [(kind, name) for kind, name in self.db.execute(
            "SELECT type, name FROM sqlite_master WHERE name NOT LIKE 'sqlite_autoindex_%'")
            if kind != "table" or name not in TABLES]
        if foreign:
            raise CommonsError("the store holds objects that are not Commons tables: "
                               + ", ".join(f"{k} {n}" for k, n in foreign[:5]))

    def close(self) -> None:
        self.db.close()

    # Every write goes through here: one transaction, one event, chained to the previous event's hash.
    def _write(self, kind: str, subject: str, body: dict, apply):
        cursor = self.db.cursor()
        cursor.execute("BEGIN IMMEDIATE")
        try:
            at = self.clock()
            row = cursor.execute("SELECT hash FROM events ORDER BY seq DESC LIMIT 1").fetchone()
            prev = row[0] if row else "0" * 64
            digest = hashlib.sha256(
                (prev + _canonical([at, self.actor, kind, subject, body])).encode()).hexdigest()
            cursor.execute("INSERT INTO events(at, actor, kind, subject, body, prev, hash) VALUES(?,?,?,?,?,?,?)",
                           (at, self.actor, kind, subject, _canonical(body), prev, digest))
            seq = cursor.lastrowid
            result = apply(cursor, seq, at)
            cursor.execute("COMMIT")
            return seq if result is None else result
        except BaseException:
            cursor.execute("ROLLBACK")
            raise

    # ---- identity: every agent has a unique name and a hash ID -----------------------------------------
    def register(self, name: str = "", kind: str = "agent", role: str = "") -> dict:
        """Register an identity. An agent registers itself (name defaults to the actor); a lead may register others.

        The ID is a hash of the registration (name, kind, role, time and a random nonce), so two registrations of
        different agents never collide, and the ID stays fixed for the agent's lifetime in this store.
        """
        name = _name(name or self.actor, "agent name")
        kind, role = _name(kind, "kind"), _text(role, "role", allow_empty=True)[:200]
        nonce = os.urandom(16).hex()

        def apply(cursor, seq, at):
            row = cursor.execute("SELECT id FROM identities WHERE name=?", (name,)).fetchone()
            if row:
                raise CommonsError(f"{name} is already registered as {row[0]}")
            digest = hashlib.sha256(_canonical([name, kind, role, at, nonce]).encode()).hexdigest()
            agent_id = "ag-" + digest[:16]
            cursor.execute("INSERT INTO identities VALUES(?,?,?,?,?,?,?)", (agent_id, name, kind, role, self.actor, seq, at))
            return {"seq": seq, "id": agent_id, "name": name, "kind": kind, "role": role}

        return self._write("identity.register", name, {"kind": kind, "role": role, "nonce": nonce}, apply)

    def identity(self, name_or_id: str = "") -> dict:
        key = name_or_id or self.actor
        row = self.db.execute("SELECT id, name, kind, role, registered_by, registered_at FROM identities WHERE name=? OR id=?",
                              (key, key)).fetchone()
        if not row:
            raise CommonsError(f"no registered identity {key}; register first")
        return {"id": row[0], "name": row[1], "kind": row[2], "role": row[3], "registered_by": row[4],
                "registered": _iso(row[5])}

    def _require_registered(self, cursor) -> None:
        if not cursor.execute("SELECT 1 FROM identities WHERE name=?", (self.actor,)).fetchone():
            raise CommonsError(f"{self.actor} is not registered; run register first so others can address you")

    def _resolve(self, cursor, name_or_id: str) -> str:
        row = cursor.execute("SELECT name FROM identities WHERE name=? OR id=?", (name_or_id, name_or_id)).fetchone()
        if not row:
            raise CommonsError(f"no registered agent {name_or_id}")
        return row[0]

    # ---- coordination: leases -------------------------------------------------------------------------
    # Time only signals liveness. A lease past its expiry is "stale": its holder stopped renewing, but may be
    # paused rather than finished, and clocks can jump. A stale lease still protects its resource until the
    # holder releases it, or someone takes or releases it with a recorded force reason.
    def _unreleased_leases(self, cursor):
        return cursor.execute("SELECT resource, owner, note, expires_at FROM leases").fetchall()

    @staticmethod
    def _lease_view(resource, owner, note, expires, at) -> dict:
        return {"resource": resource, "owner": owner, "note": note, "expires": _iso(expires),
                "state": "active" if expires > at else "stale"}

    def lease_take(self, resource: str, ttl_seconds: float = 3600.0, note: str = "", force_reason: str = "") -> dict:
        resource = normalize_resource(resource)
        if not (60 <= ttl_seconds <= 7 * 86400):
            raise CommonsError("ttl_seconds must be between 60 and 604800")
        note = _text(note, "note", allow_empty=True)
        force_reason = _text(force_reason, "force reason", allow_empty=True)

        def apply(cursor, seq, at):
            self._require_registered(cursor)
            displaced = []
            for held, owner, _, expires in self._unreleased_leases(cursor):
                if owner == self.actor or not _covers(held, resource):
                    continue
                if expires > at:
                    raise CommonsError(f"{resource} overlaps {held}, leased by {owner} until {_iso(expires)}")
                if not force_reason.strip():
                    raise CommonsError(f"{resource} overlaps {held}: {owner}'s lease went stale at {_iso(expires)} but "
                                       f"was never released. Confirm {owner} has finished, then take it with a force reason")
                displaced.append([held, owner])
            for held, _ in displaced:
                cursor.execute("DELETE FROM leases WHERE resource=?", (held,))
            cursor.execute("DELETE FROM leases WHERE resource=?", (resource,))
            cursor.execute("INSERT INTO leases VALUES(?,?,?,?,?,?)",
                           (resource, self.actor, note, seq, at, at + ttl_seconds))
            return {"seq": seq, "resource": resource, "owner": self.actor, "expires": _iso(at + ttl_seconds),
                    "displaced": displaced}

        body = {"ttl_seconds": ttl_seconds, "note": note}
        if force_reason.strip():
            body["force_reason"] = force_reason
        return self._write("lease.take", resource, body, apply)

    def lease_renew(self, resource: str, ttl_seconds: float = 3600.0) -> dict:
        resource = normalize_resource(resource)

        def apply(cursor, seq, at):
            self._require_registered(cursor)
            row = cursor.execute("SELECT owner, expires_at FROM leases WHERE resource=?", (resource,)).fetchone()
            if not row:
                raise CommonsError(f"no lease on {resource}; take it")
            if row[0] != self.actor:
                raise CommonsError(f"{resource} is leased by {row[0]}")  # a stale lease is renewed by its holder
            cursor.execute("UPDATE leases SET heartbeat_at=?, expires_at=? WHERE resource=?",
                           (at, at + ttl_seconds, resource))
            return {"seq": seq, "resource": resource, "expires": _iso(at + ttl_seconds)}

        return self._write("lease.renew", resource, {"ttl_seconds": ttl_seconds}, apply)

    def lease_release(self, resource: str, force_reason: str = "") -> dict:
        resource = normalize_resource(resource)

        def apply(cursor, seq, at):
            self._require_registered(cursor)
            row = cursor.execute("SELECT owner FROM leases WHERE resource=?", (resource,)).fetchone()
            if not row:
                raise CommonsError(f"no lease on {resource}")
            if row[0] != self.actor and not force_reason.strip():
                raise CommonsError(f"{resource} is leased by {row[0]}; only its owner releases it, or give a force reason")
            cursor.execute("DELETE FROM leases WHERE resource=?", (resource,))
            return {"seq": seq, "resource": resource, "released_from": row[0]}

        return self._write("lease.release", resource, {"force_reason": force_reason}, apply)

    def lease_check(self, resource: str) -> list[dict]:
        """Unreleased leases, active or stale, that cover the resource or lie inside it. Empty means free."""
        resource = normalize_resource(resource)
        at = self.clock()
        return [self._lease_view(*row, at) for row in self._unreleased_leases(self.db) if _covers(row[0], resource)]

    def leases(self) -> list[dict]:
        at = self.clock()
        return [self._lease_view(*row, at) for row in self._unreleased_leases(self.db)]

    # ---- coordination: agent lifecycle ------------------------------------------------------------------
    def status(self, state: str, step: str = "", question: str = "") -> dict:
        if state not in AGENT_STATES:
            raise CommonsError(f"state must be one of {', '.join(AGENT_STATES)}")
        step, question = _text(step, "step", True), _text(question, "question", True)
        if state == "blocked" and not question.strip():
            raise CommonsError("a blocked status needs the question that blocks it")

        def apply(cursor, seq, at):
            self._require_registered(cursor)
            if state == "done":
                held = [r for r, o, _, _ in self._unreleased_leases(cursor) if o == self.actor]
                if held:
                    raise CommonsError("release your leases before posting done: " + ", ".join(held))
            cursor.execute(
                "INSERT INTO agents(name, state, step, question, updated_at) VALUES(?,?,?,?,?) "
                "ON CONFLICT(name) DO UPDATE SET state=excluded.state, step=excluded.step, "
                "question=excluded.question, updated_at=excluded.updated_at",
                (self.actor, state, step, question, at))
            return {"seq": seq, "agent": self.actor, "state": state}

        return self._write("agent.status", self.actor, {"state": state, "step": step, "question": question}, apply)

    def agents(self) -> list[dict]:
        """Every registered agent with its hash ID, kind, role, latest state and read-through number."""
        rows = self.db.execute(
            "SELECT i.name, i.id, i.kind, i.role, a.state, a.step, a.question, a.updated_at, a.read_through "
            "FROM identities i LEFT JOIN agents a ON a.name = i.name ORDER BY i.name")
        return [{"agent": n, "id": i, "kind": k, "role": r, "state": s or "registered", "step": st or "",
                 "question": q or "", "updated": _iso(u) if u else "", "read_through": rt or 0}
                for n, i, k, r, s, st, q, u, rt in rows]

    # ---- coordination: messages -------------------------------------------------------------------------
    def send(self, recipient: str, body: str) -> dict:
        """Send to a registered agent by name or hash ID, or to '*' for everyone."""
        recipient = recipient if recipient == "*" else _name(recipient, "recipient")
        body = _text(body, "message")

        def apply(cursor, seq, at):
            self._require_registered(cursor)
            to = recipient if recipient == "*" else self._resolve(cursor, recipient)
            cursor.execute("INSERT INTO messages VALUES(?,?,?,?,?)", (seq, self.actor, to, body, at))
            return {"seq": seq, "to": to}

        return self._write("message.send", recipient, {"body": body}, apply)

    def _read_through(self, cursor) -> int:
        row = cursor.execute("SELECT read_through FROM agents WHERE name=?", (self.actor,)).fetchone()
        return row[0] if row else 0

    def inbox(self, include_read: bool = False) -> dict:
        through = self._read_through(self.db)
        floor = 0 if include_read else through
        rows = self.db.execute(
            "SELECT seq, sender, recipient, body, at FROM messages WHERE seq > ? AND (recipient=? OR recipient='*') "
            "AND sender != ? ORDER BY seq", (floor, self.actor, self.actor)).fetchall()
        return {"read_through": through,
                "messages": [{"seq": s, "from": f, "to": t, "body": b, "at": _iso(a)} for s, f, t, b, a in rows]}

    def ack(self, through: int) -> dict:
        if not isinstance(through, int) or through < 0:
            raise CommonsError("through must be a non-negative message number")

        def apply(cursor, seq, at):
            self._require_registered(cursor)
            top = cursor.execute("SELECT COALESCE(MAX(seq), 0) FROM events").fetchone()[0]
            if through > top:
                raise CommonsError(f"message {through} does not exist yet")
            current = self._read_through(cursor)
            new = max(current, through)  # read receipts never move backwards
            cursor.execute(
                "INSERT INTO agents(name, state, step, question, updated_at, read_through) VALUES(?, 'working', '', '', ?, ?) "
                "ON CONFLICT(name) DO UPDATE SET read_through=?", (self.actor, at, new, new))
            return {"seq": seq, "read_through": new}

        return self._write("message.ack", self.actor, {"through": through}, apply)

    # ---- knowledge: records and links -------------------------------------------------------------------
    def record_add(self, type: str, title: str, body: str = "", status: str = "open", tags=None, refs=None,
                   record_id: str = "") -> dict:
        if type not in RECORD_TYPES:
            raise CommonsError(f"type must be one of {', '.join(RECORD_TYPES)}")
        if status not in RECORD_STATUSES:
            raise CommonsError(f"status must be one of {', '.join(RECORD_STATUSES)}")
        title, body = _text(title, "title"), _text(body, "body", True)
        tags = _tags(tags)
        refs = refs or []
        if not isinstance(refs, list) or len(refs) > 64 or not all(isinstance(r, str) and len(r) <= 4096 for r in refs):
            raise CommonsError("refs must be a list of at most 64 references (paths, commits, record IDs, hashes)")
        content = _canonical([type, title, body, tags, refs])
        record_id = record_id or f"{type[:3]}-" + hashlib.sha256(content.encode()).hexdigest()[:12]
        record_id = _name(record_id, "record id")

        def apply(cursor, seq, at):
            self._require_registered(cursor)
            if cursor.execute("SELECT id FROM records WHERE id=?", (record_id,)).fetchone():
                raise _Existing()  # idempotent: same content, same id, and no event is written
            cursor.execute("INSERT INTO records VALUES(?,?,?,?,?,?,?,?,?,?)",
                           (record_id, type, status, title, body, _canonical(tags), _canonical(refs),
                            self.actor, seq, seq))
            return {"seq": seq, "id": record_id, "existing": False}

        try:
            return self._write("record.add", record_id,
                               {"type": type, "status": status, "title": title, "body": body, "tags": tags, "refs": refs},
                               apply)
        except _Existing:
            return {"seq": None, "id": record_id, "existing": True}

    def record_update(self, record_id: str, status: str = "", note: str = "", refs=None, title: str = "") -> dict:
        """Change status, append a dated note, add references or correct the title (the old title stays in the note).

        A record never grows past the text limit: once full, add a new record and link it."""
        if status and status not in RECORD_STATUSES:
            raise CommonsError(f"status must be one of {', '.join(RECORD_STATUSES)}")
        note = _text(note, "note", True)
        title = _text(title, "title", True).strip()
        refs = refs or []
        if not isinstance(refs, list) or len(refs) > 64 or not all(isinstance(r, str) and len(r) <= 4096 for r in refs):
            raise CommonsError("refs must be a list of at most 64 references (paths, commits, record IDs, hashes)")

        def apply(cursor, seq, at):
            self._require_registered(cursor)
            row = cursor.execute("SELECT status, refs, body, title FROM records WHERE id=?", (record_id,)).fetchone()
            if not row:
                raise CommonsError(f"no record {record_id}")
            new_status = status or row[0]
            new_refs = sorted(set(json.loads(row[1])) | set(refs))
            if len(new_refs) > 64:
                raise CommonsError(f"{record_id} would have more than 64 references; add a new record and link it")
            new_title = title or row[3]
            lines = ([f"title was: {row[3]}"] if title and title != row[3] else []) + ([note] if note.strip() else [])
            new_body = row[2] + "".join(f"\n[{_iso(at)} {self.actor}] {line}" for line in lines)
            if len(new_body) > MAX_TEXT:
                raise CommonsError(f"{record_id} is full ({MAX_TEXT} characters); add a new record and link it")
            cursor.execute("UPDATE records SET status=?, refs=?, body=?, title=?, updated_seq=? WHERE id=?",
                           (new_status, _canonical(new_refs), new_body, new_title, seq, record_id))
            return {"seq": seq, "id": record_id, "status": new_status, "title": new_title}

        body = {"status": status, "note": note, "refs": refs}
        if title:
            body["title"] = title
        return self._write("record.update", record_id, body, apply)

    def link(self, src: str, rel: str, dst: str) -> dict:
        if rel not in LINK_RELATIONS:
            raise CommonsError(f"relation must be one of {', '.join(LINK_RELATIONS)}")

        def apply(cursor, seq, at):
            self._require_registered(cursor)
            for rid in (src, dst):
                if not cursor.execute("SELECT 1 FROM records WHERE id=?", (rid,)).fetchone():
                    raise CommonsError(f"no record {rid}")
            cursor.execute("INSERT OR IGNORE INTO links VALUES(?,?,?,?)", (src, rel, dst, seq))
            if rel == "supersedes":
                cursor.execute("UPDATE records SET status='superseded', updated_seq=? WHERE id=?", (seq, dst))
            return {"seq": seq, "link": [src, rel, dst]}

        return self._write("link.add", src, {"rel": rel, "dst": dst}, apply)

    def _row(self, row) -> dict:
        rid, type_, status, title, body, tags, refs, author, created, updated = row
        return {"id": rid, "type": type_, "status": status, "title": title, "body": body,
                "tags": json.loads(tags), "refs": json.loads(refs), "author": author,
                "created_seq": created, "updated_seq": updated}

    def record(self, record_id: str) -> dict:
        row = self.db.execute("SELECT * FROM records WHERE id=?", (record_id,)).fetchone()
        if not row:
            raise CommonsError(f"no record {record_id}")
        out = self._row(row)
        out["links"] = [{"src": s, "rel": r, "dst": d} for s, r, d in self.db.execute(
            "SELECT src, rel, dst FROM links WHERE src=? OR dst=? ORDER BY seq", (record_id, record_id))]
        return out

    def records(self, type: str = "", status: str = "", tag: str = "", limit: int = 100) -> list[dict]:
        query, args = "SELECT * FROM records WHERE 1=1", []
        for column, value in (("type", type), ("status", status)):
            if value:
                query += f" AND {column}=?"
                args.append(value)
        query += " ORDER BY updated_seq DESC"
        rows = [self._row(r) for r in self.db.execute(query, args)]
        if tag:
            rows = [r for r in rows if tag.lower() in r["tags"]]
        return rows[:max(1, min(limit, 1000))]

    # ---- the read-before-acting rule ----------------------------------------------------------------------
    def advise(self, action: str, tags=None, text: str = "", limit: int = 10) -> dict:
        """Negative memory relevant to an action, plus leases and unread messages that bear on it.

        Deterministic retrieval: tag overlap counts 3, word overlap 1; refuted, superseded and stale
        records are excluded except refutations and counterexamples themselves. No model is consulted.
        """
        action = _text(action, "action")
        wanted_tags = set(_tags(tags))
        words = _words(action + " " + text)
        scored = []
        for record in self.records(limit=1000):
            if record["type"] not in ADVISE_TYPES:
                continue
            if record["status"] in ("superseded", "stale", "rejected"):
                continue
            score = 3 * len(wanted_tags & set(record["tags"])) + len(words & _words(record["title"] + " " + record["body"]))
            if score:
                scored.append((score, record["updated_seq"], record))
        scored.sort(key=lambda item: (-item[0], -item[1]))
        resources = [t for t in (text.split() if text else []) if t.startswith("/")]
        return {"action": action,
                "advice": [dict(r, score=s) for s, _, r in scored[:max(1, min(limit, 50))]],
                "leases": [lease for res in resources for lease in self.lease_check(res)],
                "unread": self.inbox()["messages"]}

    # ---- factory events and the log --------------------------------------------------------------------
    def emit(self, part: str, event: str, subject: str, body: dict | None = None) -> int:
        """A factory part reports something that happened, for example a refused law or a failed scenario."""
        kind = f"factory.{_name(part, 'part')}.{_name(event, 'event')}"
        body = body or {}
        if len(_canonical(body)) > MAX_TEXT:
            raise CommonsError("event body too large")
        return self._write(kind, _text(subject, "subject")[:512], body, lambda c, s, a: None)

    def events(self, after: int = 0, kind_prefix: str = "", limit: int = 100) -> list[dict]:
        rows = self.db.execute(
            "SELECT seq, at, actor, kind, subject, body FROM events WHERE seq > ? AND kind LIKE ? ORDER BY seq LIMIT ?",
            (after, kind_prefix + "%", max(1, min(limit, 10000)))).fetchall()
        return [{"seq": s, "at": _iso(a), "actor": ac, "kind": k, "subject": su, "body": json.loads(b)}
                for s, a, ac, k, su, b in rows]

    def _checkpoints(self) -> list[tuple]:
        return self.db.execute("SELECT first, last, prev, head, segment, sha256, bytes, events, archived_seq "
                               "FROM checkpoints ORDER BY last").fetchall()

    def verify(self, archives: bool = False, anchors=None) -> dict:
        """Recompute the hash chain. Any edit, deletion or reordering of past events breaks it.

        Without archives the live events are checked from the latest checkpoint (from_seq says where). With
        archives every segment is checked too, so the whole history is recomputed from the first event.
        Anchors are (seq, hash) pairs kept outside the store: each one in the checked range must still match,
        which detects a history rewritten from the start or events removed from the end."""
        result = self._verify_chain(archives, {a["seq"]: a["hash"] for a in (anchors or [])})
        return result

    def _verify_chain(self, archives: bool, anchors: dict) -> dict:
        seen: dict[int, str] = {}

        def note(rows):
            for row in rows:
                if row[0] in anchors:
                    seen[row[0]] = row[7]

        checkpoints = self._checkpoints()
        archived = {c[1]: c for c in checkpoints}
        prev, first, checked = GENESIS, 1, 0
        if archives:
            for c_first, last, c_prev, head, segment, digest, _, events, _ in checkpoints:
                path = self.directory / ARCHIVE_DIR / segment
                if c_first != first or c_prev != prev:
                    return {"ok": False, "broken_at": c_first, "checked": checked, "reason": "checkpoints are not contiguous"}
                if not path.exists() or _sha256_file(path) != digest:
                    return {"ok": False, "broken_at": c_first, "checked": checked,
                            "reason": f"segment {segment} is missing or does not match its checkpoint"}
                try:
                    _, rows = _segment_rows(path)
                except (lzma.LZMAError, EOFError, ValueError, KeyError, CommonsError) as error:
                    return {"ok": False, "broken_at": c_first, "checked": checked,
                            "reason": f"segment {segment} is unreadable ({type(error).__name__})"}
                segment_check = _check_chain(rows, prev, first)
                if not segment_check["ok"]:
                    return dict(segment_check, checked=checked + segment_check["checked"])
                if segment_check["head"] != head or segment_check["checked"] != events or rows[-1][0] != last:
                    return {"ok": False, "broken_at": c_first, "checked": checked, "reason": f"segment {segment} ends elsewhere"}
                bad = self._archived_event_mismatch(rows, archived)
                if bad:
                    return {"ok": False, "broken_at": bad, "checked": checked, "reason": "an archive record disagrees with its checkpoint"}
                note(rows)
                prev, first, checked = head, last + 1, checked + events
        elif checkpoints:
            prev, first = checkpoints[-1][3], checkpoints[-1][1] + 1
        rows = self.db.execute("SELECT seq, at, actor, kind, subject, body, prev, hash FROM events ORDER BY seq").fetchall()
        result = _check_chain(rows, prev, first)
        if not result["ok"]:
            return dict(result, checked=checked + result["checked"])
        bad = self._archived_event_mismatch(rows, archived)
        if bad:
            return {"ok": False, "broken_at": bad, "checked": checked, "reason": "an archive record disagrees with its checkpoint"}
        note(rows)
        top = rows[-1][0] if rows else first - 1
        start = 1 if archives else first
        for seq, digest in sorted(anchors.items()):
            if seq > top:
                return {"ok": False, "broken_at": seq, "checked": checked + result["checked"],
                        "reason": f"anchor at event {seq} is past the newest event {top}: events were removed"}
            if seq >= start and seen.get(seq) != digest:
                return {"ok": False, "broken_at": seq, "checked": checked + result["checked"],
                        "reason": f"anchor at event {seq} does not match: the history was rewritten"}
        checked_anchors = sum(1 for seq in anchors if seq >= start)
        return {"ok": True, "checked": checked + result["checked"], "head": result["head"], "from_seq": start,
                "archives": len(checkpoints) if archives else 0, "anchors_checked": checked_anchors,
                "anchors_before_range": len(anchors) - checked_anchors}

    def head(self) -> dict:
        """The newest event's sequence number and hash, for an anchor kept outside the store."""
        row = self.db.execute("SELECT seq, hash FROM events ORDER BY seq DESC LIMIT 1").fetchone()
        return {"seq": row[0], "hash": row[1]} if row else {"seq": 0, "hash": GENESIS}

    @staticmethod
    def _archived_event_mismatch(rows, archived: dict) -> int | None:
        """Each store.archived event in the chain must match its checkpoint row: the chain binds every segment."""
        for seq, _, _, kind, _, body, _, _ in rows:
            if kind != "store.archived":
                continue
            b = json.loads(body)
            c = archived.get(b.get("last"))
            if not c or (c[0], c[2], c[3], c[4], c[5], c[6], c[7]) != (
                    b.get("first"), b.get("prev"), b.get("head"), b.get("segment"), b.get("sha256"), b.get("bytes"),
                    b.get("events")) or c[8] != seq:
                return seq
        return None

    def storage(self, budget_bytes: int = BUDGET_BYTES, max_live_events: int = MAX_LIVE_EVENTS) -> dict:
        """Sizes of the live store and its archive, and whether the live store is over its budget."""
        live = sum(p.stat().st_size for p in (self.directory / n for n in (DB_NAME, DB_NAME + "-wal")) if p.exists())
        folder = self.directory / ARCHIVE_DIR
        segments = sorted(folder.glob("events-*.jsonl.xz")) if folder.exists() else []
        low, high = self.db.execute("SELECT MIN(seq), MAX(seq) FROM events").fetchone()
        live_events = (high - low + 1) if low else 0
        record_chars = self.db.execute("SELECT COALESCE(SUM(LENGTH(title) + LENGTH(body)), 0) FROM records").fetchone()[0]
        checkpoints = self._checkpoints()
        return {"live_bytes": live, "budget_bytes": budget_bytes, "live_events": live_events,
                "max_live_events": max_live_events, "archived_through": checkpoints[-1][1] if checkpoints else 0,
                "archive_segments": len(segments), "archive_bytes": sum(p.stat().st_size for p in segments),
                "record_characters": record_chars,
                "over_budget": live > budget_bytes or live_events > max_live_events}

    def archive(self, keep_events: int = KEEP_EVENTS, if_over: bool = False, vacuum: bool = True,
                budget_bytes: int = BUDGET_BYTES, max_live_events: int = MAX_LIVE_EVENTS) -> dict:
        """Move all but the newest keep_events events into one compressed segment and record a checkpoint.

        The segment is written, read back and chain-checked before anything is removed. The removal is one
        transaction that also appends a store.archived event naming the segment's hash, so the live chain binds
        it. Read messages, broadcasts and messages to finished agents in the archived range leave the live
        message table (their text stays in the segment); unread messages to other agents stay. Leases stay
        until released. Records are knowledge and are never archived; each is capped at the text limit.
        Which events move is decided by sequence numbers and size, never by the clock."""
        if not isinstance(keep_events, int) or keep_events < 0:
            raise CommonsError("keep_events must be a non-negative number of events")
        before = self.storage(budget_bytes, max_live_events)
        if if_over and not before["over_budget"]:
            return {"archived": 0, "reason": "within budget", "storage": before}
        checkpoints = self._checkpoints()
        base, prev = (checkpoints[-1][1], checkpoints[-1][3]) if checkpoints else (0, GENESIS)
        top = self.db.execute("SELECT COALESCE(MAX(seq), 0) FROM events").fetchone()[0]
        cut = top - keep_events
        if cut <= base:
            return {"archived": 0, "reason": f"nothing older than the newest {keep_events} events", "storage": before}
        rows = self.db.execute("SELECT seq, at, actor, kind, subject, body, prev, hash FROM events "
                               "WHERE seq > ? AND seq <= ? ORDER BY seq", (base, cut)).fetchall()
        checked = _check_chain(rows, prev, base + 1)
        if not checked["ok"]:
            raise CommonsError(f"the event chain is broken at {checked['broken_at']}; archive refused")
        head = checked["head"]
        folder = self.directory / ARCHIVE_DIR
        _refuse_links(self.directory)
        folder.mkdir(exist_ok=True)
        name = f"events-{base + 1:012d}-{cut:012d}.jsonl.xz"
        path = folder / name
        temporary = folder / f".{name}.{os.getpid()}.{os.urandom(4).hex()}.tmp"
        try:
            with lzma.open(temporary, "wt", encoding="utf-8", preset=6) as out:
                out.write(_canonical({"format": ARCHIVE_FORMAT, "first": base + 1, "last": cut, "prev": prev,
                                      "head": head, "events": len(rows)}) + "\n")
                for row in rows:
                    out.write(_event_line(row) + "\n")
            with open(temporary, "rb") as stream:
                os.fsync(stream.fileno())
            os.replace(temporary, path)
        finally:
            temporary.unlink(missing_ok=True)
        header, back = _segment_rows(path)  # read back before removing anything
        again = _check_chain(back, prev, base + 1)
        if not again["ok"] or again["head"] != head or len(back) != len(rows) or header["head"] != head:
            path.unlink(missing_ok=True)
            raise CommonsError("the written segment does not read back to the same events; archive refused")
        body = {"first": base + 1, "last": cut, "events": len(rows), "prev": prev, "head": head, "segment": name,
                "sha256": _sha256_file(path), "bytes": path.stat().st_size}

        def apply(cursor, seq, at):
            self._require_registered(cursor)
            if (cursor.execute("SELECT COALESCE(MAX(last), 0) FROM checkpoints").fetchone()[0] != base):
                raise CommonsError("another archive ran meanwhile; run archive again")
            row = cursor.execute("SELECT hash FROM events WHERE seq=?", (cut,)).fetchone()
            if not row or row[0] != head:
                raise CommonsError("events changed during archive; archive refused")
            cursor.execute("INSERT INTO checkpoints VALUES(?,?,?,?,?,?,?,?,?)",
                           (cut, base + 1, prev, head, name, body["sha256"], body["bytes"], len(rows), seq))
            cursor.execute("DELETE FROM events WHERE seq <= ?", (cut,))
            removed = cursor.execute(
                "DELETE FROM messages WHERE seq <= ? AND (recipient = '*' "
                "OR seq <= COALESCE((SELECT read_through FROM agents a WHERE a.name = messages.recipient), 0) "
                "OR (SELECT state FROM agents a WHERE a.name = messages.recipient) = 'done')", (cut,)).rowcount
            return {"seq": seq, "archived": len(rows), "segment": name, "first": base + 1, "last": cut,
                    "messages_removed": removed}

        try:
            result = self._write("store.archived", name, body, apply)
        except BaseException:
            referenced = self.db.execute("SELECT 1 FROM checkpoints WHERE segment=?", (name,)).fetchone()
            if not referenced:
                path.unlink(missing_ok=True)
            raise
        if vacuum:
            try:
                self.db.execute("VACUUM")
                self.db.execute("PRAGMA wal_checkpoint(TRUNCATE)")
            except sqlite3.OperationalError as error:
                result["vacuum"] = f"skipped ({error}); freed pages are reused by later writes"
        result["storage"] = self.storage(budget_bytes, max_live_events)
        return result

    def export_events(self, with_archive: bool = False):
        """Event lines (seq, at, actor, kind, subject, body, prev, hash): enough to recompute the chain elsewhere."""
        if with_archive:
            for checkpoint in self._checkpoints():
                for row in _segment_rows(self.directory / ARCHIVE_DIR / checkpoint[4])[1]:
                    yield _event_line(row)
        for row in self.db.execute("SELECT seq, at, actor, kind, subject, body, prev, hash FROM events ORDER BY seq"):
            yield _event_line(row)

    def export_records(self, fmt: str = "jsonl"):
        """Every record with its links, as JSON lines or as Markdown grouped by type."""
        if fmt not in ("jsonl", "md"):
            raise CommonsError("format must be jsonl or md")
        rows = [self._row(r) for r in self.db.execute("SELECT * FROM records ORDER BY type, created_seq")]
        links = self.db.execute("SELECT src, rel, dst FROM links ORDER BY seq").fetchall()
        if fmt == "jsonl":
            for record in rows:
                yield _canonical(record)
            for src, rel, dst in links:
                yield _canonical({"link": [src, rel, dst]})
            return
        yield f"# Zeno Commons records\n\n{len(rows)} records and {len(links)} links.\n"
        current = None
        for record in rows:
            if record["type"] != current:
                current = record["type"]
                yield f"\n## {current}\n"
            meta = f"{record['status']} · by {record['author']}"
            if record["tags"]:
                meta += " · tags: " + ", ".join(record["tags"])
            yield f"\n### {record['id']}: {record['title']}\n\n{meta}\n"
            if record["body"].strip():
                yield "\n" + record["body"].rstrip() + "\n"
            if record["refs"]:
                yield "\nReferences: " + ", ".join(record["refs"]) + "\n"
        if links:
            yield "\n## Links\n\n" + "".join(f"- {s} {r} {d}\n" for s, r, d in links)

    def dashboard(self) -> dict:
        counts = {}
        for type_, status, n in self.db.execute("SELECT type, status, COUNT(*) FROM records GROUP BY type, status"):
            counts.setdefault(type_, {})[status] = n
        top = self.db.execute("SELECT COALESCE(MAX(seq), 0) FROM events").fetchone()[0]
        return {"format": FORMAT, "events": top, "agents": self.agents(), "leases": self.leases(),
                "records": counts, "unread_for_me": len(self.inbox()["messages"]), "storage": self.storage()}

    # ---- migration --------------------------------------------------------------------------------------
    def import_gaps(self, path: str | os.PathLike) -> dict:
        """Import a JSON-lines gap log (zenofcis-feature-gap/1). Idempotent: content decides each record ID."""
        added = existing = 0
        for line in Path(path).read_text().splitlines():
            if not line.strip():
                continue
            row = json.loads(line)
            if "schema" in row:
                continue
            title = str(row.get("need") or row.get("task") or "gap")[:300]
            body = "\n".join(f"{k}: {row[k]}" for k in ("task", "what_zenofcis_lacks", "workaround", "evidence") if row.get(k))
            clean = lambda v: re.sub(r"[^a-z0-9._:@/-]+", "-", str(v).lower()).strip("-._:@/")[:100] or "unknown"
            tags = [clean(row.get("agent", "unknown")), "severity-" + clean(row.get("severity", "unknown")), "feature-gap"]
            out = self.record_add("gap", title, body, tags=tags)
            added, existing = added + (not out["existing"]), existing + out["existing"]
        return {"added": added, "existing": existing}


def open_default(actor: str = "") -> Commons:
    """The store named by ZENO_FCIS_COMMONS, acting as ZENO_FCIS_COMMONS_ACTOR (or the given actor)."""
    directory = os.environ.get("ZENO_FCIS_COMMONS")
    if not directory:
        raise CommonsError("set ZENO_FCIS_COMMONS to the store directory")
    return Commons(directory, actor or os.environ.get("ZENO_FCIS_COMMONS_ACTOR", "anonymous"))
