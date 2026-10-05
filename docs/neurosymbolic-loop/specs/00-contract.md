# 00 — Frozen request and observations

Parent: [design](../DESIGN.md). Status: proposed. First milestone: total Boolean.

## Inputs and outputs

Input: original canonical program bytes, complete ordered ABI/domain descriptor,
named profile/version, qualified checker identity, objective and resource policy,
and an operator-authorized provider/disclosure policy (disabled is valid).
Output: an owned immutable admitted request plus RequestId, or a typed refusal
with attempted/consumed work. No search occurs before original admission.

## Requirements

- **NSC-001:** Admission MUST freeze the exact original bytes, complete domain,
  ordered ABI, semantics/observations/errors, codec/enumeration, checker source
  identity, cost objective and limits in one canonical request. The proposer
  MUST NOT replace any of them. Hashes MUST be computed from admitted data;
  exact bindings and byte custody MUST accompany digest-based lookup.
- **NSC-002:** `FunctionalBoolV1` MUST admit only 0–6 Bool inputs, 1–16 ordered
  Bool outputs and 1–256 eager acyclic Input/Bool/And/Not/Select nodes. Every
  node MUST pass type/reference/profile checks, including unused nodes and
  unselected arms. OR MUST be lowered before admission. Arithmetic MUST fail.
- **NSC-003:** D MUST be the complete ordered product of the original's domains,
  with cardinality 1–64. Zero inputs MUST enumerate one empty tuple. Empty,
  narrowed, reordered or different domains/ABI MUST fail even when outputs
  happen to agree. Duplicate output positions MUST remain represented.
- **NSC-004:** Decode MUST be bounded, reject unknown tags/versions/fields and
  trailing data, and require canonical re-encoding equality. Candidate execution
  MUST use the program decoded from the exact supplied artifact. A different
  textual/compiled artifact MUST NOT inherit that receipt.
- **NSC-005:** Observations MUST compare full ordered output tuples. Stable
  program errors and checker failures MUST be separate types. Unexpected
  execution failure in the total Boolean profile MUST prevent equivalence,
  even if both executions fail alike. Future checked-i64 MUST preserve eager
  errors, dead/unselected arithmetic and first-error order on a complete
  admitted bounded domain; it MUST NOT be enabled by Boolean qualification.
- **NSC-006:** Cost MUST count all stored instructions once, including unused
  nodes, and the actual canonical artifact byte length. Objective V1 is the
  lexicographic pair (instructions, bytes); original component guards are
  separate requirements. Solver/tree estimates MUST NOT substitute for cost.
- **NSC-007:** All request/policy/core input data MUST be transitively immutable
  for a call; scratch MUST be fresh/exclusive and checked results owned.
  No mutable aliases, caller verdict/law/meter callback or publication authority
  may enter the transform contract.
- **NSC-008:** Intent changes MUST create a reviewed new contract/evidence chain.
  Ordinary requests may use a standing operator authorization with finite
  aggregate budgets/disclosure scope. Optimization MUST NOT silently repair or
  weaken the specification, reset a budget, or assert intent adequacy.

## Precedence and scenarios

Request/binding/profile/resource-policy admission precedes candidate work.
Within artifact admission, apply bounded byte intake and canonical decoding,
then node shape/type/reference checks, then exact schema/domain admission,
then checked cardinality/work feasibility. Precharge each step; exhausted work
returns `Inconclusive`, never a fabricated semantic difference. An invalid
original returns `Refused`; there is no fallback artifact labeled checked.

Required cases: zero-input constant; both six-input extremes; repeated output
root; swapped unequal outputs; unused input preserved; dead unsupported opcode;
forward reference/cycle; changed ABI name/order; noncanonical/trailing encoding;
exact-limit and limit-plus-one sizes; empty product factor vs zero factors;
cardinality multiplication overflow; invalid original. Later i64 cases MUST
include MAX/MIN boundaries and unselected/unused overflow, before enablement.

## Source-bound acceptance

Freeze source/body/dependency manifests. Qualify decoder round-trip and full
consumption, totality of admitted operators, correspondence to the existing
scalar evaluator, and enumerator bijection/termination. Use an independent
oracle on boundary artifacts. Kill mutations that remove an admission gate,
drop a root/input, accept trailing bytes or skip a domain row. Historical
study evidence MUST be labeled reuse input, not current implementation proof.
