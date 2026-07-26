# Formal-Evidence Importers — Design Document

## Work Package D (Issue #9)

### Inputs

- A `ToolIdentity` binding the tool name, version, and binary hash.
- A `SourceBindings` struct binding the source commit, profile hash, schema
  hash, and algorithm hash.
- A query identifier (theorem or model-checking query name).
- A claim hash (cryptographic commitment to the theorem/query statement).
- A list of named assumptions with statement hashes.
- An `EvidenceResult` (Proven, Disproven, Inconclusive, Timeout, Crash,
  SolverDisagreement).
- A retained artifact digest (SHA-256 of the proof artifact, model-check
  output, or replay log).
- A `CoverageDeclaration` (ExhaustiveFinite, Bounded, ProofAssisted, Unbounded).

### Outputs

- A canonical `EvidenceEnvelope` that is content-addressed via
  `CanonicalEncode`.
- A `ToolEvidence` value compatible with `zeno-fcis-refine`'s promotion
  pipeline.
- A `PromotionGate` evaluation that fail-closed checks all required tool
  evidence and mounted runtime refinement.

### Authority Boundary

- The **evidence envelope** is the sole authority for what was proved, by
  which tool, under which assumptions, and with what coverage.
- The **independent checker** (`EvidenceChecker` trait) is the sole authority
  for validating the retained artifact. The importer never trusts a tool's
  self-reported result without an independent check.
- The **promotion gate** is the sole authority for determining whether
  evidence is sufficient for promotion. It is fail-closed.
- The **source bindings** anchor evidence to exact protocol artifacts. Stale
  or mismatched bindings are rejected.

### Trusted Dependencies

- `zeno-fcis-codec` for `Hash32`, `CanonicalEncode`, and `EncodeError`.
- `zeno-fcis-refine` for `ToolKind`, `ToolEvidence`, `CoverageMode`, and
  integration with the existing promotion pipeline.

No external dependencies are added. Both dependencies are existing workspace
crates under ZenoFCIS control.

### Deterministic Resource Bounds

- Maximum tool name length: 64 bytes (ASCII).
- Maximum tool version length: 64 bytes (ASCII).
- Maximum query identifier length: 128 bytes (ASCII).
- Maximum assumptions per envelope: 32.
- Maximum assumption label length: 256 bytes (ASCII).
- Maximum envelopes per importer: 64.
- Maximum artifact size: enforced by `CanonicalEncode` length bounds.

### Laws

1. **Fail-closed construction**: envelopes with blocking results, unbound
   bindings, zero digests (claim or artifact), or unbounded coverage are rejected at construction.
2. **Binding consistency**: the importer rejects envelopes whose source
   bindings do not match the importer's bindings (stale commit, profile
   mismatch, schema mismatch, algorithm mismatch).
3. **Independent verification**: the importer rejects envelopes that fail the
   `EvidenceChecker` check. `RejectAllChecker` is the fail-closed default.
4. **No duplicate tools**: the importer rejects envelopes with a tool kind
   already imported.
5. **Runtime refinement required**: the promotion gate requires mounted
   runtime refinement evidence (`ToolKind::RuntimeRefinement`) for any
   production promotion.
6. **Coverage distinction**: exhaustive finite, bounded, proof-assisted, and
   unbounded coverage are explicitly distinguished. Unbounded coverage is
   always rejected.
7. **Canonical encoding**: evidence envelopes implement `CanonicalEncode` for
   content-addressed storage and deterministic comparison.

### Negative Cases

- Empty or non-ASCII tool name → rejected.
- Zero binary hash → rejected.
- Zero source commit, profile, schema, or algorithm hash → rejected.
- Empty query identifier → rejected.
- Zero artifact digest → rejected.
- Inconclusive, timeout, crash, or solver-disagreement result → rejected.
- Unbounded coverage → rejected.
- Stale source commit → rejected by importer.
- Profile/schema/algorithm mismatch → rejected by importer.
- Failed independent artifact check → rejected by importer.
- Duplicate tool kind → rejected by importer.
- Missing required tool evidence → promotion gate blocker.
- Missing runtime refinement → promotion gate blocker.

### Assumptions

- The `EvidenceChecker` implementation is trusted to correctly validate
  retained artifacts. The `StructuralChecker` is a minimal structural check,
  not a proof verification. Production code must supply a real checker.
- The `claim_hash` field is provided by the caller, who is responsible for
  computing a proper cryptographic commitment to the theorem/query statement.
- The promotion gate does not verify the correctness of the proof itself; it
  verifies that the required evidence is present and independently checked.

### Explicit Nonclaims

- An evidence envelope authenticates and classifies a proof result; it does
  not make an incorrect specification true or extend a theorem beyond its
  assumptions.
- The `StructuralChecker` does not verify proof artifacts. It checks only
  structural consistency (artifact digest matches query identity, binary hash
  is non-zero, result is proven).
- The evidence importer does not grant production authority. It produces
  evidence for the promotion gate, which is itself fail-closed.
- The promotion gate does not replace the existing `evaluate_promotion`
  function in `zeno-fcis-refine`. It provides an additional evidence-specific
  gate that complements the refine crate's promotion pipeline.
- No crate is published or claimed as production-ready.
