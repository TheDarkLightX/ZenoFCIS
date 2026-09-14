# Commit bundle validation

`CommitBundle::validate_and_apply` derives the receipt and successor together,
then checks every independently supplied identity, body and receipt field. It
returns the successor whose root it just derived. This removes three deep
component clones and a second application of the same patch.

## Preserved behavior

The public API, canonical encoders and decoders, hash domains, resource limits,
and independent transition, authority and shell checks remain unchanged. The
failure order remains reason validation, patch application, patch/commit/outbox
hashing, candidate hashing, then bundle mismatch. Inputs remain immutable.

The old implementation rebuilt a complete bundle from cloned patch and plans.
Those three concrete, immutable components necessarily compared equal to their
originals. The new implementation borrows them and compares the independently
supplied candidate ID, body and receipt in full. Literal comparisons remain
necessary even when a test hash provider deliberately produces collisions.

The shared private helper returns the existing `Receipt` and `AppliedPatch`
types. It adds no public type, cache, mutable state, or persistent format. Sealing
and validation use the same derivation. The argument relies on deterministic,
conforming `CommitmentHasher` implementations; hash-call counts intentionally
change, so arbitrary stateful callbacks are outside this preservation claim.

## Behavior checks

The three tests in `zeno-fcis-receipt/src/validation_tests.rs` retain the released
sealing and validation functions as a reference. They compare complete results,
canonical bytes, decoded bundles, malformed fields and simultaneous failures
over bounded state/patch/plan cases. Both approved SHA-256 providers and a
deliberately colliding provider are exercised.

Removing the candidate-ID, body or receipt comparison, one at a time, makes the
field differential test fail. The restored implementation passes all three
differential tests. This checks that the tests detect those specific losses of
validation; it does not establish mutation coverage for every possible defect.

The original V1 source baseline remains unchanged. The compatibility guard pins
the new source and tests, checks their reference functions against the retained
released source, and reconstructs the exact released file outside the reviewed
edits. Seven other baseline files remain byte-identical. Packaged application
qualification executes the three tests from the extracted receipt crate and
rejects missing or skipped tests. These are source checks and bounded
differential evidence, not a universal equivalence proof.

## Measurements

Two runs of each implementation used the same benchmark, Rust 1.97.1, the
workspace release profile, RustCrypto SHA-256 and one logical CPU. The order was
released, candidate, candidate, released. Each case used 40 samples, a 400 ms
warmup and a two-second measurement interval.

The table reports the average of the two per-run median estimates. Measurements
on this shared host have scheduling and frequency noise; the empty case has no
consistent improvement and the counter case varies between runs.

| Case | Released (microseconds) | Candidate (microseconds) | Time reduction |
| --- | ---: | ---: | ---: |
| Empty | 2.43 | 2.41 | Inconclusive |
| One-field counter | 5.93 | 5.07 | 14.5%, variable |
| 64 fields, 8 edits | 42.94 | 26.83 | 37.5% |
| 1,024 fields, 16 edits | 619.44 | 321.54 | 48.1% |
| 4,096 fields, 1 edit | 790.23 | 425.17 | 46.2% |
| Stale pre-state | 1.47 | 0.27 | 81.7% |

[Recorded samples and estimates](measurements/bundle-validation-20260914.json)
include source, benchmark and executable hashes. An earlier shared-target run
reused the released executable for both labels; that comparison was discarded.
The retained comparison used separate compilation targets and distinct binaries.

To reproduce, use separate checkouts and target directories for the released
commit `3b2224d5a081e7ab3e2e282ad6704bcb177d25ac` and this candidate. Copy only the
receipt benchmark, its development dependencies and the corresponding lockfile
into the released checkout; retain its original receipt implementation. In each
checkout run:

```sh
CARGO_TARGET_DIR="$PWD/target-bundle-benchmark" CARGO_INCREMENTAL=0 \
  cargo +1.97.1 bench -p zeno-fcis-receipt --bench validation --locked -- --noplot
```

Confirm distinct executable identities and pin both executions to the same CPU
when comparing. These measurements support removing duplicate validation work;
they do not establish a speedup for every application or change any destination
delivery guarantee.
