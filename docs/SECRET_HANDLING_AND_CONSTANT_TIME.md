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
+ ExposurePermit(authority, purpose, maximum bytes)
+ non-secret secret identity
+ explicit closure
    -> operation result
    + ExposureEvent
```

The event commits the public secret identity, exact permit, and public byte count. It does not contain or hash the secret value.

An exposure permit does not make an unsafe use correct. It creates a reviewable authority boundary and an audit value that project policy can admit or reject.

## Fixed and dynamic containers

`SecretBytes<N>` should be preferred whenever secret length may itself carry information. Equality, selection, and assignment visit the complete fixed array and return or consume `subtle::Choice` values.

`SecretBox` supports dynamic buffers when length is explicitly public. Its equality operation returns immediately when public lengths differ. Projects must not use it when length is secret.

Both containers:

- own their storage;
- are non-cloneable and non-formattable;
- zeroize on drop;
- support explicit clearing and replacement;
- require a permit for byte exposure.

## Dependencies

The implementation delegates low-level behavior to pinned, purpose-specific libraries:

- `zeroize = 1.9.0` for compiler-resistant erasure;
- `subtle = 2.6.1` for `Choice`, constant-time equality, and conditional assignment.

ZenoFCIS does not hand-roll cryptographic algorithms or claim stronger guarantees than those libraries provide.

## Residual risks

Zeroization cannot erase copies created before ownership transfer, compiler spills outside the owned object, swap, crash dumps, hibernation images, device DMA, caches, speculative state, or external copies made by the exposure closure.

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
