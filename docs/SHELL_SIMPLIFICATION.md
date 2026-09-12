# Smaller shell commit and replay paths

Baseline: `f77a0a5a3cf2766cec18a0fde5057086e5ce70e4`.

This pass applies the repository's standard: choose the smallest understandable
design that preserves required assurance, with behavior-preservation evidence
for each simplification. It implements remaining suggestions from Fable 5.1's
design review. Opus 5 made an isolated draft; Astra reviewed and integrated it,
shortened its comments, and independently tested the changes.

| Simplification | Preserved obligation | Evidence |
| --- | --- | --- |
| Remove repeated candidate and row-count validation on SQLite replay. | Complete history validation, exact replay identities and bytes, cached membership, first rejection. | Before-and-after corruption cases and SQL statement counts. |
| Remove repeated candidate validation on acknowledgement. | Complete history validation before caller errors, exact delivery hash, acknowledgement state and cached membership. | The same corruption cases, including simultaneous caller and stored-data errors. |
| Publish the cached record after SQLite commits. | Failed commits leave database and memory unchanged; successful commits publish before the post-commit crash point. | A real deferred-constraint failure at `COMMIT`, successful retry and replay, and existing crash-point tests. |
| Move the consumed shell's authorization records instead of cloning them. | Record contents, canonical order, retained authorization bytes, replay and conflict checks. | Rust ownership permits the move; existing authorization and replay tests pass. |

The removed SQLite checks followed complete history validation inside the same
`IMMEDIATE` transaction, with no intervening writes. SQLite's
[transaction rules](https://www.sqlite.org/lang_transaction.html) exclude a
concurrent write transaction. The exclusive `&mut self` borrow also prevents
intervening changes to the shell's cached history. Each operation still
validates every stored history record and the table counts.

The earlier candidate-membership rejection rules out replacing a cached record
later in the same commit. Publishing after successful `COMMIT` removes the
tentative cache insertion, replacement restoration and removal on commit
failure. Record preparation still precedes the commit; the existing
`AfterCommit` injection remains after memory publication.

## Measured reduction

With one committed history record and one outbox entry, SQLite tracing measured:

| Successful operation | SELECT statements before | After |
| --- | ---: | ---: |
| Idempotent replay | 18 | 10 |
| Acknowledgement | 14 | 10 |

These are statement counts for the stated case, not elapsed-time claims.
The trace probe used the existing pinned dependency and was removed after
measurement. The library gains no dependency, cache, type, field or helper.

Excluding tests, the SQLite module has 11 fewer non-comment lines and four
fewer lexical branch points. Its function count stays at 55. Lizard 1.24.0
reports commit complexity decreasing from 53 to 50 and acknowledgement
complexity from 17 to 16. The authority change replaces one expression.

## Behavior checks

The retained baseline and candidate executables agree on 76 observations:
38 corruption cases exercised through both replay and acknowledgement. They
cover every history/outbox column, missing and extra rows, simultaneous
failures, exact first errors, unchanged memory and absence of writes on
rejection. The tests explicitly bypass SQL constraints during setup to model
corrupted stored data; production admission remains intact.

A separate test uses a temporary deferred foreign key and trigger to make all
writes succeed but reject `COMMIT`. It confirms unchanged persistent and
cached state, then removes the fault and checks successful commit and replay.
The affected crates passed 40 tests including doctests. These bounded checks
support the stated preservation claims; they are not a general equivalence
proof or independent release approval.

The tests also rejected four deliberately broken variants: skipping replay
history validation, skipping acknowledgement history validation, publishing
cached state before a failed commit, and discarding retained authorization
records. Exact source restoration was checked and the affected tests passed
again afterward.

The [measurement record](shell-simplification-results.json) identifies the
sources, model provenance, query counts and validation logs. Local replay tools,
baseline executables and full logs are retained in
`/tmp/zenofcis-simplification-20260912`. The required final gate is
`python3 tools/atdd.py run --all`; its result is recorded with the commit.

SQLite query batching, additional caches and relational-evaluator changes
remain separate candidates. Lean, dependency versions, canonical formats and
the RC3 release version are unchanged.
