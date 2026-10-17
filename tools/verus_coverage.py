"""Inspect the pinned Verus VIR format, rather than infer contracts from counts.

This is a development coverage guard, not a proof checker. Verus checks the
contracts; a reviewed manifest fixes their translated content and inventory.
Unknown, malformed and changed records are refused, including added functions.
"""

from __future__ import annotations

import hashlib
import json

# Exact evaluator source arrays are translated as byte literals in the
# authority subject. Keep a finite parser bound large enough for that complete
# closure; this does not change contract, inventory or body comparisons.
MAX_VIR_CHARACTERS = 32 * 1024 * 1024


def parse_vir(source: str) -> list:
    """Read complete S-expressions, including comments and quoted atoms."""
    if len(source) > MAX_VIR_CHARACTERS:
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


def _inventory(source: str, namespace: str, body_functions: tuple[str, ...],
               declaration_modes: dict[str, str], nodes=None) -> tuple[dict, dict]:
    functions = {}
    declarations = {}
    for node in parse_vir(source) if nodes is None else nodes:
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
                or not isinstance(ensures, list)):
            raise ValueError(f"missing contract fields or executable/specification body: {name}")
        if name in functions or name in declarations:
            raise ValueError(f"duplicate VIR function: {name}")
        record = {
            "mode": mode,
            "signature_sha256": fingerprint([fields[":typ_bounds"], fields[":params"], fields[":ret"]]),
            "requires": len(requires), "ensures": len(ensures),
            "requires_sha256": fingerprint(requires),
            "ensures_sha256": fingerprint(ensures),
        }
        if name in declaration_modes:
            if (body != "None" or mode != declaration_modes[name] or requires
                    or len(ensures) != (1 if mode == "Exec" else 0)):
                raise ValueError(f"unexpected descriptor declaration contract: {name}")
            declarations[name] = {**record, "kind": "abstract-declaration",
                                  "raw_vir": node, "raw_vir_sha256": fingerprint(node)}
            continue
        if not isinstance(body, list) or not body:
            raise ValueError(f"missing contract fields or executable/specification body: {name}")
        functions[name] = record
        if mode == "Spec" or name in body_functions:
            functions[name]["body_sha256"] = fingerprint(body)
    if not functions:
        raise ValueError("VIR log has no functions from the required namespace")
    if set(declarations) != set(declaration_modes):
        raise ValueError("missing descriptor declaration")
    return dict(sorted(functions.items())), dict(sorted(declarations.items()))


def inventory(source: str, namespace: str, body_functions: tuple[str, ...] = (), *, _nodes=None) -> dict:
    """Strict concrete-body inventory; abstract declarations are refused."""
    return _inventory(source, namespace, body_functions, {}, _nodes)[0]


def descriptor_inventory(source: str, namespace: str, descriptor_prefix: str,
                         body_functions: tuple[str, ...] = (), *, _nodes=None) -> tuple[dict, dict]:
    """Retain the two closed descriptor declarations separately from real bodies.

    The pinned VIR names both sealed implementations explicitly. Missing bodies,
    a changed declaration shape, or any other bodyless function still refuse.
    Raw declaration records are retained without rewriting their fingerprints.
    """
    if not descriptor_prefix.startswith(namespace) or not descriptor_prefix.endswith("::"):
        raise ValueError("descriptor prefix is outside the required namespace")
    declarations = {descriptor_prefix + "DescriptorData::model": "Spec",
                    descriptor_prefix + "DescriptorData::project": "Exec"}
    # The pinned production VIR assigns slots 0/1 to the inherent projection
    # methods and slots 2/3 to the closed Legacy/Numeric trait implementations.
    # Keep those concrete bodies mandatory; a changed slot needs source review.
    implementations = {descriptor_prefix + f"impl&%{index}::{method}": mode
                       for index in (2, 3) for method, mode in (("model", "Spec"), ("project", "Exec"))}
    bodies = tuple(dict.fromkeys((*body_functions, *implementations)))
    functions, abstract = _inventory(source, namespace, bodies, declarations, _nodes)
    for name, mode in implementations.items():
        record = functions.get(name)
        if (record is None or record["mode"] != mode or "body_sha256" not in record
                or record["requires"] != 0 or (mode == "Exec" and record["ensures"] != 1)):
            raise ValueError(f"missing or changed concrete descriptor implementation: {name}")
    return functions, abstract


DESCRIPTOR_MODULE = "execution_v2::composition::descriptor::"


def observed_inventory(source: str, namespace: str,
                       body_functions: tuple[str, ...] = ()) -> tuple[dict, dict | None]:
    """Fresh inventory for a profile; abstract declarations are None when absent.

    A namespace that holds the closed descriptor module retains its two
    declarations separately, exactly as `descriptor_inventory` does; any other
    namespace uses the strict concrete-body inventory.
    """
    prefix = namespace + DESCRIPTOR_MODULE
    names = {node[1][2] for node in parse_vir(source)
             if node and node[0] == "Function" and isinstance(node[1], list) and len(node[1]) == 3}
    if prefix + "DescriptorData::project" in names or prefix + "DescriptorData::model" in names:
        return descriptor_inventory(source, namespace, prefix, body_functions)
    return inventory(source, namespace, body_functions), None


def profile_inventory(source: str, profile: dict) -> dict:
    """Concrete inventory under the profile's own declaration rule."""
    bodies = tuple(profile["body_covered_functions"])
    if "abstract_declarations" in profile:
        return descriptor_inventory(source, profile["namespace"],
                                    profile["namespace"] + DESCRIPTOR_MODULE, bodies)[0]
    return inventory(source, profile["namespace"], bodies)


def require_coverage(source: str, profile: dict) -> dict:
    if (not isinstance(profile, dict) or not isinstance(profile.get("namespace"), str)
            or not profile["namespace"].endswith("::")
            or not isinstance(profile.get("functions"), dict) or not profile["functions"]):
        raise ValueError("malformed VIR coverage profile")
    nodes = None
    if "compiler_id_reference" in profile:
        from verus_ids import restore
        nodes = restore(parse_vir(source), profile["namespace"], profile["compiler_id_reference"])
    body_functions = profile.get("body_covered_functions", [])
    if (not isinstance(body_functions, list)
            or any(not isinstance(name, str) for name in body_functions)
            or len(set(body_functions)) != len(body_functions)):
        raise ValueError("malformed operational body coverage")
    if "abstract_declarations" in profile:
        declarations = profile["abstract_declarations"]
        if not isinstance(declarations, dict):
            raise ValueError("malformed descriptor declaration coverage")
        prefix = profile["namespace"] + "execution_v2::composition::descriptor::"
        observed, actual_declarations = descriptor_inventory(
            source, profile["namespace"], prefix, tuple(body_functions), _nodes=nodes
        )
        if actual_declarations != declarations:
            raise ValueError("translated descriptor declarations changed")
    else:
        observed = inventory(source, profile["namespace"], tuple(body_functions), _nodes=nodes)
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
