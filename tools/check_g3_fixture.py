#!/usr/bin/env python3
"""Reproduce all zUSD fixture bytes and source hashes without rewriting them."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def check() -> dict:
    fixture = ROOT / "crates/zeno-fcis-synthesis/tests/fixtures/g3-original-zusd"
    with tempfile.TemporaryDirectory(prefix="g3-fixture-") as temporary:
        out = Path(temporary)
        process = subprocess.run(["python3", "-B", str(ROOT / "tools/test_data/g3_original_zusd/emit_rust.py"),
                                  "--out", str(out)], cwd=ROOT, text=True, capture_output=True, timeout=180)
        if process.returncode:
            raise RuntimeError(process.stderr[-3000:])
        for name in ["SOURCE.json", "data.rs", "native-cases.txt"]:
            if (out / name).read_bytes() != (fixture / name).read_bytes():
                raise RuntimeError(f"fixture drift: {name}")
        report = json.loads((out / "SOURCE.json").read_text())
        for key, path in [
            ("native_source_sha256", ROOT / "tools/test_data/g3_original_zusd/original_zusd.py"),
            ("formatted_rust_data_sha256", fixture / "data.rs"),
            ("native_cases_sha256", fixture / "native-cases.txt"),
        ]:
            if hashlib.sha256(path.read_bytes()).hexdigest() != report[key]:
                raise RuntimeError(f"stale source hash: {key}")
        if report["corpus_rows"] != 8064 or set(report["exercised_reasons"]) != set(range(200, 246)) - {210, 239, 240}:
            raise RuntimeError("corpus omitted a reachable reason")
        if set(report["unreachable_reasons"]) != {"210", "239", "240"}:
            raise RuntimeError("unreachable reason inventory drift")
    return {"status": "passed", "corpus_rows": 8064, "exercised_reasons": 43,
            "registered_reasons": 46, "byte_identical_files": 3,
            "scope": "finite characterization and fixture reproduction"}


if __name__ == "__main__":
    from telemetry import run_cli
    def _entrypoint():
        print(json.dumps(check()))
    run_cli(_entrypoint)
