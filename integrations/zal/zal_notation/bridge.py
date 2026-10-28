"""Trusted-host projection of the sibling finite semantic-library Contract.

Call ``from_finite_contract`` with an existing, trusted Python Contract value.
This module does not import a module named by data, instantiate a Contract from
JSON, execute a pack, or grant authority to a rendered interpretation. The
host object's ``data`` and ``revision`` implementations are trusted Python code.

The supported fragment has one state link, full finite input coverage, and one
output per input. Missing rows are environmental restrictions in the source
profile; this initial adapter refuses them instead of inventing rejection rows.
"""
from __future__ import annotations

from dataclasses import fields, is_dataclass
from hashlib import sha256
from itertools import product
import json
import re
from typing import Any

from . import core
from .builtin import fsm_grammar


PROFILE = "zal/semantic-library-finite-enum/1"
BRIDGE = "zal.finite-contract-state-machine/1"
SOURCE_PATH = "integrations/zal/finite_library/prototype.py"
SOURCE_SHA256 = "3ce64a86b737854eab627255a25f0a403fb13d1b257a07c3e21471dc5bcd0ed1"
SOURCE_MAX_ROWS = 4096
SOURCE_MAX_BYTES = 1 << 20
SOURCE_NAME = re.compile(r"[a-z][a-z0-9_]{0,47}\Z")


def _need(ok: bool, message: str) -> None:
    core.need(ok, "finite contract bridge: " + message)


def _name(value: Any) -> None:
    _need(type(value) is str and SOURCE_NAME.fullmatch(value) is not None,
          "identifier is outside the finite-enum/1 profile")


def _immutable_record(value: Any, names: set[str], description: str) -> None:
    _need(not isinstance(value, type) and is_dataclass(value),
          f"{description} must be a trusted immutable host dataclass, not JSON")
    _need(getattr(type(value), "__dataclass_params__").frozen,
          f"{description} must be frozen")
    _need({field.name for field in fields(value)} == names,
          f"unsupported {description} interface")


def _ports(values: Any) -> list[dict]:
    _need(type(values) is tuple and 1 <= len(values) <= 8, "invalid port tuple")
    result = []
    seen = set()
    for port in values:
        _immutable_record(port, {"name", "role", "unit", "values"}, "Port")
        for value in (port.name, port.role, port.unit):
            _name(value)
        _need(port.name not in seen, "duplicate port name")
        seen.add(port.name)
        _need(type(port.values) is tuple and 1 <= len(port.values) <= 16,
              "invalid finite enumeration")
        for value in port.values:
            _name(value)
        _need(port.values == tuple(sorted(set(port.values))),
              "enumeration must be canonical and distinct")
        result.append({"name": port.name, "role": port.role, "unit": port.unit,
                       "values": list(port.values)})
    return result


def _domain_size(ports: list[dict]) -> int:
    count = 1
    for port in ports:
        count *= len(port["values"])
        _need(count <= SOURCE_MAX_ROWS, "Cartesian domain exceeds finite-enum/1")
    return count


def _source_canonical(value: Any) -> str:
    # Match prototype.py's digest convention, independently of our SVG format.
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True,
                      allow_nan=False)


def _tuple_in_domain(value: Any, ports: list[dict], label: str) -> None:
    _need(type(value) is tuple and len(value) == len(ports), f"invalid {label} tuple")
    _need(all(type(item) is str and item in port["values"]
              for item, port in zip(value, ports)), f"{label} tuple outside declared domain")


def from_finite_contract(contract: Any) -> tuple[dict, dict, dict]:
    """Return ``(grammar, graph_model, receipt)`` for a supported host Contract.

    Event labels are canonical JSON objects containing every nonstate input.
    Outcome labels contain every output except the linked successor state.
    Source and successor states are the graph edge endpoints. Labels are data;
    they are not expressions or inferred guard conditions.

    The receipt binds the complete source snapshot and the emitted graph. Its
    status is ``projection-only`` and authority is ``none``. Partial domains,
    contradictions, nondeterminism, and multi-state contracts are explicit
    refusals in this initial adapter.
    """
    _immutable_record(contract, {"inputs", "outputs", "rows", "state", "owns"}, "Contract")
    _need(callable(getattr(contract, "data", None)), "Contract.data is required")
    inputs, outputs = _ports(contract.inputs), _ports(contract.outputs)
    input_size = _domain_size(inputs)
    _domain_size(outputs)
    _need(type(contract.state) is tuple and len(contract.state) == 1,
          "exactly one state link is supported; multi-state composition needs a separate adapter")
    link = contract.state[0]
    _need(type(link) is tuple and len(link) == 3, "invalid state link")
    for value in link:
        _name(value)
    state_input, state_output, initial = link
    in_names = [port["name"] for port in inputs]
    out_names = [port["name"] for port in outputs]
    _need(state_input in in_names and state_output in out_names, "state link references unknown ports")
    state_i, state_o = in_names.index(state_input), out_names.index(state_output)
    before, after = inputs[state_i], outputs[state_o]
    _need(all(before[key] == after[key] for key in ("role", "unit", "values")),
          "state role, unit, or enumeration mismatch")
    _need(initial in before["values"], "initial state is outside the declared enumeration")
    _need(len(before["values"]) <= core.MAX_NODES, "state node budget exceeded")

    _need(type(contract.owns) is tuple and len(contract.owns) <= 16,
          "invalid ownership declarations")
    for resource in contract.owns:
        _name(resource)
    _need(contract.owns == tuple(sorted(set(contract.owns))),
          "ownership declarations must be canonical and distinct")
    _need(type(contract.rows) is tuple and 0 < len(contract.rows) <= SOURCE_MAX_ROWS,
          "requires a nonempty bounded admitted table")
    _need(len(contract.rows) <= core.MAX_EDGES and input_size <= core.MAX_EDGES,
          f"projected edge budget exceeds {core.MAX_EDGES}")

    seen = set()
    rows = []
    for row in contract.rows:
        _need(type(row) is tuple and len(row) == 2, "invalid table row")
        x, choices = row
        _tuple_in_domain(x, inputs, "input")
        _need(x not in seen, "duplicate admitted input")
        seen.add(x)
        _need(type(choices) is tuple, "output choices must be immutable tuples")
        _need(len(choices) > 0, "contradictory admitted row has no output")
        _need(len(choices) == 1, "nondeterministic output choices are unsupported")
        y = choices[0]
        _tuple_in_domain(y, outputs, "output")
        rows.append({"input": list(x), "outputs": [list(y)]})
    _need(contract.rows == tuple(sorted(contract.rows)), "table rows must be canonical")
    expected_inputs = set(product(*(port["values"] for port in inputs)))
    _need(seen == expected_inputs,
          "partial admitted domain is unsupported; missing rows are restrictions, not default rejection")

    source_data = {
        "profile": PROFILE, "inputs": inputs, "outputs": outputs,
        "state": [list(link)], "owns": list(contract.owns), "rows": rows,
    }
    provided = contract.data()
    core.canonical(provided)
    _need(type(provided) is dict and provided.get("profile") == PROFILE,
          "unsupported source profile")
    _need(provided == source_data, "Contract.data disagrees with its immutable fields")
    source_bytes = _source_canonical(source_data).encode("utf-8")
    _need(len(source_bytes) <= SOURCE_MAX_BYTES, "source snapshot byte budget exceeded")
    source_digest = sha256(source_bytes).hexdigest()
    _need(type(getattr(contract, "revision", None)) is str and contract.revision == source_digest,
          "Contract.revision disagrees with the independently recomputed source digest")

    grammar = fsm_grammar()
    nodes = [{"id": value, "tag": "start" if value == initial else "state",
              "fields": {"name": value}} for value in before["values"]]
    edges = []
    for index, (x, choices) in enumerate(contract.rows):
        y = choices[0]
        event = _source_canonical({port["name"]: value for i, (port, value)
                                   in enumerate(zip(inputs, x)) if i != state_i})
        outcome = _source_canonical({port["name"]: value for i, (port, value)
                                     in enumerate(zip(outputs, y)) if i != state_o})
        _need(len(event) <= 1024 and len(outcome) <= 1024, "projected edge label budget exceeded")
        edges.append({"id": f"row_{index:04d}", "tag": "transition",
                      "source": [x[state_i], "out"], "target": [y[state_o], "in"],
                      "fields": {"event": event, "outcome": outcome}})
    graph = {"schema": "zal.model/1", "grammar": {"id": grammar["id"], "version": grammar["version"]},
             "kind": "graph", "body": {"nodes": nodes, "edges": edges}}
    core.validate_model(grammar, graph)
    receipt = {
        "schema": "zal.contract-projection-receipt/1", "bridge": BRIDGE,
        "status": "projection-only", "authority": "none",
        "source_profile": PROFILE, "source_contract_data": source_data,
        "source_contract_digest": source_digest,
        "source_digest_algorithm": "sha256(json: sort_keys, compact separators, ensure_ascii=True)",
        "source_prototype": {"path": SOURCE_PATH, "sha256": SOURCE_SHA256,
                             "scope": "included reference component; caller-supplied Contract remains trusted-host data"},
        "model_digest": core.model_digest(grammar, graph),
        "projection": {"state_input": state_input, "state_output": state_output,
                       "initial_state": initial, "input_port_order": in_names,
                       "output_port_order": out_names,
                       "event_fields": [name for i, name in enumerate(in_names) if i != state_i],
                       "outcome_fields": [name for i, name in enumerate(out_names) if i != state_o],
                       "label_encoding": "canonical JSON objects; all values preserved",
                       "rows_projected": len(edges)},
        "assumptions": {"source_supplied_by": "trusted Python host; no JSON-directed import",
                        "admitted_domain": "full Cartesian product of the declared input enumerations",
                        "omitted_input_rows": 0, "output_choices_per_input": 1,
                        "state_links": 1, "temporal_scope": "one step per source row",
                        "roles_units_ownership": "source declarations retained; no authenticated authority inferred"},
        "nonclaims": ["native ZAL parser or production integration", "semantic equivalence proof",
                      "human agreement", "external execution", "credential authentication",
                      "real-world ownership or non-aliasing", "reachability or liveness proof"],
    }
    core.canonical(receipt)
    return grammar, graph, receipt
