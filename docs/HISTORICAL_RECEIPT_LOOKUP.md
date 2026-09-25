# Historical receipt lookup

Status: **DEVELOPMENT CANDIDATE**, 2026-09-24, based on revision
`5f6acb01d59edf95f49ccc67bdbdc078793ab9f9`. This is not a released API or an
application migration. The source facts in the next section describe that base.

## Application need

A policy activation can commit and then lose its response. Re-evaluating the
same command against the changed state can reject it, even though the first
invocation succeeded. An application needs to distinguish an exact historical
commit, a conflicting use of its operation identity, and an operation absent
from the validated local history. Returning an old result must not execute the
transition again or renew old evidence.

**FACT.** `SqliteShell` retains canonical authorization, bundle and receipt bytes,
plus replay identity, policy, invocation and state version in its private
`ValidatedStoredCandidate` records. `open_existing` reauthorizes the history;
`snapshot` checks persisted rows against that cache. There is no public lookup
by replay identity. See
[`zeno-fcis-shell-sqlite`](../crates/zeno-fcis-shell-sqlite/src/lib.rs).

**FACT.** The authority's complete context commitment binds policy identity,
canonical context, principal, authentication-evidence commitment and replay ID.
Its command/context commitments do not depend on the current pre-state.
Currently they are computed as part of `admit_invocation`. See
[`zeno-fcis-authority`](../crates/zeno-fcis-authority/src/lib.rs).

## Preserve the authority boundary

**FACT (candidate implementation).** Two operations provide a read-only
observation over already validated records:

1. `CatalogCommitAuthority::request_bindings(...)` derives the existing
   `ExpectedInvocationBindings` from the exact admitted command/context,
   principal, authentication-evidence commitment and replay ID, under this
   authority's policy. It checks schema/type/profile bindings and fixed-ID
   validity, but creates no invocation witness and authenticates no external
   fact. `admit_invocation` reuses the same derivation, preserving its
   current commitments byte for byte.
2. `SqliteShell::lookup_committed(authority, expected_genesis, replay_id, bindings)`
   returns `HistoricalLookup` or an error. The supplied authority must match the
   shell. The expected genesis hash must be saved at original submission, and
   commits to the exact policy. The result includes the observed policy, genesis,
   semantic root/version and an optional immutable `StoredCommit`. Both command
   and complete context must match, not just replay ID or current policy state.

`StoredCommit` exposes the exact original receipt, replay identity,
invocation/authorization/candidate identities and committed state version.
Policy/genesis and observed head are on `HistoricalLookup`, including for absence.
All values come from the validated history. The
type is not a `CatalogAuthorizedTransition` or a fresh verified-fact capability
and cannot be used to publish a new transition.

| Observed condition | Required result |
|---|---|
| Replay ID absent in the expected, consistently validated lineage | Observation with `committed() == None` |
| Replay ID present and both expected bindings match | Original immutable receipt and provenance |
| Replay ID present with either expected binding different | Conflict; no receipt disclosure through this call |
| Request's original genesis differs, even on a miss | `LineageMismatch`; never absence |
| Invalid ID, schema/profile mismatch, corrupt rows or stale handle | Error; never reinterpret as absence |

Application-level authorization to read a receipt is still required. Knowledge
of a replay ID or expected digest is not a credential. Callers compare against
the original canonical request and original admitted context, including its
principal and evidence commitment. A fresh proof with different bytes does not
silently become the same historical invocation. A current read-authorization
check may use fresh credentials separately, without replacing those historical
bindings. How an API client recovers its original context is an application
contract, not a reason to accept a weaker match.

## One consistent observation

**FACT (candidate implementation).** One SQLite read transaction uses the existing validated cache.
The observation linearizes at that transaction's snapshot:

1. Verify shell identity, genesis identity, semantic root/version and complete
   persisted history against the handle's validated state in that snapshot.
2. Require the request's original genesis, then locate the replay ID in the existing validated records. Check uniqueness and
   exact expected bindings from the authorized canonical data.
3. Return bytes from the validated record, not a separately queried unchecked row.

A writer that committed before the snapshot can make a previously opened
handle stale. Return an error and require controlled reopen/revalidation; do
not fabricate absence or automatically trust newly observed rows. A concurrent
commit after the snapshot does not change what the observation says about that
snapshot. An exclusive Rust borrow serializes lookup with other shell API calls;
this does not promise coordination between independent database writers.

Missing, extra, altered or inconsistently mapped rows must fail closed. A valid
rollback of an entire trusted store, or replacing a path while an open connection
still names the old database, is outside this observation's freshness guarantee.
The result names its genesis and observed history; remote finality and rollback
resistance require an independently authenticated checkpoint contract.

Do not derive a retry's scope from the currently deployed authority. A new
genesis after migration could have no record of a predecessor's committed
operation. The required original-genesis input prevents interpreting that new
history as evidence of non-commitment. Cross-migration replay coverage, retained
predecessor lookup and namespace transfer remain application migration duties.
Geneses identify content, not unique physical database instances: separate stores
initialized from identical genesis still require shared ordering/replay authority.

## Minimality and compatibility

**FACT (candidate implementation).** The API reuses the existing replay binding, canonical receipt, authority
commitments and validated history. It adds no table, replay cache, mutable receipt
index, alternate hash preimage or database format. A linear scan of the current
cache is acceptable initially; report its cost rather than implying constant
time or an established hard history bound. Passing the authority permits reuse
of full genesis validation without adding a second genesis cache. This evaluates
genesis laws; it does not execute historical transitions during lookup.

The alternative is an application-owned receipt table, with another atomicity
edge and recovery procedure. Reusing validated records removes that duplicated
state. An index optimization requires measured need and its own consistency
argument. Existing commit, reject, replay-conflict and delivery behavior must
remain unchanged, including canonical schema v5 bytes and rejection precedence.
The new `InvalidReplayId` and `LineageMismatch` error variants are intentional
public API additions; downstream exhaustive matches need review before release.

## Acceptance before implementation is promoted

| Case | Executable closure witness |
|---|---|
| Lost response and later state changes | Commit, advance state, then look up the first operation; exact receipt bytes return without execution or changes to state, receipt/replay counts or outbox |
| Binding substitutions | Independently change command, context epoch, principal, evidence commitment, replay or policy; reject conflict or invalid scope without publication or receipt leakage |
| Reopen and evidence expiry | Reopen under the exact authority and recover the original receipt after its original evidence expires; historical observation conveys no current freshness |
| Corruption and concurrent change | Mutate/delete/add history rows or commit through another handle; return a checked snapshot result or error, never unchecked bytes or false absence |
| Genuine absence and authority separation | Uncommitted/rejected operation returns absence; malformed input errors; the returned observation cannot construct a commit capability |

Run these through the real authority and SQLite implementation, alongside
existing commit/crash/reopen tests. Differential fixtures must establish that
factoring the binding derivation does not change command or complete-context
hashes. Include distinct failures for zero IDs, replacement genesis and
commitment mismatch. CI and public docs must identify the checked revision.

## Implementation and remaining gates

The implementation is in
[`history.rs`](../crates/zeno-fcis-shell-sqlite/src/history.rs), with
[recovery and concurrency tests](../crates/zeno-fcis-shell-sqlite/src/tests/history_tests.rs).
Authority tests use four pre-change fixtures to check command/context,
authorization and candidate identities. The compile-fail documentation checks
that a `StoredCommit` cannot be passed to `commit`.

Replay the focused checks from the repository root:

```sh
cargo +1.97.1 fmt --all -- --check
cargo +1.97.1 test --locked -p zeno-fcis-authority -p zeno-fcis-shell-sqlite
cargo +1.97.1 clippy --locked -p zeno-fcis-authority -p zeno-fcis-shell-sqlite --all-targets -- -D warnings
```

These tests are not a machine-checked refinement of SQLite, authentication or
distributed replay. Application evidence-expiry behavior, committed-failure and
rejection examples under real service profiles, bounded-history performance,
and cross-migration end-to-end recovery remain integration gates.

**FACT (local validation).** The two affected packages pass 58 unit/integration
tests and three compile-fail documentation tests (baseline: 47 and two).
Formatting, focused Clippy with denied warnings, denied-warning Rustdoc and the
authority crate's host `--no-default-features` check pass. Full workspace/all-feature
CI and cross-target `no_std` builds have not been run for this candidate.
The [validation manifest](historical-receipt-validation.json) pins the checked
source files and summarizes the selected checks.

Four temporary mutations were compiled and caught by the indicated tests; the
original source was restored and the passing checks rerun:

| Removed safeguard | Failing test in `tests::history_tests` |
|---|---|
| Complete context comparison | `lookup_matches_complete_original_request_and_policy` |
| Original genesis comparison | `different_genesis_never_turns_a_historical_request_into_absence` |
| Persisted genesis validation | `corrupt_history_never_becomes_a_hit_or_absence` |
| SQLite read transaction | `a_wal_write_after_snapshot_does_not_mix_histories` |

Run an individual witness with
`cargo +1.97.1 test --locked -p zeno-fcis-shell-sqlite --lib TEST_NAME`.
These are four selected mutation checks, not exhaustive mutation coverage.
The first candidate test run failed because schema setup attempted to change a
SQLite safety setting inside a transaction. Separating read-only schema checks
from connection setup fixed the failure; existing reopen behavior is retained.

- Release review of the result types, accessors, method names and error taxonomy.
- A separate bounded bundle-export API if a demonstrated application needs one;
  this API exposes no raw authentication material or bundle bytes.
- Application read authorization, original-context recovery and profile
  migration across histories remain separate contracts.
- A portable history export/checkpoint protocol is a separate possible feature;
  this local API must not imply it already exists.

No production finality, remote authentication, rollback resistance, external
delivery completion, migration correctness or application-level idempotency is
established by this design document alone.

### Broader qualification limits

Strict workspace Clippy (all targets and features) passes on macOS arm64.
The all-feature workspace test run does not pass: three formal-tool process
execution tests fail; two also fail at the unchanged base revision. The third,
`rc3_process_success_kills_descendants_after_collecting_output`, passed in the
baseline control and remains unresolved. The full acceptance runner with Python
3.12 stops at finite synthesis because target execution requires Linux waitid
and process-group cleanup. A passing Linux run is required before promotion.
The Linux-only busy-executable CLI test now has the same platform guard as the
helper functions it exercises, fixing its pre-existing macOS compilation error.
These limits do not qualify the change as release-ready.
