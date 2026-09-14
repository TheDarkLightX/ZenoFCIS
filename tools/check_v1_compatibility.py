#!/usr/bin/env python3
"""Check unchanged V1 sources and the separately qualified receipt refactor.

This source guard does not execute tests or establish semantic equivalence.
"""

import hashlib
import json
from pathlib import Path
import sys
import textwrap

ROOT = Path(__file__).resolve().parents[1]
BASELINE = "test-data/v1-compatibility/baseline.json"
BASELINE_SHA256 = "64e5786c564a081c176a4242de34d6213d1787adf1bd133c5ae40b9e5afb4781"
QUALIFICATION = "test-data/v1-compatibility/receipt-refactor.json"
ORACLE = "test-data/v1-compatibility/receipt-v1.rs"
RECEIPT = "crates/zeno-fcis-receipt/src/lib.rs"
TEST_SOURCE = "crates/zeno-fcis-receipt/src/validation_tests.rs"
TEST_NAMES = (
    "validation_tests::every_body_identity_and_receipt_field_retains_its_check",
    "validation_tests::sealing_and_validation_match_released_results_and_bytes",
    "validation_tests::simultaneous_failures_keep_the_released_first_error",
)


def selected_path(relative: str, package_roots: dict[str, Path] | None) -> Path:
    path = Path(relative)
    if package_roots is not None and path.parts[0] == "crates":
        # Never replace an absent or changed archive file with checkout bytes.
        if path.parts[1] not in package_roots:
            raise RuntimeError(f"missing packaged compatibility input: {relative}")
        return package_roots[path.parts[1]].joinpath(*path.parts[2:])
    return ROOT / path


def checked_bytes(path: Path, item: dict, label: str) -> bytes:
    if path.is_symlink() or not path.is_file():
        raise RuntimeError(f"missing or linked compatibility input: {label}")
    data = path.read_bytes()
    if len(data) != item["bytes"] or hashlib.sha256(data).hexdigest() != item["sha256"]:
        raise RuntimeError(f"V1 compatibility source changed: {label}")
    return data


def method(source: str, name: str) -> str:
    start = f"    pub fn {name}<"
    if source.count(start) != 1:
        raise RuntimeError(f"receipt method boundary changed: {name}")
    begin = source.index(start)
    end = source.index("\n    }\n}", begin) + len("\n    }")
    return source[begin:end]


def check_receipt_regions(candidate: bytes, oracle: bytes, tests: bytes) -> None:
    """Restore only the reviewed edits, then require the complete released file."""
    current, released, test_source = (data.decode("utf-8") for data in (candidate, oracle, tests))
    old_seal = method(released, "seal")
    old_validate = method(released, "validate_and_apply")
    for name, original in (("seal", old_seal), ("validate_and_apply", old_validate)):
        changed = method(current, name)
        if changed.split(" {\n", 1)[0] != original.split(" {\n", 1)[0]:
            raise RuntimeError(f"receipt public signature changed: {name}")
        current = current.replace(changed, original, 1)
    start = ("// Keep the existing failure order: reason, patch application, component hashes,\n"
             "// then candidate hash. The returned state is exactly the one whose root is sealed.\n"
             "#[allow(clippy::too_many_arguments)]\n")
    end = "fn validate_decision_reason("
    declaration = "#[cfg(test)]\nmod validation_tests;\n\n"
    if current.count(start) != 1 or current.count(end) != 1 or current.count(declaration) != 1:
        raise RuntimeError("receipt helper or test-module boundary changed")
    begin, next_function = current.index(start), current.index(end)
    function = begin + len(start)
    if not current[function:].startswith("fn apply_and_derive_receipt<"):
        raise RuntimeError("receipt private helper boundary changed")
    finish = current.index("\n}", function) + len("\n}")
    if current[finish:next_function] != "\n\n":
        raise RuntimeError("receipt private helper boundary changed")
    current = current[:begin] + current[next_function:]
    current = current.replace(declaration, "", 1)
    if current != released:
        raise RuntimeError("receipt protected source differs from V1")

    reference_seal = textwrap.dedent(old_seal).replace("pub fn seal<", "fn released_seal<", 1)
    reference_validate = (textwrap.dedent(old_validate)
                          .replace("pub fn validate_and_apply<", "fn released_validate<", 1)
                          .replace("    &self,", "    bundle: &CommitBundle,", 1)
                          .replace("self.", "bundle.").replace("*self", "*bundle")
                          .replace("CandidateBuilder::seal::<H>", "released_seal::<H>", 1)
                          # rustfmt wraps this receiver after the name substitution.
                          .replace("    bundle.patch\n", "    bundle\n        .patch\n", 1))
    for name, expected in (("released_seal", reference_seal), ("released_validate", reference_validate)):
        start = f"fn {name}<"
        if test_source.count(start) != 1:
            raise RuntimeError(f"released receipt oracle function changed: {name}")
        begin = test_source.index(start)
        finish = test_source.index("\n}", begin) + len("\n}")
        if test_source[begin:finish] != expected:
            raise RuntimeError(f"released receipt oracle function changed: {name}")


def read_qualification() -> tuple[dict, str]:
    data = (ROOT / QUALIFICATION).read_bytes()
    record = json.loads(data)
    if (not isinstance(record, dict) or set(record) != {"format", "source", "oracle", "tests"} or
            record["format"] != "zeno-fcis/receipt-refactor/1"):
        raise RuntimeError("unknown receipt qualification")
    for name, path in (("source", RECEIPT), ("oracle", ORACLE), ("tests", TEST_SOURCE)):
        item = record[name]
        if (not isinstance(item, dict) or set(item) != {"path", "bytes", "sha256"} or
                item["path"] != path or type(item["bytes"]) is not int or item["bytes"] <= 0 or
                not isinstance(item["sha256"], str) or len(item["sha256"]) != 64 or
                any(char not in "0123456789abcdef" for char in item["sha256"])):
            raise RuntimeError(f"invalid receipt qualification input: {name}")
    return record, hashlib.sha256(data).hexdigest()


def check(package_roots: dict[str, Path] | None = None) -> dict:
    original = (ROOT / BASELINE).read_bytes()
    if hashlib.sha256(original).hexdigest() != BASELINE_SHA256:
        raise RuntimeError("historical V1 compatibility baseline changed")
    baseline = json.loads(original)
    record, record_hash = read_qualification()
    receipt_baseline = next(item for item in baseline["files"] if item["path"] == RECEIPT)
    if any(record["oracle"][key] != receipt_baseline[key] for key in ("bytes", "sha256")):
        raise RuntimeError("receipt oracle differs from historical V1 identity")
    unchanged = []
    for item in baseline["files"]:
        if item["path"] != RECEIPT:
            checked_bytes(selected_path(item["path"], package_roots), item, item["path"])
            unchanged.append(item)
    data = {}
    for name in ("source", "oracle", "tests"):
        item = record[name]
        data[name] = checked_bytes(selected_path(item["path"], package_roots), item, item["path"])
    check_receipt_regions(data["source"], data["oracle"], data["tests"])
    return {"format": "zeno-fcis/v1-compatibility-result/1", "status": "source-checked",
            "baseline": baseline, "unchanged_files": unchanged,
            "receipt_refactor": {"qualification_sha256": record_hash,
                                 "source": record["source"], "oracle": record["oracle"],
                                 "tests": record["tests"], "behavior": "not-run"}}


if __name__ == "__main__":
    try:
        result = check()
        print(f"V1 compatibility sources: {len(result['unchanged_files'])} unchanged; "
              "receipt refactor source checked; behavior tests not run")
    except (OSError, RuntimeError, ValueError, KeyError, TypeError) as error:
        print(f"V1 compatibility: FAIL: {error}", file=sys.stderr)
        sys.exit(1)
