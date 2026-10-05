# Smaller V2 integration blockers

Draft scope, 2026-10-03. The owner chose simplification first, then one mandatory
library-owned route for durable-counter, account-lockout, order-fulfillment,
inventory-reservation, compliance-gateway, withdrawal-queue,
agent-treasury-guard and prepared-counter. The bounded definition of done is
[V2_VERIFIED_CORE_PLAN.md](V2_VERIFIED_CORE_PLAN.md), with the [32-entry ledger scope](V2_LEDGER_SCOPE.md).
This checkpoint does not announce implementation acceptance or release.

The frozen intake wired all eight to checked Authority publication: seven used
V2SqliteShell and prepared-counter used HistorySqliteShell. S1 migrated
prepared-counter to the single schema-9 SQLite v2 shell and removed History.
S1 and S3 have provisional source reviews and passing bounded checkpoints;
final combined-source qualification remains open.

Remaining blockers are:

- Qualify the combined S1, S3, S2, S4, S5 implementation, including prepared-counter
  on the one SQLite v2 shell. Its 16,384-byte aggregate cap, cancellation, partition, incomplete
  finish, retry, crash, freshness and legal success cases must survive.
- Qualify the implemented replacement of embedded source bytes with a generated
  evaluator digest against the approved sources, including required gates and
  stale-manifest checks in workspace tests, ATDD and CI. Keep exact policy
  binding; changed-authority open/commit/replay and stored-genesis tampering
  must refuse. Identity-comparison removal must be caught by a negative control.
- Qualify certificate-bound delivery identity after History removal.
  Recurrent state and equal payloads must not collapse different transactions,
  lanes or ordinals. Preserve commit-order pending delivery and collisions.
- Renew the derived-footprint and mandatory library law/genesis evidence after
  simplification. Checked declarations already constrain logical targets;
  meter-grant flags are not separate permission verdicts. No independent
  universal footprint theorem is claimed. Finish direct-crate/feature-enabled
  callback and capability custody review.
- Retain account-lockout's full timestamp domain on the generic graph. Record
  its approved design changes to malformed-input refusal classes and Step limits.
  Generic execution proof and fact-class replay do not establish an all-timestamps
  graph-to-rule correspondence theorem. State that assurance delta; preserving
  a proof over generated constants remains an available stronger option.
- Renew whole-source Verus and Exec inventory, translated order/coverage profiles
  and mutation controls, native/no-std/Clippy/Miri, 12,064-input finite replay,
  eight generated-app journeys, full ATDD and independent exact-source review.
  Regenerate source-bound evidence after final edits and check required CI.
- Qualify the 19 route-required ledger entries and finish API migration/public wording.
  Withdrawal records reasons; it does not remove route-critical protections.

Compound-value execution over the retained 12-type fixture and full-width U128
zUSD are new V2.1 profile work. Schema-only proof, a smaller fixture or private
execution does not qualify them. Restoring their baseline code still requires
new integrated evidence; reducing their legal domains is not completion.

QEMU, portable sources/mirrors, archive/privacy scanners, private historical
oracle qualification and version/package cutover belong to release engineering.
They remain release obligations where applicable. Earlier proof receipts and
successful journeys retain only their named source/scope; none qualifies the
simplified final tree or authorizes shipping.
