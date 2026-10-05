# Prepared counter

Run `cargo run -- NEW_DATABASE_PATH` to prepare three ordered changes in bounded
chunks, authorize the complete result, commit it with one notification, retry the
exact invocation, restart, and drain the counter back to zero.

The state domain is 0..3 and each command contains exactly three deltas in -1..1.
Every intermediate prefix must stay in the state domain. A trusted `allowed`
context is required. This is a local non-value example, not an authentication
service or financial settlement profile.

`PreparedBatch` owns the admitted command and starting root, version and invocation.
Its inputs and partial result cannot be replaced or read. `advance` only selects
the next offset and count. Dropping it cancels; recovery repeats the original
command. Nothing is written during preparation.

`publish` obtains the current state and context, rejects stale work (including a
root that recurs at another version), and executes ordinary nominal authorization.
The pure native program independently evaluates the command. The law checker
evaluates the authored prefix and successor rules and checks the complete outbox.
Each law in `project.zeno` declares the decisions it is enforced on, and
`authority()` checks the law manifest against those declarations before it
builds the authority; `tests/lifecycle.rs` shows a manifest that binds law 501
to every commit, or to the genesis, reported as a mismatch.
The prepared state must equal the authorized state. Finally the pure application
adapter bounds the aggregate canonical command, state, authorization and bundle
before SQLite receives it. The bundle includes the patch, receipt and outbox.
This 16 KiB profile bound is not an OS memory, execution-time or database-size bound.

SQLite atomically commits state, authorization, replay identity and outbox. A
crash before commit exposes none; a crash after commit retains all. The exact
authorization bytes can be independently reauthorized for idempotent replay.
The notification destination must retain its own exact ID/hash deduplication;
the in-memory example destination survives simulated restart only in this process.

`zeno-fcis synth completion find completion.json --out completion-case` writes a
checked exit plan; `verify` and `replay` consume it with the independently supplied
model. The release gate checks that this file describes the same finite model as
the complete runtime comparison.

`cargo test` checks the complete finite decision space and canonical publication
sizes, eligible exits, chunk partitions, cancellation, failed chunks, stale and
competing operations, capacity rejection, replay and crash recovery. Exit claims
assume the reviewed allowed context. They do not promise external delivery or
fair scheduling. No intermediate checkpoint has publication authority.

The SQLite shell is the `sqlite` feature, on by default. Without it,
`cargo +1.97.1 build --no-default-features` builds the core alone: the
generated bindings, the program, the law checker, the profile, the delivery
adapter, and `authority()`, with no database; the gate checks that it also
compiles for `wasm32-unknown-unknown`. `create`, `journey`, `prepare`, and the demonstration
binary need the feature.
