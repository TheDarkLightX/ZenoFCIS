# Runtime isolation: 2.7 implementation checklist

**Development status: in progress; draft PR [#150](https://github.com/TheDarkLightX/ZenoFCIS/pull/150).**
The [proposal](RUNTIME_ISOLATION_27_PROPOSAL.md) defines the intended guarantees.
Checked items below mean the stated bounded deliverable and its listed checks
are complete. They do not promote another phase or establish deployment safety.

## Source and claim boundary

- Integration base: [`b7d6fb31258146130f0adfdde4348b88282433cf`](https://github.com/TheDarkLightX/ZenoFCIS/tree/b7d6fb31258146130f0adfdde4348b88282433cf).
- Initial proposal: [`683aabf5800f39a083d218b0387ab432ce0abc1c`](https://github.com/TheDarkLightX/ZenoFCIS/commit/683aabf5800f39a083d218b0387ab432ce0abc1c).
- 2.6 comparison source: [`dbbdec4fb58cf630b72c8821449b72896dd682aa`](https://github.com/TheDarkLightX/ZenoFCIS/tree/dbbdec4fb58cf630b72c8821449b72896dd682aa), a pinned `release/2.6.0` branch snapshot.
- Companion experiments: [PR #151](https://github.com/TheDarkLightX/ZenoFCIS/pull/151) and its [checklist](https://github.com/TheDarkLightX/ZenoFCIS/blob/draft/2.7-abstract-machine-discovery/docs/ABSTRACT_MACHINE_DISCOVERY_27_PROGRESS.md).

The first implementation increment is an opt-in, in-process owner boundary.
It must retain the actual `Production` terminal and existing policy stack,
resolve an owner-bound handle to an approved pending record, and dispatch its
exact retained bytes. Authentication of the caller at ingress, IPC confinement,
OS isolation, and durable grant restoration are separate tasks.

## Checklist

- [x] **I0 — Publish the scoped design.** Five phases, explicit assumptions,
  independently observable effects, positive journeys, mutation controls, and
  separate confidentiality/progress requirements are recorded in the proposal.
- [x] **I1 — Pin source baselines.** Record integration and 2.6 comparison
  commits without treating a moving branch name as evidence identity.
- [ ] **I2 — Add the protected intent-owner API.** In progress. Private
  construction of owner-bound handles; narrow caller session; grants bind caller
  and exact retained intent; revocation checked before dispatch; no mutable
  terminal or general transport access through the worker-facing API.
- [ ] **I3 — Integrate the real pending-intent resolver.** Reload and validate
  the actual SQLite intent immediately before dispatch, preserve existing
  `AtMostOne`/retry policy behavior, and reject stale or acknowledged records.
  A new independently maintained outbox must not substitute for the real store.
- [ ] **I4 — Exercise positive and hostile paths.** Valid original-ID dispatch,
  wrong principal, different owner, revoked grant, changed content, stale state,
  repeated calls, and policy refusal; use an independent transport-call recorder.
- [ ] **I5 — Review authority and resource limits.** Bound grant/handle state,
  define ownership and restart behavior, inspect all ways to reach dispatch,
  and run relevant Rust tests, formatting, linting, and package checks.
- [ ] **I6 — Add the next boundary after the owner.** Select a concrete IPC
  authentication and process-confinement profile, with enforced rights and
  independent OS probes. An in-process API does not complete this item.
- [ ] **I7 — Make publication/recovery correspondence executable.** Relate one
  approved Start to exact state/revision/evidence/intents, then fault-inject
  startup, dispatch, receipt and acknowledgement persistence boundaries.
- [ ] **I8 — Qualify grants, upgrade and restore.** Define durable authority,
  freshness, pending-meaning preservation, single-owner activation and late
  evidence handling. Independently restored active copies remain unsupported
  until a qualified fencing protocol exists.
- [ ] **I9 — Integrate the qualified path into an installed application.**
  Copied-executable journeys must cover the exact build/features, account,
  filesystem, process supervision and resource policy. Include useful successful
  work; refusal of every request cannot pass.

## Current implementation work

The planned first files are `crates/zeno-fcis-shell-sqlite/src/effects/owner.rs`
and `crates/zeno-fcis-shell-sqlite/tests/effects_owner.rs`, with a narrow resolver
in the existing production adapter. These paths are a work allocation, not a
claim that the module or its tests already exist.

Keep legacy trusted-terminal behavior explicit while the owner route is opt-in.
Do not describe this increment as containing arbitrary native worker code.
A grant for an old operation must not make a new operation equivalent to it;
unknown delivery outcome is not a fresh retry authorization.

## Validation record

At the initial proposal commit, document inventory/link checks, bounded privacy
checks and whitespace checks passed; the public contents received independent
review. No owner implementation, native owner test or process-isolation result
is claimed yet. Add the actual commands, counts, platform and source identities
here when I2–I5 have reproducible results. Hosted CI results are separate.

## How to continue a task

Choose an unchecked task ID and use a separate branch based on the current PR
head. State the API/files to be changed, the exact admitted and hostile inputs,
the expected observation, and the independent oracle before implementation.
Coordinate shared adapter changes before editing; keep worker confinement,
storage correspondence and grant persistence independently reviewable.

Before checking an item, record its exact source revision, dependency and
platform assumptions, replay commands, positive witnesses, intended mutations
and remaining gaps. Update this checklist in the same reviewed increment.
Source drift, skipped checks and inconclusive results leave the affected item
unchecked. Follow [repository guidance](../AGENTS.md),
[contributing](../CONTRIBUTING.md) and
[publication policy](PUBLICATION_POLICY.md).
