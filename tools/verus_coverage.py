"""Inspect the pinned Verus VIR format, rather than infer contracts from counts.

This is a development coverage guard, not a proof checker. Verus checks the
contracts; a reviewed manifest fixes their translated content and inventory.
Unknown, malformed and changed records are refused, including added functions.
"""

from __future__ import annotations

import hashlib
import json


def parse_vir(source: str) -> list:
    """Read complete S-expressions, including comments and quoted atoms."""
    if len(source) > 8 * 1024 * 1024:
        raise ValueError("VIR log exceeds this profile's size limit")
    roots: list = []
    stack = [roots]
    index = 0
    while index < len(source):
        char = source[index]
        if char.isspace():
            index += 1
        elif source.startswith(";;", index):
            end = source.find("\n", index)
            index = len(source) if end < 0 else end + 1
        elif char == "(":
            if len(stack) >= 1024:
                raise ValueError("VIR nesting exceeds this profile's limit")
            child: list = []
            stack[-1].append(child)
            stack.append(child)
            index += 1
        elif char == ")":
            if len(stack) == 1:
                raise ValueError("unexpected closing VIR parenthesis")
            stack.pop()
            index += 1
        elif char == '"':
            start = index
            index += 1
            while index < len(source):
                if source[index] == "\\":
                    index += 2
                elif source[index] == '"':
                    index += 1
                    break
                else:
                    index += 1
            else:
                raise ValueError("unterminated quoted VIR atom")
            stack[-1].append(source[start:index])
        else:
            start = index
            while index < len(source) and not source[index].isspace() and source[index] not in "()":
                index += 1
            stack[-1].append(source[start:index])
    if len(stack) != 1 or any(not isinstance(node, list) for node in roots):
        raise ValueError("incomplete or unexpected top-level VIR record")
    return roots


def fingerprint(node: list) -> str:
    encoded = json.dumps(node, ensure_ascii=True, separators=(",", ":")).encode()
    return hashlib.sha256(encoded).hexdigest()


def inventory(source: str, namespace: str, body_functions: tuple[str, ...] = ()) -> dict:
    functions = {}
    for node in parse_vir(source):
        if not node or node[0] != "Function":
            continue
        if (len(node) < 4 or not isinstance(node[1], list)
                or len(node[1]) != 3 or node[1][:2] != ["Fun", ":path"]
                or not isinstance(node[1][2], str) or (len(node) - 2) % 2):
            raise ValueError("unexpected pinned VIR function format")
        name = node[1][2]
        if not name.startswith(namespace):
            continue
        fields = {}
        for key, value in zip(node[2::2], node[3::2]):
            if not isinstance(key, str) or not key.startswith(":") or key in fields:
                raise ValueError(f"duplicate or malformed VIR function field: {name}")
            fields[key] = value
        if set(fields) != {":mode", ":typ_bounds", ":params", ":ret", ":require", ":ensure", ":d", ":body"}:
            raise ValueError(f"unexpected pinned VIR function fields: {name}")
        mode = fields[":mode"]
        requires, ensures, body = fields[":require"], fields[":ensure"], fields[":body"]
        if (mode not in ("Exec", "Spec", "Proof") or not isinstance(requires, list)
                or not isinstance(ensures, list) or not isinstance(body, list) or not body):
            raise ValueError(f"missing contract fields or executable/specification body: {name}")
        if name in functions:
            raise ValueError(f"duplicate VIR function: {name}")
        functions[name] = {
            "mode": mode,
            "signature_sha256": fingerprint([fields[":typ_bounds"], fields[":params"], fields[":ret"]]),
            "requires": len(requires), "ensures": len(ensures),
            "requires_sha256": fingerprint(requires),
            "ensures_sha256": fingerprint(ensures),
        }
        if mode == "Spec" or name in body_functions:
            functions[name]["body_sha256"] = fingerprint(body)
    if not functions:
        raise ValueError("VIR log has no functions from the required namespace")
    return dict(sorted(functions.items()))


def require_coverage(source: str, profile: dict) -> dict:
    if (not isinstance(profile, dict) or not isinstance(profile.get("namespace"), str)
            or not profile["namespace"].endswith("::")
            or not isinstance(profile.get("functions"), dict) or not profile["functions"]):
        raise ValueError("malformed VIR coverage profile")
    body_functions = profile.get("body_covered_functions", [])
    if (not isinstance(body_functions, list)
            or any(not isinstance(name, str) for name in body_functions)
            or len(set(body_functions)) != len(body_functions)):
        raise ValueError("malformed operational body coverage")
    observed = inventory(source, profile["namespace"], tuple(body_functions))
    if any(name not in observed or observed[name]["mode"] != "Exec" for name in body_functions):
        raise ValueError("operational body coverage names a missing or non-runtime function")
    expected = profile["functions"]
    if set(observed) != set(expected):
        missing = sorted(set(expected) - set(observed))
        added = sorted(set(observed) - set(expected))
        raise ValueError(f"VIR function inventory changed: missing={missing}, added={added}")
    changed = [name for name in expected if observed[name] != expected[name]]
    if changed:
        raise ValueError(f"translated contracts or specifications changed: {changed}")
    for name, function in observed.items():
        if function["mode"] == "Exec" and (function["requires"] != 0 or function["ensures"] == 0):
            raise ValueError(f"runtime function has a narrowed domain or no postcondition: {name}")
    return observed
