"""Reproducible ontology packet, evidence boundary example, and offline export."""
from __future__ import annotations

from copy import deepcopy
from hashlib import sha256
import json
from pathlib import Path

from . import core
from .ontology import ontology_example, validate_ontology
from .ontology_browser import render_browser
from .ontology_cnl import parse_ontology_cnl, render_ontology_cnl
from .ontology_rdf import export_jsonld, export_turtle
from .ontology_records import export_records_turtle, export_shapes_turtle, validate_record
from .ontology_tools import classify
from .reasoning import check_relation


def evidence_boundary() -> dict:
    """Three independent checks on fixed, explicitly supplied model data.

The small ontology is the fixed typed projection of the record's source field;
it is not a general RDF importer. The relation checker is Crucible's existing
finite one-step checker. No record field can select another checker.
    """
    model = {
        "schema": "zal.ontology/1", "id": "evidence-record-domain", "version": 1,
        "classes": ["ClaimRecord"], "properties": ["hasSource"],
        "individuals": ["record", "sourcePolicy"],
        "axioms": [{"id": "source-domain", "kind": "domain", "property": "hasSource", "class": "ClaimRecord"}],
        "facts": [{"id": "record-source", "kind": "property", "subject": "record", "property": "hasSource", "object": "sourcePolicy"}],
    }
    left = {"schema": "zal.finite-table/1", "id": "accept-when-true", "version": 1,
            "inputs": [False, True], "outputs": ["Reject", "Accept"], "rows": [[0], [1]]}
    right = {"schema": "zal.finite-table/1", "id": "always-reject", "version": 1,
             "inputs": [False, True], "outputs": ["Reject", "Accept"], "rows": [[0], [0]]}
    genuine = check_relation(left, right)
    forged = deepcopy(genuine)
    forged["status"] = "checked"
    missing = {"id": "urn:zal:demo:claim-record", "source": "urn:zal:demo:accept-when-true",
               "target": "urn:zal:demo:always-reject", "scope": core.digest(genuine["replay_request"]),
               "checker": core.digest({"checker": genuine["checker"], "method": genuine["method"]})}
    well_formed_forgery = {**missing, "receipt": core.digest(forged)}
    fresh = check_relation(left, right, **genuine["replay_request"])
    result = {"schema": "zal.ontology-evidence-boundary/1", "ontology": model,
              "classification": classify(model)["result"],
              "missing_record": missing, "missing_shape": validate_record(missing),
              "forged_record": well_formed_forgery, "forged_shape": validate_record(well_formed_forgery),
              "left_table": left, "right_table": right, "genuine_result": genuine,
              "forged_result": forged, "fresh_result": fresh,
              "genuine_matches_replay": core.canonical(genuine) == core.canonical(fresh),
              "forged_matches_replay": core.canonical(forged) == core.canonical(fresh),
              "scope": "Fixed source-field projection, explicit record shape, and two-input one-step relation. Neither shape nor class inference verifies the behavioral claim.",
              "authority": "none"}
    core.need(result["classification"]["status"] == "classified", "record classification control failed")
    core.need(not result["missing_shape"]["conforms"] and result["forged_shape"]["conforms"], "record shape controls failed")
    core.need(genuine["status"] == "refuted" and result["genuine_matches_replay"] and not result["forged_matches_replay"], "evidence replay controls failed")
    core.canonical(result)
    return result


def _json(value) -> bytes:
    core.canonical(value)
    return (json.dumps(value, ensure_ascii=False, indent=2, allow_nan=False) + "\n").encode("utf-8")


def artifacts(ontology: dict) -> tuple[dict, dict[str, bytes]]:
    """Compute all display claims afresh from an admitted document."""
    validate_ontology(ontology)
    ontology = deepcopy(ontology)
    english = render_ontology_cnl(ontology)
    core.need(core.canonical(parse_ontology_cnl(english)) == core.canonical(ontology), "ontology controlled-English roundtrip differs")
    run = classify(ontology)
    boundary = evidence_boundary()
    packet = {"schema": "zal.ontology-browser/1", "ontology": ontology, "run": run,
              "controlled_english": english, "boundary": boundary, "authority": "none"}
    files = {
        "ontology.json": _json(ontology), "ontology.english.txt": english.encode("utf-8"),
        "ontology.ttl": export_turtle(ontology).encode("utf-8"),
        "ontology.jsonld": export_jsonld(ontology).encode("utf-8"),
        "classification.json": _json(run), "evidence-boundary.json": _json(boundary),
        "record-missing.ttl": export_records_turtle([boundary["missing_record"]]).encode("utf-8"),
        "record-forged.ttl": export_records_turtle([boundary["forged_record"]]).encode("utf-8"),
        "record-shapes.ttl": export_shapes_turtle().encode("utf-8"),
        "index.html": render_browser(packet).encode("utf-8"),
        "README.md": ("# ZAL ontology packet\n\nOpen `index.html` locally. The browser shows a recorded run.\n\n"
                      "Replay with `python -m zal_notation ontology-replay --directory /path/to/packet`.\n\n"
                      "Classification is conditional on the exact supplied ontology and this finite positive rule profile. "
                      "Shared classes do not establish equivalence. Shape validation checks explicit field syntax; "
                      "the separate finite checker must replay behavioral evidence. The original illustrative ontology "
                      "does not assert that all training and programming processes are identical.\n").encode("utf-8"),
    }
    core.need(all(len(data) <= core.MAX_BYTES for data in files.values()), "ontology export exceeds artifact byte budget")
    manifest = {"schema": "zal.ontology-packet/1", "ontology_digest": core.digest(ontology),
                "implementation_digest": run["implementation"]["digest"],
                "artifacts": {name: sha256(data).hexdigest() for name, data in sorted(files.items())},
                "scope": "Content hashes are integrity metadata; replay recomputes all claims and bytes.", "authority": "none"}
    files["manifest.json"] = _json(manifest)
    return packet, files


def generate(output: Path, *, ontology: dict | None = None) -> dict:
    from .library_lens_browser import fresh_directory
    packet, files = artifacts(ontology_example() if ontology is None else ontology)
    output = fresh_directory(output)
    for name, data in files.items():
        with (output / name).open("xb") as stream:
            stream.write(data)
    return {"status": packet["run"]["result"]["status"], "directory": str(output),
            "browser": str(output / "index.html"), "artifacts": len(files),
            "ontology_digest": packet["run"]["result"]["ontology_digest"], "authority": "none"}


def replay(directory: Path) -> dict:
    from .library_lens_browser import read_artifact
    ontology = core.loads(read_artifact(directory, "ontology.json").decode("utf-8"))
    packet, expected = artifacts(ontology)
    for name, data in expected.items():
        core.need(read_artifact(directory, name) == data, "ontology artifact differs from fresh replay: " + name)
    return {"schema": "zal.ontology-packet-replay/1", "status": "matches",
            "matches_replay": True, "artifacts_checked": len(expected),
            "classification_status": packet["run"]["result"]["status"],
            "ontology_digest": core.digest(ontology),
            "implementation_digest": packet["run"]["implementation"]["digest"],
            "scope": "Exact current ontology, classification, CNL, RDF, shape examples, replay controls and browser bytes; no browser execution or authorship authentication.",
            "authority": "none"}
