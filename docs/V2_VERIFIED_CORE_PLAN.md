# V2 implementation and core verification plan

Owner requirement, 2026-10-01: build V2 while verifying the actual functional
core completely. The release remains blocked until the supported core path and
its implementation correspondence are covered. A passing model check, a tool
name, or a subset of verified functions does not close this plan.

## Baseline and definition of done

Implementation starts from `4dd979b95433bc70dcdc3ce834ff6db90eef806f`, on
`agent/v2-verified-core-20261001`. The retained 1.x checkouts are separate.
The baseline's acceptance gate passed 41/41; its Verus unit verifies two
finite-domain arithmetic functions. The inventory runtime has exhaustive
conformance evidence on 864 admitted inputs. Neither result verifies the whole
core. The required breaking changes remain in [ADR 0004](adr/0004-v2-ledger.md)
and the decision boundary in [ADR 0005](adr/0005-decision-gate.md).

V2 core completion requires all of the following:

1. A closed specification for admitted input, all decision classes, complete
   successor state, reasons, patches, effects, outbox, footprints, logical work,
   law/genesis checks, canonical identities, and replay authorization.
2. Verification of the executable functions implementing those specifications,
   including success, refusal, overflow, malformed input, and exhaustion.
3. Verified bridges between stages. A checked function's preconditions must
   follow from admission or another verified function, rather than a project
   callback or an unchecked assertion.
4. A mandatory V2 authority route that constructs the complete decision through
   library code. Project decision and law callbacks cannot substitute for it.
5. Proof coverage of every application-owned function reachable on that route,
   a named trusted base, source/build binding, and no unreviewed assumption or
   `external_body` used to obtain success.
6. Independent application examples, complete-decision comparisons, meaningful
   mutations, malformed-input and bypass negatives, and exact-head CI.
7. The remaining V2 ledger changes, migration documentation, and integration
   gates. Release approval remains separate from implementation and proof.

The mathematical correctness claim is relative to a reviewed specification and
its admitted domain. Authentication of external facts, database atomicity,
external delivery, allocator/standard-library behavior, compiler, verifier,
solver, and hardware assumptions are named separately. Effect-plan derivation
is core behavior; effect delivery is shell behavior. No template may shrink its
legal domain merely to make a proof cheaper.

## Required proof chain and implementation order

| Order | Actual core component | Required statement | Current frontier |
| --- | --- | --- | --- |
| 1 | Shared scalar IR and execution loop | Every instruction and eager prefix match the mathematical semantics; invalid input and traps refuse without exposing partial output | Admission/eager execution unit: 20 executable functions, 34 obligations; all 132 GitHub checks passed at `9882136` |
| 2 | Schema and canonical admission | Every accepted byte string decodes to exactly its typed value, respects declared ranges and limits, and consumes all bytes | Integer readers qualified at `f88f1a3`; protected flat-record development proof passes; catalog and envelope correspondence open |
| 3 | Library meter and state view | Actual accesses and steps are charged before protected work; exhaustion and overflow cannot mutate authoritative state | Owned instruction meter and one-record view implemented; exact result proofs plus separately reviewed source order; multi-record composition and mandatory route open |
| 4 | Complete decision construction | Class, reason, successor, patch, effects, outbox and order equal the bound contract | Inventory's 864-case evidence; general proof open |
| 5 | Laws and genesis | Library evaluates every applicable predicate on the exact invocation and actual initial state | Project-supplied law engine remains; mandatory library checks open |
| 6 | Authority, identity and replay | Only the bound contract/evaluator can produce authorization; stale input, changed identity, and replay mismatch refuse | 1.x nominal bindings; full V2 route open |
| 7 | Larger template domains | Every raw-input abstraction preserves rules, including account-lockout time and integer boundaries | Application-specific bridges open |
| 8 | Composition and release | Verified local contracts compose through checked interfaces; all remaining ledger/migration obligations close | Open |

The order is a dependency order. Specifying law and decision representations
may happen earlier when it determines the interpreter's required language.

## First proof unit: shared eager finite execution

The baseline instruction set has exactly ten variants: input, integer,
Boolean, checked add/subtract, equality, signed less-than, Boolean conjunction,
Boolean negation, and selection. Preserve the existing Boolean wire encoding
of exactly 0/1 and eager evaluation. An unused or unselected earlier node can
still trap; changing this to lazy evaluation changes the contract.

Proof sketch, pinned to Verus `0.2026.09.27.3cf1832`:

- Define instruction semantics over mathematical sequences and integers.
  Successful add/sub results are exact within inclusive `i64` bounds; every
  out-of-range result refuses. Invalid references also refuse.
- Prove exact domain admission and instruction evaluation using the same
  source bodies compiled by ordinary Rust 1.97.1.
- Define eager prefix evaluation recursively. The execution loop maintains
  that its local values equal the specified successful prefix, and its
  increasing index supplies the decreasing loop measure.
- Establish output projection in ABI order and exact output-domain checking.
  Clear caller scratch/output buffers on every failure.
- Derive structural/reference preconditions from verified program admission.
  Structural admission now specifies the complete shape-first, typed-prefix,
  root-order result. The evaluator itself has no executable preconditions;
  it handles malformed graphs defensively. Canonical import and production
  diagnostic wrappers remain outside this proof unit.
- Keep the source of all extracted helpers and their specifications in the
  evaluator/checker identity. Source refactoring intentionally changes identity;
  old receipts must not qualify a new implementation.
- Run negative mutations for arithmetic, Boolean encoding, eager selection,
  reference safety, result projection, error cleanup, and omitted proof coverage.

Pinned microprobes identified concrete compatibility issues before editing:
`Vec::reserve_exact` is unsupported, `i64::from(bool)` lacks a sufficient bundled
specification, and the existing iterator/closure forms need additional proofs.
An indexed loop and Boolean cast can preserve the declared semantics. Any
replacement needs native compilation, independent conformance, and explicit
allocation/resource nonclaims until the V2 meter is complete. No trusted wrapper
will be added to hide an unsupported operation.

## Work discipline and evidence

Before each implementation stage, record its source boundary, mathematical
contract, callers, preserved behavior, intentional identity/API changes, and
required checks here. After the last edit, inspect the final checker result,
function coverage, assumptions, mutation outcomes, native build, and acceptance
gate. Run `python3 tools/atdd.py run --all` immediately before each commit.

Keep proof receipts source-bound and separate from runtime authority. Verus/Z3
checks retain their stated trusted base and their own evidence kind; they do
not silently become Lean `KernelChecked` or upgrade an unchecked solver report
to the repository's `Proved` status. Independently checked proof objects remain
a separate obligation where that status requires them (ADR 0003).

Do not equate 41 passing acceptance scenarios with V2 completion. Track every
open proof and ledger entry, and retain failed probes/counterexamples.

For bounded parallel proof work, use at most two child tasks and depth one.
Children may retrieve premises or audit fixed proof subjects; the main agent
owns the specification, implementation and integration. Verify every returned
artifact before folding it into this record.

## Progress

- Baseline integration pushed in draft PR #113 at `4dd979b`; local proof,
  four mutation controls, native boundaries, zero-vulnerability npm audit, and
  41/41 acceptance passed. GitHub results are tracked independently.
- Premise search replayed enum contracts, Boolean casts, checked arithmetic,
  and slice/Vec prefix loops against the pinned verifier and ordinary Rust.
  These are compatibility probes, not a proof of the production evaluator.
- Shared admission and evaluator unit implemented; whole-core proof remains
  in progress through the remaining stages below.
- Shared admission and eager scalar execution pass the pinned checker locally:
  20 executable functions (including generated clones), 21 specification
  functions/constants and three induction lemmas; 34 verification obligations.
  The native unit has nine tests, including 2,753,237 admission comparisons
  against the retained baseline. The synthesis library's 78 tests passed.
- The gate catches 12 incorrect-behavior mutations and four coverage mutations:
  omitted or weakened postcondition, narrowed input domain, and an uncontracted
  function. A controlled omission verified the same 34 obligations, so the
  gate checks the actual translated signatures, contracts and specification bodies in
  addition to function inventory, counts, pinned tools and source hashes.
- Production `Program::try_new`, scalar-kind checks and evaluation now call the
  shared functions. Source identities include every extracted admission,
  execution and specification file. These identity changes are intentional;
  persisted 1.x receipts do not qualify this V2 implementation.
- The initial acceptance run refused the six templates' earlier source-bound
  certificates. Each was synthesized again into a fresh directory, compared
  with its independent decision table, and replayed natively: 12,064 inputs in
  total. Only certificate fields changed; contracts, search traces, program
  bytes, vectors and emitted source stayed byte-identical. The template
  manifests now contain the newly computed certificates.
- Native regression and source-bound proof receipts cover the shared unit.
  The error-string wrappers, canonical import, meter, complete decision,
  law/genesis and authority composition have not acquired formal proofs.
  Independent review found no blocker within this unit. The full acceptance
  gate and exact-head CI remain separate required checks.
- Mandatory V2 gate, law/genesis enforcement, remaining proof chain and ledger:
  open.
- First unit committed and pushed in draft PR #114 at `9882136`; full 42/42
  acceptance and the clean-head 34-obligation/16-mutation gate passed.
  All 132 GitHub checks passed at that exact head; merge authorization is
  separate from those results.
- Owned V2 meter and eager instruction execution implemented. The direct
  `execute_v2` body is in the proof subject; the `Program` convenience adapter
  is compared natively and remains an explicit correspondence obligation.
  The combined subject passes 57 obligations over 38 executable functions
  (20 retained scalar helpers and 18 additions), 31 specifications/constants
  and four induction lemmas. These counts overlap the first unit.
- Native wider-arithmetic comparisons, all 83 synthesis tests and four external
  API custody refusals passed. Exact-head evidence, full acceptance and review
  of this second unit are still required.
- The second unit's development gate passed all 57 obligations and 16 mutation
  controls. The cached-result reorder still verified all 57 obligations and
  was refused by the separate operational body inventory as intended.
  The prior scalar unit retained its 34-obligation/16-mutation pass.
  No-std compatibility and Clippy passed without implementation lint waivers.
- Six template certificates were regenerated for the second identity change,
  independently checked and replayed natively over the same 12,064 inputs.
  Again only certificates changed; scalar program/vector/source bytes and
  contract/search identities stayed unchanged.
- Second unit committed and pushed in draft PR #115 at `f82bd84`. Full 43/43
  acceptance passed immediately before the commit. Its clean-head meter,
  scalar and arithmetic gates passed 57/34/3 obligations respectively, zero
  errors, with 16/16/4 mutation controls. The scalar and meter counts overlap.
  Independent bounded source review found no blocker. GitHub checks and merge
  authorization remain separate from these local results.
- GitHub refused that second head: its Miri matrix omitted the new integration
  target and release assembly reported an archive inspection failure. The
  target now passes strict-provenance Miri. A separate reproduced native home-
  path leak was fixed, and the rebuilt CLI/archive pass the privacy scanner.
  Repair `e633429` passed 43/43 acceptance immediately before commit. The
  scanner retains refusal of malformed archives and now emits bounded error
  categories. Both GitHub release assemblies passed at that exact repair
  revision; 128 checks passed with four Miri checks still finishing at the
  recorded observation. The original CI archive cause remains unresolved.
  No privacy gate was bypassed.

## Second proof unit: owned V2 meter and instruction attempts

Stage parent: `9882136`. Add a versioned execution unit beside the shared
evaluator, rather than changing the byte-frozen 1.x `Resource` or `BudgetUsed`.
The library constructs the meter at zero and returns an opaque usage report;
the execution API accepts limits and input, never initial usage or a substitute
report. Production finite programs call this same checked implementation.

One Step is charged immediately before each eager instruction attempt, including
an unused, unselected or trapping instruction. Exhaustion prevents that attempt.
A successful graph consumes exactly its node count; a trapped instruction
consumes its Step. Invalid initial scalar tuples refuse with zero Steps. Charge
overflow or exhaustion preserves all counters; a later refusal retains charges
already consumed. Refused execution exposes no partial output.

The mathematical metered-prefix specification pairs the exact eager result with
all eight resource counters. The proof covers arbitrary inclusive i64 domains,
malformed references, output failure and limits up to u64::MAX, with no executable
preconditions. Mutations must challenge charge order, eager attempts, arithmetic
overflow, retained usage, output cleanup and translated proof coverage.

The final-result/counter theorem alone cannot establish charge-before-work
ordering: computing a node before charging and caching its result preserves
those postconditions. The second coverage manifest also fixes translated
executable bodies, with an explicit successfully verifying reorder negative.
This guard preserves reviewed source structure; it is separate from the
extensional theorem and does not prove physical CPU cost or the unfinished
protected-view implementation. A manifest update requires renewed order review.

This unit does not make the V2 authority mandatory or meter canonical decoding,
raw-value/schema projection, allocation or hashing. It does not yet implement a
protected raw-state view, atomic groups of staged operations, law/genesis checks
or authorization/replay composition. Those remain required follow-on units;
scalar inputs admitted before execution are not a proof of the raw-state bridge.

## Third proof unit: canonical raw-input boundary

Stage parent: `e633429`; isolated branch `agent/v2-canonical-input-20261001`.
The retained finite gate reads flat record roots whose leaves are signed i128,
Boolean, closed Enum or payload-free Sum. Complete successor construction
currently restricts state leaves to i128. Command and context may use all four
shapes. Empty per-source records, arbitrary legal field/type/variant IDs and
permuted variant codes must remain supported; a 16-field limit would incorrectly
exclude a legal 32-input program. Preserve the existing ZCVE tags and widths.

The first subunit establishes exact big-endian byte reads and signed conversion
over all byte strings and offsets, including truncation, excessive widths and
offset overflow. Its actual Rust source will be shared by the subsequent
private record decoder. Success must return the exact mathematical integer and
next offset; refusal must be specified precisely. No executable precondition,
application assumption or opaque external body may hide a decoding obligation.
Native comparisons must use the independent standard-library conversion, and
mutations must challenge byte order, bounds, offset, sign and coverage.

The first subunit is implemented in the production `finite::canonical_v2`
module. Its two public readers have no executable preconditions and use the
existing canonical integer byte order. The direct shared-source harness passes
nine obligations: two executable functions, five specifications and four
induction lemmas. Four independent native tests include every two-byte value,
all widths/offsets/truncations in the retained patterns, sign-bit boundaries,
both signed extremes and offsets through usize::MAX. The development gate
catches nine behavior mutations and five coverage mutations. These are
development results. Bounded independent source review accepted the readers and
their coverage, and native, Miri, Clippy and no-std checks passed. All six
templates were freshly synthesized and independently replayed over 12,064
inputs; only their source-bound certificates changed. The unit was committed
and pushed in draft PR #116 at `f88f1a3` after 44/44 acceptance. Its clean-head
byte, meter, scalar and arithmetic gates passed 9/57/34/3 obligations with
14/16/16/4 mutation controls respectively. All 134 GitHub checks passed at that
exact head. Merge authorization remains separate. Both new sources join the
evaluator identity.

Byte helpers alone do not implement a protected state view. The next subunit
must bind decoding and field interpretation to the same owned meter as eager
execution, return opaque usage and actual access observations on refusal, and
derive the scalar tuple from the complete declared raw domain. Costs and
refusal order for ingress bytes and protected field accesses must be explicit;
interpreting payloads before their stated charge is not acceptable.

The current authority witness retains admitted Value envelopes rather than
original canonical input bytes. Re-encoding those Values and feeding a checked
parser would add an unproved adapter. A later mandatory byte-based V2 route
must bind original admitted bytes and its complete schema descriptor directly.
Catalog descriptor extraction, envelope framing/hashing, complete decisions,
law/genesis evaluation, authorization and replay remain proof obligations.
This stage cannot claim to close them merely by proving byte primitives.

The finite-interpreter paper supplied by the owner is assessed in
[the assurance note](FINITE_INTERPRETER_ASSURANCE.md). Its philosophical
scope does not reduce the full-core proof obligation: a fixed specification
and its implementation can be proved over their complete declared domain,
while requirements, assumptions and future revisions remain explicit.

## Fourth proof unit: protected flat-record projection

The bounded stage contract is [V2_PROTECTED_RECORD_STAGE.md](V2_PROTECTED_RECORD_STAGE.md).
It preserves arbitrary closed-code intervals and map order, reuses the same
private meter and mathematical integer readers, and distinguishes descriptor
attempts from physical tracing. Its parent byte-reader unit is qualified at
`f88f1a3` and pushed. The actual private record implementation and its dependency
closure pass 98 obligations with zero errors: 59 executable functions, 51
specifications/constants and 12 induction lemmas. These counts overlap prior
units. The protected gate passes 23 proof-failing behavior mutations and seven
verifying coverage controls; the expanded meter gate passes its 16 controls.
The standalone integer-reader gate retains its nine-obligation/14-control pass.

Independent wire-format and metadata oracles, all 89 synthesis tests, strict
Clippy and no-std checks passed. Actual strict-provenance Miri passed four
internal record tests and the public interface test. All six templates were
freshly synthesized and independently replayed over 12,064 inputs, with only
their certificates changed. Source review, the full 45-scenario acceptance
gate, clean-head replay and exact-head GitHub CI remain required. The protected
record profile and actual source/specification files join the evaluator identity.
This unit does not close the multi-record, catalog-binding, decision or
mandatory-authority bridges.
