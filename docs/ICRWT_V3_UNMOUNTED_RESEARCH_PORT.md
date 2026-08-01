# ICRWT v3 unmounted research port

Status: implemented and tested unmounted diagnostic value

Date: 2026-08-01

## Frozen endpoints

- ZenoFCIS base: `79f2648db04f4ba07c2d9e62871fcec67d653656`
- Supplied ZenoStructures archive: `zenostructures-repair-bundle-0.6.2.zip`
- Supplied archive SHA-256: `b339abc67ccf912cf636810b3d1c8d7688f69773766f7540d4cdb5cada707e09`
- Corrected reference profile: `zenostructures/icrwt/*/v3`

The v3 profile supersedes the supplied v2 ICRWT profile. No v2 recovery root is
compatible with v3.

## Review finding that requires v3

The supplied v2 validator advanced a word by `RecoveryObservation` only. It did
not require the complete `after_snapshot` of event `i` to equal the complete
`before_snapshot` of event `i + 1`.

This admitted a splice such as:

```text
event 0: PRE snapshot A -> PRE snapshot B
event 1: PRE snapshot C -> POST snapshot D

B != C
```

The observation classes agree while the exact durable history is discontinuous.
The v2 prefix-node identity also omitted the initial snapshot, so two empty
prefixes rooted at different durable states had the same node identity.

The v2 global witness selector ordered equal-length defects by word identity
before fixed defect precedence, while the published observable contract required
defect precedence first. A lexically early `MIXED_PREFIX` could therefore hide a
higher-precedence `CHAIN_MISMATCH` at the same prefix length.

The v2 point-of-use reconstruction also omitted the constructor's unique-event
identity invariant. A forged Python `RecoveryWord` containing the same event
identity twice could be rebuilt as a valid tree.

The retained v3 falsifiers require:

```text
initial_snapshot = first.before_snapshot
event[i].after_snapshot = event[i + 1].before_snapshot
prefix_key = (initial_snapshot_commitment, exact_event_prefix)

global witness order =
  (prefix length, fixed defect precedence, word identity, event identity)

event identities are unique within one exact recovery word
```

Every query that returns a derived tree fact must operate on a value produced by
the checked constructor. The Python reference additionally rebuilds the full
tree at point of use because Python object construction can be bypassed.

## Selected invariant owner

`zeno-fcis-diagnostics` owns this value because it classifies finite replay and
fault-injection traces. It owns no datastore, transition, receipt, publication,
migration, rollback, or delivery authority.

The Rust value is constructed from exact snapshot commitments and event facts:

```text
RecoverySnapshotCommitment
  semantic_root
  durable_layout_root
  authority_root

RecoveryEvent
  exact before and after snapshot commitments
  exact canonical effect identities
  progress or checked structural stutter

RecoveryWord
  exact initial snapshot commitment
  exact ordered event tuple
```

The builder returns either one canonical tree or one canonical earliest defect.

## Mechanical guarantees

- safe public constructors cannot create a structural stutter with changed
  snapshot commitments or a nonempty effect set;
- adjacent events must share one exact snapshot commitment;
- only `PRE` and `POST` are admitted terminal classes;
- any `MIXED` prefix remains a defect after later events;
- `POST -> PRE` is rejected;
- duplicate word identity and word-identity collision are distinct defects;
- structural duplicate/collision defects and per-word defects compete under
  the same canonical global witness ordering;
- node identity includes the initial snapshot commitment and exact event prefix;
- input order cannot change the selected shortest defect witness;
- canonical encoding retains the exact word lineage.

## Explicit non-guarantees

- a snapshot commitment is not proof that every production database row was
  included;
- equality relies on the selected commitment provider and upstream canonical
  durable-layout reconstruction;
- the diagnostic tree does not prove SQLite atomicity or crash refinement;
- the tree cannot authorize commit, recovery, migration, rollback, effects, or
  M6 promotion;
- no ZenoDEX entrypoint consumes this value in this change.

## Deferred structures

HBMB, LFRL, and PCFP remain outside this port.

- HBMB needs the concrete migration phase and writer authority graph.
- LFRL needs a mounted transport manifest, independent recovery verifier, and
  one-use rollback authority tied to the real publication transaction.
- PCFP needs the actual deployed proof verifier and store-current context
  authority. The Python HMAC authorities are reference boundaries only.

Porting their reference classes before those project-owned inputs exist would
create authority-shaped library values without a valid producer.

## Evidence plan

1. Preserve all four v3 counterexamples as Rust unit tests.
2. Add constructor-mechanism tests for stutter closure and exact chain binding.
3. Check canonical behavior under word input permutation.
4. Compile with `no_std + alloc`, `#![forbid(unsafe_code)]`, and denied warnings.
5. Run focused Clippy, formatting, and the complete repository ATDD gate.
6. Keep the feature optional and absent from mounted-runtime and authority
   features.

Python/Rust byte parity and a production fault-injection adapter remain later
gates. Until those exist, the Rust port is `TESTED_ONLY`, diagnostic, and
unmounted.
