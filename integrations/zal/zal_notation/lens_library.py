"""Source-retaining Lens discovery for general abstraction-library entries.

Definitions are inert proposals. Only exact host definitions select the fixed
structural or finite Mealy checker below. Custom profiles and informal entries
remain authorable; a registered suggestion is not a proof of applicability.
"""
from __future__ import annotations

from copy import deepcopy
from hashlib import sha256
from pathlib import Path

from . import abstractions, core


def definition(identifier, title, *, summary, assumptions, preserves, forgets,
               adds, profile_hints=None, mode="construction"):
    result = {"schema": "zal.lens-definition/1", "id": identifier, "title": title,
              "summary": summary, "mode": mode, "assumptions": deepcopy(assumptions),
              "preserves": deepcopy(preserves), "forgets": deepcopy(forgets),
              "adds": deepcopy(adds), "profile_hints": deepcopy(profile_hints or [])}
    return validate_definition(result)


def validate_definition(value):
    core.canonical(value)
    core.keys(value, {"schema", "id", "title", "summary", "mode", "assumptions",
                      "preserves", "forgets", "adds", "profile_hints"}, where="lens definition")
    core.need(value["schema"] == "zal.lens-definition/1", "unsupported lens definition")
    core.name(value["id"], "lens id")
    abstractions.text(value["title"], "lens title", 200)
    abstractions.text(value["summary"], "lens summary")
    core.need(value["mode"] in ("construction", "analogy"), "unknown lens mode")
    for field in ("assumptions", "preserves", "forgets", "adds", "profile_hints"):
        rows = value[field]
        core.need(type(rows) is list and len(rows) <= 64, "lens declaration list budget exceeded")
        for row in rows:
            abstractions.text(row, field, 8000)
        core.need(len(set(rows)) == len(rows), "duplicate lens declaration")
    return value


def new_registry():
    return {"schema": "zal.lens-registry/1", "definitions": []}


def validate_registry(registry):
    core.canonical(registry)
    core.keys(registry, {"schema", "definitions"}, where="lens registry")
    core.need(registry["schema"] == "zal.lens-registry/1", "unsupported lens registry")
    rows = registry["definitions"]
    core.need(type(rows) is list and len(rows) <= 64, "lens registry budget exceeded")
    seen = set()
    for row in rows:
        validate_definition(row)
        core.need(row["id"] not in seen, "duplicate lens id")
        seen.add(row["id"])
    return registry


def register(registry, proposed):
    result = deepcopy(validate_registry(registry))
    result["definitions"].append(deepcopy(validate_definition(proposed)))
    return validate_registry(result)


def _structural_definitions():
    common = {"preserves": ["complete source entry, model and declared context"],
              "forgets": [], "adds": ["a structural viewpoint; no execution or termination theorem"]}
    return [definition("inductive-syntax", "Inductive syntax tree",
                       summary="Inspect the constructors of a supplied term model, not an execution tree.",
                       assumptions=["A valid term grammar and model are supplied."], **common),
            definition("relational-graph", "Relational graph",
                       summary="Inspect the declared graph nodes and edges; labels remain data.",
                       assumptions=["A valid graph grammar and model are supplied."], **common),
            definition("sequence-chunks", "Sequence or chunk interpretation",
                       summary="Proposed interpretation alongside trees and graphs; no generic checker is registered.",
                       assumptions=["An author supplies the ordering, chunk boundaries and intended meaning."],
                       preserves=[], forgets=[], adds=["an unresolved alternative viewpoint"])]


def default_registry():
    # Import is a fixed host dependency, never a module name taken from data.
    registry = new_registry()
    for row in _structural_definitions():
        registry = register(registry, row)
    registry = register(registry, definition("typed-category", "Typed category lens",
        summary="Author a categorical interpretation; optional Haskell tools check supplied finite tables or compose authored symbolic relationships.",
        assumptions=["Supply an explicit interpretation and request; discovery is not applicability."],
        preserves=["Only completed scoped laws established by the explicit category host."],
        forgets=["Unrepresented source behavior is not certified."], adds=["Optional typed composition and finite category/functor/inverse checks."],
        profile_hints=["zal/lens-category/1"]))
    try:
        from . import lenses
    except (ImportError, ValueError):
        # General authoring/structure remains available without the optional
        # fixed finite adapter; no missing construction is called refuted.
        return registry
    for identifier in lenses.SPECS:
        spec = lenses._spec(identifier)
        registry = register(registry, definition(identifier, spec["name"], summary=spec["summary"],
            assumptions=lenses.ASSUMPTIONS, preserves=spec["preserves"], forgets=spec["forgets"],
            adds=spec["adds"], profile_hints=[lenses.PROFILE]))
    return registry


def _adapter(proposed):
    """Exact definition matching is necessary; the actual check is still required."""
    trusted = {row["id"]: row for row in default_registry()["definitions"]}
    match = trusted.get(proposed["id"])
    if match is None or core.canonical(match) != core.canonical(proposed):
        return None
    if proposed["id"] in ("inductive-syntax", "relational-graph"):
        return "structure"
    if proposed["id"] in ("sequence-chunks", "typed-category"):
        return None
    return "finite-context"


def _entry(library, entry_digest):
    abstractions.validate_library(library)
    core.need(type(entry_digest) is str, "entry identity must be text")
    entries = {core.digest(row): row for row in library["entries"]}
    core.need(entry_digest in entries, "entry revision is absent from this library")
    return entries[entry_digest]


def _availability(entry, proposed):
    if proposed["mode"] == "analogy":
        return "analogy", "Informal analogy only; no property transfer is established."
    adapter = _adapter(proposed)
    discussion = entry["discussion"]
    if discussion is None:
        return "pending", "No formal context is supplied; the source remains a draft."
    if adapter == "structure":
        wanted = "term" if proposed["id"] == "inductive-syntax" else "graph"
        if discussion["context"]["model"]["kind"] == wanted:
            return "candidate", "Structural checker available; apply and check to obtain evidence."
        return "conditional", "Requires a supplied " + wanted + " model and its explicit source map."
    if adapter == "finite-context":
        from . import lenses
        catalog = lenses.discover(discussion["context"])
        row = next(row for row in catalog["lenses"] if row["id"] == proposed["id"])
        return row["status"], "Finite Mealy adapter only; discovery is not a checked claim."
    if proposed["id"] == "typed-category":
        return "pending", "Use the explicit category-profile tools with a supplied interpretation/request and optional trusted operator runtime; generic application is not auto-routed."
    return "pending", "No trusted checker is bound to this exact definition; hints and labels do not certify it."


def discover(library, entry_digest, registry=None):
    entry = _entry(library, entry_digest)
    registry = default_registry() if registry is None else validate_registry(registry)
    rows = []
    for proposed in registry["definitions"]:
        status, reason = _availability(entry, proposed)
        rows.append({"definition": deepcopy(proposed), "definition_digest": core.digest(proposed),
                     "status": status, "reason": reason, "evidence": "not-checked"})
    return {"schema": "zal.library-lens-catalog/1", "library_digest": core.digest(library),
            "entry_digest": entry_digest, "source_entry": deepcopy(entry),
            "entry_status": abstractions.inspect_entry(entry), "registry_digest": core.digest(registry),
            "enumeration": "registered-candidates-only", "universal_completeness": False,
            "lenses": rows, "authority": "none"}


def _bind(library, entry, proposed, payload):
    root = Path(__file__).resolve().parent
    paths = {"zal_notation/" + path.name: path for path in sorted(root.glob("*.py"))}
    finite_path = root.parent / "finite_library" / "prototype.py"
    if finite_path.is_file():
        paths["finite_library/prototype.py"] = finite_path
    implementation = core.digest({label: sha256(path.read_bytes()).hexdigest() for label, path in paths.items()})
    return {"schema": "zal.library-lens-application/1", "status": "candidate",
            "implementation_digest": implementation,
            "library_digest": core.digest(library), "entry_digest": core.digest(entry),
            "source_entry": deepcopy(entry), "definition": deepcopy(proposed),
            "definition_digest": core.digest(proposed), "payload": deepcopy(payload), "authority": "none"}


def propose(library, entry_digest, lens_id, target_entry, correspondence, *, registry=None):
    """Retain an authored view/map, including informal targets, without executing it."""
    entry = _entry(library, entry_digest)
    registry = default_registry() if registry is None else validate_registry(registry)
    core.need(type(lens_id) is str, "lens id must be text")
    matches = [row for row in registry["definitions"] if row["id"] == lens_id]
    core.need(len(matches) == 1, "lens is absent from this registered catalog")
    proposed = matches[0]
    abstractions.validate_entry(target_entry)
    core.canonical(correspondence)
    core.keys(correspondence, {"description", "pairs"}, where="declared correspondence")
    abstractions.text(correspondence["description"], "correspondence description")
    pairs = correspondence["pairs"]
    core.need(type(pairs) is list and len(pairs) <= 64, "correspondence pair budget exceeded")
    for pair in pairs:
        core.keys(pair, {"source", "target"}, where="declared correspondence pair")
        core.name(pair["source"], "source label")
        core.name(pair["target"], "target label")
    result = {"schema": "zal.library-lens-proposal/1",
              "status": "analogy" if proposed["mode"] == "analogy" else "proposed",
              "library_digest": core.digest(library), "registry_digest": core.digest(registry),
              "entry_digest": entry_digest, "source_entry": deepcopy(entry),
              "target_entry_digest": core.digest(target_entry), "target_entry": deepcopy(target_entry),
              "definition_digest": core.digest(proposed), "definition": deepcopy(proposed),
              "correspondence": deepcopy(correspondence), "evidence": "author-declared; not checked",
              "map_scope": "Supplied labels and correspondence are proposals; endpoint meaning and preservation are not certified.",
              "source_status": abstractions.inspect_entry(entry), "target_status": abstractions.inspect_entry(target_entry),
              "adoption": "none", "authority": "none"}
    core.canonical(result)
    return result


def apply(library, entry_digest, lens_id, parameters=None, *, registry=None, budget=100000):
    entry = _entry(library, entry_digest)
    registry = default_registry() if registry is None else validate_registry(registry)
    core.need(type(lens_id) is str, "lens id must be text")
    matches = [row for row in registry["definitions"] if row["id"] == lens_id]
    core.need(len(matches) == 1, "lens is absent from this registered catalog")
    proposed = matches[0]
    status, reason = _availability(entry, proposed)
    core.need(status == "candidate", reason)
    core.need(type(budget) is int and 0 <= budget <= 1000000, "invalid lens budget")
    source = entry["discussion"]["context"]
    if _adapter(proposed) == "finite-context":
        from . import lenses
        payload = lenses.apply(source, lens_id, parameters, budget=budget)
    else:
        core.need(parameters is None or type(parameters) is dict and not parameters, "structural view has no parameters")
        # Identity mapping retains the context, including custom semantics. It
        # checks structure only and cannot turn those declarations into truth.
        payload = {"source_context_digest": core.digest(source), "target_context": deepcopy(source),
                   "map": "identity on the complete supplied context/model"}
    return _bind(library, entry, proposed, payload)


def check(library, entry_digest, application, budget=100000):
    result = {"schema": "zal.library-lens-check/1", "status": "unsupported", "budget": budget,
              "authority": "none", "scope": "structural identity or the exact finite profile; no global applicability proof"}
    try:
        core.need(type(budget) is int and 0 <= budget <= 1000000, "invalid lens budget")
        entry = _entry(library, entry_digest)
        core.canonical(application)
        proposed = validate_definition(application["definition"])
        expected = _bind(library, entry, proposed, application["payload"])
        core.need(core.canonical(application) == core.canonical(expected), "entry, library, definition or envelope changed")
        adapter = _adapter(proposed)
        core.need(_availability(entry, proposed)[0] == "candidate", "definition/context has no bound checker")
        source = entry["discussion"]["context"]
        result.update(entry_digest=entry_digest, application_digest=core.digest(application),
                      definition_digest=core.digest(proposed), entry_status=abstractions.inspect_entry(entry))
        if adapter == "finite-context":
            from . import lenses
            core.need(application["payload"]["lens_id"] == proposed["id"], "payload lens differs from the bound definition")
            receipt = lenses.check(source, application["payload"], budget)
            result.update(status=receipt["status"], evidence=receipt)
        else:
            if budget == 0:
                result.update(status="incomplete", reason="No structural check work budget remains.")
                return result
            core.validate_model(source["grammar"], source["model"])
            target = apply(library, entry_digest, proposed["id"])
            core.need(core.canonical(target) == core.canonical(application), "structural identity map changed")
            result.update(status="checked", evidence={"checker": "core.validate_model and exact context identity",
                          "claim": "Supplied model has the declared structural form; no execution-tree or semantic equivalence claim.",
                          "work": 1, "work_scope": "One structural identity check; native JSON/model bounds separately limit validation."})
    except (core.NotationError, KeyError, TypeError, ValueError) as exc:
        result.update(status="unsupported", reason=str(exc))
    return result


def verify_receipt(library, entry_digest, application, receipt):
    try:
        return type(receipt) is dict and core.canonical(receipt) == core.canonical(check(library, entry_digest, application, receipt["budget"]))
    except (ValueError, KeyError, TypeError):
        return False


def restore(library, entry_digest, application):
    entry = _entry(library, entry_digest)
    core.need(application["entry_digest"] == entry_digest and
              core.canonical(application["source_entry"]) == core.canonical(entry), "retained entry changed")
    return deepcopy(entry)
