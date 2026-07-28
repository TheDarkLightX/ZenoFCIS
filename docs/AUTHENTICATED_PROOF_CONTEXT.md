# Authenticated sparse-proof context

## Purpose

An internally consistent sparse proof is not yet authorization evidence. The
proof must be interpreted against expected context supplied independently by
its consumer. This boundary makes that distinction explicit while leaving
context provenance to the application authority.

## Inputs

`SparseProof::verify_against` receives a `SparseProofContext` containing:

- the complete `AuthenticatedProfile`;
- the expected tree version;
- the expected authenticated root;
- the expected logical key.

The profile contains separate nonzero commitments for the operational tree,
the reviewed dual-root profile, and the declared state projector identity. The
caller that owns the authority boundary supplies this context from its trusted
state or protocol invocation.

## Outputs

Successful verification returns `ContextVerifiedSparseProof`. Its fields are
private, so downstream code cannot construct it without a proof that verifies
against some complete context. It does not prove that the context is trusted:
a caller can build its own tree and matching context. The witness exposes the
exact context and verified membership or absence result so a production
consumer can compare it with authority-owned state.

`verify_internal_consistency` remains available under that explicit name for
diagnostics and reference testing. The ambiguous pre-V1 `verify` method is
removed.

## Authority boundary

The proof producer controls the untrusted `SparseProof`. The proof consumer or
higher production-authority layer supplies `SparseProofContext`. The library
cannot prove where that context came from. A field copied from the proof into
the expected context does not establish an external trust anchor.

The witness grants authenticated-read evidence only. It grants no semantic
patch, candidate, effect, outbox, shell, pruning, or migration authority.

## Deterministic bounds

- every proof has exactly 256 sibling hashes;
- verification performs exactly 256 path-combination steps after fixed-size
  context checks;
- no ambient clock, randomness, filesystem, network, database, or concurrency
  input affects the result;
- this package adds no dependencies.

## Laws

For a returned witness:

1. proof profile equals the expected tree, profile, and declared projector
   identity supplied to the verifier;
2. planner construction rejects a configured projector whose declared
   commitment differs from the mounted profile, and planning requests cannot
   supply a replacement projector;
3. proof version, root, and key equal the expected values;
4. the proof leaf and siblings recompute the expected root;
5. for a fixed proof payload, changing any expected context field causes
   verification to fail;
6. changing the proof leaf or any sibling invalidates verification;
7. a witness cannot be constructed through the public Rust API without
   successful verification.

## Negative cases

Tests cover substitution of tree identity, profile identity, projector
commitment, mismatched declared projector identity, version, root, and key.
Separate mutations change the proof leaf and a sibling hash. A compile-fail
test protects the private witness fields.

## Canonical format change

`AuthenticatedProfile` now commits the projector identity.
`PlannedAuthenticatedCommit` canonical encoding is explicitly version 2 and
includes that projector commitment. This is an intentional pre-V1 incompatible
hardening change from the legacy unversioned pre-release encoding. No strict
plan decoder exists yet, so wire admission and migration remain separate work.

## Trusted dependencies and assumptions

The reference verifier uses the pinned RustCrypto SHA-256 provider through the
existing ZenoFCIS domain-separation API. The authority layer must obtain its
expected root and context from a trusted publication or invocation. The
projector commitment is declared by the configured projector; it does not by
itself attest that the implementation is complete or correct. The configured
planner prevents request-time projector substitution, while production setup
must select and independently verify the concrete implementation. That
implementation is assumed to be pure, deterministic, transitively owned and
immutable, and free of ambient or interior mutable state.

## Explicit nonclaims

- no proof of projector completeness;
- no sealed or independently attested projector implementation;
- no generic enforcement of projector purity, determinism, transitive
  immutability, or freedom from ambient and interior mutable state;
- no proof that a supplied verification context has production provenance;
- no production JMT, persistence, pruning, or crash-recovery implementation;
- no strict sparse-proof decoder;
- no binding between an authenticated plan and a catalog-authorized candidate;
- no unbounded cryptographic or parser proof;
- no protocol meaning or precedence assigned to `AuthError` branch order;
- no production-readiness or V1 qualification claim.
