# Changelog

All notable Rust API and packaging changes are recorded here. Canonical
protocol compatibility is governed separately by the identifiers and versions
embedded in ZenoFCIS values.

## Unreleased

- Give `zeno-fcis prove` a system model: inductive claims.
  `claim ID name BACKEND inductive assume [...] accept [...] failure [...] = INVARIANT;`
  states an invariant over `pre.` state paths and names the laws its induction
  step may assume, grouped by the committing decisions they are enforced on.
  - `export_inductive_smt` exports the step. Each assumed law, and the
    invariant before the step, is asserted as defined and true on its own, and
    the invariant after the step, rewritten from `pre.` to `post.` by
    `invariant_at`, as not defined-and-true. Laws assumed only on accepts or
    only on committed failures are guarded by a decision kind.
  - Results have the new scope `InductiveStepOverLaws`. Counterexamples are
    replayed: every assumed law and the invariant before the step must
    evaluate to true on the model.
  - Applications check the rest of the argument: `evaluate_invariant` on the
    exact genesis state, and `LawManifest::check_step_assumptions` against the
    law manifest. For the durable-counter template's
    `counters_never_negative`, CVC5 answers `unsat` (attested) from its action
    laws alone, and `tests/induction.rs` runs the application's checks
    through the law checker's own observer. Adding the claim changes that
    template's policy identity.
  - A pinned test requires CVC5 and Z3 to agree with exhaustive replay on
    eight inductive steps; it runs in the formal-tools workflow and in the
    gate evidence.
  - Each observed enumerated or bool value is asserted to be one of its
    declared values (`declared_domain`), which admission guarantees, and
    replay refuses a model outside them. Action laws written as
    `command == X -> ...` otherwise leave an undeclared command unconstrained.
    With this, CVC5 answers `unsat` (attested) for the compliance-gateway
    strikes invariant, from its own laws.
  - See `docs/INDUCTIVE_CLAIMS.md`, including its limits: the step cannot yet
    assume integer ranges, which live in `build.rs`, and there is no Lean
    export for it.
- Report a replayed counterexample at which a claim has no value as
  `undefined`, with the reason: an overflow, a division by zero, or an
  inexact exact division. `prove` used to report `ModelReplayFailed` whenever
  a claim's arithmetic could overflow somewhere in the i128 range, although
  the solver's model was right. Evaluation is strict, so the claim does not
  hold there. A guard such as `pre.x <= 3 -> pre.x + 1 <= 4` does not protect
  the arithmetic after it, and the reason is now visible.

- Add three application templates for `zeno-fcis new --template`:
  - `account-lockout`: failed logins committed as failures, time as a context
    input instead of a clock read, administrator unlocks, and security alerts;
  - `order-fulfillment`: a hand-written state machine that sends idempotent
    payment and shipping requests. A repeated or late callback is rejected
    because the order has moved on, and payment callbacks name the attempt
    they answer, so one about an older attempt is rejected as stale;
  - `inventory-reservation`: a decision core synthesized and verified on all
    432 inputs, commands with quantities, and a conservation law.

  Every law formula in their `project.zeno` files can constrain some
  transition and reads only declared fields:
  `check --require-substantive --require-resolved-paths` passes. Laws without
  a formula are registered in `profile.rs` instead of as placeholder formulas:
  the rejection law, which holds by construction, and inventory's law that no
  failure is committed, which its law checker enforces. Each template ships
  decision examples, a conformance test through the running application,
  direct law-checker tests, a lifecycle test, a determinism probe, and clean
  purity results. During development, every defect planted in a template's
  program or law checker failed a named test. The decision examples await
  review by the project's owner.
  - `tools/check_generated_application.py` and the release packager build and
    test each as an isolated package and compare its demonstration summary.
    `tools/check_synthesis.py` replays the inventory synthesis in Rust, Python,
    and JavaScript against an independent oracle.
  - `new` now chooses template files through exhaustive matches, and a test
    requires every application template to emit exactly the files in its
    directory.

- Refuse escaped library keys in the purity manifest check. Cargo decodes
  Unicode escapes in quoted keys, so an escaped `path` could select source
  outside `src/` while the check reported the unused default root as confined.
  Unsupported escaped keys now prevent confinement with a stated reason.

- Add determinism checks for hand-written decision code.
  - `CatalogCommitAuthority::execute_probed` executes one invocation 2 to 64
    times, each on its own copy of the admitted values. It returns the first
    decision only if every execution produced identical canonical bytes, and
    otherwise withholds it with `ProbeError::Diverged` or
    `ProbeError::FailedAfterDecision`.
  - `zeno-fcis purity <PATH>...` parses Rust source and reports clocks,
    environment reads, randomness, hash-map iteration, shared state, raw
    addresses, unsafe code, and other ambient effects, with `use` aliases and
    macro arguments resolved. A crate directory is also checked for
    confinement. What the check cannot read never leaves a result clean. An
    unreadable directory, a symbolic link to source, or a directory with no
    Rust source makes the result `unreadable`. Only a library-only package
    can be confined: a binary target, a manifest form the check does not
    recognize, `include!`, or a `#[path]` attribute prevents it.
  - The durable-counter template gains `tests/determinism.rs`, which probes
    all 64 admitted inputs and compares their digests across three
    changed-environment child processes. Planted controls show that the probe
    and the child processes catch changes the conformance tests miss.
  - docs/DETERMINISM.md explains what each check establishes.
  - The project's owner accepted the durable-counter decision examples on
    2026-09-23.

- Correct two claim boundaries found by an independent review of 1102d81.
  First, synthesis guarantees the selected program as the interpreter runs it.
  `synth run` reports emitted source as `runtime_conformance: not-run`, and
  emitted Rust, Python, or JavaScript carries the guarantee only once
  `synth verify` passes for that exact source and target. Second, the README
  and architecture guide now separate untrusted proposers from trusted
  components: the pinned Lean kernel and runtime, a solver's unrechecked
  `unsat` answer, the search's enumeration for a "no solution" result, and a
  project's law engine.

- Explain in the README how exhaustive verification differs from correctness
  by construction, and why both are proofs. The synthesis section now shows
  what each approach trusts, what a bug in the builder does, and what each can
  cover. It also separates the guarantee for a selected program, which does
  not trust the search, from a "no solution" result, which does. It names the
  full synthesis limits (65,536 inputs, 4,096 possible outputs, 16 fields on
  each side of a contract, 1,000,000 candidates) and the four things the
  guarantee depends on, including a checker that is tested but not formally
  proven.

- Explain in the README why ZenoFCIS separates deciding from acting, how
  untrusted proposers and small deterministic judges divide the work, what
  that gives developers, LLMs, and agents, and what synthesized code is
  guaranteed to do: it is correct by exhaustive verification against its
  contract, not correct by construction, within four stated limits. The
  architecture guide gains a matching section. Design record 0003 now says the
  authority runs the program itself; only re-authorization re-executes and
  compares.

- Check the time budget before the first process start as well, so an
  exhausted budget starts nothing, in the formal-tools adapter and the
  synthesis runner. The gate evidence recorder also compares the source with
  the start after every gate and stops at the first change; a change made and
  reverted between two checks is still not detected.

- Fix two regressions found by an independent review of 562926d. The busy
  executable retry in the formal-tools adapter and the synthesis runner no
  longer outlives the caller's time budget: no attempt starts after it ends,
  and the result is `Timeout` (a 1 ms budget took 259 ms before). The gate
  evidence recorder now checks the commit, tree, and tracked files after all
  collection, immediately before writing, and new tests show that a late
  change writes nothing. The durable-counter examples' header now states that
  their author had seen the model artifacts and that a second reviewer
  accepted them against the README alone.

- Add `tools/record_gate_evidence.py`, which runs the local gates for the
  committed revision and writes a revision-stamped JSON record: tool
  versions, each command's exit code and test counts, the pinned CVC5, Z3 and
  Lean checks when their executables are supplied, and an inventory of every
  ignored test and what runs it. The formal-tools workflow also runs the new
  pinned solver-route differential.

- Connect the durable-counter template's finite model to the executed
  application. The generated application's new `tests/conformance.rs` runs all
  64 admitted inputs through admission, the authority, the Rust adapter, and
  the law checker, and requires the decision, reason, new state, and
  notification to equal the synthesized program's output read through a
  declared table. It also checks schema admission against the finite input
  domain, genesis, reachability of every admitted state, and twelve examples
  drafted from the README (`tests/decision-examples.txt`). Their author had
  seen the model artifacts, a second reviewer checked them against the README
  alone, and owner review is pending. Swapping two same-type adapter bindings fails these tests.

- Retry a process start a bounded number of times, about 250 ms in total, when
  its executable is busy (`ETXTBSY`), in the formal-tools process adapter
  and the synthesis runner. A child forked by another thread while a private
  executable copy is written briefly inherits the write descriptor; this made
  two tests fail intermittently with "Text file busy". Every other error, and
  a file that stays busy, still fails closed.

- Correct overstated claims found by an independent review of 521b768. The
  zUSD record's outcome and reason-order findings are scoped, with the
  reviewer's guarded-deposit counterexample kept as a test, and design record
  0003 now states the scope, assumptions, and trusted base of every
  classification.

- Make the system-property checkers honor their stated contracts. The
  exhaustive checker decides totality on every admitted input before any
  property result. `system_verdict` refuses solver models with the wrong
  number of values or values outside the declared domains, and replays
  domain-only models, whose scripts now also request the proposed outputs
  (`SystemAnswer::Sat { input, output }`; `parse_system_answer` takes the
  output count). A bounded differential compares the exhaustive and solver
  routes on 80 cases, and eleven planted-defect controls each fail a named
  test. The three regressions come from an independent review of 521b768.

- Add kernel law harnesses in a separate `verification/` workspace. Four laws
  (budget charges, reason choice, canonical decoding, and patch overlap) each
  have one predicate, checked exhaustively over a small stated domain and at
  random under bolero 0.13.4. Planted bugs for each law are detected. bolero and
  its dependencies are pinned in `verification/Cargo.lock` and never enter the
  published crates' lockfile. A `kernel-laws` workflow runs the harnesses and
  `cargo deny` on their dependencies.

- Report law and claim paths that name no declared type or field. Add
  `resolve_path`, `law_paths`, `claim_paths` and `PathResolution` to
  `zeno-fcis-spec`. `zeno-fcis check` warns about each unresolved path, adds
  an `unresolved_paths` object to JSON output, and accepts
  `--require-resolved-paths`. Elaboration, canonical bytes and default exit
  codes are unchanged. The CLI tutorial's `check` transcript now shows the
  current JSON output and substance warnings.

- Close effect spellings that the static assurance check missed in semantic
  crates: standard I/O; grouped, glob and aliased `std` imports;
  `thread_local!`; atomics and cells written without generic arguments, such
  as `AtomicU64::new` and `RefCell::new`; and hash-ordered collections. The
  self-test now also requires that safe witnesses such as `WorkspaceCell` and
  `BTreeMap` pass. No semantic crate needed a change.

- Record what `.zeno` v1 and the `finite-i64/1` IR can state about the
  single-vault zUSD lane. A `.zeno` v1 attempt states the lane's 32 fields,
  46 reasons, invariants, conservation laws, and a guarded deposit law that
  matches four native outcomes. Characterization tests pin what v1 cannot
  state directly: the reason code a rejection carries, products beyond i128,
  literals beyond u64, a leading parenthesized scalar, and field paths the
  schema does not declare. The record scopes its reason-order findings to how
  a program records failures, after an independent review of 521b768.

- Add design records under `docs/adr/`: the meaning-first assurance review,
  the principles for the V2 assurance program, a five-level vocabulary for
  the epistemic status of evidence that classifies the V1.1 status and
  witness types, and a ledger of breaking changes deferred to V2.
  Documentation only.

- Reserve the `zeno-fcis` commitment-domain namespace. Add
  `is_reserved_domain_name`, `DomainPrefix::try_new_project` and
  `StateDomainBinding::try_new_project`, which reject names that would share a
  domain with library identities such as candidate IDs. The V1 constructors
  are unchanged. The zUSD mount computes its patch precondition through the
  shared `hash_precondition_value` helper, with byte-identical results.

- Check properties against an exact finite transition program.
  `check_system_property` enumerates every admitted input and runs a
  domain-only control that reports properties the output domains already
  imply. `export_system_smt` exports totality, property and domain-only
  obligations that include the transition relation. `system_verdict` accepts
  a solver model only after the interpreter reproduces it. Pinned CVC5 runs
  must agree with the exhaustive check on the durable-counter program.

- Classify every `.zeno` law and claim by substance: `constant-true`,
  `constant-false`, `ignores-transition` or `may-constrain-transition`.
  `zeno-fcis check` warns about formulas that cannot constrain any
  transition, adds a `substance` object to JSON output, and accepts
  `--require-substantive`. Classification is diagnostic: it changes no
  canonical bytes, program identity, existing field or default exit code.
- Add `ObligationScope` to exported formal obligations. `zeno-fcis prove`
  now states that current obligations contain no system model: accepted
  results hold for every bounded observation assignment, and counterexamples
  may be unreachable.

## 1.1.0 - 2026-09-13

- Add independently checked finite exit plans, bounded canonical plan import,
  structured completion CLI discovery and exact replay files for agents.
- Add owned ordered preparation with whole-operation resource reservations,
  chunk rollback and exact invocation/root/version checks before exposing output.
- Add the generated prepared-counter application with independent authorization,
  full publication-size admission, atomic state/outbox publication and recovery.
- Qualify both generated applications and an unchanged V1.0 consumer against actual
  crate archives; preserve the foundational protocol and wire-test source baseline.
- Reject unknown fields in Boolean synthesis domains, matching the existing closed
  JSON schema instead of silently ignoring malformed input.
- Retain Lean 4.30.0, Rust 1.97.1 and the existing external Cargo dependencies.

See [V1.1 release notes](docs/V1_1_RELEASE_NOTES.md) for the exact scope and limits.

## 1.0.0 - 2026-09-12

First stable Cargo API release. See [V1 release notes](docs/V1_RELEASE_NOTES.md)
for developer workflows, compatibility and assurance limits.

- Add concrete finite-contract synthesis with Rust, Python and JavaScript target
  conformance; keep proposal, checking, emission and application authority separate.
- Reduce redundant canonical decoding, composition hashing, finite evaluation,
  outbox encoding and parallel binding allocation with retained comparison evidence.
- Promote all 36 public package versions together without upgrading Lean or
  any external Rust dependency version. Patch the developer-tool Hono dependency
  to 4.13.5, the first version fixing three newly reported moderate advisories.
- Reject pending SQLite entries absent from the approved bundle, including
  row substitution between validation reads.
- Reject Python adapter primitive coercions and isolate synthesis runner source
  creation against pre-existing files and symlinks.
- Avoid a nested Cargo build lock deadlock in the QEMU capture runner and
  regenerate the real V1 framebuffer and serial transcript.

- Use `test-projects` and `test-data` folders and a private
  `zeno-fcis-generated-code-tests` crate. Rename `ReplayFixture` to
  `DecisionMismatchRecord`, with the previous public name retained as an alias.
- Replace the optional `imbl` ordered map with `OrderedMap` using the existing
  pinned `rpds` dependency, removing the unsound `bitmaps` dependency. Preserve
  thread-safe shared snapshots, canonical output, and the old type/feature names
  as compatibility aliases. Add branching-snapshot differential checks.
- Show dependency-check failures in CI output and retain both checker logs on
  failure, while keeping the existing advisory policy.
- Qualify the generated durable application using a CLI built from actual crate
  archives and internal dependencies resolved only from those archives; retain
  `PACKAGED-APPLICATION.json` in release evidence.
- Admit both packaged and generated-consumer dependency graphs against the
  reviewed external lock and exact local manifest/version allowlists, rejecting
  cached dependency drift and fallback to checkout sources.
- Preserve release staging paths containing spaces through encoded Cargo
  compiler arguments, remove inherited compiler overrides, and retain explicit
  argument evidence without allowing documentation warnings to be suppressed.
- Check renamed internal dependencies by actual package identity and recheck
  the clean source commit before retaining release manifests.
- Add parser-derived `describe [COMMAND...]` JSON for agents, including command
  effects, argument defaults/choices, and bounded failure recovery guidance.
- Add optional JSON generation and drift results, bounded artifact comparisons,
  and explicit artifact-read errors while preserving read-only checks.
- Implement the standard `core::error::Error` trait for eight foundational
  errors, enabling normal downstream `?` propagation without requiring `std`.
- Align workspace, generated application, external consumer, and fuzz metadata
  with the documented minimum Rust version, 1.97.1; retain existing dependencies.
- Return versioned JSON for CLI project-read failures when requested, including
  JSON graph input errors, and classify tool timeouts as execution failure (3).
- Bound CLI project-file reads before UTF-8 interpretation or parsing. Oversized
  files now return input-acquisition failure (3); in-memory parser diagnostics
  and accepted source programs are unchanged.
- Refresh quickstart commands and the complete generated-application journey,
  and report the actual error when the admitted-executable regression fails.
- Move owned footprint buffers into sealed transitions, avoiding deep clones
  while preserving normalization, reason precedence, resource bindings, and
  rejection behavior.
- Stream exact commitment preimages through the existing approved SHA-256
  providers, removing the framing buffer while preserving canonical hashes
  and a default allocating path for existing custom hashers.
- Repair overflowing persistent-map benchmark fixtures, compare all existing
  backends, and add allocating-versus-streaming commitment benchmarks with
  permanent fixture smoke checks.
- Add checked `.zeno` record/sum lowering with explicit scalar bounds and
  precise rejection of incompatible schema bindings.
- Add the development `durable-counter` CLI template and an isolated consumer
  gate covering runtime laws, nominal authorization, SQLite restart, exact
  replay, and idempotent notification delivery.
- Add generated `begin_bound_transition` for complete shell-owned invocation
  bindings and support empty generated reason, effect, and channel enums.
- Fix nested Lean temporal variable capture and add a relational/temporal
  translation corpus. Lean remains pinned at 4.30.0; the corpus can reuse an
  existing executable without installing or copying its runtime.

### Added

- a deterministic, prompt-minimized Exploitability-Potential Index scanner with
  decomposable hotspot scores, category-specific reviewer cards, candidate
  multi-stage review routes, hostile self-tests, exact baseline drift
  detection, and a read-only CI gate;
- an evidence-first LLM security playbook, dated primary-source standards
  snapshot, exploit-chain proof obligations, Chain Feasibility Index, and a
  machine-readable review-report schema;
- a closed ATDD scenario proving that repository text remains inert while
  hotspot ranking and the exact source inventory stay reproducible.

### Changed

- the LLM cybersecurity prompt now guides bounded models from threat modeling
  through hotspot triage, isolated scanners, current advisory/tactic lookup,
  finding proof, zero-day chain analysis, mitigation, and adversarial re-review;
- release assurance now retains hotspot model/inventory identities, review
  scope, chains, and residual-risk decisions without treating a low score or
  clean scan as a security certificate.

## 1.0.0-rc.3 - 2026-07-30

Authoring and composition usability candidate before stable V1.

### Added

- versioned bounded `.zeno` parsing, canonical typed project ASTs, accumulated
  diagnostics, builders, relational logic, and explicit finite/unbounded
  temporal modes in the pure `zeno-fcis-spec` crate;
- deterministic CVC5 1.3.3, Z3 4.16.0, and Lean 4.30.0 exporters and
  fail-closed process adapters in `zeno-fcis-formal-tools`;
- the `zeno-fcis` CLI for templates, checking, generation/drift, graphs,
  explanations, formal tools, and backend diagnosis;
- the executable Mini Determinator private-workspace, get/put, return,
  canonical join, conflict, rollback, budget, and replay reference;
- twenty-six RC3 BDD/ATDD scenarios, tutorials, official formal-tool checksum
  workflow, and a 36-crate public package inventory.

### Compatibility and limits

- existing protocol IDs and canonical formats are unchanged;
- `.zeno` is non-authoritative authoring input and only the lowered AST has
  canonical identity;
- finite temporal checks, generated views, and tool agreement do not grant
  production authority;
- the Mini Determinator is a semantic model. Its isolated QEMU kernel is a demonstration and carries no production OS or hardware claim.

## 1.0.0-rc.2 - 2026-07-29

Second public release candidate for the reusable ZenoFCIS core library family.

### Changed

- Catalog format 3 requires every effect and channel to bind explicit reviewed
  `OperationSemantics`, including canonical asset-scoped value-flow sets.
- Verified project laws now derive non-waivable economic families and
  committing-decision coverage from the exact catalog. Custom value relations
  require one exact registered claim with independently retained evidence.
- `CommitPlan` is explicitly non-executable committed evidence. Every effect
  definition is constructor-fixed to `CommitEffectSemantics::EvidenceOnly`,
  project catalogs reject value-moving effects, and external value movement
  must use a durable value-classified outbox channel.
- Production authority now names and binds the concrete outbox-delivery
  interpreter through `BoundDeliveryInterpreter`.
- Release-candidate packaging now uses the version-neutral
  `tools/rc_package.py` entry point and current-version-derived artifact names.
- RC2 now freezes its reusable product surface explicitly and binds nine
  human-readable BDD scenarios to a closed executable ATDD registry.
- Optional deterministic coding-agent guardrails pin Node `22.23.1` and
  Probity `1.10.0`; hostile command and transcript mutations are permanent
  acceptance evidence and AI-judged TDD is not enabled.

### Release-candidate limits

- no arbitrary downstream project receives production authorization merely by
  using the library;
- the authenticated sparse tree and included SQLite deployment remain bounded
  reference surfaces unless separately qualified;
- concrete formal-tool adapters and project proof artifacts remain
  project-owned;
- signing, hosted provenance, crates.io publication, and independent exact-head
  review remain owner or external actions.

## 1.0.0-rc.1 - 2026-07-29

First public release candidate for the reusable ZenoFCIS core library family.

### Included

- immutable values, canonical ZCVE/1 encoding, deterministic budgets, and the
  `Accept | Reject | CommittedFailure` decision algebra;
- preconditioned canonical patches, closed effect and outbox plans, receipts,
  candidate sealing, and nominal production commit authorization;
- project profiles, schemas, catalogs, generated typed transitions, relational
  laws, and fixed-size composable domain machines;
- proof-carrying composition with complete static footprint evidence and
  deterministic-parallel authorization;
- tool-neutral evidence, refinement, synthesis, and checked-backend protocols;
- strict invocation-bound decision reconstruction, derived refinement cases,
  canonical exhaustive-domain manifests, independently verified coverage, and
  content-addressed promotion reports;
- reference authenticated state, persistent collections, SQLite publication,
  mounted-runtime adapters, secret handling, and side/covert-channel policy;
- 33 publishable crates, a diagnostic ZenoDEX mount binary, complete public API
  rustdoc pages, examples, checksums, package/source archives, CycloneDX SBOM
  generation, and provenance inputs. The reproducible offline rustdoc archive
  explicitly excludes Rustdoc `1.97.1`'s nondeterministic global-search index.
- candidate-derived outbox delivery identities shared byte-for-byte by the
  reference and SQLite shells, with SQLite schema v3 rejecting old
  authorization-derived identities pending explicit migration.
- policy-bound, law-verified nominal genesis authorization; one-time pure and
  SQLite shell creation; strict receipt, bundle, and authorization decoding;
  and SQLite schema v5 full-history reauthorization with exact
  authorization/bundle/receipt/replay/outbox set reconstruction. Schema v4 and
  earlier stores are rejected pending explicit migration.
- canonical sparse-proof and authenticated-plan decoding with explicit resource
  limits, exact round-trip admission, and non-authoritative decoded plan values;
- retained-evidence projector qualification, required per-transition projection
  relations, nominal candidate-bound authenticated commits, and a production-facing
  authenticated publication port.
- a checked V1 release runbook, declared code ownership, and read-only
  reproducible package assembly for immutable `v1.0.0-rc.*` tags.

### Release-candidate limits

- no arbitrary downstream project receives production authorization merely by
  using the library;
- the authenticated sparse tree remains a bounded reference implementation;
- SQLite qualification is limited to the documented reference deployment;
- concrete Lean, SMT, CVC5, Kani, Flux, or private ESSO adapters and proof
  artifacts remain project-owned;
- concurrent execution and effect interpretation remain external shell work;
- final independent exact-head review and release attestation remain required
  before `1.0.0`.
