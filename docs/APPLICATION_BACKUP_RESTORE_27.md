# Application backup and verified restore (design, F27-28)

Status: design only, not implemented in 2.7.0. Nothing here adds a command yet.

## What exists and what is missing

A generated application keeps two files: the SQLite store (`DATABASE`) and
the submission journal (`DATABASE.submissions`). The store holds genesis, the
commits with their chain, the deliveries with their stable `delivery_id`,
the checkpoints and the upgrade records (`crates/zeno-fcis-shell-sqlite/src/v2.rs`);
the journal holds one line per submission and is what `history` audits the
store against. Today the only operational guidance is the refusal message of
`submit` and `history`: a missing journal of a store with commits must be
restored "from a backup", and the runtime assembly notes list backup and
restore as unqualified: do not clone a live store or reuse its effect
identity for another deployment, and pending delivery IDs must survive
(`docs/RUNTIME_ASSEMBLY.md`, Data, queries and operations).

The missing pieces are a consistent snapshot of both files together, a
restore that proves the snapshot is the one taken and that the restored
application is the same application, and a rule that stops a restored copy
from delivering twice or acting as a second live instance.

## Commands

Two commands on the generated application's own command line, beside
`state`, `history`, `pending` and `deliver`:

```text
APP backup DATABASE --to DIR [--format human|json]
APP restore DIR --to DATABASE [--format human|json]
```

### backup

1. Take the journal's exclusive lock, the one `submit` takes, so no commit
   is in flight; then open the store read-only.
2. Copy the store with SQLite's online backup API into `DIR/store.sqlite`
   (a consistent image of one transaction), copy the journal bytes to
   `DIR/submissions`, and read the head: version, commit count, chain hash,
   pending delivery count and the sorted list of pending `delivery_id`s.
3. Write `DIR/manifest.json`, schema `zeno-fcis/app-backup/1`: application
   name, contract identity (the same identity `version` prints), the head, the
   SHA-256 of both copied files, the pending delivery IDs, the count of
   journal lines, the time from the caller's clock marked as unverified, and
   `authority: none`. Write every file to a staging directory and rename it
   to `DIR` last; an existing `DIR` is refused.
4. Release the lock. The lock is held for the copy only; the application is
   paused, not stopped.

### restore

1. Refuse an existing `DATABASE` or `DATABASE.submissions`; restore never
   overwrites.
2. Read `manifest.json`; require the schema, this application's name and
   contract identity, and the SHA-256 of both files. A mismatch is exit 1
   with the differing field named and nothing written.
3. Copy both files to staging names, then open the staged store read-only
   and run the full read-only audit `history` runs: every commit has its
   submission, the Authority decides the recorded inputs again, the chain
   and checkpoints are consistent, upgrades after the last commit hold. The
   audit's head must equal the manifest's head and its pending delivery IDs
   the manifest's list.
4. Rename the two staged files into place in the order store, then journal,
   with directory synchronisation; a failure exposes no target.
5. Report `restored` with the head and the pending count. The store keeps
   the pending deliveries as pending with their original IDs; nothing is
   delivered by restore.

### Effect identity after restore

A restored store must not deliver what the original already delivered, and
two copies must not both deliver. The mechanism is the existing one:
every attempt carries the original `delivery_id` and canonical payload. The
file destination keeps one line per ID and refuses a different line under a
held ID, so a redelivery of an already-delivered ID is a no-op there; the
relay is at least once and relies on an idempotent receiver, as its
documentation states. Restore therefore does not need to reconstruct what
the original delivered after the snapshot; it needs the operator to stop the
original and the receiver to be idempotent. The design makes the first part
explicit:

- `restore` writes `DATABASE.restored-from`, naming the manifest's chain
  hash and SHA-256s. `state` and `version` print it, so a restored copy is
  visible as one.
- A restored application refuses `deliver` until the operator passes
  `--resume-deliveries`, acknowledging that the original instance is stopped.
  The refusal names the pending count and the file above.

This is the trust rule of the design: the application cannot know whether the
original is still running; it records that a restore happened and makes the
first delivery after it an explicit operator decision. It does not prevent an
operator from running two copies.

## Verification

- Unit tests on the manifest: every required field, refusal of each mismatch
  one at a time (name, identity, store hash, journal hash, head, pending IDs).
- Journey tests in `crates/zeno-fcis-cli/contract-app/tests`: init, several
  submits with deliveries, deliver part of them, backup; then restore to a
  fresh name and require `state`, `history` and `pending` to print the same
  as the original at the snapshot, and `deliver` to refuse until
  `--resume-deliveries`; then deliver and require the destination file to
  hold every ID exactly once.
- Interruption controls with the existing crash points: a process exit after
  the store copy but before the manifest leaves no `DIR`; an exit between the
  two renames of restore leaves no `DATABASE`.
- Corruption controls: a flipped byte in `store.sqlite`, a truncated journal,
  an edited manifest field, each refused by restore with the named field.
- A backup taken while a `submit` waits for the lock contains either the
  commit or not, never a journal line without its commit; one test runs them
  concurrently and checks the audit of the backup.

## Scope

The claim is: a backup names a consistent store-and-journal pair at one head,
and a restore admits exactly that pair after the same audit `history`
performs, with pending delivery IDs preserved. It is not a proof of
exactly-once external delivery, not power-loss qualification of the host
filesystem, not a guarantee against two live copies, and not a migration: a backup of version N restores as version N and
upgrades through the existing `--upgrade`. Retention, readiness and
disk-full behaviour from the runtime assembly list stay separate items.
