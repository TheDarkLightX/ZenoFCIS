# Proposed runtime isolation and application evidence for 2.7

[Implementation checklist and handoff](RUNTIME_ISOLATION_27_PROGRESS.md).

**Status: draft design; no new runtime guarantee or production qualification.**

This proposal gives library users a path from an approved decision to a
qualified installed application. It extends the existing authority, effect
programs, driver, policies and SQLite outbox with a protected effect owner,
isolated workers and explicit implementation evidence. Every requirement below
is proposed unless identified as existing source behavior.

## Baseline and purpose

The integration baseline is
[`b7d6fb31258146130f0adfdde4348b88282433cf`](https://github.com/TheDarkLightX/ZenoFCIS/tree/b7d6fb31258146130f0adfdde4348b88282433cf).
Its [architecture](ARCHITECTURE.md), [shell effects](SHELL_EFFECTS.md),
[simulation](SHELL_SIMULATION.md) and
[SQLite refinement](SQLITE_SHELL_REFINEMENT.md) describe the reusable foundations
and their limitations. In particular, effect declarations and response
correlation do not authenticate an external outcome or establish an OS boundary.

Separate public snapshots supply relevant design context:

| Snapshot | Relevant scope |
|---|---|
| [PR146 source, `0491f44cd443`](https://github.com/TheDarkLightX/ZenoFCIS/blob/0491f44cd44399ad321de77ada6da89d61b64e23/docs/SHELL_EFFECTS.md) | Typed effect programs, production adapters and bounded evidence |
| [PR147 design, `84c5329e6636`](https://github.com/TheDarkLightX/ZenoFCIS/blob/84c5329e663610e1360e15dbbadeadb0b240330f/docs/RUNTIME_ASSEMBLER_DESIGN.md) | Proposed application assembly and conformance obligations |
| [PR148 prototype, `cf31c71f2a1a`](https://github.com/TheDarkLightX/ZenoFCIS/blob/cf31c71f2a1a648bbea2176f177aad467e390057/docs/RUNTIME_ASSEMBLY.md) | Restricted local assembly profiles; explicitly not an OS sandbox |
| [PR149 pilot, `c504c9b8a762`](https://github.com/TheDarkLightX/ZenoFCIS/blob/c504c9b8a762a27110d40aa9d9ec1c34d56c9bae/docs/RUNTIME_REFINEMENT.md) | Finite model refinement; executable assembly correspondence remains open at that revision |

The three runtime documents are absent from the integration baseline. Their
presence in separate snapshots does not establish integration here, and this
review does not establish which gaps remain in later revisions. Implementation
must first reconcile the selected revisions and rerun affected checks.

The first deliverable should support one useful application profile. Broad
portability, arbitrary plugins and new storage engines would enlarge the task
before its central claim is established.

## 1. Declare the profile and its claims

Start with one Linux host, one protected publication owner, one SQLite state
domain, bounded isolated workers and a local durable deduplicating receiver.
The receiver records an approved intent; that alone does not establish that a
job ran or a physical action succeeded. Production value-moving or remote
provider profiles require separate qualification.

The profile trusts the declared kernel, administrator, loader, compiler,
SQLite/filesystem interface and qualified owner dependencies. It treats worker
code and worker observations as potentially malicious. Host administrator or
kernel compromise is outside this initial claim. No worker receives database
credentials, signing authority, inherited privileged descriptors or generic
authenticated network access.

Let `P` fix the model, implementation, platform, permissions, fault assumptions,
observation function and resource bounds. The safety target is:

> For every worker behavior permitted by P's enforced boundary, and every
> covered execution under P's environment assumptions, the application's
> observable trace refines the approved protocol model.

Observations include admission and refusal, exact dispatch attempts, receiver
application where covered, acknowledgements, uncertainty, cancellation and
crashes. Projection must not hide an unauthorized effect. Worker-written logs
are observations to validate, not the independent oracle for this claim.

Publish five separate statuses: decision semantics; production protocol
refinement; confinement; artifact and deployment correspondence; named
confidentiality and progress properties. Each states `proved`, `tested`,
`assumed` or `open`, with its exact scope. A successful model comparison cannot
promote an implementation or deployment claim.

Every implementation profile must fill in numerical bounds for frame sizes,
queue lengths, retained intents, attempts, execution fuel, memory, processes,
descriptors, storage, host-call deadlines and cleanup time. Missing bounds or
unsupported enforcement refuse profile admission. These are distinct resources;
guest fuel cannot bound a blocked host call.

## 2. Put exact dispatch authority in the protected owner

The existing driver checks declared capabilities, schemas and response bindings.
Those declarations describe required effects; they are not grants supplied by
an authenticated principal. The reviewed
[production Send branch](https://github.com/TheDarkLightX/ZenoFCIS/blob/0491f44cd44399ad321de77ada6da89d61b64e23/crates/zeno-fcis-shell-sqlite/src/effects/production.rs#L471)
forwards caller-supplied delivery data under its trusted-program/relay premise.
This proposal changes the boundary for admitting arbitrary workers; it does
not assert a demonstrated remote exploit in that source.

The worker-facing operation should carry an opaque intent handle. The owner
resolves it using an authenticated caller and a scoped handle table, or an
equivalently qualified capability scheme. A serialized Rust value or an
unpredictable identifier alone does not supply authorization.

Before dispatch, the owner must check that:

- the handle belongs to this deployment, tenant, principal and state domain;
- it resolves to an original durable approved intent with exact content;
- the current grant permits this action, resource and lifecycle transition;
- the attempt and resource reservation are admitted under the selected policy;
- the credential-bearing executor will perform precisely that operation.

The owner retrieves destination and payload from the retained intent. Workers
cannot replace either with caller-supplied bytes. Revoked, nonpending,
cross-domain and substituted handles fail before transport invocation. General
store writes and access to the underlying transport remain internal.

A hostname allowlist is insufficient for an API that accepts many operations.
For a future network profile, the qualified mapping must bind the effective
method, endpoint, path, relevant headers and body, including redirect behavior.
An untrusted formatter may propose bytes; a checked provider mapping validates
them before the protected executor supplies credentials. If that mapping cannot
be checked, the adapter remains a named trusted component.

Keep decisions in pure functions. The owner installs approved successors and
performs bounded mechanisms. Adding a second independent policy language or
authorization engine requires evidence that the added state and interactions
are necessary; reuse existing contracts and policy evaluation where adequate.

## 3. Separate permission, attempts and outcome knowledge

Retain distinct identities for the logical intent, content, deployment/tenant,
grant version, owner incarnation and transport attempt. A retry keeps the
logical intent identity and exact content. Neither a restart nor a timeout
creates a new business operation.

| Transition | Proposed rule |
|---|---|
| Dispatch | Admit under current authority and durably reserve the required resources before handing off the effect |
| Revoke | Stop later dispatch admissions under the revoked grant; preserve knowledge of earlier attempts |
| Observe | Validate evidence against the original intent; update its knowledge without advancing another continuation |
| Retry | Preserve logical identity and follow the profile's declared deduplication and attempt policy |
| Cancel | Before possible dispatch, establish no effect; afterward retain cancellation requested or uncertainty until evidence resolves it |
| Recover | Restore identities, reservations and uncertainty before admitting another effect |

The selected dispatch policy matters. Existing `AtMostOne` reservations can
prevent a second protocol invocation, at the cost of possible nondelivery.
Retrying for delivery requires a cooperating receiver whose effect and
deduplication record are atomic and retained for the allowed replay lifetime.
Neither policy establishes both unconditional eventual delivery and at-most-once
external execution. Safe reclamation of deduplication records is itself an
obligation; a retention timeout cannot silently reset identity.

An authenticated success report identifies its reporter. It does not prove
the physical outcome. Profile contracts must distinguish reported success,
validated completion and unresolved outcome. `NotSent` requires adequate
non-invocation evidence; an untrusted assertion cannot justify unsafe retry or
release of a reservation. After possible execution, failed cancellation or a
late local failure cannot erase `Unknown`.

Revocation and completion have different rules: an authentic late result may
resolve an old intent after its dispatch grant was revoked. It must not complete
a replacement attempt. A test must cover dispatch A, revoke A, start B, receive
A's valid completion, and verify both preserved knowledge and correct correlation.

### Ownership, cloning and fencing

A lock on one local file does not protect two independently restored copies.
Per-intent deduplication also cannot prevent divergent copies from authorizing
different operations against the same external resource. The first profile
therefore permits one active owner and requires a controlled restore procedure.
Live cloning and automatic failover remain unsupported.

A later failover profile needs recipient-enforced ownership fencing or an
equivalent protocol. Keep its generation separate from logical intent identity.
A highest-observed generation rejects older requests after the recipient has
observed a newer one; it does not establish instantaneous revocation everywhere.
Stronger claims require an atomic ownership check at the effect boundary.
Chubby's [recipient-checked sequencers](https://www.usenix.org/legacy/event/osdi06/tech/full_papers/burrows/burrows_html/)
provide a useful precedent, not an implementation for this profile.

## 4. Deliver in five phases

### Phase 1: freeze the contract and evidence closure

Select the exact source revisions, application contract, effect profile and
trusted dependencies. Define an independently reviewed required-artifact and
required-check inventory. The candidate manifest supplies hashes for those
items; it must not decide which items are required. Check exact set equality,
regular file kinds, path containment, duplicate entries, schema versions and
declared byte limits before consuming evidence.

Bind compiler and linker, target triple, dependency/build-script closure,
features, optimization and panic settings, environment/configuration, runtime
and ABI glue. Run copied-executable journeys against the exact shipped build
configuration, and check the correspondence between test and release feature
selection. Record proof inputs and the executed binary separately. Hashes
identify bytes; authenticated provenance requires a separate trust root, and
neither establishes behavior preservation.

Python acceptance conditions must use explicit checks that remain active under
normal execution and `python3 -O`. Empty inventories, omitted mandatory files,
unknown fields, duplicate records, stale outputs, timeout and inconclusive
results must not produce a success receipt. The checker must run safely against
untrusted paths and data without executing packet-supplied commands.

### Phase 2: establish intent dispatch and lifecycle correspondence

Implement the pure admission/lifecycle relation and its owner-facing boundary.
Prove that each admitted dispatch selects an approved pending intent, retains
its exact content and consumes only the allowed authority. Connect that result
to the actual transport call; a proof about an unused helper is insufficient.

Use a receiver and event observer independent of the admission implementation.
Preserve valid dispatch and original-ID recovery journeys alongside negative
controls. A policy that rejects everything must fail qualification.

### Phase 3: prove one Start publication and qualify recovery

The first implementation proof should relate one actual immutable approved
Start candidate to publication of its successor state, revision, accepted
evidence, replay binding and exact intents in one transaction. Frame unrelated
entries and prior snapshots. Preserve business rejection, technical refusal,
resource exhaustion and uncertain physical outcomes as different observations.

Specify the SQLite transaction interface as an explicit premise, then prove
the wrapper/protocol relation against it. A known pre-invocation refusal leaves
the semantic projection unchanged. An uncertain write may recover to the
approved pre-state or complete committed successor; do not assume every failed
call left the database unchanged.

Extend this relation through reopen, receipt recovery and receiver/ACK handling.
Inject failure before and after every persistence boundary, including recovery
itself. Process aborts establish process-loss evidence. Disk-full, partial I/O,
fsync failures and power loss require their own fixtures or remain assumptions.
PoWER's [crash-recoverability method](https://www.usenix.org/system/files/osdi25-leblanc.pdf)
is relevant to the proof structure without replacing SQLite qualification.

Startup must be a specified protocol too: prepare bounded configuration, bind
existing store and reservation state, then activate dispatch. A missing or
corrupt retained checkpoint must not trigger fallback initialization. Partial
first installation must recover or refuse explicitly. No effect is admitted
until all required bindings and the single-owner condition hold.

### Phase 4: qualify the installed protection domains

One supervisor must own worker processes, descendants, descriptors, queues and
resource limits, including cleanup after a requesting process dies. The covered
supervisor must survive the failure for which cleanup is promised. Killing a
worker does not undo a remote effect.

Lower the profile into enforced credentials, filesystem views, process/ptrace
rights, syscall restrictions, IPC and network policy. Deny unsupported platform
combinations before launch. Use the
[Linux confinement interfaces](https://docs.kernel.org/userspace-api/landlock.html)
according to their distinct scopes; no individual mechanism covers every right.
Test inherited descriptors, alternate entry points and colluding workers.

Run probes against the copied installed executable under its actual account,
mounts and configuration. Verify both denied operations and approved work.
Do not infer deployed confinement from source scanning or a development run.
Wasm workers are an optional separate profile; the
[runtime and host functions](https://docs.wasmtime.dev/security.html) enter its
trusted base. Native build tools and dependency scripts need isolated factory
workers without production credentials or verifier/signing write access.

### Phase 5: qualify installation, change and operation

Installation, upgrade, backup and restore must preserve semantic bindings, not
just package hashes. Upgrade admission must relate old/new contracts, codecs,
grants, reservations, pending intents and receiver identities. Preserve original
operation meaning across the switch, or refuse migration with an explicit reason.
Crash recovery must select one coherent activation state. Never silently adopt
or reinterpret retained data under a new package.

Backups must describe consistency across state, outbox and reservation stores.
Restoration must preserve their relation and original IDs; restoring an older
internally consistent database requires an independent freshness anchor for any
anti-rollback claim. Prove or qualify retained-history/work bounds and handling
of exhausted storage. An administrative bypass changes the profile and cannot
reuse its qualified identity.

The user-facing assembly report should show permissions, outcome meanings,
resource limits, evidence status and unsupported operations. ZAL's abstraction
library can present these views from the same checked contract. New permissions
and adapters require explicit review; already qualified components can reuse
unchanged, revision-bound evidence.

## 5. Validation matrix and proof scope

Before each work item starts, freeze its executable domain manifest, oracle,
resource budget and required mutations. The shared initial exploration fixture
uses two principals, two deployment identities, two logical intents, two grant
versions, two owner generations, two attempt slots per intent and a transport
queue of two. Include
valid and invalid states, delayed/duplicate observations, every modeled crash
transition and all declared result variants. These are proposed test bounds,
not production capacity limits or a completed experiment.

State fingerprints must retain every field that can affect future behavior;
justify any projection of history or observations. Freeze exact finite payload,
destination, clock and failure alphabets before claiming exploration closure.
Explore the complete reachable closure when finite. If a depth/work limit is
used, report that exact limit, remaining frontier and `incomplete` status.
Symbolic or inductive claims quantify over the complete declared admitted
domain; finite tests retain their narrower quantifiers.

| Work item | Quantifier, bounds and observations | Independent oracle and required falsifier |
|---|---|---|
| E1 Evidence admission | Every candidate within published format/byte bounds; observe accept/refuse and receipt creation in normal and optimized Python | Reviewed required inventory and fixed malformed corpus; remove a required item and its manifest entry, disable an acceptance condition, truncate a result |
| E2 Dispatch | Every bounded request, including invalid bindings, and reachable owner state; fixture above for exploration; observe exact executor calls and reservations | Separate intent-table specification plus actual call recorder; forge principal/handle, substitute content, bypass policy or reject all valid work |
| E3 Publication/recovery | Every related candidate/store state under the storage contract; enumerate every named crash boundary; observe complete durable projection and outcomes | Independently authored protocol fold plus real SQLite snapshots; omit an intent, relax replay guard, collapse Unknown, reinitialize missing state |
| E4 Confinement | Each declared authority edge and resource boundary on each exact supported platform; test limit-1, limit, limit+1 where meaningful | OS probes and surviving supervisor observations; leak a descriptor, retain a descendant, allow direct credential-bearing egress or disable enforcement |
| E5 Operation/change | Every admitted migration/restore transition under its relation; finite old/new fixtures plus all activation crash points | Independent before/after semantic projections and receiver ledger; alter pending identity, revive a grant, activate two clones or accept a stale backup |

Boundary mutations must reach the intended semantic check. A compilation error,
parse failure or stale hash alone does not establish that an authorization or
recovery guard works. Rehash semantically mutated fixtures when required to
exercise deeper checks. Restore and rerun the positive baseline. Record exact
feature selection, selected tests, executed counts and omitted cases.

### Confidentiality and progress require separate arguments

Ordinary trace inclusion does not imply confidentiality. A model permitting
either public bit can be refined by an implementation that always emits a
secret bit. Define low observations and permitted declassification, then compare
paired executions with equal public inputs and differing secrets. Include the
declared scheduler model, logs, errors and output sizes; timing and hardware
channels are excluded unless separately qualified. This is a
[hyperproperty obligation](https://www.cs.cornell.edu/fbs/publications/Hyperproperties.pdf).

Progress must state fairness, eventual receiver availability, recovery and
resource premises. Check positive completion and interrupted/recovered journeys,
then supply a ranking or temporal argument for any broader claim. Permanent
loss, starvation and arbitrary worker refusal preclude unconditional liveness.
Safety, bounded exploration completion and successful business completion are
different results.

## Review and completion criteria

Each phase produces a reviewable implementation change and scoped evidence;
none is completed by this document. Promotion requires the actual installed
path, its model/implementation relation, positive journeys, intended negative
controls and unchanged assumptions to agree. Reuse prior evidence only with an
explicit dependency-impact review; changed semantics, targets or features
invalidate the affected claims. Follow [release assurance](RELEASE_ASSURANCE.md)
and [public documentation policy](PUBLICATION_POLICY.md) for delivery.

This proposal establishes no end-to-end proof, hostile-worker containment,
external exactly-once execution, power-loss durability, multi-tenant deployment,
automatic failover, universal constant-time behavior or production value-moving
profile. Those remain explicit work items with separately bound evidence.
