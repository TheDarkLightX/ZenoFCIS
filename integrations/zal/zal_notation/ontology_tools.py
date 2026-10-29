"""Source-bound ontology tools over the pure, explicitly admitted rule profile.

The IO shell fingerprints fixed installed modules. Input data never chooses a
module, a path, a checker, or executable code. Classification is conditional on
the supplied ontology; it never upgrades a library relation or imported receipt.
"""
from __future__ import annotations

from copy import deepcopy
from hashlib import sha256
from pathlib import Path

from . import core
from .ontology import classify_ontology


SOURCE_FILES = (
    "core.py", "ontology.py", "ontology_cnl.py", "ontology_tools.py",
    "ontology_rdf.py", "ontology_records.py",
)


def implementation_identity() -> dict:
    root = Path(__file__).resolve().parent
    files = {name: sha256((root / name).read_bytes()).hexdigest() for name in SOURCE_FILES}
    return {"schema": "zal.ontology-implementation/1", "files": files,
            "digest": core.digest(files)}


# Bind this adapter instance once. If source files change in a long-running
# tools-stdio process, refuse instead of labeling loaded old functions with new
# file digests. Restart the process after editing the installed implementation.
_LOADED_IMPLEMENTATION = implementation_identity()


def _require_current_implementation() -> None:
    core.need(implementation_identity() == _LOADED_IMPLEMENTATION,
              "ontology implementation changed since adapter initialization; restart before replay")


def classify(ontology: dict, *, max_facts: int = 1024, max_work: int = 1_000_000) -> dict:
    _require_current_implementation()
    result = classify_ontology(ontology, max_facts=max_facts, max_work=max_work)
    _require_current_implementation()
    run = {"schema": "zal.ontology-run/1", "implementation": deepcopy(_LOADED_IMPLEMENTATION),
           "request": {"max_facts": max_facts, "max_work": max_work},
           "result": result, "authority": "none"}
    core.canonical(run)
    return run


def replay(ontology: dict, run: dict) -> dict:
    """Compare the entire retained run with fresh local execution and source pins."""
    core.canonical(run)
    core.keys(run, {"schema", "implementation", "request", "result", "authority"}, where="ontology run")
    core.need(run["schema"] == "zal.ontology-run/1", "unsupported ontology run schema")
    core.keys(run["request"], {"max_facts", "max_work"}, where="ontology replay request")
    fresh = classify(ontology, **run["request"])
    matches = core.canonical(run) == core.canonical(fresh)
    return {"schema": "zal.ontology-replay/1", "status": "matches" if matches else "mismatch",
            "matches_replay": matches, "current_status": fresh["result"]["status"],
            "current_ontology_digest": fresh["result"]["ontology_digest"],
            "current_implementation_digest": fresh["implementation"]["digest"],
            "scope": "Exact request, ontology, implementation files, closure, and explanations. A matching incomplete or inconsistent result is not a consistent classification.",
            "authority": "none"}


def query(ontology: dict, *, individual: str | None = None, class_name: str | None = None,
          max_facts: int = 1024, max_work: int = 1_000_000) -> dict:
    """Return entailed named type facts; absence always means not established."""
    if individual is not None:
        core.name(individual, "individual")
    if class_name is not None:
        core.name(class_name, "class")
    run = classify(ontology, max_facts=max_facts, max_work=max_work)
    facts = [fact for fact in run["result"].get("facts", [])
             if fact["kind"] == "type" and
             (individual is None or fact["individual"] == individual) and
             (class_name is None or fact["class"] == class_name)]
    return {"schema": "zal.ontology-query/1", "matches": facts,
            "absence": "not-established; not a negative assertion",
            "run": run, "authority": "none"}


def explain(ontology: dict, fact: str, *, max_facts: int = 1024, max_work: int = 1_000_000) -> dict:
    """Recompute and retain the acyclic premise subgraph for one canonical fact ID."""
    core.name(fact, "fact id")
    run = classify(ontology, max_facts=max_facts, max_work=max_work)
    facts = {item["id"]: item for item in run["result"].get("facts", [])}
    proofs = {item["fact"]: item for item in run["result"].get("proofs", [])}
    selected: set[str] = set()
    pending = [fact] if fact in facts else []
    while pending:
        current = pending.pop()
        if current in selected:
            continue
        selected.add(current)
        pending.extend(proofs[current]["premises"])
    return {"schema": "zal.ontology-explanation/1", "fact": fact,
            "status": "explained" if fact in facts else "not-established",
            "facts": [facts[key] for key in sorted(selected)],
            "proofs": [item for item in run["result"].get("proofs", []) if item["fact"] in selected],
            "run": run, "authority": "none"}


def tool_definitions() -> list[dict]:
    obj, txt = {"type": "object"}, {"type": "string"}
    bounds = {"max_facts": {"type": "integer", "minimum": 0, "maximum": 1024},
              "max_work": {"type": "integer", "minimum": 0, "maximum": 1_000_000}}
    def spec(name, description, required, properties):
        return {"name": "zal_ontology_" + name, "description": description,
                "input_schema": {"type": "object", "required": required,
                                 "additionalProperties": False, "properties": properties}}
    return [
        spec("parse", "Parse the admitted controlled-English ontology; unsupported English is refused.", ["source"], {"source": txt}),
        spec("classify", "Derive named facts in a bounded positive OWL 2 RL fragment, with explanations. Shared classes do not establish equivalence.", ["ontology"], {"ontology": obj, **bounds}),
        spec("query", "Find established type facts in a freshly classified ontology. Missing facts are unknown.", ["ontology"], {"ontology": obj, "individual": txt, "class_name": txt, **bounds}),
        spec("explain", "Recompute the premises of one canonical inferred fact. One deterministic derivation is returned, not every possible justification.", ["ontology", "fact"], {"ontology": obj, "fact": txt, **bounds}),
        spec("replay", "Replay an imported run against current ontology and implementation bytes; no imported status is trusted.", ["ontology", "run"], {"ontology": obj, "run": obj}),
        spec("validate_record", "Validate explicit evidence-record fields against a fixed SHACL subset. Conformity establishes shape, not valid evidence.", ["record"], {"record": obj}),
    ]


def dispatch(name: str, arguments: dict) -> dict:
    _require_current_implementation()
    definitions = {item["name"]: item["input_schema"] for item in tool_definitions()}
    core.canonical(arguments)
    core.need(type(name) is str and name in definitions, "unsupported ontology tool")
    schema = definitions[name]
    core.keys(arguments, set(schema["required"]), set(schema["properties"]) - set(schema["required"]), "ontology tool arguments")
    action = name.removeprefix("zal_ontology_")
    if action == "parse":
        from .ontology_cnl import parse_ontology_cnl, render_ontology_cnl
        ontology = parse_ontology_cnl(arguments["source"])
        return {"ontology": ontology, "controlled_english": render_ontology_cnl(ontology), "authority": "none"}
    if action == "validate_record":
        from .ontology_records import validate_record
        return validate_record(arguments["record"])
    return {"classify": classify, "query": query, "explain": explain, "replay": replay}[action](**arguments)
