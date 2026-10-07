# V2 law and genesis stage

Parent: `1ed6f88f7b1098bed6c409bc9aa3c12c99d43c3c`.

This unit owns a closed predicate evaluator and deterministic mandatory law
selection. It does not authorize transitions. Catalog extraction, binding the
complete decision producer to this immutable frame, canonical identities and
mandatory authority/replay composition remain separate obligations.

## Contract and proof sketch

- Preserve all three decision classes, every retained law kind, all five
  declared decision scopes and explicit genesis applicability. Add mandatory
  DecisionConformance and InitialCondition families. Require unique nonzero
  law IDs and every separately declared required ID. There is no law-count cap.
- Model the borrowed frame explicitly: original pre-state, command, context,
  complete post-state, class/reason, canonical patch, ordered effects/outbox,
  protected access observations and frozen pre-law usage. Genesis supplies the
  actual initial-state record; it exposes no fabricated invocation.
- Interpret a closed eager DAG over exact typed atoms. Integer arithmetic is
  checked, Boolean and integer equality remain distinct, and missing fields,
  unavailable observations, malformed references and undefined operations
  refuse. Program shape is checked even for inapplicable laws.
- Induct over exact successful prefixes for metadata, predicates and law lists.
  The same private meter carries arbitrary initial eight counters through every
  law. Charge Step before each node and Read before each observation. Keep
  successful charges and denied attempts on refusal; no committing artifacts
  are constructed by this engine.
- Prove exact outcome/counter/report correspondence of every production helper
  with no executable preconditions. Keep operational source-order fingerprints
  separate from extensional refinement. Challenge both with mutations.

Metadata validation and allocation are outside logical metering. This is not a
physical CPU, memory, allocator or hash-cost claim. Values remain immutable
borrowed inputs and all scratch is local. Rust/Verus erasure, compiler, vstd,
solver and platform remain named trusted components. Solver acceptance does
not silently become a repository ADR-0003 independently checked proof object.

## Completion gate

Actual-source pinned Verus with complete translated function/signature/spec/body
inventory, zero executable requires, independent native scoped/genesis oracles,
false-success, source/class/reason/genesis/meter/order/coverage controls, and a
hash-bound result receipt. Root subsequently registers sources and identities,
checks exact producer/consumer composition, runs native crate/Miri/no-std/Clippy,
fresh template replay, full acceptance and independent exact-head review.

## Exact ports and admission

`Atom` retains Bool, full I128/U128, full Enum/Sum type and variant IDs, Bytes
and Text as distinct values. `Eq` never aliases their types. `ToI128` is an
explicit conversion: Bool becomes 0/1, Enum/Sum becomes its variant ID, and an
unsigned integer converts only within the signed range. `.zeno` numeric
projection lowering must use that conversion where its schema requires it.
Add/Sub/Mul and exact/floor/ceil Div are checked for both integer types.
Division by zero, signed MIN/-1, overflow and inexact Exact division refuse.
Not/And/Select are eager; Or and implication can be lowered compositionally.
Missing projections, invalid operand types and non-Boolean roots are undefined.

The borrowed frame includes pre/command/context, complete successor, class and
reason, complete patch and ordered effects/outbox, source-tagged operation
attempts, and the opaque usage snapshot from the actual decision stage. The
shared meter continues through laws; this frozen observation is not a supplied
final-usage verdict. Reject requires a nonzero reason and empty committing
artifacts. Accept has no reason. CommittedFailure has a nonzero reason. All
records and patches have strictly increasing field IDs. Every root has the
explicit `ValueView::Leaf(Atom)` or `ValueView::Record(fields)` form. Field
selectors work only on records; PreRoot/CommandRoot/ContextRoot/PostRoot/InitialRoot
work only on leaves. Thus scalar roots never alias a real record field zero.
Committing post-state has the pre-state's exact root form, field IDs and atom types. Delivery ordinals are
strictly increasing, with arbitrary gaps, nonzero starts and u32::MAX legal.
Patch correctness, complete domains and the exact producer/frame relation are
separate composition obligations, not established by structural admission.

Metadata checks precede frame checks and all logical charges. Unique nonzero
law IDs, every separately required ID, valid eager backward-reference graphs,
and all five mandatory families are required. StateInvariant is Committing
and genesis-required; RejectNoAuthority is Reject and non-genesis;
CommittedFailureEffects is CommittedFailure and non-genesis;
DecisionConformance is Always and non-genesis; InitialCondition is Always and
genesis-required, but never selected during a transition. The other five kinds
retain every ordinary scope and either genesis declaration. No list-length cap
is introduced. Required IDs and declarations must come from the bound catalog,
not from invocation-time caller choices. Unsupported `.zeno` predicates or
lowerings must refuse at that binding boundary; there are no project callbacks.

For each law in declaration order, record Skipped only if its declared scope
excludes the frame. For an applicable law, charge Step before every eager node;
an observation additionally charges Read before selecting its value and records
the successful or denied request. First false/undefined/budget failure stops
with its law diagnostic and every prior charge/report retained. No later law is
run. Successful evaluation implies every applicable law has been checked, as
proved by `success_checks_every_applicable`. Other six resource counters are
unchanged. Byte comparisons and metadata/allocator work are outside this logical
cost profile. No partial candidate or committing artifact is produced.

The production public alias is `finite::v2_laws`; the entry is `evaluate` and
returns only an opaque checked outcome. The composable private entry is
`evaluate_into`. `laws::success_checks_every_applicable` exports successful
execution as metadata/frame validity and universal success of applicable
predicates at their exact metered prefixes. Its ghost definitions are visible
within execution_v2 for the producer/consumer composition proof.
