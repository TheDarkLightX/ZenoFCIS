"""Explicit offline candidate import. Source stores are opened read-only; no advice is written."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import sqlite3

MAX_ROWS = 4096
MAX_BYTES = 2 * 1024 * 1024
DIMENSIONS = {"tool", "version", "family", "environment", "capability", "prerequisite"}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def candidate(body, source_hash, reference, family):
    # No scope or strategy is inferred from prose or similarity.
    scope = body.get("scope", {})
    if type(scope) is not dict or not set(scope) <= DIMENSIONS:
        scope = {}
    normal = {}
    for key, values in sorted(scope.items()):
        if type(values) is str:
            values = [values]
        if type(values) is not list or not values or any(type(v) is not str or not v.strip() or len(v) > 96 for v in values):
            raise ValueError("scope-format")
        normal[key] = sorted({v.strip() for v in values})
    action = body.get("action", {"skip": [], "order": []})
    if type(action) is not dict or set(action) != {"skip", "order"}:
        raise ValueError("action-format")
    # Private imported statements remain inspectable even if no executable routing is proposed.
    statement = body.get("statement_sha256") or source_hash
    return {"id": "import-" + source_hash[:24], "statement_sha256": "", "subject": "pipeline",
            "claim": body.get("claim", "source-statement:" + str(statement)), "scope": normal, "effect": body.get("effect", "neutral"), "action": action,
            "proposing_tasks": body.get("proposing_tasks", []), "families": [family],
            "episodes": body.get("episodes", []), "parents": [], "dependencies": {}, "state": "proposed",
            "assessment": None, "observed_work": body.get("work", 0),
            "origin": "source:" + reference + ":" + str(statement)}


def commons(path):
    if path.is_symlink() or not path.is_file() or path.stat().st_size > 256 * 1024 * 1024:
        raise ValueError("commons-source-bound")
    connection = sqlite3.connect(path.resolve().as_uri() + "?mode=ro", uri=True)
    connection.execute("PRAGMA query_only=ON")
    rows = []
    # Pin the read transaction; concurrent coordination cannot change this snapshot.
    connection.execute("BEGIN")
    for record in connection.execute("SELECT id,type,body,author,updated_seq FROM records WHERE type IN ('lesson','hypothesis','claim') ORDER BY id LIMIT ?", (MAX_ROWS + 1,)):
        key, kind, text, actor, revision = record
        if len(rows) >= MAX_ROWS or len(text.encode()) > 65536:
            raise ValueError("commons-row-bound")
        try:
            body = json.loads(text)
        except (ValueError, TypeError):
            body = {"claim": " ".join(text.split())[:4096]}
        if type(body) is not dict:
            body = {"claim": " ".join(text.split())[:4096]}
        h = digest(text.encode())
        family = {"subject": body.get("subject_sha256", h), "session": str(revision), "actor": actor,
                  "tool": body.get("tool_version", "unknown"), "environment": body.get("environment", "unknown")}
        rows.append(candidate(body, h, "commons-record:" + key, family))
    failures = {}
    for actor, subject, text, event_hash in connection.execute("SELECT actor,subject,body,hash FROM events WHERE kind LIKE 'factory%' OR kind LIKE 'loop%' ORDER BY seq LIMIT ?", (MAX_ROWS + 1,)):
        if len(text.encode()) > 65536:
            raise ValueError("factory-event-bound")
        try:
            body = json.loads(text)
        except (ValueError, TypeError):
            continue
        if type(body) is not dict:
            continue
        reason = body.get("refusal_reason") or body.get("stopping_bound") or body.get("containment_refusal")
        door = body.get("door")
        problem_class = body.get("problem_class")
        subject_digest = body.get("subject_sha256")
        if not all(type(x) is str and x for x in (reason, door, problem_class, subject_digest)):
            continue
        signature = door, problem_class, reason
        failures.setdefault(signature, {})[subject_digest] = (body, event_hash, actor)
    connection.rollback()
    connection.close()
    for (door, problem_class, reason), subjects in sorted(failures.items()):
        if len(subjects) < 2:
            continue
        body = {"claim": "failure:" + door + ":" + reason, "scope": {"family": [problem_class]}, "effect": "limit" if "budget" in reason else "defect"}
        h = digest(json.dumps([door, problem_class, reason], separators=(",", ":")).encode())
        row = candidate(body, h, "factory-signature:" + h, {"subject": h, "session": "unknown", "actor": "unknown", "tool": "unknown", "environment": "unknown"})
        row["families"] = [{"subject": subject, "session": b.get("session", event), "actor": actor,
                            "tool": b.get("tool_version", "unknown"), "environment": b.get("environment", "unknown")}
                           for subject, (b, event, actor) in sorted(subjects.items())]
        row["origin"] += ":evidence:" + ",".join(event for _, event, _ in subjects.values())
        rows.append(row)
    return rows


def inbox(path):
    # An explicit single K1 proposal is read; directories are never scanned.
    data = path.read_bytes()
    if len(data) > 65536:
        raise ValueError("inbox-source-bound")
    h = digest(data)
    family = {"subject": h, "session": h, "actor": "unknown", "tool": "unknown", "environment": "unknown"}
    return [candidate({"claim": " ".join(data.decode("utf-8").split())[:4096]}, h, "inbox:" + h, family)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--commons-db", type=Path)
    source.add_argument("--inbox", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    rows = commons(args.commons_db) if args.commons_db else inbox(args.inbox)
    output = json.dumps({"schema": "zeno-fcis/kb-candidates/1", "candidates": rows, "isa": []}, separators=(",", ":")).encode()
    if len(output) > MAX_BYTES or len(rows) > 128:
        raise ValueError("candidate-output-bound")
    if args.out.parent.stat().st_mode & 0o077:
        raise ValueError("private-output-parent-required")
    descriptor = os.open(args.out, os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0), 0o600)
    with os.fdopen(descriptor, "wb") as file:
        file.write(output)
        file.flush()
        os.fsync(file.fileno())
    print(json.dumps({"status": "proposed", "path": str(args.out), "sha256": digest(output), "count": len(rows)}))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, sqlite3.Error):
        raise SystemExit("candidate-import-refused") from None
