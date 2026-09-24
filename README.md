# ZenoFCIS

ZenoFCIS is a high-assurance Rust library family for functional-core / imperative-shell systems.

Its primary design rule is:

```text
immutable state + command + policy + authenticated context
    -> pure total transition
    -> Accept | Reject | CommittedFailure
    -> exact immutable candidate
    -> atomic imperative-shell interpretation
```

The semantic kernel treats values, decisions, resource budgets, canonical bytes, and commitments as explicit protocol data. It forbids unsafe Rust and is designed for `no_std + alloc` use without clocks, randomness, networking, filesystems, databases, or executable effect closures.

## Why functional core, imperative shell

Many failures in stateful systems come from deciding and acting in the same
step. A function reads a clock, updates a row, calls a service, and chooses an
outcome, and none of that can be replayed or checked afterwards. ZenoFCIS
separates deciding from acting, then makes the boundary enforceable.

**The core decides.** A transition is a pure, total function from immutable
inputs to exactly one decision:

- accept, with a candidate;
- reject, with a reason and no change;
- commit a failure, with a reason and a candidate.

The candidate is data: an exact patch to the state, the external obligations to
queue, and the resources used. With no clock, file, network, or randomness in
the core, the same inputs produce the same bytes. How that is assured depends on
the code:

- A synthesized finite core is deterministic by construction. Its language has
  no clock, randomness, input or output, or unordered collection.
- The library's own semantic crates forbid unsafe Rust and are checked for
  ambient effects.
- A hand-written project transition is deterministic by contract, and three
  checks test that contract:
  - `zeno-fcis purity` reports clocks, environment reads, randomness,
    hash-map iteration, global state, and the other sources it can see in the
    decision code;
  - `execute_probed` runs a decision several times and withholds it if any two
    runs differ;
  - reopening a SQLite store re-executes every persisted transition and
    requires identical bytes.

  These checks detect nondeterminism; they do not prove its absence. See
  [determinism](docs/DETERMINISM.md).

**The shell acts.** Persistence, delivery, and every external operation happen
outside the core, and only on values the core produced. External work is an
outbox entry with a stable delivery identity, so a destination can recognize a
retry and perform the work once.

**Authority is a value, not a code path.** The SQLite shell, the library's
production port, cannot publish a raw decision. It publishes only a
`CatalogAuthorizedTransition`, a type that no code outside the library can
construct. The library mints one only after it:

1. runs the reviewed program itself on the admitted invocation, so no caller
   can supply the decision;
2. obtains a verdict on every applicable project law from the deployment's law
   engine, and requires each law to hold;
3. binds the result to the exact catalog, hash provider, law set, and
   deployment.

Reopening a store re-authorizes its entire history.

The strategic shape follows from one rule: **untrusted components propose, and
small deterministic checkers judge.** Hand-written code, LLM-written code,
synthesizers, solver models, databases, and mounted runtimes may all propose
values. Strict decoders, exhaustive checkers, replay, and the authority decide
what is admitted, and their verdicts are types that other code cannot forge.
Some results still rest on components that the library trusts rather than
rechecks:

- a project's law engine, whose verdicts the authority enforces;
- a solver's `unsat` answer;
- the pinned Lean kernel and runtime.

```text
proposers (untrusted)          judges (small, deterministic)       shell (effects)

application or LLM code  ─┐    canonical decode                    atomic publication
synthesizer or solver     ├──> schema and catalog admission   ──>  outbox delivery
mounted runtime or store ─┘    authority execution + law checks    with stable IDs
                               nominal authorization
```

The aim is to keep the part a person must review small: the meaning of each
decision, stated as types, reasons, laws, and claims. Machines check the rest
against it. Every published decision is checked against the laws, and a finite
core is checked against its contract on every input. Each result also states
what it establishes.
[Design record 0003](docs/adr/0003-epistemic-status.md) classifies every status
as Identified, Attested, Checked, or Proved, with its scope and trusted base, so
a passing check is not mistaken for more than it shows. The
[principles](docs/adr/0002-principles.md) and [architecture](docs/ARCHITECTURE.md)
describe the full design.

### What this gives developers, LLMs, and agents

- **Developers** test one pure function with plain values. Decisions are data,
  so tests compare exact bytes and a failure replays from its inputs. Reviewers
  start from the catalog of reasons and the laws. Generated typed APIs name
  each state field, so code need not use raw paths, and authoring diagnostics
  arrive together in one pass.
- **LLMs** get a small, well-defined target: one function with explicit inputs
  and outputs and no ambient effects, and checks that give the same answer on
  every run. The design assumes generated code can be wrong:
  - generated typed APIs narrow what it needs to write;
  - every decision is checked against the project laws before it can be
    published;
  - a finite decision core can be synthesized and verified exhaustively
    instead of being written by hand;
  - an [inductive claim](docs/INDUCTIVE_CLAIMS.md) checks an invariant over
    the full integer range by induction over the laws the authority enforces,
    for a hand-written program as much as a synthesized one. The solver's
    `unsat` for the step is attested, not independently checked.
- **Agents** get reproducible, machine-readable feedback:
  - `zeno-fcis describe` and versioned JSON results with stable exit codes;
  - counterexamples to repair against: a synthesis refutation, or a solver
    model replayed through the interpreter;
  - a closed registry of acceptance commands;
  - gate evidence stamped with the exact commit it tested.

  Checks also catch work that passes without meaning anything.
  `zeno-fcis check --require-substantive` refuses laws and claims that no
  transition outcome can change. `--require-resolved-paths` refuses laws and
  claims that read fields the schema does not declare.

## See it run

This is the published CLI running inside a virtual terminal. One command reads
the Mini Determinator project and reports its typed components, claims,
remaining checks, and content-bound program identity.

[![ZenoFCIS checking the Mini Determinator in a virtual terminal](docs/assets/marketing/terminal-mini-determinator-check.png)](docs/tutorials/MINI_DETERMINATOR.md)

The Mini Determinator links the public semantic core into a freestanding Rust
kernel, boots through UEFI in QEMU, checks opposite worker completion orders,
and rejects conflicting private writes without authoritative state change.

[![Mini Determinator kernel running in QEMU](docs/assets/marketing/mini-determinator-qemu-kernel.png)](docs/QEMU_MINI_DETERMINATOR.md)

Authoring failures are accumulated in one bounded pass. This deliberately
invalid example reports a duplicate stable ID, an unknown type reference, and
an invalid merge order together:

[![Three accumulated diagnostics from one check](docs/assets/marketing/terminal-accumulated-diagnostics.png)](docs/tutorials/MINI_DETERMINATOR.md#see-three-authoring-mistakes-at-once)

Start with the worked [Mini Determinator tutorial](docs/tutorials/MINI_DETERMINATOR.md),
reproduce the [QEMU capture](docs/QEMU_MINI_DETERMINATOR.md), or inspect all
[CLI and QEMU captures](docs/assets/marketing/README.md).

## Canonical bytes and byte-level enforcement

Canonical bytes are the one permitted byte representation of an admitted
semantic value. ZCVE/1 fixes type tags, integer and length encodings, field and
map-key order, collection shape, and optional/sum representation. Its bounded
decoder rejects malformed structure, duplicate or reordered entries, trailing
bytes, and every input that does not equal a canonical re-encoding of the
decoded value:

```text
untrusted bytes
    -> bounded structural decode
    -> typed immutable value
    -> canonical re-encode
    -> require original bytes == re-encoded bytes
    -> schema and authority admission
```

This makes state roots, candidate IDs, receipts, replay bindings, and evidence
commitments deterministic across supported implementations. Canonical bytes
are not encryption and do not establish business correctness by themselves.
Schemas establish shape; catalogs, invocation witnesses, project laws, and
nominal authority establish what the bytes mean and whether they may be
published. See the [canonical-bytes guide](docs/CANONICAL_BYTES.md).

## Start here

For a project-neutral multi-domain application, enable the composed-program
path and import the curated prelude:

```toml
[dependencies]
zeno-fcis = { version = "=1.1.0", default-features = false, features = [
    "composed-program",
] }
```

```rust
use zeno_fcis::prelude::*;
```

The supported application path is:

```text
ProjectProfile + ProjectCatalog
    -> generated transition or typed domain machines
    -> ComposedDomainProgram
    -> verified project laws
    -> CatalogCommitAuthority + policy-bound genesis
    -> authorized shell publication
```

Read the [installation guide](docs/INSTALLATION.md),
[quickstart](docs/QUICKSTART.md), [API reference](docs/API_REFERENCE.md),
[crate map](docs/CRATE_MAP.md), [feature matrix](docs/FEATURE_MATRIX.md), and
[LLM integration guide](docs/LLM_USAGE.md). Agents can discover the CLI with `zeno-fcis describe` and inspect generation drift with
`zeno-fcis generate --out generated --check --format json`. The
[V1 product contract](docs/V1_PRODUCT_CONTRACT.md) defines the stable V1 feature scope and supported adopter journeys. [BDD and ATDD](docs/ACCEPTANCE_TESTING.md)
bind those journeys to fixed executable commands, while the optional
[developer guardrails](docs/DEVELOPER_GUARDRAILS.md) reject selected unsafe
coding-agent actions before execution. The
[LLM cybersecurity review orchestrator](docs/LLM_CYBERSECURITY_REVIEW.md),
[evidence-first playbook](docs/SECURITY_REVIEW_PLAYBOOK.md),
[EPI hotspot model](docs/SECURITY_HOTSPOT_MODEL.md), and
[dated standards snapshot](docs/SECURITY_STANDARDS_SNAPSHOT.md) provide a
deterministic review queue, constrained prompts for less-capable models,
exploit-chain proof obligations, scanner guidance, and a machine-readable
report contract. Run `python3 tools/security_hotspots.py check` to reject
unreviewed ranking or model drift. The
[V1.1 release notes](docs/V1_1_RELEASE_NOTES.md) describe the new workflows and
compatibility boundaries. The [V1 release notes](docs/V1_RELEASE_NOTES.md) retain
the original stable API scope. The
[RC3 release notes](docs/RC3_RELEASE_NOTES.md) retain the historical candidate
scope. The owner-facing
[V1.1 release checklist](docs/V1_1_RELEASE_CHECKLIST.md) separates exact-source
repository evidence from signing, publication, and external review actions;
the [V1 release checklist](docs/V1_RELEASE_CHECKLIST.md) remains historical.
Runnable examples are checked permanently:

```bash
cargo +1.97.1 run -p zeno-fcis --example minimal_core --locked
cargo +1.97.1 run -p zeno-fcis --example checked_backend --features backend --locked
python3 tools/atdd.py run --all
```

## Example applications

`zeno-fcis new DIR --template NAME` creates a complete application: an
authored `project.zeno`, generated typed bindings, a decision, a law checker
that checks every decision before it is published, a SQLite store with an
outbox, tests, and a README that states its rules.

| Template | What it shows |
| --- | --- |
| `durable-counter` | The smallest complete application: a synthesized step, runtime laws, a committed failure, and restart with delivery retry. |
| `account-lockout` | Failed logins committed as failures, time as an input instead of a clock read, authority from the request context, and alerts through the outbox. |
| `order-fulfillment` | A hand-written state machine that sends idempotent payment and shipping requests and rejects duplicate or late callbacks. |
| `inventory-reservation` | A decision core synthesized and verified on all 432 inputs, commands with quantities, and a conservation law. |
| `compliance-gateway` | An expert system's rule base as the synthesis contract, checked on all 720 inputs; every decision names the rule that fired, a rule base with a conflict, a gap, or a dead rule fails the build, and claims for `zeno-fcis prove` state the strikes invariant's inductive steps. |
| `prepared-counter` | Checked bounded completion and prepared batches with a bounded publication size. |

In the account-lockout, order-fulfillment, inventory-reservation, and
compliance-gateway examples, every law formula in `project.zeno` can constrain
some transition and reads only declared fields (`zeno-fcis check --require-substantive
--require-resolved-paths` passes). Each also ships decision examples, a
conformance test through the running application, a determinism probe, and the
purity command for its decision code. `python3 tools/check_generated_application.py`
creates, builds, and tests each template in this table as an isolated package
against this checkout.

## Authoring and checked synthesis

V1 includes the inert `.zeno` language, canonical typed project AST, accumulated
diagnostics, bounded relational and temporal logic, deterministic formal-tool
adapters, and the `zeno-fcis` CLI. Start with the
[authoring contract](docs/RC3_AUTHORING_CONTRACT.md),
[language specification](docs/ZENO_LANGUAGE_V1.md),
[temporal semantics](docs/TEMPORAL_LOGIC_V1.md),
[formal-tools contract](docs/FORMAL_TOOLS_RC3.md),
[Mini Determinator reference](docs/MINI_DETERMINATOR.md),
[Mini Determinator QEMU kernel demo](docs/QEMU_MINI_DETERMINATOR.md), and
[CLI reference](docs/CLI_REFERENCE.md), and
[RC3 readiness review](docs/RC3_READINESS_REVIEW.md).

```bash
cargo +1.97.1 run -p zeno-fcis-cli --locked -- new /tmp/zeno-demo --template minimal
cargo +1.97.1 run -p zeno-fcis-cli --locked -- check /tmp/zeno-demo/project.zeno
cargo +1.97.1 run -p zeno-fcis-cli --locked -- generate \
  /tmp/zeno-demo/project.zeno --out /tmp/zeno-demo/generated
cargo +1.97.1 run -p zeno-fcis-spec --example mini_determinator --locked
python3 tools/qemu_demo.py run
```

The tutorials cover [language authoring](docs/tutorials/LANGUAGE.md),
[composition](docs/tutorials/COMPOSITION.md),
[temporal claims](docs/tutorials/TEMPORAL.md),
[formal tools](docs/tutorials/FORMAL_TOOLS.md),
[Mini Determinator replay](docs/tutorials/MINI_DETERMINATOR.md), and the
[CLI workflow](docs/tutorials/CLI.md).

The optional QEMU command builds a freestanding `no_std` kernel, boots it
through UEFI, and validates its guest serial result. It requires QEMU, OVMF,
ImageMagick, and the documented pinned nightly toolchain; it is not part of the
default library build.

`.zeno` source and every derived view are non-authoritative authoring input.
Only the lowered typed AST has canonical identity, and concrete machines still
bind through the existing authority-gated constructors.

The `full` feature is intended for workspace integration and exploration.
Reusable libraries should select only the features needed at their boundary.

### What synthesized code is guaranteed to do

**A synthesized program is guaranteed correct in a precise sense: it satisfies
its contract on every input the contract admits.** That result is a proof, not a
sample. It covers the selected program as the library's interpreter runs it.
Emitted Rust, Python, or JavaScript source carries the same guarantee once
`zeno-fcis synth verify` passes for that exact source and target. The proof is
about the contract as written, and it assumes that a small checker is correct.

#### Exhaustive verification, not correct by construction

A program can be guaranteed to meet its specification in two ways. ZenoFCIS
synthesis uses the second.

| | Correct by construction | Correct by exhaustive verification |
| --- | --- | --- |
| How correctness is shown | Every step that builds the program preserves correctness, so the result must be correct | The program is found by any method, then checked on every input the contract admits |
| What you trust | The construction rules and the tool that applies them | The checker: the `finite-i64/1` interpreter and its enumeration of inputs |
| If the builder has a bug | A wrong program can come out with no warning, unless the tool is itself verified | A wrong candidate fails the check and is never selected |
| What it can cover | Unbounded input domains | Finite domains small enough to enumerate |

Both are proofs. Checking every case of a finite domain is a proof by
exhaustion: the same kind of proof as showing that a statement holds for the
numbers 1 to 100 by checking each one. The two differ in where the trust sits
and in how many inputs they can cover, not in how strong the result is within
its domain. [Design record 0003](docs/adr/0003-epistemic-status.md) classifies
a selected program as Proved.

Exhaustive verification checks the final program, not the process that
produced it. The interpreter checks the selected program itself. Then
`zeno-fcis synth verify` replays the exact emitted source on every case in its
target runtime, so an error in code generation is caught too. Until that
command passes, the emitted source is unverified.

#### How a program is synthesized

1. A realizability check confirms that every admitted input has at least one
   acceptable output, or reports the input that has none.
2. A search enumerates the candidates of a reviewed sketch in a fixed canonical
   order.
3. A separate checker runs each candidate on every admitted input. Only a
   candidate that satisfies the contract everywhere is selected.
4. `zeno-fcis synth run` writes the program as Rust, Python, or JavaScript
   source and reports `runtime_conformance: not-run`. A separate command,
   `zeno-fcis synth verify`, replays that exact source in its target runtime on
   every admitted input, and compares each output with the interpreter and the
   contract.

The guarantee for a selected program rests on the checker, not the search, so
a heuristic or AI proposer could replace the enumerator without weakening it. A
"no solution" result is different. It is a proof only because the library's
search covers every candidate in the sketch, so it also trusts that
enumeration. A search whose budget does not cover every candidate is refused
before it starts, and that refusal never counts as evidence of no solution.

#### What is guaranteed, and what it depends on

For every input in the declared finite domain, the selected program's output
satisfies the contract. Emitted source has the same property once
`synth verify` passes for it. Inputs outside the domain are refused rather than
answered. Finite synthesis handles up to 65,536 admitted inputs, 4,096 possible
outputs, 16 fields on each side of a contract, 64-bit integers, and 1,000,000
candidate programs.

The guarantee depends on four things:

- **The contract must say what you mean.** The program is proven to match the
  contract, not your intent, and a wrong contract yields a program that is
  faithfully wrong. This is the largest risk, so the durable-counter template
  also checks decision examples that were reviewed against its README.
- **The checker must be correct.** The interpreter and its enumeration are
  small and tested, but they have not themselves been formally proven.
- **The code around the program needs its own check.** In the durable-counter
  template, `tests/conformance.rs` runs all 64 admitted inputs through the real
  application: admission, the authority, the adapter that turns outputs into
  decisions, and the law checker. It compares every decision, reason, state
  change, and notification with the model.
- **Emitted source needs its own replay, and the target runtimes are trusted.**
  `synth verify` binds its report to the exact source, the emitter, and the
  runtime's executable hash and version. It accepts only Rust 1.97.1, Python 3,
  and Node.js 22. The repository's synthesis check runs it in all three for
  the durable-counter template, on all 64 inputs, for the
  inventory-reservation template, on all 432, and for the compliance-gateway
  template, on all 720. Production must run that same source on runtimes that
  behave the same way.

In short, a synthesized program is guaranteed to be correct with respect to its
contract, on every input it accepts, provided the checker is correct. Emitted
source shares that guarantee once `synth verify` has passed for it. That is far
stronger than testing, which samples inputs. It is not a promise that the
program does what you meant: the proof covers the contract you wrote.

Properties of a finite transition are checked the same way.
[System properties](docs/SYSTEM_PROPERTIES.md) are checked on every admitted
input, with a control that flags a property the declared domains already imply.

## Implemented workspace

The workspace now includes the complete package ladder:

- semantic values with default-bounded text and byte helper admission, immutable value and envelope witnesses for repeat canonical encoding, decisions with explicit immutable budget reports, ZCVE/1 canonical encoding, exact commitments, preconditioned patches, non-executable commit-evidence plans, durable outbox plans with strict bounded canonical decoding, receipts, and complete candidate bundles;
- assume-guarantee composition, deterministic-parallel conflict checking,
  backend-neutral complete static footprint claims, nominal footprint witnesses,
  witness-gated parallel authorization, untrusted runtime refinement reports,
  strict approved-provider reconstruction into nominal validated decisions,
  canonical finite-domain manifests, independently verified exhaustive
  coverage, content-addressed promotion reports, canonical evidence envelopes,
  and the first ZenoDEX profile;
- fixed-size executable domain machines with schema-admitted state, command,
  context, and port matrices; narrow per-machine interfaces; routes derived
  exactly from global composition wiring; deterministic merge-order execution;
  global reject rollback; and terminal committed-failure preservation;
- one authority-owned composed-domain program with closed root-to-context maps,
  nominal machine ownership, complete state projection, fail-closed internal
  routing, and exact catalogued effect/outbox projection;
- vetted RustCrypto and libcrux SHA-256 providers with known-answer and cross-provider parity evidence;
- closed schema validation, root and selected-type schema-bound envelope admission, generated exact-schema and exact-catalog reconstruction, typed root/command/context smart constructors, derived command/context commitments, schema-typed direct root-field reads, updates, and context observations, raw-path-free generated mutation and context-observation surfaces, disposition-typed reason application, catalog-typed effect/channel staging, private-inner generated transitions, deterministic Rust/Python adapters, negative codec vectors, cross-language replay, and content-addressed generation manifests;
- project-neutral profiles with stable reason/effect/channel/capability/event registries, explicit evolution modes, and exact content-addressed migration evidence;
- tool-neutral, profile-bound relational-law manifests for state invariants, conservation, mint/burn authority, debit/credit-to-effect equality, fees and rounding, authority/subject/recipient relations, rejection purity, and committed-failure effects, with retained proof evidence and fresh bounded per-invocation evaluation;
- nominal catalog authorization that owns the reviewed transition program and exact project-law engine, binds the reviewed initial root/source/configuration/evidence/deployment instance, creates a private-construction `CatalogAuthorizedGenesis` only after every genesis-applicable law is satisfied, admits external command/context/principal/replay invocations, pins a sealed known-answer-verified provider plus exact outbox-delivery-interpreter/deployment/resource bindings, and creates a private-construction `CatalogAuthorizedTransition` only after every applicable transition law is satisfied;
- a reusable callable/strict JSON-line mounted-runtime adapter for complete normalized decisions from any project profile;
- strict JSON-line runtime adapters that compare complete normalized decisions and retain mismatch records;
- a permanent exact-revision mount of the real ZenoDEX Python/Rust single-vault zUSD transitions, with a retained 17-case full-decision parity report;
- an explicit dual-root sparse authenticated-state reference with strict bounded proof/plan decoding, projector-bound profiles, context-verified membership/absence witnesses, expected-version publication, and full-rebuild equality checks;
- a candidate-bound authenticated authority that verifies exact retained projector evidence at setup, requires a project-specific per-transition projection law, reconstructs persisted plans locally, and exposes a production-facing port that accepts only nominal `CatalogAuthorizedAuthenticatedCommit` values;
- [language-neutral finite synthesis](docs/LANGUAGE_NEUTRAL_SYNTHESIS.md) with relational realizability checks, canonical hole search, Rust/Python/JavaScript emission, and separate exhaustive target conformance;
- [bounded completion and ordered preparation](docs/BOUNDED_COMPLETION.md) with finite exit-path search, independent rank checking, complete resource reservations, and resumable computation that grants no publication authority;
- crash-atomic policy-pinned SQLite schema v5 publication that creates a store only from nominal `CatalogAuthorizedGenesis`, reopens without caller-supplied initial state, strictly decodes and reauthorizes the complete persisted transition history, reconstructs exact authorization/bundle/receipt/replay/outbox row-set equality and current state, validates pending delivery against exact bundle membership, rejects schema v4 and earlier stores pending explicit migration, owns a policy-bound delivery-interpreter instance, never executes `CommitPlan` evidence, and retains crash-point and adversarial-corruption tests;
- persistent collections with reference, shared hash-map, and shared ordered-map implementations, logical-entry equality, property tests, and benchmarks;
- release assurance with static effect-boundary checks, exact dependency and CI-action pins, RustSec/license/source policy, deterministic source manifests, Miri, and fuzz harnesses.
- a frozen V1 product contract, human-readable BDD scenarios, a closed
  fail-closed ATDD registry, and optional deterministic Probity guardrails with
  a pinned Node/npm graph and hostile command corpus.

## Demonstrated ZenoDEX runtime mount

ZenoFCIS mounts the existing ZenoDEX single-vault zUSD functional core through
two thin JSON-line entry points: one calls the Python transition and one calls
the independent Rust transition. Both receive the same exact state, command,
context, and policy. ZenoFCIS normalization additionally binds the profile,
algorithm, schema, codec, precedence, and budget identities.

The retained v1 corpus executes 17 state-threaded cases. It currently produces
9 accepted transitions and 8 rejections with no Python/Rust divergence. For
each case, the mount compares the complete normalized decision:

```text
decision kind and reason
+ pre-state and post-state roots
+ candidate identity and CanonicalPatch
+ CommitPlan and OutboxPlan
+ receipt and CommitBundle
+ complete decision commitment
```

The permanent `mounted-zenodex` workflow checks out the exact pinned ZenoDEX
revision, builds its Rust runtime, runs both implementations, and byte-compares
the new report with the retained
[`test-data/zenodex/zusd-v1/report.json`](test-data/zenodex/zusd-v1/report.json).
The runner can also be invoked directly:

```bash
cargo +1.97.1 run -p zeno-fcis-adapter-zenodex \
  --bin mount-zenodex-zusd --locked -- \
  <pinned-zenodex-checkout> <zenodex-rust-binary> <output-directory>
```

This establishes bounded executable integration for the mounted profile. It
does not authorize production effects, cover multiple vaults or other ZenoDEX
lanes, or replace an audit or unbounded refinement proof.

Mounted `NormalizedDecision` values remain untrusted transport. A production
promotion path must reconstruct every receipt or complete bundle through
`ValidatedNormalizedDecision`, derive case identities from the exact
invocation, and use a verified `ExhaustiveDomainManifest` when claiming finite
domain completeness. See
[`docs/VALIDATED_REFINEMENT_AND_EXHAUSTIVE_COVERAGE.md`](docs/VALIDATED_REFINEMENT_AND_EXHAUSTIVE_COVERAGE.md).

The `zeno-fcis` umbrella keeps the semantic kernel small by default. Application
code should enable the smallest explicit feature set, for example:

```toml
[dependencies]
zeno-fcis = { version = "=1.1.0", default-features = false, features = ["composed-program"] }
```

The umbrella crate's default and `no_std` feature sets are project-neutral.
Enable `zenodex-profile` for the ZenoDEX profile exports or `mounted-zenodex`
for that profile plus its concrete mounted runtime.

Important optional features include `authority`, `domain-machines`, `composed-program`, `codegen`,
`evidence`, `mounted-runtime`, `zenodex-profile`, `mounted-zenodex`,
`authenticated-state`, `authenticated-authority`, `synthesis`, `sqlite-shell`, `collections`, and
`persistent-collections`.

## Architecture

The repository keeps computation and coordination separate:

```text
pure transition
    -> immutable, content-addressed candidate
    -> external invocation + catalog/provider/deployment validation
    -> complete project-law evaluation
    -> nominal CatalogAuthorizedTransition
    -> optional qualified projector + nominal CatalogAuthorizedAuthenticatedCommit
    -> policy-pinned atomic shell publication
    -> idempotent outbox delivery
```

Before that transition path can publish, the same authority must evaluate the
reviewed initial state under every genesis-applicable law and mint a nominal
`CatalogAuthorizedGenesis`. Creation consumes that witness exactly once;
reopening accepts no replacement initial state. See
[`docs/GENESIS_AUTHORIZATION.md`](docs/GENESIS_AUTHORIZATION.md).

Projects that publish an authenticated index should additionally use the
[`authenticated-authority` boundary](docs/AUTHENTICATED_AUTHORITY_BOUNDARY.md).
Raw sparse-tree plans are reference data. The production-facing authenticated
port accepts only an exact catalog-authorized candidate whose projector evidence
and per-transition projection relation have passed the setup-owned checks.

Persistent backends are sealed behind a pure logical-map interface. Updates return new structurally shared versions; equality and canonical bytes depend on logical entries only. Map-entry ordering bytes are derived from the semantic key, the explicit persistent-entry boundary rejects mismatched key bytes, and materialization exposes only fallible APIs.

Concrete runtimes, databases, and synthesis engines remain outside the semantic authority boundary. Their adapters propose or store data; pure validators decide whether that data is admissible. A structurally valid `CommitBundle` remains reference data and cannot enter the production SQLite commit port directly.

The [design and algorithm review](docs/DESIGN_IMPROVEMENTS.md) explains recent
performance changes, preserved acceptance rules, measurements and tradeoffs.

The `zeno-fcis-domain` layer makes global composition executable without
introducing hidden shared state. Every component receives only its fixed state
row, one command, one context, and fixed typed input ports. The complete route
matrix is derived from `CompositionSpec`; state and invocation matrices bind
the exact executable composition and cannot be replayed across another
same-shaped topology. See
[`docs/FIXED_STATE_DOMAIN_MACHINES.md`](docs/FIXED_STATE_DOMAIN_MACHINES.md).
The production bridge is documented in
[`docs/COMPOSED_DOMAIN_PROGRAM.md`](docs/COMPOSED_DOMAIN_PROGRAM.md).
Its aggregate-root projection paths are required to equal the exact state paths
declared by the corresponding machine interfaces; the bounded law and its
nonclaims are documented in
[`docs/COMPOSED_ROOT_PROJECTION_CONFORMANCE.md`](docs/COMPOSED_ROOT_PROJECTION_CONFORMANCE.md).

## Verification

The main local gate is:

```bash
python3 tools/check_assurance.py --self-test
python3 tools/atdd.py self-test
python3 tools/atdd.py check
cargo +1.97.1 fmt --all -- --check
cargo +1.97.1 clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo +1.97.1 test --workspace --all-features --locked
python3 tools/atdd.py run --all
```

See [release assurance](docs/RELEASE_ASSURANCE.md) for the full stable, `no_std`, Miri, fuzz, supply-chain, and source-manifest gates. Package-specific boundaries are documented in `docs/`.

Release packaging is fail closed and reviewable:

```bash
python3 tools/rc_package.py self-test
python3 tools/rc_package.py check
python3 tools/rc_package.py build --output /tmp/zeno-fcis-v1
```

The build retains all public `.crate` packages, rustdoc, source and diagnostic
binary archives, checksums, a CycloneDX SBOM, and provenance inputs. See the
[packaging reference](docs/PACKAGING.md),
[RC3 readiness review](docs/RC3_READINESS_REVIEW.md), and
[V1.1 release checklist](docs/V1_1_RELEASE_CHECKLIST.md).

## Assurance posture

Version `1.1.0` extends the stable Cargo API with checked finite exit plans and
bounded ordered preparation. See [V1.1 release notes](docs/V1_1_RELEASE_NOTES.md)
and the [V1.1 release checklist](docs/V1_1_RELEASE_CHECKLIST.md).
The release's evidence covers the declared library, packaging and integration
checks. The pinned ZenoDEX single-vault zUSD mount is bounded executable
refinement evidence. Production
value-moving promotion still requires each profile's independently reviewed
laws and evidence, qualified concrete storage and outbox-delivery interpreters,
deployment qualification, and an exact-head audit. This release does not claim
audit completion, project-specific economic correctness, side-channel
resistance, full ZenoDEX coverage, or approval of an external JMT, ESSO,
solver, prover, compiler, or LLM runtime.

The V1 execution model is explicit: `CommitPlan` is non-executable committed
evidence, while every external operation and every value movement uses the
durable replay-safe outbox. See the
[execution-model specification](docs/COMMIT_EVIDENCE_AND_OUTBOX_MODEL.md).

## License

Dual-licensed under Apache-2.0 or MIT, at your option.
