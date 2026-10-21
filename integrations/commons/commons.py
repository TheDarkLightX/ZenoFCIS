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
CREATE TABLE IF NOT EXISTS links(
  src TEXT NOT NULL, rel TEXT NOT NULL, dst TEXT NOT NULL, seq INTEGER NOT NULL, PRIMARY KEY(src, rel, dst));
"""


class Commons:
    """One store directory. Safe for many processes: every write is one immediate transaction."""

    def __init__(self, directory: str | os.PathLike, actor: str, clock=_now):
        self.directory = Path(directory)
        self.actor = _name(actor, "actor")
        self.clock = clock
        self.directory.mkdir(parents=True, exist_ok=True)
        self.db = sqlite3.connect(self.directory / DB_NAME, timeout=30.0, isolation_level=None)
        self.db.execute("PRAGMA journal_mode=WAL")
        self.db.execute("PRAGMA busy_timeout=30000")
        self.db.execute("PRAGMA synchronous=NORMAL")
        self.db.executescript(SCHEMA)
        self.db.execute("INSERT OR IGNORE INTO meta(key, value) VALUES('format', ?)", (FORMAT,))
        found = self.db.execute("SELECT value FROM meta WHERE key='format'").fetchone()[0]
        if found != FORMAT:
            raise CommonsError(f"store format {found} is not {FORMAT}")

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

    # ---- coordination: leases -------------------------------------------------------------------------
    def _active_leases(self, cursor, at: float):
        return cursor.execute("SELECT resource, owner, note, expires_at FROM leases WHERE expires_at > ?",
                              (at,)).fetchall()

    def lease_take(self, resource: str, ttl_seconds: float = 3600.0, note: str = "") -> dict:
        resource = normalize_resource(resource)
        if not (60 <= ttl_seconds <= 7 * 86400):
            raise CommonsError("ttl_seconds must be between 60 and 604800")
        note = _text(note, "note", allow_empty=True)

        def apply(cursor, seq, at):
            for held, owner, _, expires in self._active_leases(cursor, at):
                if owner != self.actor and _covers(held, resource):
                    raise CommonsError(f"{resource} overlaps {held}, leased by {owner} until {_iso(expires)}")
            cursor.execute("DELETE FROM leases WHERE resource=?", (resource,))
            cursor.execute("INSERT INTO leases VALUES(?,?,?,?,?,?)",
                           (resource, self.actor, note, seq, at, at + ttl_seconds))
            return {"seq": seq, "resource": resource, "owner": self.actor, "expires": _iso(at + ttl_seconds)}

        return self._write("lease.take", resource, {"ttl_seconds": ttl_seconds, "note": note}, apply)

    def lease_renew(self, resource: str, ttl_seconds: float = 3600.0) -> dict:
        resource = normalize_resource(resource)

        def apply(cursor, seq, at):
            row = cursor.execute("SELECT owner, expires_at FROM leases WHERE resource=?", (resource,)).fetchone()
            if not row or row[1] <= at:
                raise CommonsError(f"no active lease on {resource}; take it again")
            if row[0] != self.actor:
                raise CommonsError(f"{resource} is leased by {row[0]}")
            cursor.execute("UPDATE leases SET heartbeat_at=?, expires_at=? WHERE resource=?",
                           (at, at + ttl_seconds, resource))
            return {"seq": seq, "resource": resource, "expires": _iso(at + ttl_seconds)}

        return self._write("lease.renew", resource, {"ttl_seconds": ttl_seconds}, apply)

    def lease_release(self, resource: str, force_reason: str = "") -> dict:
        resource = normalize_resource(resource)

        def apply(cursor, seq, at):
            row = cursor.execute("SELECT owner FROM leases WHERE resource=?", (resource,)).fetchone()
            if not row:
                raise CommonsError(f"no lease on {resource}")
            if row[0] != self.actor and not force_reason.strip():
                raise CommonsError(f"{resource} is leased by {row[0]}; only its owner releases it, or give a force reason")
            cursor.execute("DELETE FROM leases WHERE resource=?", (resource,))
            return {"seq": seq, "resource": resource, "released_from": row[0]}

        return self._write("lease.release", resource, {"force_reason": force_reason}, apply)

    def lease_check(self, resource: str) -> list[dict]:
        """Active leases that cover the resource or lie inside it. Empty means it is free to change."""
        resource = normalize_resource(resource)
        at = self.clock()
        return [{"resource": held, "owner": owner, "note": note, "expires": _iso(expires)}
                for held, owner, note, expires in self._active_leases(self.db, at) if _covers(held, resource)]

    def leases(self) -> list[dict]:
        at = self.clock()
        return [{"resource": r, "owner": o, "note": n, "expires": _iso(e)}
                for r, o, n, e in self._active_leases(self.db, at)]

    # ---- coordination: agent lifecycle ------------------------------------------------------------------
    def status(self, state: str, step: str = "", question: str = "") -> dict:
        if state not in AGENT_STATES:
            raise CommonsError(f"state must be one of {', '.join(AGENT_STATES)}")
        step, question = _text(step, "step", True), _text(question, "question", True)
        if state == "blocked" and not question.strip():
            raise CommonsError("a blocked status needs the question that blocks it")

        def apply(cursor, seq, at):
            if state == "done":
                held = [r for r, o, _, _ in self._active_leases(cursor, at) if o == self.actor]
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
        rows = self.db.execute("SELECT name, state, step, question, updated_at, read_through FROM agents ORDER BY name")
        return [{"agent": n, "state": s, "step": st, "question": q, "updated": _iso(u), "read_through": r}
                for n, s, st, q, u, r in rows]

    # ---- coordination: messages -------------------------------------------------------------------------
    def send(self, recipient: str, body: str) -> dict:
        recipient = recipient if recipient == "*" else _name(recipient, "recipient")
        body = _text(body, "message")

        def apply(cursor, seq, at):
            cursor.execute("INSERT INTO messages VALUES(?,?,?,?,?)", (seq, self.actor, recipient, body, at))
            return {"seq": seq, "to": recipient}

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

    def record_update(self, record_id: str, status: str = "", note: str = "", refs=None) -> dict:
        if status and status not in RECORD_STATUSES:
            raise CommonsError(f"status must be one of {', '.join(RECORD_STATUSES)}")
        note = _text(note, "note", True)
        refs = refs or []

        def apply(cursor, seq, at):
            row = cursor.execute("SELECT status, refs, body FROM records WHERE id=?", (record_id,)).fetchone()
            if not row:
                raise CommonsError(f"no record {record_id}")
            new_status = status or row[0]
            new_refs = sorted(set(json.loads(row[1])) | set(refs))
            new_body = row[2] + (f"\n[{_iso(at)} {self.actor}] {note}" if note.strip() else "")
            cursor.execute("UPDATE records SET status=?, refs=?, body=?, updated_seq=? WHERE id=?",
                           (new_status, _canonical(new_refs), new_body, seq, record_id))
            return {"seq": seq, "id": record_id, "status": new_status}

        return self._write("record.update", record_id, {"status": status, "note": note, "refs": refs}, apply)

    def link(self, src: str, rel: str, dst: str) -> dict:
        if rel not in LINK_RELATIONS:
            raise CommonsError(f"relation must be one of {', '.join(LINK_RELATIONS)}")

        def apply(cursor, seq, at):
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

    def verify(self) -> dict:
        """Recompute the hash chain. Any edit, deletion or reordering of past events breaks it."""
        prev, count = "0" * 64, 0
        for seq, at, actor, kind, subject, body, stored_prev, digest in self.db.execute(
                "SELECT seq, at, actor, kind, subject, body, prev, hash FROM events ORDER BY seq"):
            expected = hashlib.sha256((prev + _canonical([at, actor, kind, subject, json.loads(body)])).encode()).hexdigest()
            if stored_prev != prev or digest != expected:
                return {"ok": False, "broken_at": seq, "checked": count}
            prev, count = digest, count + 1
        return {"ok": True, "checked": count, "head": prev}

    def dashboard(self) -> dict:
        counts = {}
        for type_, status, n in self.db.execute("SELECT type, status, COUNT(*) FROM records GROUP BY type, status"):
            counts.setdefault(type_, {})[status] = n
        top = self.db.execute("SELECT COALESCE(MAX(seq), 0) FROM events").fetchone()[0]
        return {"format": FORMAT, "events": top, "agents": self.agents(), "leases": self.leases(),
                "records": counts, "unread_for_me": len(self.inbox()["messages"])}

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
            tags = [str(row.get("agent", "unknown")), "severity-" + str(row.get("severity", "unknown")).lower(), "feature-gap"]
            out = self.record_add("gap", title, body, tags=tags)
            added, existing = added + (not out["existing"]), existing + out["existing"]
        return {"added": added, "existing": existing}


def open_default(actor: str = "") -> Commons:
    """The store named by ZENO_FCIS_COMMONS, acting as ZENO_FCIS_COMMONS_ACTOR (or the given actor)."""
    directory = os.environ.get("ZENO_FCIS_COMMONS")
    if not directory:
        raise CommonsError("set ZENO_FCIS_COMMONS to the store directory")
    return Commons(directory, actor or os.environ.get("ZENO_FCIS_COMMONS_ACTOR", "anonymous"))
