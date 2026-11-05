"""Generate source-pinned, independently replayed finite research results.

Run from this directory: python3 -B run_all.py > results.json
This is a research evidence report, not a certified proof or release gate.
"""
from __future__ import annotations
from hashlib import sha256
import json
from pathlib import Path
import subprocess
import sys

import discovery_loop
import run_representation_cosearch
import representation_transport
import procedure_cosearch

HERE = Path(__file__).resolve().parent
SOURCES = (
    "discovery_loop.py", "representation_cosearch.py", "representation_transport.py",
    "procedure_cosearch.py", "run_representation_cosearch.py", "run_all.py",
    "test_discovery_loop.py", "test_representation_cosearch.py", "test_representation_transport.py",
    "test_procedure_cosearch.py", "test-data/representation_cosearch_cases.json",
)


def run():
    report = {
        "schema": "zeno-fcis/representation-discovery-evidence/2",
        "authority": "none",
        "runtime": {"python_major_minor": f"{sys.version_info.major}.{sys.version_info.minor}"},
        "source_sha256": {name: sha256((HERE / name).read_bytes()).hexdigest() for name in SOURCES},
        "boole": discovery_loop.demo(),
        "feature_procedure_search": run_representation_cosearch.run(),
        "typed_procedure_search": procedure_cosearch.demo(),
        "transport": representation_transport.demo(),
        "qualification": {
            "finite_synthesis_replay": "passed",
            "unbounded_maths_or_real_input_generalization": "not-qualified",
            "native_ZAL_integration": "not-performed",
            "runtime_speedup": "not-established",
        },
    }
    if (report["boole"]["status"] != "bounded-prototype-replayed"
            or not all(v["status"] == "finite-replayed" for v in report["typed_procedure_search"].values())
            or not all(v["direct_replay"] for v in report["transport"]["convolution"].values())
            or report["transport"]["semantics_change"]["status"] != "refuted-by-witness"):
        raise RuntimeError("evidence gate failed")
    return report


if __name__ == "__main__":
    print(json.dumps(run(), indent=2, sort_keys=True))
