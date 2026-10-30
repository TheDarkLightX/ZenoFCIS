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
from check_ordering_types256 import method_region


def include_conversions(profile):
    """Reviewed statements and defects; fingerprints still require a full proof."""
    statements = [
        ("U256::from_u128", ["u128"], "U256", "Exact zero extension: unsigned(result) = input."),
        ("U256::try_u128", ["U256"], "Result<u128,Failure>",
         "Exact value iff input < 2^128; otherwise exactly Overflow."),
        ("I256::from_i128", ["i128"], "I256", "Exact sign extension: signed(result) = input."),
        ("I256::magnitude", ["I256"], "U256", "Exact absolute signed value, including -2^255."),
        ("I256::from_magnitude", ["U256", "bool"], "Result<I256,Failure>",
         "Apply the requested sign, accepting zero with either sign; exact value iff in [-2^255,2^255), else Overflow."),
        ("I256::try_unsigned", ["I256"], "Result<U256,Failure>",
         "Exact unsigned value iff signed input is nonnegative; otherwise exactly Overflow."),
        ("I256::difference_unsigned", ["U256", "U256"], "Result<I256,Failure>",
         "Exact unsigned(a)-unsigned(b) iff the difference fits [-2^255,2^255), else Overflow; no early narrowing."),
        ("U512::from_u256", ["U256"], "U512", "Exact zero extension into 512 bits."),
        ("U512::narrow", ["U512"], "Result<U256,Failure>",
         "Exact value iff every high limb is zero, equivalently input < 2^256; otherwise exactly Overflow."),
        ("U768::from_u256", ["U256"], "U768", "Exact zero extension into 768 bits."),
        ("U768::narrow", ["U768"], "Result<U256,Failure>",
         "Exact value iff every high limb is zero, equivalently input < 2^256; otherwise exactly Overflow."),
    ]
    profile["obligations"] = [o for o in profile["obligations"] if o["stage"] != "conversions"] + [
        {"stage": "conversions", "member": member, "parameters": parameters, "result": result,
         "meaning": meaning, "refinement_display": gate.NAMESPACE + "arithmetic256::" + member}
        for member, parameters, result, meaning in statements]
    profile["supporting_obligations"] = [
        {"member": "U256::checked_sub", "parameters": ["U256", "U256"], "result": "Result<U256,Failure>",
         "meaning": "Exact unsigned difference iff nonnegative; otherwise exactly Overflow. Used before signed narrowing."},
        {"member": "I256::negative_bits", "parameters": ["U256"], "result": "U256",
         "meaning": "Two-complement bits: zero maps to zero; every nonzero U maps to 2^256-U."},
    ]
    defects = [
        ("unsigned_narrow_accepts_high_one", "U256::try_u128",
         "self.0[1] == 0", "self.0[1] <= 1"),
        ("signed_constructor_without_extension", "I256::from_i128",
         "u128::MAX", "0"),
        ("signed_magnitude_keeps_negative_bits", "I256::magnitude",
         "Self::negative_bits(self.0)", "self.0"),
        ("signed_negative_bound_excludes_minimum", "I256::from_magnitude",
         "Self::MIN.0", "Self::MAX.0"),
        ("signed_to_unsigned_wrong_failure", "I256::try_unsigned",
         "Failure::Overflow", "Failure::Domain"),
        ("unsigned_difference_wrong_negative_sign", "I256::difference_unsigned",
         "Self::from_magnitude(b.checked_sub(a)?, true)", "Self::from_magnitude(b.checked_sub(a)?, false)"),
        ("wide512_narrow_ignores_top_word", "U512::narrow",
         "self.0[2] == 0 && self.0[3] == 0", "self.0[2] == 0"),
        ("wide768_narrow_ignores_top_word", "U768::narrow",
         "self.0[2] == 0 && self.0[3] == 0 && self.0[4] == 0 && self.0[5] == 0",
         "self.0[2] == 0 && self.0[3] == 0 && self.0[4] == 0"),
    ]
    profile["controls"] = [c for c in profile["controls"] if c["stage"] != "conversions"] + [
        {"stage": "conversions", "name": name, "member": member, "old": old, "new": new,
         "error_source": str(gate.SUBJECT)} for name, member, old, new in defects]
    profile["scope"] = (
        "The eight actual associated constants and eleven public conversions with exact integer values "
        "and exact Overflow partitions, without public input preconditions. The actual negative_bits and "
        "U256::checked_sub dependencies and two-limb carry/borrow helper instantiations are refined. "
        "Other helper widths, other arithmetic and compiler/ghost-erasure correspondence are not proved.")
    if any(o["stage"] == "checked" for o in profile["obligations"]):
        include_checked(profile)


def include_checked(profile):
    statements = [
        ("U256::checked_add", ["U256", "U256"], "Result<U256,Failure>",
         "Exact unsigned sum iff it is below 2^256; otherwise exactly Overflow."),
        ("U256::checked_sub", ["U256", "U256"], "Result<U256,Failure>",
         "Exact unsigned difference iff nonnegative; otherwise exactly Overflow. Same statement as the conversion dependency."),
        ("I256::checked_add", ["I256", "I256"], "Result<I256,Failure>",
         "Exact signed sum iff in [-2^255,2^255); otherwise exactly Overflow."),
        ("I256::checked_sub", ["I256", "I256"], "Result<I256,Failure>",
         "Exact signed difference iff in [-2^255,2^255); otherwise exactly Overflow."),
        ("I256::checked_neg", ["I256"], "Result<I256,Failure>",
         "Exact signed negation except at -2^255, which returns exactly Overflow; zero maps to zero."),
    ]
    profile["obligations"] = [o for o in profile["obligations"] if o["stage"] != "checked"] + [
        {"stage": "checked", "member": member, "parameters": parameters, "result": result,
         "meaning": meaning, "refinement_display": gate.NAMESPACE + "arithmetic256::" + member}
        for member, parameters, result, meaning in statements]
    profile["supporting_obligations"] = [o for o in profile.get("supporting_obligations", [])
                                        if o["member"] != "U256::checked_sub"]
    defects = [
        ("unsigned_add_misses_zero_wrap", "U256::checked_add", "if carry {", "if carry && value[0] != 0 {"),
        ("unsigned_sub_misses_max_wrap", "U256::checked_sub", "if borrow {",
         "if borrow && value[0] != u128::MAX {"),
        ("signed_add_wrong_sign_partition", "I256::checked_add",
         "self.is_negative() == other.is_negative()", "self.is_negative() != other.is_negative()"),
        ("signed_sub_wrong_sign_partition", "I256::checked_sub",
         "self.is_negative() != other.is_negative()", "self.is_negative() == other.is_negative()"),
        ("signed_negation_refuses_zero", "I256::checked_neg",
         "self.0.0 == Self::MIN.0.0", "self.0.0 == Self::ZERO.0.0"),
    ]
    profile["controls"] = [c for c in profile["controls"] if c["stage"] != "checked"] + [
        {"stage": "checked", "name": name, "member": member, "old": old, "new": new,
         "error_source": str(gate.SUBJECT)} for name, member, old, new in defects]
    profile["scope"] = (
        "The eight actual associated constants, eleven public conversions and five checked 256-bit "
        "add/subtract/negation methods with exact integer values and exact Overflow partitions, without "
        "public input preconditions. The actual negative_bits dependency and two-limb carry/borrow helper "
        "instantiations are refined. U256 has no checked_neg method. Other helper widths, the multiply/divide "
        "family, other arithmetic and compiler/ghost-erasure correspondence are not proved.")
    if any(o["stage"] == "division" for o in profile["obligations"]):
        include_division(profile)


def include_division(profile):
    """Reviewed exact division statements; helper defects must fail inside the actual divider."""
    statements = [
        ("U256::div_rem", ["U256", "U256"], "Result<(U256,U256),Failure>",
         "For D > 0, exactly the canonical witness N = Q*D + R with Q >= 0 and 0 <= R < D; "
         "for D = 0, exactly ZeroDivisor."),
        ("U256::checked_div", ["U256", "U256", "Rounding"], "Result<U256,Failure>",
         "ZeroDivisor iff D = 0; otherwise Inexact iff Exact and N mod D > 0; otherwise floor(N/D), "
         "or ceil(N/D) for Ceil, with the exact Overflow partition of that value."),
    ]
    profile["obligations"] = [o for o in profile["obligations"] if o["stage"] != "division"] + [
        {"stage": "division", "member": member, "parameters": parameters, "result": result,
         "meaning": meaning, "refinement_display": gate.NAMESPACE + "arithmetic256::" + member}
        for member, parameters, result, meaning in statements]
    defects = [
        ("division_shift_drops_incoming_bit", "U256::div_rem", "divide",
         "r[i] = (r[i] << 1) | carry;", "r[i] = r[i] << 1;"),
        ("division_sets_wrong_quotient_bit", "U256::div_rem", "divide",
         "q[bit / 128] |= 1u128 << (bit % 128);", "q[bit / 128] |= 1u128 << ((bit + 1) % 128);"),
        ("division_ignores_shifted_out_bit", "U256::div_rem", "divide",
         "if carry != 0 || !matches!(compare(&r, &d), Ordering::Less) {",
         "if !matches!(compare(&r, &d), Ordering::Less) {"),
        ("unsigned_div_rem_swaps_results", "U256::div_rem", None,
         "Ok((Self(q), Self(r)))", "Ok((Self(r), Self(q)))"),
        ("unsigned_checked_div_inverts_remainder", "U256::checked_div", None,
         "r.0 != Self::ZERO.0", "r.0 == Self::ZERO.0"),
        ("unsigned_checked_div_always_ceil", "U256::checked_div", None,
         "Self::rounded(q, r.0 != Self::ZERO.0, mode)", "Self::rounded(q, r.0 != Self::ZERO.0, Rounding::Ceil)"),
    ]
    profile["controls"] = [c for c in profile["controls"] if c["stage"] != "division"] + [
        {"stage": "division", "name": name, "member": member, **({"helper": helper} if helper else {}),
         "old": old, "new": new, "error_source": str(gate.SUBJECT)}
        for name, member, helper, old, new in defects]
    profile["scope"] = (
        "The eight actual associated constants, eleven public conversions and five checked 256-bit "
        "add/subtract/negation methods with exact integer values and exact Overflow partitions, and "
        "U256::div_rem and U256::checked_div with exact canonical quotient, rounding and refusal "
        "contracts, without public input preconditions. The actual negative_bits dependency, the "
        "add/subtract carry and borrow helpers and the private divider are refined at every limb width. "
        "U256 has no checked_neg method. The multiply family, the wider division wrappers, other "
        "arithmetic and compiler/ghost-erasure correspondence are not proved.")
    if any(o["stage"] == "multiplication" for o in profile["obligations"]):
        include_multiplication(profile)


def include_multiplication(profile):
    """Reviewed exact product statements; helper defects must fail inside the actual helpers."""
    statements = [
        ("U256::full_product", ["U256", "U256"], "Result<U512,Failure>",
         "Always succeeds with the exact 512-bit product: W(result) = U(a)*U(b)."),
        ("U256::checked_mul", ["U256", "U256"], "Result<U256,Failure>",
         "Exact product iff it is below 2^256; otherwise exactly Overflow."),
    ]
    profile["obligations"] = [o for o in profile["obligations"] if o["stage"] != "multiplication"] + [
        {"stage": "multiplication", "member": member, "parameters": parameters, "result": result,
         "meaning": meaning, "refinement_display": gate.NAMESPACE + "arithmetic256::" + member}
        for member, parameters, result, meaning in statements]
    defects = [
        ("word_insertion_drops_carry", "U256::full_product", "add_word",
         "carry = overflow as u128;", "carry = 0;"),
        ("product_adds_high_word_at_low_position", "U256::full_product", "multiply",
         "add_word(&mut out, at, product[0])?;", "add_word(&mut out, at, product[1])?;"),
        ("product_uses_wrong_limb_position", "U256::full_product", "multiply",
         "let at = i.checked_add(j).ok_or(Failure::Invariant)?;",
         "let at = j.checked_add(j).ok_or(Failure::Invariant)?;"),
        ("unsigned_full_product_squares_left", "U256::full_product", None,
         "multiply(self.0, other.0)", "multiply(self.0, self.0)"),
        ("unsigned_checked_mul_squares_right", "U256::checked_mul", None,
         "self.full_product(other)?.narrow()", "other.full_product(other)?.narrow()"),
    ]
    profile["controls"] = [c for c in profile["controls"] if c["stage"] != "multiplication"] + [
        {"stage": "multiplication", "name": name, "member": member, **({"helper": helper} if helper else {}),
         "old": old, "new": new, "error_source": str(gate.SUBJECT)}
        for name, member, helper, old, new in defects]
    profile["scope"] = (
        "The eight actual associated constants, eleven public conversions and five checked 256-bit "
        "add/subtract/negation methods with exact integer values and exact Overflow partitions; "
        "U256::div_rem and U256::checked_div with exact canonical quotient, rounding and refusal "
        "contracts; and U256::full_product and U256::checked_mul with exact product contracts, all "
        "without public input preconditions. The actual negative_bits dependency, the add/subtract "
        "carry and borrow helpers, the private divider, word insertion and schoolbook multiplier are "
        "refined at every limb width (the multiplier whenever its output has room for the product). "
        "U256 has no checked_neg method. The wider wrappers, the mul_div, root and signed "
        "multiply/divide families, other arithmetic and compiler/ghost-erasure correspondence are "
        "not proved.")


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
    for item in ("impl U256 {", "impl I256 {", "impl U512 {", "impl U768 {"):
        entries.append({"text": "#[cfg_attr(verus_keep_ghost, verifier::external)]",
                        "following": item,
                        "reason": "Unproved arithmetic outside this selected closure; excluded items supply no callable specification."})
    gate.verifier.check_native_configuration(gate.read(root, gate.SUBJECT), gate.SUBJECT, entries)
    return entries


def record(root, out, cache, refresh_ordering, previous_source=None, conversions=False, checked=False,
           division=False, multiplication=False):
    profile = json.loads(gate.read(root, gate.PROFILE))
    if conversions or checked:
        include_conversions(profile)
    if checked:
        include_checked(profile)
    if division:
        include_division(profile)
    if multiplication:
        include_multiplication(profile)
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
    for obligation in profile["obligations"] + profile.get("supporting_obligations", []):
        member = obligation["member"].rsplit("::", 1)[-1]
        types = ([gate.expected_type(t) for t in obligation["parameters"]],
                 gate.expected_type(obligation["result"]))
        candidates = [n[1][2] for n in nodes if n and n[0] == "Function"
                      and n[1][2] in bodies and n[1][2].endswith("::" + member)
                      and gate.parameter_types(n) == types]
        gate.require(len(candidates) == 1, f"ambiguous actual refinement: {obligation['member']}")
        obligation["refinement"] = candidates[0]
        obligation["report_function"] = gate.report_function(obligation)
        gate.require(obligation["report_function"] in profile["report_functions"],
                     "actual refinement is absent from the complete verifier report")
    for control in profile["controls"]:
        # Exact unique mutation expression and enclosing actual declaration contract.
        obligation = next(o for o in profile["obligations"] if o["member"] == control["member"])
        control["error_function"] = obligation["report_function"]
        source = gate.read(root, Path(control["error_source"]))
        if "helper" in control:
            control["error_function"] = gate.NAMESPACE + "arithmetic256::" + control["helper"]
            control["error_region"] = list(gate.helper_region(source, control["helper"]))
        elif control["stage"] != "constants":
            control["error_region"] = list(method_region(source, *control["member"].split("::")))
        else:
            at = source.index(control["old"])
            before = source.rfind("#[cfg_attr(verus_keep_ghost, verus_spec(", 0, at)
            gate.require(before >= 0, "control lacks actual declaration contract")
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
    parser.add_argument("--include-conversions", action="store_true",
                        help="Add the reviewed complete conversion stage and its semantic controls")
    parser.add_argument("--include-checked", action="store_true",
                        help="Include conversions and the five checked add/subtract/negation methods")
    parser.add_argument("--include-division", action="store_true",
                        help="Add the reviewed U256 div_rem/checked_div stage and its semantic controls")
    parser.add_argument("--include-multiplication", action="store_true",
                        help="Add the reviewed U256 full_product/checked_mul stage and its semantic controls")
    parser.add_argument("--previous-source", type=Path,
                        help="Exact previous profile sources, required only for a compiler-ID migration review")
    args = parser.parse_args()
    record(gate.ROOT, args.out.absolute(), args.cache, args.refresh_ordering_bindings,
           args.previous_source, args.include_conversions, args.include_checked, args.include_division,
           args.include_multiplication)


if __name__ == "__main__":
    main()
