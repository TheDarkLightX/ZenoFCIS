# V2.3 Zeno Core Standard Library — bounded design for review

Status: designed, not implemented, proved or released. The accompanying `V23_CORE_CANDIDATE_CATALOG.json` contains 24 concrete candidates; its examples are proposed requirements, not observed execution. No source changes or heavy checks were performed.
V2.2 G1's reservation, rate-limiter and approval families remain V2.2 requirements. V2.3 packages and expands those seeds into a useful library; this design does not move unfinished V2.2 work or declare optional slips taken.

## Intake and actual capabilities

Read the entire 1,118-line `docs/V2_1_V2_2_PLAN.md` in four contiguous chunks. No separate existing V2.3 plan was found initially; Root confirmed this is new scope. Subsequently read Root's complete `V23_PLAN_DRAFT.md` through its final companion-artifact paragraph.
Factory source: `/tmp/zenofcis-v22-completion-20261007`, observed HEAD `a1ce03e2bcd917ecb9a61a29be429f5ca09a85b8`, with active working-tree stage1/G11 changes. ZAL source: `/tmp/zenofcis-proof-challenge-20261007`, verified HEAD `7eafb5c6524d1b2ba76e9437cb534b5234b1b106`.
Read all ZAL `SPECIFICATION.md` (367 lines), `GRAMMAR.md` (226), `HARNESS_INTEGRATION.md` (116) and the complete source map; inspected actual `behavior.py`, `factory.py` and MCP registration. Historical source-map statements that all G features are future do not override the newer V2.2 worktree.

| Existing source | What it actually supplies | Limit relevant to this library |
| --- | --- | --- |
| `crates/zeno-fcis/src/program.rs` | Checked catalog/program binding, declaration data, actual evaluation and private Publication custody | Declarations and serialized policy remain untrusted proposals; no application callbacks. |
| `finite/evaluation/mod.rs`, `finite/mod.rs` | Bool/i64 eager DAG, checked add/sub, comparisons, select; finite relational synthesis | 256 nodes; synthesis 16 input/16 output fields, 65,536 input tuples, 4,096 output tuples, 100m conservative node work. |
| CLI `contract/model.rs:22`, `contract/graph.rs:310`, `contract/rules.rs:225` | Fixed-schema contracts, ordered cases, complete state assignments, explicit Accept/Reject/CommittedFailure, genesis/laws/outbox | Factory 32 inputs/16 outputs/256 nodes; decision Mul/Div refused although law language supports them; no parameter-family/import/compose keys. |
| CLI `templates/` | Eight actual templates: durable/prepared counter, inventory, order fulfillment, compliance, withdrawal, treasury, lockout | Useful source/example seeds; templates are not a uniform core package or proof of every specialization. |
| G11 shared checker/API | Source-bound real-evaluator equivalence and exact usage over finite domains | Equality to another program does not by itself prove that either implements the right laws. Default F3 cap 1e8 is not the smaller synthesis input cap. |
| `zeno-fcis-domain` and `zeno-fcis-composed-program` | Retired stubs retaining historical algorithms in private oracle | Do not restore callback authoring. `FEATURE_MATRIX.md` recommending them is stale and needs later final-doc repair. |
| `zeno-fcis-compose/src/lib.rs:1` | Subject-bound composition evidence interface | Explicit external EvidenceVerifier trust; not an automatic kernel proof of composed behavior. |
| CLI `main.rs:1366`; formal-tools `lib.rs:48` | SMT export and evidence statuses; historical Lean exporter test-only | Public Lean export returns UnsupportedMode. Do not promise `prove --backend lean` supplies reusable family theorems today. |
| CLI `synth/mod.rs:74` | Actual Rust, Python and JavaScript emitters plus native runners | More languages must mean a new qualified profile, not relabeling these three; emitted code has no Publication authority. |
| ZAL `behavior.py:479`, `factory.py:21,140`, `mcp.py:17` | Tested finite FSM checker, two canonical surfaces, explanations/comparison/trace, seven MCP tools, reviewed declaration export and actual Authority conformance lane | Flat 1–8 states/events, up to 4 fresh Boolean facts, 1,024 raw tuples; no arithmetic/effects/concurrency; factory subset needs >=2 states. |

## Product and package contract

A proof-function inventory answers which implementation contracts/bodies a verifier covered. The standard library answers which application behaviors a developer can safely reuse, under which parameters, observations, assumptions and evidence. Neither substitutes for the other.
Publish data packages first, not handwritten Rust business cores: a versioned manifest, schemas, parameter constraints, declarative relation/rules, laws, independent owner examples, canonical program/policy bytes, source maps, evidence references and one runnable app use. Generated Rust is declaration plumbing to the existing checked library.
Proposed commands are new work: `core list/show`, `core instantiate`, `core check`, `core compose --check`. Initial local catalog uses exact content/version pins and no online dependency resolver. Instance artifacts bind package hash, normalized parameters, complete state/command/context domains, genesis, rule ordering, outcome/effect schemas, compiler/checker/evaluator and tool identities.
Each core's pure semantics is `step(parameters, immutable_state, command, explicit_context) -> Reject(reason) | Accept(successor, effect_data) | CommittedFailure(reason, successor, effect_data)`. Technical refusal/inconclusive is distinct. Reject preserves all state and emits nothing; committed failure explicitly records its permitted changes.
Context names such as authorized, now, price and principal do not authenticate facts. Package manifests name who supplies each fact, its units, scope, freshness and consistency assumptions. External acquisition and publication remain shells.
The catalog records concrete state/command/context, bounded parameters, positive and negative examples, edge obligations, invariants, dependencies and pending proofs for every candidate. Names/IDs are proposal identifiers, not stable released APIs.

## Achievable first ten, then expansion

| Rank | Core | Useful first guarantee | Initial shape |
| --- | --- | --- | --- |
| 1 | Bounded counter | Exact increment/decrement, bounds, refusal unchanged | One scalar, capacity <=7, delta <=3. |
| 2 | Consumable budget | Exact spending and explicit authorized reset; failure cannot refund | Spent <=7; fixed unit. |
| 3 | Reservation pool | Reserve/release conserve; consume/replenish exact | Two scalars, total capacity <=7. |
| 4 | Approval quorum | Distinct fixed principals; exact quorum; execute once | Three Boolean vote slots, K=1..3. |
| 5 | Versioned register | Expected-version guard and no revision wrap | Revision <=7, value <=3. |
| 6 | Single-request idempotency slot | Exact duplicate is harmless; payload conflict refused | Small exact key/tag/result domains; no eviction. |
| 7 | Capped retry | Last permitted attempt works; callbacks bind attempt; no refund | Attempts <=3 and explicit phases. |
| 8 | Finite FSM | Disjoint declared transitions and explicit rejection | Small control enum; current ZAL route for control-only instances. |
| 9 | Logical deadline | Exact due boundary and no accepted time regression | Bounded logical time and duration. |
| 10 | Epoch quota | Monotone epoch reset and exact per-epoch consumption | Supplied bounded epoch, no division. |

The new reservation candidate proposes a total-cap invariant; existing inventory has separate field bounds. This is a deliberate new contract, not an equivalence claim. Fixed epoch quota and three-person quorum must prove their mappings to the G1 seed families rather than silently renaming different semantics.
Catalog 11–18 adds fenced lease, circuit breaker, escrow, two-account transfer, two-bin stock, two-lane scheduling, debounce and discrete token refill using fixed scalar layouts. These are follow-ons after useful apps, not release-count padding.
Catalog 19–24 defers qualification of keyed maps, FIFO, multiasset ledger, U128 budget, fixed-point allocation and portfolio guard to the actual wider-value/compound/arithmetic capabilities delivered by G3 or later explicit work. A U128 codec tag or toy flattened fixture is not full-width executable support.
Before claiming any proposed parameter range supported, compute exact input/output products, graph sizes and relation-check work for every instance. Shrinking a named supported domain to pass a cap is a contract change. Beyond a cap, say unsupported/inconclusive; never replace full coverage with boundary examples.

## Evidence, parameterization and nonvacuity

Separate five evidence subjects: interpreter implementation; component relation/laws; exact parameter instance; generated artifact/lowering; composed application plus its shell. A green engine proof only establishes the first.
Example tests detect mistakes but give no universal coverage. Complete finite relation/transition checks establish only their exact declared domain and trusted enumeration/evaluator assumptions. Preserve existing ADR classifications, including the finite Selected claim, and record coverage rather than inventing a stronger status label.
SMT UNSAT and solver agreement remain Attested; replayed SAT can refute a component on a checked concrete input. `KernelChecked` names an actual Lean-checked obligation, not any successful formal command. Source-bound Verus theorems may establish their stated claims without being relabeled KernelChecked.
A family claim requires every stated parameter value: either a checked theorem quantifying over the range, or complete checked enumeration of the whole bounded parameter/instance space. Both also require exact specialization/encoding/runtime refinement. Checking C=3 and C=7 does not prove C=1..7; a theorem for arbitrary mathematical integers does not automatically cover eager bounded execution.
G1 integration should reuse its genuine qualified theorems/evidence if available at final V2.2 head. Public Lean support is currently blocked; restoring a qualified route or authoring source-bound Verus component theorems is explicit implementation work. No public API receives a trusted user callback to assert that a law holds.
For each published core require nonempty legal genesis, totality on admitted inputs, at least one feasible successful witness for every intended command family, exact threshold successes, explicit business refusals, technical-refusal behavior, arithmetic bounds including inactive eager branches, and induction/base cases for temporal safety claims.
Reject-all implementations must fail positive examples and relation realizability. Domain-tautological invariants and unreachable successful branches receive visible diagnostics. Owner-reviewed examples should be independently written from the public contract, not copied from generated outputs.

## Pure composition before optimization

Introduce one bounded, data-only composition manifest: pinned instances, explicit state ownership, typed command/context bindings, named output-to-input edges, stable occurrence order, initial-state mapping, effect ownership and a global invariant. Compilation proposes one ordinary complete checked descriptor; it does not build a chain of publication callbacks.
Start with fixed acyclic sequential dataflow and one atomic application transition. Every edge must discharge producer output range -> consumer precondition, exact unit/domain mapping, source/epoch/request binding, frame/noninterference and total resource budget. One owner controls each state field, nonce, reservation, quota and delivery occurrence.
Local invariants are necessary but insufficient: prove the coupled invariant and whole composed decision. For example, separately valid approval and budget objects must bind the same request, amount and version; otherwise approval for 2 can authorize spending 7.
Default composition failure is abort-all: no component candidate becomes authoritative until every required check and the shared invariant pass. CommittedFailure requires an explicit whole-composition contract specifying which changes survive; a failed later stage never implicitly preserves earlier debits or refunds attempts.
First-profile lowering accepts only range-safe eager component computations across the entire admitted product; a lazy sequential reference cannot be flattened into eager Selects without a trap/precedence proof. Refuse unsupported composition instead of evaluating a trapping inactive branch or inventing placeholder semantics.
Check complete decision class/reason, every successor field, exact ordered deliveries and payloads, and chosen resource observations against an independently written composite reference. Alias/omission/duplicate/reordered occurrences, mismatched version bindings and shared-cap double spending are required negative controls.
Publish through the existing single checked Authority/Publication and SQLite transaction. Two individually valid publications do not form an atomic composition. Parallel component execution, distributed transactions, arbitrary maps and proof-carrying callbacks are outside the first library slice.

## Six worked application compositions required for release

These are exact proposed acceptance journeys, not executed demonstrations. Use reduced but complete finite configurations and report their product/work limits before implementation; never imply all maximal parameter products fit current caps.

1. **Warehouse reservation service:** reservation_pool + idempotent_request_slot + finite_fsm. Start capacity=5, available=5, held=0. New order key1 reserves3 -> available2/held3 and one pending order; same key/payload retry leaves both unchanged. Ship consumes held3 once -> available2/held0 with one shipment intent. Conflicting key payload or ship-before-reserve refuses globally. Shared invariant: order's reserved quantity equals its share of pool holdings; total committed shipment equals consumed units.
2. **Expense approval app:** approval_quorum + consumable_budget + idempotent_request_slot. B=7,K=2,spent=0, request amount3. Principal0 then1 approve that exact request/version; Execute atomically sets executed, spent=3 and emits one payment intent. Retry creates no new spend/effect; one vote or amount changed to4 refuses. Shared invariant binds votes, request amount, spend increment and delivery payload. Model payment intent, not bank settlement.
3. **Reliable job runner:** capped_retry + idempotent_request_slot + logical_deadline. A=3: Start consumes attempt1; matching failure retains charge and arms logical due=4; Start at now3 refuses, at now4 consumes attempt2. Success for attempt1 now refuses as stale; success2 completes once. Shared invariant identifies one active attempt and no duplicate execution intent. Eventual completion requires an explicit environment assumption, not a retry safety theorem.
4. **Document release workflow:** finite_fsm + approval_quorum + versioned_register. Draft revision2, two required reviewers approve revision2; Release(expected2) atomically yields revision3/released. Editing to revision3 before release invalidates the old approvals; stale release refuses every change. Shared invariant: approval subject equals released immutable content/version. The finite value tag is the complete modeled content domain; real document digests/identity are a separate adapter qualification.
5. **API quota service:** epoch_quota + idempotent_request_slot + versioned_register. Q=5, epoch1 used4; new request amount1 yields used5 and one service intent. Exact retry leaves used5; new request amount1 refuses. Epoch2 resets then consumes to1; delayed epoch1 refuses. Shared invariant: each first accepted request contributes exactly once to current-epoch usage. External epoch derivation and request identity require explicit shell evidence.
6. **Equipment booking:** reservation_pool + logical_deadline + versioned_register. Capacity2: reserve1 -> available1/held1, booking version1, deadline4. Confirm before expiry binds version1; timeout at4 releases exactly once and advances version. A concurrent confirm/timeout resolves by one shell CAS; the loser rereads and refuses stale version. Shared invariant: booking state, held quantity and release eligibility agree; pure semantics alone is not a concurrent lock.

Every app must also demonstrate a successful fresh instance install, generated positive examples, one exact coupled-invariant mutant caught, persistent commit/replay, refusal with no write, and delivery replay/idempotency where relevant. At least one app should reuse each of the first ten cores; the six above cover all ten.

## ZAL UX and language follow-ons

Existing ZAL already has canonical English/symbolic views, exact help/explanation, shortest invariant witnesses, revision-bound proposals/review, shared terminal/MCP workflow and reviewed exports. Improve discovery by letting users select a library behavior, see parameter bounds and one successful/refused scenario, then inspect assumptions, coupled invariants and evidence when needed.
Do not present arithmetic budgets or effectful retries as current finite-fsm/1 ZAL. First support control-only FSM library instances; a later versioned arithmetic/effect profile needs exact eager/trap semantics, transition requirements and actual lowering conformance. State invariants alone cannot express every authorization requirement.
Preserve explicit owner examples and distinguishing witnesses, stale-review refusal and read-only explanations. The inspected MCP lists seven tools and no acceptance tool. A separately installed skill mentioning optional native review is not evidence that this source supports it. Cooperative same-user review is not authenticated approval; protected review storage is a separate deployment contract.
Measure task success and misunderstanding on the six app journeys, especially positive boundary cases, supplied authorization, failure-vs-reject and stale evidence. No usability superiority or novelty claim is supported by this design; native browser/accessibility/harness qualification is distinct from DOM-double tests.
For more languages, first specify whether a profile calls the checked engine or independently executes generated source. TypeScript bindings to existing JavaScript are useful ergonomics, not a new interpreter proof. Choose one additional compiled language only after an application requirement; pin its exact integer/Boolean/enum/refusal/byte ABI, native toolchain and full finite conformance/negative corpus. No target shell gains authority merely from an engine call.

## Bounded delivery gate

Stage A: reconcile the actual V2.2 G1 seed evidence, establish package/instance/composition manifest schemas, publish three seed packages with explicit statuses. Stage B: deliver the first ten and six apps; require complete declared relation checks and component law evidence, actual Authority conformance, shared-invariant checks, source-bound replay and independent review. Stage C: add supported-profile follow-ons driven by app demand; wider cores wait for their separately qualified capabilities.
Freeze exact program/policy/domain/parameter/evidence hashes; qualify the actual declaration/compiler/core path with pinned tools. Update coverage inventories for new checked functions; run native/ATDD/package/CI gates on the final tree. A catalog count, copied proof reference or examples-only release cannot be labeled a formally checked standard library.
Catalog validation performed here: JSON roundtrip, exactly 24 unique IDs, valid dependency references, first-ten list, explicit Accept witness and separate rejection/edge obligations for each core. This validates planning structure only. No theorem, synthesis, generated app, language runner or composed journey was executed.
