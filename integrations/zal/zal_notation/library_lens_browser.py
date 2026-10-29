"""Generic entry-bound read-only Lens export and deterministic replay.

The fixed host reconstructs applications/receipts and all browser bytes. Imported
HTML, statuses, claimed hashes, templates and modules are never executed/trusted.
"""
from __future__ import annotations

from copy import deepcopy
from hashlib import sha256
import argparse
import json
import os
from pathlib import Path, PurePosixPath

from . import abstractions as A, core, lens_library as V, lenses as L, render, sharing
from . import reading_typography

ROOT = Path(__file__).resolve().parent
MAX_REQUEST = 1_000_000
MAX_FILES = 768
MAX_TOTAL = 24_000_000
ASSETS = ("index.html", "app.js", "styles.css")
SCHEMA = "zal.library-lens-browser-request/1"


def safe_file(base: Path, name: str) -> Path:
    """Confine a relative local artifact; reject links and path/URL ambiguity."""
    core.need(type(name) is str and bool(name) and len(name) <= 256,
              'Invalid artifact path.')
    core.need(not any(ord(c) < 32 or c in '\\:?#\x7f' for c in name),
              'Unsafe artifact path.')
    parts = name.split('/')
    core.need(all(part not in ('', '.', '..') for part in parts),
              'Unsafe artifact path.')
    base = Path(base)
    core.need(not base.is_symlink() and base.is_dir(), 'Artifact root is not a real directory.')
    base = base.resolve()
    target = base
    for part in PurePosixPath(name).parts:
        target = target / part
        core.need(not target.is_symlink(), 'Artifact symlinks are not admitted.')
    core.need(target.resolve().is_relative_to(base) and target.is_file(),
              'Artifact is absent or escapes packet directory.')
    return target


def read_artifact(base: Path, name: str) -> bytes:
    path = safe_file(base, name)
    core.need(path.stat().st_size <= core.MAX_BYTES, 'Artifact byte budget exceeded.')
    with path.open('rb') as stream:
        data = stream.read(core.MAX_BYTES + 1)
    core.need(len(data) <= core.MAX_BYTES, 'Artifact byte budget exceeded.')
    return data


def fresh_directory(output: Path) -> Path:
    """Create a new destination only; never delete, replace, or merge user files."""
    output = Path(output).absolute()
    core.need(not output.exists() and not output.is_symlink(),
              'Lens output must be a new directory; existing paths are not overwritten.')
    core.need(not any(parent.is_symlink() for parent in output.parents),
              'Lens output cannot pass through a symlink.')
    output.parent.mkdir(parents=True, exist_ok=True)
    output.mkdir()
    return output



def _json(value):
    return (core.canonical(value) + "\n").encode("utf-8")


def _hash(data):
    return sha256(data).hexdigest()


def _integer(value):
    core.need(type(value) is int and 0 <= value <= 1_000_000, "invalid finite work budget")


def validate_request(request):
    core.canonical(request)
    sharing._browser_numbers(request, exact=False)  # Rendering floats are allowed; semantic contexts enforce their own exact profile.
    core.keys(request, {"schema", "library", "registry", "entries", "checks", "proposals", "category_operations"}, where="browser request")
    core.need(request["schema"] == SCHEMA, "unsupported browser request schema")
    A.validate_library(request["library"])
    V.validate_registry(request["registry"])
    core.need(len(request["registry"]["definitions"]) <= 16, "browser registry exceeds 16 definitions")
    chosen = request["entries"]
    core.need(type(chosen) is list and 1 <= len(chosen) <= 8 and all(type(x) is str for x in chosen)
              and len(set(chosen)) == len(chosen), "select 1..8 distinct exact entries")
    available = {core.digest(e) for e in request["library"]["entries"]}
    core.need(set(chosen) <= available, "selected entry is absent or stale")
    core.need(type(request["checks"]) is list and len(request["checks"]) <= 128, "check request budget exceeded")
    seen = set()
    ids = {d["id"] for d in request["registry"]["definitions"]}
    for row in request["checks"]:
        core.keys(row, {"entry_digest", "lens_id", "parameters", "construction_budget", "check_budget"}, where="view check")
        core.need(type(row["entry_digest"]) is str and row["entry_digest"] in chosen and
                  type(row["lens_id"]) is str and row["lens_id"] in ids, "unknown view check endpoint")
        key = (row["entry_digest"], row["lens_id"])
        core.need(key not in seen, "duplicate view check")
        seen.add(key)
        core.need(type(row["parameters"]) is dict, "view parameters must be inert data")
        _integer(row["construction_budget"]); _integer(row["check_budget"])
    core.need(type(request["proposals"]) is list and len(request["proposals"]) <= 32, "proposal budget exceeded")
    for row in request["proposals"]:
        core.keys(row, {"entry_digest", "lens_id", "target_entry", "correspondence"}, where="authored proposal")
        core.need(type(row["entry_digest"]) is str and row["entry_digest"] in chosen, "proposal source is absent")
        V.propose(request["library"], registry=request["registry"], **row)
    core.need(type(request["category_operations"]) is list and len(request["category_operations"]) <= 16,
              "category operation budget exceeded")
    category_seen = set()
    for row in request['category_operations']:
        core.keys(row, {'entry_digest', 'application', 'record'}, where='category attachment')
        core.need(type(row['entry_digest']) is str and row['entry_digest'] in chosen and
                  type(row['application']) is dict and type(row['record']) is dict, 'invalid category attachment endpoint/data')
        key = (row['entry_digest'], core.digest(row['application']))
        core.need(key not in category_seen, 'duplicate category application')
        category_seen.add(key)
    core.need(len(_json(request)) <= MAX_REQUEST, "browser request byte budget exceeded")
    return request


def _producer_identity():
    paths = {"zal_notation/" + p.name: p for p in sorted(ROOT.glob("*.py"))}
    paths["finite_library/prototype.py"] = L.SEMANTIC
    paths.update({"lens_assets/" + name: ROOT / "lens_assets" / name for name in ASSETS})
    paths.update({"reading_assets/" + name: reading_typography.ASSET_ROOT / name
                  for name in reading_typography.ASSETS})
    profile = ROOT.parent / 'category_profile'
    if profile.is_dir():
        paths.update({'category_profile/' + p.name: p for p in profile.glob('*.py') if not p.name.startswith('test_')})
        paths['category_profile/DEPENDENCIES.json'] = profile / 'DEPENDENCIES.json'
        provider = profile / 'provider'
        paths.update({'category_profile/provider/' + str(p.relative_to(provider)): p
                      for p in provider.rglob('*') if p.is_file() and '__pycache__' not in p.parts})
    result = {name: _hash(path.read_bytes()) for name, path in paths.items()}
    return result


def _meaning(entry):
    status = A.inspect_entry(entry)
    return {**status, "label": "Meaning unresolved" if status["status"] == "draft" else "Model linked · laws declared",
            "explanation": "Structure and finite Lens results do not prove this entry's authored interpretation or declared laws."}


def _algebra_operations(application, receipt):
    """Read actual checker-produced algebra; do not invent a native result."""
    algebra = application.get("payload", {}).get("algebra")
    if not algebra or receipt["status"] != "checked":
        return None
    target = application["target_context"]["semantics"]["parameters"]
    result = {key: deepcopy(algebra[key]) for key in
              ("state_order", "generators", "elements", "multiplication", "identity", "units", "composition_order")}
    result["carrier"] = deepcopy(target["carrier"])
    result["scope"] = "Queries of the recorded checked finite effect table; not live execution, trace equivalence or machine automorphisms."
    result["inverses"] = {str(a): next((b for b in result["carrier"]
        if algebra["multiplication"][a][b] == algebra["identity"] == algebra["multiplication"][b][a]), None)
        for a in result["carrier"]}
    result["period_example"] = None
    for generator, effect in algebra["generators"].items():
        if effect not in algebra["units"]:
            continue
        current = algebra["identity"]
        for count in range(1, len(algebra["elements"]) + 1):
            current = algebra["multiplication"][effect][current]
            if current == algebra["identity"]:
                result["period_example"] = {"generator": generator, "repetitions": count,
                    "effect": current, "empty_word_steps": 0, "nonempty_word_steps": count,
                    "claim": "Same final effect, different command-word length; intermediate history was forgotten."}
                break
        if result["period_example"]:
            break
    return result


def _view(library, entry, candidate, options, prefix, files):
    definition = candidate["definition"]
    context = entry["discussion"]["context"] if entry["discussion"] else None
    view = {"id": definition["id"], "name": definition["title"], "type": "proposed",
            "status": candidate["status"], "summary": definition["summary"],
            "preserves": definition["preserves"], "forgets": definition["forgets"], "adds": definition["adds"],
            "relationClaims": "Proposed declarations; no successful check recorded.",
            "assumptions": {"definition": definition["assumptions"], "source": context["assumptions"] if context else []},
            "bounds": [candidate["reason"], "Registered choices only; absence or unsupported is not mathematical impossibility."],
            "mapping": [], "sourceEntryDigest": core.digest(entry), "sourceContextDigest": core.digest(context) if context else None,
            "definitionDigest": candidate["definition_digest"], "sourcePath": prefix + "/source.entry.json",
            "operations": None, "targetContextDigest": None, "targetModelDigest": None}
    if definition["id"] == "parity-quotient" and options and options["parameters"]:
        view["name"] = "State quotient · explicit map"
    if candidate["status"] != "candidate":
        return view
    params = options["parameters"] if options else {}
    construction = options["construction_budget"] if options else 100_000
    check_budget = options["check_budget"] if options else 100_000
    try:
        application = V.apply(library, core.digest(entry), definition["id"], params,
                              budget=construction, registry={"schema": "zal.lens-registry/1", "definitions": [definition]})
        receipt = V.check(library, core.digest(entry), application, check_budget)
        # check itself reconstructs the complete application; replay invokes it freshly again.
    except L.finite.Incomplete:
        view.update(status="incomplete", reason="Construction budget exhausted; no target or successful claim.")
        return view
    except (core.NotationError, L.Unsupported, L.finite.Refused, KeyError, TypeError, ValueError) as exc:
        view.update(status="unsupported", reason=str(exc))
        return view
    view["status"] = receipt["status"]
    inner = application["payload"]
    structural = definition["id"] in ("inductive-syntax", "relational-graph")
    target = inner["target_context"]
    spec = L.SPECS.get(definition["id"])
    view["type"] = "representation" if structural else spec[1]
    view.update(_receipt={"application": application, "receipt": receipt},
                receiptPath=prefix + "/" + definition["id"] + ".receipt.json",
                relationClaims="Exact structural identity only; interpretation/laws stay declared." if structural else
                    "Finite claim only; word/trace/output losses and authored interpretation remain explicit.",
                evidenceScope="structure" if structural else "finite")
    files[view["receiptPath"]] = _json(view["_receipt"])
    view["targetContextDigest"] = core.digest(target) if target else None
    view["targetModelDigest"] = core.model_digest(target["grammar"], target["model"]) if target else None
    view["assumptions"]["target"] = target["assumptions"] if target else []
    if structural:
        view["bounds"].append(receipt.get("evidence", {}).get("claim", "No completed structural result."))
        notation = entry["discussion"]["notation"]
        svg = render.render_svg(target["grammar"], target["model"], notation)
    else:
        contract = L.source_contract(context)
        view["bounds"].append(f"{len(contract.inputs[0].values)} declared states; {len(contract.rows)} total rows; every declared input included.")
        view["lensRevision"] = inner["lens_revision"]
        svg = L.render_application(context, inner) if target else None
        if definition["id"] == "parity-quotient":
            view["mapping"] = [{"source": k, "target": v} for k, v in inner["parameters"]["mapping"].items()]
        if "algebra" in inner.get("payload", {}):
            algebra = inner["payload"]["algebra"]
            view["mapping"] = [{"effect": i, "images": e, "unit": i in algebra["units"]}
                               for i, e in enumerate(algebra["elements"])]
            view["operations"] = _algebra_operations(inner, receipt)
            view["bounds"].append("Effect units are not the machine's symmetry group; that separate construction may be unavailable.")
        if definition["id"] == "finite-table":
            view["_controlledText"] = inner["payload"]["controlled_table"]
            view["controlledTextPath"] = prefix + "/table.txt"
            files[view["controlledTextPath"]] = view["_controlledText"].encode("utf-8")
    if svg:
        view["svgPath"] = prefix + "/" + definition["id"] + ".svg"
        files[view["svgPath"]] = svg.encode("utf-8")
    return view


def _subject_data(library, registry, entry, index, files, checks, proposals, categories, category_host, approved):
    identity = core.digest(entry)
    prefix = f"entries/{index}"
    files[prefix + "/source.entry.json"] = _json(entry)
    files[prefix + "/source.story.txt"] = entry["story"].encode("utf-8")
    catalog = V.discover(library, identity, registry)
    files[prefix + "/catalog.json"] = _json(catalog)
    context = entry["discussion"]["context"] if entry["discussion"] else None
    subject = {"libraryDigest": core.digest(library), "object": {"key": core.digest({"library": core.digest(library), "entry": identity}), "id": entry["id"], "name": entry["title"], "description": entry["purpose"] or entry["story"],
                "entryDigest": identity, "contextDigest": core.digest(context) if context else None,
                "modelDigest": core.model_digest(context["grammar"], context["model"]) if context else None,
                "sourceHash": _hash(entry["story"].encode("utf-8")), "sourcePath": prefix + "/source.entry.json",
                "_sourceText": entry["story"], "entry": entry, "meaning": _meaning(entry)},
               "catalog": catalog, "catalogDigest": core.digest(catalog), "lenses": []}
    source_view = {"id": "retained-source", "name": "Retained authored source", "type": "source", "status": "authored",
            "summary": "Exact retained story, alternatives, requirements and interpretation. Returning here is not inversion.",
            "sourceEntryDigest": identity, "sourceContextDigest": core.digest(context) if context else None,
            "preserves": ["Retained authored source entry"], "forgets": [], "adds": [], "mapping": [],
            "bounds": ["Content identity is not mathematical equivalence or authentication."], "operations": None,
            "sourcePath": prefix + "/source.entry.json", "story": entry["story"], "entry": entry,
            "relationClaims": "Authored interpretation and declared laws; no semantic proof is implied.",
            "assumptions": {"source": context["assumptions"] if context else []}}
    subject["lenses"].append(source_view)
    for candidate in catalog["lenses"]:
        subject["lenses"].append(_view(library, entry, candidate, checks.get((identity, candidate["definition"]["id"])), prefix, files))
    for n, spec in enumerate(row for row in proposals if row["entry_digest"] == identity):
        proposed = V.propose(library, registry=registry, **spec)
        path = prefix + f"/proposal-{n}.json"
        files[path] = _json(proposed)
        subject["lenses"].append({"id": f"authored-proposal-{n}", "name": proposed["definition"]["title"] + " · authored target",
            "type": "proposed", "status": proposed["status"], "summary": proposed["correspondence"]["description"],
            "sourceEntryDigest": identity, "sourceContextDigest": core.digest(context) if context else None,
            "preserves": proposed["definition"]["preserves"], "forgets": proposed["definition"]["forgets"],
            "adds": proposed["definition"]["adds"], "mapping": proposed["correspondence"]["pairs"],
            "bounds": [proposed["map_scope"]], "assumptions": {"definition": proposed["definition"]["assumptions"]},
            "proposal": proposed, "sourcePath": prefix + "/source.entry.json", "targetEntry": proposed["target_entry"],
            "relationClaims": "Proposed labels/map; endpoint meaning and property transfer are not checked.", "operations": None})
    if categories:
        from category_profile.browser import attachment
        for i, row in enumerate(categories):
            if row['entry_digest'] == identity:
                subject['lenses'].append(attachment(library, entry, catalog, row, i, prefix, files, category_host, approved))
    return subject


def artifacts(request, *, category_host=None):
    """Deterministic request/source construction with separately trusted evidence."""
    request = deepcopy(validate_request(request))
    library, registry = request["library"], request["registry"]
    files = {"request.json": _json(request)}
    entries = {core.digest(e): e for e in library["entries"]}
    checks = {(row["entry_digest"], row["lens_id"]): row for row in request["checks"]}
    approved = None
    if request['category_operations']:
        from category_profile.browser import approval
        approved = approval(category_host)
    subjects = []
    for index, identity in enumerate(request["entries"]):
        subjects.append(_subject_data(library, registry, entries[identity], index, files, checks, request["proposals"],
                                      request['category_operations'], category_host, approved))
    core.need(len(subjects) <= 8, "browser subject budget exceeded")
    source_pins = _producer_identity()
    packet = {"schema": "zal.library-lens-browser/1", "subjects": subjects,
        "requestSha256": _hash(files["request.json"]), "libraryDigest": core.digest(library), "registryDigest": core.digest(registry),
        "producerDigest": core.digest(source_pins), "producerSourcePins": source_pins, "categoryApproval": approved,
        "authority": "none", "nativeFcisQualification": "not-performed", "browserChecks": "not-performed",
        "scope": "Read-only recorded entry-bound structural/finite checks; source meaning remains authored; registry is discovery provenance, not admission.",
        "recheckCommand": "python3 -B -m zal_notation.library_lens_browser" + " replay DIRECTORY --request-sha256 " + _hash(files["request.json"])}
    if request['category_operations']:
        packet['recheckCommand'] += (' --category-evidence-catalog OPERATOR_APPROVAL.json'
                                     ' --category-evidence-sha256 OPERATOR_APPROVED_RAW_SHA256')
    for name in ASSETS:
        files[name] = (ROOT / "lens_assets" / name).read_bytes()
    files["styles.css"] += reading_typography.font_css().encode("utf-8")
    files["data/packet.json"] = _json(packet)
    encoded = core.canonical(packet).replace("<", "\\u003c").replace("\u2028", "\\u2028").replace("\u2029", "\\u2029")
    files["data/packet.js"] = ("// Recorded entry-bound data. No live browser checker.\nwindow.ZAL_PACKET = " + encoded + ";\n").encode("utf-8")
    # Semantic artifacts live beside packet data; the shell permits only confined paths.
    files = {(name if name in ASSETS or name.startswith("data/") or name == "request.json" else "data/" + name): data
             for name, data in files.items()}
    files["manifest.json"] = _json({"schema": "zal.library-lens-browser-manifest/1", "requestSha256": packet["requestSha256"],
        "producerDigest": packet["producerDigest"], "files": {name: _hash(data) for name, data in sorted(files.items())},
        "scope": "Content identities only; replay reconstructs bytes, not merely these declared hashes."})
    core.need(len(files) <= MAX_FILES and sum(map(len, files.values())) <= MAX_TOTAL, "browser artifact budget exceeded")
    core.need(all(len(data) <= core.MAX_BYTES for data in files.values()), "browser artifact byte budget exceeded")
    return files, packet


def export(request, output, *, category_host=None):
    files, packet = artifacts(request, category_host=category_host)  # All validation/checking before any output write.
    output = fresh_directory(Path(output))
    for name, data in files.items():
        target = output / name
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open("xb") as stream:
            stream.write(data)
    return {"schema": "zal.library-lens-browser-export/1", "output": str(output), "files": len(files),
            "subjects": len(packet["subjects"]), "request_sha256": packet["requestSha256"],
            "producer_digest": packet["producerDigest"], "browser_checks": "not-performed"}


def replay(directory, expected_request_sha256, *, category_host=None):
    """Reconstruct every expected byte, anchored to a caller's retained subject pin."""
    directory = Path(directory)
    request_bytes = read_artifact(directory, "request.json")
    core.need(type(expected_request_sha256) is str and len(expected_request_sha256) == 64 and
              _hash(request_bytes) == expected_request_sha256, "expected request/source pin differs")
    core.need(len(request_bytes) <= MAX_REQUEST, "browser request byte budget exceeded")
    request = core.loads(request_bytes.decode("utf-8"))
    files, packet = artifacts(request, category_host=category_host)
    actual = set()
    for root, dirs, names in os.walk(directory, followlinks=False):
        core.need(all(not (Path(root) / d).is_symlink() for d in dirs), "browser directory symlink refused")
        for name in names:
            path = Path(root) / name
            core.need(not path.is_symlink(), "browser artifact symlink refused")
            actual.add(path.relative_to(directory).as_posix())
            core.need(len(actual) <= MAX_FILES, "browser inventory budget exceeded")
    core.need(actual == set(files), "browser artifact inventory differs")
    for name, expected in files.items():
        core.need(read_artifact(directory, name) == expected, "reconstructed artifact differs: " + name)
    return {"schema": "zal.library-lens-browser-replay/1", "status": "replayed", "files": len(files),
            "subjects": len(packet["subjects"]), "request_sha256": packet["requestSha256"],
            "scope": "Exact generic entry-bound semantic and browser-byte reconstruction; imported HTML compared, not executed; no browser qualification or authorship authentication."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    build = sub.add_parser("export"); build.add_argument("request"); build.add_argument("output")
    check = sub.add_parser("replay"); check.add_argument("directory"); check.add_argument("--request-sha256", required=True)
    for command in (build, check):
        command.add_argument('--category-evidence-catalog', type=Path, help='trusted operator approval file, never loaded from request/packet')
        command.add_argument('--category-evidence-sha256', help='operator-approved raw catalog SHA256')
    args = parser.parse_args()
    category_host = None
    if args.category_evidence_catalog is not None or args.category_evidence_sha256 is not None:
        from category_profile.host import operator_context
        category_host = operator_context(evidence_catalog=args.category_evidence_catalog,
                                         evidence_sha256=args.category_evidence_sha256)
    if args.action == "export":
        with Path(args.request).open("rb") as stream:
            raw = stream.read(MAX_REQUEST + 1)
        core.need(len(raw) <= MAX_REQUEST, "browser request byte budget exceeded")
        result = export(core.loads(raw.decode("utf-8")), args.output, category_host=category_host)
    else:
        result = replay(args.directory, args.request_sha256, category_host=category_host)
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
