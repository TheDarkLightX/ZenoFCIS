#!/usr/bin/env python3
"""Produce numeric proof fingerprints from a fresh complete positive run.

This is a development producer, not qualification: review the generated diff,
then run check_numeric256.py, including all semantic controls. The optional
ordering refresh changes only the arithmetic source binding and its reviewed
annotation exceptions; it preserves every ordering obligation and fingerprint.
"""
from __future__ import annotations

import argparse
import copy
import json
from pathlib import Path
import shutil

import check_numeric256 as gate
from verus_coverage import fingerprint, inventory, parse_vir
import verus_ids


def arithmetic_exceptions(root):
    entries = [
        {"text": "#[cfg(test)]",
         "following": '#[allow(dead_code)] #[path = "reference/legacy_scalar.rs"] mod legacy_scalar;',
         "reason": "Legacy test reference is excluded from both production and proof builds."},
        {"text": "#[cfg(test)]", "following": '#[path = "tests.rs"] mod tests;',
         "reason": "Arithmetic tests are checked separately in native suites."},
    ]
    for kind in ("Failure", "Rounding"):
        entries.append({"text": "#[cfg_attr(verus_keep_ghost, verus_verify)]",
                        "following": f"#[derive(Clone, Copy, Debug, Eq, PartialEq)] pub enum {kind}",
                        "reason": "Verifies the actual closed arithmetic result or rounding datatype and derives."})
    for kind in ("U512", "U768"):
        entries.append({"text": "#[cfg_attr(verus_keep_ghost, verus_verify)]",
                        "following": f"#[derive(Clone, Copy, Debug, Eq, PartialEq)] pub struct {kind}",
                        "reason": "Verifies the actual owned fixed-width datatype and its derived clone."})
    for kind in ("U256", "I256", "U512", "U768"):
        entries.extend([
            {"text": "#[cfg_attr(verus_keep_ghost, cfg_eval)]",
             "following": f"#[cfg_attr(verus_keep_ghost, verus_verify)] impl {kind} {{",
             "reason": "Expands only erased annotations before the pinned impl frontend."},
            {"text": "#[cfg_attr(verus_keep_ghost, verus_verify)]", "following": f"impl {kind} {{",
             "reason": "Verifies only the selected actual numeric constants and methods in this impl block."},
        ])
    for item in ("fn add<const N: usize>", "fn subtract<const N: usize>",
                 "fn add_word<const N: usize>", "fn multiply<const A: usize, const B: usize, const O: usize>",
                 "fn divide<const N: usize>", "impl U256 {", "impl I256 {", "impl U512 {", "impl U768 {"):
        entries.append({"text": "#[cfg_attr(verus_keep_ghost, verifier::external)]",
                        "following": item,
                        "reason": "Unproved arithmetic outside this selected closure; excluded items supply no callable specification."})
    gate.verifier.check_native_configuration(gate.read(root, gate.SUBJECT), gate.SUBJECT, entries)
    return entries


def record(root, out, cache, refresh_ordering, previous_source=None):
    profile = json.loads(gate.read(root, gate.PROFILE))
    previous = json.loads(gate.read(root, Path("verification/verus/ordering_types256_profile.json")))
    exceptions = {key: entries for key, entries in previous["native_configuration_exceptions"].items()
                  if key != str(gate.SUBJECT) and key != str(Path("verification/verus/ordering256_types_harness.rs"))}
    exceptions[str(gate.SUBJECT)] = arithmetic_exceptions(root)
    exceptions[str(gate.HARNESS)] = [{
        "text": "#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene, cfg_eval))]",
        "following": "extern crate alloc;", "reason": "Pinned annotation frontend features; no native branch."}]
    profile["native_configuration_exceptions"] = exceptions
    profile["source_hashes"] = {str(p): gate.verifier.digest(root / p) for p in gate.UNIT}
    gate.configuration(root, profile)
    pin = json.loads(gate.read(root, gate.verifier.PIN))
    tools = gate.verifier.prepare_tools(cache, pin, False)
    proc, report = gate.prove(tools, pin, out, root)
    gate.require(proc.returncode == 0 and gate.verifier.proof_succeeded(report, pin),
                 "profile producer requires a complete positive proof; inspect its logs")
    vir = (out / "logs/crate.vir").read_text()
    nodes = parse_vir(vir)
    bodies = sorted(n[1][2] for n in nodes if n and n[0] == "Function"
                    and n[1][2].startswith(gate.NAMESPACE)
                    and dict(zip(n[2::2], n[3::2]))[":mode"] in ("Exec", "Proof"))
    profile["body_covered_functions"] = bodies
    profile["functions"] = inventory(vir, gate.NAMESPACE, tuple(bodies))
    profile["report_functions"] = sorted(n for n in report["func-details"] if n.startswith(gate.NAMESPACE))
    profile["expected_verified"] = report["verification-results"]["verified"]
    profile["smt_sections"], queries = gate.solver_sections(out / "logs", root)
    gate.require(queries == profile["expected_verified"], "incomplete producer solver logs")
    for obligation in profile["obligations"]:
        member = obligation["member"].rsplit("::", 1)[-1]
        types = ([gate.expected_type(t) for t in obligation["parameters"]],
                 gate.expected_type(obligation["result"]))
        candidates = [n[1][2] for n in nodes if n and n[0] == "Function"
                      and n[1][2] in bodies and n[1][2].endswith("::" + member)
                      and gate.parameter_types(n) == types]
        gate.require(len(candidates) == 1, f"ambiguous actual refinement: {obligation['member']}")
        obligation["refinement"] = candidates[0]
    for control in profile["controls"]:
        # Exact unique mutation expression and enclosing constant contract.
        obligation = next(o for o in profile["obligations"] if o["member"] == control["member"])
        control["error_function"] = obligation["refinement"]
        source = gate.read(root, Path(control["error_source"]))
        at = source.index(control["old"])
        before = source.rfind("#[cfg_attr(verus_keep_ghost, verus_spec(", 0, at)
        gate.require(before >= 0, "control lacks actual constant contract")
        control["error_region"] = [source.count("\n", 0, before) + 1, source.count("\n", 0, at) + 2]
    gate.check_inventory(vir, profile)
    gate.smt_coverage(out / "logs", root, profile)
    gate.control_sources(root, profile)
    gate.verifier.verify_tool_files(tools, pin)
    gate.require(profile["source_hashes"] == {str(p): gate.verifier.digest(root / p) for p in gate.UNIT},
                 "sources changed during profile production")
    (root / gate.PROFILE).write_text(json.dumps(profile, indent=2) + "\n")
    if refresh_ordering:
        # Retain every previous type/order contract and body verbatim in VIR;
        # append only the new checked representation lemma.
        ordering_path = Path("verification/verus/ordering_types256_profile.json")
        ordering = json.loads(gate.read(root, ordering_path))
        ordering_proc, ordering_report = gate.prove(tools, pin, out.parent / (out.name + "-ordering"), root,
            Path("verification/verus/ordering256_types_harness.rs"))
        gate.require(ordering_proc.returncode == 0 and gate.verifier.proof_succeeded(ordering_report, pin),
                     "ordering profile refresh requires a whole positive proof")
        added = ordering["namespace"] + "number256::numeric_representation"
        ordering_bodies = sorted(set(ordering["body_covered_functions"]) | {added})
        ordering_vir = (out.parent / (out.name + "-ordering") / "logs/crate.vir").read_text()
        fresh = inventory(ordering_vir, ordering["namespace"], tuple(ordering_bodies))
        gate.require(set(fresh) == set(ordering["functions"]) | {added}, "ordering inventory changed")
        if not all(fresh[n] == r for n, r in ordering["functions"].items()):
            gate.require(previous_source is not None,
                         "ordering fingerprints changed: provide exact previous sources for a compiler-ID review")
            baseline = out.parent / (out.name + "-previous-source")
            for source, digest in ordering["source_hashes"].items():
                gate.require(gate.verifier.digest(previous_source / source) == digest,
                             "previous ordering source does not match the reviewed profile")
                target = baseline / source
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(previous_source / source, target)
            baseline_proc, baseline_report = gate.prove(tools, pin,
                out.parent / (out.name + "-previous-proof"), baseline,
                Path("verification/verus/ordering256_types_harness.rs"))
            gate.require(baseline_proc.returncode == 0 and gate.verifier.proof_succeeded(baseline_report, pin),
                         "previous ordering proof did not pass")
            baseline_vir = (out.parent / (out.name + "-previous-proof") / "logs/crate.vir").read_text()
            gate.require(inventory(baseline_vir, ordering["namespace"],
                         tuple(ordering["body_covered_functions"])) == ordering["functions"],
                         "previous ordering profile did not replay exactly")
            baseline_nodes = verus_ids.functions(parse_vir(baseline_vir), ordering["namespace"])
            reference = {"schema": verus_ids.SCHEMA, "functions": {
                name: {"sha256": fingerprint(node),
                       "ids": [[family, value] for family, _, value in verus_ids.occurrences(node)]}
                for name, node in baseline_nodes.items()}}
            retained = [copy.deepcopy(n) for n in parse_vir(ordering_vir)
                        if n and n[0] == "Function" and n[1][2] in baseline_nodes]
            restored = verus_ids.restore(retained, ordering["namespace"], reference)
            gate.require(inventory("", ordering["namespace"], tuple(ordering["body_covered_functions"]),
                         _nodes=restored) == ordering["functions"],
                         "existing ordering semantics changed, beyond compiler IDs")
            review = {"schema": "zeno-fcis/ordering-numeric-extension-review/1",
                      "previous_source_bindings": ordering["source_hashes"],
                      "retained_functions": sorted(baseline_nodes), "added_function": added,
                      "retained_contracts_and_bodies_equal_under_checked_id_bijection": True}
            (out / "ordering-extension-review.json").write_text(json.dumps(review, indent=2) + "\n")
        ordering["body_covered_functions"] = ordering_bodies
        ordering["functions"] = fresh
        ordering["expected_verified"] = ordering_report["verification-results"]["verified"]
        ordering["smt_sections"], ordering_queries = gate.solver_sections(
            out.parent / (out.name + "-ordering") / "logs", root)
        gate.require(ordering_queries == ordering["expected_verified"], "incomplete ordering solver logs")
        (root / ordering_path).write_text(json.dumps(ordering, indent=2) + "\n")
        for path in (Path("verification/verus/ordering256_profile.json"),
                     Path("verification/verus/ordering_types256_profile.json")):
            old = json.loads(gate.read(root, path))
            for source in old["source_hashes"]:
                if source in profile["source_hashes"]:
                    old["source_hashes"][source] = profile["source_hashes"][source]
            old["native_configuration_exceptions"][str(gate.SUBJECT)] = exceptions[str(gate.SUBJECT)]
            (root / path).write_text(json.dumps(old, indent=2) + "\n")
    print("numeric256 profile produced; review the diff and run full qualification")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, default=gate.verifier.cache_directory())
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--refresh-ordering-bindings", action="store_true")
    parser.add_argument("--previous-source", type=Path,
                        help="Exact previous profile sources, required only for a compiler-ID migration review")
    args = parser.parse_args()
    record(gate.ROOT, args.out.absolute(), args.cache, args.refresh_ordering_bindings, args.previous_source)


if __name__ == "__main__":
    main()
