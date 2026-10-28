# Runtime isolation: 2.7 implementation checklist

**Development status: in progress; proposal [#150](https://github.com/TheDarkLightX/ZenoFCIS/pull/150).**
The [proposal](RUNTIME_ISOLATION_27_PROPOSAL.md) defines the intended guarantees.
Checked implementation items below identify source and test coverage present in
this development checkpoint. They do not record a fresh pass on the final
checkpoint, qualify an installed application or establish deployment safety.
Exact-head native checks and the remaining I5–I9 work are pending.

## Source and claim boundary

Use the exact reviewed commit and dependency lockfile for continuation. Record
`git rev-parse HEAD`, the working-tree state, features, toolchain, target and
platform with each check. A moving branch, this checklist or an earlier local
result cannot identify a later checked build. No private run packet or local
validation result is published by this document.

The implementation provides an opt-in, in-process `TrustedDispatcher` boundary.
This is software that enforces approved dispatch authority; it is not the human
who sets the application's policy. The canonical API is
`effects::dispatcher::TrustedDispatcher`; `effects::owner::Owner` remains a
compatibility alias.
It retains the actual `Production` terminal and existing durable policy relay,
resolves a dispatcher-bound handle to an approved pending record, and dispatches its
exact retained bytes. Authentication of the caller at ingress, complete startup/installed protection
qualification, and durable grant restoration remain required tasks.

## Checklist

- [x] **I0 — Publish the scoped design.** Five phases, explicit assumptions,
  independently observable effects, positive journeys, mutation controls, and
  separate confidentiality/progress requirements are recorded in the proposal.
- [ ] **I1 — Qualify the final source snapshot.** Record the exact reviewed
  checkpoint, dependencies, tools and features with its completed checks.
  Historical comparison results do not qualify the assembled head.
- [x] **I2 — Add the protected intent-dispatch API.** Private
  construction of dispatcher-bound handles; narrow caller session; grants bind caller
  and exact retained intent; revocation checked before dispatch; no mutable
  terminal or general transport access through the worker-facing API.
- [x] **I3 — Integrate the real pending-intent resolver.** Reload and validate
  the actual SQLite intent immediately before dispatch, preserve existing
  `AtMostOne`/retry policy behavior, and reject stale or acknowledged records.
  A new independently maintained outbox must not substitute for the real store.
- [x] **I4 — Add positive and hostile tests.** Cover valid original-ID dispatch,
  wrong principal, different owner, revoked grant, changed content, stale state,
  repeated calls, and policy refusal; use an independent transport-call recorder.
- [ ] **I5 — Review authority and resource limits.** Bound grant/handle state,
  define ownership and restart behavior, inspect all ways to reach dispatch,
  and run relevant Rust tests, formatting, linting, and package checks. The
  source provides explicit limits and registered checks; their exact-head
  execution and assembled release package qualification remain pending.
  Package inventory alone does not close this item.
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

The trusted dispatcher implementation is in
[`owner.rs`](../crates/zeno-fcis-shell-sqlite/src/effects/owner.rs), with
[`real-store tests`](../crates/zeno-fcis-shell-sqlite/tests/effects_owner.rs)
and a narrow audited resolver in the existing production adapter.
The [`channel adapter`](../crates/zeno-fcis-shell-sqlite/src/effects/channel.rs)
accepts fixed 16-byte requests containing only a sequence and a slot into an
immutable supervisor-held handle table. Trusted administration binds the identity
and handles; no identity, destination or payload arrives in a worker request.
Malformed frames, replay, missing slots, exhausted budgets and unwinds stop that
channel. Current grants and durable reservations still govern every dispatch.
The supervisor must protect the IPC endpoints, enforce framing and deadlines,
and contain native workers. This adapter alone does not complete I6.

The Linux x86-64
[`native supervisor`](../crates/zeno-fcis-shell-sqlite/src/effects/native_worker.rs)
now owns a fixed channel and dedicated worker/control connections. Its embedded
[`bootstrap`](../crates/zeno-fcis-shell-sqlite/src/effects/native_worker.py)
checks pinned root-owned runtime files, copies the image and GNU libraries into a
read-only filesystem, installs the syscall/namespace policy and bounds resources.
Worker stderr is captured separately from trusted supervisor status. Closing the
connection never rolls back a possible transport call or clears its reservation.

The required `native-worker-host` ATDD scenario builds an independent
[`native probe`](../crates/zeno-fcis-shell-sqlite/tests/native_worker/probe.c)
and exercises the actual SQLite dispatch path through the supervisor. Its host
journeys include useful dispatch, revocation, forgery/replay, lost replies,
unwind, resource pressure and requester death after readiness. The
[`qualification command`](../tools/check_native_worker.py) checks its fixed test
inventory and normal/optimized Python bootstrap refusals; ignored host tests in
ordinary Cargo runs remain mandatory through this explicit ATDD route.

The bootstrap also binds its initial exec-chain child to the guardian with a
parent-death signal and compares the expected parent PID before and after
installing it. It refuses a multithreaded bootstrap before using that pre-exec
hook. The required [startup checker](../tools/check_native_startup.py) pauses an
isolated copy before, during and after linking, kills the guardian and observes
an independently compiled sleeping native leaf. It requires no living
descendants at those cuts in each Python mode. In each mode an unlinked control must reproduce
an actual sleeping `/worker` beyond the supervision budget; verified cleanup
then removes only that controller's owned descendants. Closing stdin or applying
a CPU limit alone would not establish cleanup for this witness.

I6 remains open for bubblewrap's internal fork/init lifetime cuts, complete
account/runtime qualification (including executable privilege metadata) and the
multiprocess factory-worker profile. The initial exec-chain cut tests and
post-readiness death test do not establish cleanup at every startup boundary.
I9 additionally requires the installed application route.


Trusted administration creates/revokes grants and supplies authenticated session
identities. Workers receive an opaque non-cloneable handle and a dispatch-only
session. A private owner-incarnation brand separates even same-genesis stores
with the same principal, intent and owner-local grant number. Grants retain the
complete approved delivery, and dispatch reloads the real SQLite pending record.
Grant count, retained intent bytes and recorded dispatch attempts have explicit
limits. The owner exposes no mutable terminal or policy state to a worker.

The protected profile uses the existing durable `AtMostOne` relay. Grant changes
cannot clear its reservations. Opening requires an existing source-, deployment-,
tenant- and store-bound reservation file; initialization is a separate explicit
operation. Restart invalidates all volatile grants and handles, while preserving
send reservations. Unknown outcomes, malformed success reports and transport
unwinds cannot authorize a second invocation. Same-file advisory locks do not
fence independently restored copies. Completion observation and durable grant
restoration remain I7/I8 work.

Keep legacy trusted-terminal behavior explicit while the owner route is opt-in.
Do not describe this increment as containing arbitrary native worker code.
A grant for an old operation must not make a new operation equivalent to it;
unknown delivery outcome is not a fresh retry authorization.

## Portable reproduction and pending validation

These are commands for the next reviewed checkout, not recorded passes. The
repository pins Rust 1.97.1; use Python 3.11+ and locked dependencies. `--offline`
requires the corresponding dependency cache to exist; a missing tool or cache
is a failed prerequisite, not permission to change toolchains or relax limits.

The runtime files, real-store tests, native probe and checkers are retained in
the repository. The
[`admission contract`](../crates/zeno-fcis-shell-sqlite/tests/contracts/intent-owner-admission/project.zeno)
and its independent examples specify all valuations of seven binding facts and
exact refusal precedence. The library test compares the native pure admission
function with the retained table. That relation alone does not check concrete
identity comparisons, storage, transport or process isolation.

Freeze the selected source before qualification and put build/evidence outputs
outside it. For example, from the reviewed repository root:

```sh
git rev-parse HEAD
git status --short
review_root=$(mktemp -d "${TMPDIR:-/tmp}/zeno-runtime-review.XXXXXX")
export CARGO_TARGET_DIR="$review_root/target"
python3 -B tools/check_native_worker.py --self-test
python3 -O -B tools/check_native_worker.py --self-test
cargo +1.97.1 test --locked --offline -p zeno-fcis-shell-sqlite --features crash-injection --test effects_owner
cargo +1.97.1 test --locked --offline -p zeno-fcis-shell-sqlite --features crash-injection --test effects_fencing
cargo +1.97.1 test --locked --offline -p zeno-fcis-shell-sqlite --lib effects::owner::tests::admission_matches_every_checked_contract_case
cargo +1.97.1 test --locked --offline -p zeno-fcis-shell-sqlite --features crash-injection --test effects_production dispatch
cargo +1.97.1 test --locked --offline -p zeno-fcis-shell-sqlite --doc effects::owner
cargo +1.97.1 clippy --locked --offline -p zeno-fcis-shell-sqlite --features crash-injection --lib --test effects_owner --test effects_fencing -- -D warnings
```

The ordinary owner target leaves the explicitly ignored host tests unexecuted.
Its passing summary would not supply native-worker qualification. Fencing tests
use a local fixture receiver and trusted fence administration; they do not
provide a deployed receiver-fencing protocol or physical-action evidence.

After separate review of the exact host, tool/runtime identities, owned output
paths, budgets and cleanup protocol, run the development host gate:

```sh
python3 -B tools/check_native_worker.py \
  --work-dir "$review_root/native-host" --target-dir "$CARGO_TARGET_DIR"
```

This command compiles the independent C probe and Rust tests and starts native
workers. It requires a supported unprivileged Linux x86-64 host with the pinned
runtime inventory, bubblewrap, prlimit, GNU runtime libraries, seccomp support
and a suitable C compiler. It refuses unsupported conditions. It checks the
required test inventory and both Python modes, including the dedicated startup
checker; do not run that checker without its enclosing reviewed inputs.
`python3 tools/atdd.py run --scenario native-worker-host` selects the same host
qualification route through the acceptance registry. These are alternative
entry points, not separate evidence to count twice.

I5–I9 remain open. In particular, the native gate is a development-host test,
not installed-release/account qualification, every internal bubblewrap
fork/init cut, executable privilege-metadata qualification, aggregate
multiprocess build-worker enforcement or general noninterference. Complete
publication/recovery correspondence, durable grant restoration, upgrades,
backup freshness and installed application journeys require separate evidence.

The assembler's copied-executable catalog journeys and the finite transport
refinement pilot are separate routes; see [assembly](RUNTIME_ASSEMBLY.md) and
[refinement](RUNTIME_REFINEMENT.md). Neither promotes this checkpoint to a
release. The final assembled 2.7 acceptance run and exact shipped
source/build/package checks remain pending.

Before publishing changes to these documents, follow
[publication policy](PUBLICATION_POLICY.md): inspect the full contents, confirm
public sources, keep the approved document inventory current, and run the
bounded documentation/link/privacy checks. Preserve private diagnostics outside
the publishing checkout.

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
