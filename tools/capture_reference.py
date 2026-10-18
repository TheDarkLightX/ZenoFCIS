"""Independent black-box capture artifact reference (stdlib, test-only).

Derived from the public finite-program semantics and example policy. It does
not call Rust, the capture door or an executable reference. Successful replay
is development evidence, never an acceptance or publication authority.
"""
import argparse
import hashlib
import itertools
import json
from pathlib import Path
import re

import program_search_reference as IR

OP = {"Input": 0, "Int": 1, "Bool": 2, "Add": 3, "Sub": 4, "Eq": 5, "Lt": 6,
      "And": 7, "Not": 8, "Select": 9}
MAX_TUPLES = 4096


def decimal(value):
    if type(value) is not str or not re.fullmatch(r"(?:0|-?[1-9][0-9]*)", value):
        raise ValueError("canonical decimal required")
    n = int(value)
    if not IR.I64_MIN <= n <= IR.I64_MAX:
        raise ValueError("i64 bound")
    return n


def domains(data):
    if type(data) is not list:
        raise ValueError("domain list")
    result = []
    for d in data:
        if d == {"kind": "Bool"}:
            result.append((True, 0, 1))
        elif type(d) is dict and set(d) == {"kind", "min", "max"} and d["kind"] == "Int":
            lo, hi = decimal(d["min"]), decimal(d["max"])
            if lo > hi:
                raise ValueError("empty domain")
            result.append((False, lo, hi))
        else:
            raise ValueError("domain shape")
    return tuple(result)


def points(spec, maximum=MAX_TUPLES):
    if type(maximum) is not int or not 0 <= maximum <= MAX_TUPLES:
        raise ValueError("tuple budget")
    ds = domains(spec["inputs"])
    size = 1
    for _, lo, hi in ds:
        size *= hi - lo + 1
    if size > maximum:
        raise ValueError("domain over budget")
    return tuple(itertools.product(*(range(lo, hi + 1) for _, lo, hi in ds)))


def fixture(data):
    if type(data) is not dict or set(data) != {"inputs", "outputs", "nodes", "roots"}:
        raise ValueError("program fields")
    inputs, outputs = domains(data["inputs"]), domains(data["outputs"])
    if len(inputs) > 32 or not 1 <= len(outputs) <= 16 or not 1 <= len(data["nodes"]) <= 256:
        raise ValueError("program bounds")
    nodes = []
    for n in data["nodes"]:
        if type(n) is not list or not n or n[0] not in OP or len(n) != IR.ARITY[OP[n[0]]] + 1:
            raise ValueError("instruction shape")
        tag, args = n[0], n[1:]
        if tag == "Int":
            args = [decimal(args[0])]
        elif tag == "Bool":
            if type(args[0]) is not bool:
                raise ValueError("Boolean literal")
            args = [int(args[0])]
        elif any(type(a) is not int or not 0 <= a <= 65535 for a in args):
            raise ValueError("reference shape")
        nodes.append((OP[tag], *args))
    if type(data["roots"]) is not list or any(type(a) is not int for a in data["roots"]):
        raise ValueError("root shape")
    p = IR.Program(inputs, outputs, tuple(nodes), tuple(data["roots"]))
    IR._type_check(p)
    return p


def table_rows(spec, rows):
    ps, outputs = points(spec), domains(spec["outputs"])
    if type(rows) is not list or len(rows) != len(ps):
        raise ValueError("complete row count")
    if any(type(r) is not list or len(r) != len(outputs) or
           any(type(v) is not int or not lo <= v <= hi for v, (_, lo, hi) in zip(r, outputs)) for r in rows):
        raise ValueError("successful integral rows required")
    return tuple(tuple(r) for r in rows)


def equal(a, b):
    """Load-bearing comparison seam, exercised by planted negative fixtures."""
    return a == b


def replay(spec, rows, candidate):
    expected = table_rows(spec, rows)
    p = fixture(candidate)
    if p.inputs != domains(spec["inputs"]) or p.outputs != domains(spec["outputs"]):
        return False
    return all(equal(IR.run(p, point), ("ok", want)) for point, want in zip(points(spec), expected))


def policies(spec, rows, candidate, examples, anchor=None):
    """Independent precheck, distance choice and eager-prefix outcome table."""
    expected, ps, p = table_rows(spec, rows), points(spec), fixture(candidate)
    if p.inputs != domains(spec["inputs"]) or p.outputs != domains(spec["outputs"]):
        raise ValueError("ABI differs")
    if len(set(examples)) != len(examples) or any(type(i) is not int or not 0 <= i < len(ps) for i in examples):
        raise ValueError("example ordinal")
    observed = [IR.run(p, point) for point in ps]
    bad = [i for i, (got, want) in enumerate(zip(observed, expected)) if not equal(got, ("ok", want))]
    pre = next((i for i in examples if i in bad), None)
    chosen = min(bad, key=lambda i: (-sum(abs(a - b) for a, b in zip(ps[i], ps[anchor]))
                                   if anchor is not None else 0, i)) if bad else None
    outcomes = []
    for j, (tag, *args) in enumerate(p.nodes):
        if tag != 9:
            continue
        prefix = IR.Program(p.inputs, ((True, 0, 1),), p.nodes[:j], (args[0],))
        values = [IR.run(prefix, point) for point in ps]
        for outcome in (0, 1):
            ordinals = [i for i, v in enumerate(values) if v == ("ok", (outcome,))]
            outcomes.append({"node": j, "outcome": outcome, "reachable": bool(ordinals),
                             "first": ordinals[0] if ordinals else None,
                             "covered": any(i in examples for i in ordinals)})
    return {"precheck_failure": pre, "bad": bad, "chosen": chosen, "coverage": outcomes}


def report_rows(rows):
    """Decode /2 scalar syntax before the independent, narrower i64 replay.

    This does not broaden the reference model or alter stored table hashing.
    """
    def scalar(value):
        if type(value) is int:
            number = value
        elif type(value) is str and len(value) <= 40:
            number = int(value)
            if str(number) != value:
                raise ValueError("noncanonical report integer")
        else:
            raise ValueError("report integer")
        if not -(1 << 127) <= number < (1 << 127):
            raise ValueError("report i128 bound")
        return number
    if type(rows) is not list or any(type(row) is not list for row in rows):
        raise ValueError("report rows")
    return [[scalar(value) for value in row] for row in rows]


def check_owner_report(report):
    """Verify complete table encoding and any claimed capture independently."""
    if report.get("schema") not in ("zeno-fcis/black-box-capture/1", "zeno-fcis/black-box-capture/2") or report.get("authority") != "none":
        return False
    detail = report["detail"]
    session = detail["session"]
    spec, rows = session["spec"], session["rows"]
    if report["schema"].endswith("/2"):
        if report.get("integer_encoding") != "safe-number-or-decimal-string/1":
            return False
        rows = report_rows(rows)
    table_rows(spec, rows)
    digest = hashlib.sha256(json.dumps(rows, separators=(",", ":")).encode("ascii")).hexdigest()
    if not equal(digest, session["table_sha256"]) or not replay(spec, rows, detail["table_program"]):
        return False
    if report["status"] == "captured-exact":
        return session.get("receipt") is not None and replay(spec, rows, session["candidate"])
    return True


def self_test(stub=None):
    spec = {"inputs": [{"kind": "Int", "min": "0", "max": "3"}], "outputs": [{"kind": "Bool"}]}
    right = {**spec, "nodes": [["Input", 0], ["Int", "2"], ["Lt", 0, 1]], "roots": [2]}
    wrong = {**spec, "nodes": [["Bool", False]], "roots": [0]}
    rows = [[1], [1], [0], [0]]
    prior = equal
    try:
        if stub == "replay":
            globals()["equal"] = lambda *_: True
        # Both positive AND planted negative are mandatory; all-refusal fails.
        return replay(spec, rows, right) and not replay(spec, rows, wrong)
    finally:
        globals()["equal"] = prior


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", nargs="?", type=Path)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--stub", choices=["replay"])
    args = parser.parse_args(argv)
    try:
        passed = self_test(args.stub) if args.self_test else args.report is not None and \
            check_owner_report(json.loads(args.report.read_text()))
    except (ValueError, KeyError, TypeError, OSError):
        passed = False
    print(json.dumps({"status": "PASS" if passed else "FAIL", "authority": "none"}))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
