# Bounded completion and ordered preparation

Implementation contract for the optional finite synthesis extension. The
baseline is `7d950878dbf039fdeafcecbe1f63933f96394f45`.

Enable the existing `synthesis` feature. The runnable example uses public
umbrella imports and produces a small JSON result for automated consumers:

```sh
cargo +1.97.1 run -p zeno-fcis --example bounded_completion --features synthesis --locked
```

The API entry points are `CompletionProblem::try_new`, `find_completion`,
`verify_completion`, `verify_completion_bytes`, and `PreparedFold::{start, advance, finish}`. All diagnostics
are typed. `NoExit` carries the first exact state without a path; failed folds
carry an absolute item index. `PreparedFold` debug output reveals progress and
operation identity, without dumping its program, inputs, or partial accumulator.
Capacity errors include the resource, complete required amount for admitted
tuples, and declared limit; input sizing preserves prefix/domain error order.
The item schema supplies one fixed encoded size, avoiding per-item encoding;
invalid-item diagnostics identify the input position without printing its contents.

See [the complete example](../crates/zeno-fcis/examples/bounded_completion.rs).
No new plugin, language-specific callback, solver installation, or Lean upgrade
is needed. Existing finite-IR language emitters remain applicable to the step
programs; a native application still needs its normal target-conformance gate.

## Scope and ownership

The application owns the meaning of terminal states, admitted commands, its
trusted current state/version and invocation identity, and all economic laws.
The synthesis crate owns finite enumeration, checked interpretation, resource
admission, ordered preparation, and verification of decreasing completion
steps. Both additions remain pure `no_std + alloc` code. They cannot construct
publication authority, execute effects, change SQLite, or bypass project laws.

The finite completion model has inputs `(state fields, command fields)` and
outputs `(accepted: Bool, successor state fields)`. A separate closed Boolean
program defines terminal states. Every tuple in the declared state domain is
in scope, including tuples that may not be reachable in a downstream system.
Environment facts must be fixed in the reviewed program/state; this is not an
adversarial scheduling game or an unconditional financial exit guarantee.

## Required behavior

1. Admission bounds the complete state/command product before enumeration.
   State and command limits cover the actual canonical finite tuple encoding.
   A malformed/trapping program never becomes positive completion evidence.
2. Reverse breadth-first search finds a shortest exit path or returns the first
   lexicographic state without one. Equal-length choices use lexicographic
   command order. Independent verification checks the expected problem identity,
   terminal rows, every selected command, and strict rank decrease. The verifier
   need not trust search and does not certify shortest-path optimality.
3. Ordered preparation owns a closed program, initial accumulator, admitted
   ordered items, and starting context. No callbacks or ambient state enter.
   Each program invocation maps `(accumulator, item)` to the next accumulator.
4. Start reserves the complete modeled read/write/evaluation and canonical
   input/output costs through existing immutable `BudgetLimits`. It bounds
   retained input bytes, result bytes, item count, and per-call chunk count.
5. Advancing uses an expected offset and nonzero item count. It changes only
   private preparation state, only after the entire chunk succeeds. Failure
   leaves the previous accumulator and cursor unchanged. The first failed
   item is reported by its absolute position, independent of chunk partition.
6. Finish requires all items and the exact starting state root, version, and
   invocation binding. Its result remains ordinary untrusted scalar data for
   normal application authorization. Dropping preparation is cancellation;
   replay from the original admitted inputs is recovery.
7. Changing chunk partitions preserves the complete final finite tuple and
   its canonical bytes, the operation identity, and reserved costs. Arithmetic,
   rounding, eager evaluation, and error order remain those of the existing IR.

## Design choices and compatibility

Use modules in the existing synthesis crate, its closed finite IR, existing
budget meter, and canonical codec. Owning bounded input tuples removes a new
chunk-authentication protocol. An exclusively owned preparation buffer is
operational scratch; it is not authoritative semantic state. The accumulator
is not exposed before completion. No new solver, trait hierarchy, byte-budget
monad, global effect chain, external proof system, or core publication type is
needed. Inputs are moved into preparation, and each call copies only the small
accumulator into temporary work before committing progress.

Existing program semantics, accepted/rejected results, candidate/receipt
formats, delivery identities, authority constructors, and store schema remain
unchanged. New completion/preparation identities use separate version-1 hash
domains. Source-bound finite certificates must be refreshed if their included
checker source changes. These extensions are opt-in, with no automatic
activation for existing profiles and no migration of authoritative stores.

## Evidence plan

Compare the real Rust fold with an independent whole-operation reference over
all small states and chunk partitions. Retain failures for zero/repeated/skipped
offsets, a trap after partial work, stale root/version/invocation, capacity
boundaries, incomplete finish, cancellation/restart, and rounding changes.
Compare completion search with independent finite reachability and corrupt
each supplied plan binding/rank/command. Include a dead-end capacity example
and demonstrate that an enabled self-loop is insufficient. Keep the ordinary
authority, SQLite, replay, and outbox acceptance gates unchanged.

## Executed focused checks

The original 26 focused Rust tests include every two-state graph with one optional
successor per state and every terminal-state set (36 models). Fold comparison
covers 390 complete chunk partitions over 120 small initial/input instances,
plus a separate order-dependent recurrence and checked-arithmetic traps.
The public example, progress-only debug output, private accumulator boundary,
and `no_std` umbrella build are checked by the new acceptance scenario.

[Nine deliberate faults](../test-data/bounded-completion/mutation-results.json)
were rejected by the actual Rust tests: wrong problem binding, non-decreasing
ranks, hidden terminal-state traps, missing output capacity, missing tuple
framing, ignored starting version, reversed items, ignored offsets, and incomplete
capacity diagnostics. The
record names the exact source/test hashes and commands. It is historical
negative evidence, not an exhaustive fault model or a proof certificate.

## Limits of the claim

These are bounded finite operations, not unbounded temporal proofs. Scheduling,
input availability, economic eligibility, and independently trusted context
remain application obligations. Logical resource reservations are not RAM,
wall-clock, allocator-failure, or OS isolation guarantees. Canonical finite
tuple bounds do not cover an application's larger candidate, witness, receipt,
or outbox; those still need their normal admission and publication checks.

Preparation retains all bounded inputs. It does not provide paged storage,
smaller monolithic final commits, authenticated checkpoint loading, real crash
durability, or atomic external settlement. Persisting an accumulator and calling
it verified is unsupported. Recovery replays original inputs. Any future
authoritative pending-operation format requires an explicit profile/version
and a separate observational-refinement and storage qualification.


## Portable plans and agent workflow

V1.1 imports the existing canonical plan bytes with bounds derived from the
independently admitted model. The decoder limits bytes, nodes, depth, tuple
length and payload before allocation. It rejects alternate profiles, wrong
bindings, noncanonical/truncated/trailing encodings and invalid ranks/commands.
Only the independent verifier constructs a `VerifiedCompletion`.

```sh
zeno-fcis synth completion discover
zeno-fcis new counter --template prepared-counter
cd counter
zeno-fcis synth completion find completion.json --out completion-case
zeno-fcis synth completion verify completion.json --plan completion-case/plan.zcve
zeno-fcis synth completion replay completion.json --case completion-case
```

`find` creates a new directory containing the normalized closed problem,
canonical plan on success and deterministic result. `verify` and `replay` use
a separately supplied model. A matched replay of a failing claim retains the
failure exit code and reports `replay: "matched"`; reproduction is not acceptance.
These files contain data, never executable checkpoints or authority.

The [prepared application](../crates/zeno-fcis-cli/templates/prepared-counter/README.md)
connects complete private work to ordinary authorization and SQLite publication.
Its 216-case corpus checks decisions, successor state, notification, rejection
precedence and complete publication bytes. Four partitions, cancellation, failed
chunks, context/root/version changes, a recurring root, competing database handles,
all seven commit crash points and exact replay are exercised. The release gate
builds and runs it from actual crate archives.

`completion_scaling` measures chains of 8, 64, 512 and 4,096 declared states,
including search, independent verification and strict import:

```sh
cargo +1.97.1 run -p zeno-fcis --example completion_scaling --features synthesis --release --locked
```

Elapsed times describe that local run only. They establish no comparative speedup,
universal complexity bound, constant-time behavior or production latency promise.
