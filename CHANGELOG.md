# Changelog

All notable Rust API and packaging changes are recorded here. Canonical
protocol compatibility is governed separately by the identifiers and versions
embedded in ZenoFCIS values.

## Unreleased

- Add twelve decision examples to three application templates, for rules and
  rule precedences that a blind review of the first 72 examples against the
  READMEs found untested. `compliance-gateway` (now 28): a reinstatement by a
  non-reviewer of an account with no strikes, decided by rule 1, and three
  pairs of hold rules that apply together, each decided by priority.
  `withdrawal-queue` (now 26): a deposit that fills the vault to its
  capacity, and a request that breaks both the caller rule and the lane
  rule, decided by rule 1. `agent-treasury-guard` (now 30): `below_reserve`
  and `slippage_too_wide` on a sell, a price from the future, an unapproved
  model with a stale price, a slippage and a reserve breach together, and a
  failure whose refund is the held amount, not the command's amount field.
  Each template's conformance test runs them through the application. The
  review also noted that no withdrawal-queue example ticks in must-serve
  with nothing due; that vault is unreachable, and the conformance test's
  assertion that must-serve is reached only with a pending lane and no pause
  is now a named test, which also requires every tick from such a vault to
  pay, and the README says so. The agent-treasury-guard README's
  `SwapFailed` row now says the refund is the held `pending_amount`, as law
  508 states and the new example shows; it said "refunds `amount`", which
  reads as the command's field. An AI agent wrote the examples from the
  README rules; review of these twelve by the project's owner is pending,
  and the acceptance of the earlier examples stands.
- `IdempotentDestination`, `MemoryDestination`, and `DeliveryCollision` move,
  unchanged, from `zeno-fcis-shell-sqlite` to `zeno-fcis-shell`, the pure
  reference model, where the in-memory destination needs only `alloc`. The
  `std::error::Error` impl of `DeliveryCollision` is behind `zeno-fcis-shell`'s
  `std` feature, which `zeno-fcis-shell-sqlite` enables. `zeno-fcis-shell-sqlite`
  re-exports the three at their previous paths, so existing code and type
  identities are unchanged, which a test checks by passing a value built
  through the `zeno_fcis_shell_sqlite` path to a function that takes
  `zeno_fcis_shell::MemoryDestination`.
- Every application template puts its SQLite shell behind a `sqlite` feature,
  on by default: `zeno-fcis-shell-sqlite` is optional, and `Shell`, `create`,
  `invoke`, `journey` (with `prepare` in `prepared-counter`), and the
  demonstration binary, which declares `required-features`, need it. The core
  builds without it: the generated bindings, the program, the law checker, the
  profile, the delivery adapter, whose destination wraps the pure
  `MemoryDestination`, and `authority()`, so the `Authority` type is the same
  with and without the shell. `tools/check_generated_application.py` now also
  runs `cargo clippy --lib --no-default-features --target wasm32-unknown-unknown
  -- -D warnings` on every generated application, and the `adopter-acceptance`
  and `release-candidate` workflows install that target. Each template's
  `Cargo.toml`, `src/lib.rs`, and `src/delivery.rs` changed, so its source
  hash, and the program, checker, and policy hashes derived from it, changed;
  the semantic program hash of each `project.zeno` did not.
- A demo site under `site/`, a workspace of its own like `verification/`: a
  `cdylib` for `wasm32-unknown-unknown` that runs the account-lockout
  application's authority, program, and law checker over the library's
  in-memory reference shell, with the application written by `zeno-fcis new`
  at build time; a static page; a headless replay, `site/tests/replay.mjs`, of
  the template's decision examples and README demonstration; and
  `site/build.py`, which builds and tests everything and requires two builds
  of the module to be identical. Nothing is published.
- The `account-lockout` template declares the range of each of its integer
  types in `project.zeno` (`Attempts in 0..=2`, `UnixTime in
  0..=4102444800`, and `LockDeadline in 0..=4102445700`), and its `build.rs`
  no longer binds them. It adds claim 600, `lock_state_stays_consistent`:
  law 500's formula over the account before a decision, assuming laws 501
  and 502 on accepts and law 503 on committed failures. With the declared
  ranges, CVC5 1.3.3 answers `unsat` for the induction step, which is
  attested, not independently checked, and Z3 4.16.0 agrees. Without them,
  CVC5 finds a login near the top of the i128 range, after which
  `last_seen + 900` overflows and the invariant has no value, and Z3 a
  failed login from `failed_attempts` 3, from which law 503 constrains only
  `last_seen`; the schema admits neither.
  - `tests/claims.rs` checks that the claim restates law 500 exactly, that
    the manifest enforces each assumed law where the claim assumes it and
    refuses a law assumed outside its scope, that the law checker's observer
    reads every field the invariant reads and gives it its stated value on a
    grid of 129 admitted accounts across every boundary it compares, that
    the invariant holds on the exact genesis, and that the declared ranges
    are the generated schema's bounds and agree with the program's
    constants. `tests/conformance.rs` evaluates the invariant before and
    after each of the 295 committed decisions in its grid and the 12
    committed examples. `tests/laws.rs` adds decisions that only a formula
    refuses. The law checker observes the account through one
    `state_observations`, which the tests share.
  - Twelve planted defects, in the observer, the laws, the ranges, the
    program, and the checker, each failed a named test; the two weakened
    laws and the widened `Attempts` range were also refuted by `prove`.
  - Declaring the ranges and the claim changes the template's canonical
    project bytes and semantic program hash; the program, the schema's
    bounds, and the demonstration summary are unchanged.

- Declare the inclusive range of an `int` type in `project.zeno`:
  `type ID int name in MIN..=MAX;`.
  - Schema lowering takes a declared range as the type's `I128` bounds, so
    `build.rs` needs no binding for it. A binding that states other bounds is
    refused as `SchemaLoweringError::IncompatiblePrimitive`.
  - Inductive steps assume the declared range of every observed path of that
    type, and replay refuses a model outside it. Soundness rests on the
    authority checking every command, context, and state against its schema,
    whose bounds for that type are the declared range.
  - `declared_domain` and `StepHypotheses::domains` now return
    `DeclaredDomain`, either variant values or a range. Both were added since
    1.1.0 and have not been released.
  - A project that declares no range keeps its canonical bytes and semantic
    hash.

- The commit authority checks the command and context of every invocation,
  and the initial state at genesis, against its own catalog's schema.
  - Before, it compared only the schema hash each envelope recorded. An
    envelope's constructor accepts any `CommitmentHasher`, so a caller's own
    hasher could record the catalog's schema hash for a value validated
    against a looser schema, with an out-of-range integer or an undeclared
    variant.
  - Committed states were not affected: the pre-state and post-state of
    every transition were already validated against the catalog's schema.
    The templates' programs and law checkers also convert their inputs to
    typed values, which refuse such values.
  - A refused input is `AuthorityError::Mismatch(AuthorityField::Schema)`,
    and honest inputs are unaffected.
  - ADR 0003 now lists the check, and assumption (g) of the inductive-claims
    argument rests on the authority instead of on each application's
    conversions.

- Add the `agent-treasury-guard` application template for
  `zeno-fcis new --template`. An AI agent proposes swaps for a treasury; the
  proposal is only a command, and the decision is the guard. Whatever the
  agent proposes, the treasury commits at most its daily budget, keeps its
  reserve, stays within the slippage bound against the oracle price, and has
  at most one swap outstanding. Only an approved model identity, carried in
  the context, may propose. Accepted swaps leave through the outbox as
  requests shaped after ZenoDEX's `SwapIntent`, numbered by their proposal
  tick, and a settlement or failure naming another intent is rejected as
  stale. A failure refunds the swap and commits as a failure; it does not
  restore the budget.
  - A hand-written adapter computes the guard's facts with checked
    arithmetic; a core that `zeno-fcis synth` selected from a sketch with
    three holes, and checked on all 6,144 fact tuples, decides which rule
    applies first. The gate replays the core in Rust, Python, and JavaScript
    against an independent restatement of the rules.
  - `project.zeno` carries two inductive claims for `zeno-fcis prove`.
    Claim 600 states law 500's invariant over the treasury before a
    decision: the reserve, a non-negative base, a day's committed value
    between 0 and the budget, and the swap bookkeeping. It assumes laws 501
    and 502 on every commit, 503 to 507 on accepts, and 508 on committed
    failures, the scopes the manifest enforces them on. Claim 601 states the
    budget's part from the clock, the day accounting, the proposal's debit
    and limits, and the refund. CVC5 1.3.3 answers `unsat` for both steps
    (attested; `prove` exits 2), and Z3 4.16.0 answers `unsat` too, recorded
    as `blocked: UnsupportedEvidence`. Stating claim 600 found six
    transitions the draft's formulas admitted and the README's rules
    forbid: a buy of 0, a buy into the reserve, a proposal over the budget,
    a sell of more base than held, a sell at a negative price, and a
    settlement from a negative held minimum. Laws 505 and 507 now state the
    positive amount, the floors, the budget, and the positive price, and
    law 500 the non-negative held minimum. `tests/claims.rs` checks that
    claim 600 restates law 500, that the law checker's own observer reads
    every field the invariants read and gives each its stated value on all
    105,840 admitted treasuries at a tick, that both hold on the exact
    genesis, and that the manifest enforces each assumed law where the
    claim assumes it.
  - Every law formula is substantive with resolved paths. The template ships
    30 decision examples, a conformance test that decides 25,094 inputs over
    a scaled finite domain and evaluates both invariants before and after
    each of its 4,596 committed decisions, including a search of every state
    one day can reach that shows no sequence of proposals commits more than
    the budget, direct law-checker tests with one decision per law that
    breaks only that law and the six counterexamples each refused by the
    law that now states its rule, a lifecycle test, a determinism probe over
    864 inputs, and clean purity results. The gate, the release packager,
    and the ATDD scenarios cover it with the other examples. The first 24
    decision examples were reviewed and accepted by the project's owner on
    2026-09-24; the six added since await review.

- Add the `withdrawal-queue` application template: a vault with two
  withdrawal lanes whose keeper tick runs a controller step that
  `zeno-fcis synth` selected from a sketch of a fair policy, checking every
  hole assignment against the tick rules on all 384 inputs. The table that
  step defines ships with the vault's contract in OrbitSynthesis's canonical
  format, with the contract's SHA-256 pin. OrbitSynthesis's checker accepted
  it for every sequence of requests and alarms, with no fairness assumption:
  an alarm can delay a withdrawal but never freeze it, and a due lane is
  paid within 8 ticks. Two rejected strategies ship with it, a fixed
  priority and one that defers to alarms in must-serve; the checker rejects
  each with a counterexample cycle.
  - The law checker's refinement check ties each concrete tick to the model:
    the contract's transition table must allow the output the decision made
    and give the plant state it left, and the certified strategy's row must
    select that output and next memory. The checker reads neither the
    synthesized step nor, for the contract check, any strategy row.
  - `tests/controller.rs` re-checks the shipped JSON with an independent
    Rust implementation of the decision procedure, requires the synthesized
    step to equal the certified table on every input, the Rust tables to
    equal the JSON, and both to equal a restatement of the README's rules,
    and computes the 8-tick response bound. `tests/conformance.rs` explores
    420 reachable vaults and 3,360 decisions against a reference model
    written in words, and checks the bound on the running application.
  - Two inductive claims for `zeno-fcis prove` state that the action laws
    alone keep the vault solvent and within capacity, law 500's invariant,
    and the pause within the controller's range, for every integer. Stating
    them showed where the formulas under-described the tick, so the laws now
    state the deposit's capacity, positive amounts, an amount leaving a lane
    exactly when the lane is emptied, and when must-serve is set. CVC5
    attests both steps; `tests/claims.rs` checks the observer, the exact
    genesis, and the law manifest.
  - `tools/check_generated_application.py` replays the synthesis in Rust,
    Python, and JavaScript against an independent oracle, and runs
    OrbitSynthesis's checker on the shipped files when `ORBIT_SYNTHESIS_ROOT`
    names a checkout, otherwise reporting that the check did not run.

- Add the `compliance-gateway` application template for
  `zeno-fcis new --template`: an expert system's transfer-screening rule
  base, `rules.txt`, becomes the synthesis contract, and `zeno-fcis synth`
  selects a decision program that is checked against it on all 720 inputs.
  Every decision names the rule that fired: a blocked transfer is a committed
  failure under that rule's reason, with a strike on record and an alert
  through the outbox; a held transfer queues a review ticket naming the rule;
  three strikes freeze the account until a reviewer reinstates it. The rule
  base is checked over all 720 inputs for consistency (no two rules of one
  priority match the same transfer), totality, and liveness (every rule
  decides some transfer); a rule base that fails a check fails `cargo build`
  and is refused again when the authority is built, so the application never
  decides under it. The law checker evaluates the rule base itself against
  every decision, independently of the synthesized step. An inductive
  claim states that the strikes stay within their bounds under every change
  laws 501 to 503 admit, with each enumerated field bounded to its declared
  variants. CVC5 attests the induction step. `tests/claims.rs` checks the
  base case, the law checker's observer, and the law manifest, and the
  conformance test evaluates the invariant before and after every committed
  decision. `tools/check_synthesis.py`
  replays the synthesis in Rust, Python, and JavaScript against a separate
  Python evaluation of the same rule base. The template also carries the
  rule base as a Tau Language specification with a local check script; it
  needs IDNI's binary and is not part of any gate. The first 24 decision
  examples were reviewed and accepted by the project's owner on 2026-09-24;
  the four added since await review.

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
  program or law checker failed a named test. The decision examples were
  reviewed and accepted by the project's owner on 2026-09-24.
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
  alone, and the project's owner accepted them on 2026-09-23. Swapping two same-type adapter bindings fails these tests.

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
