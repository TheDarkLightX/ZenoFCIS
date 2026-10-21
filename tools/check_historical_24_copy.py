#!/usr/bin/env python3
"""Check the recorded compilation edits against the exact 2.4.0 tag subject.

Recording is a review preparation operation. A record is not approval of its
edits. Checking reconstructs each copied file from the tag subject and every
recorded edit, then compares both its bytes and its SHA-256.
"""
from __future__ import annotations

import argparse
import difflib
import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
SUBJECT = "8c444487f96d6acd14c9f7fa55d9ba88ca780fb6"
PREFIX = Path("crates/zeno-fcis-synthesis/src/finite")
COPIES = PREFIX / "evaluation/historical_i64"
RECORD = ROOT / "release/historical-24-copy.json"
FIXTURE = ROOT / "crates/zeno-fcis-synthesis/tests/test-data/historical_24_copy.rs"
BINDINGS = ROOT / "crates/zeno-fcis-synthesis/src/finite/historical_24_sources.rs"
TEST_METADATA = ROOT / "crates/zeno-fcis-synthesis/tests/test-data/released_24_contract.rs"
SHARED_METER = "crates/zeno-fcis-synthesis/src/finite/execution_v2/meter.rs"
LOCAL_BINDINGS = [
    "crates/zeno-fcis-synthesis/src/finite/evaluation/historical_i64.rs",
    "crates/zeno-fcis-synthesis/src/finite/execution_v2/historical.rs",
    "crates/zeno-fcis-synthesis/src/finite_runtime.rs",
]


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def original(path: str) -> bytes:
    return subprocess.check_output(["git", "show", f"{SUBJECT}:{path}"], cwd=ROOT)


def record() -> dict:
    rows = []
    for target in sorted((ROOT / COPIES).rglob("*.rs")):
        source = ("crates/zeno-fcis-cli/templates/order-fulfillment/synthesized/transition.rs"
                  if target.name == "order_fixture.rs"
                  else str(PREFIX / target.relative_to(ROOT / COPIES)))
        before = original(source)
        after = target.read_bytes()
        old_lines = before.decode().splitlines(keepends=True)
        new_lines = after.decode().splitlines(keepends=True)
        changes = []
        for kind, a, b, c, d in difflib.SequenceMatcher(a=old_lines, b=new_lines, autojunk=False).get_opcodes():
            if kind != "equal":
                changes.append({"start": a, "end": b,
                                "before": old_lines[a:b], "after": new_lines[c:d]})
        rows.append({"source": source, "copy": str(target.relative_to(ROOT)),
                     "source_sha256": digest(before), "copy_sha256": digest(after),
                     "changes": changes})
    return {"schema": "zeno-fcis/historical-copy/1", "tag": "v2.4.0",
            "subject": SUBJECT, "files": rows,
            "shared_meter_sha256": digest((ROOT / SHARED_METER).read_bytes()),
            "binding_sources": [{"path": path, "sha256": digest((ROOT / path).read_bytes())}
                                for path in LOCAL_BINDINGS]}


def check(document: dict) -> dict:
    if document.get("subject") != SUBJECT or document.get("tag") != "v2.4.0":
        raise RuntimeError("historical source must be the closed 2.4.0 release subject")
    meter = original(SHARED_METER)
    relocated_meter = meter.replace(b"pub(super)", b"pub(crate)")
    compiled_meter = (ROOT / SHARED_METER).read_bytes()
    if compiled_meter not in (meter, relocated_meter) or digest(compiled_meter) != document.get("shared_meter_sha256"):
        raise RuntimeError("shared meter differs beyond the recorded compilation visibility edit")
    expected_paths = {row["copy"] for row in document["files"]}
    actual_paths = {str(path.relative_to(ROOT)) for path in (ROOT / COPIES).rglob("*.rs")}
    if expected_paths != actual_paths:
        raise RuntimeError("copied source membership differs from the record")
    if [row["path"] for row in document.get("binding_sources", [])] != LOCAL_BINDINGS:
        raise RuntimeError("historical binding source membership differs")
    for row in document["binding_sources"]:
        if digest((ROOT / row["path"]).read_bytes()) != row["sha256"]:
            raise RuntimeError("historical binding source changed; regenerate its reviewed record")
    for row in document["files"]:
        raw = original(row["source"])
        if digest(raw) != row["source_sha256"]:
            raise RuntimeError(f"tag source hash differs: {row['source']}")
        lines = raw.decode().splitlines(keepends=True)
        restored = []
        position = 0
        for change in row["changes"]:
            start, end = change["start"], change["end"]
            if not position <= start <= end <= len(lines) or lines[start:end] != change["before"]:
                raise RuntimeError(f"recorded source lines differ: {row['source']}")
            restored.extend(lines[position:start])
            restored.extend(change["after"])
            position = end
        restored.extend(lines[position:])
        expected = "".join(restored).encode()
        copied = (ROOT / row["copy"]).read_bytes()
        if copied != expected or digest(copied) != row["copy_sha256"]:
            raise RuntimeError(f"historical copy differs after recorded edits: {row['copy']}")
    return {"status": "passed", "subject": SUBJECT, "files": len(document["files"])}


def fixture(document: dict, destination: Path = FIXTURE) -> str:
    lines = ["//! Generated by tools/check_historical_24_copy.py --record; do not edit.",
             "pub(super) const SOURCES: &[(&str, &[u8], &str)] = &["]
    rows = [(row["copy"], row["copy_sha256"]) for row in document["files"]]
    rows.append((SHARED_METER, document["shared_meter_sha256"]))
    rows.extend((row["path"], row["sha256"]) for row in document["binding_sources"])
    for name, sha256 in rows:
        relative = os.path.relpath(ROOT / name, destination.parent)
        lines.append(f"    ({json.dumps(name)}, include_bytes!({json.dumps(relative)}), {json.dumps(sha256)}),")
    return "\n".join([*lines, "];", ""])


def bindings(document: dict) -> str:
    return fixture(document, BINDINGS)


def test_metadata() -> str:
    source = original("crates/zeno-fcis-cli/tests/fixtures/withdrawal-queue-adopted/src/v2_contract_v1.rs").decode()
    end = source.rfind("\n}\n", 0, source.index("pub enum BindFailure")) + 3
    source = source[:end].replace("zeno_fcis_synthesis::finite::", "zeno_fcis_synthesis::finite::historical_i64::")
    source = source.replace(" v2_authority as authority,", "")
    source = source.replace('"../v2/', '"../../../zeno-fcis-cli/tests/fixtures/withdrawal-queue-adopted/v2/')
    return "// Generated released declaration data by tools/check_historical_24_copy.py --record.\n" + source


def report(document: dict) -> str:
    lines = ["Released 2.4.0 invocation evaluator copy", "", "Subject: " + SUBJECT,
             "", "Every edit is recorded below; source and copy hashes are in HISTORICAL_COPY.json.",
             "Evaluation bodies are copied. Relocation and visibility edits bind a private released constructor."]
    meter = original(SHARED_METER).decode().splitlines(keepends=True)
    compiled_meter = (ROOT / SHARED_METER).read_text().splitlines(keepends=True)
    if meter != compiled_meter:
        lines.extend(["", "Shared meter compilation visibility (no body edits):", "```diff",
            "".join(difflib.unified_diff(meter, compiled_meter, fromfile=SHARED_METER,
                tofile=SHARED_METER)).rstrip(), "```"])
    for row in document["files"]:
        if not row["changes"]:
            continue
        before = original(row["source"]).decode().splitlines(keepends=True)
        after = (ROOT / row["copy"]).read_text().splitlines(keepends=True)
        lines.extend(["", "```diff", "".join(difflib.unified_diff(before, after,
                      fromfile=row["source"], tofile=row["copy"])).rstrip(), "```"])
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--record", action="store_true", help="prepare a new compilation-edit record for review")
    parser.add_argument("--report", type=Path, help="write the complete recorded copy edits for review")
    args = parser.parse_args()
    if args.record:
        document = record()
        check(document)
        RECORD.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n")
        FIXTURE.parent.mkdir(parents=True, exist_ok=True)
        FIXTURE.write_text(fixture(document))
        BINDINGS.write_text(bindings(document))
        TEST_METADATA.write_text(test_metadata())
    document = json.loads(RECORD.read_text())
    result = check(document)
    if FIXTURE.read_text() != fixture(document):
        raise RuntimeError("generated source-hash fixture differs; regenerate through --record")
    if BINDINGS.read_text() != bindings(document):
        raise RuntimeError("generated runtime source binding differs; regenerate through --record")
    if TEST_METADATA.read_text() != test_metadata():
        raise RuntimeError("generated released test declarations differ; regenerate through --record")
    if args.report:
        args.report.write_text(report(document))
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    from telemetry import run_cli
    def _entrypoint():
        raise SystemExit(main())
    run_cli(_entrypoint)
