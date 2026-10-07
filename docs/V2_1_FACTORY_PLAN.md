# ZenoFCIS 2.1: high assurance software factory roadmap

> **Superseded on 2026-10-05** by [the 2.1 and 2.2 plan](V2_1_V2_2_PLAN.md),
> which is the plan of record. This earlier roadmap is kept for its design
> notes and research. For example, section 6 on transform and optimization
> with independent equivalence checks is cited by the `rust-typestate-newtype`
> skill. Where the two documents differ, the new plan governs.

Status: planned; updated 2026-10-04. Track 6 now has application-derived native
optimization evidence, recorded below; the product feature remains planned.

ZenoFCIS is being built as a **neurosymbolic software factory for high assurance
FCIS applications**, with the ambition to become a **state of the art high
assurance software factory**. Neural models propose specifications and programs;
symbolic checking governs acceptance under the supported contracts.
Its foundation is a **formally verified functional core**; 2.1 adds ways to
review, generate, compose, optimize and maintain applications around that foundation.
This document specifies future work. It does not claim those features or
comparative leadership are achieved.

## Prerequisite: finish V2

The [V2 implementation and proof plan](V2_VERIFIED_CORE_PLAN.md) and
[route-required ledger scope](V2_LEDGER_SCOPE.md) are the completion contract
for the smaller V2. Original-byte
admission, complete decisions, library-owned laws and genesis, mandatory
authority, source identity, replay, required shell changes and migration
documentation must close on the exact qualified source. Complete supported
application paths and their existing legal domains must be preserved.

2.1 adds semantic review, generated integration, parameterized component
families, checked upgrade operations, explicit evidence dependencies,
maintenance measurements and independently checked program transformations.
Consolidating the existing transition abstractions
and making the verified route mandatory are already V2 obligations.

The factory-level acceptance target is:

```text
Under published assumptions Gamma:
    AcceptArtifact(S, P, e) implies that actual artifact P conforms to S.
```

Here S includes its admitted domain, required successful behavior and complete
refusal behavior; P identifies the actual execution artifact; e binds its
evidence, tools and dependencies. This is a target for a qualified acceptance
checker. A manifest, passing example or unchecked solver answer alone cannot
establish it. Specification adequacy, external facts and effect delivery retain
their own obligations. The [epistemic-status ADR](adr/0003-epistemic-status.md)
continues to govern evidence labels.

## New supported execution profile

The owner decision of 2026-10-03 moves compound-value execution and full-width
U128 zUSD to V2.1 as a new supported profile. The prerequisite is the smaller
V2's mandatory Authority → Publication → SQLite v2 route for its eight named
templates. Existing V2 template domains must survive; compound and zUSD domains
must not be silently reduced to fit the scalar profile.

Status: planned. Schema construction, compound parsing, historical private
execution and a smaller checked fixture do not establish this profile's
end-to-end authoritative execution.

The profile must support the retained 12-type compound fixture, including
nested/root payloads and its used tuple, record, sum, vector, map, bytes, text
and integer shapes. It must preserve the full-width zUSD producer, laws,
reasons, successor, one-root patch, effects/outbox and refusal accounting.
Keep successful wide values, including the historical 2^63 deposit, and test
both U128 boundaries with independent arithmetic/representation witnesses.
No field arithmetic or signed-64 approximation may replace the declared
integer, overflow and rounding semantics.

Qualification requires exact canonical admission and output construction,
protected access and charge-before-work, complete law/genesis frames, private
publication custody, identity-bound replay and the supported SQLite route.
Start from a reviewed profile contract and counterexamples, then choose the
smallest execution representation that can meet it. A second interpreter is
not accepted merely because historical code exists; its correspondence and
integration would need their own proof and custody evidence.

The local removal baseline remains recoverable from git:

```text
refs/simplify/baseline
94bac799b44859b158e50377313b7e0ef6c1bc51
```

This local ref has not yet been published. Preserve a reachable recovery commit
or source archive before claiming remote recovery.

The baseline tree above contains the compound walker at
`crates/zeno-fcis-synthesis/src/finite/execution_v2/compound.rs`, its
`compound/spec.rs` and `compound/tests.rs`, and the related Original schema,
value, selector, law and composition arms. S3 removes these from the smaller V2; its removed-path/test ledger
identifies the further restoration candidates. Restored bytes bring their
historical scope and open obligations; they do not retain qualification after
integration with a changed value model, evaluator identity or artifact shape.

Acceptance must retain the earlier fixture's legal domain, test bodies and
assertions. Replay the retained corpus, required successes and boundary classes;
state the finite replay bounds and compare complete decisions with independent
retained oracles. Prove full-domain semantics wherever the claim is universal;
bounded replay alone cannot establish it. Prove reachable executable bodies and checked
bridges; run source/body coverage, meaningful mutations, native comparisons,
exact-head CI and independent review. State any account-rule or other semantic
correspondence limits rather than broadening V2's earlier evidence labels.

Release engineering separately owns QEMU, portable source transport/mirrors,
archive/privacy scans, the private historical oracle and package/version polish.
They remain required where applicable before shipping. They are not compound
execution evidence and are not part of the smaller V2 core completion claim.

## What the precedents teach us

The following are observations from official project documentation and the
Fiat-Crypto paper, assessed on 2026-10-02. They are different systems and
domains, rather than a ranking of assurance.

| Precedent | Relevant capability | Proposed application to ZenoFCIS |
| --- | --- | --- |
| [Fiat-Crypto](https://github.com/mit-plv/fiat-crypto) | Specialized arithmetic generated from a Coq development | Prove reusable component families and transformations, then qualify each specialization |
| [EverParse](https://project-everest.github.io/everparse/) | Verified parsers and formatters generated from restricted declarative formats | Keep each supported application profile explicit and prove its admission boundary |
| [Dafny](https://dafny.org/) | A verification-aware programming language | Treat specifications and implementation correspondence as part of authoring |
| [Rosette](https://docs.racket-lang.org/rosette-guide/index.html) | Solver-aided verification and synthesis | Explore alternatives through a small interpreter with explicit logical semantics |
| [verus-spec-check](https://github.com/verus-lang/verus-spec-check) | Testing, fuzzing, model checking and mutation of specifications | Evaluate it as an auxiliary way to challenge contracts and trusted assumptions |

Fiat-Crypto's 2019 paper demonstrates specialization through partial evaluation
and explains how proof effort can follow algorithm families rather than every
use case. Our proposed application is to reuse proofs for bounded accounting,
reservation and authorization components, with explicit instantiation and
composition obligations. [Fiat-Crypto paper](https://adam.chlipala.net/papers/FiatCryptoSP19/FiatCryptoSP19.pdf)

Its documented backend boundary matters: the Bedrock2 AST translation has a
proof, other backends lack that proof, and printing Bedrock2 C source remains
unproved. Rust output therefore does not inherit an end-to-end Rust theorem.
Our acceptance must identify every lowering, printing, compilation and adapter
step and either check it or name its trusted assumption.
[Backend status](https://github.com/mit-plv/fiat-crypto#status-of-backends)

## Delivery order and acceptance criteria

Every track begins with its mathematical contract and counterexamples, then
implements the smallest useful interface. Status for each track is **planned**.
An implementation must retain source-bound proofs, native comparisons,
meaningful mutations and independent review on its final revision.

| Order | Additional feature | Concrete acceptance criterion |
| --- | --- | --- |
| Supported-profile prerequisite | Compound values and full-width U128 zUSD | Qualify the retained domains and successful wide cases through the same mandatory authority route, as specified above |
| 1 | Review specifications with distinguishing examples | Replay witnesses for ambiguity and weakened rules; distinguish findings, bounded absence and inconclusive searches |
| 2 | Generate application integration | One reviewed supported contract produces a runnable app using the mandatory V2 route, with no project decision or law callback |
| 3 | Reuse verified component families | A parameterized reservation component retains its complete decision and conservation properties across the full supported parameter domain |
| 4 | Check upgrades and evidence dependencies | Qualify a real schema upgrade, historical replay and pending-effect handling; report exactly which obligations need fresh evidence |
| 5 | Measure agent maintenance | Run a preregistered comparative pilot with independent hidden acceptance and equal budgets; publish failures and uncertainty |
| 6 | Transform and optimize pure programs | Independently check each candidate over the complete declared finite domain, with explicit observations, refusal/resource semantics and artifact-bound evidence |
| Research alongside 1 and 3 | Evaluate external specification tools and Fiat-Crypto | Produce pinned compatibility and assurance-boundary reports before adopting a dependency |

### 1. Review meaning before locking a specification

Bring existing domain, vacuity and resolved-path checks into one review report.
Extend it with searches for inconsistent constraints, missing outcomes,
unreachable branches under declared precedence, excessive freedom and required
successful cases that cannot occur. Type-only properties are labeled with that
scope. A requirement holding independently of the implementation deserves
inspection before being presented as evidence about its behavior.

For two deterministic proposed meanings, the distinguishing-example query is:

```text
Find x in the shared admitted domain such that S1(x) != S2(x).
```

The difference compares the complete decision, including class, reason,
successor, patch and ordered effects. Domain changes are reported separately.
A bounded sequence search records its initial state and trace-length bound.
For nondeterministic or partial specifications, additionally search for two
different permitted outcomes for the same input and for inputs with none.

Present the witness as a product question. For an order app: payment
confirmation follows an accepted cancellation; should it refuse, or reopen the
order? The owner chooses the intended rule. Store that choice with an
independently authored example and its reviewed contract revision.

Use exhaustive comparison where the complete finite domain is supported, or a
pinned symbolic query with its stated theory and bounds. Replay every proposed
witness through the actual supported semantics. Timeout, UNKNOWN, unsupported
theory and failed replay remain inconclusive. Absence within a bounded trace
does not imply absence in every history.

Acceptance includes planted specifications with an empty domain, conflicting
required outcomes, an omitted effect, a shadowed branch, an unintended second
outcome and a reject-everything policy that violates a required success case.
The review must find each supported defect and produce a concrete replayable
example or explicit logical explanation. It cannot certify all human intent.

### 2. Generate the repetitive integration

Provide one documented progression: review rules, build, qualify, run and
upgrade. Reuse V2's program family and private authority results. Generate
schema bindings, complete decision and law metadata, checked construction,
source identity, native/Wasm wrappers and evidence commands from the same
reviewed declarations. Generated native/Wasm execution needs its own exact
target correspondence evidence before becoming an authoritative optimized path.
Any such path must also meet [ADR 0005](adr/0005-decision-gate.md)'s separate
design, authority-owned meter/state view, complete-decision and provenance
requirements.

An agent must not manually duplicate reason mapping, state projection,
outbox construction or policy identity between templates and runtime glue.
Generated connectors must themselves have implementation correspondence or an
explicit open obligation; generation alone is insufficient. Hide proof-tool
details in ordinary app authoring and show the checked scope and remaining
requirements when they affect a user's decision.

Extend the existing MCP and synthesis skill with semantic-review witnesses,
build/qualification results and upgrade checks. Tool output carries exact
artifact revisions and named evidence status; it cannot report an unfinished
run as authorization. Proposed commands and tool names require interface review
before being treated as installed capabilities.

Acceptance starts with a complete inventory app and its independent decision
examples, then a second application to challenge reuse. A realistic rule change
must regenerate all affected integration without a trusted handwritten decision
adapter. Preserve all original legal inputs and all three decision classes.

### 3. Build reusable verified families

Start with bounded reservation and accounting over the supported V2 profile.
For a reservation parameter C, establish the declared conservation relation
and bounds for every supported C, including zero and the maximum supported
value. Include reservation, release, refusal, committed failure, overflow,
logical resources, reason mapping and ordered effect plans.

The app supplies policy data and discharges explicit component assumptions.
It does not supply a substitute execution or success verdict. Composition must
prove that one component's actual outputs satisfy the next component's inputs,
including identities and effects. Publish the instantiation obligations that
remain after the generic theorem is reused.

Follow with idempotent command processing and authorization components.
Durable scheduling may prove safety of generated plans; delivery liveness or
exactly-once behavior requires separate stated environment and destination
assumptions. Additional collection or arithmetic profiles must gain their own
proofs and admission before use. Expand capability without narrowing earlier
domains to preserve a green proof.

### 4. Treat an upgrade as a checked operation

A migration m must establish at least:

```text
I_old(s) implies I_new(m(s)).
```

Also specify which observable information and complete decisions it preserves.
For a compatible upgrade, relate old and new steps through the state and
command mappings. Intentional behavior changes need their own reviewed
compatibility statement; invariant preservation alone does not establish
behavioral or replay compatibility.

Bind both old and new schemas, contracts, execution identities and migration
artifact. Define malformed-state refusal and atomic failure, how old history
continues to use its original interpretation, and how pending effects retain
or explicitly change their order, destination and idempotency meaning.
Rollback requires its own information-preservation and pending-effect rules.

Add an explicit evidence-dependency inventory: implementation, specification,
domain, tools, assumptions and composition bridges. A change invalidates every
dependent obligation. Reuse requires a checked dependency argument; a prior
receipt cannot qualify changed source. Presentation changes may retain evidence
only when qualification inputs actually stay unchanged.

Acceptance upgrades an app with existing state, history and pending outbox
entries, checks replay before and after, and refuses a lossy mapping and a
change in effect identity that was not declared or reviewed. A permitted
remapping must discharge the stated pending-effect compatibility obligations.
Fault injection must demonstrate the specified atomic failure boundary. This
extends V2's required migration documentation and shell baseline rather than
postponing them.

### 5. Demonstrate agent maintainability

Preregister a pilot across reservation, order processing and authorization.
Include domain expansion, new behavior, proof repair, a schema upgrade and a
tempting change that weakens a specification. Compare ZenoFCIS with a strong
Rust-plus-verification workflow using comparable models, budgets, initial
requirements and independently assessed acceptance criteria.

Keep held-out acceptance separate from agent-generated tests. Count every
attempt, failed run and timeout. Measure correctly completed useful changes,
incorrect changes accepted, human review minutes, total expenditure, proof
repair effort and median/tail completion time. Record changed assumptions and
specification weakening as outcomes, rather than treating green tools as success.

The pilot diagnoses engineering problems. A broader representative comparison
with uncertainty estimates is needed for a leadership claim. Publish the task
scope and raw results; improvement on a selected pilot does not establish
universal superiority or production qualification.

### 6. Transform and optimize with independent equivalence checks

Provide `transform` for checking a supplied replacement and `optimize` for
proposing smaller pure programs through bounded e-graph search. Start with the
existing total Boolean acyclic scalar graphs, then qualify checked-i64 as a
separate extension. Arbitrary Rust and additional execution profiles require
separate qualification.
[egg's equality-saturation tutorial](https://egraphs-good.github.io/egg/egg/tutorials/_01_background/index.html)
provides the proposed search mechanism; e-class membership and rewrite
explanations do not authorize a replacement.

The equivalence target is:

```text
For every input x in the complete declared finite domain D:
    observe(execute(original, x)) == observe(execute(candidate, x)).
```

The original domain, ABI and execution semantics are fixed qualification inputs.
Admit both programs and independently compare their actual lowered artifacts
under those semantics. The checker's implementation correspondence and artifact
binding need their own qualification; a comparison of an unbound AST is
insufficient. Exhaustive checking establishes equivalence only for the declared
domain and observations, under the published checker and execution assumptions.

`transform` returns checked equivalence, a replayable counterexample, or an
inconclusive result with a stated cause. Reject domain shrinking and refuse an
empty domain for qualification. Sampling, partial enumeration, timeout,
unsupported semantics and checker failure cannot produce checked equivalence.
Compute coverage without count overflow and verify complete enumeration before
issuing a receipt. Large finite domains may remain impractical to exhaust.

`optimize` treats `egg`, custom rewrite rules and candidate extraction as
untrusted proposal machinery. Bound search time and memory, lower each candidate
and pass it through `transform`. Return the best independently accepted
improvement or the original with no checked improvement. A search failure cannot
promote an unchecked candidate. Pin the optimizer and review its dependency and
license compatibility before adoption; this plan installs no dependency.

Define two explicit equivalence profiles. Functional equivalence compares
declared values and specified failures. Application equivalence compares the
complete decision: class, reason, successor, patch, ordered effects/outbox and
every authoritative observation, including required law results, resource
usage, traces and refusal behavior. A functional-equivalence receipt alone
cannot authorize replacement of an application core.

Smaller graphs often change metering. Any resource change needs a separately
specified and checked refinement, with budgets, resource-observing laws and
refusal behavior requalified. Refresh execution identity and dependent evidence
before using the replacement through the mandatory authority route. Do not
silently ignore observations to make optimization pass.

Preserve checked overflow and eager evaluation, including failing unused or
unselected nodes. For example, `(x + 1) - 1` cannot become `x` on a domain
containing the maximum checked integer. Even `not(not(x))` becoming `x` can
change resource usage despite preserving Boolean outputs.

Measure cost on the actual lowered shared DAG: node count and canonical encoded
size, with any additional objective stated explicitly. A tree-cost extractor
does not by itself minimize shared multi-output DAGs. Report a checked cost
improvement, not global minimality.

Bind each receipt to both canonical artifacts, the complete domain and ABI,
execution semantics, equivalence/observation profile, checker revision, coverage
and measured costs. Record optimizer and rewrite revisions as proposal
provenance. Replay must reject stale or tampered bindings. Extend the MCP and
synthesis skill with these checks and their evidence status after interface
review; proposed command names are not installed capabilities.

Acceptance includes a useful simplification checked across its whole domain,
an overflow rewrite and failing eager-node removal rejected with replayed
witnesses, and complete-decision changes to reasons, successor and effect order
rejected. Check that metering changes fail the exact application profile,
deliberately invalid custom rewrites cannot pass the checker, domain shrinking
is rejected, truncated enumeration is inconclusive, and stale/tampered receipts
are refused. Independently recompute cost from the emitted DAG. These gates
supplement the track's source-bound proofs, native comparisons and review.

#### Published research baseline and implementation design

The [published semantic e-graph experiment at immutable commit
`a69ed8db594d95279a46bff0f65185ef67d51f98`](https://github.com/TheDarkLightX/ZenoFCIS/tree/a69ed8db594d95279a46bff0f65185ef67d51f98/experiments/semantic-egraphs)
provides a bounded research input. Fresh Python replay of its stored Boolean
artifacts confirms 816 semantic versus 895 local instructions, with 18 wins,
78 ties and four regressions; checked fallback totals 812. The original
12-failure run and one-failure resource amendment remain failed; the successful
configuration is exploratory. This review did not rerun Rust optimization or
Lean, and the Lean model does not verify the implementation.

Follow the [source/evidence review](V2_1_EGRAPH_REVIEW.md) and
[transform design](V2_1_TRANSFORM_DESIGN.md): implement canonical, receipt-bound
`transform` for total Boolean programs first, then bounded external proposals
with checked fallback. Qualify eager-error checked-i64 separately. Complete
application decisions, metering, identities and replay require their own
acceptance before activation. Dependency/license qualification and enforced
process time/memory limits remain open. This elaborates track 6 without changing
the delivery order or adding a smaller-V2 completion condition.

#### Neurosymbolic software factory

The [neurosymbolic loop design](neurosymbolic-loop/DESIGN.md) and its
[six implementation specifications](neurosymbolic-loop/specs/INDEX.md) extend
track 6 with bounded neural proposals, symbolic checking and replayable feedback.
The original contract stays fixed across attempts; only checked improvements
replace the incumbent. Changes to product meaning require a reviewed new
contract and evidence chain. The design supplies conditional proof arguments
and a future evaluation protocol; it does not implement the product loop.
The separately scoped research evidence below supports that feature without
claiming a replayed neural search experiment. Existing V2 and 2.1 delivery
priorities remain unchanged.

#### Application-derived checked optimization evidence

The [withdrawal-queue feature evidence](research/V2_1_CHECKED_OPTIMIZATION_EVIDENCE.md)
records a neurosymbolic e-graph discovery as reported by the owner, followed by
independently reproduced native checking. The actual kernel falls from 16 to 7
nodes, the retained controller from 69 to 60, and the current decision scalar
graph from 106 to 100. Their complete finite scalar products contain 16, 384
and 1,296,000 tuples respectively; canonical encoded size also decreases.
Native debug/optimized checks and independent Astra Max review passed within
the recorded scopes. The original model/e-graph search was not rerun.

This is concrete research evidence for track 6's proposal-plus-independent-check
architecture. Use the Boolean kernel as an application-derived regression for
the first transform milestone. The resource counterexample also supports the
required distinction between functional equality and application adoption:
smaller graphs change Steps and boundary refusal behavior. Product checker,
receipt/replay, capped proposal loop and application activation remain planned;
this case does not widen supported domains or add a smaller-V2 completion gate.

#### Boolean and checked-integer diagnostic benchmarks

The [benchmark design and public seeds](benchmarks/README.md) provide 32
program pairs: 16 Boolean and 16 future checked-i64 cases. Two separately written
host evaluators checked all 355 declared input tuples and the 11 difference
witnesses. The packet covers shared outputs, guards, overflow, eager errors,
output order and no-improvement controls. These are development fixture checks;
production decoding, native correspondence, canonical-byte costs and optimizer
usefulness remain unqualified or unmeasured.

The [evaluation protocol](benchmarks/PROTOCOL.md) separates public regression
cases from future project extractions, generated stress cases and sequestered
family-based evaluation. Herbie inspires the diagnostic approach; comparable
effectiveness is a future question. The [bounded handoff](benchmarks/CLAUDE_HANDOFF.md)
starts with Boolean supplied-candidate fixture ingestion at the existing track-6
milestone. This adds no smaller-V2 release condition or enabled runtime feature.

## External-tool investigations

### verus-spec-check

The project documents specification mutation and branch-coverage modes, with
property-testing, fuzzing and optional Kani backends. It also states that it is
under active development with incomplete or missing features. These are useful
ways to expose weaknesses; passing them does not prove specification adequacy
or an assumed axiom. [Official usage](https://github.com/verus-lang/verus-spec-check)

Pin the candidate revision and test it against ZenoFCIS's exact Verus/vstd and
native Rust pins. ZenoFCIS shares ordinary Rust bodies using conditional
`verus_spec` attributes, while this tool documents annotating functions inside
`verus!`; compatibility must be demonstrated without introducing an unproved
test copy or rewriting production solely for the tool.

Try three controlled cases: a weak top-level contract, a false arithmetic
assumption and an unreachable required branch. Compare findings with existing
independent examples and mutation gates. Bound time and record counterexamples,
unsupported forms and failures. Adoption requires preserved ordinary proof
results and actual-source correspondence. Existing verification stays mandatory
if this auxiliary tool is unavailable.

### Fiat-Crypto

First study its operation specifications, reusable transformations, partial
evaluation and parameter specialization. Map those stages to a reservation
component and identify where ZenoFCIS can reuse an argument rather than adding
per-app trusted glue. This architecture lesson does not require a dependency.

For a component F with parameters p and a specializer T, the reusable target is:

```text
For every supported p and admitted x:
    execute(T(F, p), x) = F(p, x).
```

Prove the component's laws and the specializer's correspondence separately.
Each application still discharges its parameter and composition assumptions.
The emitted Rust or Wasm artifact needs its own checked connection to T's
output; a theorem about an internal representation cannot silently cover an
unproved printer or backend.

Direct integration is a separate bounded research decision. Fiat-Crypto is
suited to modular cryptographic arithmetic; financial accounting needs explicit
integer, overflow and rounding semantics. For example, 16 + 1 modulo 17 equals
0, which is unsuitable as an ordinary account-balance update. Any wider-integer
route must preserve the application's declared arithmetic rather than replacing
it with field operations.

Pin one small generated arithmetic instance and inventory proof boundaries
from mathematical specification through actual Rust and compiled target. Compare
against an independent integer oracle, plant representation/backend mutations,
and investigate a checked translation or direct source proof for missing steps.
Cryptographic use additionally needs its own secret-handling and side-channel
qualification. These comparisons supplement the required proof argument.

Review the exact files and dependency licenses before importing anything. The
project offers a choice of MIT, Apache-2.0 and BSD-1-Clause; preserve applicable
notices and inspect the chosen submodules separately.
[Upstream license declaration](https://github.com/mit-plv/fiat-crypto#license)

No Fiat-Crypto or specification-tool dependency is installed by this plan.
An integration decision must report an actual useful capability, additional
trusted assumptions, proof gaps, maintenance/build cost and a simpler alternative.

## Additional research inputs

The [strategy-synthesis and privacy assessment](TABULAR_SYNTHESIS_FACTORY_LESSONS.md)
examines arXiv 2411.03351v1 and 2608.29674. Candidate strategy generation and
local selection inform tracks 1, 2 and 5; an optional privacy-budget controller
could challenge track 3's reuse boundary. Empirical privacy evaluation is kept
separate from a formal privacy guarantee. These research inputs add no further
implementation track, dependency or completed feature, and do not defer V2
obligations.

## Completion and positioning

The release includes downloadable binaries for each qualified platform, built
from the final reviewed revision. Prepare the CLI archives, any supported Wasm
module, checksums, licenses and the source/toolchain evidence together. Test the
packaged executables on their declared targets, including installation and an
actual checked synthesis/replay journey. A cross-compiled file alone does not
qualify a platform. Keep unsupported targets explicit.

Reuse the release-candidate packaging workflow where applicable; add the 2.1
version and platform jobs only when the accepted feature scope and gates are
ready. Retain exact revision provenance and verify archive checksums after
assembly and upload. Commit and push the qualified work before publishing the
release. If the available GitHub credentials cannot upload release assets, the
owner will provide the required token at that stage; no token belongs in source,
archives or evidence logs. A binary package does not expand the proved domain
or remove the documented compiler/platform assumptions.

2.1 qualification requires its accepted feature scope, supported domains,
checked bridges, migration examples, independent review and exact-head CI.
Each research experiment retains its result even if it leads to declining an
integration. Evidence must show what improved and what remains conditional.

The phrase **state of the art high assurance software factory** describes what
ZenoFCIS intends to become. Completed releases state their demonstrated scope.
Comparative claims require the maintenance results and assurance argument that
support them. V2 completion remains the immediate implementation priority.
