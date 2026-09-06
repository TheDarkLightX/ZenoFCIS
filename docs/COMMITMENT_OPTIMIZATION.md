# Commitment allocation optimization

Base: `8cdb8775ba2f559729bc2a6b598e4bb2192eed5b`.
Branch: `agent/streaming-commitments-20260905`.

## Contract and preflight

The codec owns canonical domain framing. The crypto crate owns the two sealed
SHA-256 providers. A commitment currently allocates the complete framed
preimage even when the caller already owns canonical payload bytes. This
change feeds borrowed segments of the same preimage to the approved providers.
Preimages of at most 128 bytes instead use a fixed local array and one raw
hash call, avoiding repeated incremental-update overhead for small inputs.
Hashing remains linear in input size; only the intermediate allocation and
copy are removed. Value encoding itself can still allocate.

The public `CommitmentHasher` extension defaults to concatenation followed
by the existing `hash` method, preserving existing provider implementations.
The approved providers use their existing incremental APIs. The allocating
`domain_preimage` function remains the independent reference. No new provider,
algorithm ID, wire format, external dependency version, or toolchain is needed.
The benchmark uses the already locked Criterion version.

Inputs are immutable borrowed slices, used only during the call. Provider
state is fresh and exclusively owned. No reference escapes, no mutable cache
is introduced, and no trusted "already checked" flag is accepted. Sealed
provider admission, known-answer verification, domain limits, length checks,
and all authorization, replay, and SQLite publication paths remain in force.
There is no business arithmetic or persisted schema change. The exact equation
required is `hash_parts(parts) = hash(concat(parts))`, including empty parts.

The changed semantic files are `zeno-fcis-codec/src/lib.rs` and
`zeno-fcis-crypto/src/lib.rs`, both covered by the existing semantic assurance
scan. Benchmarks are non-authoritative test tools. Their input construction,
canonical output parity, and timing boundaries must be checked before timing.
No custom allocator, unsafe code, assembly, global mutable state, or additional
Lean runtime is part of this change.

## Evidence plan

1. Retain the existing collections benchmark overflow as a failing witness.
2. Check the default hasher extension, exact framing, length/version/name
   boundaries, SHA-256 block and padding boundaries, arbitrary segment splits,
   and both approved providers against the allocating reference and fixed
   digests. Reject a provider that handles raw hashes correctly but segmented
   commitments incorrectly.
3. Repair fixture widths and compare the existing persistent backends on
   identical inputs. Label canonical encoding separately from commitment cost.
4. Benchmark allocating versus streaming commitments with both providers;
   report local measurements and unchanged asymptotic hashing cost separately.
5. Run focused checks, no-default-feature checks, benchmark smoke tests,
   package checks, and all ATDD scenarios immediately before committing.

Formal qualification, production readiness, target-wide performance, and
release approval are not implied by these checks.

## Preservation argument and validation

Both paths use the original six fields in the original order and endian
convention. For the small path, the checked total length is at most 128. After
each copy, the initialized prefix equals the concatenation of the parts seen
so far; every prefix length is at most the checked total. Only that prefix is
hashed. Larger inputs use the provider's incremental concatenation contract.
The allocating reference is unchanged. This is an implementation argument
supported by tests, not a new formal theorem.

The new corpus checks all two-part splits for lengths 0 through 130, bytewise
segments, larger chunk sizes, empty segments, three domain names including the
65,535-byte maximum, three versions, and both sides of the small-buffer and
SHA-256 block/padding boundaries. Both approved providers match the reference.
The fixed-vector admission check now exercises each raw vector through both
hashing APIs while retaining the report's count of four distinct vectors.
A provider with correct raw hashes but broken segmented hashes is rejected.
The domain fixed vector exercises the small path; the differential corpus
covers large framed inputs. Inputs over 4 GiB were not allocated for testing;
libcrux retains the existing per-update `u32` chunk ceiling.

Focused tests, Clippy, reference/provider parity, and benchmark smoke checks
passed during development. Repository-wide results and exact source/log
identities belong to the final completion receipt. The existing CI and full
ATDD release-contract scenario now run the benchmark smoke checks permanently.
These smoke checks do not measure performance.

## Local measurements

Measured on one AMD Ryzen Threadripper 1950X, x86-64, Rust 1.97.1, with the
workspace release profile: overflow checks, thin LTO, and one codegen unit.
Criterion used 40 samples, 0.5 seconds of warmup, and a one-second target
measurement per case. The input domain was `zeno-fcis/bench`, version 1.
The reference timing includes preimage allocation, copying, hashing, and drop.
Input construction and correctness checks are outside both timed paths.

| Payload bytes | RustCrypto reference / optimized | Time reduction | libcrux reference / optimized | Time reduction |
|---:|---:|---:|---:|---:|
| 0 | 92.27 / 92.87 ns | -0.65% | 296.41 / 294.11 ns | 0.78% |
| 64 | 138.36 / 137.86 ns | 0.36% | 552.55 / 547.13 ns | 0.98% |
| 1,024 | 715.13 / 673.80 ns | 5.78% | 4.336 / 4.289 us | 1.08% |
| 65,536 | 38.230 / 36.144 us | 5.46% | 254.140 / 252.150 us | 0.78% |
| 1,048,576 | 614.650 / 578.610 us | 5.86% | 4.090 / 4.019 ms | 1.73% |

These are within-run point-estimate comparisons, not application throughput or
portable speed guarantees. Host load and CPU placement were uncontrolled.
Tiny-input time differences are small; the buffer removal is the consistent
benefit. The removed allocation held `payload_bytes + domain_bytes + 26`
bytes. This storage reduction follows from the code, not an allocator trace.
Small inputs still copy at most 128 bytes onto the stack. Large payloads are
borrowed directly, while SHA-256 retains its own fixed-size internal buffers.
Existing custom providers keep their allocating default for large inputs.

The benchmark label `streamed` includes the small-stack path. Criterion's
historical `change` output compares a case against an earlier run; the table
instead compares `materialized` and `streamed` from the same final run.

Replay with the existing pinned tools and lockfile:

```sh
cargo +1.97.1 bench -p zeno-fcis-crypto --bench commitment --all-features --locked -- --sample-size 40 --warm-up-time 0.5 --measurement-time 1 --noplot
cargo +1.97.1 bench --profile dev -p zeno-fcis-crypto -p zeno-fcis-collections --all-features --locked -- --test
```

The providers already expose incremental hashing; see the pinned
[RustCrypto API](https://docs.rs/sha2/0.11.0/sha2/#incremental-api).
No external package version or Lean installation changed. Criterion 0.5.1 was
already locked and is added only as a crypto development dependency.

## Additional optimization methods

The follow-up audit identified five further candidates at the base revision.
Their performance remains unmeasured. Ownership transfer is now implemented
as a subsequent bounded change; the remaining items are future candidates.

1. **Transfer ownership during sealing.** Implemented by moving the consuming
   builder's footprint buffers into normalization, preserving reason order,
   limits, and exact decision bytes. See the
   [ownership-transfer record](OWNED_FOOTPRINT_SEALING.md).
2. **Bulk-build persistent maps.** Repeated reference insertion clones every
   intermediate map. A private builder could consume bounded input once and
   publish one immutable result. Preserve duplicate replacement order and
   compare against the existing fold. See the
   [reference backend](../crates/zeno-fcis-collections/src/reference.rs).
3. **Resolve schema references once.** Validation binary-searches a type ID at
   every value node, including repeated homogeneous elements. Resolve a private
   type graph or hoist repeated lookups while retaining every node/depth check
   and error order. See [schema validation](../crates/zeno-fcis-schema/src/validate.rs).
4. **Generate borrowed readers and static paths.** Generated reads clone the
   field value and allocate a path. Borrow admitted subtrees and reuse static
   path descriptions while preserving footprint registration and the owning
   API. See [generated readers](../crates/zeno-fcis-bootstrap/src/templates.rs)
   and [typed adapters](../crates/zeno-fcis-codegen/src/adapters.rs).
5. **Reuse bounded file-hashing scratch memory.** Toolchain inventory reads
   complete files into vectors. A bounded chunk buffer can reduce peak RAM
   while preserving descriptor-relative reads, exact content hashes, byte
   ceilings, and change detection. It would not reduce runtime disk usage.
   See [formal-tool inventory](../crates/zeno-fcis-formal-tools/src/lib.rs).
