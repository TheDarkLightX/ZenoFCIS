# Lessons from Verus for ZenoFCIS

Research date: 2026-10-01. Status: design recommendations with an implemented
shared-source verification pilot; this document does not close the V2
ledger or establish a proof of the complete runtime.

## Recommendation

Use Verus to establish that executable Rust implements an explicit mathematical
contract. Start with shared library code, where one checked implementation can
benefit every application using it. Keep finite synthesis for small decision
domains and independent examples for checking the meaning of specifications.

The authorship of code is not its assurance level. A human or LLM can write
verified code; a generator can emit code whose connection to its specification
has never been checked. Track the specification, executable implementation,
proof coverage, assumptions, and actual build as separate artifacts.

Verus supports annotated executable Rust, mathematical specifications, and
proofs discharged with Z3. Its supported Rust subset and compiler assumptions
make a compatibility pilot necessary. Its attributes also offer incremental
adoption while retaining ordinary Rust function signatures.
[Verus overview][v-overview], [executable attributes][v-attributes].

## Source baseline and current ZenoFCIS gap

This assessment examines ZenoFCIS commit
`b49ad7bf0fc10df16181e8c49ccf9c349b8c79c0` and Verus documentation at
`d85f75780de687461a30e7771985c4a3e9ea0e58`. The latter was also the observed
head of [the owner's fork](https://github.com/TheDarkLightX/verus).
The integration's binary is a separate pinned upstream release, described in
[the pilot record](../verification/verus/README.md); the documentation snapshot
must not be mistaken for the executable's source revision.

At the ZenoFCIS baseline:

| Component | Existing behavior | Remaining obligation |
| --- | --- | --- |
| [Finite importer](../crates/zeno-fcis-synthesis/src/finite_runtime.rs) | Checks canonical program encoding and the IR's type/topology/shape | Prove decoder/evaluator correspondence and preserve exact artifact binding |
| [Finite decision interpreter](../crates/zeno-fcis-authority/src/finite_decision.rs) | Imports a closed plan, checks schema projections, constructs reasons, updates and outbox | Verify the complete implementation and make the intended V2 route mandatory |
| [Inventory conformance](../crates/zeno-fcis-cli/templates/inventory-reservation/tests/conformance.rs) | Compares all 864 admitted inputs against an independent application model | Preserve this independent challenge while expanding proof coverage |
| [Public authority](../crates/zeno-fcis-authority/src/lib.rs) | Still accepts `CatalogTransitionProgram` implementations | Prevent a project callback from substituting for the V2 library decision gate |
| [Law engine](../crates/zeno-fcis-laws/src/lib.rs) | `ProjectLawEngine` supplies law and genesis verdicts | Library evaluation of laws and initial conditions |
| [Resource accounting](../crates/zeno-fcis-core/src/lib.rs) | Supplies budgets and reports; the optional interpreter charges selected operations | Authority-owned state access and complete work accounting, including planned `Step` |

[ADR 0003](adr/0003-epistemic-status.md) controls evidence classifications;
[ADR 0004](adr/0004-v2-ledger.md) records the breaking changes;
[ADR 0005](adr/0005-decision-gate.md) specifies the intended complete gate.
Adding a verifier does not implement these API obligations by itself.

## Lessons to apply

### 1. Make implementation refinement a first-class obligation

The useful theorem is that the running function produces the specified result
for every admitted invocation. A theorem about a second implementation leaves
the translation between them to be established.

For ZenoFCIS, define the observable decision as a tuple containing its class,
reason, successor state, patch, effects, outbox entries and ordering, footprint,
resource observations, and receipt/replay bindings. Prove every component that
the claim names. For unused decision classes or plans, specify their exclusion
or explicit empty value. Do not infer outbox correctness from state equality.

Prefer one shared source body used by both verification and the runtime build.
Bind the checker receipt to its exact source and build configuration. A source
hash identifies the subject; it does not prove the executable has those bytes
or that the compiler preserved its meaning.

### 2. Treat specifications and preconditions as reviewed product behavior

Verus connects functions through preconditions and postconditions, and removes
verification-only code before ordinary execution. Therefore an unverified
caller does not acquire runtime input validation merely by calling a function
with a `requires` clause. [Contracts and ghost code][v-contracts].

Retain schema admission at the external boundary. Prove that its accepted
values satisfy the core's preconditions, including integer ranges, stable IDs,
context fields, and enum variants. Authenticate facts in the shell and pass
them inward explicitly. A proof about an `authorized` Boolean establishes how
the core uses it, not how the Boolean became trustworthy.

Write rejection precedence as part of the specification. Specify success and
failure conditions in both directions when the behavior must be exact; an
implementation that rejects every request must fail that specification.
Keep independently authored examples and deliberate wrong-decision tests to
challenge the rules themselves.

### 3. Use symbolic reasoning where enumeration sets the wrong domain

Verus specifications can express mathematical integers and quantified
properties. Executable arithmetic still uses machine types with obligations
at conversions and overflow boundaries. Its solver is particularly useful for
linear arithmetic; more difficult arithmetic needs explicit proof work.
[Ghost versus executable code][v-ghost], [nonlinear arithmetic][v-nonlinear].

For ZenoFCIS, choose application bounds from intended behavior and operational
limits. Choose the checking route afterward. Retain exhaustive enumeration
when it is cheap, then evaluate symbolic proofs for larger ranges and sequence
invariants. A proof over mathematical integers does not silently extend the
current finite-i64 evaluator or provide a 256-bit runtime integer type.

Account-lockout time reasoning is a candidate: prove that its abstraction of
timestamps preserves every rule that uses time. Only then can a finite proof
about the abstract facts apply to all admitted timestamps. Merely proving the
abstract transition leaves that bridge open.

### 4. Borrow spec-to-code generation carefully

`exec_spec_verified!` generates executable counterparts and equivalence
obligations for a supported fragment. Its documented proof coverage includes
termination, overflow, and function preconditions. Some supported `vstd`
translations remain unverified, including collection operations marked with
an asterisk. `exec_spec_unverified!` omits the equivalence proof.
[Spec-to-executable generation][v-exec-spec].

Apply the architectural idea to ZenoFCIS: author a rule once and mechanically
connect it to execution. Assess the generated operations and their assumptions
before choosing the macro. The first pilot uses ordinary Rust attributes to
avoid introducing a second generation pipeline. A future generator may emit
both the implementation and proof obligations, but the gate must check those
obligations and preserve independent requirements examples.

### 5. Make assumptions visible and enforceable

Verus exposes `assume`, `external_body`, and assumed function specifications as
ways to trust behavior without proving it. Its `--no-cheating` tests demonstrate
refusal of several such mechanisms. Imported trusted libraries still matter.
[Trusted components][v-tcb], [assumed specifications][v-assume],
[no-cheating regression tests][v-no-cheating].

For every proof subject, record the user assumptions and trusted dependencies.
Run `--no-cheating` for the application-owned proof unit and require an expected
verification count. Review transitive library specifications, including the
standard-library interfaces needed by the verified code. Count and hash any
explicitly accepted exceptions; an LLM must not introduce an assumption just
to make the verifier return success.

A replayed Verus success should retain its own tool and trust-base description.
Under ADR 0003, a tool report that the authority does not independently check
does not automatically acquire `Proved` status. Verus success is not a Lean
`KernelChecked` result. Stronger evidence admission needs a separately reviewed
design and an actually implemented checking route.

### 6. Separate functional correctness, determinism, and canonical bytes

A postcondition can permit multiple correct results. For consensus and replay,
ZenoFCIS also needs a uniquely determined observable result. Specify both the
logical decision and its canonical representation, including order and
duplicate handling. Prove that equal admitted inputs and policy give equal
observable bytes through the relevant encoder.

For example, the generation macro can translate logical maps to `HashMap`.
Logical map equality does not determine serialization order. Retain explicit
canonical encoding and reject any dependency on iteration order at that
boundary. [Generated collection representations][v-exec-spec].

Keep purity scans and execution probes as supplementary detectors. A proof of
an arithmetic helper does not establish determinism of every caller, imported
dependency, or external context source.

### 7. Prove resource accounting separately from termination

General executable termination checking in Verus is deliberately lightweight:
its documented guarantee depends on callees terminating. It should not be
treated as a complete termination proof. [Termination checking][v-termination].

The V2 contract must specify what one unit of work measures and where charging
occurs. Prove that protected work cannot bypass its meter, that overflow or
exhaustion refuses before publication, and that failed charging preserves the
appropriate state. Include admission work, interpreter steps, builder work,
and dynamic allocations where the resource policy claims to bound them.
A finite IR graph or a loop ranking function alone does not establish a
complete runtime cost bound.

### 8. Preserve the core/shell proof boundary

Use pure typed inputs and owned decisions as the integration seam. A verified
core can establish how it derives effects. Database transactions, concurrent
updates, crashes, authentication, and external delivery need their own
contracts and evidence.

For system invariants, establish the actual genesis state and preservation by
every admitted transition. For shell integration, retain stale-root refusal,
atomic publication, replay, and outbox tests. These obligations remain even if
the decision implementation is verified. Avoid presenting a local arithmetic
proof as an end-to-end application guarantee.

## Integration approach and pilot

The owner has authorized a compatibility check and an initial integration.
The work is based on the existing finite-contract branch and keeps its public
API and input limits intact. See [the pilot record](../verification/verus/README.md)
for commands and the current evidence status.

The pilot's two functions compute interval width and a product bounded by a
work limit. Their specifications cover all `i64`/`u64` arguments, including
failure cases. They are used in the finite decision contract's admission
calculation. This is a small executable-source verification target that can
validate toolchain coexistence and CI policy before attempting the full
interpreter.

Its proof plan uses wider arithmetic to represent exact intermediate values,
checks the output bound before narrowing, and requires deliberate wrong-width
and wrong-limit changes to fail verification. Changes to helper source must
also change the interpreter identity; extracting code must not remove it from
the source commitment.

Compatibility has several distinct tests:

| Dimension | Required evidence |
| --- | --- |
| Host and tools | Pinned Linux verifier archive, matching Rust toolchain, successful execution |
| Ordinary Rust | Shared source compiles with ZenoFCIS's pinned Rust 1.97.1 and the no-std authority build |
| Proof | Expected functions and obligations verify under the pinned Verus release with no application assumptions |
| Runtime binding | The actual authority imports the shared source and binds it into evaluator identity |
| Regression behavior | Boundary tests, unchanged finite domain, independent template conformance |
| Maintenance | Repeatable CI command and source-bound receipt; explicit upgrade procedure |

Passing this table for the pilot establishes compatibility for its supported
subset. It does not establish that all ZenoFCIS crates, dependencies, targets,
or future proofs are supported.

## Prioritized follow-up

| Priority | Work | Acceptance condition |
| --- | --- | --- |
| P0 | Shared-source pilot implemented; qualify it in CI | Proof, ordinary build, boundary negatives, mutation rejection, tool/source receipt |
| P1 | Verify the finite evaluator's supported operations and evaluation loop | Mathematical semantics match successful results and all error cases; totality and work obligations named |
| P1 | Verify inventory's complete decision construction | All 864 existing cases still match independently; reasons, patches, outbox, footprint and claimed meter behavior included |
| P1 | Complete mandatory V2 authority, law and genesis enforcement | ADR 0005's bypass, replay, mutation, and identity acceptance conditions pass |
| P2 | Prove canonical decoding/encoding and state-view boundaries | Exact admitted-byte and snapshot properties with specified limits |
| P2 | Add larger-domain template proofs | No reduction of legal inputs solely to make enumeration feasible; representation bridge proved |
| P3 | Extend spec-to-code support or proof libraries where pilots expose a need | Reproducer, stated semantics, proof, regression tests, and reviewed trust-base change |

P1 items can inform one another, but no proof result substitutes for unfinished
authority enforcement. Keep the V2 ledger open until its own obligations are
implemented and checked.

## How to use and extend the fork

Use `TheDarkLightX/verus` for explicit, reviewable extensions. Pin a known
upstream baseline and maintain a small patch series. An upstream prebuilt
release verifies upstream code; it cannot validate fork-specific verifier
changes. Those require a new build and a new tool identity.

Prefer reusable proof lemmas, examples, and verified library implementations
before changing the verifier. Extend language or library support when a real
ZenoFCIS proof demonstrates the missing capability. For each extension:

1. Preserve a minimal supported-behavior example and a failing counterexample.
2. State the intended semantics, unsupported cases, and affected trust base.
3. Check positive and negative behavior using the fork's normal regression
   suite; run ZenoFCIS's proof suite with the resulting pinned build.
4. Require review for soundness-sensitive changes. Do not remove a proof
   obligation, add a trusted wrapper, or silence a failure to make a project
   pass.
5. Propose generally useful changes upstream and retain an explicit upgrade
   and divergence record in the fork.

The inspected Verus license is MIT; retain its notices when redistributing
Verus code or binaries. Dependency and distribution review still belongs to
the particular package being shipped. [License][v-license].

## LLM authoring and evidence policy

A future MCP/skill should choose among finite synthesis, shared-interpreter
execution, and verified Rust according to the domain and the available proof
route. It should emit a specification and examples before proposing code,
run the checker, preserve counterexamples, and surface incomplete obligations.

Every retained verification record should contain the specification and source
hashes, target functions, preconditions and result contract, verifier and solver
identities, Rust toolchain/target/features, exact command, verification counts,
exit result, assumptions, mutation results, and runtime build binding.
Record timeout, unknown, unsupported, failure, and success distinctly. Neither
an LLM assertion nor the existence of a receipt grants runtime authority.

Only claim the checked scope. After a complete verified implementation is
actually integrated, appropriate wording is: "The named functional core was
verified against specification X using Verus version Y, under assumptions Z."
For this pilot, the claim is limited to the two finite-domain arithmetic
functions. Neither statement promises that the requirements are complete or
that the whole application, UI, compiler, or shell is correct.

## Primary sources

The links below pin inspected source documents rather than moving documentation
pages. Recommendations above are ZenoFCIS design proposals, not upstream
claims about compatibility with this repository.

[v-overview]: https://github.com/verus-lang/verus/blob/d85f75780de687461a30e7771985c4a3e9ea0e58/source/docs/guide/src/overview.md
[v-attributes]: https://github.com/verus-lang/verus/blob/d85f75780de687461a30e7771985c4a3e9ea0e58/source/docs/guide/src/exec_attr.md
[v-contracts]: https://github.com/verus-lang/verus/blob/d85f75780de687461a30e7771985c4a3e9ea0e58/source/docs/guide/src/requires_ensures.md
[v-ghost]: https://github.com/verus-lang/verus/blob/d85f75780de687461a30e7771985c4a3e9ea0e58/source/docs/guide/src/ghost_vs_exec.md
[v-nonlinear]: https://github.com/verus-lang/verus/blob/d85f75780de687461a30e7771985c4a3e9ea0e58/source/docs/guide/src/nonlinear.md
[v-exec-spec]: https://github.com/verus-lang/verus/blob/d85f75780de687461a30e7771985c4a3e9ea0e58/source/docs/guide/src/exec_spec.md
[v-tcb]: https://github.com/verus-lang/verus/blob/d85f75780de687461a30e7771985c4a3e9ea0e58/source/docs/guide/src/tcb.md
[v-assume]: https://github.com/verus-lang/verus/blob/d85f75780de687461a30e7771985c4a3e9ea0e58/source/docs/guide/src/reference-assume-specification.md
[v-no-cheating]: https://github.com/verus-lang/verus/blob/d85f75780de687461a30e7771985c4a3e9ea0e58/source/rust_verify_test/tests/no_cheating.rs
[v-termination]: https://github.com/verus-lang/verus/blob/d85f75780de687461a30e7771985c4a3e9ea0e58/source/docs/guide/src/exec_termination.md
[v-license]: https://github.com/verus-lang/verus/blob/d85f75780de687461a30e7771985c4a3e9ea0e58/LICENSE
