"""End-to-end, profile-scoped Tier-1 Cranelift research catalog probe.

This module never modifies compiler or ZenoFCIS main sources. Its only authority
is a freshly replayable record of the exact x86-64 SysV profile-2 model.
No finding about Cranelift's emitted code is implied by a raw-byte comparison.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))

ZENO_BASE = "31afaef8d9fb723b44918f430e10d63f0cb93ccf"
UPSTREAM = "b1f81d2ad4fc64fbe0e70a7cc5fccac1a7864c49"
SOURCE_DIR = ROOT / "test-data/machine-code/kernels"
SOURCES = ("signed-saturating-add-x86", "signed-saturating-add-adc-x86")
SCHEMA = "zeno-fcis/cranelift-tier1-catalog/1"


def canonical(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"),
                      ensure_ascii=True, allow_nan=False).encode("ascii")


def sha(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def meaning(problem: dict) -> dict:
    """Profile-2 observable contract, explicitly excluding search grammar/cost."""
    if problem.get("schema") != "zeno-fcis/machine-problem/2":
        raise ValueError("only profile-2 word semantics admitted")
    if problem.get("target") != "x86_64-sysv-register/2" or problem.get("inputs") != ["x", "y"]:
        raise ValueError("profile target/input ABI changed")
    if len(problem.get("outputs", [])) != 1:
        raise ValueError("only one-result semantics admitted")
    return {"schema": problem["schema"], "target": problem["target"],
            "inputs": problem["inputs"], "outputs": problem["outputs"],
            "observations": "one returned u64 in rax; modeled defined flag reads required",
            "word_bits": 64, "signed_interpretation": "two's complement",
            "overflow": "saturated to signed i64 min/max, not wrap/refuse",
            "poison_or_undefined": "no CLIF poison modeling; x86 undefined-flag reads refused by profile checker",
            "initial_state": "all unmodeled scratch contents and modeled CF,ZF,SF,OF as declared in checker",
            "domain": "all pairs of 64-bit words x,y under the registered ISA model"}


def key(semantic: dict, code: bytes) -> str:
    return sha(canonical({"semantic": semantic, "code_hex": code.hex()}))


def put(record: dict, semantic: dict, name: str, code: bytes, result: dict, source: str) -> None:
    """Content-key dedup; same bytes+contract may not overwrite another result."""
    identity = key(semantic, code)
    if result["status"] != "equivalent":
        raise ValueError("cannot catalog an unverified result")
    checked_cost = result["cost"]
    if checked_cost["bytes"] != len(code):
        raise ValueError("checker cost disagrees with exact bytes")
    incoming = {"key": identity, "semantic": semantic, "code_hex": code.hex(),
                "code_sha256": sha(code), "cost": checked_cost,
                "verification": result, "provenance": [{"name": name, "path": source}]}
    if identity in record:
        old = record[identity]
        if any(old[field] != incoming[field] for field in
               ("semantic", "code_hex", "code_sha256", "cost", "verification")):
            raise ValueError("deduplication key collision or inconsistent receipt")
        if incoming["provenance"][0] not in old["provenance"]:
            old["provenance"].append(incoming["provenance"][0])
    else:
        record[identity] = incoming


def store(path: Path, value: dict) -> None:
    path.write_bytes(canonical(value) + b"\n")


def _checker():
    from tools.machine_code import session
    return session


def _git_head() -> str:
    process = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT,
                             capture_output=True, text=True, check=False)
    return process.stdout.strip() if process.returncode == 0 else "unknown-local-checkout"


def run(output: Path) -> dict:
    from tools.machine_code import library, session
    from tools.machine_code.model import read_json

    if output.exists():
        raise ValueError("output already exists; use an entirely fresh directory")
    output.mkdir(parents=True, mode=0o700)
    identity = session.identity()  # includes source hashes and solver library input hashes
    retained = {}
    loaded = []
    contract = None
    for name in SOURCES:
        source_path = SOURCE_DIR / (name + ".json")
        item = read_json(source_path)
        problem, code = library._validate(item, name)
        semantic = meaning(problem)
        if contract is None:
            contract = semantic
        elif canonical(contract) != canonical(semantic):
            raise ValueError("the two variants do not have an identical semantic contract")
        verdict = session._checker(problem).check(problem, code, timeout_ms=10000)
        if verdict["status"] != "equivalent":
            raise ValueError(f"{name}: independent checker returned {verdict['status']}")
        if verdict["cost"] != item["cost"]:
            raise ValueError(f"{name}: declared cost differs from checker")
        put(retained, semantic, name, code, verdict, str(source_path.relative_to(ROOT)))
        loaded.append((name, source_path, problem, code))
    if len(retained) != 2:
        raise ValueError("expected two distinct, independently checked x86 sequences")

    name, source, problem, code = loaded[1]
    session_dir = output / "session"
    input_problem = output / "search-problem.json"
    store(input_problem, problem)
    opened = session.open_session(input_problem, session_dir)
    proposal_path = output / "proposed-22.json"
    store(proposal_path, {"schema": "zeno-fcis/machine-proposal/1",
                         "code_hex": code.hex(),
                         "provenance": {"kind": "human", "name": "pre-existing-library-candidate",
                                        "note": "already-published 22-byte proposal; NOT discovered by this search"}})
    accepted = session.submit(session_dir, proposal_path, timeout_ms=3000)
    if accepted["status"] != "equivalent":
        raise ValueError("previously checked 22-byte proposal failed fresh session")
    searched = session.search(session_dir, max_candidates=1500, max_checks=12,
                              deadline_ms=9000, timeout_ms=500, prune=True)
    emitted = session.emit(session_dir, output / "artifact", timeout_ms=3000)
    if emitted.get("status") != "emitted":
        raise ValueError("no emitted artifact available for independent replay")
    replayed = session.replay_artifact(output / "artifact", timeout_ms=3000)
    if replayed.get("status") != "replayed":
        raise ValueError("artifact did not independently replay")
    emitted_code = (output / "artifact/kernel.bin").read_bytes()
    checked = session._checker(problem).check(problem, emitted_code, timeout_ms=10000)
    if checked["status"] != "equivalent":
        raise ValueError("emitted candidate did not recheck independently")
    put(retained, contract, "bounded-search-incumbent", emitted_code, checked,
        "session/events.jsonl")

    old = retained[key(contract, loaded[0][3])]
    new = retained[key(contract, loaded[1][3])]
    expected_old_sha = "214c51e666af6d4b9370537a3a66f1a40035c3bbb0b0d3ed3430b721afd25c15"
    expected_new_sha = "823ee5322b076a7b0a04997c0b1404e33645d58b29822a5d76cd58efad4b6425"
    if old["code_sha256"] != expected_old_sha or new["code_sha256"] != expected_new_sha:
        raise ValueError("pinned native-kernel bytes differ")
    catalog = {
        "schema": SCHEMA, "authority": "none",
        "zenofcis_base_commit": ZENO_BASE, "wasmtime_upstream_commit": UPSTREAM,
        "experiment_commit": _git_head(), "checker_source_solver_identity": identity,
        "source_files": {name: {"path": str(path.relative_to(ROOT)),
                                 "file_sha256": sha(path.read_bytes())}
                         for name, path, _, _ in loaded},
        "comparison": {
            "metric": "stand-alone x86 machine-code bytes including ret; NOT Cranelift native CLIF bytes",
            "old": old["cost"], "new": new["cost"],
            "native_byte_difference": old["cost"]["bytes"] - new["cost"]["bytes"],
            "cranelift_emitted_baseline": "not measured by this Python program",
            "cranelift_optimization": "not attempted"
        },
        "search": {"source": "ZenoFCIS session.search with stored independently checked proposal",
                   "max_candidates": 1500, "max_checks": 12, "deadline_ms": 9000,
                   "solver_ms": 500, "result": searched,
                   "warning": "22-byte proposal predates search; status/cost not global optimality"},
        "emission": {"status": emitted["status"], "replay_status": replayed["status"],
                     "emitted_sha256": sha(emitted_code),
                     "artifact_relative_path": "artifact"},
        "entries": sorted(retained.values(), key=lambda r: r["key"]),
        "nonclaims": ["No Cranelift byte reduction established", "No ISLE rewrite implemented",
                      "No proof of complete physical x86 ISA", "No worldwide timing claim",
                      "No claim of global optimality", "SMT under modeled profile differs from native finite tests"]
    }
    store(output / "catalog.json", catalog)
    replay_catalog(catalog, output / "artifact")
    return catalog



def validate_catalog_envelope(catalog: dict) -> None:
    """Reject altered/missing baseline rows, pins or cost comparisons without Z3.

    Semantic verification of every candidate and every embedded solver receipt
    remains the separate, mandatory replay_catalog() step. Search timings and
    a historical search stop reason are not themselves an equivalence proof.
    """
    if catalog.get("schema") != SCHEMA or catalog.get("authority") != "none":
        raise ValueError("catalog schema or authority changed")
    if (catalog.get("zenofcis_base_commit") != ZENO_BASE
            or catalog.get("wasmtime_upstream_commit") != UPSTREAM):
        raise ValueError("catalog source revision pins changed")
    if (catalog.get("experiment_commit") != _git_head()):
        raise ValueError("catalog experiment revision differs from checkout")
    expected_sources = {}
    expected_rows = {}
    for name in SOURCES:
        path = SOURCE_DIR / (name + ".json")
        raw = path.read_bytes()
        item = json.loads(raw)
        if item.get("name") != name:
            raise ValueError("baseline source identity changed")
        contract = meaning(item["problem"])
        code = bytes.fromhex(item["code_hex"])
        expected_sources[name] = {"path": str(path.relative_to(ROOT)),
                                  "file_sha256": sha(raw)}
        expected_rows[key(contract, code)] = {
            "name": name, "path": str(path.relative_to(ROOT)),
            "semantic": contract, "code_hex": code.hex(),
            "code_sha256": sha(code), "cost": item["cost"],
        }
    if catalog.get("source_files") != expected_sources:
        raise ValueError("baseline input hashes or paths changed")
    rows = catalog.get("entries")
    if not isinstance(rows, list) or len(rows) < len(SOURCES):
        raise ValueError("missing required baseline catalog entries")
    rows_by_key = {row.get("key"): row for row in rows if isinstance(row, dict)}
    if len(rows_by_key) != len(rows):
        raise ValueError("malformed or duplicate catalog entries")
    for identity, expected in expected_rows.items():
        row = rows_by_key.get(identity)
        if row is None:
            raise ValueError("pinned source candidate omitted from catalog")
        for field in ("semantic", "code_hex", "code_sha256", "cost"):
            if row.get(field) != expected[field]:
                raise ValueError(f"baseline {expected['name']} {field} changed")
        if (row.get("verification", {}).get("status") != "equivalent"
                or row["verification"].get("cost") != expected["cost"]):
            raise ValueError("baseline checker receipt cost or status altered")
        provenance = row.get("provenance")
        if (not isinstance(provenance, list)
                or {"name": expected["name"], "path": expected["path"]}
                not in provenance):
            raise ValueError("baseline provenance missing")
    original = [json.loads((SOURCE_DIR / (name + ".json")).read_text())
                for name in SOURCES]
    old_cost, new_cost = original[0]["cost"], original[1]["cost"]
    expected_comparison = {
        "metric": "stand-alone x86 machine-code bytes including ret; NOT Cranelift native CLIF bytes",
        "old": old_cost, "new": new_cost,
        "native_byte_difference": old_cost["bytes"] - new_cost["bytes"],
        "cranelift_emitted_baseline": "not measured by this Python program",
        "cranelift_optimization": "not attempted",
    }
    if catalog.get("comparison") != expected_comparison:
        raise ValueError("catalog native cost comparison or scope changed")
    emission = catalog.get("emission")
    if (not isinstance(emission, dict)
            or emission.get("status") != "emitted"
            or emission.get("replay_status") != "replayed"
            or emission.get("artifact_relative_path") != "artifact"
            or emission.get("emitted_sha256") not in
            {row.get("code_sha256") for row in rows}):
        raise ValueError("catalog emission reference inconsistent with catalog entries")


def replay_catalog(catalog: dict, artifact_dir: Path | None = None) -> None:
    """Recheck every catalog row with current independent checker and source identity."""
    from tools.machine_code import session
    validate_catalog_envelope(catalog)
    if catalog["checker_source_solver_identity"] != session.identity():
        raise ValueError("checker or solver sources have changed")
    original = {n: session._validated(read_entry(SOURCE_DIR / (n + ".json")))
                for n in SOURCES}
    seen = set()
    for entry in catalog["entries"]:
        if entry["key"] in seen:
            raise ValueError("duplicate catalog key")
        seen.add(entry["key"])
        raw = bytes.fromhex(entry["code_hex"])
        if entry["code_sha256"] != sha(raw) or entry["key"] != key(entry["semantic"], raw):
            raise ValueError("catalog code or key changed")
        matching = [p for p in original.values() if meaning(p) == entry["semantic"]
                    and all(i.form() in p["grammar"] for i in session._checker(p).decode(p["target"], raw)[:-1])]
        if not matching:
            raise ValueError("no pinned grammar admits the catalog candidate")
        verdict = session._checker(matching[0]).check(matching[0], raw, timeout_ms=10000)
        if verdict != entry["verification"] or verdict["cost"] != entry["cost"]:
            raise ValueError("fresh verification differs from receipt")
    if artifact_dir is not None:
        replayed = session.replay_artifact(artifact_dir, timeout_ms=3000)
        if replayed.get("status") != "replayed":
            raise ValueError("emitted artifact no longer independently replays")
        code = (artifact_dir / "kernel.bin").read_bytes()
        if sha(code) != catalog["emission"]["emitted_sha256"]:
            raise ValueError("emitted artifact bytes differ from catalog")


def read_entry(path: Path) -> dict:
    from tools.machine_code.model import read_json
    return read_json(path)["problem"]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", type=Path, help="create fresh result directory")
    parser.add_argument("--replay", type=Path, help="recheck previous catalog against current pinned sources")
    args = parser.parse_args()
    if (args.out is None) == (args.replay is None):
        parser.error("choose exactly one of --out or --replay")
    try:
        if args.replay:
            replay_catalog(json.loads(args.replay.read_text()), args.replay.parent / "artifact")
            print(json.dumps({"status": "replayed", "catalog": str(args.replay)}))
        else:
            result = run(args.out)
            print(json.dumps({"status": "checked-and-replayed",
                              "entries": len(result["entries"]),
                              "search_status": result["search"]["result"]["status"],
                              "bytes_difference_between_raw_kernels": result["comparison"]["native_byte_difference"],
                              "cranelift_savings": "not established"}, sort_keys=True))
    except (ValueError, KeyError, OSError) as exc:
        print(json.dumps({"status": "refused", "reason": str(exc), "authority": "none"}))
        raise SystemExit(1) from exc


if __name__ == "__main__":
    main()
