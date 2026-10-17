"""Compare complete scalar decisions with the pinned library probe.

Numbers in channel 303 are observations; channels 300..302 are economic
intents. The framework independently binds ordinals, destinations and
idempotency values, which the text example format itself does not expose.
"""
from __future__ import annotations

import json
import subprocess
import time
from pathlib import Path

from build_contracts import APP, REASONS
from run_judges import PINNED_CLI
from spot_reference import Command, Decision, N, U, Pool, World, bounded, command_values, state_values


def expected_observation(decision):
    deliveries = tuple((e.channel, (e.source, e.destination, e.amount)) for e in decision.effects)
    if decision.reason is None: deliveries += ((303, decision.outputs),)
    return ("accept" if decision.reason is None else "reject",
            None if decision.reason is None else REASONS[decision.reason], state_values(decision.post), deliveries)


def parse_observation(example):
    sections = example.split("|")
    tokens = sections[-2].split()
    reason = None if tokens[1] == "-" else int(tokens[1])
    deliveries = () if sections[-1].strip() == "-" else tuple(
        (int(row.split()[0]), tuple(map(int, row.split()[1:]))) for row in sections[-1].split(";"))
    return tokens[0], reason, tuple(map(int, tokens[2:])), deliveries


def probe(world, command, expected, out, label, cli=PINNED_CLI, app=APP):
    out = Path(out)
    out.mkdir(parents=True, exist_ok=True)
    values = state_values(world) + command_values(command) + (1,)
    argv = [str(cli), "--no-kb", "contract", "probe", "explain", str(app), "--input", " ".join(map(str, values)),
            "--out", str(out / f"{label}.json")]
    started = time.monotonic()
    try:
        result = subprocess.run(argv, capture_output=True, text=True, timeout=20, check=False)
    except subprocess.TimeoutExpired as error:
        def as_text(value): return value.decode(errors="replace") if type(value) is bytes else value or ""
        (out / f"{label}.command.json").write_text(json.dumps({"argv": argv, "exit_code": 124,
            "status": "timeout-UNKNOWN", "seconds": round(time.monotonic() - started, 3),
            "stdout": as_text(error.stdout), "stderr": as_text(error.stderr)}, indent=2) + "\n")
        raise
    (out / f"{label}.command.json").write_text(json.dumps({"argv": argv, "exit_code": result.returncode,
        "seconds": round(time.monotonic() - started, 3), "stdout": result.stdout, "stderr": result.stderr}, indent=2) + "\n")
    if result.returncode != 0: raise ValueError(f"{label}: probe exit {result.returncode}: {result.stderr}")
    packet = json.loads((out / f"{label}.json").read_text())
    authority = packet["result"]["authority"]
    if "example" not in authority: raise ValueError(f"{label}: library refusal {authority}")
    actual = parse_observation(authority["example"])
    wanted = expected_observation(expected)
    if actual != wanted: raise ValueError(f"{label}: decision mismatch\nexpected {wanted}\nactual   {actual}")
    return packet


def world_json(world):
    p = world.pool
    return {"pool": None if p is None else [p.r0, p.r1, p.supply, p.status],
            "balances": list(world.balances), "shares": list(world.shares)}


def decode_world(value):
    if type(value) is not dict or set(value) != {"pool", "balances", "shares"}: raise ValueError("closed snapshot required")
    p = value["pool"]
    if p is not None and (type(p) is not list or len(p) != 4): raise ValueError("pool must be null or four integers")
    for field, count in (("balances", 8), ("shares", 3)):
        if type(value[field]) is not list or len(value[field]) != count or any(type(v) is not int for v in value[field]):
            raise ValueError(f"{field} must be {count} exact integers")
        if not all(bounded(v,U if field=="balances" else N) for v in value[field]):
            raise ValueError(f"{field} is outside the native profile; narrowing is forbidden")
    if p is not None and (not all(bounded(v,N) for v in p[:3]) or not bounded(p[3],2)):
        raise ValueError("pool is outside the native profile")
    try: return World(None if p is None else Pool(*p), tuple(value["balances"]), tuple(value["shares"]))
    except TypeError as error: raise ValueError(str(error)) from error


def decode_command(value):
    if type(value) is not dict or set(value) != set(Command.__dataclass_fields__) or any(type(v) is not int for v in value.values()):
        raise ValueError("DOMAIN: closed command of exact integers; fee/tier fields are forbidden")
    return Command(**value)


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result: raise ValueError(f"duplicate JSON key {key}")
        result[key] = value
    return result


def strict_json(text):
    def invalid(value): raise ValueError(f"non-integer JSON number {value}")
    return json.loads(text, object_pairs_hook=unique_object, parse_float=invalid, parse_constant=invalid)
