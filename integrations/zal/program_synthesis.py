"""Prepare a bounded ZAL2 eligibility relation for the existing finite synthesizer.

The selected event's Accept bit is an advisory projection, not a complete
decision program or a runtime authorization. The CLI's synthesizer, target
conformance runner and transform checker remain separate deciding boundaries.
"""
from __future__ import annotations

import argparse
import hashlib
import itertools
import json
from pathlib import Path

import adt
import behavior
import behavior2
import constructs
import diagnostics
import effects
import symmetry2
import typed2

FORMAT = "zal/eligibility-synthesis/1"
MAX_BYTES = 65_536
MAX_ROWS = 32
MAX_MODEL_ROWS = 256
MAX_NODES = 256
MAX_ASSIGNMENTS = 6_000
MAX_STEPS = 8_000_000


def _need(condition, message):
    if not condition:
        raise behavior2.Refusal("program-synthesis: " + message)


def _sha(data):
    return hashlib.sha256(data).hexdigest()


def _pairs(pairs):
    result = {}
    for key, value in pairs:
        _need(key not in result, "duplicate binding key")
        result[key] = value
    return result


def _number(_):
    raise behavior2.Refusal("program-synthesis: unsupported binding number")


def _binding(data, model):
    _need(type(data) is bytes and len(data) <= MAX_BYTES, "binding byte limit")
    try:
        document = json.loads(data.decode("utf-8"), object_pairs_hook=_pairs,
                              parse_float=_number, parse_constant=_number)
    except behavior2.Refusal:
        raise
    except (UnicodeError, ValueError, RecursionError) as error:
        raise behavior2.Refusal("program-synthesis: invalid binding JSON") from error
    _need(type(document) is dict and set(document) == {"format", "model_revision", "observations", "ui"},
          "binding shape")
    _need(document["format"] == "zal/app-binding/1" and document["model_revision"] == model.revision,
          "binding format or model revision")
    observations = document["observations"]
    _need(type(observations) is list and len(observations) == 1, "exactly one observation required")
    item = observations[0]
    _need(type(item) is dict and set(item) == {"name", "kind", "source", "role", "label", "unit"},
          "observation shape")
    _need(item["name"] == model.facts[0] and item["kind"] == "fact"
          and item["source"] == "server-context" and item["role"] == "authorization",
          "the Boolean authorization observation must be server context")
    _need(type(item["label"]) is str and type(item["unit"]) is str, "observation label and unit")
    # UI metadata is not consumed or validated here; the app-binding route must
    # independently validate it before any generated application is admitted.
    _need(type(document["ui"]) is dict, "UI binding shape")
    return item


def _contract(rows):
    """A Boolean relation, built from the accepted rows of the ZAL table."""
    nodes = [["input", i] for i in range(4)]
    constants = {}

    def add(node):
        nodes.append(node)
        return len(nodes) - 1

    def integer(value):
        if value not in constants:
            constants[value] = add(["int", value])
        return constants[value]

    terms = []
    for row in rows:
        if not row["eligible"]:
            continue
        state, count, role = row["input"]
        a = add(["eq", 0, integer(state)])
        b = add(["eq", 1, integer(count)])
        c = 2 if role else add(["not", 2])
        terms.append(add(["and", add(["and", a, b]), c]))
    if not terms:
        selected = add(["bool", False])
    else:
        selected = terms[0]
        for term in terms[1:]:
            selected = add(["not", add(["and", add(["not", selected]), add(["not", term])])])
    root = add(["eq", 3, selected])
    _need(len(nodes) <= MAX_NODES, "relation graph exceeds 256 nodes")
    return {"nodes": nodes, "roots": [root]}


def _reference(rows, state_bounds, variable_bounds):
    """Independently encode every table row as a complete selection chain."""
    nodes = [["Input", 0], ["Input", 1], ["Input", 2], ["Bool", False], ["Bool", True]]
    constants = {}

    def add(node):
        nodes.append(node)
        return len(nodes) - 1

    def integer(value):
        if value not in constants:
            constants[value] = add(["Int", str(value)])
        return constants[value]

    root = 3
    for row in rows:
        state, count, role = row["input"]
        a = add(["Eq", 0, integer(state)])
        b = add(["Eq", 1, integer(count)])
        c = 2 if role else add(["Not", 2])
        condition = add(["And", add(["And", a, b]), c])
        root = add(["Select", condition, 4 if row["eligible"] else 3, root])
    _need(len(nodes) <= MAX_NODES, "reference graph exceeds 256 nodes")
    return {"inputs": [{"kind": "Int", "min": str(state_bounds[0]), "max": str(state_bounds[1])},
                       {"kind": "Int", "min": str(variable_bounds[0]), "max": str(variable_bounds[1])},
                       {"kind": "Bool"}],
            "outputs": [{"kind": "Bool"}], "nodes": nodes, "roots": [root]}


def _sketch(state_bounds, variable_bounds):
    """Three interchangeable predicates, conjoined; no selected guard is supplied."""
    domains = [range(state_bounds[0], state_bounds[1] + 1),
               range(variable_bounds[0], variable_bounds[1] + 1)]
    values = sorted(set(itertools.chain.from_iterable(domains)))
    nodes = [["input", i] for i in range(3)]
    indices = {}
    for value in values:
        indices[value] = len(nodes)
        nodes.append(["int", value])
    alternatives = [["input", 2], ["not", 2], ["bool", False], ["bool", True]]
    for index, domain in enumerate(domains):
        for value in domain:
            alternatives.append(["eq", index, indices[value]])
            if value != domain.start:
                alternatives.append(["lt", index, indices[value]])
    for hole in (1, 2, 3):
        nodes.append({"hole": hole, "alternatives": alternatives})
    first = len(nodes) - 3
    nodes.extend([["and", first, first + 1], ["and", first + 3, first + 2]])
    _need(len(nodes) <= MAX_NODES and len(alternatives) ** 3 <= MAX_ASSIGNMENTS,
          "declared grammar exceeds candidate budget")
    return {"nodes": nodes, "roots": [len(nodes) - 1]}, alternatives, values


def _reference_problem(problem, reference):
    """Let the native finite synthesizer encode the independently built lookup."""
    nodes = []
    for index, instruction in enumerate(reference["nodes"]):
        op, *args = instruction
        op = op.lower()
        if op == "int":
            args[0] = int(args[0])
        node = [op, *args]
        if index == 3:
            _need(node == ["bool", False], "reference base constant")
            node = {"hole": 1, "alternatives": [node]}
        nodes.append(node)
    return {**problem, "sketch": {"nodes": nodes, "roots": list(reference["roots"])}}


def prepare(source, binding, event):
    """Return owned JSON artifacts; a completed CLI check is still required."""
    _need(type(source) is bytes and len(source) <= MAX_BYTES, "source byte limit")
    _need(type(event) is str and bool(event), "event")
    try:
        model = behavior2.parse(source.decode("utf-8"))
    except UnicodeError as error:
        raise behavior2.Refusal("program-synthesis: source is not UTF-8") from error
    _need(len(model.states) >= 2 and len(model.variables) == 1 and len(model.facts) == 1
          and not model.inputs, "only one integer variable and one Boolean context fact are supported")
    _need(not model.invariants and not model.properties and not model.fairness
          and not model.players and not model.games and not model.obligations,
          "invariants, temporal games, fairness and unresolved obligations are unsupported")
    _need(event in model.events, "unknown fixed event")
    item = _binding(binding, model)
    variable = model.variables[0]
    state_bounds = (150, 149 + len(model.states))
    variable_bounds = (variable.low, variable.high)
    count = len(model.states) * (variable.high - variable.low + 1) * 2
    _need(0 < count <= MAX_ROWS, "fixed-event Cartesian domain exceeds 32 inputs")
    _need(count * len(model.events) <= MAX_MODEL_ROWS, "complete model domain exceeds 256 inputs")
    report = behavior2.check(model, "program-synthesis/reference-interpreter", no_symmetry=True)
    _need(report.get("status") == "pass-with-scope", "ZAL finite check did not pass")
    rows = []
    for state_id, value, role in itertools.product(range(state_bounds[0], state_bounds[1] + 1),
                                                    range(variable.low, variable.high + 1), (False, True)):
        state = model.states[state_id - state_bounds[0]]
        outcome = model.decide(state, {variable.name: value}, event, {model.facts[0]: role})
        _need(outcome["class"] in {"Accept", "Reject"}, "unsupported decision class")
        rows.append({"input": [state_id, value, int(role)], "eligible": outcome["class"] == "Accept",
                     "reference_outcome": outcome})
    accepted = sum(row["eligible"] for row in rows)
    _need(0 < accepted < len(rows), "fixed-event projection must contain both Accept and Reject")
    sketch, alternatives, constants = _sketch(state_bounds, variable_bounds)
    inputs = [{"name": "pre.control", "type": {"kind": "int", "min": state_bounds[0], "max": state_bounds[1]}},
              {"name": f"pre.{variable.name}", "type": {"kind": "int", "min": variable.low, "max": variable.high}},
              {"name": f"context.{model.facts[0]}", "type": {"kind": "bool"}}]
    problem = {"schema": "zeno-fcis/synthesis-problem/1", "profile": "zeno-fcis/finite-i128/1",
               "inputs": inputs, "outputs": [{"name": "eligibility.accepted", "type": {"kind": "bool"}}],
               "contract": _contract(rows), "sketch": sketch}
    reference = _reference(rows, state_bounds, variable_bounds)
    source_hashes = {module.__name__: _sha(Path(module.__file__).read_bytes())
                     for module in (adt, behavior, behavior2, constructs, diagnostics, effects, symmetry2, typed2)}
    mapping = {"format": FORMAT, "authority": "none", "model_revision": model.revision,
               "source_sha256": _sha(source), "binding_sha256": _sha(binding),
               "adapter_sha256": _sha(Path(__file__).read_bytes()), "interpreter_sources": source_hashes,
               "event": event, "event_id": 120 + model.events.index(event),
               "states": {name: 150 + index for index, name in enumerate(model.states)},
               "variable": {"name": variable.name, "field": 170, "domain": list(variable_bounds)},
               "fact": {"name": model.facts[0], "field": 160, "source": item["source"], "role": item["role"]},
               "tuple_order": ["pre.control", f"pre.{variable.name}", f"context.{model.facts[0]}"],
               "output": "Accept bit for the fixed event only", "declared_model_tuples": report.get("input_tuples"),
               "projection_tuples": len(rows), "accepted_tuples": accepted,
               "rejected_tuples": len(rows) - accepted, "checker_status": report["status"],
               "grammar": {"shape": "and of three independently selected Boolean predicates",
                           "constants": constants, "alternatives": alternatives,
                           "candidate_space": len(alternatives) ** 3,
                           "max_assignments": MAX_ASSIGNMENTS, "max_steps": MAX_STEPS},
               "authentication": "host responsibility; role labels do not authenticate context"}
    return {"problem.json": problem, "reference-program.json": reference,
            "reference-problem.json": _reference_problem(problem, reference),
            "table.json": {"format": FORMAT, "rows": rows}, "mapping.json": mapping}


def write_new(artifacts, output):
    output = Path(output)
    output.mkdir(mode=0o700, parents=False, exist_ok=False)
    hashes = {}
    for name, value in artifacts.items():
        data = (json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False) + "\n").encode("utf-8")
        with (output / name).open("xb") as stream:
            stream.write(data)
        hashes[name] = _sha(data)
    with (output / "artifact-sha256.json").open("x", encoding="ascii") as stream:
        json.dump(hashes, stream, indent=2, sort_keys=True)
        stream.write("\n")
    return hashes


def _read_bounded(path):
    with path.open("rb") as stream:
        data = stream.read(MAX_BYTES + 1)
    _need(len(data) <= MAX_BYTES, "input byte limit")
    return data


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--binding", required=True, type=Path)
    parser.add_argument("--event", required=True)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        artifacts = prepare(_read_bounded(args.source), _read_bounded(args.binding), args.event)
        hashes = write_new(artifacts, args.out)
    except (behavior2.Refusal, OSError, ValueError) as error:
        print(json.dumps({"status": "refused", "authority": "none", "reason": str(error)}))
        return 1
    print(json.dumps({"status": "prepared", "authority": "none", "artifacts": hashes}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
