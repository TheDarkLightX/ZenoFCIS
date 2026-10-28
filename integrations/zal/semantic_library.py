"""Admit the reservation fragment of the included finite semantic library.

Packages are closed JSON data. They cannot install code, evidence, imports or
approval. The existing ZAL parser/checker and factory remain the execution path.
This adapter preserves the complete one-step decision, reason and successor;
resource ownership remains a declared assumption, retained in the report.
"""
from __future__ import annotations

import json
import re
from hashlib import sha256
from itertools import product

import behavior
from behavior import Refusal

PROFILE = "zal/semantic-library-finite-enum/1"
MAX_BYTES = 1 << 20
NAME = re.compile(r"[a-z][a-z0-9_]{0,47}\Z")
NATIVE_NAME = re.compile(r"[a-z][a-z0-9_]{0,31}\Z")
RESERVED = {"none", "true", "false", "state", "not", "and", "or", "is", "default"}
INPUT_PORTS = (("state", "lifecycle", "control"),
               ("event", "operation", "control"),
               ("authorized", "authorization_observation", "boolean"))
OUTPUT_PORTS = (("outcome", "business_decision", "control"),
                ("next_state", "lifecycle", "control"),
                ("reason", "decision_reason", "control"))


def need(condition, message):
    if not condition:
        raise Refusal("semantic-library: " + message)


def _keys(value, keys, label):
    need(type(value) is dict and set(value) == set(keys), f"invalid {label} fields")


def _name(value, native=False):
    pattern = NATIVE_NAME if native else NAME
    need(type(value) is str and pattern.fullmatch(value) is not None,
         "identifier outside the supported profile")
    if native:
        need(value not in RESERVED, "reserved ZAL identifier")


def _names(value, maximum, label, native=False):
    need(type(value) is list and 1 <= len(value) <= maximum, f"invalid {label} domain")
    for item in value:
        _name(item, native)
    need(value == sorted(set(value)), f"{label} must be sorted and distinct")
    return value


def _ports(value, expected):
    need(type(value) is list and len(value) == len(expected), "unsupported port layout")
    for port, (name, role, unit) in zip(value, expected):
        _keys(port, ("name", "role", "unit", "values"), "port")
        need((port["name"], port["role"], port["unit"]) == (name, role, unit),
             "unsupported port name, semantic role or unit")
        _names(port["values"], 16, name)
    return [port["values"] for port in value]


def _unique(pairs):
    result = {}
    for key, value in pairs:
        need(key not in result, "duplicate JSON key")
        result[key] = value
    return result


def _constant(value):
    raise Refusal("semantic-library: non-JSON numeric constant")


def load(source):
    """Read one bounded, duplicate-free package, without executing its contents."""
    need(type(source) is str, "source must be JSON text")
    need(len(source) <= MAX_BYTES, "source exceeds 1 MiB")
    try:
        need(len(source.encode("utf-8")) <= MAX_BYTES, "source exceeds 1 MiB")
        return json.loads(source, object_pairs_hook=_unique, parse_constant=_constant)
    except (ValueError, UnicodeError, RecursionError) as error:
        if isinstance(error, Refusal):
            raise
        raise Refusal("semantic-library: malformed JSON") from error


def admit(source, machine="reservation"):
    """Return a new ZAL model and its checked source correspondence record.

    Supports total, deterministic, effect-free reservation tables with a single
    control-state link and Boolean authorization observation. Restricted domains,
    nondeterminism and changed roles/units refuse; they are never filled in.
    """
    data = load(source)
    _keys(data, ("profile", "inputs", "outputs", "rows", "state", "owns"), "package")
    need(data["profile"] == PROFILE, "unsupported profile")
    _name(machine, native=True)
    states, events, auth = _ports(data["inputs"], INPUT_PORTS)
    outcomes, successors, reasons = _ports(data["outputs"], OUTPUT_PORTS)
    _names(states, 8, "state", native=True)
    _names(events, 8, "event", native=True)
    for reason in reasons:
        _name(reason, native=True)
    need(auth == ["no", "yes"] and outcomes == ["accept", "reject"], "unsupported decision or context encoding")
    need(successors == states, "successor domain differs from state domain")
    links = data["state"]
    need(type(links) is list and len(links) == 1 and type(links[0]) is list
         and len(links[0]) == 3 and links[0][:2] == ["state", "next_state"]
         and type(links[0][2]) is str and links[0][2] in states, "invalid state link or initial state")
    initial = links[0][2]
    owns = data["owns"]
    need(type(owns) is list and len(owns) <= 16, "invalid ownership declarations")
    for resource in owns:
        _name(resource)
    need(owns == sorted(set(owns)), "ownership declarations must be sorted and distinct")
    expected = list(product(states, events, auth))
    need(len(expected) <= 32, "native profile supports at most 32 table rows")
    rows = data["rows"]
    need(type(rows) is list and len(rows) == len(expected), "requires the full Cartesian input domain")
    table = {}
    for row, x in zip(rows, expected):
        _keys(row, ("input", "outputs"), "row")
        need(type(row["input"]) is list and row["input"] == list(x), "missing, duplicate or noncanonical input row")
        choices = row["outputs"]
        need(type(choices) is list and len(choices) == 1, "requires exactly one output per input")
        y = choices[0]
        need(type(y) is list and len(y) == 3 and all(type(v) is str for v in y), "invalid output tuple")
        decision, target, reason = y
        need(decision in outcomes and target in states and reason in reasons, "output outside declared domain")
        need((decision == "accept" and reason == "ok") or (decision == "reject" and target == x[0]),
             "native observation mapping would lose meaning")
        table[x] = tuple(y)
    lines = ["zal 1;", f"machine {machine};", "states " + ", ".join(states) + ";",
             "events " + ", ".join(events) + ";", "context authorized;", f"initial {initial};",
             "step atomic; frame control_only;", "default reject unmatched unchanged;"]
    for index, ((state, event, auth), (decision, target, reason)) in enumerate(table.items()):
        guard = "authorized" if auth == "yes" else "!authorized"
        outcome = f"accept {target}" if decision == "accept" else f"reject {reason} unchanged"
        lines.append(f"rule r{index}: {state} + {event} [{guard}] -> {outcome};")
    model = behavior.parse("\n".join(lines) + "\n")
    checked = behavior.check(model)
    need(checked["status"] == "pass-with-scope", "emitted model failed its ZAL check")
    # Do not use the emitted rules as the expected table. Compare every native
    # result directly with the incoming contract, including explicit refusal.
    for (state, event, auth), expected_output in table.items():
        actual = model.decide(state, event, {"authorized": auth == "yes"})
        observed = (actual["class"].lower(), actual["state"],
                    "ok" if actual["class"] == "Accept" else actual["reason"])
        need(observed == expected_output, f"lowering mismatch at {(state, event, auth)!r}")
    report = {
        "schema": "zal/semantic-library-correspondence/1", "status": "pass-with-scope",
        "authority": "none", "human_agreement": "pending", "native_fcis_qualification": "not-performed",
        "source_profile": PROFILE, "source_revision": sha256(behavior.canonical(data).encode()).hexdigest(),
        "source_sha256": sha256(source.encode("utf-8")).hexdigest(), "source_text": source,
        "source_contract": data, "model_revision": model.revision, "profile": behavior.PROFILE,
        "input_tuples": len(table), "observations": ["decision", "reason", "successor", "empty deliveries"],
        "initial_state": initial, "symbolic": model.render(), "english": model.render(True),
        "assumptions": {"authorized": "supplied Boolean observation, not authenticated identity",
                        "owns": owns, "ownership": "declared resources, not authenticated ownership"},
        "check": checked,
        "nonclaims": ["unbounded identity history", "external effects", "shared-resource composition",
                      "human intent", "source-level formal proof", "publication authority"],
    }
    return model, report


def admit_library(library, entry_id, expected_revision, machine="reservation"):
    """Resolve a pinned finite entry, then independently check its lowering.

    Retrieval and finite fit are separate operations. This accepts no executable
    plugin and does not convert a library selection into human agreement.
    """
    from finite_library.prototype import Library, Refused, canonical

    need(type(library) is Library, "requires the finite-library profile")
    need(type(expected_revision) is str
         and re.fullmatch(r"[a-f0-9]{64}", expected_revision) is not None,
         "requires an exact semantic entry revision")
    try:
        entry = library.resolve(entry_id, expected_revision)
    except Refused as error:
        raise Refusal("semantic-library: " + str(error)) from error
    model, report = admit(canonical(entry.meaning.data()).decode("ascii"), machine)
    report["library_entry"] = {"id": entry.id, "revision": entry.revision,
                               "dependencies": [list(item) for item in entry.dependencies]}
    return model, report
