# Kernel law harnesses

## Purpose

The kernel crates state their laws in documentation and unit tests. This
package checks four of those laws mechanically. Each law is one predicate,
checked two ways:

- an exhaustive harness enumerates a small, stated domain completely;
- a random harness runs the same predicate under bolero over a broad domain.

The harnesses live in a separate `verification/` workspace. Their dependencies
never enter the lockfile of the published crates.

## Inputs and outputs

| Law | Statement | Exhaustive domain | Random domain |
| --- | --- | --- | --- |
| budget-charge | `Budget::charge` succeeds exactly when the new total stays within the limit. A failure reports the resource, the limit, and the attempted total (`u64::MAX` on overflow), and it changes no consumption. `finish` reports the exact usage. | Limits for two resources from {0, 1, 2, 2^64 − 2, 2^64 − 1}, and every sequence of up to three charges over three resources and those amounts: 90,400 cases. | Seven arbitrary limits and up to 32 arbitrary charges. |
| reason-choice | `first_reason` returns the least reason by precedence, then code bytes, for every ordering of its input. `collect_and_choose` keeps the collection in its original order. | Every list of up to four reasons over three precedences and three codes, each in every order: 7,381 lists. | Up to 64 reasons over eight precedences and four codes, including an empty code and a prefix code. |
| codec-canonical | `decode_value` accepts a byte string only when the decoded value passes `AdmittedValue` validation and re-encodes to exactly those bytes. | Every byte string of length at most two. Also every one-byte change, truncation, and extension of at least 60 sample encodings, of which more than 1,000 decode. | Arbitrary byte strings, and one-byte changes to the sample encodings. |
| patch-overlap | `CanonicalPatch::try_new` rejects with `OverlappingPaths` exactly when one operation's path is a prefix of, or equal to, another's. Otherwise the operations come out in canonical path order. | Every sequence of up to three of the 40 paths of length at most three over three segments: 65,641 cases. | Up to eight paths of up to four segments, over all five segment kinds. |

Commands:

- `cargo +1.97.1 test --manifest-path verification/Cargo.toml --locked` runs
  every harness.
- The exhaustive harnesses are named `*_holds_on_every_small_input`. They are
  deterministic, and the acceptance gate runs only them.
- A random harness draws fresh inputs on each run. A failure prints
  `BOLERO_RANDOM_SEED=<seed>`, which replays it.
- `cargo +1.97.1 deny --manifest-path verification/Cargo.toml --config deny.toml check`
  applies the repository's supply-chain policy to the harness dependencies.

## Controls

Each exhaustive harness was run against a planted bug in the kernel source.
The bugs were edited into a working tree and reverted; none is committed.

| Planted bug | Result |
| --- | --- |
| Consumption written before the limit check | Detected by budget-charge |
| Code tie-break removed from reason order | Detected by reason-choice |
| Overlap checked on paths sorted by canonical bytes, which start with the path length | Detected by patch-overlap |
| Decoder's trailing-byte check and re-encode comparison both removed | Detected by codec-canonical |
| Decoder's re-encode comparison alone removed | Not detected |

The last bug survives. In these domains the inner decoder already rejects every
non-canonical input, so no tested input reaches the final comparison. The
harnesses do not establish whether that comparison is redundant for all inputs.

## Authority boundary

The harnesses call only public kernel APIs and grant no authority. They change
no kernel source, canonical byte, or identity.

## Trusted dependencies

- The kernel crates under test, and the Rust 1.97.1 compiler.
- bolero 0.13.4 generates the random inputs. It and its dependencies are pinned
  in `verification/Cargo.lock`. The exhaustive harnesses use plain loops and
  depend on no generator.

## Deterministic resource bounds

The exhaustive domains are fixed, and each harness asserts its own case count.
The random harnesses use bolero's default iteration count and cap each input:
32 charges, 64 reasons, and eight paths of four segments.

## Laws

1. Each law has exactly one predicate. Both of its harnesses call that
   predicate.
2. An exhaustive harness asserts the number of cases it enumerated, so a
   shrunken domain fails.

## Assumptions

- The stated domains contain the boundary cases that matter: zero and maximum
  amounts, equal precedences, codes that are prefixes of other codes, and paths
  that are prefixes of other paths.

## Explicit nonclaims

- An exhaustive result is complete only for its stated domain. In the terms of
  [design record 0003](adr/0003-epistemic-status.md), it is Proved for that
  domain. Beyond it, the random results are Checked samples.
- The harnesses do not yet run under a fuzzing engine or Kani. The predicates
  are written so they can.
- The other kernel laws are not covered yet.
- The codec harness covers the property of `fuzz/fuzz_targets/codec_roundtrip.rs`,
  which CI only builds. That target still exists.
