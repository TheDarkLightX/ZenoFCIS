# V2 normal program API migration

The [smaller V2 plan](V2_VERIFIED_CORE_PLAN.md) defines the supported scope.
Simplification is in progress; the current implementation and intermediate
checks do not constitute final combined qualification or a released API.

The default `zeno-fcis` root and `prelude` now select the existing checked declarative execution family. This is an intentional breaking API cutover, available with default features and with `default-features = false`. Optional legacy features do not restore the old root exports.

```rust
use zeno_fcis::prelude::*;
```

Use `program::schema` for the complete original schema description, `program::scalar` for the closed control graph, `program::declaration` for original input bindings, complete branches, reasons, footprints and channels, and `program::law` for required transition/genesis laws. `ProgramDefinition` is the actual checked descriptor. These declarations are proposals, not authority.

Call `bind_catalog(original_schema, description, catalog_limits, original_policy, definition, framing, channel_roots)`, then `bind_program(&catalog)`. The `policy_bytes` helper serializes the proposed complete policy only. Review and bind the exact retained bytes; successful serialization is neither admission nor policy adequacy. Schema digest configuration remains a declared framing boundary; this API does not compute a schema digest.

Pass exact original state, command and context envelopes as `Invocation`. `Program::publish` returns a private non-Clone `Publication` only after actual library evaluation succeeds with a committing business class. `PublicationOutcome::Reject` retains the audited noncommitting outcome. A technical publication refusal retains the actual usage/read/attempt/law reports and grants no committing capability. A refusal during output encoding or publication replay can retain a successful core evaluation and its audit subject; these reports do not authorize publication. `publish_genesis` evaluates genuine initial laws. `replay_publication` and `replay_genesis_publication` recompute before comparing complete subjects.

The publication poststate is an original envelope. Its ordered delivery destination, payload and idempotency values are bare original ZCVE; checked destination/payload roots accompany them. The idempotency atom is a policy marker, not a globally unique delivery identity. A shell must accept the genuine capability, bind exact authority/prestate/subject/wires and handle actual durable state comparison. Feature `sqlite-shell` exposes the V2 adapter as `zeno_fcis::sqlite`; the shell has its own qualification and trust boundary.

## S1 storage migration

The unreleased schema-9 `V2SqliteShell` replaces the separate schema-7 V2 and
schema-8 History adapters. `HistorySqliteShell`, `admit_history` and the core
History publication wrappers are removed. Use the bound Authority directly for
`publish_genesis`, `publish` and exact publication replay, then pass the genuine
publication to the surviving shell. Old database versions are refused; there
is no implicit migration or permission to relabel an old store as schema 9.

The shell stores the complete policy/evaluator identity once. It compacts each
publication by removing that identity from its nested audited subject, and
reconstructs the full bytes before exact core replay. Public compact/expand
helpers return untrusted bytes, never a publication capability. Full subjects
remain the inputs to genesis and certificate commitments; delivery identity
also binds the transaction certificate, distinguishing fresh operations after
a state recurs.

The old history-pair export/import API is retired. Full retained subjects can
go directly through the Authority replay method, which compares their complete
bytes with its recomputed subject before returning a capability. For compact
retained data, keep its source identity and compare it with the destination
authority before reconstructing and replaying the full subject. Do not replace
the retained identity with the destination's identity before checking equality.
Prepared-counter's owned compact replay artifact performs these checks and
preserves exact-key idempotency after later commits.

Prepared-counter retains its 16,384-byte aggregate limit. Its accounting includes
the owned compact authorization and the copy persisted in the complete durable
bundle, including certificate, state and delivery fields. Identity construction,
copying and hashing remain outside that logical transition bound. The storage
encoding change does not remove root/version/CAS, crash, replay or delivery
obligations. Final-source qualification remains required.

## S5 invocation kinds

Genesis and ordinary transitions share `Outcome`, `Evaluation`, `Publication`
and `PublicationOutcome`; the separate `Genesis*` wrappers are retired.
The invoked `Kind` is retained through evaluation, publication, replay and
technical refusal. Use `publish_genesis` for initialization and `publish` for
ordinary decisions; a shared capability type does not make these interchangeable.

`V2SqliteShell` accepts only Genesis for creation and only Transition for
ordinary commits and publication bundles. A genuine publication of the wrong
kind returns `Error::InvocationKind` before a history refresh or checkpoint
write; a foreign identity still returns `Error::Identity` first. Wrong-kind
bundle refusal also precedes the version check. Correct-kind bundles retain
their existing refresh, version, identity and pre-state checks. These are
unreleased API changes; final combined qualification remains required.

## V1 consumer migration

V2 breaks the V1 root API on purpose (ADR 0004). A V1.x consumer of the
umbrella crate makes these changes:

| V1 | V2 |
| --- | --- |
| `use zeno_fcis::prelude::*;` | `use zeno_fcis::legacy::prelude::*;` for V1 data types and standalone tools; the crate root is now the checked program API |
| `Value::Bool(true)` (public variants) | `Value::boolean(true)`; variants are private, so use the constructors and `kind()` |
| `BudgetLimits::zero().with_limit(..)`, `Budget::new`, `charge` | `zero_limits().with_limit(Resource::Step, n)`; the library owns the meter, so a caller cannot charge or report usage |
| `BudgetExceeded` as a standard error | no caller-side equivalent; budget refusals arrive inside `Refusal` from the library route |
| `TransitionDecision` | `PublicationOutcome` from the checked route |

`legacy` contains inert compatibility data and standalone evidence utilities
only. It does not contain native authoring, meter construction, candidate
sealing or reference-shell commit functions. Those are retired, and their
original algorithms and assertions run only in the private kernel-law oracle.
A consumer that implemented native transitions, budgets or sealers moves to
checked `program` declarations, as described in the table below.

The pinned external consumer (`test-projects/external-consumer`) is the
executable example, with exactly the changes above. Its feature selection is
unchanged: `default-features = false` with `std`, `bootstrap`,
`composed-program`, `backend`, `authenticated-authority` and `authoring`. It
compiles and runs against the locked workspace graph in the acceptance scenario
"Compile the V1 consumer through its documented V2 migration".

`test-data/v1-compatibility/baseline.json` is now a V2 migration pin. It pins
the migrated consumer and the V2 foundational protocol sources with their
embedded wire tests, and `tools/check_v1_compatibility.py` refuses when any
pinned file differs. V2 intentionally changes the V1.0 foundational sources
(ADR 0004). The V1.0 pins remain in git history at `02694c2`. The pin shows that
the documented migration compiles; it does not establish universal downstream
source compatibility.

## Normal authoring and compatibility

`Limits` configure the eight resources, including `Resource::Step`. Only library execution constructs `Usage` and the private meter. Applications do not supply usage, seal a candidate, provide a native transition callback or return a law verdict. Raw caller values cannot substitute for original envelopes. Declaration bytes cannot be mutated through the immutable catalog/program borrow.

| Previous normal operation | Current normal route | Remaining compatibility obligation |
|---|---|---|
| `Transition`, `DomainMachine`, `CatalogTransitionProgram` application implementation | Complete `ProgramDefinition`, `bind_catalog`, `bind_program` | Native authoring is retired; complete historical algorithms remain executable in the private kernel-law oracle. |
| `Budget::new`, `finish`, claimed `BudgetUsed` | `zero_limits`, policy limits, actual `Evaluation::usage` | Caller meter constructors/reports are retired from normal packages; original budget assertions remain private oracle tests. |
| `CandidateBuilder::seal`, `CataloguedTransitionBuilder` | `Program::publish` | Normal application sealers are retired. Publication construction is library-private and checked across crate boundaries. |
| `GeneratedTransition`, `begin_*transition`, raw `pre_state` | Closed declarative graph over original frames | Bootstrap emits complete untrusted proposals and calls the actual checked catalog/authority constructors. Template retirement has separate source-bound qualification. |
| Application `ProjectLawEngine` | Closed laws, required IDs, guarded observations, actual genesis | Historical native law algorithms remain in the private verification oracle; declarations cannot supply application verdicts. |
| Unchecked `BoundDeliveryInterpreter` | Ordered wire deliveries in a private Publication | Persistence and physical delivery remain shell responsibilities. |
| `zeno_fcis::core`, `receipt`, `authority`, `transition`, `domain`, `synthesis` | No normal whole-module aliases | Explicit `legacy` contains inert compatibility data and standalone evidence utilities, not native authoring constructors. |

The `minimal_core` example directly binds and invokes a checked program; its support file contains only example declarations and manually authored original schema/input bytes. The original native minimal-core and bounded-preparation algorithms/assertions remain executable in the private kernel-law oracle. `checked_backend` and `completion_scaling` expose standalone evidence computations, not publication authority. No normal package depends on that private oracle for production execution.

The original `bounded_completion` main and every original check run as `oracle::tests::bounded_completion::original_bounded_completion_example_runs` in the private verification suite. Run `cargo +1.97.1 test --manifest-path verification/Cargo.toml -p zeno-fcis-kernel-laws --all-features --locked --offline -- --test-threads=1` to execute the complete suite. The obsolete normal example registration is retired after that unchanged private execution. Its accumulator `-100..=100`, items `-3..=3`, chunks of two then one, incomplete-finish refusal and result `[6]` remain distinct from prepared-counter's original domain. The normal `finite::v2_continuation` replacement now executes those fold domains with shared charged usage and a private cursor; its whole-source proof, native comparisons and independent controls have bounded acceptance. Renew that evidence with the final integrated source. Source retirement alone did not implement it. `completion_scaling` remains unchanged.

Native Rust hosts execute the same checked closed scalar/record bodies exposed by `Program`. A Wasm library compilation uses those same bodies; it does not establish native/Wasm execution parity or a universal Rust/code-generation theorem. Arbitrary native code, legacy synthesis/compiler outputs, and newly generated application callbacks have no automatic bridge into authority. These direct reexports do not extend the supported scalar/record execution profile or shrink legal template domains.

This umbrella adds no protocol enums. Handle existing `#[non_exhaustive]` enums, including `Resource`, with an unsupported/refused fallback for unfamiliar outcomes; never grant commit authority from that fallback. The [smaller V2 ledger](V2_LEDGER_SCOPE.md) withdraws the blanket protocol-enum migration. Final supported-route proof and release checks remain mandatory. Any additional optimized native/Wasm execution path needs its own qualification before becoming authoritative.

Bootstrap generates `GeneratedProgramProposal` and pure `GeneratedDeclarations` helpers. Supply the complete original schema description/bytes, reviewed policy, execution descriptor and original frame/channel roots. `GeneratedProject::bind_program` checks exact generated catalog correspondence, then calls the same actual `v2_catalog::bind_original`/`v2_authority::bind` implementation selected by the root `Program` API. It cannot infer policy adequacy from catalog metadata. Typed adapters retain complete original envelopes; declaration and inert effect/channel helpers grant no publication capability.

Use `zeno-fcis describe` to discover the normal workflow and `new --template` to copy a supported declaration example without overwriting a nonempty directory. `check --format json` reports authoring diagnostics with runtime admission not run; `generate` renders authoring artifacts and has the same nonclaim. MCP invokes these exact CLI commands. Standalone `synth` tools still provide bounded finite/target evidence and cannot turn generated native code into runtime authority. See [agent installation and usage](LLM_SYNTHESIS.md) and the [synthesis-first skill](../skills/zenofcis-synthesis-first/SKILL.md).
