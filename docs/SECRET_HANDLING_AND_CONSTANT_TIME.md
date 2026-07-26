# Secret handling and constant-time boundaries

`zeno-fcis-secret` supplies a narrow, reviewable boundary for owning secret bytes. It is not a cryptographic primitive and it does not claim physical constant-time execution by itself.

## Design goals

Secret values should be difficult to expose accidentally, impossible to format through ordinary traits, erased when their owned storage is released, and handled through source-level constant-time operations where practical.

The crate therefore does not implement the following for secret containers:

```text
Debug
Display
Clone
Copy
AsRef<[u8]>
Deref
Serde serialization
CanonicalEncode
ordinary PartialEq
```

The absence of these traits is intentional. Secret material is never a ZenoFCIS protocol value and must never enter canonical receipts, logs, errors, profiles, or evidence bundles.

## Hardened execution token

`subtle` documents that its debug-build invariant checks may contain secret-dependent branches. Constant-time and exposure operations require `HardenedExecution`, which public callers can construct only when debug assertions are disabled.

This is a build-profile guard, not compiled-code proof. A production profile must still retain symbolic or dynamic compiled-code evidence and deployment-specific timing evidence through `zeno-fcis-security`.

## Explicit exposure

Secret bytes can be borrowed only through:

```text
HardenedExecution
+ container-bound non-secret secret identity
+ ExposurePermit(secret identity, authority, purpose, maximum bytes)
+ explicit closure
    -> operation result
    + ExposureEvent
```

The event commits the public secret identity, exact permit, and public byte count. It does not contain or hash the secret value.

An `ExposurePermit` is a closed structural claim. Anyone who knows its public fields can construct one. Construction does not prove that the authority approved it. Project policy must admit the exact permit commitment and, where one-shot behavior is required, reject replay of that commitment. The Rust value is non-cloneable and consumed by an exposure, which prevents accidental reuse in safe local code.

`Exposed<T>` is `must_use`, but this crate does not durably publish its `ExposureEvent`. A panic inside the exposure closure or a caller that deliberately discards the result can prevent audit delivery after the closure observed the bytes. A production shell must arrange durable audit obligations around exposure sites and treat this event as one input to that mechanism.

## Fixed and dynamic containers

`SecretBytes<N>` should be preferred whenever secret length may itself carry information. Equality, selection, and assignment visit the complete fixed array. Their secret-derived branch condition is carried by the opaque `SecretChoice` adapter so the external `subtle::Choice` type and its ordinary boolean escape hatches do not enter the public API.

`SecretBox` supports dynamic buffers when length is explicitly public. Its equality operation returns immediately when public lengths differ. Projects must not use it when length is secret.

Both containers:

- own their storage;
- bind a caller-supplied non-secret identity that must match the exposure permit;
- are non-cloneable and non-formattable;
- zeroize on drop;
- zeroize owned constructor or replacement inputs before returning a validation error;
- support explicit clearing and replacement;
- require a permit for byte exposure.

## Dependencies

The implementation delegates low-level behavior to pinned, purpose-specific libraries:

- `zeroize = 1.9.0` for compiler-resistant erasure;
- `subtle = 2.6.1` for `Choice`, constant-time equality, and conditional assignment.

ZenoFCIS does not hand-roll cryptographic algorithms or claim stronger guarantees than those libraries provide.

The permanent `secret-hardening` workflow runs the crate in an optimized profile, checks its `no_std` configuration, and retains content-addressed LLVM IR together with the exact Rust toolchain and source manifest. Retained IR is inspection input. Its existence is not constant-time evidence. Workspace assurance separately evaluates the exact locked dependency graph with pinned `cargo-deny` and `cargo-audit` versions, while Miri exercises the secret containers under strict provenance.

## Residual risks

Zeroization cannot erase copies created before ownership transfer, previous `Vec` reallocations, compiler spills outside the owned object, swap, crash dumps, hibernation images, device DMA, caches, speculative state, or external copies made by the exposure closure. A secret identity must be public metadata chosen independently of the secret bytes; deriving it by hashing low-entropy secret material would create an offline guessing oracle.

Constant-time source patterns can be changed by compilation or affected by the processor. Production deployment should additionally consider:

```text
locked or isolated memory
core dumps disabled
swap/hibernation policy
process sandboxing
core/cache/memory partitioning
microarchitectural flushing
IOMMU isolation
compiled-code constant-time analysis
statistical timing tests
power/EM evaluation where relevant
```

Those are deployment obligations modeled by `DeploymentContract` and security evidence, not properties inferred from this crate.

## Production rule

A project may treat secret handling as production evidence only when all of the following are bound to the exact release and deployment:

1. every secret-bearing type uses an approved container or audited equivalent;
2. exposure sites have reviewed permits and purposes;
3. release-mode compiled code passes the project’s constant-time checker;
4. dynamic and statistical tests cover the supported target classes;
5. operating-system and hardware mitigations satisfy the deployment contract;
6. independent review confirms secrets do not enter logs, errors, receipts, dumps, or canonical state.
