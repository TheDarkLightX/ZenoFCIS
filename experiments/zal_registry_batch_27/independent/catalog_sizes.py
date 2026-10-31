"""Measure the actual pinned default_registry functions with synthetic suppliers.

Only test suppliers vary; the producer and validators are not rewritten. A
catalog always includes typed-category, so zero definitions is not supported.
The 15-definition point uses the original suppliers. Other points are synthetic
fixtures, not a supported configurable catalog API or an asymptotic theorem.
"""
from __future__ import annotations

import argparse
from contextlib import contextmanager
from copy import deepcopy
import hashlib
import importlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import resource
import statistics
import sys
import time

SIZES = (1, 2, 3, 4, 8, 15, 16, 32, 64)
FIELDS = ("assumptions", "preserves", "forgets", "adds", "profile_hints")


def load(root, alias):
    path = root / "integrations/zal/zal_notation"
    spec = importlib.util.spec_from_file_location(alias, path / "__init__.py",
                                                submodule_search_locations=[str(path)])
    package = importlib.util.module_from_spec(spec)
    sys.modules[alias] = package
    spec.loader.exec_module(package)
    return (importlib.import_module(alias + ".lens_library"),
            importlib.import_module(alias + ".lenses"))


@contextmanager
def suppliers(V, L, count):
    if type(count) is not int or count < 1:
        raise ValueError("fixture count must be a positive exact integer")
    structural, specs = V._structural_definitions, L.SPECS
    if count == 15:
        yield
        return
    structural_rows = structural()[:min(3, count - 1)]

    def fresh_structural():
        return [V.definition(row["id"], row["title"], summary=row["summary"],
                             mode=row["mode"], **{field: row[field] for field in FIELDS})
                for row in structural_rows]

    template = next(iter(specs.values()))
    V._structural_definitions = fresh_structural
    L.SPECS = {"synthetic-" + str(i).zfill(2): deepcopy(template) for i in range(max(0, count - 4))}
    try:
        yield
    finally:
        V._structural_definitions, L.SPECS = structural, specs


def counts(V):
    result = {name: 0 for name in ("validate_definition", "validate_registry", "register")}
    codes = {getattr(V, name).__code__: name for name in result}

    def profile(frame, event, arg):
        if event == "call" and frame.f_code in codes:
            result[codes[frame.f_code]] += 1

    sys.setprofile(profile)
    try:
        registry = V.default_registry()
    finally:
        sys.setprofile(None)
    return registry, result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline-root", required=True, type=Path)
    parser.add_argument("--candidate-root", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    resource.setrlimit(resource.RLIMIT_CPU, (180, 180))
    resource.setrlimit(resource.RLIMIT_AS, (1024**3, 1024**3))
    os.sched_setaffinity(0, {min(os.sched_getaffinity(0))})
    with args.output.open("x") as stream:
        result = {"schema": "zal/independent-catalog-size-sweep/1", "pass": False,
                  "python": sys.version, "platform": platform.platform(), "affinity": sorted(os.sched_getaffinity(0)),
                  "timing": "30 interleaved observations per backend per size, AB then BA alternating; no profiling during timing",
                  "scope": __doc__, "sizes": []}
        try:
            modules = {"baseline": load(args.baseline_root.resolve(), "baseline_zal"),
                       "candidate": load(args.candidate_root.resolve(), "candidate_zal")}
            result["producer_sha256"] = {role: hashlib.sha256(Path(V.__file__).read_bytes()).hexdigest()
                                         for role, (V, L) in modules.items()}
            for size in SIZES:
                row = {"definitions": size, "backends": {}}
                result["sizes"].append(row)
                for role, (V, L) in modules.items():
                    with suppliers(V, L, size):
                        catalog, work = counts(V)
                        if len(catalog["definitions"]) != size:
                            raise RuntimeError("supplier fixture constructed wrong size")
                        canonical = V.core.canonical(catalog)
                        row["backends"][role] = {"validation_calls": work, "samples_ns": [],
                            "catalog_sha256": hashlib.sha256(canonical.encode()).hexdigest(),
                            "catalog_encoded_bytes": len((canonical + "\n").encode())}
                for index in range(30):
                    for role in (("baseline", "candidate") if index % 2 == 0 else ("candidate", "baseline")):
                        V, L = modules[role]
                        with suppliers(V, L, size):
                            start = time.perf_counter_ns()
                            catalog = V.default_registry()
                            elapsed = time.perf_counter_ns() - start
                            encoded = V.core.canonical(catalog).encode()
                        if hashlib.sha256(encoded).hexdigest() != row["backends"][role]["catalog_sha256"]:
                            raise RuntimeError("timed catalog changed")
                        row["backends"][role]["samples_ns"].append(elapsed)
                for data in row["backends"].values():
                    data["median_ns"] = statistics.median(data["samples_ns"])
                    data["min_ns"], data["max_ns"] = min(data["samples_ns"]), max(data["samples_ns"])
                a, b = row["backends"]["baseline"], row["backends"]["candidate"]
                if a["catalog_sha256"] != b["catalog_sha256"]:
                    raise RuntimeError("catalog differential mismatch")
                row["median_speedup"] = a["median_ns"] / b["median_ns"]
                print(json.dumps({"size": size, "baseline_ms": a["median_ns"] / 1e6,
                                  "candidate_ms": b["median_ns"] / 1e6, "ratio": row["median_speedup"]}), flush=True)
            result["pass"] = True
        except BaseException as error:
            result["failure"] = {"type": type(error).__name__, "message": str(error)}
        finally:
            stream.write(json.dumps(result, indent=2, sort_keys=True) + "\n")
    return 0 if result["pass"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
