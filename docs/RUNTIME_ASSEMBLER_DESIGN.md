# Runtime assembler: checked application assembly (2.7 proposal)

**Status: design and bounded executable models, not a production implementation
or a release commitment.** This proposal extends existing application generation
and qualification. It introduces no third business-decision layer and does not
claim to build every application.

## Purpose and baseline

An adopter should be able to select a supported core, UI, store and effect
adapter, then get either a reproducible runnable package with explicit evidence
and operating limits, or a precise refusal. Today these capabilities exist on
several routes; accepting each part separately does not establish that their
combination works.

The design base is main `06a74a7c0b483034c9d607153bd11fb6a95ff096`.
The feature inventory was also inspected at the public 2.6 development revision
[`eb9af7f5cbba320ca2dd172ed6d162b71d163606`](https://github.com/TheDarkLightX/ZenoFCIS/tree/eb9af7f5cbba320ca2dd172ed6d162b71d163606).
That development branch is not assumed released. Its implementation is not
silently included in this design-only change.

| Existing implementation | Reuse; do not reimplement |
|---|---|
| `crates/zeno-fcis-cli/src/app_factory.rs`, `contract_files.rs`, `tools/app_factory.py` | Official contract-to-app routes, source pinning, `zeno-fcis/app-manifest/1`, static assets and generated tests. The manifest explicitly has no authority. Collection contracts select a different plain-form scaffold. |
| `checked_composition.rs`, `composition_command.rs` | `/1` and `/2` checked aggregate generation within their effect-free restrictions; `/3` joint checking but **no aggregate emission**. `/3` bindings read common pre-state, not earlier provisional successors. |
| `zeno-fcis-plan` and SQLite V2 | Closed first-order plans, nominal authorization, atomic state/replay/outbox publication and recovery. `CommitPlan` is evidence, not executable work; external work uses `OutboxPlan`. |
| `zeno-fcis-shell::effects` and SQLite effects handler | Registry, pure `start`/`resume` programs, bounded retry, correlated response admission, record/replay, fault simulation and production-handler checks already exist. |
| `zeno-fcis-shell-sqlite/src/collections.rs` | Keyed records, transactional writes, roots and journal; existing collection summaries are not a new feature. |
| 2.6 `docs/BUILD_AN_APP.md`, `docs/SHELL_EFFECTS.md` | Authenticated ingress, original-key recovery, configured parties, bounded program connector, pending deliveries and explicit at-least-once limits. Connector exit zero is acknowledgement, not proof of business success. |

Every path in this table is relative to the pinned public 2.6 development tree
linked above (not an assertion that it exists at the design base). The
[existing generated application](GENERATED_APPLICATION_MILESTONE.md),
[packaged qualification](PACKAGED_APPLICATION_QUALIFICATION.md), and
[composition profile](COMPOSED_DOMAIN_COMPOSITION.md) remain authoritative about
implemented behavior. Nothing here relaxes their refusals or changes stable IDs.

## Fixed ownership boundary

```text
reviewed requirements + core + adapters + evidence + declared configuration
                  -> offline assembler checks and packages
                  -> installed, source-bound application

untrusted bytes -> authenticated bounded input -> pure core decision
                -> nominal authorization -> durable state + intent commit
                -> constrained shell I/O -> bounded correlated observation
                -> pure core decision
```

The **pure core** owns business authorization, visibility and action semantics,
validation, ordering, cancellation, retry eligibility, effect protocol and domain
outcomes. A UI preview never authorizes dispatch. Time, identity and external
facts enter as explicit observations under declared trust assumptions; the core
does not read ambient state. Its invocation inputs and results remain immutable.

The **shell** owns credentials, sockets, storage handles and process handles.
It executes only authorized first-order operations and reports facts. It cannot
choose a substitute policy, upgrade permissions, convert an uncertain effect
to failure, grant approval, or infer an omitted business rule. Existing pure
shell-effect programs remain the owners of transport protocol decisions.

The **assembler** checks compatibility and packages an admitted combination.
Its checks, dependency graph, UI assets and manifest do not authorize state
publication. Startup validates the installed bindings; runtime Authority still
checks every invocation. No service locator, dynamic policy plugin, callback
closure, new workflow engine or second state store is introduced by default.

## Smallest implementation: one versioned assembly description

Extend the existing app factory with a proposed `app-assembly/1` input and an
assembly section in a **new version** of its output manifest; do not reinterpret
`app-manifest/1`. Exact identifier allocation is deferred to implementation
review. Retain the existing generator entry point and rejection behavior.

The description is data, with these required groups:

- Core: exact source/policy/program/schema identities, supported profile,
  genesis, resource limits and checked composition receipt where applicable.
- UI: descriptor and asset identities, view/action mapping, declared query
  shapes, locale/unit labels and presentation-law receipt.
- Store: interpreter identity, transaction/replay semantics, schema identity,
  durability assumptions, query/upgrade/restore profile and capacity bounds.
- Effects: closed operation/channel IDs, payload and result schema identities,
  adapter executable/configuration identities, permitted destinations,
  capabilities, acknowledgement meaning, reconciliation and deduplication mode.
- Identity: admitted principal/context schema, authentication adapter identity,
  issuer/audience/session assumptions and business-authorization policy binding.
- Operations: host/runtime target, exact dependency lock, build tools,
  configuration schema, secret **references**, health contract and log schema.
- Evidence: checker identities, subject hashes, bounds, premises, verdict and
  replay inputs. `unknown`, `refused` and absent evidence cannot become passed.

Use canonical field ordering and encoding, bounded list/byte sizes, unique
identifiers and closed enum values. Duplicate IDs, unknown required fields,
path escapes, unsupported versions and incompatible units refuse before build.
Resolve local source inputs to immutable content identities. Do not execute
manifest-supplied arbitrary build commands or admit evidence by a success flag.
An implementation must choose and publish exact numeric limits and refusal
precedence before exposing this proposed format as a stable API.

### Check and package pipeline

1. Decode and bound inputs; resolve the supported profile, without fallback.
2. Check identities, dependency closure and qualified adapter capability sets.
   Hash equality establishes identity, not semantic correctness or provenance.
3. Replay core and interface obligations. A semantic edge binds producer,
   consumer, evidence kind, artifact, revision and attempt, not merely a Boolean
   or equal wire type. Assumptions need the named provider guarantee.
4. Check UI-to-core enablement, query authorization and effect/store protocol
   compatibility. Retain checks at runtime where finite design evidence cannot
   replace them. Missing adapter qualification refuses this profile.
5. Generate using the existing supported route, build with the reviewed lock,
   then run the actual installed-app acceptance journey, including refusal and
   recovery cases. A compilation pass alone does not qualify an application.
6. Record canonical source/artifact identities, exact checks and their premises.
   Emit only to a fresh staging directory; failures leave no installable package.
   Publish the package atomically only after all required checks pass.

A mismatch report names the boundary, expected/actual public identity, refusal
class and remedy without exposing secrets. Repeated identical inputs reproduce
manifest content; timestamps and local paths belong in separate diagnostics.
Runtime startup must refuse drift in executable, policy, schema or adapter
configuration. Source guards remain conditional on their covered inputs.

Only the deployment owner under the existing setup-authorization policy may
approve a new assembly/configuration binding. The assembler cannot grant itself
new capabilities. Bind approvals to the manifest and deployment identity; keep
revocation and accepted-version policy outside user-supplied manifests. A change
that broadens authority needs explicit reauthorization. Drain or pause in-flight
work, reconcile ambiguous attempts under their original bindings, then perform
the reviewed atomic switch. Restart must not revive a revoked binding or silently
route an old intent through a more privileged adapter. The implementation must
qualify this upgrade/revocation protocol, including its recovery state.

## Supported profiles and deliberate refusals

| Proposed MVP qualification lane | Admission rule |
|---|---|
| Standalone finite scalar app | Existing factory domain/descriptor limits, existing generated UI and store; one named adapter profile at a time. |
| Standalone keyed collection app | Existing collection scaffold, explicit bounded table/query UI and store qualification; no claim that scalar ZenoUI automatically supports collections. |
| `/1` or `/2` composition to app | Only a checked, emitted aggregate that independently passes the target factory's supported-domain tests; preserve existing no-channel/no-outbox and other composition restrictions. |
| `/3` joint composition | Check/replay only. Runnable assembly **refuses** until a separate aggregate or joint-host bridge has its own preservation and publication obligations. |

The first complete target is a **single-store finite scalar task-status app with
a local durable deduplicating non-value-moving test adapter**, using the existing
scalar factory, authenticated configured-party ingress and SQLite publication.
It earns qualification only after every applicable A1–A12 gate below passes.
The keyed collection lane is a separate subsequent target; combining collections
with effects is not admitted merely because both exist independently. `/3`
hosting remains deferred.

MVP means the first target implementation, **not completed functionality in this
PR**. No multi-store transaction, distributed scheduler, arbitrary SQL,
user-supplied code execution, automatic service composition, Windows portability
or universal UI promise follows. Keyed collections and effectful scalar apps
need separate qualification lanes; feature availability is not closed under
arbitrary combinations.

## Architecture-conformance gate (proposed)

A supported package must satisfy an explicit architecture policy and threat
model, not an undefined claim to follow every best practice. Reuse the existing
[`tools/check_assurance.py`](../tools/check_assurance.py) dependency-ring,
unsafe-code and resolved semantic-source checks, its negative controls, and the
nominal authority types described in [Architecture](ARCHITECTURE.md). At the
inspected 2.6 revision, the shell-effects registry/driver already checks declared
port capabilities and response binding. These checks exist; the following
**whole assembled-app gate is proposed, not implemented by this PR**.

Extract a source-bound dependency/call/effect graph from the exact resolved
build, including generated code, feature selection and adapters. Every node is
classified as pure semantic code, preparation, authority, interpreter, adapter
or an explicitly reviewed trusted-base component. Unknown/unresolved edges
refuse rather than disappear. The declared policy must require:

| Rule | Obligation | Must-fail acceptance mutation |
|---|---|---|
| C1 pure boundary | No transitive ambient filesystem/network/clock/random/global mutable state, unsafe or FFI in authoritative pure invocations. Reviewed trusted providers remain explicit; private invocation scratch needs exclusive ownership and cannot escape. | Import a clock through an alias, retain a mutable input alias, or introduce an unresolved foreign call. |
| C2 mediated publication | Every semantic publication path requires the correct privately constructed nominal Authority result; preparation, UI and assembler receipts confer no authority. | Add a raw store-write path from preview or construct an authorization from a manifest success flag. |
| C3 decision/action separation | Externally visible business work uses approved first-order OutboxPlan operations within the supported profile; existing value-moving refusals remain. Infrastructure reads, authentication, Clock/Wait/Random and storage use named qualified ports, never hidden semantic decisions. CommitPlan evidence is never executed. | Execute a callback carried in a decision, run a CommitPlan record, or let a shell branch grant business approval. |
| C4 bounded capabilities | Every reachable operation's capability is included in the app's declared needs, the approved deployment grant and the handler's enforced scope; lower rings cannot depend on higher rings. | Add network access to a file-only adapter, a wildcard destination, an undeclared secret read, delegated excess rights, inherited child-process credentials or a reverse dependency. |
| C5 no confused deputy | Bind authenticated principal, policy, subject, operation, destination, revision and attempt across input, approved intent, adapter request and response. | Substitute another tenant, destination, producer or attempt while retaining a type-compatible payload. |
| C6 closed implementation | Resolved dependency/build closure matches the reviewed graph and evidence, including generated code and feature flags; no alternate unmediated call path is admitted. | Add an unreviewed dependency, feature-controlled bypass or dynamically loaded policy implementation. |

Capability reachability includes minting, copying/delegating/transferring handles,
credential-derived access, inherited child-process permissions, broker/deputy
calls and ambient authority, not just direct imports. Conservatively close all
possible authority acquisitions or prove that authority cannot change; undeclared
delegation or escalation refuses. Account explicitly for bootstrap/loader retained
authority in the trusted base and deployed configuration. The MVP grants no
generic authority-minting operation.

For a package artifact A, let Actual(A) be its conservatively extracted
authority graph, alpha group implementation nodes into declared components, and
P be the approved rights graph. Require alpha(Actual(A)) to be a subset of P;
missing required edges are a separate correctness/liveness failure, not excess
authority. Require the reachable capability fixed point for each component to
be a subset of its approved grant. Authorization must both mediate the path and
bind the exact executed action/resource/principal; graph dominance alone does
not prevent substitution. Bind these checks to source, binary, build flags,
dependency identities and deployment configuration, with unestablished mappings
reported as gaps.

C4 checks capability containment **relative to supplied requirements and
policy**; sufficiency requires the separate required-operation and correctness
checks. It cannot infer the smallest real-world permission
set or prove that the requirements are correct. The owner must justify each
capability and its destinations; removing an unnecessary permission is a
reviewable policy refinement, not an assembler heuristic.

Keep three evidence levels distinct: (1) resolved source/graph checks establish
their declared structural properties; (2) mechanized semantic/refinement proofs
and protocol checks establish named properties under their assumptions; (3)
qualified OS/runtime configuration enforces actual file, process, network and
secret isolation. A source checker is not a whole-program proof, and an
application capability manifest does not itself sandbox a process. Compilers,
linkers, dependency resolution and unsafe platform adapters require explicit
trusted-base or correspondence accounting. The gate must report each level,
coverage gaps and unsupported host profiles separately. An OS enforcement gap
cannot be converted into a passing source-check result.

Extend A9 and A12 below with C1–C6 mutation controls, actual denied-operation
probes against the installed app, and exact-source proof replay where available.
The current scalar assembly model's capability Boolean only abstracts one
admission predicate; it does not extract this graph or prove these rules.

## Effect lifecycle and recovery contract

Reuse the existing outbox/driver and its correlated epoch, ordinal, port, store,
handle and subject checks. Add a domain-level status projection, not a parallel
transport scheduler. Distinguish delivery acknowledgement from domain completion.

```text
core accepts request -> atomic durable (state, intent, stable delivery ID)
Pending -> Running -> Succeeded | Failed | Unknown
Pending -> Cancelled, only before an irrevocable send is admitted
Running/Unknown -> CancelRequested -> Cancelled | Succeeded | Failed | Unknown
Unknown -> correlated reconciliation -> Succeeded | Failed | Unknown
```

These are a proposed bounded application's vocabulary, not replacements for
existing stable transport result variants. A production design must specify
which status changes are core transitions and which are projections of durable
protocol evidence; no shell-written status bypasses Authority.

- Persist approved intent before dispatch. Crash before publication sends
  nothing; crash after publication recovers pending work by original ID.
- Preserve existing V2 recovery ordering: after reopening and auditing, a found
  exact publication replays without the original version guard; an absent key
  retries with the original guard. V2 checks that guard before replay lookup.
- Retain request identity and canonical payload. An exact replay returns the
  original result; changed payload under the same key refuses. A new attempt is
  an explicit core decision with a new attempt identity, not an automatic way
  to bypass deduplication.
- Every domain receipt binds producer, artifact, revision, attempt, effect ID,
  payload digest and result schema. Wrong/stale/reordered receipts reject
  unchanged. Retain such reports for bounded audit/reconciliation; ignoring an
  old attempt's success must not justify an unsafe new attempt. Store producer
  authentication evidence and verification-policy identity separately from the
  receipt content digest. A digest is not a signature; authenticating a producer
  does not make its report truthful.
- An optional durable inbox holds untrusted observations only. Validate the
  receipt through the core, then atomically publish its accepted evidence and
  domain-state transition through nominal Authority. Shell receipt persistence
  alone grants no authority. A crash
  between sink application and local acknowledgement is **Unknown**, never an
  invented failure or success. Timeout is an observation, not proof of failure.
- Pure policy controls retry count/deadline/backoff and cancellation permission;
  shell Clock/Wait/Random handlers supply only bounded observations. Exhaustion
  remains explicit. Restart does not reset the retry budget.
- Cancellation before send and dispatch contend on one authoritative revision.
  After a possible send, cancel requests cannot promise undo. A late success
  may win according to the authored policy. Reconciliation and compensation are
  distinct first-order operations, with their own authorization.
- External exactly-once behavior requires a cooperating durable deduplicating
  sink or shared transaction. Otherwise retain at-least-once/Unknown semantics;
  operator reconciliation cannot silently relabel history.

The acceptance harness must inject interruption before/after commit, send,
receiver application, receipt persistence and acknowledgement. Test actual
SQLite/adapter restart, not only a Python state machine or completed-request
restart. Storage durability still depends on the qualified SQLite, filesystem,
OS and hardware profile; neither a hash nor a transaction API proves power-loss
behavior.

## Complete-app requirements and acceptance map

All rows are proposed implementation gates. The bounded models in this PR cover
only the explicitly identified abstraction; they do not mark these rows done.

| ID / requirement | Design integration | Required acceptance evidence |
|---|---|---|
| A1 usable forms and tables | Core-derived actions/validation; scalar and collection renderer lanes; stable row IDs; explicit empty/loading/error states | Actual browser create/edit/search journey; validation and refusal reasons; keyboard-only operation, focus order, labels, screen-reader status and contrast checks. No hidden privileged actions. |
| A2 stale and repeated actions | Consent binds principal, policy, revision and exact command; edits invalidate it; server rechecks Authority | Double click, lost reply, sign-out/in, Back/Forward, dismissal and stale snapshot. Exact retry recovers; changed request/key and stale revision refuse. Accessible pending state never implies failure. |
| A3 identity and authorization | Shell authenticates; pure core authorizes each command and query scope | Wrong/expired credential, wrong issuer/audience, principal change, revoked role and cross-tenant row all deny. Authenticated transport alone cannot promote or read another tenant. |
| A4 real effects and status | Existing durable outbox plus declared adapter acknowledgement/reconciliation contract | Real test adapter with durable ledger; pending/status/retry/cancel; wrong artifact/attempt; duplicate/late response; crash at every boundary; no nondurable dispatch. Unknown remains visible. |
| A5 composition to runnable package | Profile intersection checks and existing aggregate/factory routes | Supported aggregate checks, builds, installs and runs; `/3`, channels/outbox and unsupported descriptors refuse where the route does not support them. Same-step successor assumptions rejected. |
| A6 data queries and pagination | Pure query specification and authorization; bounded store adapter; cursor binds principal, query, schema and snapshot revision | Stable sort with unique tie-break; no missing/duplicate rows across pages of a fixed snapshot; expired/rebound cursor refuses; filter/limit bounds; concurrent writes yield declared restart/snapshot behavior. No arbitrary SQL. |
| A7 schema upgrades | Explicit versioned core migration, compatibility check and atomic store switch | Old executable/schema refuses unsupported store unchanged; valid upgrade preserves declared invariants; interruption rolls back or resumes by recorded phase; no implicit re-adoption as migration. |
| A8 backup and restore | Audited consistent snapshot plus journal/root/schema identities; secret handling outside export | Restore to a fresh destination, replay and compare complete state/outbox; corrupt/missing data refuses without altering destination; pending effects retain IDs and reconcile rather than resend under new IDs. |
| A9 configuration and secrets | Typed config, secret references resolved by restricted shell, no credentials in assets/logs | Invalid/missing config fails before service; least-privilege file/network/process permissions; secret rotation and revoked access; redaction checks across logs, archives and errors. |
| A10 health and operations | Separate liveness/readiness/backlog; bounded structured logs with correlation IDs; no business authority in health | Disk full, unavailable adapter, stuck Unknown, denied secret and overload are observable without false readiness; retention/backpressure tested; operator recovery follows core authorization. |
| A11 reproducible installation | Existing packaged-source qualification, reviewed exact dependency graph and host profile | Two clean supported builds compare declared deterministic outputs; fresh offline install and startup; moved paths/changed bytes refuse; uninstall leaves operator-owned data intact. |
| A12 limits and evidence | Manifest-bound capacities, checker budgets, adapter premises and source identities | Exact boundary and over-limit tests; evidence substitution/missing premise/unsupported version refuse; incomplete search is not a pass; full relevant runtime tests remain required. |

For query cursors, the core decides allowed projection/filter/order and authorizes
the request; the shell executes that bounded query against the declared snapshot.
Fetched rows are observations, not trusted proof of completeness. Qualify
adapter correspondence separately. That qualification may use mechanized
implementation-refinement proofs of maps, records or journals against abstract
specifications, including parameterized or unbounded properties where proved.
The finite models here are a design aid, not a ceiling on attainable assurance.
A data-structure correctness proof must still be composed with the actual
concurrency, storage durability and authentication contracts. Backup/restore of a store containing pending
external work must state whether it is disaster recovery or cloning; cloning a
live effect identity domain is refused by default.

## Implementation sequence and future work

1. Specify the compatibility table and exact bounded assembly schema; add
   negative fixtures before implementation. Retain existing factory routes.
2. Qualify one scalar and one collection reference app independently. Add usable
   UI/query status and configuration documentation for the actual schema, not
   copied example contexts. Implement A1–A3 and A6 before calling them usable.
3. Qualify one non-value-moving durable adapter with A4 fault injection, status
   and reconciliation. Integrate existing pure effect programs; do not substitute
   a new retry loop in the shell.
4. Close A5 and A7–A12 with installed artifacts and operating instructions. Only
   then describe that named profile as complete, under recorded assumptions.

Future profiles may support `/3` hosting, multi-store protocols, broader
identity providers and UI/data models. Each requires its own refinement,
authorization, migration and recovery obligations. Progress/fairness, resource
isolation and hostile multi-tenant security need separate evidence; finite
safety checks do not establish liveness or an OS sandbox.

## Why this structure; research basis

These papers motivate obligations, not claims that they prove ZenoFCIS:

- [VOC: A Methodology for the Translation Validation of Optimizing Compilers (Zuck, Pnueli, Fang and Goldberg)](https://www.jucs.org/jucs_9_3/voc_a_methodology_for.html): validate the particular transformation's semantic relation. A matching manifest hash is only identity evidence. Here, replay core/aggregate/UI correspondence for the emitted artifact.
- [A Compositional Semantics for Verified Separate Compilation and Linking](https://flint.cs.yale.edu/certikos/publications/vscl.pdf) and [Certified Concurrent Abstraction Layers](https://flint.cs.yale.edu/certikos/publications/ccal.pdf): component assumptions and behavioral interfaces must compose. Equal types or independent component passes are insufficient.
- [Verdi](https://homes.cs.washington.edu/~ztatlock/pubs/verdi-wilcox-pldi15.pdf): verified system transformers preserve stated properties under their model. Its extraction/shim boundary illustrates why our real adapter correspondence remains an explicit obligation, not a consequence of a finite model.
- [GoJournal](https://www.usenix.org/system/files/osdi21-chajed.pdf): atomicity and durability differ; durability has a synchronization boundary and storage assumptions. Dispatch needs the promised durable commit, not merely buffered acceptance.
- [CAmkES/capDL architecture translation validation](https://sel4.systems/Research/pdfs/formally-verified-software-real-world.pdf): compare generated low-level authority structure with the declared component policy. Its historical initializer result is about a model, not a proof of the initializer C implementation; it is not a generic end-to-end guarantee for our packages.
- [seL4 integrity](https://sel4.org/Research/pdfs/sel4-enforces-integrity.pdf): capabilities bound authority under a specified policy. A least-privilege adapter does not establish business authorization or truthful external observations.

Compared with a new runtime framework, this design retains one decision path,
one authoritative durable store per MVP app, the existing outbox and one factory
entry point. The added concepts are a compatibility description and qualification
profile, not another orchestrator. Existing acceptance checks are retained;
none is removed on the grounds that a smaller design is automatically correct.

## Evidence and nonclaims for this change

The executable design fixtures are described in
[`examples/runtime-assembly-design/README.md`](../examples/runtime-assembly-design/README.md).
Their native checker runs exercise bounded laws and negative cases; Boolean
matches abstract actual hashes, authenticated identities and durability facts.
They help expose specification mistakes, but do not implement those boundaries.

This change adds no production adapter, UI, authentication provider, query
engine, migration mechanism or runnable composition bridge. It does not change
crate APIs, stable protocol identifiers, dependencies or release versions.
Native model checks, independent reference checks, runtime integration tests and
browser checks are distinct evidence categories. A model pass is not a completed
app checklist, production qualification, independent formal proof or deployment
permission.
