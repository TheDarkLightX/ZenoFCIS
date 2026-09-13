# ZenoFCIS 1.1.0

V1.1 adds checked finite exit plans and bounded preparation of ordered work. An
application can prove that every state in its declared finite model has a path
to a terminal state, prepare a command in small chunks, and authorize its complete
result through the existing state and outbox publication boundary.

## New developer workflows

- `CompletionProblem`, `find_completion` and `verify_completion` separate finite
  search from independent checking of every state and a decreasing exit rank.
  `verify_completion_bytes` admits portable canonical plans with bounded decoding
  and checks them against a model supplied independently by the consumer.
- `PreparedFold` owns its inputs and reserves complete modeled resource costs.
  It accepts only the next ordered chunk, rolls back a failed chunk, and exposes
  a result only after completion and an exact root/version/invocation match.
- `zeno-fcis synth completion discover|find|verify|replay` provides closed JSON
  models, deterministic diagnostics, canonical plans and inert reproduction files
  for agents. Reproducing a failed claim retains its failure exit code.
- `zeno-fcis new counter --template prepared-counter` generates an application
  with typed commands, independent transition and law checks, capacity admission,
  nominal authorization, SQLite publication, exact replay and notification retry.

```sh
zeno-fcis new counter --template prepared-counter
cd counter
zeno-fcis synth completion find completion.json --out completion-case
zeno-fcis synth completion verify completion.json --plan completion-case/plan.zcve
zeno-fcis synth completion replay completion.json --case completion-case
cargo test
cargo run -- counter.sqlite
```

The example checks every one of its 216 state/command/context decisions. Its
largest aggregate canonical publication is 1,893 bytes; this includes command,
successor state, authorization and the full bundle containing receipt and outbox.
Tests cover exact capacity, one byte below it, all chunk partitions, cancellation,
failed chunks, competing operations, recurring state roots, every SQLite commit
crash point, exact replay, restart and interrupted delivery.

## Compatibility and assurance

All 36 public crates use version 1.1.0. Rust remains 1.97.1 and Lean remains
4.30.0; no external Rust dependencies are upgraded. Existing canonical protocol
identifiers, state roots, receipts, delivery identities and SQLite schema retain
their meanings. Completion and preparation are optional finite-synthesis APIs.
New profiles own separate identities; source-bound certificates must be regenerated
when their checker sources change. Regenerating a template or changing its
source-bound policy produces a different application identity; an existing store
requires its original approved policy or an explicitly reviewed migration.

The release gate checks an unchanged V1.0 consumer against V1.1 packages and pins
the unchanged foundational implementations with their embedded wire tests. This
is a specific compatibility check, not proof that every downstream source program
will compile unchanged. Both generated applications are built and exercised from
the actual crate archives, with the reviewed dependency graph.

Finite completion establishes available paths under the declared environment.
It does not establish scheduler fairness, unconditional economic eligibility or
external delivery. Preparation creates ordinary data; only existing application
authorization and SQLite publication create committed state and outbox obligations.
The profile's byte limit does not bound OS memory, execution time or database size.
No serialized partial accumulator, new proof backend, hash chain, paged storage
protocol or automatic settlement authority is introduced.

See [bounded completion](BOUNDED_COMPLETION.md), the
[prepared application](../crates/zeno-fcis-cli/templates/prepared-counter/README.md)
and the [V1.1 release checklist](V1_1_RELEASE_CHECKLIST.md).
