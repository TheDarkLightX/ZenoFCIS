# ZenoFCIS 2.1: checked program transformation

Status: implementable design, 2026-10-04; no feature or dependency installed.
Prepared by gpt-6-astra at max reasoning effort. This elaborates **track 6** of
the [existing 2.1 roadmap](V2_1_FACTORY_PLAN.md), informed by the
[published-study review](V2_1_EGRAPH_REVIEW.md).

Build `transform` first: check a supplied canonical replacement against the
original over its complete declared finite domain. Then build `optimize`: a
bounded untrusted proposer whose candidates pass through that same checker.
The first supported claim concerns total Boolean scalar programs only.
It cannot activate a replacement application or create a publication capability.

The [withdrawal-queue feature evidence](research/V2_1_CHECKED_OPTIMIZATION_EVIDENCE.md)
now supplies an application-derived optimization candidate and independently
reproduced native scalar checking. The owner reports neurosymbolic e-graph
discovery; the local run reproduced its rewrite rather than rerunning that search.
Its 16-to-7-node Boolean kernel fits the first profile and is a stage 6.1
regression input. The full research results and resource counterexamples support
this design; they do not complete the product checker, receipt or proposal loop.

## Placement and reuse

Finish smaller V2, including its mandatory Authority → Publication → SQLite
route and existing legal template domains. Preserve the roadmap's supported
compound/U128 prerequisite and tracks 1–5. Track 6 implementation follows that
order; this bounded design and research review add no smaller-V2 blocker.
Application adoption later consumes track 2's checked integration and track 4's
upgrade/evidence-dependency machinery. The maintenance pilot need not wait for
an optimizer, and the optimizer need not acquire a new leadership benchmark.

The experiment demonstrates a useful search candidate, not a release threshold:
816 semantic versus 895 local instructions, 18 wins and four regressions;
protected selection totals 812. The original two configurations remain failed,
and the successful configuration is exploratory. Fresh Python replay confirms
the stored Boolean results; Rust and Lean executions were not rerun.
See the [bounded replay receipt](evidence/v2_1_egraphs/replay-20261004.json).

| Reuse as input | Rebuild or qualify for 2.1 |
| --- | --- |
| Fixed 100-case corpus, original/repeat receipts and both failures | A new exact-source acceptance suite; historical receipts cannot qualify changed code |
| Total-Boolean operators, 33 ordinary identities, exact signatures | Profile admission and correspondence to the final V2 scalar semantics |
| Local simplifier and shared-output lowering strategy | Bounded proposer decoding, canonical artifacts and actual emitted cost |
| Python expression/DAG interpreters and eager-error examples | Independent test oracle, stable error codec, strict receipt validation and replay |
| Lean completeness and guarded-selection argument | Executable enumeration, decoder, observation and acceptance-loop refinement |
| egg/ILP search implementation | Optional separate executable, exact dependencies, enforced resource limits and license review |

Keep one scalar IR. In the inspected V2 working tree,
`crates/zeno-fcis-synthesis/src/finite/ir.rs:54–119` already has an owned,
validated `Program`, immutable accessors, `evaluate` and `execute_v2`;
`ir.rs:148–160` constructs versioned program data. The metered adapter explicitly
grants no transition authority and has its own proof-boundary qualification.
Use those representations and qualified evaluator bodies, rather than copying
the historical interpreter. These are orientation references to a dirty
snapshot, not claims about the final V2 revision; repin them before coding.

Place pure admission/checking in `zeno-fcis-synthesis::finite::transform`,
following the repository's proof/source-coverage conventions. Put search,
process supervision and files in a CLI-side module or small proposer executable
that depends on synthesis. No egg, ILP, native solver, wall-clock read or
subprocess dependency enters the authoritative functional core. Keep the
existing authority API unchanged.

## Contract before implementation

For admitted canonical artifacts P and Q and a fixed profile S:

```text
D = the complete product of P's declared ordered input domains
same_schema(P, Q) = exact input domains, output domains and ordered ABI identity
O_S(P, x) = complete successful output tuple or the specified stable error

Equivalent(P, Q, S) implies:
  same_schema(P, Q)
  D is nonempty and exhaustible within the admitted limits
  for every x in D: O_S(P, x) = O_S(Q, x)
```

The candidate cannot supply a smaller domain, observation function, interpreter
or acceptance callback. ABI bindings are fixed qualification inputs; unused
inputs and duplicate output positions remain in that ABI. Where external field
names/IDs supplement positional schemas, include their canonical ABI descriptor
and compare it exactly. Invalid artifact admission, unsupported profiles,
incomplete work and semantic mismatch are distinct outcomes.

`FunctionalBoolV1` accepts 0–6 Boolean inputs, 1–16 Boolean outputs, 1–256 eager
acyclic nodes, and only Input/Bool/And/Not/Select. It scans all nodes, including
unused nodes and unselected arms. Zero inputs mean one empty tuple; an empty
domain is invalid. OR is a proposer convenience lowered before acceptance.
Schema/type/reference checks precede execution. Stable errors are tagged data,
not formatted `Debug` strings. For admitted total Boolean programs, an
unexpected evaluator failure prevents a receipt even if both sides fail alike.

Define a canonical inverse of the existing scalar program representation, with
bounded decode, exact field/tag validation, complete input consumption and
`encode(decode(bytes)) == bytes`. Admit the *actual candidate bytes*, construct
the program from them, and execute that program. The accepted artifact is the
canonical interpreted DAG. A receipt for it says nothing about a separately
printed or compiled Rust/Wasm artifact; any such target needs its own
exact-artifact correspondence lane before promotion.

Use one documented enumeration order. The experiment maps `vi` to row bit `i`;
the inspected `Space` iterator advances the last ABI field fastest
(`finite/mod.rs:384–467`). Reuse and qualify the latter product enumerator;
give it an explicit version and derive proposer signatures using that same
order. Do not silently reinterpret historical truth vectors. Compute widths
in `i128`/`u128`, multiply with checked bounds before allocation, and reject a
cardinality above the profile limit. Complete coverage needs a bijective
ordinal-to-tuple argument plus terminal/count checks; equal counts alone do
not rule out duplicated rows and missed inputs.

## Small API and custody

Illustrative Rust surface; names require interface review before CLI/MCP use:

```rust
pub enum FunctionalProfile { BoolV1 } // Add CheckedI64V1 only after qualification.

pub struct CheckPolicy { /* private, validated profile and work limits */ }
pub struct CheckedReplacement { /* owned canonical candidate + bound receipt */ }
pub struct CheckReport { /* outcome + library-computed work on every exit */ }

pub enum CheckOutcome {
    Equivalent(CheckedReplacement),
    Different(Counterexample),
    Refused(AdmissionReason),
    Inconclusive(CheckStop),
}

pub fn transform(original: &[u8], candidate: &[u8],
                 policy: &CheckPolicy) -> CheckReport;
pub fn replay_transform(original: &[u8], candidate: &[u8], receipt: &[u8],
                        policy: &CheckPolicy) -> CheckReport;
```

Use private fields, checked constructors and immutable getters. Canonical
artifact buffers and policy data are owned or borrowed under Rust's immutable
alias rules for the complete invocation. No interior-mutable authority data or
escaping scratch buffers. `CheckedReplacement` owns the exact candidate it
checked and cannot be deserialized directly from an untrusted receipt. It is
an equivalence result, not `Authority`, `Evaluation` or `Publication`.

The checker decodes both artifacts, fixes the schema/domain, preflights work,
then runs independent source and candidate evaluations for every ordinal.
Compare full ordered observations exactly; use a hash only for binding or
indexing, never instead of observation equality. Return the first differing
tuple with both typed observations and both artifact identities. Replaying
that witness through the same actual execution semantics must reproduce it.
Only complete exhaustion can construct `Equivalent`.

Charge checker work before parsing, allocation, node attempts, comparisons and
receipt construction under a documented checker-work profile. This is separate
from application runtime metering. Return accumulated measured work on invalid
input, mismatch, exhaustion and retry; preserve the refused charge's attempted
amount. Unexpected panic, process death or allocation failure yields an outer
inconclusive record with the last known accounting and an explicit incomplete
accounting flag. It cannot synthesize successful usage or equivalence.

The optimizer initializes its incumbent to the admitted original. A local
simplifier and egg may each propose candidates; neither may declare an accepted
baseline. Check every candidate against the original, then choose the lowest
actual cost among checked strict improvements. Preserve the incumbent and its
receipt if later searches fail. If none qualifies, return the original with
`NoCheckedImprovement` and the actual stop reason. An invalid or unsupported
original produces refusal, not a purported safe fallback. Ties keep the
original/incumbent; among equal improvements use canonical bytes as the final
deterministic tie-break. Search nondeterminism cannot change the check's meaning.

## Receipt and replay

Introduce one versioned canonical equivalence receipt with these bindings:

| Field group | Required content |
| --- | --- |
| Subject | Domain-separated original and candidate artifact digests, lengths and codec version; immutable byte custody in the checked result |
| Meaning | Exact schema/ABI and domain descriptor commitments; execution semantic profile; observation/error-codec version; enumeration version |
| Checker | Source closure and executable/build identity, relevant dependency/tool pins and declared assumptions, derived by the qualification/build path |
| Completion | Expected cardinality, visited count, final enumeration state, complete status and deterministic observation-trace commitment |
| Cost/accounting | Both emitted node counts and canonical byte lengths; objective version; checker-work limits and measured usage |
| Proposal provenance | Separately labeled optimizer/rules/egg/extractor revisions, seed and configuration, stop/solver status; no authority derived from those assertions |

The domain descriptor contains the actual complete ordered domains, not just a
caller-provided digest. Compute hashes from admitted bytes in the checker. Bind
all qualification-relevant checker dependencies; a stale manifest invalidates
dependent evidence. Proposal logs, durations and optional diagnostic fields
remain a separate report so wall-clock data does not affect semantic receipts.
Use exact version/tag decoding and refuse unknown/trailing fields.

`replay_transform` re-admits both supplied artifacts, validates every binding,
reruns the complete check and compares the deterministic receipt fields. A
stored file is evidence to replay, never a constructor for a trusted witness.
Report receipt-admission/replay overhead separately from the reproduced check's
deterministic work fields, while retaining total work on every replay exit.
Changing bytes, schema, domain, profile, ABI order, checker identity, count,
trace or measured costs must prevent receipt acceptance. Same truth function
with different canonical bytes still has a different artifact identity.

## Search and cost limits

Initial conservative profile, to be measured before adoption:

| Boundary | Enforced limit |
| --- | --- |
| Scalar artifact/check | 64 KiB per artifact; the Boolean arity/node bounds above; at most 64 valuations; checked upper bound of 1,000,000 checker work units |
| Proposal stream | At most 8 complete candidates, each at most 64 KiB; reject overlong/truncated output without unbounded buffering |
| One proposer worker | 5-second external wall deadline; 512 MiB process-group memory limit; one solver thread; bounded stdout/stderr |
| Whole optimize request | 20-second search deadline and fixed total candidate/check allowance; retain already checked incumbent; no new worker after expiry |
| Standalone checker worker | 2-second external wall deadline and 64 MiB memory cap in addition to deterministic internal work limits |

The checker and supervisor enforce the smaller of policy and compiled ceilings.
On Linux, run workers in an owned cgroup with `memory.max`, no swap allowance,
and a supervisor deadline that terminates the entire group. Test process-tree
cleanup and over-limit refusal. Record termination/reporting latency separately;
these controls do not assert hard-real-time scheduling. Unsupported hosts must
provide and qualify an equivalent enforcement mechanism before enabling the
proposer. An unavailable cap yields `OptimizerUnavailable`; pure `transform`
remains usable with its bounded in-process algorithm and caller supervision.

The 1,000-node/three-iteration search configuration can be an initial heuristic
inside those limits. It is not a hard memory limit: the study reached 1,351
nodes. CBC's solve timeout excludes model construction/reconstruction and is
also not the worker deadline. Saturation, native calls, extraction and decoding
all fall under external enforcement. No silent retry with larger resources.

Measure cost from the admitted candidate DAG, counting every stored instruction
once, including unused nodes, plus the length of its actual canonical encoding.
Initial objective: lexicographic `(instruction_count, canonical_bytes_length)`,
with **neither measure allowed to increase** relative to the original for an
`Improved` result. At least one must strictly decrease. Report both deltas and
any alternative future objective by version. Do not count a tree estimate,
surrogate LP objective or receipt size as execution-artifact cost.

Extraction quality and equivalence are independent. Research reproduction must
keep its original positive-optimal-status rule. A future product may accept a
fully decoded feasible incumbent from a timed-out/unknown-optimality search
*only after* artifact admission, complete equivalence and actual cost checks;
record the search failure/status. Infeasible, partial or undecodable output
never enters the candidate path. Neither successful extraction nor passing
equivalence claims global optimality or application runtime speedup.

## Checked-i64 and application boundaries

`CheckedI64V1` is a later bounded extension of `transform`, not a widening of
Boolean congruence. Keep the original full ordered integer intervals and
checked signed arithmetic; admit only domain products at most 65,536 tuples
and within the complete-work ceiling. The full i64 interval has `2^64` values:
reject it as too large for this profile without overflow or domain shrinking.
Ranges near either endpoint can still have small admissible cardinality.

Compare `Ok(ordered i64 tuple)` and each stable error exactly. Evaluate every
stored instruction eagerly and preserve the first observable failure under
the declared error ABI. In particular reject `(MAX + 1) - 1 → MAX`, removal
of an unused overflowing node, and removal of an unselected overflowing arm.
Arithmetic errors are valid observations in this profile; checker exhaustion
is not a program error. Do not seed arithmetic e-class merging from successful
rows alone. Start with supplied whole-program candidates; qualify any arithmetic
proposer separately after the checker and error controls exist.

A functional receipt alone never authorizes an application replacement.
`ApplicationExactV1` is a later, separately admitted profile that compares both
complete applications through the library-owned route on the same original
state/command/context bytes and bound policy. Its observation includes:

- Accept, Reject or CommittedFailure, stable reason and precedence;
- complete successor, canonical patch, ordered effects/outbox, destinations,
  payloads, channels and idempotency meaning;
- actual transition and genesis law frames/results;
- authority-owned resource usage, protected access/trace observations and every
  admission, execution, law, budget and publication refusal;
- source/contract/evaluator identities and the exact replay/publication binding.

The inspected implementation has these decision fields in
`execution_v2/decision.rs:176–199,267–334`; instruction attempts charge before
evaluation in `execution_v2/mod.rs:162–177`. Authority binding derives identity
from checked catalog/evaluator data, and replay recomputes before comparison
(`authority/bound.rs:34–81,103–117`). Follow those ownership boundaries. Do not
add a caller verdict, mutable meter, law callback or optimization bypass.

Identity deserves explicit treatment: P and Q normally have different source
identities, so their full sealed publication bytes will differ. Exact comparison
must report that difference; it cannot silently erase identity fields. A
compatible activation needs track 4's reviewed old/new identity mapping and
evidence dependencies, with independently recomputed bindings on each side.
Historical replay continues under its original artifact and policy. New
activation does not reinterpret existing records or pending effects.

Even double-negation removal changes instruction usage. Thus equal Boolean
outputs can fail the exact application profile at large budgets, and change
refusal to success at a budget boundary. Default: refuse application promotion.
A separate resource-refinement contract must specify allowed old/new usage and
outcome relations for every admitted input and supported budget/policy. It must
explicitly decide whether a former budget refusal may become success, preserve
charge-before-work and measured refusal usage, and requalify resource-observing
laws. A componentwise lower-cost inequality alone does not establish this.
No resource-refinement mode ships in the first Boolean milestone.

Complete-application finite checking requires a complete raw-domain enumeration
or a proved coverage/refinement bridge from raw inputs. A few abstract Boolean
guards cannot qualify a large state/command domain. If that bridge or complete
budget domain is impractical, leave application promotion unavailable. This
does not invalidate the scoped functional tool.

## Implementation tasks and acceptance

Each stage starts with its contract/counterexamples and ends at a reviewable
source revision. Formal proofs and their executable correspondence, native
comparisons, meaningful mutation tests and independent review follow the
existing roadmap; the study does not replace them.

| Stage | Small deliverable | Required checks |
| --- | --- | --- |
| 6.0 Contract | Final artifact/ABI/observation/error versions; caps and cost objective; no runtime integration | Handwritten distinguishers for ordered roots, zero inputs, changed schema, meter change and eager error; document claims and trusted base |
| 6.1 `transform` Boolean | Canonical scalar decoder, total-Bool admission, independent complete comparison and private result | Qualify decoder round-trip/full consumption and same-schema checks; prove enumeration coverage and successful-loop invariant; compare actual evaluator with independent Python on the retained corpus plus new boundaries |
| 6.2 Receipt/replay | Canonical receipt, source closure and evidence dependency binding | Tamper each binding/count/cost/profile, stale checker, changed byte encoding, swapped unequal outputs, repeated roots, missing/duplicated row, source mutation and insufficient final receipt budget; all must fail the intended gate |
| 6.3 `optimize` | Checked local baseline first, optional capped egg proposer, best accepted fallback | Invalid rewrite rejected; candidate cost independently recomputed; regressions retain incumbent; timeout/unknown solver, oversized output, panic and memory/deadline termination retain only checked artifacts and accounting |
| 6.4 checked-i64 | Supplied-candidate functional extension, separately enabled | Both endpoints and narrow endpoint intervals; full-width cardinality refusal; eager dead/unselected overflow; output-domain errors; first-error precedence and no partial-success observations |
| 6.5 application adoption, only if selected for release | One actual complete application and its reviewed upgrade/resource contract | Reason/successor/patch/effect-order/law/trace/usage/identity mutants; boundary budgets; original-byte custody; exact replay and pending-effect handling through existing Authority → Publication → SQLite route |

For Boolean 6.1–6.3, prove or discharge: admitted decode corresponds to exact
artifact bytes; every admitted opcode is total; the enumerator covers exactly
D; equal compared observations imply pointwise equivalence; no incomplete exit
constructs a checked result; checker work is charged before work; and returned
bytes/receipts cannot acquire mutable aliases. The Lean model supplies the
abstract acceptance lemma. Connect it to executable bodies or prove the
equivalent statement in the repository's qualified toolchain. Do not require
proof of an optimizer or global extraction optimality to establish checked
equivalence of a particular candidate.

Before releasing each enabled profile, run its exact-source/body coverage and
proof gates, native comparisons, independent mutation review, applicable ATDD
and exact-head CI. Test the packaged CLI journey on declared targets. Keep
unimplemented modes unavailable and label remaining assumptions per
[ADR 0003](adr/0003-epistemic-status.md). Application activation remains subject
to [ADR 0005](adr/0005-decision-gate.md); a successful functional milestone does
not fulfill the whole-application gates.

Before adopting egg/CBC, inspect the exact fork/patch and complete dependency
licenses/notices, verify locked source and native library identities, and
measure build and maintenance cost against the checked local-only proposer.
The published pins are documented in the review; fork/solver license
compatibility and fresh Rust/Lean reproduction remain open here. CBC may stay
optional if its cost is unjustified. That choice cannot weaken `transform`.
