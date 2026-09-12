# Outbox encoding and SQLite checks

Delivery identifiers now encode the outbox entry directly into the buffer that
already holds its candidate identity. This removes the temporary complete-entry
buffer and its copy, without changing the identifier, validation, or public API.
The production change is one statement in `OutboxEntry::delivery_id`.

Baseline: `696559dae92884d4fb1367d41f110e197abb2f9b`.
The [measurement record](outbox-encoding-results.json) binds this review to source,
executed comparisons, and actual advisory-model receipts.

## Why the smaller encoding preserves behavior

Both versions hash the same candidate prefix followed by the same encoded entry.
`OutboxEntry::encode_to` appends the ordinal and channel, then two length-prefixed
blobs. Its `put_length` and `put_blob` helpers depend on the supplied blob length,
not the existing output length. Destination and payload values still encode into
fresh buffers in the same order and retain their existing validation limits.

A destination error still precedes a payload error. Failed encoding discards the
private buffer and never reaches the hash provider. Successful encoding reaches
the same commitment domain with identical bytes and the same hash-provider calls.
No canonical format, schema, authority type, budget or checker identity changes.

The plan crate retains 43 production functions, 384 code lines inside functions,
and 75 branch points measured by Lizard 1.24.0. `delivery_id` retains complexity 3.
One temporary buffer disappears without adding a helper, field, trait, cache,
configuration option or dependency. This is an allocation improvement, not a
claim that cyclomatic complexity decreased or the whole operation is zero-copy.

## Measured resource use

An external probe called the real RustCrypto SHA-256 provider through both
implementations, using the same pinned Rust 1.97.1 dev libraries with debug
information disabled. Its allocator instrumentation was outside the library;
production remains safe Rust. Inputs were constructed before counting, and all
measured temporary allocations were released before each call returned.

| Payload bytes | Allocation/reallocation calls before | After | Requested bytes before | After |
| ---: | ---: | ---: | ---: | ---: |
| 0 | 7 | 4 | 168 | 112 |
| 1 | 7 | 4 | 168 | 112 |
| 16 | 9 | 6 | 259 | 261 |
| 64 | 9 | 6 | 377 | 309 |
| 1,024 | 9 | 6 | 3,257 | 2,219 |
| 65,536 | 9 | 6 | 196,793 | 131,243 |

Every measured case uses three fewer allocation/reallocation calls. Vector growth
means this is not an improvement in every memory measure: the 16-byte payload
requests two more bytes in total and increases peak live requested storage from
134 to 149 bytes. Other measured peaks decrease, including 96 to 72 bytes for an
empty payload and 131,148 to 131,131 bytes for a 65,536-byte payload.

Six interleaved timing rounds produced mixed changes, from about 1.9% less time
to 6.6% more time. The evidence supports less allocation and copying, not a
consistent speed improvement. These local measurements do not predict all inputs,
release-profile performance, total process memory, or allocator-failure behavior.

## SQLite candidate rejected by a counterexample

Reusing four prepared SELECT statements looked attractive: the prototype preserved
138 initial corruption/type observations and complete retained histories through
128 commits, and reduced local nonempty snapshot times by about 20–42%. It kept
all row reads and checks, retaining about 17 KiB of compiled-statement storage.
These measurements describe a rejected prototype; production SQLite queries are
unchanged.

A stronger regression exposed a diagnostic difference:

1. Commit, then read a snapshot twice.
2. Rename the outbox `payload_bytes` column.
3. Read another snapshot, then flush the statement cache and read again.

The reused statement returns `SqliteFailure` with the missing-column message.
Fresh preparation returns `SqlInputError`, which also includes the SQL string and
error offset. Both reject, but the required error detail is not preserved. The
baseline passes this regression and the cache candidate fails it.

The change was rejected instead of weakening the assertion or adding schema
tracking, diagnostic reconstruction, or connection modes. SQLite documents that
prepared statements can be [automatically regenerated after schema changes](https://www.sqlite.org/c3ref/c_stmtstatus_counter.html).
The exact Rust error difference above comes from the recorded run against pinned
rusqlite 0.40.1, not from assuming identical failure paths.

## Retained regression evidence

The final implementation passed 57 unit tests and two compile-fail documentation
tests across plan, shell, authority and SQLite, plus Clippy with warnings denied,
`no_std` compilation for plan, formatting, static assurance and documentation
checks. The new regression tests also pass against the original implementation.

- 7,066 delivery cases compare complete hash-provider input bytes and slice
  boundaries, SHA-256 results, exact encoding errors, and unchanged input values.
  They cover every `Value` variant, nested values, invalid text and ordering,
  excessive depth, integer boundaries, three candidate prefixes, and payloads
  through 65,536 bytes. A separately captured preimage and SHA-256 identifier
  supply a permanent fixed example.
- 78 replay/acknowledgement observations preserve exact first errors, unchanged
  memory, no database writes on rejection, and transaction cleanup. A later-row
  SQL type error must still precede an earlier-row field error.
- Repeated reads, schema changes and repair, reopening, and a three-transition
  history exercise fresh row reads and distinct candidate bindings. Corrupting
  the last candidate's outbox row must still fail full-history validation.
- Four deliberate faults are rejected: dropping the candidate prefix, hashing
  invalid encodings, checking only the first history candidate, and reading only
  the first outbox row. The rejected statement-cache prototype supplies a fifth
  independently reproduced failure.

Fable 5.1 identified both candidates in its initial review. Opus 5 implemented
the SQLite prototype and the selected one-line encoding change. Codex reviewed
them, found the SQLite counterexample, checked the encoding premise, strengthened
the tests, and owned validation. Fable's follow-up review did not complete because
usage credits were unavailable; it is not counted as a review. Neither completed
model task had a wall-clock cutoff. Models remained advisory.

Raw probes, source copies, model receipts and logs are retained in
`/tmp/zenofcis-sqlite-outbox-20260912`. These are bounded checks, not a general
proof or stable V1 release qualification. Lean, locked dependencies, public APIs,
persistent formats and version `1.0.0-rc.3` remain unchanged. The required
precommit acceptance run is recorded separately with the final commit.

## Hosted Miri follow-up

The [previous Miri run](https://github.com/TheDarkLightX/ZenoFCIS/actions/runs/34703756250)
was cancelled at the existing 50-minute job limit. Its log reports no assertion
failure; it stops in the synthesis choice tests after the first completed case.
The next test compares all 1,268 assignments against the previous implementation,
whose repeated value reconstruction is expensive under interpretation.

The job allowance is now 180 minutes. Every package, test, input case, strict
provenance flag and toolchain pin is unchanged. Opus 5 made the one-line change;
Codex checked the exact diff. This permits more runner time and does not establish
that the complete Miri run passes. GitHub's
[job timeout setting](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax#jobsjob_idtimeout-minutes)
controls cancellation; the fresh hosted result remains a release gate.
