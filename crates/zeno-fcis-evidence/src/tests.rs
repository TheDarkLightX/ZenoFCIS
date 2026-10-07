//! Positive, boundary, and negative tests for evidence envelopes and importers.

use super::*;
use alloc::vec;

use zeno_fcis_codec::RustCryptoSha256 as TestHasher;

const ARTIFACT: &[u8] = b"retained native evidence fixture";
fn inputs(envelopes: Vec<EvidenceEnvelope>) -> Vec<EvidenceInput> {
    envelopes
        .into_iter()
        .map(|envelope| {
            EvidenceInput::new(
                envelope,
                EvidenceArtifact::new::<TestHasher>(ARTIFACT.to_vec()),
            )
        })
        .collect()
}

fn nonzero_hash(byte: u8) -> Hash32 {
    let mut bytes = [0_u8; 32];
    bytes[0] = byte;
    Hash32::new(bytes)
}

fn valid_bindings() -> SourceBindings {
    SourceBindings::try_new(nonzero_hash(2), nonzero_hash(3), nonzero_hash(4))
        .unwrap_or_else(|e| panic!("bindings: {e}"))
}

fn valid_tool() -> ToolIdentity {
    ToolIdentity::try_new("kani", "0.62.0", nonzero_hash(5))
        .unwrap_or_else(|e| panic!("tool identity: {e}"))
}

fn valid_envelope(
    kind: EvidenceKind,
    result: EvidenceResult,
    coverage: CoverageDeclaration,
    bindings: SourceBindings,
) -> EvidenceEnvelope {
    let query_id = "query_001";
    let claim_hash = nonzero_hash(10);
    let artifact_digest = TestHasher::hash(ARTIFACT);
    EvidenceEnvelope::try_new(
        valid_tool(),
        kind,
        bindings,
        query_id,
        claim_hash,
        vec![
            Assumption::try_new("axiom_1", nonzero_hash(6))
                .unwrap_or_else(|e| panic!("assumption: {e}")),
        ],
        result,
        artifact_digest,
        coverage,
    )
    .unwrap_or_else(|e| panic!("envelope: {e}"))
}

// ---------------------------------------------------------------------------
// Tool identity tests
// ---------------------------------------------------------------------------

#[test]
fn tool_identity_rejects_empty_name() {
    let error = ToolIdentity::try_new("", "1.0", nonzero_hash(1));
    assert_eq!(error, Err(EvidenceError::InvalidToolName));
}

#[test]
fn tool_identity_rejects_non_ascii_name() {
    let error = ToolIdentity::try_new("kaniñ", "1.0", nonzero_hash(1));
    assert_eq!(error, Err(EvidenceError::InvalidToolName));
}

#[test]
fn tool_identity_rejects_zero_binary_hash() {
    let error = ToolIdentity::try_new("kani", "1.0", Hash32::ZERO);
    assert_eq!(error, Err(EvidenceError::ZeroBinaryHash));
}

#[test]
fn tool_identity_accepts_valid_fields() {
    let tool =
        ToolIdentity::try_new("lean", "4.15.0", nonzero_hash(1)).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(tool.name(), "lean");
    assert_eq!(tool.version(), "4.15.0");
}

// ---------------------------------------------------------------------------
// Source bindings tests
// ---------------------------------------------------------------------------

#[test]
fn source_bindings_reject_zero_profile() {
    let error = SourceBindings::try_new(Hash32::ZERO, nonzero_hash(3), nonzero_hash(4));
    assert_eq!(error, Err(EvidenceError::UnboundProfile));
}

#[test]
fn source_bindings_accept_all_nonzero() {
    assert!(valid_bindings().validate().is_ok());
}

// ---------------------------------------------------------------------------
// Evidence result tests
// ---------------------------------------------------------------------------

#[test]
fn attestation_is_reported_as_success() {
    assert!(EvidenceResult::Attested.is_conclusive_success());
    assert!(!EvidenceResult::Attested.is_blocking());
}

#[test]
fn disproven_is_blocking() {
    assert!(EvidenceResult::Disproven.is_blocking());
    assert!(!EvidenceResult::Disproven.is_conclusive_success());
}

#[test]
fn timeout_is_blocking() {
    assert!(EvidenceResult::Timeout.is_blocking());
}

#[test]
fn solver_disagreement_is_blocking() {
    assert!(EvidenceResult::SolverDisagreement.is_blocking());
}

// ---------------------------------------------------------------------------
// Coverage declaration tests
// ---------------------------------------------------------------------------

#[test]
fn unbounded_coverage_is_not_admissible() {
    assert!(!CoverageDeclaration::Unbounded.is_admissible());
}

#[test]
fn exhaustive_finite_is_admissible() {
    let cov = CoverageDeclaration::ExhaustiveFinite {
        domain_hash: nonzero_hash(1),
        cardinality: 100,
    };
    assert!(cov.is_admissible());
    assert!(cov.to_coverage_mode().is_some());
}

#[test]
fn unbounded_coverage_returns_none_for_refine() {
    assert!(CoverageDeclaration::Unbounded.to_coverage_mode().is_none());
}

// ---------------------------------------------------------------------------
// Envelope construction tests
// ---------------------------------------------------------------------------

#[test]
fn envelope_rejects_blocking_result() {
    let error = EvidenceEnvelope::try_new(
        valid_tool(),
        EvidenceKind::Z3,
        valid_bindings(),
        "query_001",
        nonzero_hash(10),
        vec![],
        EvidenceResult::Timeout,
        nonzero_hash(7),
        CoverageDeclaration::Bounded { case_budget: 10 },
    );
    assert_eq!(
        error,
        Err(EvidenceError::BlockingResult {
            result: EvidenceResult::Timeout
        })
    );
}

#[test]
fn envelope_rejects_unbounded_coverage() {
    let error = EvidenceEnvelope::try_new(
        valid_tool(),
        EvidenceKind::Z3,
        valid_bindings(),
        "query_001",
        nonzero_hash(10),
        vec![],
        EvidenceResult::Attested,
        nonzero_hash(7),
        CoverageDeclaration::Unbounded,
    );
    assert_eq!(error, Err(EvidenceError::UnboundedCoverage));
}

#[test]
fn envelope_rejects_zero_artifact_digest() {
    let error = EvidenceEnvelope::try_new(
        valid_tool(),
        EvidenceKind::Z3,
        valid_bindings(),
        "query_001",
        nonzero_hash(10),
        vec![],
        EvidenceResult::Attested,
        Hash32::ZERO,
        CoverageDeclaration::Bounded { case_budget: 10 },
    );
    assert_eq!(error, Err(EvidenceError::ZeroArtifactDigest));
}

#[test]
fn envelope_rejects_zero_claim_hash() {
    let error = EvidenceEnvelope::try_new(
        valid_tool(),
        EvidenceKind::Z3,
        valid_bindings(),
        "query_001",
        Hash32::ZERO,
        vec![],
        EvidenceResult::Attested,
        nonzero_hash(7),
        CoverageDeclaration::Bounded { case_budget: 10 },
    );
    assert_eq!(error, Err(EvidenceError::ZeroClaimHash));
}

#[test]
fn envelope_rejects_empty_query_id() {
    let error = EvidenceEnvelope::try_new(
        valid_tool(),
        EvidenceKind::Z3,
        valid_bindings(),
        "",
        nonzero_hash(10),
        vec![],
        EvidenceResult::Attested,
        nonzero_hash(7),
        CoverageDeclaration::Bounded { case_budget: 10 },
    );
    assert_eq!(error, Err(EvidenceError::InvalidQueryId));
}

#[test]
fn envelope_accepts_valid_construction() {
    let envelope = valid_envelope(
        EvidenceKind::Kani,
        EvidenceResult::Attested,
        CoverageDeclaration::Bounded { case_budget: 10 },
        valid_bindings(),
    );
    assert_eq!(envelope.result(), EvidenceResult::Attested);
    assert_eq!(envelope.kind(), EvidenceKind::Kani);
}

#[test]
fn envelope_has_exact_v2_bytes_without_a_source_commit_placeholder() {
    let envelope = valid_envelope(
        EvidenceKind::Lean,
        EvidenceResult::Attested,
        CoverageDeclaration::ProofAssisted {
            theorem_claim: nonzero_hash(8),
        },
        valid_bindings(),
    );
    let bytes = envelope
        .canonical_bytes()
        .unwrap_or_else(|e| panic!("encode: {e}"));
    let mut expected = Vec::from(&b"ZFCIS-EVIDENCE\0"[..]);
    expected.extend_from_slice(&2_u16.to_be_bytes());
    expected.extend_from_slice(&4_u32.to_be_bytes());
    expected.extend_from_slice(b"kani");
    expected.extend_from_slice(&6_u32.to_be_bytes());
    expected.extend_from_slice(b"0.62.0");
    expected.extend_from_slice(nonzero_hash(5).as_bytes());
    expected.push(2); // Lean evidence kind.
    for binding in [2, 3, 4] {
        expected.extend_from_slice(nonzero_hash(binding).as_bytes());
    }
    expected.extend_from_slice(&9_u32.to_be_bytes());
    expected.extend_from_slice(b"query_001");
    expected.extend_from_slice(nonzero_hash(10).as_bytes());
    expected.extend_from_slice(&1_u16.to_be_bytes());
    expected.extend_from_slice(&7_u32.to_be_bytes());
    expected.extend_from_slice(b"axiom_1");
    expected.extend_from_slice(nonzero_hash(6).as_bytes());
    expected.push(0); // Attested, with no kernel-proof claim.
    expected.extend_from_slice(TestHasher::hash(ARTIFACT).as_bytes());
    expected.push(2); // Declared proof-assisted coverage.
    expected.extend_from_slice(nonzero_hash(8).as_bytes());
    assert_eq!(bytes, expected);
}

// ---------------------------------------------------------------------------
// Importer tests
// ---------------------------------------------------------------------------

#[test]
fn importer_rejects_profile_mismatch() {
    let bindings = valid_bindings();
    let mut importer =
        EvidenceImporter::try_new(bindings).unwrap_or_else(|e| panic!("importer: {e}"));
    let bad_bindings = SourceBindings::try_new(nonzero_hash(88), nonzero_hash(3), nonzero_hash(4))
        .unwrap_or_else(|e| panic!("bad bindings: {e}"));
    let envelope = valid_envelope(
        EvidenceKind::Z3,
        EvidenceResult::Attested,
        CoverageDeclaration::Bounded { case_budget: 10 },
        bad_bindings,
    );
    let result = importer.import::<TestHasher, _>(inputs(vec![envelope]), &StructuralChecker);
    assert_eq!(result, Err(EvidenceError::ProfileMismatch));
}

#[test]
fn importer_rejects_failed_artifact_check() {
    let bindings = valid_bindings();
    let mut importer =
        EvidenceImporter::try_new(bindings).unwrap_or_else(|e| panic!("importer: {e}"));
    let envelope = valid_envelope(
        EvidenceKind::Z3,
        EvidenceResult::Attested,
        CoverageDeclaration::Bounded { case_budget: 10 },
        bindings,
    );
    let result = importer.import::<TestHasher, _>(inputs(vec![envelope]), &RejectAllChecker);
    assert_eq!(
        result,
        Err(EvidenceError::ArtifactCheckFailed {
            kind: EvidenceKind::Z3
        })
    );
}

#[test]
fn importer_rejects_duplicate_tool_kind() {
    let bindings = valid_bindings();
    let mut importer =
        EvidenceImporter::try_new(bindings).unwrap_or_else(|e| panic!("importer: {e}"));
    let envelope = valid_envelope(
        EvidenceKind::Z3,
        EvidenceResult::Attested,
        CoverageDeclaration::Bounded { case_budget: 10 },
        bindings,
    );
    importer
        .import::<TestHasher, _>(inputs(vec![envelope]), &StructuralChecker)
        .unwrap_or_else(|e| panic!("first import: {e}"));
    let duplicate = valid_envelope(
        EvidenceKind::Z3,
        EvidenceResult::Attested,
        CoverageDeclaration::Bounded { case_budget: 20 },
        bindings,
    );
    let result = importer.import::<TestHasher, _>(inputs(vec![duplicate]), &StructuralChecker);
    assert_eq!(
        result,
        Err(EvidenceError::DuplicateEvidenceKind {
            kind: EvidenceKind::Z3
        })
    );
}

#[test]
fn importer_accepts_valid_envelopes() {
    let bindings = valid_bindings();
    let mut importer =
        EvidenceImporter::try_new(bindings).unwrap_or_else(|e| panic!("importer: {e}"));
    let z3 = valid_envelope(
        EvidenceKind::Z3,
        EvidenceResult::Attested,
        CoverageDeclaration::Bounded { case_budget: 10 },
        bindings,
    );
    let lean = valid_envelope(
        EvidenceKind::Lean,
        EvidenceResult::Attested,
        CoverageDeclaration::ProofAssisted {
            theorem_claim: nonzero_hash(8),
        },
        bindings,
    );
    importer
        .import::<TestHasher, _>(inputs(vec![z3, lean]), &StructuralChecker)
        .unwrap_or_else(|e| panic!("import: {e}"));
    assert_eq!(importer.envelopes().len(), 2);
}

#[test]
fn importer_tracks_runtime_refinement() {
    let bindings = valid_bindings();
    let mut importer =
        EvidenceImporter::try_new(bindings).unwrap_or_else(|e| panic!("importer: {e}"));
    assert!(!importer.has_runtime_refinement());
    let runtime = valid_envelope(
        EvidenceKind::RuntimeRefinement,
        EvidenceResult::Attested,
        CoverageDeclaration::ExhaustiveFinite {
            domain_hash: nonzero_hash(9),
            cardinality: 1,
        },
        bindings,
    );
    importer
        .import::<TestHasher, _>(inputs(vec![runtime]), &StructuralChecker)
        .unwrap_or_else(|e| panic!("import: {e}"));
    assert!(importer.has_runtime_refinement());
}

#[test]
fn importer_converts_to_tool_evidence() {
    let bindings = valid_bindings();
    let mut importer =
        EvidenceImporter::try_new(bindings).unwrap_or_else(|e| panic!("importer: {e}"));
    let z3 = valid_envelope(
        EvidenceKind::Z3,
        EvidenceResult::Attested,
        CoverageDeclaration::Bounded { case_budget: 10 },
        bindings,
    );
    importer
        .import::<TestHasher, _>(inputs(vec![z3]), &StructuralChecker)
        .unwrap_or_else(|e| panic!("import: {e}"));
    let tool_evidence = importer.to_tool_evidence();
    assert_eq!(tool_evidence.len(), 1);
    assert_eq!(tool_evidence[0].kind(), EvidenceKind::Z3);
    assert_eq!(tool_evidence[0].artifact_bytes(), ARTIFACT);
    assert_eq!(tool_evidence[0].artifact(), TestHasher::hash(ARTIFACT));
}

// ---------------------------------------------------------------------------
// Promotion gate tests
// ---------------------------------------------------------------------------

#[test]
fn promotion_gate_requires_runtime_refinement() {
    let bindings = valid_bindings();
    let importer = EvidenceImporter::try_new(bindings).unwrap_or_else(|e| panic!("importer: {e}"));
    let gate = PromotionGate::try_new(vec![], true).unwrap_or_else(|e| panic!("gate: {e}"));
    assert!(!gate.is_satisfied(&importer));
    let blockers = gate.evaluate(&importer);
    assert_eq!(blockers, [PromotionBlocker::MissingRuntimeRefinement]);
}

#[test]
fn promotion_gate_requires_all_tools() {
    let bindings = valid_bindings();
    let importer = EvidenceImporter::try_new(bindings).unwrap_or_else(|e| panic!("importer: {e}"));
    let gate = PromotionGate::try_new(vec![EvidenceKind::Z3, EvidenceKind::Lean], false)
        .unwrap_or_else(|e| panic!("gate: {e}"));
    let blockers = gate.evaluate(&importer);
    assert_eq!(blockers.len(), 2);
}

#[test]
fn promotion_gate_satisfied_with_all_evidence() {
    let bindings = valid_bindings();
    let mut importer =
        EvidenceImporter::try_new(bindings).unwrap_or_else(|e| panic!("importer: {e}"));
    let z3 = valid_envelope(
        EvidenceKind::Z3,
        EvidenceResult::Attested,
        CoverageDeclaration::Bounded { case_budget: 10 },
        bindings,
    );
    let runtime = valid_envelope(
        EvidenceKind::RuntimeRefinement,
        EvidenceResult::Attested,
        CoverageDeclaration::ExhaustiveFinite {
            domain_hash: nonzero_hash(9),
            cardinality: 1,
        },
        bindings,
    );
    importer
        .import::<TestHasher, _>(inputs(vec![z3, runtime]), &StructuralChecker)
        .unwrap_or_else(|e| panic!("import: {e}"));
    let gate = PromotionGate::try_new(vec![EvidenceKind::Z3], true)
        .unwrap_or_else(|e| panic!("gate: {e}"));
    assert!(gate.is_satisfied(&importer));
}

#[test]
fn promotion_gate_rejects_duplicate_tools() {
    let error = PromotionGate::try_new(vec![EvidenceKind::Z3, EvidenceKind::Z3], false);
    assert_eq!(error, Err(EvidenceError::InvalidPromotionGate));
}

// ---------------------------------------------------------------------------
// Best coverage tests
// ---------------------------------------------------------------------------

#[test]
fn best_coverage_prefers_exhaustive_over_bounded() {
    let bindings = valid_bindings();
    let mut importer =
        EvidenceImporter::try_new(bindings).unwrap_or_else(|e| panic!("importer: {e}"));
    let bounded = valid_envelope(
        EvidenceKind::Z3,
        EvidenceResult::Attested,
        CoverageDeclaration::Bounded { case_budget: 10 },
        bindings,
    );
    importer
        .import::<TestHasher, _>(inputs(vec![bounded]), &StructuralChecker)
        .unwrap_or_else(|e| panic!("import: {e}"));
    let exhaustive = valid_envelope(
        EvidenceKind::Lean,
        EvidenceResult::Attested,
        CoverageDeclaration::ExhaustiveFinite {
            domain_hash: nonzero_hash(9),
            cardinality: 100,
        },
        bindings,
    );
    importer
        .import::<TestHasher, _>(inputs(vec![exhaustive]), &StructuralChecker)
        .unwrap_or_else(|e| panic!("import: {e}"));
    let best = importer.best_coverage();
    assert!(matches!(
        best,
        Some(CoverageMode::Exhaustive {
            cardinality: 100,
            ..
        })
    ));
}

#[test]
fn best_coverage_returns_none_for_empty_importer() {
    let bindings = valid_bindings();
    let importer = EvidenceImporter::try_new(bindings).unwrap_or_else(|e| panic!("importer: {e}"));
    assert!(importer.best_coverage().is_none());
}

// ---------------------------------------------------------------------------
// Assumption tests
// ---------------------------------------------------------------------------

#[test]
fn assumption_rejects_empty_label() {
    let error = Assumption::try_new("", nonzero_hash(1));
    assert_eq!(error, Err(EvidenceError::InvalidAssumptionLabel));
}

#[test]
fn assumption_rejects_zero_hash() {
    let error = Assumption::try_new("axiom_1", Hash32::ZERO);
    assert_eq!(error, Err(EvidenceError::ZeroAssumptionHash));
}

// ---------------------------------------------------------------------------
// Checker tests
// ---------------------------------------------------------------------------

#[test]
fn reject_all_checker_always_returns_false() {
    let envelope = valid_envelope(
        EvidenceKind::Z3,
        EvidenceResult::Attested,
        CoverageDeclaration::Bounded { case_budget: 10 },
        valid_bindings(),
    );
    assert!(!RejectAllChecker.check(&envelope, ARTIFACT));
}

#[test]
fn structural_checker_validates_artifact_and_result() {
    let bindings = valid_bindings();
    let mut importer =
        EvidenceImporter::try_new(bindings).unwrap_or_else(|e| panic!("importer: {e}"));
    let envelope = valid_envelope(
        EvidenceKind::Z3,
        EvidenceResult::Attested,
        CoverageDeclaration::Bounded { case_budget: 10 },
        bindings,
    );
    assert!(StructuralChecker.check(&envelope, ARTIFACT));
    importer
        .import::<TestHasher, _>(inputs(vec![envelope]), &StructuralChecker)
        .unwrap_or_else(|e| panic!("import: {e}"));
    assert_eq!(importer.envelopes().len(), 1);
}

// ---------------------------------------------------------------------------
// All tool kinds test
// ---------------------------------------------------------------------------

#[test]
fn all_tool_kinds_can_be_enveloped() {
    let kinds = [
        EvidenceKind::Z3,
        EvidenceKind::Cvc5,
        EvidenceKind::Lean,
        EvidenceKind::Kani,
        EvidenceKind::TranslationValidation,
        EvidenceKind::CodecVectors,
        EvidenceKind::RuntimeRefinement,
    ];
    for kind in kinds {
        let envelope = valid_envelope(
            kind,
            EvidenceResult::Attested,
            CoverageDeclaration::Bounded { case_budget: 10 },
            valid_bindings(),
        );
        assert_eq!(envelope.kind(), kind);
    }
}

// ---------------------------------------------------------------------------
// Additional negative tests for complete coverage
// ---------------------------------------------------------------------------

#[test]
fn inconclusive_is_blocking() {
    assert!(EvidenceResult::Inconclusive.is_blocking());
    assert!(!EvidenceResult::Inconclusive.is_conclusive_success());
}

#[test]
fn crash_is_blocking() {
    assert!(EvidenceResult::Crash.is_blocking());
    assert!(!EvidenceResult::Crash.is_conclusive_success());
}

#[test]
fn importer_rejects_schema_mismatch() {
    let bindings = valid_bindings();
    let mut importer =
        EvidenceImporter::try_new(bindings).unwrap_or_else(|e| panic!("importer: {e}"));
    let bad_bindings = SourceBindings::try_new(nonzero_hash(2), nonzero_hash(99), nonzero_hash(4))
        .unwrap_or_else(|e| panic!("bad bindings: {e}"));
    let envelope = valid_envelope(
        EvidenceKind::Z3,
        EvidenceResult::Attested,
        CoverageDeclaration::Bounded { case_budget: 10 },
        bad_bindings,
    );
    let result = importer.import::<TestHasher, _>(inputs(vec![envelope]), &StructuralChecker);
    assert_eq!(result, Err(EvidenceError::SchemaMismatch));
}

#[test]
fn importer_rejects_algorithm_mismatch() {
    let bindings = valid_bindings();
    let mut importer =
        EvidenceImporter::try_new(bindings).unwrap_or_else(|e| panic!("importer: {e}"));
    let bad_bindings = SourceBindings::try_new(nonzero_hash(2), nonzero_hash(3), nonzero_hash(99))
        .unwrap_or_else(|e| panic!("bad bindings: {e}"));
    let envelope = valid_envelope(
        EvidenceKind::Z3,
        EvidenceResult::Attested,
        CoverageDeclaration::Bounded { case_budget: 10 },
        bad_bindings,
    );
    let result = importer.import::<TestHasher, _>(inputs(vec![envelope]), &StructuralChecker);
    assert_eq!(result, Err(EvidenceError::AlgorithmMismatch));
}

#[test]
fn envelope_rejects_too_many_assumptions() {
    let mut assumptions = Vec::new();
    for i in 0..33u8 {
        assumptions.push(
            Assumption::try_new(&alloc::string::String::from("a"), nonzero_hash(i + 1))
                .unwrap_or_else(|e| panic!("assumption: {e}")),
        );
    }
    let error = EvidenceEnvelope::try_new(
        valid_tool(),
        EvidenceKind::Z3,
        valid_bindings(),
        "query_001",
        nonzero_hash(10),
        assumptions,
        EvidenceResult::Attested,
        nonzero_hash(7),
        CoverageDeclaration::Bounded { case_budget: 10 },
    );
    assert_eq!(error, Err(EvidenceError::TooManyAssumptions));
}

#[test]
fn importer_rejects_too_many_envelopes() {
    let bindings = valid_bindings();
    let mut importer =
        EvidenceImporter::try_new(bindings).unwrap_or_else(|e| panic!("importer: {e}"));
    let all_kinds = [
        EvidenceKind::Z3,
        EvidenceKind::Cvc5,
        EvidenceKind::Lean,
        EvidenceKind::Kani,
        EvidenceKind::TranslationValidation,
        EvidenceKind::CodecVectors,
        EvidenceKind::RuntimeRefinement,
    ];
    let mut envelopes = Vec::new();
    for i in 0..65u8 {
        let kind = all_kinds[i as usize % all_kinds.len()];
        let env = valid_envelope(
            kind,
            EvidenceResult::Attested,
            CoverageDeclaration::Bounded { case_budget: 10 },
            bindings,
        );
        envelopes.push(env);
    }
    let result = importer.import::<TestHasher, _>(inputs(envelopes), &StructuralChecker);
    assert_eq!(result, Err(EvidenceError::TooManyEnvelopes));
}

#[test]
fn best_coverage_prefers_proof_assisted_over_bounded() {
    let bindings = valid_bindings();
    let mut importer =
        EvidenceImporter::try_new(bindings).unwrap_or_else(|e| panic!("importer: {e}"));
    let bounded = valid_envelope(
        EvidenceKind::Z3,
        EvidenceResult::Attested,
        CoverageDeclaration::Bounded { case_budget: 10 },
        bindings,
    );
    importer
        .import::<TestHasher, _>(inputs(vec![bounded]), &StructuralChecker)
        .unwrap_or_else(|e| panic!("import: {e}"));
    let proof = valid_envelope(
        EvidenceKind::Lean,
        EvidenceResult::Attested,
        CoverageDeclaration::ProofAssisted {
            theorem_claim: nonzero_hash(8),
        },
        bindings,
    );
    importer
        .import::<TestHasher, _>(inputs(vec![proof]), &StructuralChecker)
        .unwrap_or_else(|e| panic!("import: {e}"));
    let best = importer.best_coverage();
    assert!(matches!(best, Some(CoverageMode::ProofAssisted { .. })));
}

#[test]
fn best_coverage_keeps_exhaustive_over_proof_assisted() {
    let bindings = valid_bindings();
    let mut importer =
        EvidenceImporter::try_new(bindings).unwrap_or_else(|e| panic!("importer: {e}"));
    let exhaustive = valid_envelope(
        EvidenceKind::Z3,
        EvidenceResult::Attested,
        CoverageDeclaration::ExhaustiveFinite {
            domain_hash: nonzero_hash(9),
            cardinality: 100,
        },
        bindings,
    );
    importer
        .import::<TestHasher, _>(inputs(vec![exhaustive]), &StructuralChecker)
        .unwrap_or_else(|e| panic!("import: {e}"));
    let proof = valid_envelope(
        EvidenceKind::Lean,
        EvidenceResult::Attested,
        CoverageDeclaration::ProofAssisted {
            theorem_claim: nonzero_hash(8),
        },
        bindings,
    );
    importer
        .import::<TestHasher, _>(inputs(vec![proof]), &StructuralChecker)
        .unwrap_or_else(|e| panic!("import: {e}"));
    let best = importer.best_coverage();
    assert!(matches!(
        best,
        Some(CoverageMode::Exhaustive {
            cardinality: 100,
            ..
        })
    ));
}

#[test]
fn envelope_rejects_zero_domain_hash() {
    let error = EvidenceEnvelope::try_new(
        valid_tool(),
        EvidenceKind::Z3,
        valid_bindings(),
        "query_001",
        nonzero_hash(10),
        vec![],
        EvidenceResult::Attested,
        nonzero_hash(7),
        CoverageDeclaration::ExhaustiveFinite {
            domain_hash: Hash32::ZERO,
            cardinality: 100,
        },
    );
    assert_eq!(error, Err(EvidenceError::ZeroDomainHash));
}

#[test]
fn envelope_rejects_zero_cardinality() {
    let error = EvidenceEnvelope::try_new(
        valid_tool(),
        EvidenceKind::Z3,
        valid_bindings(),
        "query_001",
        nonzero_hash(10),
        vec![],
        EvidenceResult::Attested,
        nonzero_hash(7),
        CoverageDeclaration::ExhaustiveFinite {
            domain_hash: nonzero_hash(1),
            cardinality: 0,
        },
    );
    assert_eq!(error, Err(EvidenceError::ZeroCardinality));
}

#[test]
fn envelope_rejects_zero_case_budget() {
    let error = EvidenceEnvelope::try_new(
        valid_tool(),
        EvidenceKind::Z3,
        valid_bindings(),
        "query_001",
        nonzero_hash(10),
        vec![],
        EvidenceResult::Attested,
        nonzero_hash(7),
        CoverageDeclaration::Bounded { case_budget: 0 },
    );
    assert_eq!(error, Err(EvidenceError::ZeroCaseBudget));
}

#[test]
fn envelope_rejects_zero_theorem_claim() {
    let error = EvidenceEnvelope::try_new(
        valid_tool(),
        EvidenceKind::Z3,
        valid_bindings(),
        "query_001",
        nonzero_hash(10),
        vec![],
        EvidenceResult::Attested,
        nonzero_hash(7),
        CoverageDeclaration::ProofAssisted {
            theorem_claim: Hash32::ZERO,
        },
    );
    assert_eq!(error, Err(EvidenceError::ZeroTheoremClaim));
}

#[test]
fn source_bindings_reject_zero_schema() {
    let error = SourceBindings::try_new(nonzero_hash(2), Hash32::ZERO, nonzero_hash(4));
    assert_eq!(error, Err(EvidenceError::UnboundSchema));
}

#[test]
fn source_bindings_reject_zero_algorithm() {
    let error = SourceBindings::try_new(nonzero_hash(2), nonzero_hash(3), Hash32::ZERO);
    assert_eq!(error, Err(EvidenceError::UnboundAlgorithm));
}

// ---------------------------------------------------------------------------
// Builder pattern tests
// ---------------------------------------------------------------------------

#[test]
fn builder_creates_valid_envelope() {
    let envelope = EvidenceEnvelopeBuilder::new(valid_tool(), EvidenceKind::Kani, valid_bindings())
        .query_id("theorem_001")
        .claim_hash(nonzero_hash(10))
        .artifact_digest(nonzero_hash(7))
        .result(EvidenceResult::Attested)
        .coverage(CoverageDeclaration::Bounded { case_budget: 100 })
        .build()
        .unwrap_or_else(|e| panic!("builder: {e}"));
    assert_eq!(envelope.kind(), EvidenceKind::Kani);
    assert_eq!(envelope.query_id(), "theorem_001");
}

#[test]
fn builder_rejects_missing_query_id() {
    let error = EvidenceEnvelopeBuilder::new(valid_tool(), EvidenceKind::Kani, valid_bindings())
        .claim_hash(nonzero_hash(10))
        .artifact_digest(nonzero_hash(7))
        .result(EvidenceResult::Attested)
        .coverage(CoverageDeclaration::Bounded { case_budget: 100 })
        .build();
    assert_eq!(error, Err(EvidenceError::MissingQueryId));
}

#[test]
fn builder_rejects_missing_claim_hash() {
    let error = EvidenceEnvelopeBuilder::new(valid_tool(), EvidenceKind::Kani, valid_bindings())
        .query_id("theorem_001")
        .artifact_digest(nonzero_hash(7))
        .result(EvidenceResult::Attested)
        .coverage(CoverageDeclaration::Bounded { case_budget: 100 })
        .build();
    assert_eq!(error, Err(EvidenceError::MissingClaimHash));
}

#[test]
fn builder_rejects_missing_result() {
    let error = EvidenceEnvelopeBuilder::new(valid_tool(), EvidenceKind::Kani, valid_bindings())
        .query_id("theorem_001")
        .claim_hash(nonzero_hash(10))
        .artifact_digest(nonzero_hash(7))
        .coverage(CoverageDeclaration::Bounded { case_budget: 100 })
        .build();
    assert_eq!(error, Err(EvidenceError::MissingResult));
}

#[test]
fn builder_rejects_missing_artifact_digest() {
    let error = EvidenceEnvelopeBuilder::new(valid_tool(), EvidenceKind::Kani, valid_bindings())
        .query_id("theorem_001")
        .claim_hash(nonzero_hash(10))
        .result(EvidenceResult::Attested)
        .coverage(CoverageDeclaration::Bounded { case_budget: 100 })
        .build();
    assert_eq!(error, Err(EvidenceError::MissingArtifactDigest));
}

#[test]
fn builder_rejects_missing_coverage() {
    let error = EvidenceEnvelopeBuilder::new(valid_tool(), EvidenceKind::Kani, valid_bindings())
        .query_id("theorem_001")
        .claim_hash(nonzero_hash(10))
        .artifact_digest(nonzero_hash(7))
        .result(EvidenceResult::Attested)
        .build();
    assert_eq!(error, Err(EvidenceError::MissingCoverage));
}

#[test]
fn builder_propagates_validation_errors() {
    let error = EvidenceEnvelopeBuilder::new(valid_tool(), EvidenceKind::Kani, valid_bindings())
        .query_id("theorem_001")
        .claim_hash(nonzero_hash(10))
        .artifact_digest(nonzero_hash(7))
        .result(EvidenceResult::Timeout)
        .coverage(CoverageDeclaration::Bounded { case_budget: 100 })
        .build();
    assert_eq!(
        error,
        Err(EvidenceError::BlockingResult {
            result: EvidenceResult::Timeout
        })
    );
}

#[test]
fn builder_adds_assumptions() {
    let a1 = Assumption::try_new("axiom_1", nonzero_hash(6))
        .unwrap_or_else(|e| panic!("assumption: {e}"));
    let a2 = Assumption::try_new("axiom_2", nonzero_hash(7))
        .unwrap_or_else(|e| panic!("assumption: {e}"));
    let envelope = EvidenceEnvelopeBuilder::new(valid_tool(), EvidenceKind::Kani, valid_bindings())
        .query_id("theorem_001")
        .claim_hash(nonzero_hash(10))
        .artifact_digest(nonzero_hash(7))
        .result(EvidenceResult::Attested)
        .coverage(CoverageDeclaration::Bounded { case_budget: 100 })
        .assumption(a1)
        .assumption(a2)
        .build()
        .unwrap_or_else(|e| panic!("builder: {e}"));
    assert_eq!(envelope.assumptions().len(), 2);
}

// ---------------------------------------------------------------------------
// QueryId newtype tests
// ---------------------------------------------------------------------------

#[test]
fn query_id_accepts_valid_string() {
    let qid = QueryId::try_new("theorem_001").unwrap_or_else(|e| panic!("query id: {e}"));
    assert_eq!(qid.as_str(), "theorem_001");
}

#[test]
fn query_id_rejects_empty() {
    assert_eq!(QueryId::try_new(""), Err(EvidenceError::InvalidQueryId));
}

#[test]
fn query_id_rejects_non_ascii() {
    assert_eq!(
        QueryId::try_new("theorem_ñ"),
        Err(EvidenceError::InvalidQueryId)
    );
}

#[test]
fn query_id_display_matches_inner() {
    let qid = QueryId::try_new("lemma_42").unwrap_or_else(|e| panic!("query id: {e}"));
    assert_eq!(alloc::format!("{qid}"), "lemma_42");
}

// ---------------------------------------------------------------------------
// EvidenceResult TryFrom tests
// ---------------------------------------------------------------------------

#[test]
fn evidence_result_try_from_valid_tags() {
    assert_eq!(EvidenceResult::try_from(0), Ok(EvidenceResult::Attested));
    assert_eq!(EvidenceResult::try_from(1), Ok(EvidenceResult::Disproven));
    assert_eq!(
        EvidenceResult::try_from(2),
        Ok(EvidenceResult::Inconclusive)
    );
    assert_eq!(EvidenceResult::try_from(3), Ok(EvidenceResult::Timeout));
    assert_eq!(EvidenceResult::try_from(4), Ok(EvidenceResult::Crash));
    assert_eq!(
        EvidenceResult::try_from(5),
        Ok(EvidenceResult::SolverDisagreement)
    );
}

#[test]
fn evidence_result_try_from_invalid_tag() {
    assert_eq!(
        EvidenceResult::try_from(6),
        Err(EvidenceError::InvalidResultTag)
    );
    assert_eq!(
        EvidenceResult::try_from(255),
        Err(EvidenceError::InvalidResultTag)
    );
}

// ---------------------------------------------------------------------------
// Canonical encoding round-trip tests
// ---------------------------------------------------------------------------

#[test]
fn envelope_canonical_bytes_are_deterministic() {
    let envelope = valid_envelope(
        EvidenceKind::Kani,
        EvidenceResult::Attested,
        CoverageDeclaration::Bounded { case_budget: 10 },
        valid_bindings(),
    );
    let bytes_v1 = envelope
        .canonical_bytes()
        .unwrap_or_else(|e| panic!("encode v1: {e}"));
    let bytes_v2 = envelope
        .canonical_bytes()
        .unwrap_or_else(|e| panic!("encode v2: {e}"));
    assert_eq!(bytes_v1, bytes_v2, "canonical bytes must be deterministic");
}

#[test]
fn envelope_canonical_bytes_differ_for_different_claims() {
    let envelope_a = valid_envelope(
        EvidenceKind::Kani,
        EvidenceResult::Attested,
        CoverageDeclaration::Bounded { case_budget: 10 },
        valid_bindings(),
    );
    let envelope_b = EvidenceEnvelope::try_new(
        valid_tool(),
        EvidenceKind::Kani,
        valid_bindings(),
        "query_001",
        nonzero_hash(99),
        vec![],
        EvidenceResult::Attested,
        nonzero_hash(7),
        CoverageDeclaration::Bounded { case_budget: 10 },
    )
    .unwrap_or_else(|e| panic!("envelope b: {e}"));
    let bytes_a = envelope_a
        .canonical_bytes()
        .unwrap_or_else(|e| panic!("encode a: {e}"));
    let bytes_b = envelope_b
        .canonical_bytes()
        .unwrap_or_else(|e| panic!("encode b: {e}"));
    assert_ne!(
        bytes_a, bytes_b,
        "different claim hashes must produce different canonical bytes"
    );
}

#[test]
fn envelope_canonical_bytes_differ_for_different_results() {
    let envelope_proven = valid_envelope(
        EvidenceKind::Z3,
        EvidenceResult::Attested,
        CoverageDeclaration::Bounded { case_budget: 10 },
        valid_bindings(),
    );
    let envelope_disproven = EvidenceEnvelope::try_new(
        valid_tool(),
        EvidenceKind::Z3,
        valid_bindings(),
        "query_002",
        nonzero_hash(11),
        vec![],
        EvidenceResult::Attested,
        nonzero_hash(8),
        CoverageDeclaration::Bounded { case_budget: 20 },
    )
    .unwrap_or_else(|e| panic!("envelope: {e}"));
    let bytes_proven = envelope_proven
        .canonical_bytes()
        .unwrap_or_else(|e| panic!("encode: {e}"));
    let bytes_disproven = envelope_disproven
        .canonical_bytes()
        .unwrap_or_else(|e| panic!("encode: {e}"));
    assert_ne!(
        bytes_proven, bytes_disproven,
        "different envelopes must produce different canonical bytes"
    );
}

#[test]
fn envelope_canonical_bytes_differ_for_different_coverage() {
    let bounded = valid_envelope(
        EvidenceKind::Kani,
        EvidenceResult::Attested,
        CoverageDeclaration::Bounded { case_budget: 10 },
        valid_bindings(),
    );
    let exhaustive = valid_envelope(
        EvidenceKind::Kani,
        EvidenceResult::Attested,
        CoverageDeclaration::ExhaustiveFinite {
            domain_hash: nonzero_hash(9),
            cardinality: 100,
        },
        valid_bindings(),
    );
    let bytes_bounded = bounded
        .canonical_bytes()
        .unwrap_or_else(|e| panic!("encode: {e}"));
    let bytes_exhaustive = exhaustive
        .canonical_bytes()
        .unwrap_or_else(|e| panic!("encode: {e}"));
    assert_ne!(
        bytes_bounded, bytes_exhaustive,
        "different coverage must produce different canonical bytes"
    );
}

#[test]
fn envelope_canonical_bytes_include_assumptions() {
    let with_assumptions = valid_envelope(
        EvidenceKind::Kani,
        EvidenceResult::Attested,
        CoverageDeclaration::Bounded { case_budget: 10 },
        valid_bindings(),
    );
    let without_assumptions = EvidenceEnvelope::try_new(
        valid_tool(),
        EvidenceKind::Kani,
        valid_bindings(),
        "query_001",
        nonzero_hash(10),
        vec![],
        EvidenceResult::Attested,
        nonzero_hash(7),
        CoverageDeclaration::Bounded { case_budget: 10 },
    )
    .unwrap_or_else(|e| panic!("envelope: {e}"));
    let bytes_with = with_assumptions
        .canonical_bytes()
        .unwrap_or_else(|e| panic!("encode: {e}"));
    let bytes_without = without_assumptions
        .canonical_bytes()
        .unwrap_or_else(|e| panic!("encode: {e}"));
    assert_ne!(
        bytes_with, bytes_without,
        "assumptions must be reflected in canonical bytes"
    );
}

#[test]
fn evidence_result_round_trips_through_u8() {
    for tag in 0..=5_u8 {
        let result =
            EvidenceResult::try_from(tag).unwrap_or_else(|e| panic!("decode tag {tag}: {e}"));
        assert_eq!(result as u8, tag, "round-trip tag {tag} failed");
    }
}

#[test]
fn importer_checks_actual_bytes_before_external_checker_and_preserves_prior_state() {
    use core::cell::Cell;
    struct Checker(Cell<usize>);
    impl EvidenceChecker for Checker {
        fn check(&self, _: &EvidenceEnvelope, bytes: &[u8]) -> bool {
            self.0.set(self.0.get() + 1);
            bytes == ARTIFACT
        }
    }
    let envelope = valid_envelope(
        EvidenceKind::Z3,
        EvidenceResult::Attested,
        CoverageDeclaration::Bounded { case_budget: 1 },
        valid_bindings(),
    );
    let altered = EvidenceInput::new(
        envelope.clone(),
        EvidenceArtifact::new::<TestHasher>(b"altered artifact bytes".to_vec()),
    );
    let checker = Checker(Cell::new(0));
    let mut importer =
        EvidenceImporter::try_new(valid_bindings()).unwrap_or_else(|e| panic!("importer: {e}"));
    let before = importer.clone();
    assert_eq!(
        importer.import::<TestHasher, _>(vec![altered], &checker),
        Err(EvidenceError::ArtifactDigestMismatch {
            kind: EvidenceKind::Z3
        })
    );
    assert_eq!(checker.0.get(), 0);
    assert_eq!(importer, before);
    importer
        .import::<TestHasher, _>(inputs(vec![envelope]), &checker)
        .unwrap_or_else(|e| panic!("valid artifact: {e}"));
    assert_eq!(checker.0.get(), 1);
    assert_eq!(importer.inputs()[0].artifact().bytes(), ARTIFACT);
}

#[test]
fn importer_rejects_artifact_bound_to_different_bytes() {
    let artifact = EvidenceArtifact::new::<TestHasher>([ARTIFACT, &[0]].concat());
    let envelope = EvidenceEnvelope::try_new(
        valid_tool(),
        EvidenceKind::Lean,
        valid_bindings(),
        "query_001",
        nonzero_hash(10),
        vec![],
        EvidenceResult::Attested,
        TestHasher::hash(ARTIFACT),
        CoverageDeclaration::Bounded { case_budget: 1 },
    )
    .unwrap_or_else(|e| panic!("declared envelope: {e}"));
    assert_ne!(artifact.digest(), envelope.artifact_digest());
    assert_ne!(artifact.digest(), TestHasher::hash(ARTIFACT));
    let input = EvidenceInput::new(envelope, artifact);
    let mut importer =
        EvidenceImporter::try_new(valid_bindings()).unwrap_or_else(|e| panic!("importer: {e}"));
    assert_eq!(
        importer.import::<TestHasher, _>(vec![input], &StructuralChecker),
        Err(EvidenceError::ArtifactDigestMismatch {
            kind: EvidenceKind::Lean
        })
    );
}
