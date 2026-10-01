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
| 1 | Shared scalar IR and execution loop | Every instruction and eager prefix match the mathematical semantics; invalid input and traps refuse without exposing partial output | Admission/eager execution unit checked locally: 20 executable functions, 34 obligations; exact-head CI pending |
| 2 | Schema and canonical admission | Every accepted byte string decodes to exactly its typed value, respects declared ranges and limits, and consumes all bytes | Structural IR admission checked; codec and schema correspondence open |
| 3 | Library meter and state view | Actual accesses and steps are charged before protected work; exhaustion and overflow cannot mutate authoritative state | Existing budget APIs; mandatory V2 path open |
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
