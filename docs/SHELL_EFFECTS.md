# Shell effects: port registry and pure programs

An application's shell commits publications, recovers from a lost reply and
delivers outbox records. The aim is one shell that runs unchanged against real
storage, a deterministic faulty world or a recorded run. This first stage
declares the shell's effects and writes the shell's decisions as pure programs.
Handlers that carry effects out, simulation and replay come in later stages.

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

The `v2_recovery_rule` test runs both cases against a real SQLite store.

## The registry

`zeno_fcis_shell::effects::registry` holds `zeno-fcis/shell-effects/1`. It
gives a signature for each of ten ports: ReadState, Commit, ReadHistory,
ReadOutbox, Send, Acknowledge, Reopen, Clock, Random and Wait. Each signature
states:

- the port's tag;
- its argument and result schemas, with byte caps and checked integer ranges;
- its capability and effect footprint, over durable state, volatile state,
  transport, clock and jitter;
- the IDs of the handler laws it must satisfy;
- the default V2 calls it maps to.

`registry::check` refuses a registry with a missing, repeated or misordered
port, a malformed schema, or a footprint beyond its capability. It also
refuses a law that is unknown or never named, and a store port with no V2
mapping. `registry::conforms` checks each request and result value against its
schema. The registry's canonical bytes have a SHA-256 identity.

## Programs and the driver

A program is `start(invocation, policy)` and `resume(continuation, response)`.
Both are total, pure functions that return `Done(outcome)` or
`Request(id, operation, continuation)`. There is no async, closure, macro or
I/O. A continuation has no `Clone`, so it is resumed at most once.

- **Commit with recovery** follows the rule above, with a bounded retry budget.
  Running out of retries gives an explicit `Unknown`.
- **Outbox delivery** reads the pending records and sends each one's exact
  bytes. It acknowledges a record only when the transport's report matches its
  ID and payload hash. NotSent and Unknown leave the record pending; a retry
  waits a backoff plus a jitter draw. An unordered outbox, an early wake or an
  out-of-range draw stops the run with `Protocol`.

The driver checks before a run that a program declares the capability of every
port it can request. It holds one outstanding request. It admits a response
only if the epoch, ordinal, port, store, handle and subject all match, and its
result fits the schema. The subject is the publication subject or the delivery
ID. A duplicate, stale, wrong-store or wrong-handle response is refused, and
the program does not advance.

The registry pins the SHA-256 of `program.rs`, the file the purity checker and
the conformance test read. The conformance test checks every response branch of
both programs against a table.

## Publication binding

A program carries `PublicationData`: the original inputs, the Authority
identity and the publication subject. That data grants nothing. Before a
commit, `zeno_fcis_shell_sqlite::effects::bind_publication` rebuilds the
publication through Authority. It refuses another identity, another subject
or inputs that do not commit. No V2 API changes.

## Checks

`tools/check_shell_effects_controls.py` builds six mutants. Each must fail its
named check while the unmodified tree passes:

| Control | Check |
|---|---|
| Acknowledge signature removed | `port_registry` |
| Subject comparison removed from binding | `publication_binding` |
| Store comparison removed from the driver | `wrong_store_response` |
| Ordinal binding removed from the driver | `duplicate_response` |
| Recovery always retries unversioned | `step_conformance` |
| A step function reads the clock | `zeno-fcis purity crates/zeno-fcis-shell` |

## Scope and limits

The evidence is tests over stated cases and a static rule check; it is not a
proof. Handler laws are stated but not yet checked against any handler. One
store, one trusted writer and two handles are in scope. Creation, upgrades,
multiple stores, multiple processes, exactly-once external effects and
power-loss durability are not.
