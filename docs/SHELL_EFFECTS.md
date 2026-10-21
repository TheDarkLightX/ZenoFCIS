# Shell effects: ports, pure programs, simulation and replay

An application's shell commits publications, recovers from a lost reply and
delivers outbox records. The aim is one shell that runs unchanged against real
storage, a deterministic faulty world or a recorded run. The shell's effects
are declared once, its decisions are pure programs, and handlers carry the
effects out. This page covers the registry, the programs, the simulation, the
record and replay layers, the explorers and the production SQLite handler.

## A commit whose reply is lost

A program asks to commit key K at version 3. The store applies the commit, but
the reply is lost, so the program sees `Unknown`. It reopens the store and reads
the audited history:

- If K is there with the exact publication, it retries without a version guard.
  V2 then answers with the original receipt, marked as a replay. Retrying
  `commit_at(3)` would be refused, because V2 checks the expected version
  before it looks up the replay key.
- If K is absent, it retries with the original guard, version 3. It never
  relaxes a guard because a record is missing.

After a crash there is no reply at all. The caller restarts the retained
request with `CommitProgram::recover`, which reopens and reads history first.
The `v2_recovery_rule` test runs both lost-reply cases against a real SQLite
store.

## The registry

`zeno_fcis_shell::effects::registry` holds `zeno-fcis/shell-effects/1`: one
signature for each of ten ports (ReadState, Commit, ReadHistory, ReadOutbox,
Send, Acknowledge, Reopen, Clock, Random and Wait). Each gives the tag, the
argument and result schemas with byte caps and integer ranges, the capability
and footprint, the IDs of its handler laws, and its default V2 calls.
- `registry::check` refuses a missing, repeated or misordered port, a malformed
  schema, a footprint beyond its capability, an unknown or unused law, and a
  store port with no V2 mapping. `registry::conforms` checks values.
- Send maps to the external transport that `relay::export_line` feeds. Clock,
  Random and Wait have no default V2 API. ReadHistory maps to
  `Lineage::audit_history_read_only` and also needs Authority: the adapter
  rebuilds each commit's publication data with `audited_publication`.
- Send answers `Sent`, `NotSent(Code)` or `Unknown(Fault)`. Wait answers a
  tick, `Interrupted` or a fault; an interrupted delivery run ends with
  `Refused(Interrupted)` and leaves the record pending.

## Programs and the driver

A program is `start(invocation, policy)` and `resume(continuation, response)`:
total, pure functions that return `Done(outcome)` or
`Request(id, operation, continuation)`, with no async, closure, macro or I/O.
A continuation has no `Clone`, so it is resumed at most once.
- **Commit with recovery** follows the rule above with a bounded retry budget.
  Running out of retries gives an explicit `Unknown`.
- **Outbox delivery** sends each pending record's exact bytes and acknowledges
  it only when the transport's report matches its ID and payload hash. NotSent
  and Unknown leave it pending; a retry waits a backoff plus a jitter draw. An
  unordered outbox, an early wake or an out-of-range draw ends with `Protocol`.

The driver checks the program's declared capabilities and holds one
outstanding request. It admits a response only if the epoch, ordinal, port,
store, handle and subject match and the result fits its schema; otherwise the
program does not advance. The registry pins the SHA-256 of `program.rs`, the
file the purity checker and the conformance test read.

`zeno_fcis_shell_sqlite::effects::bind_publication` rebuilds a publication
from `PublicationData` through Authority and refuses another identity, another
subject or inputs that do not commit. No V2 API changes.

## Simulation, stacks, record and replay

`effects::simulation` is a terminal handler over an explicit world: durable
store, each handle's volatile state, transport queue, receiver with durable
deduplication, and an epoch clock. Every fault point is an explicit choice:
answer, busy, an ambiguous reply, a lost reply after the write, a crash before
or after it, drop, duplicate, a wake time or interruption, and a jitter draw. A
lost reply may still arrive; it is offered late at every later point and must be
refused. Only the simulation chooses.

A stack is layers in front of one terminal. `handler::bind` refuses a terminal
that does not cover a port the program can request. The record layer appends
every request, answer, crash and outcome to a transcript. Each link holds the
SHA-256 of its predecessor, starting from a manifest that binds the registry,
the program source, the stack, the schedule, the initial world and the start.
This is integrity evidence, not a signature. `record::replay` re-runs the
program from a transcript with no handler: each request must equal the
recorded one, and anything missing, extra, reordered or changed refuses.

## Exploration and laws

`effects::explore` runs every schedule of a profile depth-first or
breadth-first. Each node re-runs from the start, because a continuation is
never resumed twice; states at every choice are fingerprinted and expanded
once. A run is `complete` only with no frontier left; a budget stop is
`incomplete` and a malformed profile or choice is `refused`.

The frozen profile has two handles, two keys and two publication bodies, two
deliveries per commit, a queue of two, ticks 0 to 3, jitter below 2, and retry
budgets 0 to 2. One crashed run per actor may restart. Two scenarios are
explored: two commit programs on two handles, and delivery to a deduplicating
receiver.
- Every handled request is checked against the nine registry laws. Probes from
  every reached store cover requests no program makes, such as changed bytes
  under a key or an unknown delivery ID.
- Each stack is qualified separately: the bare simulation, record, and
  identity forwarder with record. Every final transcript must replay exactly.
- Every final state must hold the protocol invariants: the store is the fold of
  its history, each key applies once, receipts belong to the original key, no
  effect repeats past deduplication, nothing is sent before it is committed,
  and only a record that was sent is acknowledged.

## The production SQLite handler

`zeno_fcis_shell_sqlite::effects::production::Production` is the terminal for
one handle on one store. Each port calls the V2 API the registry names, so no
SQL is added or reordered. Send goes to a caller's `Transport`; Clock, Random
and Wait go to an opt-in `Time`, and stay uncovered without one. V2 refusals
keep their codes. SQLite busy or full is `NotApplied`, because the transaction
rolled back. Any other SQLite failure is `Unknown` for a write and `NotApplied`
for a read.

A store's identity is the SHA-256 of its original genesis envelope, its
Authority's identity and the delivery interpreter's identity; a handle adds
its number. Two stores created from the same genesis share an identity. A
request for another store or handle is refused untouched.

`handler::Fault` sits in front of any terminal. It makes the simulation's
fault choices for real: busy, an ambiguous answer, a lost answer after
forwarding, a crash before or after, and a duplicate Send. It never
forwards a commit or an acknowledgement twice. The
`effects_production` tests check the production handler against the
independent protocol model from the SQLite fault simulation. After every
step they compare a complete snapshot of every `v2_*` table and the
receiver. They cover:
- every fault schedule of two commit programs on two handles, and of
  delivery to a deduplicating receiver;
- an interruption at each of the eight statement boundaries of a commit,
  followed by recovery;
- a real busy store;
- the ten real process aborts, each followed by recovery and redelivery.

Disk-full and other I/O failures are qualified only in the simulation, by
the `NotApplied` and `Unknown` branches they map to. No deterministic SQLite
fixture produces them.

## Checks

`tools/check_shell_effects_controls.py` builds eighteen mutants. Each must fail
its named check while the unmodified tree passes:

| Control | Check |
|---|---|
| Acknowledge signature removed | `port_registry` |
| Subject comparison removed from binding | `publication_binding` |
| Store comparison removed from the driver | `wrong_store_response` |
| Ordinal binding removed from the driver | `duplicate_response` |
| Recovery always retries unversioned | `step_conformance` |
| A step function reads the clock | `zeno-fcis purity crates/zeno-fcis-shell` |
| A commit writes one of its two outbox rows | `simulation_laws` |
| Reopen resets the replay keys | `simulation_laws` |
| An acknowledgement sets another record's bit | `simulation_laws` |
| A wait wakes before its deadline | `simulation_laws` |
| A jitter draw reaches its bound | `simulation_laws` |
| The driver admits older ordinals | `late_replies_refused` |
| Replay continues live when the transcript ends | `replay_is_exact` |
| Chain verification skips its link checks | `transcript_chain` |
| Binding skips the coverage check | `stack_binding` |
| The store identity ignores the Authority | `store_identity_binds_genesis_authority_and_interpreter` |
| Production commits `At(v)` without its version | `production_agrees_with_model` |
| Production reports SQLite busy as `Unknown` | `production_real_busy` |

The default tests explore retry budgets 0 and 1 in the simulation, and fewer
on SQLite. The ignored tests `explore_frozen_profile_all_budgets` and
`production_agrees_with_model_all_budgets` cover every budget.

## Scope and limits

The evidence is finite exploration of the stated profile, tests and a static
rule check; it is not a proof. The laws are checked against the simulation and
its stacks. SQLite is checked by agreement with the independent model, and
the transport is only a test receiver. The premises are one store,
one trusted writer and relay, and two handles; history absence is trusted
only under that single-writer premise. Creation, upgrades, multiple stores,
multiple processes, exactly-once external effects and power-loss durability are
not covered.
