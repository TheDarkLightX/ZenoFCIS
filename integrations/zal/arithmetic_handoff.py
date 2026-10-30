"""Pure, bounded requirements discovery -> inert full-width obligation records.

The ontology is recomputed, never accepted as a supplied verdict. The mapping
policy is explicit and versioned: names beginning with Needs are requirements;
this profile understands only wide, unsigned and signed arithmetic. Absence is
unknown. Mutation is invocation-local and no output aliases caller data.
"""
from __future__ import annotations

from copy import deepcopy

from zal_notation import core
from zal_notation.ontology import classify_ontology

PROFILE = "zeno-fcis/ontology-arithmetic-handoff/1"
WIDE = "NeedsWideArithmetic"
UNSIGNED = "NeedsUnsignedArithmetic"
SIGNED = "NeedsSignedArithmetic"
KNOWN = frozenset((WIDE, UNSIGNED, SIGNED))
MAX_BINDINGS = 16
NUMBER = "crates/zeno-fcis-value/src/profile2/number256.rs"
SCHEMA = "crates/zeno-fcis-schema/src/profile2/"


def select(status, complete, consistency, present, wide, unsigned, signed,
           unsupported, target):
    """Finite decision boundary, exhaustively checked independently by the gate.

    Internal inputs are exact booleans and closed classifier states. An absent
    opposite class is not a negative assertion: 'bound' selects a conditional
    requirement under this document, not a permanently exclusive world fact.
    """
    if status != "classified":
        return status
    if not complete or consistency != "consistent":
        return "incomplete"
    if not present:
        return "missing"
    if unsupported or target not in ("U256", "I256"):
        return "unsupported"
    if unsigned and signed:
        return "inconsistent"
    if not wide or not (unsigned or signed):
        return "missing"
    expected = "U256" if unsigned else "I256"
    return "bound" if target == expected else "type-mismatch"


def _bindings(bindings):
    core.canonical(bindings)
    core.need(type(bindings) is list and 1 <= len(bindings) <= MAX_BINDINGS,
              "expected 1..16 explicit bindings")
    individuals, identifiers = set(), set()
    for row in bindings:
        core.keys(row, {"individual", "target", "type_id"}, where="arithmetic binding")
        core.name(row["individual"], "individual")
        core.name(row["target"], "target")
        core.need(type(row["type_id"]) is int and 1 <= row["type_id"] <= 0xFFFFFFFF,
                  "type_id must be a nonzero u32")
        core.need(row["individual"] not in individuals and row["type_id"] not in identifiers,
                  "duplicate individual or Schema2 type_id")
        individuals.add(row["individual"])
        identifiers.add(row["type_id"])
    return sorted(deepcopy(bindings), key=lambda row: row["individual"])


def _obligation(kind, conclusion, interfaces, *, quantifiers, status="open", witness=None):
    return {"kind": kind, "status": status, "conclusion": conclusion,
            "quantifiers": quantifiers, "interfaces": interfaces,
            "witness": witness, "evidence": [], "authority": "none"}


def obligations(target, binding, state):
    """Explicit contracts, including the unproved refinement edges."""
    unsigned = target == "U256"
    methods = (["U256::from_words_le", "U256::words_le"] if unsigned else
               ["U256::from_words_le", "I256::from_bits", "I256::bits", "I256::is_negative"])
    interface = lambda path, symbols: {"path": path, "symbols": symbols}
    return [
        _obligation("nominal-binding", "The supplied target equals the discovered nominal type.",
                    [], quantifiers="This supplied binding only.",
                    status="satisfied" if state == "bound" else "refuted",
                    witness={"required": target, "supplied": binding["target"]}),
        _obligation("representation", "Constructors and views preserve all 256 bits" +
                    ("." if unsigned else "; is_negative agrees with the two's-complement integer sign."),
                    [interface(NUMBER, methods)],
                    quantifiers="For every lo, hi in [0, 2^128), without preconditions."),
        _obligation("numeric-order", "cmp(a,b) equals comparison of the mathematical interpretations.",
                    [interface(NUMBER, [target + "::cmp"])],
                    quantifiers="For all representable a and b of the selected nominal type."),
        _obligation("schema-C0", "Successful Schema::try_new implies normalized, closed, acyclic definitions under the supplied caps.",
                    [interface(SCHEMA + "model.rs", ["Schema::try_new", "TypeKind::" + target])],
                    quantifiers="All supplied Schema2 definitions and limits; implementation refinement remains open."),
        _obligation("schema-S1", "Successful admission implies the exact nominal kind and inclusive numeric range; the owned witness retains the exact value and report.",
                    [interface(SCHEMA + "validate.rs", ["Schema::validate_value", "Schema::validate_root"]),
                     interface(SCHEMA + "envelope.rs", ["SchemaAdmittedEnvelope::try_new"])],
                    quantifiers="All recursive Value2 inputs and explicit validation limits; finite scalar tests are insufficient."),
        _obligation("schema-S2", "Expected-envelope admission consumes complete canonical codec2 bytes and preserves selected type, schema identity, admission and refusal precedence.",
                    [interface(SCHEMA + "envelope.rs", ["SchemaAdmittedEnvelope::decode_expected", "SchemaAdmittedEnvelope::try_from_envelope"])],
                    quantifiers="All byte strings, expected schemas and decoding/validation limits."),
        _obligation("schema-S3", "The explicit profile2 route is disjoint from legacy/default admission and retains resource boundaries.",
                    [interface(SCHEMA + "mod.rs", ["SchemaProfile::VALUE2"]),
                     interface("crates/zeno-fcis-value/src/profile2/value.rs", ["Value::try_into_legacy"])],
                    quantifiers="All legacy and profile2 inputs; no implicit narrowing or signedness coercion."),
        _obligation("native-corpus", "Actual constructors, comparison, nominal Schema2 admission and envelope identity agree with the independent retained integer vectors.",
                    [interface("crates/zeno-fcis-schema/tests/ontology_arithmetic_handoff.rs", ["independent_vectors", "requirements_select_actual_schema_types"])],
                    quantifiers="Only the deterministic corpus and explicit negative controls, not the full 256-bit domain."),
        _obligation("operation-contracts", "Select each required arithmetic operation and specify overflow, refusal, rounding and resource contracts before proving its implementation.",
                    [interface("crates/zeno-fcis-value/src/profile2/arithmetic256.rs", [])],
                    quantifiers="Unspecified: these ontology examples do not select addition, multiplication, division or a financial transition."),
    ]


def build(ontology, bindings, *, max_facts=1024, max_work=1_000_000):
    """Produce conditional requirements; arithmetic obligations start open.

    No IO, evidence import, command execution, schema construction, behavioral
    acceptance or publication occurs here. The outer adapter binds actual files.
    """
    selected = _bindings(bindings)
    run = classify_ontology(ontology, max_facts=max_facts, max_work=max_work)
    requirements = []
    for binding in selected:
        individual = binding["individual"]
        classes = set(run["types"].get(individual, ()))
        unknown = sorted(c for c in classes if c.startswith("Needs") and c not in KNOWN)
        state = select(run["status"], run["complete"], run["consistency"],
                       individual in run["types"], WIDE in classes, UNSIGNED in classes,
                       SIGNED in classes, bool(unknown), binding["target"])
        row = {"binding": binding, "status": state, "unsupported_requirements": unknown,
               "type_requirement": None, "obligations": [], "authority": "none"}
        if state in ("bound", "type-mismatch"):
            target = "U256" if UNSIGNED in classes else "I256"
            relevant = {WIDE, UNSIGNED if target == "U256" else SIGNED}
            roots = sorted(f["id"] for f in run["facts"] if f["kind"] == "type"
                           and f["individual"] == individual and f["class"] in relevant)
            row["type_requirement"] = {
                "nominal_type": target, "bits": 256, "signed": target == "I256",
                "minimum": "0" if target == "U256" else str(-(1 << 255)),
                "maximum": str((1 << (256 if target == "U256" else 255)) - 1),
                "interpretation": ("lo + 2^128 * hi" if target == "U256" else
                                   "u = lo + 2^128 * hi; value = u if u < 2^255 else u - 2^256"),
                "classes": sorted(relevant), "fact_ids": roots,
                "rust_type": "zeno_fcis_value::profile2::" + target,
                "value_constructor": "Value::unsigned256" if target == "U256" else "Value::signed256",
                "schema_kind": "TypeKind::" + target,
                "schema_profile": "SchemaProfile::VALUE2", "schema_type_id": binding["type_id"],
                "features": ["value-profile-2", "schema-profile-2"],
            }
            row["obligations"] = obligations(target, binding, state)
        requirements.append(row)
    result = {"schema": PROFILE, "status": "ready" if all(r["status"] == "bound" for r in requirements) else "blocked",
              "ontology_digest": run["ontology_digest"], "classification": run,
              "requirements": requirements, "authority": "none",
              "assumptions": ["The supplied ontology expresses the intended requirements.",
                              "The Needs-prefixed requirement vocabulary is interpreted only by this versioned mapping policy.",
                              "A complete consistent closure establishes conditional entailment in the admitted profile, not completeness of human requirements."],
              "nonclaims": ["Arithmetic correctness from inference", "Execution or publication authorization",
                            "Schema2 implementation refinement", "Complete DeFi application correctness"],
              "execution_policy": "replay_argv_is_untrusted_data_never_execute_from_validation"}
    core.canonical(result)
    return result
