# Mounted ZenoDEX single-vault zUSD refinement v1

## Scope

This package mounts the real Python and Rust single-vault zUSD transitions from
ZenoDEX commit `e3cf1aad40e487893230ae0c55f1f0dda62f9955`. The transition sources
themselves are unchanged from parent commit
`e0122cd1fa2aa6a4c5da594c99c6e7d6edf04ce2`. The linked ZenoDEX change adds
only canonical JSON-line entry points.

The permanent `mounted-zenodex` workflow builds the pinned Rust runtime with
Rust 1.89.0, invokes the Python authority and Rust runtime on the same 17-case
state-threaded corpus, normalizes each result into a complete ZenoFCIS decision,
and requires byte-exact agreement.

## Inputs

Each case binds:

- the complete 32-field `ZusdStateV1` pre-state;
- one closed `ZusdCommandV1` value;
- the exact ZenoDEX mount and transition revisions;
- the explicit `require_oracle_authorization = false` policy;
- the Python entry point and Rust subcommand;
- the zUSD schema, profile, precedence, algorithm, codec, and budget identities.

The native request is exactly one compact LF-terminated JSON line with fixed
field order, exact fields, integer state/command values, and a 64 KiB bound.
JSON is shell transport only. Protocol artifacts use ZCVE/1.

## Outputs

Each native runtime returns decision kind, stable reason, native pre/post roots,
native receipt, and complete successor state. The adapter independently
recomputes the native ZenoDEX roots and receipt before constructing:

- an unchanged-state `RejectReceipt`, or
- a one-operation root-replacement `CanonicalPatch`;
- an empty evidence-profile `CommitPlan`;
- an empty `OutboxPlan`;
- a candidate-bound `Receipt` and `CommitBundle`;
- a complete `NormalizedDecision` and decision commitment.

The comparison covers kind, reason, all bindings, pre/post roots, candidate ID,
patch, commit plan, outbox plan, receipt, bundle, and decision commitment. A
transport or normalized disagreement writes the exact request, both outputs,
and the canonical replay fixture when available.

## Authority boundary

The ZenoDEX engines decide only their native pure transition. The mount adapter
validates and translates that observation. `CandidateBuilder`,
`NormalizedDecision::try_new`, and `compare_exact` decide whether the bounded
transport shape is internally present and equal. The result remains untrusted
mounted evidence. Production promotion must separately reconstruct it through
`ValidatedNormalizedDecision` and, for exhaustive claims, bind a canonical
finite-domain manifest and independently checked enumeration artifact.

The runner is an imperative evidence shell. Process spawning, timeout handling,
filesystem retention, Git revision inspection, and JSON transport are outside
semantic authority. Neither runtime receives publication or production
authority from this comparison.

## Trusted dependencies

- ZenoDEX Python transition and Rust shadow at the pinned revisions;
- CPython used to execute the explicit bridge;
- Rust 1.89.0 for the ZenoDEX runtime;
- Rust 1.97.1 for ZenoFCIS;
- pinned Serde/Serde JSON for non-consensus transport;
- pinned RustCrypto SHA-256 for independent native checks and ZenoFCIS hashes;
- GitHub Actions checkout and artifact actions pinned by commit.

## Deterministic resource bounds

- 64 KiB maximum native request or response line;
- five-second process deadline per engine per case;
- 64 KiB maximum captured stderr;
- exactly 32 state fields;
- at most one patch operation;
- zero commit-plan effects;
- zero outbox entries;
- exactly 17 retained state-threaded cases in the v1 corpus.

These bounds are committed into the decision budget identity where they affect
the semantic adapter. Wall-clock timeout and process-output handling remain
shell controls.

## Laws and negative cases

- identical admitted native observations normalize to identical complete
  decisions;
- each decoded native observation is bound to the exact canonical input used
  during decoding and cannot be normalized for another input;
- accepted results bind a candidate and complete bundle;
- rejected results preserve state and carry no candidate, plan, outbox, or
  bundle;
- native state roots and accepted receipts equal independently recomputed
  values;
- unknown reasons, unknown fields, reordered fields, whitespace aliases,
  duplicate fields, malformed integers, wrong roots, wrong receipts, extra
  output, stderr, crash, timeout, and tool disagreement fail closed;
- every accepted patch validates against its exact pre-state and old-value hash;
- replay artifacts bind the exact canonical input and both complete decisions.

## Assumptions

- the pinned ZenoDEX revision remains retrievable by full commit ID and is
  checked out without tracked or untracked worktree changes;
- CPython integer semantics match the source transition assumptions;
- the Rust and Python entry points call the reviewed transitions rather than a
  substituted implementation;
- the selected 17 cases are representative regression evidence, not exhaustive
  state-space coverage.

## Explicit nonclaims

This is bounded executable refinement evidence. It is not an unbounded proof,
an audit, a production authenticated-state mount, a real balance/effect
interpreter, multi-vault coverage, full ZenoDEX lane coverage, economic
correctness evidence, or production authorization. `CommittedFailure` remains
reserved and distinct in the normalized algebra; the current zUSD engines do
not emit it. Empty commit and outbox plans describe this pure evidence profile
and do not authorize production mint, burn, transfer, or delivery effects.
