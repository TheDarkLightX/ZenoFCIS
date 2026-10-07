# V2 record-to-execution proof unit

Historical stage contract. S3 of the smaller V2 simplification removes this
standalone record driver and public API. The supported route uses the shared
protected decoder and composition evaluator through mandatory Authority. Six
record-driver tests and their independent oracles migrate to composition tests;
lower-level malformed graphs remain checked at the evaluator boundary. This
does not claim unchanged public refusal ordering for the removed entry point.
The original contract below is retained as migration history, not current API
instructions or final qualification. See [the current plan](V2_VERIFIED_CORE_PLAN.md)
and [API migration](V2_PROGRAM_API_MIGRATION.md).

Parent: `5f88a67b6ab470e208f2a2507dfcbc9a1d6e2e73`, the protected flat-record
unit. Its clean-head five proof gates passed after 45/45 acceptance. Astra at
xhigh accepted that exact bounded unit; draft PR #117 is pushed. Work
continues in the isolated `agent/v2-record-execution-20261001` worktree; the
parent's source and branch head stay fixed during review.

This stage connects the actual protected decoder to eager scalar execution.
It does not construct a complete decision or authorize a transition. The full
definition of V2 completion remains in [V2_VERIFIED_CORE_PLAN.md](V2_VERIFIED_CORE_PLAN.md).

## Mathematical subject and domain

An invocation contains three original raw record slices, named State, Command
and Context, with their complete closed field descriptors. The leaf language,
canonical widths, full-width IDs, closed code intervals and arbitrary variant
order are exactly those in [V2_PROTECTED_RECORD_STAGE.md](V2_PROTECTED_RECORD_STAGE.md).
All three per-source records may be empty. No new field count, dense ID,
nonzero generic ID, variant-order or nonnegative-code restriction is allowed.
The low-level execution entry remains total for malformed graphs and machine
inputs. Do not introduce `Program::try_new` shape/type admission into this entry
or change its malformed-graph evaluation behavior; later catalog admission may
impose its separately checked rules.

Each program-input position binds a (source, field ID) pair. Bindings may
permute fields across all three sources. Every declared field must appear
exactly once; unknown or duplicate pairs refuse. The binding count equals the
program input-domain count. Each bound leaf's complete scalar domain equals
that program-input domain: Bool is Bool; I128, Enum and Sum are their exact
inclusive i64 intervals. Matching sampled values or a narrower interval is
insufficient. Descriptor and binding admission are metadata work outside the
logical raw-access/instruction cost profile, with total executable contracts.

Define the result from the admitted raw bytes themselves: decode State,
Command and Context in that fixed source order, then permute the complete
decoded scalar records into binding order, then apply the previously checked
eager instruction semantics. The same meter carries all eight counters through
every stage. The final result equals that sequential mathematical transition,
including exact refusal and retained observations. Also prove the bridge:
successful typed decoding and valid complete bindings imply that the resulting
scalar tuple is admitted by the exact program input domains.

## Refusal and observation order

1. Check all three descriptors in State/Command/Context order before any raw
   access. Invalid metadata returns Schema with its source, zero charges and
   no attempts. Then check complete bindings and exact domain matching;
   invalid bindings return Binding with zero charges and no attempts in the
   public zero-usage wrapper. The private composable helper instead preserves
   arbitrary initial counters and all existing source-attempt prefixes.
2. Invoke the private record projector for State, then Command, then Context,
   passing the same private meter. Each record charges its entire slice as
   Byte before its header and one Read before each raw field ID/payload.
   Canonical empty records attempt to charge their five header bytes. An empty
   descriptor with an empty raw slice attempts zero Byte and, if that charge
   succeeds, refuses Header. A charge may still refuse for arbitrary private
   initial consumption already above its limit, even when its amount is zero.
   A refused record stops the pipeline: no later record is decoded and no Step
   is charged. Every record refusal identifies its source, including Byte and
   Header failures that append no Read attempt.
3. Keep each source's descriptor-based attempt sequence, including a denied
   request. Reports identify the source unambiguously and retain successful
   earlier records' charges/attempts when a later record refuses. They remain
   logical permission reports, not physical byte-access tracing.
4. Construct the complete scalar tuple in declared ABI order from the already
   decoded records. This metadata/scalar projection adds no raw Read or Step
   charges. Invalid defensive lookups return Binding and retain earlier usage
   and attempts; the admission/typing bridge must establish that those lookups
   succeed for valid metadata and successful record decoding.
5. Call private `evaluate_into` with the same meter, never public `execute_v2`
   or a newly created meter. Step is charged before every eager instruction
   attempt, including unused/unselected/trapping instructions. Preserve its
   exact overflow/exhaustion and scalar-failure behavior.
6. Every refusal exposes no partial scalar result and retains the complete
   meter and attempts accumulated before refusal. The public wrapper creates
   zero initial usage and returns an opaque complete outcome. Neither caller
   initial usage nor a replacement report is accepted.

The private composable helper must additionally be total for arbitrary initial
eight counters and existing source-attempt prefixes, preserving both. No
Candidate, Write, Effect, WitnessByte or Depth work is introduced here. Their
initial counters remain unchanged; this is an ingress/Read/Step profile. Later
complete-decision construction must perform its own specified charges through
that same owned meter. Metadata validation, allocation, hashing and physical
CPU/I/O cost remain outside this logical profile.

## Actual-source architecture and proof obligations

Add a private execution_v2 module that calls the already checked production
record projector and eager evaluator. Closed source identifiers, bindings,
borrowed raw-record descriptors and an opaque outcome form the public bounded
API. Keep meter custody private. A later mandatory authority route must pass
original admitted bytes and catalog-bound descriptors directly into the private
composable helper; an unproved Value re-encoder cannot stand in for that bridge.

All new application-owned executable helpers have total exact contracts and
zero executable `requires`. Derive projection/admission lemmas from the existing
closed wire specifications, instead of assuming typed projection or wrapping
it in an opaque external body. Include the complete new dependency closure in
the direct-source harnesses and reviewed translated signature/spec/body
inventories. Retain separate proof and executable-order checks: final outcomes
cannot alone rule out cached decoding or instruction work before its charge.
Adding the new cost profile and actual sources changes evaluator identities;
regenerate affected template certificates from fresh synthesis and replay.

## Definition of done

Pinned proof of exact sequential outcomes and typed ABI projection across the
complete declared domain; total helper/public custody contracts; exact function,
contract, specification and body coverage; independent native comparisons that
exhaust the four-leaf corpus's six complete-record source permutations and 24
ABI permutations (144 combinations), plus zero/32/100-field boundary records,
all leaf shapes, malformed
metadata/raw inputs/graphs and limits, shared counters and retained refusal
prefixes; actual public canonical Value conformance; meaningful proof and
coverage mutations for meter reset, source substitution, binding permutation,
domain weakening, omitted fields, skipped records, eager attempts, rollback,
cleanup and ordering; Miri/no-std/strict Clippy; fresh six-template synthesis
and independent/native replay; registered acceptance scenario and complete
ATDD immediately before commit; clean-head proof replay; Astra peer review
with model and source hashes; a draft development PR.

Still open: catalog descriptor extraction and contract admission; original
envelope/hash binding; complete successor/patch/reason/effect/outbox artifacts;
law and genesis evaluation; mandatory V2 authority and replay; compiler/erasure
and platform assumptions; remaining V2 ledger, migration and shell obligations.
