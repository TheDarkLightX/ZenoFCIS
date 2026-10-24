#!/usr/bin/env python3
"""Exhaustive bounded binding of structural collection admission to Lean.

Shapes are enumerated by the explicit grammar in BOUNDS, then deduplicated by
their complete structural metadata. Both implementations receive every tuple
in each shape's whole finite domain. No samples or boundary probes are used.
"""
from __future__ import annotations

import argparse
from collections import Counter
import gzip
import hashlib
from itertools import product
import json
from math import prod
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
DRIVER = Path("tools/lean/CollectionsListing.lean")
PROOFS = Path("verification/lean")
MODULES = ("Collections", "CollectionsPatterns", "CollectionsPatternsExecutable")
VERSIONS = ("4.28.0", "4.32.0")
RUST_TEST = "collections::reduction_differential::admissible_listing"
BOUNDS = {
    "maps": [1, 2], "key_types": [0, 1], "input_key_positions_max": 2,
    "entry_roles_max": 2, "total_key_positions_max": 4,
    "numeric_fields_per_map_max": 1, "stored_key_fields_per_map_max": 1,
    "numeric_intervals": [[0, 0], [0, 1], [1, 1]],
    "summaries_max": 1, "summary_intervals": [[0, 0], [0, 1], [0, 2]],
    "tallies_max": 1, "tally_weight": ["count", "numeric_field"],
    "tally_selector": ["all", "numeric_field_equals_one"],
    "ordinary_scalar_positions_max": 0, "ordered_keys": False,
    "state_laws": "arbitrary common truth filter; formula interpretation is outside this differential",
    "field_order": "numeric field precedes stored key; entry roles preserve declaration order",
    "position_order": "summary, entry presence and fields, input keys",
}
SOURCES = (
    Path("crates/zeno-fcis-contract/src/collections.rs"),
    Path("crates/zeno-fcis-contract/src/collections/reduction_differential.rs"),
    Path("crates/zeno-fcis-contract/src/internals/review/domain.rs"),
    DRIVER, *(PROOFS / (module + ".lean") for module in MODULES),
    Path("tools/check_collections_reduction.py"), Path("Cargo.lock"),
)


def digest(path: Path) -> str:
    result = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1 << 20), b""):
            result.update(block)
    return result.hexdigest()


def layout_options():
    amounts = [None, *(tuple(interval) for interval in BOUNDS["numeric_intervals"])]
    return list(product(amounts, (None, 0, 1)))


def encode_shape(records, input_types, bindings, summary, tally):
    key_counts = Counter(input_types)
    key_counts.update(records[map_id][1] for map_id, _ in bindings
                      if records[map_id][1] is not None)
    if sum(key_counts.values()) > BOUNDS["total_key_positions_max"]:
        return None
    domains, entries, summaries = [], [], []

    def position(least, greatest):
        index = len(domains)
        domains.append({"least": least, "greatest": greatest})
        return index

    if summary is not None:
        map_id, field, greatest = summary
        summaries.append({"mapId": map_id, "position": position(0, greatest), "field": field})
    for map_id, input_slot in bindings:
        amount_domain, stored_type = records[map_id]
        presence = position(0, 1)
        fields, stored = [], []
        if amount_domain is not None:
            fields.append(position(*amount_domain))
        if stored_type is not None:
            field = position(0, key_counts[stored_type] - 1)
            fields.append(field)
            stored.append({"position": field, "keyType": stored_type})
        entries.append({"mapId": map_id, "key": input_slot, "presence": presence,
                        "fields": fields, "storedKeys": stored})
    keys = [{"position": position(0, key_counts[key_type] - 1), "keyType": key_type}
            for key_type in input_types]
    for entry in entries:
        entry["key"] = keys[entry["key"]]["position"]
    return {"domains": domains, "keys": keys, "entries": entries,
            "summaries": summaries, "tallies": [] if tally is None else [tally]}


def all_shapes():
    """All metadata shapes of BOUNDS, including unused maps and absent stored keys."""
    shapes = set()
    declarations = 0
    layouts = layout_options()
    for maps in BOUNDS["maps"]:
        for map_types in product(BOUNDS["key_types"], repeat=maps):
            for inputs in range(BOUNDS["input_key_positions_max"] + 1):
                for input_types in product(BOUNDS["key_types"], repeat=inputs):
                    slots = [(m, k) for m in range(maps) for k in range(inputs)
                             if map_types[m] == input_types[k]]
                    for roles in range(BOUNDS["entry_roles_max"] + 1):
                        for bindings in product(slots, repeat=roles):
                            for records in product(layouts, repeat=maps):
                                summaries = [None]
                                for m in range(maps):
                                    fields = [None] + ([0] if records[m][0] is not None else [])
                                    summaries.extend((m, field, upper) for field in fields for upper in range(3))
                                tallies = [None]
                                for target in range(maps):
                                    for source in range(maps):
                                        if (source == target or records[target][0] is None
                                                or records[source][1] != map_types[target]):
                                            continue
                                        numeric = records[source][0] is not None
                                        for weight, selector in product([None, 0] if numeric else [None], repeat=2):
                                            tallies.append({"target": target, "field": 0,
                                                "source": source, "group": int(numeric),
                                                "weight": weight, "selector": selector})
                                for summary, tally in product(summaries, tallies):
                                    shape = encode_shape(records, input_types, bindings, summary, tally)
                                    if shape is not None:
                                        declarations += 1
                                        shapes.add(json.dumps(shape, sort_keys=True, separators=(",", ":")))
    return sorted(shapes), declarations


def validate_shapes(lines):
    tuples, maxima = 0, Counter()
    simultaneous = False
    for line in lines:
        shape = json.loads(line)
        domains, entries = shape["domains"], shape["entries"]
        keys = shape["keys"] + [key for entry in entries for key in entry["storedKeys"]]
        width = len(domains)
        cardinality = prod(d["greatest"] + 1 - d["least"] for d in domains)
        if any(not 0 <= d["least"] <= d["greatest"] <= 3 for d in domains):
            raise ValueError("shape domain is outside the stated grammar")
        if len(keys) > 4 or len(entries) > 2 or len(shape["summaries"]) > 1 or len(shape["tallies"]) > 1:
            raise ValueError("shape count bound exceeded")
        for entry in entries:
            if any(not 0 <= i < width for i in [entry["key"], entry["presence"], *entry["fields"]]):
                raise ValueError("entry position is not in its tuple")
            if entry["key"] not in [key["position"] for key in shape["keys"]]:
                raise ValueError("entry is not named by an input key")
        types = len({key["keyType"] for key in keys})
        maps = len({entry["mapId"] for entry in entries})
        simultaneous |= len(keys) == 4 and types == 2 and maps == 2
        for name, value in {"key_positions": len(keys), "key_types": types, "named_maps": maps,
                            "entry_roles": len(entries), "tuple_width": width,
                            "tuples_per_shape": cardinality}.items():
            maxima[name] = max(maxima[name], value)
        tuples += cardinality
    if not simultaneous:
        raise ValueError("no shape simultaneously has four keys, two types and two maps")
    return {"shapes": len(lines), "tuples": tuples, "maxima": dict(maxima),
            "four_keys_two_types_two_maps": simultaneous}


def run(command, log, env=None):
    started = time.monotonic()
    with log.open("w") as output:
        result = subprocess.run(command, cwd=ROOT, env=env, text=True,
                                stdout=output, stderr=subprocess.STDOUT, timeout=10800)
    event = {"command": list(map(str, command)), "exit": result.returncode,
             "seconds": round(time.monotonic() - started, 3), "log": log.name}
    print(json.dumps(event), flush=True)
    if result.returncode:
        raise RuntimeError(f"command failed: {log.name}\n{log.read_text()[-5000:]}")
    return event


def check_listing(path, expected_shapes):
    admitted = 0
    with path.open() as source:
        for index, line in enumerate(source):
            identity, body = line.rstrip("\n").split(":", 1)
            if int(identity) != index:
                raise ValueError("listing shape identities are not consecutive")
            if body:
                if not body.endswith(","):
                    raise ValueError("listing tuple is not terminated")
                tuples = body[:-1].split(",")
                if any(a >= b for a, b in zip(tuples, tuples[1:])):
                    raise ValueError("listing is not a strictly increasing set")
                admitted += len(tuples)
        count = index + 1 if "index" in locals() else 0
    if count != expected_shapes:
        raise ValueError("listing does not cover every shape")
    return admitted


def compare_listings(left, right):
    """Symmetric byte comparison, with exact extra/missing tuples on failure."""
    with left.open("rb") as lhs, right.open("rb") as rhs:
        index = 0
        while True:
            a, b = lhs.readline(), rhs.readline()
            if a != b:
                def members(row):
                    if not row:
                        return set()
                    body = row.decode().rstrip("\n").split(":", 1)[1]
                    return set(body[:-1].split(",")) if body else set()
                return {"equal": False, "shape": index,
                        "rust_only": sorted(members(a) - members(b))[:16],
                        "lean_only": sorted(members(b) - members(a))[:16]}
            if not a:
                return {"equal": True, "sha256": digest(left), "bytes": left.stat().st_size}
            index += 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--controls", action="store_true")
    parser.add_argument("--versions", nargs="+", choices=VERSIONS, default=list(VERSIONS))
    parser.add_argument("--keep-listings", action="store_true")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    out = args.out.resolve()
    source_hashes = {str(path): digest(ROOT / path) for path in SOURCES}
    lines, declarations = all_shapes()
    counts = validate_shapes(lines)
    print("GRAMMAR " + json.dumps({**counts, "declarations_before_dedup": declarations}), flush=True)
    shapes = out / "shapes.jsonl"
    shapes.write_text("\n".join(lines) + "\n")
    result = {"schema": "zeno-fcis/collections-reduction-differential/1", "bounds": BOUNDS,
              "shape_coverage": counts, "declarations_before_dedup": declarations,
              "source_sha256": source_hashes, "shapes_sha256": digest(shapes),
              "commands": [], "comparisons": {}, "controls": []}

    def save():
        (out / "receipt.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")

    save()
    native = out / "rust.listing"
    native_counts = out / "rust-counts.json"
    native_env = os.environ.copy()
    native_env.update(CRED_SHAPES=str(shapes), CRED_LISTING=str(native), CRED_COUNTS=str(native_counts))
    command = ["cargo", "+1.97.1", "test", "--locked", "--offline", "-p", "zeno-fcis-contract",
               "--lib", RUST_TEST, "--", "--ignored", "--exact", "--nocapture"]
    result["commands"].append(run(command, out / "rust.log", native_env))
    rust_stats = json.loads(native_counts.read_text())
    if rust_stats["shapes"] != counts["shapes"] or rust_stats["tuples"] != counts["tuples"]:
        raise ValueError("native harness omitted shapes or whole-domain tuples")
    if check_listing(native, counts["shapes"]) != rust_stats["admitted"]:
        raise ValueError("native count differs from its canonical listing")
    result["native_counts"] = rust_stats
    save()
    with tempfile.TemporaryDirectory(prefix="collections-binding-", dir=out) as directory:
        temporary = Path(directory)
        snapshot = temporary / "source"
        snapshot.mkdir()
        for module in MODULES:
            (snapshot / (module + ".lean")).write_bytes((ROOT / PROOFS / (module + ".lean")).read_bytes())
        driver = snapshot / DRIVER.name
        original_driver = (ROOT / DRIVER).read_bytes()
        driver.write_bytes(original_driver)
        for version in args.versions:
            build = temporary / ("build-" + version)
            build.mkdir()
            env = os.environ.copy()
            env["LEAN_PATH"] = str(build)
            prefix = ["elan", "run", "leanprover/lean4:v" + version, "lean"]
            for module in MODULES:
                command = prefix + ["--root", str(snapshot), "-o", str(build / (module + ".olean")),
                                    str(snapshot / (module + ".lean"))]
                result["commands"].append(run(command, out / (version + "-" + module + ".log"), env))
            listing, stats = out / (version + ".listing"), out / (version + "-counts.json")
            command = prefix + ["--run", str(driver), str(shapes), str(listing), str(stats)]
            result["commands"].append(run(command, out / (version + "-listing.log"), env))
            lean_stats = json.loads(stats.read_text())
            if lean_stats != rust_stats:
                raise ValueError(f"Lean {version} counts differ: {lean_stats} versus {rust_stats}")
            if check_listing(listing, counts["shapes"]) != lean_stats["admitted"]:
                raise ValueError("Lean count differs from its canonical listing")
            comparison = compare_listings(native, listing)
            result["comparisons"][version] = comparison
            save()
            if not comparison["equal"]:
                raise ValueError(f"admissible sets differ: {comparison}")
            if args.controls and version == args.versions[0]:
                anchor = b"    let selected := domain.filter (structuralAdmissible shape)"
                if original_driver.count(anchor) != 1:
                    raise ValueError("enumeration control anchor is not unique")
                mutant = original_driver.replace(anchor, anchor + b" |>.filter (fun tuple =>\n"
                    b"      (activeKeys shape tuple).all (fun key => valueAt tuple key.position == 0))")
                driver.write_bytes(mutant)
                mutant_listing, mutant_stats = out / "collapsed.listing", out / "collapsed-counts.json"
                mutated_command = prefix + ["--run", str(driver), str(shapes), str(mutant_listing), str(mutant_stats)]
                result["commands"].append(run(mutated_command, out / "collapsed.log", env))
                mismatch = compare_listings(native, mutant_listing)
                if mismatch["equal"] or not mismatch["rust_only"] or mismatch["lean_only"]:
                    raise ValueError("collapsed equality patterns were not detected as missing views")
                driver.write_bytes(original_driver)
                restored_listing, restored_stats = out / "restored.listing", out / "restored-counts.json"
                restored_command = prefix + ["--run", str(driver), str(shapes), str(restored_listing), str(restored_stats)]
                result["commands"].append(run(restored_command, out / "restored.log", env))
                restored = compare_listings(native, restored_listing)
                if not restored["equal"] or driver.read_bytes() != original_driver:
                    raise ValueError("enumeration restoration differs")
                result["controls"].append({"control": "collapse_distinct_equality_patterns",
                    "normal": 0, "mutated_export": 0, "mutated_comparison": 1, "restored": 0,
                    "normal_sha256": hashlib.sha256(original_driver).hexdigest(),
                    "mutated_sha256": hashlib.sha256(mutant).hexdigest(),
                    "restored_sha256": digest(driver), "witness": mismatch})
                save()
    if {str(path): digest(ROOT / path) for path in SOURCES} != source_hashes:
        raise ValueError("differential sources changed during the run")
    result["source_stable"] = True
    result["accepted"] = True
    save()
    if not args.keep_listings:
        for path in [shapes, *out.glob("*.listing")]:
            # Deterministic compression keeps the complete canonical listings.
            with path.open("rb") as source, (out / (path.name + ".gz")).open("wb") as raw:
                with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0) as compressed:
                    shutil.copyfileobj(source, compressed)
            path.unlink()
    print("PASS: exhaustive canonical admissible listings are byte-identical in both directions", flush=True)


if __name__ == "__main__":
    main()
