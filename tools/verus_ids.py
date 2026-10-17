"""Restore only reviewed compiler IDs through a checked, scoped bijection.

No atom is erased. Occurrences are paired in traversal order in each function
record. Both directions of the renaming must agree, and the restored complete
record must have its original hash. Unknown ID forms remain exact atoms.
"""
from __future__ import annotations

import re

SCHEMA = "zeno-fcis/vir-id-bijection/1"


def functions(nodes, namespace):
    result = {}
    for node in nodes:
        if not node or node[0] != "Function":
            continue
        if (len(node) < 2 or not isinstance(node[1], list) or len(node[1]) != 3
                or node[1][:2] != ["Fun", ":path"] or not isinstance(node[1][2], str)):
            raise ValueError("malformed function in compiler-ID reference")
        name = node[1][2]
        if name.startswith(namespace):
            if name in result:
                raise ValueError("duplicate function in compiler-ID reference")
            result[name] = node
    return result


def occurrences(node, path=()):
    if not isinstance(node, list):
        return
    # Read IDs are crate-global; RustcId disambiguators are function-local.
    # Preliminary read kind, variable spelling, bindings and every other atom
    # remain covered by the complete record hash.
    family, index = None, None
    if node and node[0] == "UnfinalizedReadKind":
        if len(node) != 5 or node[1] != ":preliminary_kind" or node[3] != ":id":
            raise ValueError("unknown UnfinalizedReadKind format")
        family, index = "read", 4
    elif len(node) >= 2 and node[:2] == ["VarIdentDisambiguate", "RustcId"]:
        if len(node) != 3:
            raise ValueError("unknown RustcId format")
        family, index = "local", 2
    if family is not None:
        value = node[index]
        if not isinstance(value, str) or re.fullmatch(r"0|[1-9][0-9]*", value) is None:
            raise ValueError("noncanonical compiler ID")
        yield [family, list((*path, index)), value]
    for index, child in enumerate(node):
        yield from occurrences(child, (*path, index))


def reference(source, profile):
    """Propose a reference only after exact, existing coverage has passed.

    A reference is profile data, subject to the same review and source binding
    as its original fingerprints. This function does not approve a profile.
    """
    from verus_coverage import fingerprint, parse_vir, require_coverage
    if "compiler_id_reference" in profile:
        raise ValueError("reference generation requires the original exact profile")
    require_coverage(source, profile)
    records = functions(parse_vir(source), profile["namespace"])
    return {"schema": SCHEMA, "functions": {
        name: {"sha256": fingerprint(node),
               "ids": [[family, value] for family, _, value in occurrences(node)]}
        for name, node in sorted(records.items())}}


def restore(nodes, namespace, reviewed):
    """Mutate invocation-owned parsed nodes, never the caller's profile.

    Full record hashes check changes to all bodies, requires, ensures and IDs
    not explicitly recognized above. Maps span the complete function inventory;
    they are not reset at contract/body boundaries.
    """
    from verus_coverage import fingerprint
    if (not isinstance(reviewed, dict) or set(reviewed) != {"schema", "functions"}
            or reviewed["schema"] != SCHEMA or not isinstance(reviewed["functions"], dict)):
        raise ValueError("malformed compiler-ID reference")
    actual = functions(nodes, namespace)
    if set(actual) != set(reviewed["functions"]):
        raise ValueError("VIR function inventory changed in compiler-ID reference")
    forward, reverse = {}, {}
    changed = []
    for name, node in sorted(actual.items()):
        expected = reviewed["functions"][name]
        if not isinstance(expected, dict) or set(expected) != {"sha256", "ids"} or not isinstance(expected["ids"], list):
            raise ValueError("malformed compiler-ID function reference")
        seen = list(occurrences(node))
        if len(seen) != len(expected["ids"]):
            raise ValueError(f"translated contracts or specifications changed: {[name]}")
        for observed, old in zip(seen, expected["ids"]):
            if (not isinstance(old, list) or len(old) != 2 or old[0] != observed[0]
                    or not isinstance(old[1], str) or re.fullmatch(r"0|[1-9][0-9]*", old[1]) is None):
                raise ValueError(f"translated contracts or specifications changed: {[name]}")
            family, path, value = observed
            scope = (family, name if family == "local" else "")
            left, right = (*scope, old[1]), (*scope, value)
            if forward.get(left, right) != right or reverse.get(right, left) != left:
                raise ValueError(f"nonbijective compiler-ID renaming: {name}")
            forward[left], reverse[right] = right, left
            target = node
            for index in path[:-1]:
                target = target[index]
            target[path[-1]] = old[1]
        if fingerprint(node) != expected["sha256"]:
            changed.append(name)
    if changed:
        raise ValueError(f"translated contracts or specifications changed: {changed}")
    return nodes


def emit(node):
    return "(" + " ".join(emit(child) for child in node) + ")" if isinstance(node, list) else node
