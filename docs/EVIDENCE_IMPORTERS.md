# Retained evidence and external attestations

V2 calls the successful externally reported result `EvidenceResult::Attested`.
`EvidenceKind` replaces `ToolKind`; `AttestedLawEvidence` replaces the old law
evidence status name. `DecisionCoverageStatus::AttestedUnreachable` records an
external reachability assertion. None of these names has a deprecated alias.
Ordinal tags are unchanged; a successful tag does not mean kernel proof.

`SourceBindings` contains exactly the profile, schema, and algorithm digests,
all nonzero. The old `source_commit` field and constructor argument are gone.
These are declared protocol bindings, not an automatically established source
revision. `EvidenceEnvelope` records producer identity, those bindings, exact
claim/query, assumptions, result, artifact digest, and declared coverage. It is
untrusted metadata until a consumer checks the corresponding bytes and subject.
Its V2 canonical encoding begins with `ZFCIS-EVIDENCE\0` and version 2, and
omits the former source-commit digest. Old envelope encodings are not V2 evidence.

`EvidenceArtifact::new::<H>(bytes)` takes ownership of exact immutable bytes
and computes their digest. Its fields are private. It has no constructor from
only a digest and exposes no mutable byte access. A consumer recomputes the
digest using its selected provider; a caller-selected provider cannot bypass
that check. `EvidenceInput` pairs this container with an untrusted envelope.

`EvidenceImporter::import::<H, C>` applies these checks to an entire batch:

1. The count must fit the existing envelope bound.
2. Profile, schema, and algorithm must match the importer's bindings.
3. The artifact's cached digest must recompute under `H` and equal the envelope.
4. `EvidenceChecker::check(envelope, artifact_bytes)` must accept those bytes.
5. No evidence kind may duplicate an existing or earlier batch item.

Only after every check succeeds does the importer replace its retained inputs.
Any refusal leaves its previous evidence unchanged. `inputs()` retains the
bytes alongside each envelope; `envelopes()` is a borrowed iterator. Conversion
to refinement `ToolEvidence` preserves the bytes. An envelope alone cannot
produce that proposal without an artifact.

The same custody applies to composition and footprint evidence.
`EvidenceVerifier` and `FootprintEvidenceVerifier` receive the complete claim
and exact artifact bytes. Complete-footprint admission checks the full expected
binding, then the pinned verifier identity, then the recomputed digest, before
calling the external verifier. Refinement `ProofVerifier` likewise receives
`&ToolEvidence` and bytes; both promotion evaluators check the digest first.
No hash-only verifier implementation satisfies these V2 traits.

`RejectAllChecker` always refuses. `StructuralChecker` remains a structural
fixture: it does not verify a theorem or inspect artifact semantics. The
library's digest check still precedes it. A successful external callback is an
attestation under that callback's trusted semantics. Neither that callback,
importer, nor a legacy promotion report creates a V2 Authority capability.
Callers must select an appropriate hash provider and checker, retain the real
artifact, and bind the full intended subject. Claimed tool binary hashes alone
do not prove which executable ran.

Bounds remain 64-byte ASCII tool names/versions, 128-byte ASCII query IDs,
32 assumptions of at most 256 ASCII bytes each, and 64 imported envelopes.
The artifact container adds no arbitrary byte cap; callers must bound retained
artifact allocation. Law evidence keeps its separate explicit byte budget.
All code is `no_std + alloc`, without tool execution or I/O.

Native negative controls cover schema/profile/algorithm substitution, altered
artifact bytes, mismatched providers, rejection before external callbacks,
failed checks, duplicate kinds, immutable custody, and retired hash-only APIs.
These checks establish the tested native integrity behavior. They do not prove
an external tool result, establish whole-workspace qualification, or validate
historical evidence under the new canonical formats.
