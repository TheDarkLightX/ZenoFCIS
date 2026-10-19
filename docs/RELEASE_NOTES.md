# ZenoFCIS 2.5.0 release notes

Code is LLM-generated or synthesized;
acceptance depends on the stated check, domain and assumptions. The
[installation guide](INSTALLATION.md) describes how to verify published assets
once they are available.

## Arithmetic loops

**Tested and finite checked.** Arithmetic expressions now reach the normal
loop doors, exact full-domain transform checks, receipt replay and point
evaluation through the signed 128-bit route. Proposers still supply no
acceptance or publication authority. A finite check establishes agreement
only over its complete declared domain; a stopped check is inconclusive.

## Containment and CPU budgets

**Tested.** The portable POSIX containment route has explicit backend
selection alongside the systemd route. Supported profiles and prerequisites
are described in the [CLI reference](CLI_REFERENCE.md). Backend selection does
not silently substitute a weaker backend when the selected one is unavailable.
Linux runtime checks establish the observed containment behavior; the backend
name alone does not qualify another operating system or arbitrary executable.

Stage limits measure CPU time, with conservative FINISH uncertainty and a
separate wall backstop. POSIX requests below the counter's FINISH uncertainty
are refused before START: at HZ=100, 18 and 19 ms refuse, while 20 ms is
admissible subject to the unchanged FINISH checks. An unreplayed session has
no trusted incumbent; its inconclusive report preserves durable charges and
has an empty current-invocation attempts list. Systemd's counter policy and
hard resource caps are unchanged.

**Known limits.** Under extreme concurrent filesystem I/O a contained capture
may stop inconclusive; it fails closed. Host-dependent systemd paths need a
host with a user manager. A refusal is no verdict and grants no authority.
On a shared host, another local user can create the POSIX backend's fixed
evidence directory, `/tmp/zeno-fcis-containment-<uid>`, first; contained
commands are then refused until it is removed, which fails closed. If the
caller is killed during a contained POSIX run, the guardian can exit with
code 101 instead of its refusal code after cleanup; only the exit code
differs. Both are planned for 2.5.1.

## Arithmetic and specifications

**Tested and finite checked.** The main scalar route uses checked signed
128-bit arithmetic, including the signed minimum. Explicit signed fused floor
division preserves the exact product until the final fit check. Units and
scales are checked before erasure; explicit target scales and bounded rounded
chains retain prefix rounding and refusal semantics. The quantity-enabled
route caps dependent chains at 16 steps; historical policy/2 compatibility
remains separately checked. Native regression, independent arithmetic and
semantic mutation controls cover their declared cases, not every program.

**Proved in the reviewed proof profiles.** The arithmetic repair restores the
signed-floor quotient/remainder argument and division projection, extends the
assembly term proof with the signed-floor operation, and brings the retained
verification workspace into line with 128-bit scalars. These are Verus proofs
under their reviewed specifications, pinned verifier, solver, translation and
erasure assumptions. The pinned Verus profiles are run on the release source
as part of release qualification.
They are distinct from Lean kernel-checked results and from solver-attested
queries, and do not prove the whole compiler, runtime or application shell.

## Spot pool

ZenoDEX Spot: for any group of accounts and any number of operations, rounding gains stay below one atom per asset per account. This is kernel-checked in Lean for the pool's contract model. The Rust implementation is checked against that contract exhaustively on bounded domains and by replay. A machine-checked refinement from the Rust code to the model, and a no-profitable-round-trip theorem, are planned for 2.5.1.

## Knowledge snapshots

**Tested.** A knowledge invocation freezes the selected external files into a
bounded snapshot, binds their bytes to its identity and refuses inconsistent
or incomplete acquisition. Configuration precedence remains explicit, and
retries cannot silently change the invocation's knowledge. Retrieved facts
and proposed plans remain advisory; a snapshot does not authenticate facts,
establish owner intent or authorize a decision.

## Qualification tools

**Tested tooling.** Qualification tools add
bounded worker scheduling, deterministic source identities and separate
source policies for approved test-only changes. Offline replay and planted
controls check those tools within their stated scope. These changes are
intended to shorten qualification; no end-to-end speedup has been measured
for this release. A missing, timed-out or stale check never counts as a pass.

The checked program API and private publication boundary remain in force.
Canonical formats keep their own versioned identifiers. Tests, finite checks,
solver-attested results, kernel-checked theorems and proofs under a tool's
trusted base describe different evidence; none alone establishes release,
deployment or external-effect authority.

---

## Historical 2.4.0 source and qualification

The 2.4.0 notes below describe the changes recorded
in the [changelog](../CHANGELOG.md#240---2026-10-15). Final source qualification,
hosted checks, packaging, signing and publication remain separate gates; these
notes do not claim those gates have passed. Published downloads must be checked
against their signed checksum list as the [installation guide](INSTALLATION.md)
describes.

## Arithmetic and execution

- Authored rules gain checked scalar multiplication, floor/ceiling division,
  fused mul-div and exact product comparisons, with explicit arithmetic
  profiles and refusal boundaries. Narrow and fused operations remain distinct.
- The Spot arithmetic pilot covers fee splits, bounded exact-in swaps,
  liquidity burns and closed graph bounds. It does not establish business
  acceptance, authorization or correctness of arbitrary histories.
- Four closed evaluator adapters are inlined. Earlier paired measurements on
  one serial workload do not establish a release-wide performance improvement.
- Parallel checker containment follows the caller's CPU policy and runtime
  ceiling. Failed offline runs publish no receipt; retained loop receipts
  remain replay-checked history.

## Search, dialogue and knowledge

- `zeno-fcis-program-search` proposes smaller finite decision programs under
  explicit budgets. `transform check` and matched receipt replay decide
  equivalence; the search itself supplies no adoption or publication authority.
- The bounded loop adds expression candidates, sketch completion, family fits,
  transfer, meaning keys, teach-back and decomposition. `loop pipeline`,
  `loop strategy` and `loop rules` add guarded completion, repair, strategy
  selection and rewrite proposals. Family claims hold only on checked instances.
- `loop question --specification` searches finite sketch meanings without an
  existing core. Counts, minimax partitions and conflicts are relative to that
  grammar and domain. A budget stop is inconclusive and spends no attempt.
- `loop learn --spec FILE` searches the declared checked vocabulary. Native
  reference completion and `loop capture` require complete finite capture,
  exact checking and receipt replay. Example agreement alone is a proposal;
  unsupported containment and incomplete work remain unqualified.
- Structured usage knowledge, bounded stratified Datalog and typed strategy
  plans remain advisory. Explicit `--kb PATH` values override `ZENO_FCIS_KB`;
  without `--kb`, that environment variable supplies an OS-separated path
  list. With neither configured, the neutral compiled example is used. Private
  knowledge and evidence remain outside source and release packages.

## Contract judges and composition

- `contract review --gaps`, `profile`, `lemma`, `scenario` and `probe` expose
  specification ambiguity, state behavior and counterexamples over declared
  finite domains. Malformed inputs and exhausted budgets cannot become passes.
- `contract check-composition` adds the composed-domain `/3` profile: merge-order
  joint steps and bounded local, assumption and coupling obligations. The
  induction argument is documented, not machine-checked.
- `loop decompose --compose` compares the composition checker's joint step
  with the core over every declared input. This no-outbox profile refuses
  source channels or deliveries with `compose-core-deliveries-unsupported`.
  Bounds obligations alone do not establish behavioral equivalence.

The component catalog’s declared laws do not fully specify its decisions. Finite-domain contract-judge runs across all 77 supported instances (107,518 input tuples) found 0 decisions uniquely determined by those laws; in every instance the laws also admitted a core that rejects every request. All 77 declared state laws were inductive over their respective finite domains. These invariant checks establish bounded state preservation, while acceptance conditions and rejection reasons still require additional specification and behavior examples. Mutant results cover only the finite mutation catalog, and these advisory checks provide neither a general proof nor release qualification.

## Platforms, storage and compatibility

- The release set requires Linux x86_64 GNU (glibc 2.39 or later) and Apple
  silicon macOS executables, each with its own build, custody and installed
  binary checks. Linux containment and `synth verify` execution remain
  unavailable on macOS (exit 2); the macOS binary is ad-hoc signed, not notarized.
- SQLite uses `synchronous=EXTRA`; Apple builds also use `fullfsync`. Durability
  against power loss still depends on the drive. The OCaml scalar consumer
  joins the acceptance portfolio.
- The checked program and private publication boundary introduced in 2.3.0
  remains the supported V2 route. Canonical protocol identifiers are versioned
  independently. Historical fixtures and qualification counts below keep
  their original subjects and are not 2.4.0 qualification evidence.

## Historical 2.3.0 source and qualification

ZenoFCIS 2.3.0 introduced the first stable V2 source line. It combines the
V2.2 and V2.3 source line into one version, with no separate stable 2.2.0
release. V2 deliberately changes the 1.1.0 API. These
notes describe the release's source changes and locally observed checks. They
do not themselves claim final acceptance or hosted platform qualification;
each published artifact's qualification belongs to its release workflow run,
and the signed `DOWNLOADS.sha256` lists the published assets. The earlier
prereleases retain their [candidate records](RELEASE_CANDIDATES.md).

An application declares its original schema, decision program, reasons,
channels and laws as data. The library admits the inputs, computes the
decision and resource use, checks every applicable declared law, and issues
a private publication capability only for a committing outcome. Applications
cannot replace this route with decision or law callbacks, caller-reported
usage or caller-built publications. Specification, search and evidence tools
grant no publication authority.

### Library and contract specification

- The crate root and prelude use the checked program API. `bind_catalog` and
  `bind_program` admit declarations; `Program::publish` evaluates original
  state, command and supplied context with a library-owned eight-resource
  meter. Genesis and publication replay use the same checked route. See the
  [API migration guide](V2_PROGRAM_API_MIGRATION.md).
- `generate contract` binds the schema, Rust declarations and encoded policy
  before writing them. `new --contract` generates an operational application
  bound to a matching ZenoFCIS source tree. The compiler's functional core
  is in `zeno-fcis-contract`; its supported API is `checked_composition` and
  `ContractError`.
- `contract review` compares supplied decision examples with exhaustive
  decisions within its tuple cap, or a documented boundary set for larger
  domains. Mutant witnesses and law-refusal findings are advisory.
  `contract draft` records labels and their provenance as supplied assumptions.
- `contract diff` uses the store's comparison functions for program
  successors and renames. A structural classification does not establish
  behavioural equivalence. `contract adopt` preserves JSON layout and
  escaped key spellings, including compact and CRLF rule files.
- Library numeric descriptors support checked signed and unsigned 128-bit
  arithmetic and whole declared compound values. Nested field selection and
  a numeric CLI specification or lineage-upgrade route are unsupported. CLI
  contracts support up to 4,096 nodes, 64 inputs and 64 outputs with checked
  signed 64-bit expressions. See [wide values](WIDE_VALUE_EXECUTION.md).

### Durable stores and migrations

The schema-10 SQLite store checks identity, freshness and the complete
database schema inside a transaction. Opening a lineage replays its history;
upgrades are recorded as chained, replayable steps. An unexpected schema
object, including a user trigger, is refused.

Program successors require the store to re-establish all five admission
premises, including exhaustive program comparison. Data migrations require
forward simulation over every law-satisfying old state within the declared
domain cap. Rule changes require the new state laws and declared inductive
claims to hold on the current state. Migration checks run again during
generation, upgrade and audit.

`migration/2` adds closed-expression field splits through the checked
evaluator, total injective payload maps, and explicit drops of state fields
the old contract never reads. Forward simulation must confirm the mapping.
The existing carry, rename, default and table-split forms remain available.

Generated applications provide `init`, `submit`, `decide`, `state`,
`history`, `pending`, `deliver` and `version`. Delivery preserves original
IDs and canonical payloads across upgrades. Its consuming-token lifecycle
enforces acknowledgement order; external transport is at least once and
needs receiver idempotency for one effect. A worker's success report is
trusted shell input. Crash-injection hooks are disabled by default behind a
dedicated test feature.

The store sets `synchronous=EXTRA` on every platform to sync the
rollback-journal deletion at commit. Apple builds also enable `fullfsync`
for SQLite file syncs. Power-loss durability still depends on the drive
honouring the requested syncs.

### Equivalence and bounded search

`transform check` compares outputs and exact failure classes on every tuple
of the declared finite domain under eager semantics. `transform replay`
reproduces the receipt. `optimize` and `loop` propose candidates; acceptance
requires the checker result. Search establishes no optimality or convergence.

Loop sessions default to `checked-i64-v1`, scale work ceilings with the
original program, and use `zeno-fcis/checker-work/4`. Earlier work-policy
sessions must be reopened. Repeatable `loop run --seed` inputs are ordinary
checked proposals with no model call. Equivalent review mutants can be
exported with `contract review --mutant-programs`.

`loop` and `transform check --offline-parallel` require Linux cgroup v2,
memory and pids controllers, and a running systemd user manager. Checker
children have a 64 MiB memory cap, zero swap, 64 tasks and bounded inherited
CPU policy. Operational failure is inconclusive; a failed offline run writes
no receipt. Serial checks, replay and ordinary library use do not require
this runtime.

Canonical serialization uses fixed 16-byte words, reserves its encoded size
and appends byte slices in bulk; legacy numeric projection reserves output
space. No release-wide speedup is asserted. `transform`, `optimize` and
`loop` retain their 256-node and 16-output bound. Enumeration defaults to
10^8 tuples; contract review and migration admission default to 2^20.

### Certificates, composition and integrations

`transform check-certificates` and `replay-certificates` support declared
domains beyond enumeration for quantifier-free Boolean and linear integer
reasoning over eager checked-i64 programs. Every difference query needs a
complete Alethe certificate accepted by the pinned Carcara checker. The
scalar query refinement covers eager node evaluation, ordered outputs and
refusals, Boolean encoding and a nonbinding Step limit under its named
trusted base. Certificate receipts grant no adoption or publication
authority; adoption still needs the exhaustive transform receipt.

Solver-only symbolic results and inductive claim steps are attested by
their tools. Counterexamples count only after runtime replay. The normal
CLI's `prove --backend lean` returns `UnsupportedMode`; these results do
not become Lean kernel-checked evidence. See
[symbolic checks](SYMBOLIC_CHECKS.md) and
[inductive claims](INDUCTIVE_CLAIMS.md).

`contract check-composition` checks finite leaves and interface implications
without enumerating the joint product, and validates a generated aggregate
against those leaves. The safety theorem is over the abstract evaluator
specification. Exhaustive local/StateFact/interface tables, assumption
discharge, aggregate validation, interval and meter checks, and the link to
actual Authority decisions form its tested trusted base. The validator's
Rust implementation is not proved. A command selects one leaf, with peer
views from the same pre-state. Conditional histories require checked genesis
and the stated outer assumptions; outer guarantees discharge nothing.
There is no composed publication capability or theorem of liveness,
fairness, traces, delivery, continuous dynamics, asynchronous or shared-write
coupling.

Six added application templates cover account lockout, order fulfillment,
inventory reservation, compliance, withdrawal queues and treasury controls.
Local checks of ten finite component families covered 77 closed parameter
instances and 107,518 raw transition inputs. Each of 17 planted law mutants
produced one law-refusal finding. Certificate issuance and replay passed for
all ten families; the accompanying tamper qualification passed. A stored
certificate identifies evidence; replay supports only its declared closed
finite scope.

Six coupled applications passed full-domain checks over 373,056 configured
input tuples and 180 unignored, unfiltered native tests. Those tests include
53 named fault controls. Separate warehouse reviews found five law refusals
and caught a destination substitution through complete outbox comparison.
Finite checks, native tests and certificate replays overlap; these totals
are not additive and establish no formal family theorem.

The optional [Tau kit](TAU_KIT.md) requires paired SMT agreement for each
exported query and refuses incomplete or disagreeing runs. Agreement is
attested, with no whole-domain Tau or kernel-proof inference. Local native
checks covered 262,144 allowance-ledger inputs and 20,480 spend-approval
inputs. Separate Tau/SMT runs corroborated 150 positive allowance queries
and rejected 10 planted controls (160 pairs), then corroborated 373 positive
spend queries and rejected six controls (379 pairs). Every positive Tau `T`
was paired with SMT `unsat`, and every control `F` with SMT `sat`; neither
run reported disagreement, `UNKNOWN` or failure. These query results grant
no authority and do not authenticate supplied authorization or context.

The OCaml consumer is a client of the generated Rust application; Rust retains
decisions, laws and storage. ZAL remains an optional finite-model specification
prototype, outside the released API; supplied labels and scripted review
are not human approval or authenticated authorization.

### Observed assurance scope

Installed Verus profiles currently record the following positive inventory
snapshots. Whole-crate counts include shared dependencies; rows and
whole/restricted counts must not be summed as independent proofs. Remaining
qualification gates and their controls are separate from these inventories.

| Profile scope | Whole-crate snapshot | Selected scope |
| --- | ---: | --- |
| Shared execution dependency closure | 1,161 | Selected bodies vary by profile |
| Checker | 1,282 | 688 selected Exec bodies |
| Scalar query | 1,325 | 688 selected Exec bodies |
| Checked assembly | 1,188 | 27 restricted obligations; zero selected Exec bodies |

The scalar query qualification passed 43 native tests and two targeted fault
controls. Those controls do not claim whole-negative qualification. Local
Linux preparation checks passed 148 SQLite tests with zero ignored tests;
the OCaml consumer passed 24 native negative cases and two overflow controls.
A local compliance-gateway synthesis replay checked 2,880 inputs for each of
Rust, Python and JavaScript. Emitted-source conformance applies only to the
exact artifact and target verified by `synth verify`.

These observations do not establish final acceptance or hosted results.
Verus relies on the reviewed specifications, translation and erasure,
pinned vstd/Z3, Rust compilers, allocation and platform assumptions. The
production SQLite shell, operating system, hardware and external receivers
remain trusted. Crash and parity checks are tests, with no whole-shell or
end-to-end theorem. No check establishes that supplied laws, labels or
examples capture human intent. Supplied principals, authorization, time and
context are application inputs; ZenoFCIS does not authenticate them.

### Platforms and upgrading

The release process publishes a binary for each of these targets only after
its platform qualification, installation and release-custody checks pass:

| Target | Requirements and qualification boundary |
| --- | --- |
| Linux x86_64 GNU | glibc 2.39 floor; qualification belongs to the completed matching Linux host run |
| macOS on Apple silicon | `aarch64-apple-darwin`; minimum-version metadata is macOS 11.0; qualification belongs to the completed macOS 15 arm64 run, not every macOS version |

There is no Windows or Intel macOS binary. Linux-only containment and
emitted-code execution for `synth verify` do not become macOS-qualified
features. A configured workflow or minimum-version field does not establish
a qualified host, published executable, notarization or verified download
signature. Follow the [installation guide](INSTALLATION.md) and the actual
release record for available assets and their verification.

Source and generated applications use Rust `1.97.1` and a matching source
tree; an installed CLI needs `new --source`. V2.3 identities are refreshed
and source-bound. Retain matching source and binary records instead of
assuming earlier candidate receipts or stores are interchangeable.

From 1.1.0, the checked API replaces native transition callbacks,
caller-built meters, application law engines and public candidate sealing.
V1 data/evidence tools are under `zeno_fcis::legacy`; `Value::Bool(true)`
becomes `Value::boolean(true)` and `TransitionDecision` becomes
`PublicationOutcome`. Older databases are not migrated implicitly. See the
[migration guide](V2_PROGRAM_API_MIGRATION.md) for the exact source changes.

The security policy supports the 2.3.x line and supersedes 1.1.x and 1.0.x;
candidates are unsupported. [`SECURITY.md`](../SECURITY.md) states the policy
and the private reporting route.
