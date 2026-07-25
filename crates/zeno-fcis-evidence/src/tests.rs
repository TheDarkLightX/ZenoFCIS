//! Positive, boundary, and negative tests for evidence envelopes and importers.

use super::*;
use alloc::vec;

fn nonzero_hash(byte: u8) -> Hash32 {
    let mut bytes = [0_u8; 32];
    bytes[0] = byte;
    Hash32::new(bytes)
}

fn valid_bindings() -> SourceBindings {
    SourceBindings::try_new(
        nonzero_hash(1),
        nonzero_hash(2),
        nonzero_hash(3),
        nonzero_hash(4),
    )
    .unwrap_or_else(|e| panic!("bindings: {e}"))
}

fn valid_tool() -> ToolIdentity {
    ToolIdentity::try_new("kani", "0.62.0", nonzero_hash(5))
        .unwrap_or_else(|e| panic!("tool identity: {e}"))
}

fn valid_envelope(
    kind: ToolKind,
    result: EvidenceResult,
    coverage: CoverageDeclaration,
    bindings: SourceBindings,
) -> EvidenceEnvelope {
    let query_id = "query_001";
    let claim_hash = nonzero_hash(10);
    let artifact_digest = nonzero_hash(7);
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
fn source_bindings_reject_zero_source_commit() {
    let error = SourceBindings::try_new(
        Hash32::ZERO,
        nonzero_hash(2),
        nonzero_hash(3),
        nonzero_hash(4),
    );
    assert_eq!(error, Err(EvidenceError::UnboundSourceCommit));
}

#[test]
fn source_bindings_reject_zero_profile() {
    let error = SourceBindings::try_new(
        nonzero_hash(1),
        Hash32::ZERO,
        nonzero_hash(3),
        nonzero_hash(4),
    );
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
fn proven_is_conclusive_success() {
    assert!(EvidenceResult::Proven.is_conclusive_success());
    assert!(!EvidenceResult::Proven.is_blocking());
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
        ToolKind::Z3,
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
        ToolKind::Z3,
        valid_bindings(),
        "query_001",
        nonzero_hash(10),
        vec![],
        EvidenceResult::Proven,
        nonzero_hash(7),
        CoverageDeclaration::Unbounded,
    );
    assert_eq!(error, Err(EvidenceError::UnboundedCoverage));
}

#[test]
fn envelope_rejects_zero_artifact_digest() {
    let error = EvidenceEnvelope::try_new(
        valid_tool(),
        ToolKind::Z3,
        valid_bindings(),
        "query_001",
        nonzero_hash(10),
        vec![],
        EvidenceResult::Proven,
        Hash32::ZERO,
        CoverageDeclaration::Bounded { case_budget: 10 },
    );
    assert_eq!(error, Err(EvidenceError::ZeroArtifactDigest));
}

#[test]
fn envelope_rejects_zero_claim_hash() {
    let error = EvidenceEnvelope::try_new(
        valid_tool(),
        ToolKind::Z3,
        valid_bindings(),
        "query_001",
        Hash32::ZERO,
        vec![],
        EvidenceResult::Proven,
        nonzero_hash(7),
        CoverageDeclaration::Bounded { case_budget: 10 },
    );
    assert_eq!(error, Err(EvidenceError::ZeroClaimHash));
}

#[test]
fn envelope_rejects_empty_query_id() {
    let error = EvidenceEnvelope::try_new(
        valid_tool(),
        ToolKind::Z3,
        valid_bindings(),
        "",
        nonzero_hash(10),
        vec![],
        EvidenceResult::Proven,
        nonzero_hash(7),
        CoverageDeclaration::Bounded { case_budget: 10 },
    );
    assert_eq!(error, Err(EvidenceError::InvalidQueryId));
}

#[test]
fn envelope_accepts_valid_construction() {
    let envelope = valid_envelope(
        ToolKind::Kani,
        EvidenceResult::Proven,
        CoverageDeclaration::Bounded { case_budget: 10 },
        valid_bindings(),
    );
    assert_eq!(envelope.result(), EvidenceResult::Proven);
    assert_eq!(envelope.kind(), ToolKind::Kani);
}

#[test]
fn envelope_round_trips_canonical_encoding() {
    let envelope = valid_envelope(
        ToolKind::Lean,
        EvidenceResult::Proven,
        CoverageDeclaration::ProofAssisted {
            theorem_claim: nonzero_hash(8),
        },
        valid_bindings(),
    );
    let bytes = envelope
        .canonical_bytes()
        .unwrap_or_else(|e| panic!("encode: {e}"));
    assert!(!bytes.is_empty());
    assert!(bytes.len() > 100);
}

// ---------------------------------------------------------------------------
// Importer tests
// ---------------------------------------------------------------------------

#[test]
fn importer_rejects_stale_source_commit() {
    let bindings = valid_bindings();
    let mut importer =
        EvidenceImporter::try_new(bindings).unwrap_or_else(|e| panic!("importer: {e}"));
    let stale_bindings = SourceBindings::try_new(
        nonzero_hash(99),
        nonzero_hash(2),
        nonzero_hash(3),
        nonzero_hash(4),
    )
    .unwrap_or_else(|e| panic!("stale bindings: {e}"));
    let envelope = valid_envelope(
        ToolKind::Z3,
        EvidenceResult::Proven,
        CoverageDeclaration::Bounded { case_budget: 10 },
        stale_bindings,
    );
    let result = importer.import(vec![envelope], &StructuralChecker);
    assert_eq!(result, Err(EvidenceError::StaleSourceCommit));
}

#[test]
fn importer_rejects_profile_mismatch() {
    let bindings = valid_bindings();
    let mut importer =
        EvidenceImporter::try_new(bindings).unwrap_or_else(|e| panic!("importer: {e}"));
    let bad_bindings = SourceBindings::try_new(
        nonzero_hash(1),
        nonzero_hash(88),
        nonzero_hash(3),
        nonzero_hash(4),
    )
    .unwrap_or_else(|e| panic!("bad bindings: {e}"));
    let envelope = valid_envelope(
        ToolKind::Z3,
        EvidenceResult::Proven,
        CoverageDeclaration::Bounded { case_budget: 10 },
        bad_bindings,
    );
    let result = importer.import(vec![envelope], &StructuralChecker);
    assert_eq!(result, Err(EvidenceError::ProfileMismatch));
}

#[test]
fn importer_rejects_failed_artifact_check() {
    let bindings = valid_bindings();
    let mut importer =
        EvidenceImporter::try_new(bindings).unwrap_or_else(|e| panic!("importer: {e}"));
    let envelope = valid_envelope(
        ToolKind::Z3,
        EvidenceResult::Proven,
        CoverageDeclaration::Bounded { case_budget: 10 },
        bindings,
    );
    let result = importer.import(vec![envelope], &RejectAllChecker);
    assert_eq!(
        result,
        Err(EvidenceError::ArtifactCheckFailed { kind: ToolKind::Z3 })
    );
}

#[test]
fn importer_rejects_duplicate_tool_kind() {
    let bindings = valid_bindings();
    let mut importer =
        EvidenceImporter::try_new(bindings).unwrap_or_else(|e| panic!("importer: {e}"));
    let envelope = valid_envelope(
        ToolKind::Z3,
        EvidenceResult::Proven,
        CoverageDeclaration::Bounded { case_budget: 10 },
        bindings,
    );
    importer
        .import(vec![envelope], &StructuralChecker)
        .unwrap_or_else(|e| panic!("first import: {e}"));
    let duplicate = valid_envelope(
        ToolKind::Z3,
        EvidenceResult::Proven,
        CoverageDeclaration::Bounded { case_budget: 20 },
        bindings,
    );
    let result = importer.import(vec![duplicate], &StructuralChecker);
    assert_eq!(
        result,
        Err(EvidenceError::DuplicateToolKind { kind: ToolKind::Z3 })
    );
}

#[test]
fn importer_accepts_valid_envelopes() {
    let bindings = valid_bindings();
    let mut importer =
        EvidenceImporter::try_new(bindings).unwrap_or_else(|e| panic!("importer: {e}"));
    let z3 = valid_envelope(
        ToolKind::Z3,
        EvidenceResult::Proven,
        CoverageDeclaration::Bounded { case_budget: 10 },
        bindings,
    );
    let lean = valid_envelope(
        ToolKind::Lean,
        EvidenceResult::Proven,
        CoverageDeclaration::ProofAssisted {
            theorem_claim: nonzero_hash(8),
        },
        bindings,
    );
    importer
        .import(vec![z3, lean], &StructuralChecker)
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
        ToolKind::RuntimeRefinement,
        EvidenceResult::Proven,
        CoverageDeclaration::ExhaustiveFinite {
            domain_hash: nonzero_hash(9),
            cardinality: 1,
        },
        bindings,
    );
    importer
        .import(vec![runtime], &StructuralChecker)
        .unwrap_or_else(|e| panic!("import: {e}"));
    assert!(importer.has_runtime_refinement());
}

#[test]
fn importer_converts_to_tool_evidence() {
    let bindings = valid_bindings();
    let mut importer =
        EvidenceImporter::try_new(bindings).unwrap_or_else(|e| panic!("importer: {e}"));
    let z3 = valid_envelope(
        ToolKind::Z3,
        EvidenceResult::Proven,
        CoverageDeclaration::Bounded { case_budget: 10 },
        bindings,
    );
    importer
        .import(vec![z3], &StructuralChecker)
        .unwrap_or_else(|e| panic!("import: {e}"));
    let tool_evidence = importer.to_tool_evidence();
    assert_eq!(tool_evidence.len(), 1);
    assert_eq!(tool_evidence[0].kind(), ToolKind::Z3);
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
    let gate = PromotionGate::try_new(vec![ToolKind::Z3, ToolKind::Lean], false)
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
        ToolKind::Z3,
        EvidenceResult::Proven,
        CoverageDeclaration::Bounded { case_budget: 10 },
        bindings,
    );
    let runtime = valid_envelope(
        ToolKind::RuntimeRefinement,
        EvidenceResult::Proven,
        CoverageDeclaration::ExhaustiveFinite {
            domain_hash: nonzero_hash(9),
            cardinality: 1,
        },
        bindings,
    );
    importer
        .import(vec![z3, runtime], &StructuralChecker)
        .unwrap_or_else(|e| panic!("import: {e}"));
    let gate =
        PromotionGate::try_new(vec![ToolKind::Z3], true).unwrap_or_else(|e| panic!("gate: {e}"));
    assert!(gate.is_satisfied(&importer));
}

#[test]
fn promotion_gate_rejects_duplicate_tools() {
    let error = PromotionGate::try_new(vec![ToolKind::Z3, ToolKind::Z3], false);
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
        ToolKind::Z3,
        EvidenceResult::Proven,
        CoverageDeclaration::Bounded { case_budget: 10 },
        bindings,
    );
    importer
        .import(vec![bounded], &StructuralChecker)
        .unwrap_or_else(|e| panic!("import: {e}"));
    let exhaustive = valid_envelope(
        ToolKind::Lean,
        EvidenceResult::Proven,
        CoverageDeclaration::ExhaustiveFinite {
            domain_hash: nonzero_hash(9),
            cardinality: 100,
        },
        bindings,
    );
    importer
        .import(vec![exhaustive], &StructuralChecker)
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
        ToolKind::Z3,
        EvidenceResult::Proven,
        CoverageDeclaration::Bounded { case_budget: 10 },
        valid_bindings(),
    );
    assert!(!RejectAllChecker.check(&envelope));
}

#[test]
fn structural_checker_validates_artifact_and_result() {
    let bindings = valid_bindings();
    let mut importer =
        EvidenceImporter::try_new(bindings).unwrap_or_else(|e| panic!("importer: {e}"));
    let envelope = valid_envelope(
        ToolKind::Z3,
        EvidenceResult::Proven,
        CoverageDeclaration::Bounded { case_budget: 10 },
        bindings,
    );
    assert!(StructuralChecker.check(&envelope));
    importer
        .import(vec![envelope], &StructuralChecker)
        .unwrap_or_else(|e| panic!("import: {e}"));
    assert_eq!(importer.envelopes().len(), 1);
}

// ---------------------------------------------------------------------------
// All tool kinds test
// ---------------------------------------------------------------------------

#[test]
fn all_tool_kinds_can_be_enveloped() {
    let kinds = [
        ToolKind::Z3,
        ToolKind::Cvc5,
        ToolKind::Lean,
        ToolKind::Kani,
        ToolKind::TranslationValidation,
        ToolKind::CodecVectors,
        ToolKind::RuntimeRefinement,
    ];
    for kind in kinds {
        let envelope = valid_envelope(
            kind,
            EvidenceResult::Proven,
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
    let bad_bindings = SourceBindings::try_new(
        nonzero_hash(1),
        nonzero_hash(2),
        nonzero_hash(99),
        nonzero_hash(4),
    )
    .unwrap_or_else(|e| panic!("bad bindings: {e}"));
    let envelope = valid_envelope(
        ToolKind::Z3,
        EvidenceResult::Proven,
        CoverageDeclaration::Bounded { case_budget: 10 },
        bad_bindings,
    );
    let result = importer.import(vec![envelope], &StructuralChecker);
    assert_eq!(result, Err(EvidenceError::SchemaMismatch));
}

#[test]
fn importer_rejects_algorithm_mismatch() {
    let bindings = valid_bindings();
    let mut importer =
        EvidenceImporter::try_new(bindings).unwrap_or_else(|e| panic!("importer: {e}"));
    let bad_bindings = SourceBindings::try_new(
        nonzero_hash(1),
        nonzero_hash(2),
        nonzero_hash(3),
        nonzero_hash(99),
    )
    .unwrap_or_else(|e| panic!("bad bindings: {e}"));
    let envelope = valid_envelope(
        ToolKind::Z3,
        EvidenceResult::Proven,
        CoverageDeclaration::Bounded { case_budget: 10 },
        bad_bindings,
    );
    let result = importer.import(vec![envelope], &StructuralChecker);
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
        ToolKind::Z3,
        valid_bindings(),
        "query_001",
        nonzero_hash(10),
        assumptions,
        EvidenceResult::Proven,
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
        ToolKind::Z3,
        ToolKind::Cvc5,
        ToolKind::Lean,
        ToolKind::Kani,
        ToolKind::TranslationValidation,
        ToolKind::CodecVectors,
        ToolKind::RuntimeRefinement,
    ];
    let mut envelopes = Vec::new();
    for i in 0..65u8 {
        let kind = all_kinds[i as usize % all_kinds.len()];
        let env = valid_envelope(
            kind,
            EvidenceResult::Proven,
            CoverageDeclaration::Bounded { case_budget: 10 },
            bindings,
        );
        envelopes.push(env);
    }
    let result = importer.import(envelopes, &StructuralChecker);
    assert_eq!(result, Err(EvidenceError::TooManyEnvelopes));
}

#[test]
fn best_coverage_prefers_proof_assisted_over_bounded() {
    let bindings = valid_bindings();
    let mut importer =
        EvidenceImporter::try_new(bindings).unwrap_or_else(|e| panic!("importer: {e}"));
    let bounded = valid_envelope(
        ToolKind::Z3,
        EvidenceResult::Proven,
        CoverageDeclaration::Bounded { case_budget: 10 },
        bindings,
    );
    importer
        .import(vec![bounded], &StructuralChecker)
        .unwrap_or_else(|e| panic!("import: {e}"));
    let proof = valid_envelope(
        ToolKind::Lean,
        EvidenceResult::Proven,
        CoverageDeclaration::ProofAssisted {
            theorem_claim: nonzero_hash(8),
        },
        bindings,
    );
    importer
        .import(vec![proof], &StructuralChecker)
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
        ToolKind::Z3,
        EvidenceResult::Proven,
        CoverageDeclaration::ExhaustiveFinite {
            domain_hash: nonzero_hash(9),
            cardinality: 100,
        },
        bindings,
    );
    importer
        .import(vec![exhaustive], &StructuralChecker)
        .unwrap_or_else(|e| panic!("import: {e}"));
    let proof = valid_envelope(
        ToolKind::Lean,
        EvidenceResult::Proven,
        CoverageDeclaration::ProofAssisted {
            theorem_claim: nonzero_hash(8),
        },
        bindings,
    );
    importer
        .import(vec![proof], &StructuralChecker)
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
        ToolKind::Z3,
        valid_bindings(),
        "query_001",
        nonzero_hash(10),
        vec![],
        EvidenceResult::Proven,
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
        ToolKind::Z3,
        valid_bindings(),
        "query_001",
        nonzero_hash(10),
        vec![],
        EvidenceResult::Proven,
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
        ToolKind::Z3,
        valid_bindings(),
        "query_001",
        nonzero_hash(10),
        vec![],
        EvidenceResult::Proven,
        nonzero_hash(7),
        CoverageDeclaration::Bounded { case_budget: 0 },
    );
    assert_eq!(error, Err(EvidenceError::ZeroCaseBudget));
}

#[test]
fn envelope_rejects_zero_theorem_claim() {
    let error = EvidenceEnvelope::try_new(
        valid_tool(),
        ToolKind::Z3,
        valid_bindings(),
        "query_001",
        nonzero_hash(10),
        vec![],
        EvidenceResult::Proven,
        nonzero_hash(7),
        CoverageDeclaration::ProofAssisted {
            theorem_claim: Hash32::ZERO,
        },
    );
    assert_eq!(error, Err(EvidenceError::ZeroTheoremClaim));
}

#[test]
fn source_bindings_reject_zero_schema() {
    let error = SourceBindings::try_new(
        nonzero_hash(1),
        nonzero_hash(2),
        Hash32::ZERO,
        nonzero_hash(4),
    );
    assert_eq!(error, Err(EvidenceError::UnboundSchema));
}

#[test]
fn source_bindings_reject_zero_algorithm() {
    let error = SourceBindings::try_new(
        nonzero_hash(1),
        nonzero_hash(2),
        nonzero_hash(3),
        Hash32::ZERO,
    );
    assert_eq!(error, Err(EvidenceError::UnboundAlgorithm));
}
