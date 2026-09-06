# Owned footprint sealing

Base: `b1fc97f3e43dbbd0441e3d8bcd324de4dd8b511a`.
Branch: `agent/owned-footprints-20260905`.

## Preflight and contract

`CataloguedTransitionBuilder::seal` consumes a fresh, exclusively owned builder.
Its read, write, context, and effect-path vectors have no external mutable
aliases. At the base revision the footprint helpers cloned these vectors and
their boxed path atoms before normalization. The remainder of sealing uses the resulting
`Footprint`, not the original vectors.

The implementation transfers these vectors into the existing normalization function using
`core::mem::take`. Only the consuming `seal` operation calls the private take
helpers. Selecting the reason remains first. A committing outcome normalizes
reads, writes, contexts, and effect paths in that order. Rejection normalizes
only reads and contexts, retaining empty writes/effects and discarding staged
candidate plans. Any error consumes and discards the private builder.

The change removes four vector-clone sites on committing paths and two on
rejection paths, including cloning each nonempty path-atom buffer. Empty vectors
already avoid allocation. Sorting, deduplication, canonical encoding, hashing,
and publication costs remain. No latency or allocator-trace measurement is
claimed for this stage.

The public API, schema and format versions, reason precedence, profile and
invocation commitments, state ownership, budgets, and path limits are unchanged.
`TransitionResourceReport` is still derived from the completed footprint with
the same caller-reported budget and configured limits. Revalidation remains
mandatory. This builder produces candidate artifacts; nominal authorization
and atomic SQLite publication remain separate, unchanged boundaries.

The only production file in scope is
[`zeno-fcis-transition/src/lib.rs`](../crates/zeno-fcis-transition/src/lib.rs),
already covered by the repository's semantic assurance scan. No new package,
unsafe code, buffer pool, shared mutation, serialization format, toolchain, or
Lean runtime is needed.

## Validation plan

Run the additional boundary corpus against the unchanged implementation before
the refactor, then against the candidate. Cover Accept, Reject, and
CommittedFailure; both staging orders; empty, short, and maximum-depth context
paths admitted by the existing builder API; repeated observations; exact path
limits; one-over-limit errors; and retained pre-state references.

Compare complete decisions across equivalent staging orders, assert exact
normalized path sets, reason IDs, budgets, limits, successor states, and
unchanged rejected state. Retain the existing catalog, invalid-plan, stale-input,
resource, replay, and shell tests. Run the full ATDD suite immediately before
committing. The completion receipt binds the final source and validation logs.

## Evidence

The 18-case corpus passed against the unchanged implementation and the candidate.
An additional manual export compared the canonical bytes of each complete
footprint, resource report, and commit bundle or rejection receipt, together
with decision kind and reason ID. Both versions produced identical vectors.
The comparison held the fixture, dependencies, and compiler fixed and replaced
only the transition source with the exact base file for the reference run.
The SHA-256 of the 18 diagnostic vector lines was
`92e9a3e937bcba2cc8b32c2c21dd95b417c73ba07f212e0ea73b557669031835`.

A temporary context-omission mutation failed the new exact-footprint assertion.
The candidate source was restored byte-for-byte afterward. This checks that
the tests detect a lost observation even if both construction orders agree.
The bounded corpus is not a proof over arbitrary programs or a claim that
recorded context paths were independently observed by an external system.

The manual export is an ignored test because its diagnostic framing is for
source-revision comparison. Its bytes are not a new protocol or proof format.
The underlying preservation test runs normally in the workspace suite.

```sh
cargo +1.97.1 test -p zeno-fcis-transition --all-features --locked
cargo +1.97.1 test -p zeno-fcis-transition --all-features --locked export_footprint_sealing_vectors -- --ignored --nocapture
cargo +1.97.1 check -p zeno-fcis-transition --no-default-features --locked
```

Independent read-only review found no blocker in the production change.
Repository-wide validation is recorded in the completion receipt. No Lean
installation, runtime copy, dependency change, or publication is involved.
