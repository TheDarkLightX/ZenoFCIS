# ZenoFCIS 2.1 and 2.2 plan

Status: in progress, 2026-10-05.

- **2.1:** F1 to F6, the independent F6 review's fixes and F6.1
  program-successor upgrades are implemented and committed on the local 2.1
  branch, each after a full ATDD run on its exact tree, as are the F4.1
  optimizer improvements. A study that built real apps with the 2.1 tools found
  defects on the documented path; F7 fixes them before release.
  Nothing here is released until review and exact-head CI pass.
- **2.2:** the owner approved this scope on 2026-10-05 and asked for the
  additions G7 to G14. G14 holds the features found by the app study.

This plan covers the complete factory feature set across two releases. Every
feature from the earlier roadmap has a place, a contract and a fixed acceptance
list. The earlier roadmap, [V2_1_FACTORY_PLAN.md](V2_1_FACTORY_PLAN.md), is kept
for its design notes and research. It is not a separate source of
requirements.

ZenoFCIS is being built as a **neurosymbolic software factory** for high-assurance
applications. Its foundation is a **formally verified functional core**.
Neural and symbolic tools propose; small checkers judge, under explicit
contracts.

- **2.1, the factory loop:** contract to application, meaning review, checked
  optimization by e-graph and by an adaptive neural loop, and adoption of a
  checked result into a live application.
- **2.2, reuse, evolution and wider values:** verified component families,
  data-migrating upgrades, 128-bit and compound values (including zUSD), the
  maintenance study, the external-tool reports, and more platforms.

## Prerequisite

The [smaller V2](V2_VERIFIED_CORE_PLAN.md) is committed, and its exact-head CI is
green.

## What keeps this plan finishable

The first V2 plan had too many requirements that never ended, not too many
features. Every feature here therefore follows six rules:

1. **One contract per feature,** with a command, a fixed acceptance list and an
   explicit "not in this feature" that points to where that work does live.
   Nothing is dropped silently.
2. **Changes to the verified core are their own feature (G3).** No other feature
   changes `canonical_v2/`, `evaluation/` or `execution_v2/`.
3. **Proposers may be untrusted.** Their output passes the existing core checks
   or a small judge that reuses the verified evaluator. Every judge is listed in
   the trusted base and has planted-defect tests.
4. **Working features first; mutation suites run in CI.** Under the owner's
   decision of 2026-10-04, the local pre-commit gate is proofs, native tests,
   ATDD and review. Mutation-control suites run in the cloud after push, and any
   control that fails there is an open defect.
5. **No new crates or dependencies in 2.1.** The evaluator's identity digest
   covers the root `Cargo.toml` and `Cargo.lock`. New code therefore goes into
   existing crates, mostly the CLI. A dependency needed later goes through G3
   or a reviewed identity change.
6. **Plans record decisions and acceptance.** Agent assignments, evidence custody
   and history belong in commits and review records.
7. **2.2 is built in stages, each with one integration target.** A stage is done
   when its target passes. Work outside the current stage starts only when it
   cannot delay that target (see "2.2 stages").

## Architecture

```text
reviewed contract (project.zeno + rules + decision examples)
  │
  ├─ F2 review ──► witnesses for the owner (advisory; grants no authority)
  │
  ├─ F1 generate ──► app ──► Authority::bind (verified core) ──► SQLite v2 shell
  │
  └─ program P0 from the contract (F7 export-program)
        │
        ├─ F4 e-graph optimizer ─────────┐   proposers (untrusted)
        ├─ F5 adaptive neural loop ──────┤   model candidates and e-graph strategies
        │    (MCP, provider, fake)       │
        │                                ▼
        │        F3 checker: exhaustive equivalence on the verified evaluator
        │                                │  receipt, or a replayed counterexample fed back
        │                                ▼
        └─ F6 adoption ──► rules updated ──► F6 upgrade of the live app
                            (explicit decision on the Step-usage observation)
```

- **Core (unchanged in 2.1):** admission, metered evaluation, decisions, laws and
  genesis, authority, publication.
- **Judges:**
  - the core itself;
  - F3's exhaustive checker;
  - F6's upgrade checks: the recorded Tier A premises (F6.1), or the core's
    genesis evaluation.
- **Proposers:**
  - F1's generator: a wrong output is refused by `Authority::bind` or by the
    owner's decision examples;
  - F2's search: a wrong output is only a misleading suggestion, which the owner
    reviews;
  - F4 and F5: a wrong candidate is refused by F3.
- **Functional core and shell inside each tool:** the logic is pure library code.
  Clocks, model calls, files and process limits stay in the CLI shell.

## Research basis

- **Dana Edwards, *Checked Semantic Equality Saturation for Finite Boolean
  Programs* (draft, 2026-10-03):**
  - Exact truth-table signatures, packed into a `u64` for up to six inputs,
    soundly merge equal classes.
  - They gave an 8.8% instruction reduction over a strong local simplifier, and
    4.4% beyond rewrite-only e-graphs, with no regressions against them.
  - Arithmetic and comparison nodes must not be optimized away, because eager
    evaluation traps on them.
  - ILP extraction caused the failed runs.
- **Functional-core research brief (2026-10-04):**
  - The withdrawal-queue kernel falls from 12 to 3 compute operations, an exact
    local minimum. The full graph falls from 69 to 60 nodes.
  - Step usage is observable: at budget 60 the original refuses and the candidate
    succeeds. Adoption therefore needs an explicit decision about the meter.
  - Qualify one candidate end to end before scaling up the optimizer.
- **Van der Cruysse et al., *Parallel and Customizable Equality Saturation*
  (CC '26):**
  - composable saturation strategies;
  - custom per-class annotations;
  - parallel saturation with deferred updates.

  F4's engine is shaped by the first two. Parallel saturation is a measured
  option, not a requirement.

## 2.1 features

### F1. Generated application integration

**Problem.** A new application currently needs a repository-only Python emitter
and glue code copied from a template.

**Contract.**
- Pure emitter modules in the CLI crate parse the rules and the elaborated
  project into a contract model.
- The model renders `src/v2_contract.rs` and builds the descriptor in memory, so
  the library encodes `v2/policy.zcve`.
- `zeno-fcis generate contract <app> [--check]` writes or checks those files.
- `zeno-fcis new <dir> --contract <contract-dir>` scaffolds a runnable app on the
  V2 route.

**Acceptance.**
- `--check` reproduces all 8 templates' `src/v2_contract.rs` and `v2/policy.zcve`
  byte for byte.
- The Python emitter is retired, with its test assertions migrated.
- A new app made only from a contract directory passes genesis, commit, replay,
  delivery and its decision examples, with no callbacks.

**Not in F1:** non-SQLite shells.

### F2. Contract review with distinguishing examples

**Problem.** The core proves that the code follows the contract. Nothing checks
that the contract says what the owner means.

**Contract.** `zeno-fcis contract review <app>` writes an advisory review packet:

1. **Inputs:** every input when the domain has at most 2^20 tuples; otherwise a
   deterministic boundary set.
2. **Decision table:** each input run through the library route.
3. **Rule mutants** from a fixed catalog applied to the F1 rules model:
   comparison flips, constants moved by 1, a dropped guard conjunct, swapped
   adjacent branches, a changed reason, a dropped effect or outbox entry.
4. **Witnesses:** the first distinguishing input for each mutant, written as a
   proposed decision example.
5. **Remaining mutants:** each classified as equivalent over the full domain, or
   as not distinguished within the boundary set.

**Acceptance.**
- It runs deterministically on all 8 templates, and every mutant is
  distinguished or classified.
- The account-lockout packet includes witnesses at the lock threshold and at
  `now + 900`.
- Its decision table agrees with every existing reviewed decision example.

**Not in F2:** applying suggestions automatically. Solver-guided search for
domains too large to enumerate is part of G1.

### F3. Checked program transform

**Problem.** Candidate programs need an independent acceptance path that
produces evidence.

**Contract.** `zeno-fcis transform check|replay` (pure module in the CLI crate):

- **Admission:** both programs pass the existing codec, with the same input ABI
  and the same outputs.
- **Comparison:** every input of the complete declared domain, run through the
  verified metered evaluator under eager semantics, at a full Step budget that
  no admitted program can exhaust.
  - `Equivalent` holds iff every tuple gives identical outputs or the identical
    failure class, and the declared Step limit L never binds: no tuple's true
    Step usage exceeds L in either program.
  - If some tuple's results differ, the result is the first `Counterexample`,
    as a replayable witness.
  - `Inconclusive` covers the domain cap (default 10^8 tuples), a binding Step
    limit, and the checker's own enumeration check.
- **Usage report:** Step usage never decides equivalence, but V2 seals it into
  publications, so it is always reported: each program's maximum, the number
  of tuples where the candidate uses more, and `usage_preserved`. A binding
  limit is reported with its counts and the smallest limit that never binds.
- **Receipt:** deterministic JSON binding both programs' hashes, the domain, L,
  counts, usage and the checker identity. `replay` must reproduce it byte for
  byte.

**Acceptance.**
- The withdrawal-queue kernel, controller and decision graph are `Equivalent`
  over 16, 384 and 1,296,000 inputs.
- The 32 benchmark pairs and 11 recorded witnesses are reproduced.
- At least six planted wrong candidates are refused.
- The verified core is byte-unchanged.

**Not in F3:** proposers (F4, F5).

### F4. E-graph optimizer

**Problem.** Smaller equivalent graphs exist, but finding them by hand does not
scale.

**Contract.** `zeno-fcis optimize --program P0 [--strategy S] [--budget ...]`:

- **Engine:** an in-house e-graph (union-find, hash-consing, rebuild) over the
  existing instruction set. It adds no dependency; `egg` is not in the lockfile.
- **Annotations, per class:**
  - an exact truth-table signature, for Boolean classes with at most six inputs.
    Classes with equal signatures are merged, which Edwards's Proposition 1
    shows is sound;
  - the node's Step cost;
  - a "may trap" flag.
- **Strategies:** composable phases from a closed set: Boolean rewrites, semantic
  merge, select simplification, constant folding, sharing. Each phase has bounded
  rounds. The default strategy is fixed and versioned.
- **Safety:**
  - no rule removes or reorders a "may trap" node;
  - arithmetic nodes are rewritten only by constant folding;
  - every result still goes through F3.
- **Extraction:** a deterministic tree/DAG cost search with no ILP solver. Cost is
  instruction count, then canonical byte length, then Step usage.
- **Output:** the best F3-accepted candidate with its receipt, or "no checked
  improvement". Termination bounds are printed.

**Acceptance.**
- The withdrawal kernel and controller reach at most 7 and 60 nodes, each
  F3-accepted.
- On the 16 Boolean benchmark pairs, every result is accepted and none is larger
  than its input.
- With semantic merge on, the aggregate result is no worse than with it off.
- A planted unsound rule yields only refused candidates.

**Not in F4:** parallel saturation, which is measured and optional.

### F4.1. Optimizer improvements from the literature review

**Contract.** The 2026-10-05 literature review measured techniques on this
optimizer. Four are adopted, each kept only for its measured benefit:
- exact support-local tables, with FRAIG-style merging: two classes merge only
  when their exact value and poison tables over their minimal input sets are
  identical;
- cut rewriting from precomputed minimum circuits for every 3-input Boolean
  function;
- a fixed, versioned strategy portfolio under deterministic work budgets, which
  is now the CLI default, so output is byte-identical on repeat;
- profile-aware proposals, which the loop passes through its engine seam.

Choice fusion of checked candidates is included as an option.

**Acceptance and result.** Every improved result is F3-accepted, and its receipt
replays. The table gives node counts measured on the integrated 2.1 tree with a
release build.

| Subject | Original | F4 | Target | F4.1 |
| --- | ---: | ---: | ---: | ---: |
| Withdrawal kernel | 16 | 7 | 7 | 7 |
| Retained controller | 69 | 46 | ≤ 40 | 35 |
| Withdrawal decision graph | 106 | 100 | ≤ 94 | 88 |
| Published 100-case corpus | 1,506 | 514 | ≤ 509 | 508 |

On the corpus, F4.1 wins 6 cases and ties 94, with no losses against F4.

This makes no optimality or convergence claim. The review's remaining
techniques are in G12.

### F5. Adaptive neural optimization loop

**Problem.** Neural proposers can find candidates that rewrite rules miss. The
loop has to stay safe, bounded and resumable no matter what the model does.

**Contract.** The loop in the
[neurosymbolic design](neurosymbolic-loop/DESIGN.md), implemented to its
[specifications](neurosymbolic-loop/specs/INDEX.md) 00–05:

- **Request:** frozen and canonical, with the original P0, domain, ABI,
  observations, cost objective, limits and disclosure policy.
- **Incumbent:** either the original or a checked replacement. It can only be
  built from a genuine F3 result.
- **Proposals:** complete candidates, or strategies in a closed DSL over F4's
  rule IDs, rounds and extractors. No model-supplied code, path or callback is
  ever executed.
- **Feedback:** counterexamples, replayed before they are shown to the model;
  admission reasons; cost results.
- **Selection:** strictly lexicographic, never worse than the original in any
  cost component, with ties keeping the incumbent.
- **Limits:**
  - the design's caps on attempts, model calls, tokens, check work and deadlines;
  - durable reservations made before each step, which are never refunded on
    failure;
  - resume re-admits and replays before reusing anything, and a stale resume
    returns no trusted incumbent.
- **Profiles:** `FunctionalBoolV1` (up to 6 Boolean inputs, up to 64 tuples), and
  `CheckedI64V1`, the full instruction set with eager traps, up to F3's domain
  cap.
- **Proposers:**
  - a deterministic local proposer;
  - a fake provider, used in tests;
  - the existing MCP tool and skill, so an agent session can drive the loop;
  - a hosted-model provider adapter, off by default, with spending disabled
    until the owner approves a provider, a disclosure policy and a budget.

**Acceptance.**
- **Invariant tests:**
  - every reachable incumbent equals P0 on the whole domain;
  - costs descend strictly on updates;
  - every failure path, including a panic or a killed worker, leaves the
    incumbent unchanged.
- **Resource tests:** caps are enforced; failure never refunds; resuming after a
  crash or with a stale ledger never yields a trusted incumbent.
- **Planted defects** are rejected:
  - a forged receipt;
  - a stale receipt;
  - a fake model success;
  - a removed cost guard;
  - a refund on failure;
  - a late response;
  - a ledger rollback.
- **End to end:** the MCP-driven loop (fake client), and the fake provider, each
  improve at least one benchmark case through F3.
- **Evaluation:** a preregistered run on the benchmark protocol's public and
  held-out sets reports every result, including failures. With a hosted model,
  this runs only after the owner approves it.

**Not in F5:** training or fine-tuning a model.

### F6. Adoption and checked contract upgrade (same state schema)

**Problem.** An accepted candidate is useless until a live application can adopt
it. The V2 shell refuses to open history under a different contract, so any rule
change today means a new database.

**Contract.**
- **Adoption:** `zeno-fcis contract adopt <app> --receipt R` replaces a decision
  graph in the F1 rules model. The adopter must record an explicit decision about
  Step usage:
  - `usage_preserved`, which F3 reports for the pair;
  - or a versioned contract change, which is the usual case for a smaller graph,
    because sealed usage changes.

  The contract is then regenerated, and the change is refused unless F3 replays
  the receipt.
- **Upgrade:** `V2SqliteShell::upgrade(new_authority)` and `zeno-fcis app upgrade`:
  - **Precondition:** identical state schemas.
  - **Check (two tiers):**
    - **Tier A (F6.1), program successor.** Admitted at any reachable state
      when five premises hold. The upgrade record stores them, and audits
      re-derive them:
      1. the canonical policies are identical except the decision program and
         its Step limit;
      2. generated law 991 pins every decision to the case table;
      3. the adoption receipt is F3 `Equivalent`;
      4. neither Step limit binds;
      5. no law reads Step usage.

      Under these premises both contracts reach the same states, so every law,
      proved inductive claim and decision example is preserved. Sealed Step
      usage still changes, so an adoption is still a versioned change.
    - **Other contracts.** The new contract's genesis laws must hold on the
      current state, using the core's genesis evaluation. Generated law 990
      makes this succeed only at the declared genesis state. G2's
      forward-simulation check lifts that limit.
  - **Record:** a chained upgrade record (old identity, new identity, state root,
    certificate). Each history segment replays under its own contract.
  - **Deliveries:** pending ones keep their certificate-bound IDs and commit order.
  - **Storage:** a new SQLite schema version, never upgraded silently.

**Acceptance.**
- The withdrawal-queue decision graph candidate (106 to 100 nodes) is adopted as
  a versioned contract change.
- A live withdrawal-queue database is upgraded and keeps committing, and both
  segments replay.
- These cases refuse, each with no write:
  - a different state schema;
  - a genesis-law violation;
  - a missing old contract;
  - a forged upgrade record;
  - an unreplayable receipt.
- A delivery pending before the upgrade is delivered once, with its original ID.
- This end-to-end candidate is the research brief's "one qualified candidate"
  completion condition.

**Not in F6:** schema-changing migration (G2).

### F7. Fixes from building real apps

**Problem.** On 2026-10-05 a Claude subagent built three applications with the
2.1 tools as a first-time user:
- an escrow with money and deadlines;
- a spend-approval matrix;
- the owner's `allowance.tau` rules.

It ran the whole optimize, adopt and upgrade journey. The contract route,
review, transform, optimize and adoption worked and were strict. The study found
these defects on the documented path:

1. **Generated apps do not build as their READMEs say.** They resolve the
   published 1.1.0 crates and fail with 12 compile errors. Only a repository test
   script knows the `[patch.crates-io]` binding.
2. **The generator accepts contracts that the library then refuses:**
   - an idempotency ordinal other than 0 passes generation, then the catalog
     refuses it with an opaque `Descriptor` error;
   - a case with two deliveries generates, but every decision on it is refused
     at run time, because the generated Effect limit is fixed at 1;
   - with committed-failure cases but no failure law, generation succeeds and
     framework law 908 refuses every committed failure at run time.
3. **Review and the generated app parse decision examples differently:**
   multi-section inputs, payload-only deliveries and indented comments.
4. **Review hides refusals.** A planted rule bug passed review with "findings
   0" and exit 0, although under it the Authority refuses 16 decisions as law
   violations.
5. **No command exports a contract's decision program,** so the optimize-to-adopt
   journey needed a script that parses generated Rust.
6. **Store errors print as Debug names,** such as `Identity` and
   `Upgrade(Genesis)`.

The study ran before F6.1, so its "upgrade only at genesis" finding is
re-tested here. Rule changes for live stores, symbolic checks for large domains
and an operational app CLI are 2.2 features (G14).

**Contract.**
- **Out-of-the-box builds.** `zeno-fcis new` writes the dependency binding to a
  ZenoFCIS source tree:
  - the `[patch.crates-io]` paths and the pinned `Cargo.lock`;
  - the source tree is the one named by `--source <path>`, else the tree the CLI
    was built from when it still exists. Otherwise `new` refuses and names
    `--source`;
  - the 2.1 archive ships the source archive, so an installed binary has a tree
    to bind to.

  Generated READMEs give exactly the steps the acceptance test runs.
- **Generator–library parity.** The generator renders what the rules declare:
  - the idempotency domain covers every ordinal the rules use;
  - the Effect limit is the largest delivery count of any case.

  It refuses a contract that has committed-failure cases but no failure law,
  naming a failure case. When a refusal still reaches the catalog check, the
  error names the rules entry being rendered, not only `Descriptor`.
- **One examples grammar.** Review and the generated app accept exactly the
  same decision-example files. Either they share one parser, or a differential
  test over a common corpus pins agreement.
- **Review reports refusals.** The summary counts refusals by class:
  - A law refusal on a pre-state that satisfies every declared state law is a
    finding, and review exits 1. The finding names the law when the refusal
    carries its ID. Otherwise it names the case and input, and naming the law
    moves to G3 if that needs a core change.
  - A refusal on a pre-state that breaks a declared state law is reported
    separately, as a refusal on a state the laws exclude.
  - Review still does not decide reachability: a law-consistent state may be
    unreachable.
- **Program export.** `zeno-fcis contract export-program <contract-dir> --out
  <file>` writes the decision program in the encoding that `optimize`,
  `transform` and `loop` read.
- **Readable store errors.** Each shell store error has a one-line message
  saying what happened and what to do. JSON output keeps the variant name.
- **Docs:**
  - a `policy.json` reference that covers law kinds, their required scopes and
    framework laws 908 and 909;
  - release builds recommended for review and transform;
  - the `bundles` count documented as excluding genesis;
  - `generate contract` help listing `v2/schema.zcve` as an output;
  - `contract adopt` documented as re-rendering `policy.json`.

**Acceptance.**
- In a clean directory outside the repository, following each generated README
  builds the app and passes `cargo test --offline`. This runs in ATDD and in the
  2.1 archive journey.
- The study's original escrow design generates and runs: one payout case with
  two deliveries, using ordinals 0 and 1.
- A contract with failure cases but no failure law is refused, and the message
  names a failure case.
- A corpus that includes the study's three example-file cases parses identically
  in review and in the generated app.
- Review reports the study's planted spend-approval bug (the CFO check moved
  from tier 1 to tier 2) as a finding and exits 1. The unchanged contract still
  reviews with no findings.
- The study's spend-approval app runs export-program, optimize, `contract
  adopt` and app upgrade with no other scripts:
  - under F6.1, a version 1 store with committed history upgrades to the
    adopted version and keeps committing;
  - both segments replay.
- Each store error variant has a message test.

**Not in F7:** rule changes for live stores (G14.1), symbolic checks for large
domains (G14.2), an operational app CLI (G14.3), laws over deliveries (G14.5)
and larger program limits (G3).

### 2.1 release

- **One platform:** a Linux x86-64 CLI archive built with `tools/rc_package.py`,
  plus checksums, licenses and the source archive. The installed binary runs an
  F1 → F3 → F6 journey, starting from a generated README (F7).
- **Version:** the release moves the workspace version from 1.1.0 to 2.1.0; the
  app study saw `--version` print 1.1.0. Crate manifests and `Cargo.lock` are
  inside the evaluator identity. This one reviewed change therefore also
  regenerates the evaluator identity, every fixture receipt (`contract
  refresh-receipts`) and the gate profiles.
- **Other platforms:** G6.

## 2.2 features

### 2.2 stages

2.2 holds nineteen feature contracts, and its main risk is breadth. By the
owner's decision of 2026-10-05 it is built in three stages. 2.2 starts after
the 2.1 review repairs land.

**Stage 1: one operational journey.** The target is one application taken
through its whole life from the generated command line alone:

1. **create** an app from the spend-approval contract (F1);
2. **submit** commands: a request is created, approved and executed (G14.3);
3. **deliver** its effects: the payment reaches an idempotent test receiver
   exactly once, through the typed delivery API and the relay (G13, G9);
4. **change** its contract: a rule change, shown first as a plain-language diff
   and then admitted on the live database (G8, G14.1);
5. **migrate** its database: a state layout change with a declared migration,
   admitted by forward simulation over every observation G2 names (G2);
6. **replay** its history: a full audit replays all three segments, each under
   its own contract, with every delivery and its original ID intact.

- **Features:** G8, G13, G14.3, G9, G14.1 and G2, and nothing else.
- **Acceptance:** one ATDD scenario runs the six steps in order, and each
  feature's own acceptance list passes. Each refusal on the way, such as a
  mismatched upgrade path or an invalid migration, writes nothing.
- **Why spend-approval:** its domain is small enough to check a migration on
  every input. Apps with money or time ranges are too large for that, so a
  migration of such an app is not shown in stage 1. It would rest on solver
  evidence, which is attested and not proved (G14.2).

**Stage 2: stronger checking and reuse.** G11 comes first, then G10, G1, G7 and
G14.4. G14.2 was built alongside stage 1's first features because it shares no
code with them; it is integrated when it passes review and is not part of the
journey.

**Stage 3: the rest.** G3 (after the 2.1 release), G14.5, G12, G4, G5, G6 and
G14.6. G12, G4, G6 and G14.6 can slip to 2.3 without blocking anything else.

### G1. Reusable verified component families

Parameterized building blocks, for example:

- a reservation pool with capacity C and per-request maximum Q;
- a rate limiter with window W and limit N;
- an approval queue with quorum K.

Each family is a parameterized F1 rules file with laws. Its evidence must cover
every parameter value in the declared range and establish that:

- every applicable law holds on every decision, so the component never hits a
  law refusal;
- the family's conservation properties hold.

Instantiating a family yields an F1 app that carries the proof reference.

The current public Lean exporter returns `UnsupportedMode`; its historical
implementation is test-only. A qualified family proof route must therefore be
implemented or supplied by source-bound component theorems, not inferred from
the retained `KernelChecked` status. SMT-only evidence remains Attested. The
[V2.3 roadmap](V2_3_PLAN.md) expands the G1 seeds without moving this requirement.

**Acceptance.**
- Three families, each checked over its whole parameter range and reported with
  its true evidence status (Attested for a solver result, KernelChecked for Lean,
  per ADR 0003).
- Every instance passes F2 review and its generated tests.
- A planted law-violating family is refused.

### G2. Data-migrating upgrades

F6 extended to state schema changes. A declarative migration `m` maps the old
state schema to the new one: field renames, additions with defaults, and splits
by a closed expression set. G2 keeps behaviour: a rule change that alters
decisions is not a forward simulation, and is G14.1.

**Admission is by forward simulation,** not by the new contract's genesis laws.
Generated law 990 pins genesis to the declared state, so a genesis-law check
could migrate only stores still at genesis; this is the same issue as F6's
H1. The check:

- `m` maps the old genesis state to the new genesis state;
- for every admitted old input, the new Authority on `m(state)` gives the same
  observations. The check compares every one of them, and the upgrade record
  names them:
  - the decision class and the reason, for refusals as well as commits;
  - the successor state, which must equal `m(old successor)`;
  - every delivery, in order: channel, destination and payload. A payout must
    be the same payout. When the migration changes a payload's schema it
    declares the payload mapping, and deliveries are compared through it.

  Matching successor states alone admits nothing. A migration that does not
  preserve one of these observations is a behaviour change, which is G14.1.

Every state reachable under the old contract then maps to one reachable under
the new, so the migrated store satisfies every new law and proved invariant at
any point in its history, and from a mapped state the new contract makes the
same decisions and sends the same deliveries. The guarantee covers the
observations listed above and no others: sealed identities, certificates,
delivery IDs issued after the upgrade and Step usage may differ.
The check is exhaustive over the F2/F3 domain;
boundary-set runs are advisory only. F6.1's Tier A program-successor upgrade is
the identity case (`m` is the identity) and shares this path.

Two further rules:

- **Rename tier:** names are part of the schema bytes, so a pure rename fails
  F6's same-schema check today. Admit it as an exact change: the new contract
  must equal the old one with the names substituted. The state is re-framed,
  and the upgrade is admitted at any state.
- **Routes:** migrations are built only between consecutive versions, or a
  shortcut must agree with the composed route. Samples use states whose
  carried-over fields hold non-default values and compare every preserved
  observation, not only successor states.

(Source of the forward-simulation framing: the 2026-10-05 assessment of
Nakahata's Algebraic Architecture Theory preprint, arXiv:2609.27638. That
paper's own machinery does not apply directly, because its morphisms preserve
every law by definition.)

Old segments stay replayable. **Acceptance:** a real template migrates live with
history and pending deliveries intact, and each kind of invalid migration
refuses with no write.

### G3. Wider values: 128-bit integers, compound values and zUSD

This is the only feature that changes the verified core.

- **Design choice made first:** widen the existing interpreter's scalar domain
  to i128/u128, or add a second interpreter. Widening is the first option to cost.
- **Compound values:** either flattened at admission or executed. This is costed
  in the same step.
- **Acceptance:**
  - the retained 12-type compound fixture and the full-width zUSD profile (12
    commands, 46 reasons, the U128 domains and the 2^63 deposit) run through the
    mandatory route;
  - the whole proof, all gates and identities are regenerated;
  - no existing domain shrinks.
- **Larger rule bases (from the app study).** The study hit `6 outputs and 294
  program nodes exceed 16 and 256` at about 60 cases. The limits are 256
  nodes, 16 outputs and 32 inputs. G3 raises them or compiles one program per
  command variant, with the same acceptance conditions.

### G4. Agent maintenance study

A preregistered comparative pilot of agents maintaining applications, with and
without the factory tools. It uses hidden acceptance tests and equal budgets,
and publishes every failure and its uncertainty. The deliverable is a published
report; it makes no product claim before the results.

**Owner decision, 2026-10-05:** the agents are Claude subagents driven from the
lead session, so no hosted model provider is needed. A first qualitative pass
ran on 2026-10-05: one subagent built three apps with the 2.1 tools, including
one from the owner's Tau spec. Its defects became F7 and its features G14. It
was one agent's unreviewed session, so it is not the G4 study.

### G5. External-tool reports

Pinned compatibility and assurance-boundary reports on `verus-spec-check` and
Fiat-Crypto. Each report states whether to adopt the tool, its license, and its
printing and compiler boundaries. Fiat-Crypto applies only to modular
cryptographic arithmetic, not financial accounting. Adopting either tool is a
separate decision.

### G7. Examples-first contract authoring

**Problem.** Owners think in concrete decisions ("this request should be
refused"), not in rules files. Today a rules file is written by hand or by an
AI, and the review tool (F2) only reports on it afterwards.

**Contract.** `zeno-fcis contract draft` plus MCP tools, so an agent session can
drive it. The rounds:

1. **Owner input:** a plain-language description and a few decision examples.
2. **Proposal:** a proposer (an agent through MCP, or the owner) supplies a
   complete rules file.
3. **Build and review:** the generator builds it, and the library binds it
   (F1). Review (F2) then produces distinguishing inputs and decision tables.
4. **Questions:** the owner is asked to label each distinguishing input with
   the expected decision. Labels become decision examples.
5. **Repeat** until every distinguishing input has an owner label and the
   contract agrees with all of them, or the budget runs out.

Proposals are untrusted; the generator, the library binding and the owner's
labels are the judges. The transcript records every proposal and every label
as provenance.

**Acceptance.**
- Three templates are re-derived from their plain-language descriptions.
  Each resulting contract agrees with every reviewed example, and its
  generated app passes its tests.
- A planted wrong proposal is caught by an owner example.
- Nothing is accepted without the owner's labels.
- The hosted model stays off. Agents connect through MCP, as in F5.

**Not in G7:** training models, or adopting a contract without owner labels.

### G8. Contract change classifier and plain-language diff

**Problem.** An owner upgrading an application must know which kind of change
they are making, because each kind needs a different admission path.

**Contract.** `zeno-fcis contract diff OLD NEW` decides the change kind exactly,
by comparing canonical bytes and structure:

| Kind | What changed | Admission path |
| --- | --- | --- |
| Identical | nothing | none |
| Program successor | only the decision program and its Step limit | at any state; needs an F3 receipt (F6.1) |
| Rename | only names | at any state; exact change (G2) |
| Layout change | the state layout | forward-simulated migration (G2) |
| Rule change | laws or cases, with the layout unchanged | forward simulation when every old decision is preserved (G2); otherwise the behaviour-change upgrade (G14.1) |
| Unrelated | anything else | refused |

The classifier decides only the structural kind. It runs no semantic check, so
for a rule change it names both paths, and the path's own check (G2 or G14.1)
decides which one applies.

It prints a plain-language summary that says which paths exist today. Upgrade
commands refuse a path that does not match the classification.

**Acceptance.**
- Every adoption fixture and a set of planted changes are classified
  correctly.
- A mismatched path is refused with no write.
- The summary names every changed law, case, field, command, channel and
  reason.

### G9. Delivery relay for real systems

**Problem.** Deliveries such as payouts and notifications currently reach only
an in-memory destination. A live application must reach external systems.

**Contract.**
- **Relay protocol:** dependency-free, at the shell's edge. The shell exports
  pending deliveries as a durable, ordered stream: delivery ID, channel,
  destination and payload. It accepts acknowledgments bound to the delivery ID
  and payload hash. A reference relay script performs the outbound calls
  (HTTP or a queue), sends each delivery ID as the idempotency key, and retries
  with backoff.
- **Strength, stated plainly:** transport is at-least-once, and every attempt
  carries the same identity. A receiver that honours idempotency keys sees each
  effect once.
- **Dependencies:** an in-process HTTP client would add a dependency, which
  changes `Cargo.lock` and so the evaluator identity. It needs a separate,
  reviewed identity decision.

**Acceptance.**
- Crash and restart at every step loses no delivery.
- Repeats carry the same ID.
- Forged or mismatched acknowledgments are refused.
- A receiver timeout leaves the delivery pending.
- The withdrawal-queue journey delivers each payout exactly once to an
  idempotent test receiver.

### G10. Checker throughput and work limits

**Problem.** An exhaustive check near the 10^8-tuple cap takes minutes on one
thread, and the checker has no work metering (F3 open item).

**Contract.**
- **Parallel enumeration** in the shell over disjoint ordinal ranges. Verdicts
  merge deterministically, and the first counterexample is the one with the
  smallest ordinal.
- **Work metering and deadlines.** Running out of work or time is
  `Inconclusive`, never a verdict.
- **Progress reporting.**
- Receipts stay byte-identical to the serial checker's.

**Acceptance.**
- Byte-identical verdicts and receipts against the serial checker on every
  benchmark.
- Measured speed-up reported.
- Cap and deadline behaviour tested.
- The neural loop uses the metering.

### G11. Verified checker core

**Problem.** The transform checker is a judge in the trusted base. It is tested
and mutation-checked, but not proved.

**Contract.** A Verus-verified unit for the checker's enumeration and comparison:

- enumeration visits every tuple of the declared domain exactly once, in
  order;
- `Equivalent` holds iff every observation is equal;
- a counterexample is the first differing ordinal.

It uses the already-verified evaluator's specification. As in the existing
gates, a native harness and a source digest tie the shipped checker to the
verified code. G10's parallel wrapper merges over this core.

The unit serves both equivalence judges: `transform check` in the CLI, and the
comparison the store shell runs itself before it admits a program-successor
upgrade (added by the 2.1 review repair). Two tested judges that must agree
become one proved core, so they cannot diverge.

**Acceptance.**
- The proof passes with the pinned toolchain.
- The shipped code is the verified code, in the CLI and in the store shell.
- All F3 tests and planted defects pass, and so do the shell's upgrade tests.
- CI runs the checker's mutation controls.

### G12. Optimizer depth and real-model evaluation

The literature review's remaining techniques, each landing only with measured
benefit:

- choice fusion of every loop-checked candidate, not only the incumbent (F4.1
  fuses only the incumbent; neutral on the inputs so far);
- a 4-input cut table;
- solver-free exact extraction, after first measuring the gap to greedy
  extraction;
- range-aware comparison rules;
- trap-set reduction.

Also:

- **A sealed held-out benchmark set,** generated with a fixed seed and its
  digest published before any tuning.
- **A preregistered real-model run** of the neural loop. It runs only after the
  owner approves a provider, what program data may be disclosed, and a budget.
- **The loop on real contracts (from the app study).** The loop refused a
  9-input app:
  - the boolean profile admits at most 6 inputs;
  - the i64 profile has a fixed ceiling of 1,000,000 work units per check;
  - the local proposer made no attempts on a 24-node program.

  G12 makes the i64 profile the default and scales the ceiling with the
  domain, under a hard cap. It also seeds proposals with review's equivalent
  mutants and with optimizer candidates. In the study, one equivalent mutant
  applied by hand let `optimize` reach 63 nodes, against 67 alone.

**Acceptance.**
- Each technique is kept only if it improves the corpus or withdrawal results
  with no regression.
- Held-out results are reported in full, failures included.
- No convergence or optimality claim.

### G13. Typed delivery API

Recommended by the typestate experience report (Heuer et al., FUNARCH '26,
doi:10.1145/3830438.3830958). The SQLite shell's delivery lifecycle becomes
consuming typed tokens: `Pending` to `Delivered` to acknowledged. Acknowledging
the wrong or an undelivered delivery then fails to compile. All eight templates
and generated applications migrate, and the old calls are removed afterwards.
G9's relay is built on this API.

**Acceptance.**
- Compile-fail fixtures cover each misuse.
- Every template journey and generated-application check passes.
- Recorded behaviour and storage are unchanged.

### Q1. Proportional qualification (deferred by owner decision)

When local mutation testing resumes, cut its cost without changing any control:
- one positive proof per source digest, shared by all gates;
- scoped `--verify-module` re-verification for mutants that change only one
  executable function's body;
- control selection by change.

Its acceptance is identical caught/survived results against whole-crate runs on
two gates. Until then, CI runs the full suite.

### G6. More release platforms

**Platforms (owner decision, 2026-10-05):** Ubuntu Linux (x86-64), which is the
2.1 release platform, plus macOS. Each added platform is qualified by an
installed-binary journey on that platform. A cross-compiled file alone does not
count. macOS needs a macOS machine or CI runner for the journey.

### G14. Features from building real apps

The owner asked that 2.2 also gain features found by actually building
applications with ZenoFCIS, including one that works with Tau, the IDNI
specification language. The study described under F7 built an escrow, a
spend-approval matrix and the owner's `allowance.tau` rules, and logged 31
friction points and 7 bugs. Its 2.1 defects are F7. These are its features,
each with a contract and acceptance list like every other item.

#### G14.1 Rule changes for live stores

**Problem.** A changed business rule cannot reach an existing store. The
study's example moved an escrow dispute window from 14 to 30 days. Today's
routes cannot carry it:
- F6 admits only identical behaviour (F6.1) or a store at genesis;
- G2's forward simulation also needs the new contract to repeat every old
  transition, which a behaviour change does not do.

**Contract.** `zeno-fcis contract evolve <app> --to <new-contract>` and a
behaviour-change tier in the shell's upgrade.
- **Admission:** at the store's current state, every state law of the new
  contract holds and every proved inductive claim of the new contract holds.
  Genesis exactness (law 990) applies only to new stores.
- **Owner review:** the G8 classifier labels the change a rule change. Its
  plain-language diff is recorded with the upgrade.
- **Record:** a chained upgrade record, as in F6. Each segment replays under its
  own contract.
- **Audit before upgrade:** the new build can audit an old store read-only
  before upgrading it.

**True strength.**
- After the upgrade, every law holds on every later state, because the
  Authority checks the laws at each commit.
- Each proved inductive claim holds from the upgrade on, because it holds at the
  upgrade state and every step preserves it.
- Facts that rest only on reachability from the new contract's genesis do not
  carry over, because the history was made under the old rules.

**Acceptance.**
- The study's dispute-window change upgrades a store with committed history,
  and the store keeps committing.
- A change whose new state law fails at the current state is refused, with no
  write.
- Both segments replay, and audit-before-upgrade works.

**Verified core:** if checking state laws at a non-genesis state needs a new
core entry point, that entry point ships with G3.

#### G14.2 Symbolic per-case checks

**Problem.** Exhaustive checks stop at about 10^8 input tuples. The study
measured about 352,000 tuples per second in a release build. Any app with
money or time amounts is therefore `domain-too-large`, so `optimize`,
`transform check` and the loop are inconclusive on the apps that matter most.
In the study, one SMT query per rule case:
- proved the escrow's conservation law over its whole domain of about 10^76
  tuples in 10–26 seconds;
- showed that law alone is not inductive;
- caught a planted rule bug.

**Contract.** `zeno-fcis contract check-symbolic <app>` and `transform check
--symbolic` use the pinned CVC5 and Z3 through the existing formal-tools
adapters:
- **Invariants:** per case, one query. It checks that the domain bounds, the
  case's path condition and an owner-supplied strengthening invariant imply
  each state law on the post-state.
- **Equivalence:** per pair of cases whose path conditions overlap, one query.
  It applies beyond the exhaustive cap.
- **Evidence:**
  - a counterexample is replayed through the verified evaluator, so a refutation
    is checked;
  - an UNSAT answer keeps today's classification (`ProposedUnsat` for CVC5,
    whose proof is not independently checked). A disagreement between the
    solvers is inconclusive;
  - every run includes planted controls that must fail;
  - a symbolic result never replaces the exhaustive check where that fits;
  - an answer's evidence class travels with it into every report, receipt and
    record that cites it. No admission path takes a `ProposedUnsat` answer as
    proof: `contract adopt`, store upgrades and loop replacements accept only
    an exhaustive receipt, until a solver certificate is independently
    checked.

**Acceptance.**
- The escrow conservation law, with its strengthening, holds on every case.
- The law alone is refuted, with a replayed counterexample.
- The study's planted spend-approval bug is refuted.
- A transform beyond the exhaustive cap is checked symbolically.
- A planted solver disagreement is reported as inconclusive.

**Not in G14.2:** Lean-checked solver certificates.

#### G14.3 Operational app CLI

**Problem.** A generated app's binary only replays its built-in examples on a
new database, or audits, upgrades and migrates it. The study wrote two tools,
104 and 73 lines, to submit one command and to compute one decision.

**Contract.**
- Generated apps gain `init`, `submit`, `decide` (a dry run), `state`,
  `history`, `pending`, `deliver` and `version`.
  - `deliver` uses G9's relay when one is configured, and a file destination
    otherwise.
  - `version` prints the contract version and identity.
- Generated package names include the contract version, so two versions never
  share a cached library build.

**Acceptance.**
- The study's spend-approval operation, including delivery of the pending
  payment, runs through the generated CLI alone.
- Each command has a test.
- Version 1 and version 2 builds in one target directory print different
  identities.

#### G14.4 Tau kit

The owner asked for something that works with Tau. Tau stays an optional,
independent second judge used through documented scripts. It is never in CI and
never a dependency, and its slow runs are always disclosed.

**Study results (stock Tau 0.7.0-alpha):**
- **Stream execution (`r always`):** not viable on these specs. There was no
  answer after 11–17 minutes, at up to 4.4 GB of memory.
- **Per-rule `valid` queries:** one to a few seconds each, and they caught
  every planted defect.
- **Decision checks** (each Authority decision substituted as constants):
  agreement on 2,263 allowance decisions and 353 spend-approval decisions.
- **Soundness:** `valid` printed T for a false formula, while printing F both
  for a weaker form of it and for its counterexample instance. Any `valid` query
  ending in a period prints T. Every T must therefore be corroborated.

**Contract.**
- `zeno-fcis contract export --tau <app> --out <dir>` writes three things:
  per-rule `valid` queries, decision-check queries for stored or reviewed
  decisions, and a mapping from contract names to Tau names.
- A documented runner script:
  - runs at most 4 processes, with timeouts;
  - adds planted controls that must print F;
  - refuses queries with a trailing period;
  - corroborates every T with an SMT solver;
  - reports every disagreement and never resolves one silently.
- `zeno-fcis contract import --tau <spec>` translates a finite ladder spec, such
  as `allowance.tau`, into a contract directory. The output states any scaled
  widths, and the kit checks the translation.

**Acceptance.**
- `allowance.tau` imports, and its review matches the study's.
- The kit reproduces the study's per-rule and decision checks.
- Every planted control fails.
- The study's false-T reproduction is reported as a disagreement.

#### G14.5 Laws over deliveries

**Problem.** A payment app needs laws such as "paid out equals released".
`check` accepts a law over outbox projections, but the contract route refuses it
("effect, outbox and event projections has no contract form").

**Contract.** Rules files can state laws over deliveries: counts and amounts per
channel. The generator renders them. Whether the core already evaluates these
projections is settled first; if it needs a change, this ships with G3.

**Acceptance.**
- An escrow law that payouts equal releases generates and holds on the study's
  journey.
- A planted double payout is refused.

#### G14.6 Authoring fixes

These come from the study's minor findings:
- field names scoped per record;
- a scope-aware `--require-substantive`;
- post-states that may omit unchanged fields;
- clear guidance when removing a rule leaves an unused reason;
- journey examples matched independently of file order;
- generated files kept out of the contract directory;
- an explained `unresolved_obligations` count;
- `contract adopt` keeping the `policy.json` layout.

**Acceptance:** one regression test per item.

## Definition of done

**For 2.1:**
1. F1 through F7 meet their acceptance lists.
2. ATDD has a scenario for each, and `python3 tools/atdd.py run --all` passes
   immediately before each commit.
3. Whole-core proof, regenerated gate profiles with positive checks, native,
   strict Clippy, no-std and no-default checks pass locally. Mutation suites run
   in CI after push (rule 4).
4. The docs (README, `ARCHITECTURE.md`, CLI reference, changelog) state each
   tool's true strength:
   - F2 is advisory;
   - F3 proves equivalence only over the declared domain and observations, under
     the named checker and evaluator assumptions;
   - F5 makes no convergence or optimality claim.
5. Independent review and full CI at the exact head both pass.
6. The Linux archive's install journey passes.

**For 2.2:** the same six conditions, applied to G1 through G14, stage by stage.
Stage 1 is done when its operational journey passes at an independently
reviewed head. The G3 core change also re-runs every proof gate and regenerates
all identities and certificates.

## Order and estimate

All estimates are in agent working days.

| Phase | Work | Depends on | Estimate |
| --- | --- | --- | --- |
| 2.1-A | F1, F3 (done) | V2 commit | 4–6 / 4–6 |
| 2.1-B | F6 and F2, F6 review fixes and F6.1 (done) | F1, F3 | 6–8 / 4–5 |
| 2.1-C | F4 and F4.1 from the literature review (done) | F3 | 6–8 |
| 2.1-D | F5 (done, with the F4 engine wired in) | F3, F4 | 10–15 |
| 2.1-F | F7, fixes from the app study | F1–F6 | 6–9 |
| 2.1-E | Release and docs | all | 2 |
| 2.2 | G1 / G2 / G3 / G4 / G5 / G6 | 2.1 | 10–15 / 7–10 / 15–25 / 7–10 / 3–5 / 3–5 |
| 2.2 additions | G7 / G8 / G9 / G10 / G11 / G12 / G13 | 2.1 | 8–12 / 3–5 / 6–8 / 4–6 / 10–15 / 6–10 / 3–4 |
| 2.2 from the app study | G14.1 / G14.2 / G14.3 / G14.4 / G14.5 / G14.6 | 2.1; G14.1 after G8; G14.4 after G14.2 | 6–9 / 5–8 / 4–6 / 5–7 / 3–6 / 2–3 |

- **2.1:** about 46–64 working days serially, or about 5–6 weeks with two or
  three builders working in parallel. F7 and the release remain.
- **2.2:** about 110–170 working days serially, including the additions and
  G14, or about 11–17 weeks in parallel.
- **Build order,** by the stages under "2.2 stages":
  1. Stage 1, the operational journey on spend-approval: G8 classifier and G13
     typed delivery API first, then G14.3 app CLI and G9 relay, then G14.1 rule
     changes (after G8) and G2 migrations, then the journey scenario.
  2. Stage 2: G11 verified checker first, then G10 throughput, G1 components,
     G7 authoring and G14.4 Tau kit (after G14.2, which is already built).
  3. Stage 3: G3 wider values (after the 2.1 release, with G14.5 if it needs
     the core), G12, G4, G5, G6, G14.6.
- **Can slip to 2.3 without blocking anything else:** G12, G4, G6 and G14.6.

G3 starts only after the 2.1 release, because it changes the verified core that
2.1's evidence depends on. G4 needs the 2.1 tools. Every other 2.2 feature may
start once its 2.1 dependency is committed.
