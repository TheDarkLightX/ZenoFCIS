//! One composition identity per public entry, with unchanged blocker order.

use core::sync::atomic::{AtomicUsize, Ordering};

use zeno_fcis_codec::{CommitmentHasher, Hash32};
use zeno_fcis_compose::{
    AccessPath, Assumption, AssumptionDischarge, ClaimEvidence, ComponentContract, ComponentId,
    CompositionBlocker, CompositionClaim, CompositionEvidence, CompositionSpec,
    DecisionClassCoverage, DecisionCoverageStatus, EvidenceVerifier, Footprint,
    FootprintAuthorityBinding, FootprintCompletenessClaim, FootprintCompletenessEvidence,
    FootprintEvidenceVerifier, FootprintProofMethod, FrameRule, Guarantee,
    ParallelAuthorizationError, ParallelParityEvidence, ParallelVerificationContext, PathAtom,
    PathSet, ProviderGuarantee, authorize_deterministic_parallel, verify_assume_guarantee,
    verify_deterministic_parallel,
};

#[derive(Clone, Copy, Debug)]
struct TestHasher;

impl CommitmentHasher for TestHasher {
    const ALGORITHM_ID: &'static str = "test-only/1";

    fn hash(bytes: &[u8]) -> Hash32 {
        let mut output = [0_u8; 32];
        for (index, byte) in bytes.iter().enumerate() {
            let slot = index % output.len();
            output[slot] = output[slot]
                .wrapping_add(*byte)
                .rotate_left((index % 8) as u32);
        }
        Hash32::new(output)
    }
}

static COMMITMENTS: AtomicUsize = AtomicUsize::new(0);

/// Delegates to [`TestHasher`] and counts every commitment it computes.
///
/// The counter is process-wide, so exactly one test in this file uses this
/// hasher. Tests running in parallel cannot disturb its counts.
#[derive(Clone, Copy, Debug)]
struct CountingHasher;

impl CommitmentHasher for CountingHasher {
    const ALGORITHM_ID: &'static str = "test-only/counting/1";

    fn hash(bytes: &[u8]) -> Hash32 {
        COMMITMENTS.fetch_add(1, Ordering::Relaxed);
        TestHasher::hash(bytes)
    }
}

fn reset_commitments() {
    COMMITMENTS.store(0, Ordering::Relaxed);
}

fn commitments() -> usize {
    COMMITMENTS.load(Ordering::Relaxed)
}

/// Accepts every claim, so the counted hashes are specification commitments.
struct AcceptAllVerifier;

impl EvidenceVerifier for AcceptAllVerifier {
    fn verify(&self, _: &CompositionClaim, _: Hash32) -> bool {
        true
    }
}

struct ExactVerifier;

impl EvidenceVerifier for ExactVerifier {
    fn verify(&self, claim: &CompositionClaim, artifact: Hash32) -> bool {
        claim.commitment::<TestHasher>().ok() == Some(artifact)
    }
}

struct FootprintVerifier {
    accept: bool,
}

impl FootprintEvidenceVerifier for FootprintVerifier {
    fn verifier_hash(&self) -> Hash32 {
        hash(44)
    }

    fn verify(&self, _: &FootprintCompletenessClaim, _: Hash32) -> bool {
        self.accept
    }
}

fn hash(byte: u8) -> Hash32 {
    Hash32::new([byte; 32])
}

fn path(namespace: u32, field: u16) -> AccessPath {
    AccessPath::try_new(namespace, vec![PathAtom::Field(field)])
        .unwrap_or_else(|error| panic!("path: {error}"))
}

fn contract(
    id: u32,
    profile: u8,
    assumptions: Vec<Assumption>,
    guarantees: Vec<Guarantee>,
    frames: Vec<FrameRule>,
) -> ComponentContract {
    ComponentContract::try_new(
        ComponentId::new(id),
        hash(profile),
        Footprint::default(),
        assumptions,
        guarantees,
        frames,
    )
    .unwrap_or_else(|error| panic!("contract: {error}"))
}

fn spec_of(components: Vec<ComponentContract>) -> CompositionSpec {
    let merge_order = {
        let mut order: Vec<ComponentId> = components.iter().map(ComponentContract::id).collect();
        order.sort();
        order
    };
    CompositionSpec::try_new(2, components, Vec::new(), Vec::new(), merge_order)
        .unwrap_or_else(|error| panic!("spec: {error}"))
}

fn context_for(spec: &CompositionSpec, spec_hash: Hash32) -> ParallelVerificationContext {
    ParallelVerificationContext::try_new(
        spec_hash,
        hash(51),
        hash(52),
        hash(53),
        hash(54),
        hash(55),
        hash(56),
        spec.merge_order().to_vec(),
    )
    .unwrap_or_else(|error| panic!("context: {error}"))
}

fn spec_hash(spec: &CompositionSpec) -> Hash32 {
    spec.commitment::<TestHasher>()
        .unwrap_or_else(|error| panic!("spec commitment: {error}"))
}

fn accepted_parity(context: &ParallelVerificationContext) -> ParallelParityEvidence {
    let result = hash(57);
    let claim = CompositionClaim::ParallelParity {
        context: Box::new(context.clone()),
        sequential_result: result,
        composed_result: result,
    };
    let artifact = claim
        .commitment::<TestHasher>()
        .unwrap_or_else(|error| panic!("parity artifact: {error}"));
    ParallelParityEvidence::try_new(context.clone(), result, result, artifact)
        .unwrap_or_else(|error| panic!("parity: {error}"))
}

fn authority_binding(component: u32, profile: u8) -> FootprintAuthorityBinding {
    FootprintAuthorityBinding::try_new(
        ComponentId::new(component),
        hash(profile),
        hash(21),
        Footprint::default(),
        PathSet::empty(),
        hash(30),
        hash(31),
        hash(32),
        hash(33),
        hash(43),
        hash(44),
    )
    .unwrap_or_else(|error| panic!("authority binding: {error}"))
}

fn footprint_evidence(binding: FootprintAuthorityBinding) -> FootprintCompletenessEvidence {
    let method = FootprintProofMethod::theorem(hash(40), hash(41))
        .unwrap_or_else(|error| panic!("proof method: {error}"));
    let claim = FootprintCompletenessClaim::try_new(
        binding,
        method,
        DecisionClassCoverage::new(
            DecisionCoverageStatus::Covered,
            DecisionCoverageStatus::Covered,
            DecisionCoverageStatus::Covered,
        ),
        hash(42),
    )
    .unwrap_or_else(|error| panic!("footprint claim: {error}"));
    let artifact = claim
        .commitment::<TestHasher>()
        .unwrap_or_else(|error| panic!("footprint artifact: {error}"));
    FootprintCompletenessEvidence::try_new(claim, artifact, hash(44))
        .unwrap_or_else(|error| panic!("footprint evidence: {error}"))
}

/// A two-component parallel composition with nothing left to block it.
fn accepted_parallel_case() -> (
    CompositionSpec,
    CompositionEvidence,
    ParallelVerificationContext,
    Vec<FootprintAuthorityBinding>,
    Vec<FootprintCompletenessEvidence>,
) {
    let spec = spec_of(vec![
        contract(1, 11, Vec::new(), Vec::new(), Vec::new()),
        contract(2, 12, Vec::new(), Vec::new(), Vec::new()),
    ]);
    let context = context_for(&spec, spec_hash(&spec));
    let evidence =
        CompositionEvidence::try_new(Vec::new(), Vec::new(), Some(accepted_parity(&context)))
            .unwrap_or_else(|error| panic!("evidence: {error}"));
    let bindings = vec![authority_binding(1, 11), authority_binding(2, 12)];
    let footprint_items = bindings.iter().cloned().map(footprint_evidence).collect();
    (spec, evidence, context, bindings, footprint_items)
}

#[test]
fn each_public_entry_commits_the_specification_once_after_every_footprint_check() {
    // One test owns the process-wide counter so the counts stay isolated.
    let (spec, evidence, context, bindings, footprint_items) = accepted_parallel_case();

    reset_commitments();
    let report = verify_assume_guarantee::<CountingHasher, _>(&spec, &evidence, &AcceptAllVerifier);
    assert!(report.is_verified());
    assert_eq!(commitments(), 1);

    reset_commitments();
    let report = verify_deterministic_parallel::<CountingHasher, _>(
        &spec,
        &evidence,
        &context,
        &AcceptAllVerifier,
    );
    assert!(report.is_verified());
    assert_eq!(commitments(), 1);

    // Every authorization rejection that precedes composition checking must
    // still be decided before the specification is committed at all.
    let mismatched_bindings = vec![authority_binding(1, 88), authority_binding(2, 12)];
    let mismatched_evidence = vec![
        footprint_evidence(authority_binding(1, 88)),
        footprint_evidence(authority_binding(2, 12)),
    ];
    let early_rejections = [
        (bindings[..1].to_vec(), footprint_items.clone(), true),
        (
            vec![bindings[0].clone(), bindings[0].clone()],
            footprint_items.clone(),
            true,
        ),
        (bindings.clone(), footprint_items[..1].to_vec(), true),
        (
            bindings.clone(),
            vec![footprint_items[0].clone(), footprint_items[0].clone()],
            true,
        ),
        (mismatched_bindings, footprint_items.clone(), true),
        (bindings.clone(), mismatched_evidence, true),
        (bindings.clone(), footprint_items.clone(), false),
    ];
    for (candidate_bindings, candidate_evidence, accept_footprints) in early_rejections {
        reset_commitments();
        let outcome = authorize_deterministic_parallel::<CountingHasher, _, _>(
            &spec,
            &evidence,
            &context,
            &candidate_bindings,
            candidate_evidence,
            &AcceptAllVerifier,
            &FootprintVerifier {
                accept: accept_footprints,
            },
        );
        assert!(outcome.is_err());
        assert!(!matches!(
            outcome,
            Err(ParallelAuthorizationError::Composition(_))
        ));
        assert_eq!(commitments(), 0);
    }

    reset_commitments();
    let authorization = authorize_deterministic_parallel::<CountingHasher, _, _>(
        &spec,
        &evidence,
        &context,
        &bindings,
        footprint_items,
        &AcceptAllVerifier,
        &FootprintVerifier { accept: true },
    )
    .unwrap_or_else(|error| panic!("authorization: {error}"));
    assert_eq!(commitments(), 1);
    assert_eq!(authorization.spec_hash(), context.composition_spec_hash());
}

#[test]
fn missing_evidence_blockers_keep_their_component_order() {
    let guarantee = hash(20);
    let frame_claim = hash(21);
    let assumption = hash(22);
    let frame = FrameRule::try_new(path(9, 1), vec![ComponentId::new(2)], frame_claim)
        .unwrap_or_else(|error| panic!("frame: {error}"));
    let spec = spec_of(vec![
        contract(
            1,
            11,
            vec![Assumption::new(assumption, PathSet::empty())],
            vec![Guarantee::new(guarantee, PathSet::empty())],
            vec![frame],
        ),
        contract(2, 12, Vec::new(), Vec::new(), Vec::new()),
    ]);
    let evidence = CompositionEvidence::try_new(Vec::new(), Vec::new(), None)
        .unwrap_or_else(|error| panic!("evidence: {error}"));

    let report = verify_assume_guarantee::<TestHasher, _>(&spec, &evidence, &ExactVerifier);
    assert_eq!(
        report.blockers(),
        [
            CompositionBlocker::MissingGuaranteeEvidence {
                component: ComponentId::new(1),
                claim: guarantee,
            },
            CompositionBlocker::MissingFrameEvidence {
                component: ComponentId::new(1),
                claim: frame_claim,
            },
            CompositionBlocker::MissingAssumptionDischarge {
                component: ComponentId::new(1),
                claim: assumption,
            },
        ]
        .as_slice()
    );
}

#[test]
fn provider_guarantees_are_found_at_the_first_middle_and_last_sorted_claim() {
    let claims = [hash(10), hash(20), hash(30)];
    let assumptions = [hash(60), hash(61), hash(62)];
    let provider = contract(
        1,
        11,
        Vec::new(),
        claims
            .iter()
            .map(|claim| Guarantee::new(*claim, PathSet::empty()))
            .collect(),
        Vec::new(),
    );
    let consumer = contract(
        2,
        12,
        assumptions
            .iter()
            .map(|claim| Assumption::new(*claim, PathSet::empty()))
            .collect(),
        Vec::new(),
        Vec::new(),
    );
    let spec = spec_of(vec![provider, consumer]);
    let identity = spec_hash(&spec);

    let mut claim_evidence: Vec<ClaimEvidence> = claims
        .iter()
        .map(|claim| {
            let statement = CompositionClaim::Guarantee {
                spec_hash: identity,
                component: ComponentId::new(1),
                claim: *claim,
            };
            ClaimEvidence::new(
                *claim,
                statement
                    .commitment::<TestHasher>()
                    .unwrap_or_else(|error| panic!("guarantee artifact: {error}")),
            )
        })
        .collect();
    claim_evidence.sort();

    let discharges = assumptions
        .iter()
        .zip(claims)
        .map(|(assumption, guarantee)| {
            let providers = vec![
                ProviderGuarantee::try_new(ComponentId::new(1), guarantee)
                    .unwrap_or_else(|error| panic!("provider: {error}")),
            ];
            let statement = CompositionClaim::AssumptionDischarge {
                spec_hash: identity,
                component: ComponentId::new(2),
                assumption: *assumption,
                providers: providers.clone().into_boxed_slice(),
            };
            AssumptionDischarge::try_new(
                ComponentId::new(2),
                *assumption,
                providers,
                statement
                    .commitment::<TestHasher>()
                    .unwrap_or_else(|error| panic!("discharge artifact: {error}")),
            )
            .unwrap_or_else(|error| panic!("discharge: {error}"))
        })
        .collect();
    let evidence = CompositionEvidence::try_new(claim_evidence.clone(), discharges, None)
        .unwrap_or_else(|error| panic!("evidence: {error}"));
    assert!(
        verify_assume_guarantee::<TestHasher, _>(&spec, &evidence, &ExactVerifier).is_verified()
    );

    // A claim that would sort between two declared guarantees is still unknown.
    let absent = hash(25);
    let providers = vec![
        ProviderGuarantee::try_new(ComponentId::new(1), absent)
            .unwrap_or_else(|error| panic!("absent provider: {error}")),
    ];
    let statement = CompositionClaim::AssumptionDischarge {
        spec_hash: identity,
        component: ComponentId::new(2),
        assumption: assumptions[2],
        providers: providers.clone().into_boxed_slice(),
    };
    let substituted = AssumptionDischarge::try_new(
        ComponentId::new(2),
        assumptions[2],
        providers,
        statement
            .commitment::<TestHasher>()
            .unwrap_or_else(|error| panic!("substituted artifact: {error}")),
    )
    .unwrap_or_else(|error| panic!("substituted discharge: {error}"));
    let mut discharges: Vec<_> = evidence.assumption_discharges().to_vec();
    discharges[2] = substituted;
    let evidence = CompositionEvidence::try_new(claim_evidence, discharges, None)
        .unwrap_or_else(|error| panic!("substituted evidence: {error}"));

    let report = verify_assume_guarantee::<TestHasher, _>(&spec, &evidence, &ExactVerifier);
    assert_eq!(
        report.blockers(),
        [
            CompositionBlocker::UnknownProviderGuarantee {
                assumption: assumptions[2],
                component: ComponentId::new(1),
                guarantee: absent,
            },
            CompositionBlocker::MissingAssumptionDischarge {
                component: ComponentId::new(2),
                claim: assumptions[2],
            },
        ]
        .as_slice()
    );
}

#[test]
fn parity_blockers_keep_their_order_for_context_result_and_verifier_failures() {
    let spec = spec_of(vec![
        contract(1, 11, Vec::new(), Vec::new(), Vec::new()),
        contract(2, 12, Vec::new(), Vec::new(), Vec::new()),
    ]);
    let expected = context_for(&spec, spec_hash(&spec));

    // A parity record bound to another context, with unequal results and an
    // artifact the verifier rejects.
    let mutated = ParallelVerificationContext::try_new(
        expected.composition_spec_hash(),
        hash(99),
        hash(52),
        hash(53),
        hash(54),
        hash(55),
        hash(56),
        expected.merge_order().to_vec(),
    )
    .unwrap_or_else(|error| panic!("mutated context: {error}"));
    let parity = ParallelParityEvidence::try_new(mutated, hash(57), hash(58), hash(59))
        .unwrap_or_else(|error| panic!("parity: {error}"));
    let evidence = CompositionEvidence::try_new(Vec::new(), Vec::new(), Some(parity))
        .unwrap_or_else(|error| panic!("evidence: {error}"));
    let report =
        verify_deterministic_parallel::<TestHasher, _>(&spec, &evidence, &expected, &ExactVerifier);
    assert_eq!(
        report.blockers(),
        [
            CompositionBlocker::ParallelContextMismatch,
            CompositionBlocker::SequentialParityMismatch,
            CompositionBlocker::UnverifiedSequentialParityEvidence,
        ]
        .as_slice()
    );

    // An expected context bound to another specification blocks on its own.
    let unbound = context_for(&spec, hash(1));
    let evidence =
        CompositionEvidence::try_new(Vec::new(), Vec::new(), Some(accepted_parity(&unbound)))
            .unwrap_or_else(|error| panic!("unbound evidence: {error}"));
    let report =
        verify_deterministic_parallel::<TestHasher, _>(&spec, &evidence, &unbound, &ExactVerifier);
    assert_eq!(
        report.blockers(),
        [CompositionBlocker::ParallelContextMismatch].as_slice()
    );
}

#[test]
fn a_missing_composition_obligation_still_blocks_authorization() {
    let (spec, _, context, bindings, footprint_items) = accepted_parallel_case();
    let evidence = CompositionEvidence::try_new(Vec::new(), Vec::new(), None)
        .unwrap_or_else(|error| panic!("evidence: {error}"));

    let outcome = authorize_deterministic_parallel::<TestHasher, _, _>(
        &spec,
        &evidence,
        &context,
        &bindings,
        footprint_items,
        &ExactVerifier,
        &FootprintVerifier { accept: true },
    );
    let Err(ParallelAuthorizationError::Composition(report)) = outcome else {
        panic!("expected a composition blocker report")
    };
    assert_eq!(
        report.blockers(),
        [CompositionBlocker::MissingSequentialParityEvidence].as_slice()
    );
}
