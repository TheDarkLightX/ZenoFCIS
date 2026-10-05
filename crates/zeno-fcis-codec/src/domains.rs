//! Closed registry of library-owned commitment domains.
//!
//! Every entry fixes both its exact name and version. Public project constructors
//! cannot recreate reserved domains from strings, including names in this registry.
//! Selecting a declared constant grants no publication or verification authority.

use super::Domain;

/// Fixed domain `zeno-fcis/access-map-key`, version 1.
pub const ACCESS_MAP_KEY: Domain<'static> = Domain {
    name: "zeno-fcis/access-map-key",
    version: 1,
};

/// Fixed domain `zeno-fcis/auth-empty`, version 1.
pub const AUTH_EMPTY: Domain<'static> = Domain {
    name: "zeno-fcis/auth-empty",
    version: 1,
};

/// Fixed domain `zeno-fcis/auth-leaf`, version 1.
pub const AUTH_LEAF: Domain<'static> = Domain {
    name: "zeno-fcis/auth-leaf",
    version: 1,
};

/// Fixed domain `zeno-fcis/auth-node`, version 1.
pub const AUTH_NODE: Domain<'static> = Domain {
    name: "zeno-fcis/auth-node",
    version: 1,
};

/// Fixed domain `zeno-fcis/auth-patch`, version 1.
pub const AUTH_PATCH: Domain<'static> = Domain {
    name: "zeno-fcis/auth-patch",
    version: 1,
};

/// Fixed domain `zeno-fcis/authenticated-authority-config`, version 1.
pub const AUTHENTICATED_AUTHORITY_CONFIG: Domain<'static> = Domain {
    name: "zeno-fcis/authenticated-authority-config",
    version: 1,
};

/// Fixed domain `zeno-fcis/authenticated-authorization`, version 1.
pub const AUTHENTICATED_AUTHORIZATION: Domain<'static> = Domain {
    name: "zeno-fcis/authenticated-authorization",
    version: 1,
};

/// Fixed domain `zeno-fcis/authorization-invocation`, version 2.
pub const AUTHORIZATION_INVOCATION: Domain<'static> = Domain {
    name: "zeno-fcis/authorization-invocation",
    version: 2,
};

/// Fixed domain `zeno-fcis/authorization-policy`, version 2.
pub const AUTHORIZATION_POLICY: Domain<'static> = Domain {
    name: "zeno-fcis/authorization-policy",
    version: 2,
};

/// Fixed domain `zeno-fcis/authorized-bundle`, version 2.
pub const AUTHORIZED_BUNDLE: Domain<'static> = Domain {
    name: "zeno-fcis/authorized-bundle",
    version: 2,
};

/// Fixed domain `zeno-fcis/authorized-genesis`, version 2.
pub const AUTHORIZED_GENESIS: Domain<'static> = Domain {
    name: "zeno-fcis/authorized-genesis",
    version: 2,
};

/// Fixed domain `zeno-fcis/authorized-reject`, version 2.
pub const AUTHORIZED_REJECT: Domain<'static> = Domain {
    name: "zeno-fcis/authorized-reject",
    version: 2,
};

/// Fixed domain `zeno-fcis/authorized-transition`, version 2.
pub const AUTHORIZED_TRANSITION: Domain<'static> = Domain {
    name: "zeno-fcis/authorized-transition",
    version: 2,
};

/// Fixed domain `zeno-fcis/backend-certificate`, version 1.
pub const BACKEND_CERTIFICATE: Domain<'static> = Domain {
    name: "zeno-fcis/backend-certificate",
    version: 1,
};

/// Fixed domain `zeno-fcis/backend-identity`, version 1.
pub const BACKEND_IDENTITY: Domain<'static> = Domain {
    name: "zeno-fcis/backend-identity",
    version: 1,
};

/// Fixed domain `zeno-fcis/backend-request`, version 1.
pub const BACKEND_REQUEST: Domain<'static> = Domain {
    name: "zeno-fcis/backend-request",
    version: 1,
};

/// Fixed domain `zeno-fcis/backend-response`, version 1.
pub const BACKEND_RESPONSE: Domain<'static> = Domain {
    name: "zeno-fcis/backend-response",
    version: 1,
};

/// Fixed domain `zeno-fcis/backend-synthesis-checker`, version 1.
pub const BACKEND_SYNTHESIS_CHECKER: Domain<'static> = Domain {
    name: "zeno-fcis/backend-synthesis-checker",
    version: 1,
};

/// Fixed domain `zeno-fcis/backend-synthesis-composition`, version 1.
pub const BACKEND_SYNTHESIS_COMPOSITION: Domain<'static> = Domain {
    name: "zeno-fcis/backend-synthesis-composition",
    version: 1,
};

/// Fixed domain `zeno-fcis/backend-synthesis-reference`, version 1.
pub const BACKEND_SYNTHESIS_REFERENCE: Domain<'static> = Domain {
    name: "zeno-fcis/backend-synthesis-reference",
    version: 1,
};

/// Fixed domain `zeno-fcis/bench`, version 1.
pub const BENCH: Domain<'static> = Domain {
    name: "zeno-fcis/bench",
    version: 1,
};

/// Fixed domain `zeno-fcis/bootstrap-file`, version 1.
pub const BOOTSTRAP_FILE: Domain<'static> = Domain {
    name: "zeno-fcis/bootstrap-file",
    version: 1,
};

/// Fixed domain `zeno-fcis/bootstrap-manifest`, version 1.
pub const BOOTSTRAP_MANIFEST: Domain<'static> = Domain {
    name: "zeno-fcis/bootstrap-manifest",
    version: 1,
};

/// Fixed domain `zeno-fcis/candidate`, version 1.
pub const CANDIDATE: Domain<'static> = Domain {
    name: "zeno-fcis/candidate",
    version: 1,
};

/// Fixed domain `zeno-fcis/catalog-manifest`, version 3.
pub const CATALOG_MANIFEST: Domain<'static> = Domain {
    name: "zeno-fcis/catalog-manifest",
    version: 3,
};

/// Fixed domain `zeno-fcis/channel-definition`, version 3.
pub const CHANNEL_DEFINITION: Domain<'static> = Domain {
    name: "zeno-fcis/channel-definition",
    version: 3,
};

/// Fixed domain `zeno-fcis/channel-registry`, version 3.
pub const CHANNEL_REGISTRY: Domain<'static> = Domain {
    name: "zeno-fcis/channel-registry",
    version: 3,
};

/// Fixed domain `zeno-fcis/commit-plan`, version 1.
pub const COMMIT_PLAN: Domain<'static> = Domain {
    name: "zeno-fcis/commit-plan",
    version: 1,
};

/// Fixed domain `zeno-fcis/complete-footprint-claim`, version 1.
pub const COMPLETE_FOOTPRINT_CLAIM: Domain<'static> = Domain {
    name: "zeno-fcis/complete-footprint-claim",
    version: 1,
};

/// Fixed domain `zeno-fcis/complete-footprint-evidence`, version 1.
pub const COMPLETE_FOOTPRINT_EVIDENCE: Domain<'static> = Domain {
    name: "zeno-fcis/complete-footprint-evidence",
    version: 1,
};

/// Fixed domain `zeno-fcis/complete-footprint-witness`, version 1.
pub const COMPLETE_FOOTPRINT_WITNESS: Domain<'static> = Domain {
    name: "zeno-fcis/complete-footprint-witness",
    version: 1,
};

/// Fixed domain `zeno-fcis/complete-invocation-context`, version 1.
pub const COMPLETE_INVOCATION_CONTEXT: Domain<'static> = Domain {
    name: "zeno-fcis/complete-invocation-context",
    version: 1,
};

/// Fixed domain `zeno-fcis/completion-checker-source`, version 1.
pub const COMPLETION_CHECKER_SOURCE: Domain<'static> = Domain {
    name: "zeno-fcis/completion-checker-source",
    version: 1,
};

/// Fixed domain `zeno-fcis/completion-ir-source`, version 1.
pub const COMPLETION_IR_SOURCE: Domain<'static> = Domain {
    name: "zeno-fcis/completion-ir-source",
    version: 1,
};

/// Fixed domain `zeno-fcis/completion-problem`, version 1.
pub const COMPLETION_PROBLEM: Domain<'static> = Domain {
    name: "zeno-fcis/completion-problem",
    version: 1,
};

/// Fixed domain `zeno-fcis/completion-space-source`, version 1.
pub const COMPLETION_SPACE_SOURCE: Domain<'static> = Domain {
    name: "zeno-fcis/completion-space-source",
    version: 1,
};

/// Fixed domain `zeno-fcis/composed-domain-program`, version 1.
pub const COMPOSED_DOMAIN_PROGRAM: Domain<'static> = Domain {
    name: "zeno-fcis/composed-domain-program",
    version: 1,
};

/// Fixed domain `zeno-fcis/composition-claim`, version 2.
pub const COMPOSITION_CLAIM: Domain<'static> = Domain {
    name: "zeno-fcis/composition-claim",
    version: 2,
};

/// Fixed domain `zeno-fcis/composition-spec`, version 2.
pub const COMPOSITION_SPEC: Domain<'static> = Domain {
    name: "zeno-fcis/composition-spec",
    version: 2,
};

/// Fixed domain `zeno-fcis/delivery`, version 1.
pub const DELIVERY: Domain<'static> = Domain {
    name: "zeno-fcis/delivery",
    version: 1,
};

/// Fixed domain `zeno-fcis/delivery-interpreter`, version 2.
pub const DELIVERY_INTERPRETER: Domain<'static> = Domain {
    name: "zeno-fcis/delivery-interpreter",
    version: 2,
};

/// Fixed domain `zeno-fcis/determinism-probe`, version 1.
pub const DETERMINISM_PROBE: Domain<'static> = Domain {
    name: "zeno-fcis/determinism-probe",
    version: 1,
};

/// Fixed domain `zeno-fcis/deterministic-parallel-authorization`, version 1.
pub const DETERMINISTIC_PARALLEL_AUTHORIZATION: Domain<'static> = Domain {
    name: "zeno-fcis/deterministic-parallel-authorization",
    version: 1,
};

/// Fixed domain `zeno-fcis/effect-definition`, version 3.
pub const EFFECT_DEFINITION: Domain<'static> = Domain {
    name: "zeno-fcis/effect-definition",
    version: 3,
};

/// Fixed domain `zeno-fcis/effect-registry`, version 3.
pub const EFFECT_REGISTRY: Domain<'static> = Domain {
    name: "zeno-fcis/effect-registry",
    version: 3,
};

/// Fixed domain `zeno-fcis/executable-composition`, version 1.
pub const EXECUTABLE_COMPOSITION: Domain<'static> = Domain {
    name: "zeno-fcis/executable-composition",
    version: 1,
};

/// Fixed domain `zeno-fcis/exhaustive-coverage-claim`, version 1.
pub const EXHAUSTIVE_COVERAGE_CLAIM: Domain<'static> = Domain {
    name: "zeno-fcis/exhaustive-coverage-claim",
    version: 1,
};

/// Fixed domain `zeno-fcis/exhaustive-domain`, version 1.
pub const EXHAUSTIVE_DOMAIN: Domain<'static> = Domain {
    name: "zeno-fcis/exhaustive-domain",
    version: 1,
};

/// Fixed domain `zeno-fcis/exhaustive-footprint-domain`, version 1.
pub const EXHAUSTIVE_FOOTPRINT_DOMAIN: Domain<'static> = Domain {
    name: "zeno-fcis/exhaustive-footprint-domain",
    version: 1,
};

/// Fixed domain `zeno-fcis/finite-admission-source`, version 1.
pub const FINITE_ADMISSION_SOURCE: Domain<'static> = Domain {
    name: "zeno-fcis/finite-admission-source",
    version: 1,
};

/// Fixed domain `zeno-fcis/finite-admission-specification`, version 1.
pub const FINITE_ADMISSION_SPECIFICATION: Domain<'static> = Domain {
    name: "zeno-fcis/finite-admission-specification",
    version: 1,
};

/// Fixed domain `zeno-fcis/finite-algorithm`, version 1.
pub const FINITE_ALGORITHM: Domain<'static> = Domain {
    name: "zeno-fcis/finite-algorithm",
    version: 1,
};

/// Fixed domain `zeno-fcis/finite-checker`, version 1.
pub const FINITE_CHECKER: Domain<'static> = Domain {
    name: "zeno-fcis/finite-checker",
    version: 1,
};

/// Fixed domain `zeno-fcis/finite-checker-source`, version 1.
pub const FINITE_CHECKER_SOURCE: Domain<'static> = Domain {
    name: "zeno-fcis/finite-checker-source",
    version: 1,
};

/// Fixed domain `zeno-fcis/finite-closed-totality`, version 1.
pub const FINITE_CLOSED_TOTALITY: Domain<'static> = Domain {
    name: "zeno-fcis/finite-closed-totality",
    version: 1,
};

/// Fixed domain `zeno-fcis/finite-contract`, version 1.
pub const FINITE_CONTRACT: Domain<'static> = Domain {
    name: "zeno-fcis/finite-contract",
    version: 1,
};

/// Fixed domain `zeno-fcis/finite-contract-coverage`, version 1.
pub const FINITE_CONTRACT_COVERAGE: Domain<'static> = Domain {
    name: "zeno-fcis/finite-contract-coverage",
    version: 1,
};

/// Fixed domain `zeno-fcis/finite-decision-contract`, version 1.
pub const FINITE_DECISION_CONTRACT: Domain<'static> = Domain {
    name: "zeno-fcis/finite-decision-contract",
    version: 1,
};

/// Fixed domain `zeno-fcis/finite-decision-evaluator`, version 2.
pub const FINITE_DECISION_EVALUATOR: Domain<'static> = Domain {
    name: "zeno-fcis/finite-decision-evaluator",
    version: 2,
};

/// Fixed domain `zeno-fcis/finite-execution-source`, version 1.
pub const FINITE_EXECUTION_SOURCE: Domain<'static> = Domain {
    name: "zeno-fcis/finite-execution-source",
    version: 1,
};

/// Fixed domain `zeno-fcis/finite-execution-specification`, version 1.
pub const FINITE_EXECUTION_SPECIFICATION: Domain<'static> = Domain {
    name: "zeno-fcis/finite-execution-specification",
    version: 1,
};

/// Fixed domain `zeno-fcis/finite-grammar`, version 1.
pub const FINITE_GRAMMAR: Domain<'static> = Domain {
    name: "zeno-fcis/finite-grammar",
    version: 1,
};

/// Fixed domain `zeno-fcis/finite-ir-source`, version 1.
pub const FINITE_IR_SOURCE: Domain<'static> = Domain {
    name: "zeno-fcis/finite-ir-source",
    version: 1,
};

/// Fixed domain `zeno-fcis/finite-runtime-source`, version 1.
pub const FINITE_RUNTIME_SOURCE: Domain<'static> = Domain {
    name: "zeno-fcis/finite-runtime-source",
    version: 1,
};

/// Fixed domain `zeno-fcis/finite-schema`, version 1.
pub const FINITE_SCHEMA: Domain<'static> = Domain {
    name: "zeno-fcis/finite-schema",
    version: 1,
};

/// Fixed domain `zeno-fcis/finite-v2-execution-source`, version 1.
pub const FINITE_V2_EXECUTION_SOURCE: Domain<'static> = Domain {
    name: "zeno-fcis/finite-v2-execution-source",
    version: 1,
};

/// Fixed domain `zeno-fcis/finite-v2-execution-specification`, version 1.
pub const FINITE_V2_EXECUTION_SPECIFICATION: Domain<'static> = Domain {
    name: "zeno-fcis/finite-v2-execution-specification",
    version: 1,
};

/// Fixed domain `zeno-fcis/finite-v2-input-source`, version 1.
pub const FINITE_V2_INPUT_SOURCE: Domain<'static> = Domain {
    name: "zeno-fcis/finite-v2-input-source",
    version: 1,
};

/// Fixed domain `zeno-fcis/finite-v2-input-specification`, version 1.
pub const FINITE_V2_INPUT_SPECIFICATION: Domain<'static> = Domain {
    name: "zeno-fcis/finite-v2-input-specification",
    version: 1,
};

/// Fixed domain `zeno-fcis/finite-v2-integer-source`, version 1.
pub const FINITE_V2_INTEGER_SOURCE: Domain<'static> = Domain {
    name: "zeno-fcis/finite-v2-integer-source",
    version: 1,
};

/// Fixed domain `zeno-fcis/finite-v2-integer-specification`, version 1.
pub const FINITE_V2_INTEGER_SPECIFICATION: Domain<'static> = Domain {
    name: "zeno-fcis/finite-v2-integer-specification",
    version: 1,
};

/// Fixed domain `zeno-fcis/finite-v2-meter-source`, version 1.
pub const FINITE_V2_METER_SOURCE: Domain<'static> = Domain {
    name: "zeno-fcis/finite-v2-meter-source",
    version: 1,
};

/// Fixed domain `zeno-fcis/finite-v2-record-execution-source`, version 1.
pub const FINITE_V2_RECORD_EXECUTION_SOURCE: Domain<'static> = Domain {
    name: "zeno-fcis/finite-v2-record-execution-source",
    version: 1,
};

/// Fixed domain `zeno-fcis/finite-v2-record-execution-specification`, version 1.
pub const FINITE_V2_RECORD_EXECUTION_SPECIFICATION: Domain<'static> = Domain {
    name: "zeno-fcis/finite-v2-record-execution-specification",
    version: 1,
};

/// Fixed domain `zeno-fcis/fold-input`, version 1.
pub const FOLD_INPUT: Domain<'static> = Domain {
    name: "zeno-fcis/fold-input",
    version: 1,
};

/// Fixed domain `zeno-fcis/formal-run`, version 3.
pub const FORMAL_RUN: Domain<'static> = Domain {
    name: "zeno-fcis/formal-run",
    version: 3,
};

/// Fixed domain `zeno-fcis/formal-source`, version 1.
pub const FORMAL_SOURCE: Domain<'static> = Domain {
    name: "zeno-fcis/formal-source",
    version: 1,
};

/// Fixed domain `zeno-fcis/generated-file`, version 1.
pub const GENERATED_FILE: Domain<'static> = Domain {
    name: "zeno-fcis/generated-file",
    version: 1,
};

/// Fixed domain `zeno-fcis/generation-manifest`, version 1.
pub const GENERATION_MANIFEST: Domain<'static> = Domain {
    name: "zeno-fcis/generation-manifest",
    version: 1,
};

/// Fixed domain `zeno-fcis/genesis-law-evaluation`, version 1.
pub const GENESIS_LAW_EVALUATION: Domain<'static> = Domain {
    name: "zeno-fcis/genesis-law-evaluation",
    version: 1,
};

/// Fixed domain `zeno-fcis/genesis-law-input`, version 1.
pub const GENESIS_LAW_INPUT: Domain<'static> = Domain {
    name: "zeno-fcis/genesis-law-input",
    version: 1,
};

/// Fixed domain `zeno-fcis/genesis-policy-binding`, version 2.
pub const GENESIS_POLICY_BINDING: Domain<'static> = Domain {
    name: "zeno-fcis/genesis-policy-binding",
    version: 2,
};

/// Fixed domain `zeno-fcis/law-assumptions`, version 1.
pub const LAW_ASSUMPTIONS: Domain<'static> = Domain {
    name: "zeno-fcis/law-assumptions",
    version: 1,
};

/// Fixed domain `zeno-fcis/law-coverage`, version 1.
pub const LAW_COVERAGE: Domain<'static> = Domain {
    name: "zeno-fcis/law-coverage",
    version: 1,
};

/// Fixed domain `zeno-fcis/law-definition`, version 1.
pub const LAW_DEFINITION: Domain<'static> = Domain {
    name: "zeno-fcis/law-definition",
    version: 1,
};

/// Fixed domain `zeno-fcis/law-evaluation`, version 1.
pub const LAW_EVALUATION: Domain<'static> = Domain {
    name: "zeno-fcis/law-evaluation",
    version: 1,
};

/// Fixed domain `zeno-fcis/law-input`, version 1.
pub const LAW_INPUT: Domain<'static> = Domain {
    name: "zeno-fcis/law-input",
    version: 1,
};

/// Fixed domain `zeno-fcis/law-manifest`, version 1.
pub const LAW_MANIFEST: Domain<'static> = Domain {
    name: "zeno-fcis/law-manifest",
    version: 1,
};

/// Fixed domain `zeno-fcis/law-proof-subject`, version 1.
pub const LAW_PROOF_SUBJECT: Domain<'static> = Domain {
    name: "zeno-fcis/law-proof-subject",
    version: 1,
};

/// Fixed domain `zeno-fcis/law-query-id`, version 1.
pub const LAW_QUERY_ID: Domain<'static> = Domain {
    name: "zeno-fcis/law-query-id",
    version: 1,
};

/// Fixed domain `zeno-fcis/mounted-counterexample`, version 1.
pub const MOUNTED_COUNTEREXAMPLE: Domain<'static> = Domain {
    name: "zeno-fcis/mounted-counterexample",
    version: 1,
};

/// Fixed domain `zeno-fcis/mounted-input`, version 1.
pub const MOUNTED_INPUT: Domain<'static> = Domain {
    name: "zeno-fcis/mounted-input",
    version: 1,
};

/// Fixed domain `zeno-fcis/normalized-decision`, version 1.
pub const NORMALIZED_DECISION: Domain<'static> = Domain {
    name: "zeno-fcis/normalized-decision",
    version: 1,
};

/// Fixed domain `zeno-fcis/observed-footprint`, version 1.
pub const OBSERVED_FOOTPRINT: Domain<'static> = Domain {
    name: "zeno-fcis/observed-footprint",
    version: 1,
};

/// Fixed domain `zeno-fcis/outbox-entry`, version 1.
pub const OUTBOX_ENTRY: Domain<'static> = Domain {
    name: "zeno-fcis/outbox-entry",
    version: 1,
};

/// Fixed domain `zeno-fcis/outbox-plan`, version 1.
pub const OUTBOX_PLAN: Domain<'static> = Domain {
    name: "zeno-fcis/outbox-plan",
    version: 1,
};

/// Fixed domain `zeno-fcis/parity`, version 1.
pub const PARITY: Domain<'static> = Domain {
    name: "zeno-fcis/parity",
    version: 1,
};

/// Fixed domain `zeno-fcis/patch`, version 1.
pub const PATCH: Domain<'static> = Domain {
    name: "zeno-fcis/patch",
    version: 1,
};

/// Fixed domain `zeno-fcis/prepared-fold`, version 1.
pub const PREPARED_FOLD: Domain<'static> = Domain {
    name: "zeno-fcis/prepared-fold",
    version: 1,
};

/// Fixed domain `zeno-fcis/profile-evolution`, version 1.
pub const PROFILE_EVOLUTION: Domain<'static> = Domain {
    name: "zeno-fcis/profile-evolution",
    version: 1,
};

/// Fixed domain `zeno-fcis/project-catalog`, version 3.
pub const PROJECT_CATALOG: Domain<'static> = Domain {
    name: "zeno-fcis/project-catalog",
    version: 3,
};

/// Fixed domain `zeno-fcis/project-profile`, version 1.
pub const PROJECT_PROFILE: Domain<'static> = Domain {
    name: "zeno-fcis/project-profile",
    version: 1,
};

/// Fixed domain `zeno-fcis/project-spec`, version 1.
pub const PROJECT_SPEC: Domain<'static> = Domain {
    name: "zeno-fcis/project-spec",
    version: 1,
};

/// Fixed domain `zeno-fcis/projection-relation-subject`, version 1.
pub const PROJECTION_RELATION_SUBJECT: Domain<'static> = Domain {
    name: "zeno-fcis/projection-relation-subject",
    version: 1,
};

/// Fixed domain `zeno-fcis/projector-evidence`, version 1.
pub const PROJECTOR_EVIDENCE: Domain<'static> = Domain {
    name: "zeno-fcis/projector-evidence",
    version: 1,
};

/// Fixed domain `zeno-fcis/projector-qualification`, version 1.
pub const PROJECTOR_QUALIFICATION: Domain<'static> = Domain {
    name: "zeno-fcis/projector-qualification",
    version: 1,
};

/// Fixed domain `zeno-fcis/promotion-evidence`, version 1.
pub const PROMOTION_EVIDENCE: Domain<'static> = Domain {
    name: "zeno-fcis/promotion-evidence",
    version: 1,
};

/// Fixed domain `zeno-fcis/promotion-policy`, version 1.
pub const PROMOTION_POLICY: Domain<'static> = Domain {
    name: "zeno-fcis/promotion-policy",
    version: 1,
};

/// Fixed domain `zeno-fcis/promotion-report`, version 1.
pub const PROMOTION_REPORT: Domain<'static> = Domain {
    name: "zeno-fcis/promotion-report",
    version: 1,
};

/// Fixed domain `zeno-fcis/reason-definition`, version 3.
pub const REASON_DEFINITION: Domain<'static> = Domain {
    name: "zeno-fcis/reason-definition",
    version: 3,
};

/// Fixed domain `zeno-fcis/reason-registry`, version 3.
pub const REASON_REGISTRY: Domain<'static> = Domain {
    name: "zeno-fcis/reason-registry",
    version: 3,
};

/// Fixed domain `zeno-fcis/refinement-case`, version 1.
pub const REFINEMENT_CASE: Domain<'static> = Domain {
    name: "zeno-fcis/refinement-case",
    version: 1,
};

/// Fixed domain `zeno-fcis/refinement-input`, version 1.
pub const REFINEMENT_INPUT: Domain<'static> = Domain {
    name: "zeno-fcis/refinement-input",
    version: 1,
};

/// Fixed domain `zeno-fcis/schema`, version 1.
pub const SCHEMA: Domain<'static> = Domain {
    name: "zeno-fcis/schema",
    version: 1,
};

/// Fixed domain `zeno-fcis/secret-exposure-permit`, version 1.
pub const SECRET_EXPOSURE_PERMIT: Domain<'static> = Domain {
    name: "zeno-fcis/secret-exposure-permit",
    version: 1,
};

/// Fixed domain `zeno-fcis/security-capacity-evidence-set`, version 1.
pub const SECURITY_CAPACITY_EVIDENCE_SET: Domain<'static> = Domain {
    name: "zeno-fcis/security-capacity-evidence-set",
    version: 1,
};

/// Fixed domain `zeno-fcis/security-deployment`, version 1.
pub const SECURITY_DEPLOYMENT: Domain<'static> = Domain {
    name: "zeno-fcis/security-deployment",
    version: 1,
};

/// Fixed domain `zeno-fcis/security-evidence-set`, version 1.
pub const SECURITY_EVIDENCE_SET: Domain<'static> = Domain {
    name: "zeno-fcis/security-evidence-set",
    version: 1,
};

/// Fixed domain `zeno-fcis/security-leakage-report-set`, version 1.
pub const SECURITY_LEAKAGE_REPORT_SET: Domain<'static> = Domain {
    name: "zeno-fcis/security-leakage-report-set",
    version: 1,
};

/// Fixed domain `zeno-fcis/security-policy`, version 1.
pub const SECURITY_POLICY: Domain<'static> = Domain {
    name: "zeno-fcis/security-policy",
    version: 1,
};

/// Fixed domain `zeno-fcis/security-promotion-policy`, version 1.
pub const SECURITY_PROMOTION_POLICY: Domain<'static> = Domain {
    name: "zeno-fcis/security-promotion-policy",
    version: 1,
};

/// Fixed domain `zeno-fcis/security-trace`, version 1.
pub const SECURITY_TRACE: Domain<'static> = Domain {
    name: "zeno-fcis/security-trace",
    version: 1,
};

/// Fixed domain `zeno-fcis/semantic-program`, version 1.
pub const SEMANTIC_PROGRAM: Domain<'static> = Domain {
    name: "zeno-fcis/semantic-program",
    version: 1,
};

/// Fixed domain `zeno-fcis/synthesis-assignment`, version 1.
pub const SYNTHESIS_ASSIGNMENT: Domain<'static> = Domain {
    name: "zeno-fcis/synthesis-assignment",
    version: 1,
};

/// Fixed domain `zeno-fcis/synthesis-certificate`, version 1.
pub const SYNTHESIS_CERTIFICATE: Domain<'static> = Domain {
    name: "zeno-fcis/synthesis-certificate",
    version: 1,
};

/// Fixed domain `zeno-fcis/synthesis-compiled`, version 1.
pub const SYNTHESIS_COMPILED: Domain<'static> = Domain {
    name: "zeno-fcis/synthesis-compiled",
    version: 1,
};

/// Fixed domain `zeno-fcis/synthesis-counterexample`, version 1.
pub const SYNTHESIS_COUNTEREXAMPLE: Domain<'static> = Domain {
    name: "zeno-fcis/synthesis-counterexample",
    version: 1,
};

/// Fixed domain `zeno-fcis/synthesis-problem`, version 1.
pub const SYNTHESIS_PROBLEM: Domain<'static> = Domain {
    name: "zeno-fcis/synthesis-problem",
    version: 1,
};

/// Fixed domain `zeno-fcis/synthesis-trace`, version 1.
pub const SYNTHESIS_TRACE: Domain<'static> = Domain {
    name: "zeno-fcis/synthesis-trace",
    version: 1,
};

/// Fixed domain `zeno-fcis/test`, version 1.
pub const TEST: Domain<'static> = Domain {
    name: "zeno-fcis/test",
    version: 1,
};

/// Fixed domain `zeno-fcis/transition-resources`, version 1.
pub const TRANSITION_RESOURCES: Domain<'static> = Domain {
    name: "zeno-fcis/transition-resources",
    version: 1,
};

/// Fixed domain `zeno-fcis/v2/certificate`, version 1.
pub const V2_CERTIFICATE: Domain<'static> = Domain {
    name: "zeno-fcis/v2/certificate",
    version: 1,
};

/// Fixed domain `zeno-fcis/v2/chain`, version 1.
pub const V2_CHAIN: Domain<'static> = Domain {
    name: "zeno-fcis/v2/chain",
    version: 1,
};

/// Fixed domain `zeno-fcis/v2/checkpoint`, version 1.
pub const V2_CHECKPOINT: Domain<'static> = Domain {
    name: "zeno-fcis/v2/checkpoint",
    version: 1,
};

/// Fixed domain `zeno-fcis/v2/effect-delivery`, version 1.
pub const V2_EFFECT_DELIVERY: Domain<'static> = Domain {
    name: "zeno-fcis/v2/effect-delivery",
    version: 1,
};

/// Fixed domain `zeno-fcis/v2/genesis`, version 1.
pub const V2_GENESIS: Domain<'static> = Domain {
    name: "zeno-fcis/v2/genesis",
    version: 1,
};

/// Fixed domain `zeno-fcis/v2/publication`, version 1.
pub const V2_PUBLICATION: Domain<'static> = Domain {
    name: "zeno-fcis/v2/publication",
    version: 1,
};

/// Fixed domain `zeno-fcis/v2/state`, version 1.
pub const V2_STATE: Domain<'static> = Domain {
    name: "zeno-fcis/v2/state",
    version: 1,
};

/// Fixed domain `zeno-fcis/value`, version 1.
pub const VALUE: Domain<'static> = Domain {
    name: "zeno-fcis/value",
    version: 1,
};

/// Fixed domain `zeno-fcis/vector`, version 1.
pub const VECTOR: Domain<'static> = Domain {
    name: "zeno-fcis/vector",
    version: 1,
};

/// Fixed domain `zeno-fcis/vector-set`, version 1.
pub const VECTOR_SET: Domain<'static> = Domain {
    name: "zeno-fcis/vector-set",
    version: 1,
};

/// Fixed domain `zeno-fcis/verified-law-set`, version 1.
pub const VERIFIED_LAW_SET: Domain<'static> = Domain {
    name: "zeno-fcis/verified-law-set",
    version: 1,
};

/// Fixed domain `zeno-fcis/zenodex/zusd/mount-algorithm`, version 1.
pub const ZENODEX_ZUSD_MOUNT_ALGORITHM: Domain<'static> = Domain {
    name: "zeno-fcis/zenodex/zusd/mount-algorithm",
    version: 1,
};

/// Fixed domain `zeno-fcis/zenodex/zusd/mount-budget`, version 1.
pub const ZENODEX_ZUSD_MOUNT_BUDGET: Domain<'static> = Domain {
    name: "zeno-fcis/zenodex/zusd/mount-budget",
    version: 1,
};

/// Fixed domain `zeno-fcis/zenodex/zusd/mount-case`, version 1.
pub const ZENODEX_ZUSD_MOUNT_CASE: Domain<'static> = Domain {
    name: "zeno-fcis/zenodex/zusd/mount-case",
    version: 1,
};

/// Fixed domain `zeno-fcis/zenodex/zusd/mount-context`, version 1.
pub const ZENODEX_ZUSD_MOUNT_CONTEXT: Domain<'static> = Domain {
    name: "zeno-fcis/zenodex/zusd/mount-context",
    version: 1,
};

/// Fixed domain `zeno-fcis/zenodex/zusd/mount-schema`, version 1.
pub const ZENODEX_ZUSD_MOUNT_SCHEMA: Domain<'static> = Domain {
    name: "zeno-fcis/zenodex/zusd/mount-schema",
    version: 1,
};

/// Complete registry, sorted by exact domain name.
pub const ALL: &[Domain<'static>] = &[
    ACCESS_MAP_KEY,
    AUTH_EMPTY,
    AUTH_LEAF,
    AUTH_NODE,
    AUTH_PATCH,
    AUTHENTICATED_AUTHORITY_CONFIG,
    AUTHENTICATED_AUTHORIZATION,
    AUTHORIZATION_INVOCATION,
    AUTHORIZATION_POLICY,
    AUTHORIZED_BUNDLE,
    AUTHORIZED_GENESIS,
    AUTHORIZED_REJECT,
    AUTHORIZED_TRANSITION,
    BACKEND_CERTIFICATE,
    BACKEND_IDENTITY,
    BACKEND_REQUEST,
    BACKEND_RESPONSE,
    BACKEND_SYNTHESIS_CHECKER,
    BACKEND_SYNTHESIS_COMPOSITION,
    BACKEND_SYNTHESIS_REFERENCE,
    BENCH,
    BOOTSTRAP_FILE,
    BOOTSTRAP_MANIFEST,
    CANDIDATE,
    CATALOG_MANIFEST,
    CHANNEL_DEFINITION,
    CHANNEL_REGISTRY,
    COMMIT_PLAN,
    COMPLETE_FOOTPRINT_CLAIM,
    COMPLETE_FOOTPRINT_EVIDENCE,
    COMPLETE_FOOTPRINT_WITNESS,
    COMPLETE_INVOCATION_CONTEXT,
    COMPLETION_CHECKER_SOURCE,
    COMPLETION_IR_SOURCE,
    COMPLETION_PROBLEM,
    COMPLETION_SPACE_SOURCE,
    COMPOSED_DOMAIN_PROGRAM,
    COMPOSITION_CLAIM,
    COMPOSITION_SPEC,
    DELIVERY,
    DELIVERY_INTERPRETER,
    DETERMINISM_PROBE,
    DETERMINISTIC_PARALLEL_AUTHORIZATION,
    EFFECT_DEFINITION,
    EFFECT_REGISTRY,
    EXECUTABLE_COMPOSITION,
    EXHAUSTIVE_COVERAGE_CLAIM,
    EXHAUSTIVE_DOMAIN,
    EXHAUSTIVE_FOOTPRINT_DOMAIN,
    FINITE_ADMISSION_SOURCE,
    FINITE_ADMISSION_SPECIFICATION,
    FINITE_ALGORITHM,
    FINITE_CHECKER,
    FINITE_CHECKER_SOURCE,
    FINITE_CLOSED_TOTALITY,
    FINITE_CONTRACT,
    FINITE_CONTRACT_COVERAGE,
    FINITE_DECISION_CONTRACT,
    FINITE_DECISION_EVALUATOR,
    FINITE_EXECUTION_SOURCE,
    FINITE_EXECUTION_SPECIFICATION,
    FINITE_GRAMMAR,
    FINITE_IR_SOURCE,
    FINITE_RUNTIME_SOURCE,
    FINITE_SCHEMA,
    FINITE_V2_EXECUTION_SOURCE,
    FINITE_V2_EXECUTION_SPECIFICATION,
    FINITE_V2_INPUT_SOURCE,
    FINITE_V2_INPUT_SPECIFICATION,
    FINITE_V2_INTEGER_SOURCE,
    FINITE_V2_INTEGER_SPECIFICATION,
    FINITE_V2_METER_SOURCE,
    FINITE_V2_RECORD_EXECUTION_SOURCE,
    FINITE_V2_RECORD_EXECUTION_SPECIFICATION,
    FOLD_INPUT,
    FORMAL_RUN,
    FORMAL_SOURCE,
    GENERATED_FILE,
    GENERATION_MANIFEST,
    GENESIS_LAW_EVALUATION,
    GENESIS_LAW_INPUT,
    GENESIS_POLICY_BINDING,
    LAW_ASSUMPTIONS,
    LAW_COVERAGE,
    LAW_DEFINITION,
    LAW_EVALUATION,
    LAW_INPUT,
    LAW_MANIFEST,
    LAW_PROOF_SUBJECT,
    LAW_QUERY_ID,
    MOUNTED_COUNTEREXAMPLE,
    MOUNTED_INPUT,
    NORMALIZED_DECISION,
    OBSERVED_FOOTPRINT,
    OUTBOX_ENTRY,
    OUTBOX_PLAN,
    PARITY,
    PATCH,
    PREPARED_FOLD,
    PROFILE_EVOLUTION,
    PROJECT_CATALOG,
    PROJECT_PROFILE,
    PROJECT_SPEC,
    PROJECTION_RELATION_SUBJECT,
    PROJECTOR_EVIDENCE,
    PROJECTOR_QUALIFICATION,
    PROMOTION_EVIDENCE,
    PROMOTION_POLICY,
    PROMOTION_REPORT,
    REASON_DEFINITION,
    REASON_REGISTRY,
    REFINEMENT_CASE,
    REFINEMENT_INPUT,
    SCHEMA,
    SECRET_EXPOSURE_PERMIT,
    SECURITY_CAPACITY_EVIDENCE_SET,
    SECURITY_DEPLOYMENT,
    SECURITY_EVIDENCE_SET,
    SECURITY_LEAKAGE_REPORT_SET,
    SECURITY_POLICY,
    SECURITY_PROMOTION_POLICY,
    SECURITY_TRACE,
    SEMANTIC_PROGRAM,
    SYNTHESIS_ASSIGNMENT,
    SYNTHESIS_CERTIFICATE,
    SYNTHESIS_COMPILED,
    SYNTHESIS_COUNTEREXAMPLE,
    SYNTHESIS_PROBLEM,
    SYNTHESIS_TRACE,
    TEST,
    TRANSITION_RESOURCES,
    V2_CERTIFICATE,
    V2_CHAIN,
    V2_CHECKPOINT,
    V2_EFFECT_DELIVERY,
    V2_GENESIS,
    V2_PUBLICATION,
    V2_STATE,
    VALUE,
    VECTOR,
    VECTOR_SET,
    VERIFIED_LAW_SET,
    ZENODEX_ZUSD_MOUNT_ALGORITHM,
    ZENODEX_ZUSD_MOUNT_BUDGET,
    ZENODEX_ZUSD_MOUNT_CASE,
    ZENODEX_ZUSD_MOUNT_CONTEXT,
    ZENODEX_ZUSD_MOUNT_SCHEMA,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EncodeError, domain_preimage, is_reserved_domain_name};
    use alloc::vec::Vec;

    // Frozen name/version inventory from the original call sites. Keep these
    // literals independent of the production registry constants.
    const ORIGINAL_AND_V2_NAMES: &[(&str, u16)] = &[
        ("zeno-fcis/access-map-key", 1),
        ("zeno-fcis/auth-empty", 1),
        ("zeno-fcis/auth-leaf", 1),
        ("zeno-fcis/auth-node", 1),
        ("zeno-fcis/auth-patch", 1),
        ("zeno-fcis/authenticated-authority-config", 1),
        ("zeno-fcis/authenticated-authorization", 1),
        ("zeno-fcis/authorization-invocation", 2),
        ("zeno-fcis/authorization-policy", 2),
        ("zeno-fcis/authorized-bundle", 2),
        ("zeno-fcis/authorized-genesis", 2),
        ("zeno-fcis/authorized-reject", 2),
        ("zeno-fcis/authorized-transition", 2),
        ("zeno-fcis/backend-certificate", 1),
        ("zeno-fcis/backend-identity", 1),
        ("zeno-fcis/backend-request", 1),
        ("zeno-fcis/backend-response", 1),
        ("zeno-fcis/backend-synthesis-checker", 1),
        ("zeno-fcis/backend-synthesis-composition", 1),
        ("zeno-fcis/backend-synthesis-reference", 1),
        ("zeno-fcis/bench", 1),
        ("zeno-fcis/bootstrap-file", 1),
        ("zeno-fcis/bootstrap-manifest", 1),
        ("zeno-fcis/candidate", 1),
        ("zeno-fcis/catalog-manifest", 3),
        ("zeno-fcis/channel-definition", 3),
        ("zeno-fcis/channel-registry", 3),
        ("zeno-fcis/commit-plan", 1),
        ("zeno-fcis/complete-footprint-claim", 1),
        ("zeno-fcis/complete-footprint-evidence", 1),
        ("zeno-fcis/complete-footprint-witness", 1),
        ("zeno-fcis/complete-invocation-context", 1),
        ("zeno-fcis/completion-checker-source", 1),
        ("zeno-fcis/completion-ir-source", 1),
        ("zeno-fcis/completion-problem", 1),
        ("zeno-fcis/completion-space-source", 1),
        ("zeno-fcis/composed-domain-program", 1),
        ("zeno-fcis/composition-claim", 2),
        ("zeno-fcis/composition-spec", 2),
        ("zeno-fcis/delivery", 1),
        ("zeno-fcis/delivery-interpreter", 2),
        ("zeno-fcis/determinism-probe", 1),
        ("zeno-fcis/deterministic-parallel-authorization", 1),
        ("zeno-fcis/effect-definition", 3),
        ("zeno-fcis/effect-registry", 3),
        ("zeno-fcis/executable-composition", 1),
        ("zeno-fcis/exhaustive-coverage-claim", 1),
        ("zeno-fcis/exhaustive-domain", 1),
        ("zeno-fcis/exhaustive-footprint-domain", 1),
        ("zeno-fcis/finite-admission-source", 1),
        ("zeno-fcis/finite-admission-specification", 1),
        ("zeno-fcis/finite-algorithm", 1),
        ("zeno-fcis/finite-checker", 1),
        ("zeno-fcis/finite-checker-source", 1),
        ("zeno-fcis/finite-closed-totality", 1),
        ("zeno-fcis/finite-contract", 1),
        ("zeno-fcis/finite-contract-coverage", 1),
        ("zeno-fcis/finite-decision-contract", 1),
        ("zeno-fcis/finite-decision-evaluator", 2),
        ("zeno-fcis/finite-execution-source", 1),
        ("zeno-fcis/finite-execution-specification", 1),
        ("zeno-fcis/finite-grammar", 1),
        ("zeno-fcis/finite-ir-source", 1),
        ("zeno-fcis/finite-runtime-source", 1),
        ("zeno-fcis/finite-schema", 1),
        ("zeno-fcis/finite-v2-execution-source", 1),
        ("zeno-fcis/finite-v2-execution-specification", 1),
        ("zeno-fcis/finite-v2-input-source", 1),
        ("zeno-fcis/finite-v2-input-specification", 1),
        ("zeno-fcis/finite-v2-integer-source", 1),
        ("zeno-fcis/finite-v2-integer-specification", 1),
        ("zeno-fcis/finite-v2-meter-source", 1),
        ("zeno-fcis/finite-v2-record-execution-source", 1),
        ("zeno-fcis/finite-v2-record-execution-specification", 1),
        ("zeno-fcis/fold-input", 1),
        ("zeno-fcis/formal-run", 3),
        ("zeno-fcis/formal-source", 1),
        ("zeno-fcis/generated-file", 1),
        ("zeno-fcis/generation-manifest", 1),
        ("zeno-fcis/genesis-law-evaluation", 1),
        ("zeno-fcis/genesis-law-input", 1),
        ("zeno-fcis/genesis-policy-binding", 2),
        ("zeno-fcis/law-assumptions", 1),
        ("zeno-fcis/law-coverage", 1),
        ("zeno-fcis/law-definition", 1),
        ("zeno-fcis/law-evaluation", 1),
        ("zeno-fcis/law-input", 1),
        ("zeno-fcis/law-manifest", 1),
        ("zeno-fcis/law-proof-subject", 1),
        ("zeno-fcis/law-query-id", 1),
        ("zeno-fcis/mounted-counterexample", 1),
        ("zeno-fcis/mounted-input", 1),
        ("zeno-fcis/normalized-decision", 1),
        ("zeno-fcis/observed-footprint", 1),
        ("zeno-fcis/outbox-entry", 1),
        ("zeno-fcis/outbox-plan", 1),
        ("zeno-fcis/parity", 1),
        ("zeno-fcis/patch", 1),
        ("zeno-fcis/prepared-fold", 1),
        ("zeno-fcis/profile-evolution", 1),
        ("zeno-fcis/project-catalog", 3),
        ("zeno-fcis/project-profile", 1),
        ("zeno-fcis/project-spec", 1),
        ("zeno-fcis/projection-relation-subject", 1),
        ("zeno-fcis/projector-evidence", 1),
        ("zeno-fcis/projector-qualification", 1),
        ("zeno-fcis/promotion-evidence", 1),
        ("zeno-fcis/promotion-policy", 1),
        ("zeno-fcis/promotion-report", 1),
        ("zeno-fcis/reason-definition", 3),
        ("zeno-fcis/reason-registry", 3),
        ("zeno-fcis/refinement-case", 1),
        ("zeno-fcis/refinement-input", 1),
        ("zeno-fcis/schema", 1),
        ("zeno-fcis/secret-exposure-permit", 1),
        ("zeno-fcis/security-capacity-evidence-set", 1),
        ("zeno-fcis/security-deployment", 1),
        ("zeno-fcis/security-evidence-set", 1),
        ("zeno-fcis/security-leakage-report-set", 1),
        ("zeno-fcis/security-policy", 1),
        ("zeno-fcis/security-promotion-policy", 1),
        ("zeno-fcis/security-trace", 1),
        ("zeno-fcis/semantic-program", 1),
        ("zeno-fcis/synthesis-assignment", 1),
        ("zeno-fcis/synthesis-certificate", 1),
        ("zeno-fcis/synthesis-compiled", 1),
        ("zeno-fcis/synthesis-counterexample", 1),
        ("zeno-fcis/synthesis-problem", 1),
        ("zeno-fcis/synthesis-trace", 1),
        ("zeno-fcis/test", 1),
        ("zeno-fcis/transition-resources", 1),
        ("zeno-fcis/v2/certificate", 1),
        ("zeno-fcis/v2/chain", 1),
        ("zeno-fcis/v2/checkpoint", 1),
        ("zeno-fcis/v2/effect-delivery", 1),
        ("zeno-fcis/v2/genesis", 1),
        ("zeno-fcis/v2/publication", 1),
        ("zeno-fcis/v2/state", 1),
        ("zeno-fcis/value", 1),
        ("zeno-fcis/vector", 1),
        ("zeno-fcis/vector-set", 1),
        ("zeno-fcis/verified-law-set", 1),
        ("zeno-fcis/zenodex/zusd/mount-algorithm", 1),
        ("zeno-fcis/zenodex/zusd/mount-budget", 1),
        ("zeno-fcis/zenodex/zusd/mount-case", 1),
        ("zeno-fcis/zenodex/zusd/mount-context", 1),
        ("zeno-fcis/zenodex/zusd/mount-schema", 1),
    ];

    #[test]
    fn closed_registry_preserves_every_exact_domain_preimage() {
        assert_eq!(ALL.len(), 147);
        assert_eq!(ALL.len(), ORIGINAL_AND_V2_NAMES.len());
        for (domain, (name, version)) in ALL.iter().zip(ORIGINAL_AND_V2_NAMES) {
            assert_eq!((domain.name(), domain.version()), (*name, *version));
            let payload = b"\0exact retained payload\xff";
            let mut expected = Vec::from(&b"ZENOFCIS-HASH\0"[..]);
            expected.extend_from_slice(&version.to_be_bytes());
            let name_length =
                u16::try_from(name.len()).unwrap_or_else(|error| panic!("domain length: {error}"));
            expected.extend_from_slice(&name_length.to_be_bytes());
            expected.extend_from_slice(name.as_bytes());
            expected.extend_from_slice(&24_u64.to_be_bytes());
            expected.extend_from_slice(payload);
            assert_eq!(domain_preimage(*domain, payload), Ok(expected), "{name}");
            assert!(is_reserved_domain_name(name));
            assert_eq!(Domain::new(name, *version), Err(EncodeError::InvalidDomain));
        }
        for pair in ALL.windows(2) {
            assert!(pair[0].name() < pair[1].name());
        }
    }

    #[test]
    fn ordinary_constructor_rejects_the_entire_reserved_namespace() {
        for reserved in ["zeno-fcis", "zeno-fcis/", "zeno-fcis/provider-new-tag"] {
            for version in [0, 1, u16::MAX] {
                assert_eq!(
                    Domain::new(reserved, version),
                    Err(EncodeError::InvalidDomain)
                );
            }
        }
        for allowed in [
            "zeno-fcisx",
            "zeno-fcis-app/state",
            "app/zeno-fcis/state",
            "ZENO-FCIS/state",
        ] {
            for version in [0, 1, u16::MAX] {
                let domain = Domain::new(allowed, version)
                    .unwrap_or_else(|error| panic!("allowed name: {error}"));
                assert_eq!((domain.name(), domain.version()), (allowed, version));
            }
        }
    }
}
