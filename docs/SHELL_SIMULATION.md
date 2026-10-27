# SQLite shell fault simulation

Applications keep state, commit receipts, replay keys and pending deliveries in
one SQLite store. Operators need interrupted publication to leave a complete
commit or no commit, and a retry with the original key to recover its answer.

The `simulation` integration test exercises a bounded counter application with
two notifications per commit. Its two counters range from zero to three.
With allowed context, all 32 state/command inputs are checked separately:
24 commit and eight reject. This fixture keeps the counter's state and genesis
laws and uses a smaller decision law for the two-notification branch shape.
A SplitMix64 seed selects actions and faults; a
logical counter supplies time. No wall clock or random input decides a step.
The default corpus uses four processes and runs 256 seeds of 64 steps twice.
It compares schedules,
events, complete database projections and their canonical final bytes. An ignored
campaign runs 4,096 seeds of 128 steps twice. Each schedule starts with a
commit/export pair and weights later choices toward transport retries. Each
worker checks that all action kinds and seven crash points were exercised.

Actions include fresh publication, exact and changed retries, stale expected
versions on a second handle, export, delivery, acknowledgements, reopen and
audit. Transport schedules drop, duplicate and reorder deliveries, and lose a
reply after the receiver commits. The receiver uses a separate SQLite file;
its effect and deduplication ID share one transaction.

A pure Rust protocol model predicts state, replay records, receipts, chained
commitments and deliveries independently of the store and its reference model.
The real authority supplies opaque identity and publication bytes to the test
fixture. The model independently encodes storage rows and derives notification
payloads and delivery IDs. This tests the shell protocol; it does not establish
independent correctness of the authority, canonical codec or hash functions.
Every step reads all `v2_*` tables in one consistent read transaction, including
genesis, checkpoints and the empty upgrade table, and compares state and events.
Every checkpoint must match a predicted history anchor; explicitly requested
checkpoints must exist. Additional valid cached prefixes are projected away.

The checked invariants are:

- State is the fold of committed commands from genesis.
- Each replay key names one applied command; a retry does not apply it again.
- Every required delivery exists, without missing or extra rows.
- Only the matching delivery ID and payload hash can be acknowledged; a
  duplicate acknowledgement is refused and acknowledgement is monotone.
- State, commit receipt and outbox publication are atomic.
- Each delivery has at most one effect at the durable deduplicating receiver.

The seeded corpus uses separate named SQLite memory databases with keeper
connections that preserve logical durable state across handle reopens. File
durability is exercised by the separate receiver and process-abort tests.

Seven injected errors test rollback and recovery. Separately, a child process
aborts before BEGIN, after validation, after the state and replay writes, after
each of the two outbox writes, before COMMIT, and after COMMIT. A feature-only
first-outbox-row point closes the gap between the two INSERTs. Additional cases abort
immediately after the commit call returns and between delivery and acknowledgement.
The parent waits, reads the file, reopens, audits and retries the original key.
A busy fault holds a write lock on another connection with zero wait time.
Deterministic disk-full and generic I/O faults are not covered.

An explicitly enabled `crash-injection` API supplies per-call abort selection
and an interleaving callback between synchronization and BEGIN. The callback
is consumed immediately, retains no state and cannot borrow its own shell.
The feature also permits a connection-local busy timeout. Default builds have
no abort selector, callback or timeout setter; existing error injection remains.

Freshness has several guards, including a stored-anchor and data-version check
inside the transaction, an expected-version check and a state-update CAS.
The interleaving test checks the named stale-writer refusal. Removing just the
CAS does not bypass the earlier transaction check on that schedule; mutation
results account for these layers and the unique commit-sequence constraint.
The combined cached-tip/CAS control changes the named refusal; the sequence
constraint still prevents double application. The CAS-only diagnostic records
that the earlier guards mask its removal on this schedule.

Tests that deliberately omit receiver deduplication show duplicate effects.
Tests also show that retrying a logical command with a new key and new state
can apply it again, and that absence in a stale snapshot is not non-commit.

Schedules use JSON `[seed, [[action, fault, choice], ...]]`. A failure reports
its seed, step, expected and actual values, minimizes the step list by delta
debugging, and saves a JSON regression in `tests/sim/fixtures/`. The normal
regression test replays every JSON file in that folder. The comparator and
minimizer have self-tests. Source-mutation controls build in isolated copies;
a build failure never counts as detecting a defect.

Run the normal tests with:

```sh
cargo +1.97.1 test -p zeno-fcis-shell-sqlite --locked --features crash-injection
python3 tools/check_shell_simulation_mutations.py
```

These are tested observations over bounded schedules, with premises of SQLite
atomicity, locking and durability, the operating system and file system, the
canonical encodings and the hash implementations. They are not power-failure
qualification. Transport is at least once. An acknowledgement is not proof of
external delivery. Rolling back to an earlier consistent database is undetectable
without an external anchor. Only one store is covered; upgrades, lineage and
multiple-store protocols remain later work.
