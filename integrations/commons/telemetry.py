"""Import optional CLI observations and summarize consecutive subject episodes.

Imported observations are untrusted reports, not checker evidence. Deduplication
uses the hash of the exact JSON line (without its line terminator), retained in
the store's existing meta table in the same transaction as its event.
"""
from datetime import datetime
import hashlib
import json
import re
from statistics import mean

from commons import CommonsError, _Existing

SCHEMA = "zeno-fcis/telemetry/1"
HEX = re.compile(r"^[0-9a-f]{64}$")
WORD = re.compile(r"^[a-z][a-z0-9_-]{0,127}$")


def validate(row):
    if not isinstance(row, dict) or row.get("schema") != SCHEMA:
        raise CommonsError("expected zeno-fcis/telemetry/1")
    required = {"schema", "timestamp", "actor", "command", "inputs", "subject_sha256", "exit_code", "verdict", "work_units", "duration_ms"}
    if set(row) != required:
        raise CommonsError("telemetry fields differ from schema")
    if not isinstance(row["actor"], str) or not 1 <= len(row["actor"]) <= 128:
        raise CommonsError("invalid telemetry actor")
    command = row["command"]
    if not isinstance(command, list) or len(command) > 16 or not all(isinstance(x, str) and WORD.fullmatch(x) for x in command):
        raise CommonsError("invalid telemetry command path")
    if row["verdict"] not in (None, "accepted", "refuted", "unknown", "refused"):
        raise CommonsError("invalid telemetry verdict")
    for name in ("exit_code", "duration_ms"):
        if type(row[name]) is not int or row[name] < 0:
            raise CommonsError(f"invalid telemetry {name}")
    try:
        if not isinstance(row["timestamp"], str) or not row["timestamp"].endswith("Z"):
            raise ValueError()
        datetime.fromisoformat(row["timestamp"].replace("Z", "+00:00"))
    except ValueError as error:
        raise CommonsError("telemetry time must be UTC RFC3339") from error
    inputs = row["inputs"]
    if not isinstance(inputs, list) or len(inputs) > 1024:
        raise CommonsError("invalid telemetry inputs")
    for item in inputs:
        if not isinstance(item, dict) or set(item) != {"name", "sha256"} or not isinstance(item["name"], str):
            raise CommonsError("telemetry inputs hold names and digests only")
        if item["sha256"] is not None and (not isinstance(item["sha256"], str) or not HEX.fullmatch(item["sha256"])):
            raise CommonsError("invalid input sha256")
    digests = sorted({item["sha256"] for item in inputs if item["sha256"]})
    expected = hashlib.sha256("\n".join(digests).encode()).hexdigest() if digests else None
    if row["subject_sha256"] != expected:
        raise CommonsError("telemetry subject differs from its input digest set")
    work = row["work_units"]
    if work is not None and (not isinstance(work, dict) or not all(isinstance(key, str) and type(value) is int and value >= 0 for key, value in work.items())):
        raise CommonsError("telemetry work units must be nonnegative numeric observations")
    return row


def import_telemetry(store, path):
    imported = duplicate = 0
    with open(path, "rb") as source:
        for number, raw in enumerate(source, 1):
            if len(raw) > 65536 or not raw.endswith(b"\n"):
                raise CommonsError(f"telemetry line {number} is oversized or incomplete; earlier lines retained")
            line = raw.rstrip(b"\r\n")
            try:
                row = validate(json.loads(line))
            except (ValueError, TypeError) as error:
                raise CommonsError(f"invalid telemetry line {number}; earlier lines retained: {error}") from error
            digest = hashlib.sha256(line).hexdigest()
            key = "telemetry/1/" + digest
            def apply(cursor, seq, at):
                if cursor.execute("SELECT 1 FROM meta WHERE key=?", (key,)).fetchone():
                    raise _Existing()
                cursor.execute("INSERT INTO meta(key,value) VALUES(?,?)", (key, str(seq)))
            try:
                store._write("factory.cli." + (".".join(row["command"]) or "usage"),
                             row["subject_sha256"] or "unknown", {"line_sha256": digest, "telemetry": row}, apply)
                imported += 1
            except _Existing:
                duplicate += 1
    return {"imported": imported, "duplicates": duplicate, "authority": "none"}


def summarize(rows):
    """Episodes are maximal consecutive actor/subject runs; measure the first acceptance.

    A changed actor OR subject splits an episode. Different commands may occur
    inside it. Missing subjects form singletons. Incomplete episodes contribute
    runs/failures but never an invented acceptance or duration.
    """
    episodes = []
    current = []
    for row in rows:
        if current and (row["actor"], row["subject_sha256"]) != (current[-1]["actor"], current[-1]["subject_sha256"]):
            episodes.append(current); current = []
        current.append(row)
        if row["subject_sha256"] is None:
            episodes.append(current); current = []
    if current:
        episodes.append(current)
    commands = {}
    for episode in episodes:
        for command in sorted({" ".join(row["command"]) for row in episode}):
            selected = [row for row in episode if " ".join(row["command"]) == command]
            first_accept = next((index for index, row in enumerate(selected) if row["verdict"] == "accepted"), None)
            accepted = first_accept is not None
            prefix = selected[:first_accept + 1] if accepted else selected
            start = datetime.fromisoformat(selected[0]["timestamp"].replace("Z", "+00:00"))
            end = datetime.fromisoformat(prefix[-1]["timestamp"].replace("Z", "+00:00"))
            elapsed = max(0, round((end - start).total_seconds() * 1000) + prefix[-1]["duration_ms"])
            stats = commands.setdefault(command, {"command": command, "runs": 0, "failures": 0,
                "accepted_episodes": 0, "iterations": [], "times": []})
            stats["runs"] += len(selected)
            stats["failures"] += sum(row["exit_code"] != 0 or row["verdict"] in ("refuted", "unknown", "refused") for row in selected)
            if accepted:
                stats["accepted_episodes"] += 1
                stats["iterations"].append(len(prefix))
                stats["times"].append(elapsed)
    result = []
    for command, stats in sorted(commands.items()):
        iterations, times = stats.pop("iterations"), stats.pop("times")
        result.append({**stats, "failure_rate": stats["failures"] / stats["runs"],
            "iterations_to_accept": mean(iterations) if iterations else None,
            "time_to_accept_ms": mean(times) if times else None})
    return {"schema": "zeno-fcis/commons-loops/1", "authority": "none", "episodes": len(episodes), "commands": result}


def loops(store):
    rows = []
    for event in store.export_events(with_archive=True):
        event = json.loads(event)
        if not event["kind"].startswith("factory.cli."):
            continue
        body = json.loads(event["body"]) if isinstance(event["body"], str) else event["body"]
        if isinstance(body, dict) and "telemetry" in body:
            rows.append(validate(body["telemetry"]))
    return summarize(rows)
