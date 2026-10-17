"""Separate reviewed test dependencies and plan evidence invalidation.

This tool neither caches proof results nor promotes an old receipt. A plan
names evidence that must be re-executed; it is not qualification. Unknown
dependencies, changed production anchors and legacy receipts invalidate all.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
POLICY = Path("verification/verus/qualification_sources.json")
SCHEMA = "zeno-fcis/qualification-source-bindings/1"
ALL_EVIDENCE = ("positive-proof", "proof-controls", "native-tests", "tool-tests")


def split(snapshot, policy, harness):
    """Exclude tests only at an exactly source-pinned, reviewed boundary.

    Changes to *any* other dependency restore conservative all-source binding.
    This prevents a later unguarded import from inheriting an old test exclusion.
    The policy itself remains a production dependency, outside its own anchors.
    """
    if not isinstance(snapshot, dict) or any(not isinstance(k, str) or not isinstance(v, str)
                                             for k, v in snapshot.items()):
        raise ValueError("malformed qualification source snapshot")
    if not isinstance(policy, dict) or not isinstance(policy.get("harnesses"), dict):
        policy = {}
    rule = policy.get("harnesses", {}).get(str(harness), {})
    if not isinstance(rule, dict):
        rule = {}
    tests = rule.get("tests", [])
    valid = (policy.get("schema") == "zeno-fcis/qualification-source-policy/1"
             and isinstance(tests, list) and all(isinstance(p, str) and p in snapshot for p in tests)
             and tests == sorted(set(tests))
             and str(POLICY) not in tests
             and isinstance(rule.get("production"), dict))
    if valid:
        production = {p: h for p, h in snapshot.items() if p not in tests and p != str(POLICY)}
        valid = production == rule["production"]
    names = set(tests) if valid else set()
    return {"schema": SCHEMA, "harness": str(harness), "separated": bool(valid),
            "controls_use_tests": rule.get("controls_use_tests") is not False,
            "production": {p: h for p, h in snapshot.items() if p not in names},
            "tests": {p: h for p, h in snapshot.items() if p in names}}


def bindings(snapshot, harness):
    path = ROOT / POLICY
    if any(p.is_symlink() for p in (path, *path.parents)):
        raise ValueError("symlink qualification dependency policy")
    raw = path.read_bytes()
    policy_hash = hashlib.sha256(raw).hexdigest()
    if str(POLICY) in snapshot and snapshot[str(POLICY)] != policy_hash:
        raise ValueError("qualification dependency policy changed after source snapshot")
    result = split(snapshot, json.loads(raw), harness)
    result["policy_sha256"] = policy_hash
    return result


def changed_evidence(previous, current):
    """A test-only delta cannot rebind proof or prove that a test still passes."""
    required = {"schema", "harness", "separated", "controls_use_tests", "production", "tests", "policy_sha256"}
    for value in (previous, current):
        if (not isinstance(value, dict) or set(value) != required or value.get("schema") != SCHEMA
                or not isinstance(value.get("production"), dict) or not isinstance(value.get("tests"), dict)
                or set(value["production"]) & set(value["tests"])):
            return ALL_EVIDENCE
    if (previous["harness"] != current["harness"]
            or previous["policy_sha256"] != current["policy_sha256"]
            or previous["controls_use_tests"] != current["controls_use_tests"]
            or previous["production"] != current["production"]):
        return ALL_EVIDENCE
    if previous["tests"] != current["tests"]:
        if previous["separated"] is not True or current["separated"] is not True:
            return ALL_EVIDENCE
        # Mixed proof/native controls are indivisible evidence here. Re-execute
        # every such control, including its existing proof leg; a native-only
        # test edit cannot inherit an old native kill from the previous receipt.
        controls = ("proof-controls",) if current["controls_use_tests"] is not False else ()
        return (*controls, "native-tests", "tool-tests")
    return ()


def run_changed(previous, current, checks):
    """Execute every invalidated check; never manufacture retained evidence.

    Callbacks must perform their ordinary independent acceptance checks. This
    returns only fresh results, not a combined qualification receipt or cache hit.
    """
    required = changed_evidence(previous, current)
    if any(name not in checks for name in required):
        raise ValueError("missing required qualification evidence check")
    return {name: checks[name]() for name in required}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("previous", type=Path, nargs="?")
    parser.add_argument("current", type=Path, nargs="?")
    parser.add_argument("--snapshot-gate", choices=(
        "check_verus", "check_authority_v2", "check_canonical_bytes", "check_catalog_v2",
        "check_checked_assembly", "check_checker", "check_composition_v2", "check_decision_v2",
        "check_envelope_frame_v2", "check_finite_execution", "check_guarded_laws_v2", "check_laws_v2",
        "check_metered_execution", "check_original_output_v2", "check_protected_input",
        "check_publication_v2", "check_schema_binding_v2"))
    args = parser.parse_args()
    if args.snapshot_gate:
        if args.previous or args.current:
            parser.error("snapshot mode does not take receipts")
        gate = importlib.import_module(args.snapshot_gate)
        snapshot = gate.snapshot(ROOT) if args.snapshot_gate == "check_catalog_v2" else gate.snapshot()
        print(json.dumps({"source_bindings": bindings(snapshot, gate.HARNESS),
                          "qualification_complete": False}, indent=2, sort_keys=True))
        return
    if args.previous is None or args.current is None:
        parser.error("provide previous/current receipts or --snapshot-gate")
    before, after = (json.loads(p.read_text()) for p in (args.previous, args.current))
    print(json.dumps({"required_evidence": changed_evidence(before.get("source_bindings"), after.get("source_bindings")),
                      "qualification_complete": False}, indent=2))


if __name__ == "__main__":
    main()
