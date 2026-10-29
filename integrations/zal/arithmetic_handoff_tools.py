"""Source-binding shell for the inert requirements handoff; never runs proofs."""
from __future__ import annotations

from hashlib import sha256
from pathlib import Path

from zal_notation import core
from zal_notation.ontology_tools import implementation_identity, _require_current_implementation
from arithmetic_handoff import build

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
PIN = HERE / "arithmetic_handoff_sources.json"
LOCAL = ("arithmetic_handoff.py", "arithmetic_handoff_tools.py", "arithmetic_handoff_sources.json",
         "mcp.py", "zal_notation/__main__.py")


def _read(path):
    core.need(path.is_file() and not any(p.is_symlink() for p in (path, *path.parents)),
              "missing or symlinked handoff source")
    core.need(path.stat().st_size <= core.MAX_BYTES, "handoff source exceeds bound")
    return path.read_bytes()


def implementation():
    return {name: sha256(_read(HERE / name)).hexdigest() for name in LOCAL}


_LOADED = implementation()


def source_binding():
    core.need(implementation() == _LOADED, "handoff implementation changed; restart before replay")
    _require_current_implementation()
    pin = core.loads(_read(PIN).decode("utf-8"))
    core.keys(pin, {"schema", "repository", "base_revision", "files"}, where="handoff source pin")
    core.need(pin["schema"] == "zeno-fcis/arithmetic-handoff-sources/1", "unsupported source pin")
    core.need(type(pin["files"]) is dict and 1 <= len(pin["files"]) <= 128, "invalid source inventory")
    for name, expected in pin["files"].items():
        path = Path(name)
        core.need(not path.is_absolute() and ".." not in path.parts and str(path) == name,
                  "invalid source path")
        core.need(sha256(_read(ROOT / path)).hexdigest() == expected,
                  "bound arithmetic contract source changed: " + name)
    return {"contracts": pin, "bridge": dict(_LOADED), "ontology_implementation": implementation_identity()}


def handoff(ontology, bindings, *, max_facts=1024, max_work=1_000_000):
    before = source_binding()
    result = build(ontology, bindings, max_facts=max_facts, max_work=max_work)
    core.need(source_binding() == before, "handoff source changed during invocation")
    result["source_binding"] = before
    source_id = core.digest(before)
    for row in result["requirements"]:
        row["requirement_id"] = "r-" + core.digest({"ontology": result["ontology_digest"],
            "source": source_id, "binding": row["binding"], "requirement": row["type_requirement"]})
        for obligation in row["obligations"]:
            obligation["id"] = row["requirement_id"] + "/" + obligation["kind"]
            obligation["source_binding_digest"] = source_id
    core.canonical(result)
    return result


def replay(ontology, bindings, packet, *, max_facts=1024, max_work=1_000_000):
    core.canonical(packet)
    fresh = handoff(ontology, bindings, max_facts=max_facts, max_work=max_work)
    matched = core.canonical(fresh) == core.canonical(packet)
    return {"schema": "zeno-fcis/arithmetic-handoff-replay/1",
            "status": "matches" if matched else "mismatch", "matches_replay": matched,
            "current_status": fresh["status"], "authority": "none",
            "scope": "Exact recomputation of requirements, obligations and source bindings; no proof receipt is imported or executed."}


def tool_definitions():
    bounds = {"max_facts": {"type": "integer", "minimum": 0, "maximum": 1024},
              "max_work": {"type": "integer", "minimum": 0, "maximum": 1_000_000}}
    properties = {"ontology": {"type": "object"}, "bindings": {"type": "array", "minItems": 1,
        "maxItems": 16, "items": {"type": "object", "required": ["individual", "target", "type_id"],
        "additionalProperties": False, "properties": {"individual": {"type": "string"},
        "target": {"type": "string"}, "type_id": {"type": "integer", "minimum": 1, "maximum": 4294967295}}}}, **bounds}
    return [{"name": "zal_arithmetic_" + name, "description": description,
             "inputSchema": {"type": "object", "additionalProperties": False,
                 "required": ["ontology", "bindings"] + (["packet"] if name == "replay" else []),
                 "properties": {**properties, **({"packet": {"type": "object"}} if name == "replay" else {})}}}
            for name, description in (
                ("handoff", "Discover source-bound U256/I256 requirements and open proof obligations. No proof execution or financial behavior. Authority: none."),
                ("replay", "Recompute an inert arithmetic handoff; altered obligations, signedness and evidence labels mismatch. No command execution. Authority: none."))]


def dispatch(name, arguments):
    definitions = {t["name"]: t["inputSchema"] for t in tool_definitions()}
    core.need(type(name) is str and name in definitions, "unsupported arithmetic tool")
    core.canonical(arguments)
    spec = definitions[name]
    core.keys(arguments, set(spec["required"]), set(spec["properties"]) - set(spec["required"]), "arithmetic tool arguments")
    return (handoff if name == "zal_arithmetic_handoff" else replay)(**arguments)
