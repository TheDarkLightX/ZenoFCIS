//! Adversarial laws for complete static footprint authorization.

use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher, Hash32};
use zeno_fcis_compose::{
    AccessPath, ComponentContract, ComponentId, CompositionClaim, CompositionEvidence,
    CompositionSpec, ContractError, DecisionClassCoverage, DecisionCoverageStatus,
    EvidenceVerifier, ExhaustiveFootprintDomain, Footprint, FootprintAuthorityBinding,
    FootprintCompletenessClaim, FootprintCompletenessEvidence, FootprintEvidenceVerifier,
    FootprintProofKind, FootprintProofMethod, FootprintWitnessError,
    MAX_EXHAUSTIVE_FOOTPRINT_INPUTS, ParallelAuthorizationError, ParallelParityEvidence,
    ParallelVerificationContext, PathAtom, PathSet, authorize_deterministic_parallel,
    verify_complete_footprint,
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

#[derive(Clone, Copy)]
struct ExactFootprintVerifier {
    identity: Hash32,
    accept: bool,
}

impl FootprintEvidenceVerifier for ExactFootprintVerifier {
    fn verifier_hash(&self) -> Hash32 {
        self.identity
    }

    fn verify(&self, claim: &FootprintCompletenessClaim, artifact: Hash32) -> bool {
        self.accept && claim.commitment::<TestHasher>().ok() == Some(artifact)
    }
}

struct ExactCompositionVerifier;

impl EvidenceVerifier for ExactCompositionVerifier {
    fn verify(&self, claim: &CompositionClaim, artifact: Hash32) -> bool {
        claim.commitment::<TestHasher>().ok() == Some(artifact)
    }
}

fn hash(byte: u8) -> Hash32 {
    Hash32::new([byte; 32])
}

fn indexed_hash(index: usize) -> Hash32 {
    let mut bytes = [0_u8; 32];
    let ordinal = u64::try_from(index)
        .unwrap_or_else(|error| panic!("indexed hash conversion: {error}"))
        .saturating_add(1);
    bytes[..8].copy_from_slice(&ordinal.to_be_bytes());
    Hash32::new(bytes)
}

fn path(namespace: u32, atoms: Vec<PathAtom>) -> AccessPath {
    AccessPath::try_new(namespace, atoms).unwrap_or_else(|error| panic!("path: {error}"))
}

fn set(paths: Vec<AccessPath>) -> PathSet {
    PathSet::try_new(paths).unwrap_or_else(|error| panic!("set: {error}"))
}

fn empty_footprint() -> Footprint {
    Footprint::default()
}

#[allow(clippy::too_many_arguments)]
fn binding(
    component: u32,
    profile: u8,
    program: u8,
    footprint: Footprint,
    outbox: PathSet,
    schema: u8,
    catalog: u8,
    algorithm: u8,
    source: u8,
) -> FootprintAuthorityBinding {
    FootprintAuthorityBinding::try_new(
        ComponentId::new(component),
        hash(profile),
        hash(program),
        footprint,
        outbox,
        hash(schema),
        hash(catalog),
        hash(algorithm),
        hash(source),
        hash(43),
        hash(44),
    )
    .unwrap_or_else(|error| panic!("binding: {error}"))
}

fn base_binding(component: u32, profile: u8) -> FootprintAuthorityBinding {
    binding(
        component,
        profile,
        20 + component as u8,
        empty_footprint(),
        PathSet::empty(),
        30,
        31,
        32,
        33,
    )
}

fn claim(binding: FootprintAuthorityBinding) -> FootprintCompletenessClaim {
    let method = FootprintProofMethod::theorem(hash(40), hash(41))
        .unwrap_or_else(|error| panic!("method: {error}"));
    FootprintCompletenessClaim::try_new(
        binding,
        method,
        DecisionClassCoverage::new(
            DecisionCoverageStatus::Covered,
            DecisionCoverageStatus::Covered,
            DecisionCoverageStatus::Covered,
        ),
        hash(42),
    )
    .unwrap_or_else(|error| panic!("claim: {error}"))
}

fn exact_footprint_verifier() -> ExactFootprintVerifier {
    ExactFootprintVerifier {
        identity: hash(44),
        accept: true,
    }
}

fn footprint_evidence(binding: FootprintAuthorityBinding) -> FootprintCompletenessEvidence {
    let claim = claim(binding);
    let artifact = claim
        .commitment::<TestHasher>()
        .unwrap_or_else(|error| panic!("claim commitment: {error}"));
    FootprintCompletenessEvidence::try_new(claim, artifact, hash(44))
        .unwrap_or_else(|error| panic!("evidence: {error}"))
}

#[test]
fn exhaustive_domain_is_canonical_unique_and_nonempty() {
    let left = ExhaustiveFootprintDomain::try_new(hash(1), hash(2), vec![hash(5), hash(4)])
        .unwrap_or_else(|error| panic!("left domain: {error}"));
    let right = ExhaustiveFootprintDomain::try_new(hash(1), hash(2), vec![hash(4), hash(5)])
        .unwrap_or_else(|error| panic!("right domain: {error}"));

    assert_eq!(left, right);
    assert_eq!(left.input_hashes(), &[hash(4), hash(5)]);
    assert_eq!(
        left.canonical_bytes()
            .unwrap_or_else(|error| panic!("left bytes: {error}")),
        right
            .canonical_bytes()
            .unwrap_or_else(|error| panic!("right bytes: {error}"))
    );
    assert_eq!(
        ExhaustiveFootprintDomain::try_new(hash(1), hash(2), Vec::new()),
        Err(ContractError::ExhaustiveDomainCardinality)
    );
    assert_eq!(
        ExhaustiveFootprintDomain::try_new(hash(1), hash(2), vec![hash(4), hash(4)]),
        Err(ContractError::DuplicateExhaustiveInput)
    );
}

#[test]
fn exhaustive_domain_enforces_the_exact_cardinality_boundary() {
    let exact_inputs = (0..MAX_EXHAUSTIVE_FOOTPRINT_INPUTS)
        .map(indexed_hash)
        .collect();
    let exact = ExhaustiveFootprintDomain::try_new(hash(1), hash(2), exact_inputs)
        .unwrap_or_else(|error| panic!("exact boundary domain: {error}"));
    assert_eq!(exact.input_hashes().len(), MAX_EXHAUSTIVE_FOOTPRINT_INPUTS);

    let oversized_inputs = (0..=MAX_EXHAUSTIVE_FOOTPRINT_INPUTS)
        .map(indexed_hash)
        .collect();
    assert_eq!(
        ExhaustiveFootprintDomain::try_new(hash(1), hash(2), oversized_inputs),
        Err(ContractError::ExhaustiveDomainCardinality)
    );
}

#[test]
fn exhaustive_proof_method_requires_an_exact_domain_manifest() {
    let domain = ExhaustiveFootprintDomain::try_new(hash(1), hash(2), vec![hash(3)])
        .unwrap_or_else(|error| panic!("domain: {error}"));
    let method = FootprintProofMethod::exhaustive_finite_domain(hash(4), hash(5), domain.clone())
        .unwrap_or_else(|error| panic!("method: {error}"));

    assert_eq!(method.kind(), FootprintProofKind::ExhaustiveFiniteDomain);
    assert_eq!(method.exhaustive_domain(), Some(&domain));
    assert_eq!(
        FootprintProofMethod::static_analysis(Hash32::ZERO, hash(5)),
        Err(ContractError::ZeroHash)
    );
}

#[test]
fn declared_footprints_reject_rare_effect_outbox_and_map_key_substitutions() {
    let common_effect = path(10, vec![PathAtom::Field(1)]);
    let rare_effect = path(10, vec![PathAtom::Field(2)]);
    let declared = Footprint::new(
        PathSet::empty(),
        PathSet::empty(),
        PathSet::empty(),
        set(vec![common_effect]),
    );
    let observed_rare_effect = Footprint::new(
        PathSet::empty(),
        PathSet::empty(),
        PathSet::empty(),
        set(vec![rare_effect]),
    );
    assert!(!declared.covers_observed(&observed_rare_effect));

    let declared_outbox = set(vec![path(11, vec![PathAtom::MapKey(hash(8))])]);
    let committed_failure_outbox = set(vec![path(11, vec![PathAtom::MapKey(hash(9))])]);
    assert!(!declared_outbox.covers_all(&committed_failure_outbox));
}

#[test]
fn every_authority_identity_substitution_invalidates_witness_minting() {
    let expected = base_binding(1, 10);
    let changed_footprint = Footprint::new(
        PathSet::empty(),
        PathSet::empty(),
        PathSet::empty(),
        set(vec![path(12, vec![PathAtom::Field(1)])]),
    );
    let changed_outbox = set(vec![path(13, vec![PathAtom::MapKey(hash(90))])]);
    let changed_toolchain = FootprintAuthorityBinding::try_new(
        ComponentId::new(1),
        hash(10),
        hash(21),
        empty_footprint(),
        PathSet::empty(),
        hash(30),
        hash(31),
        hash(32),
        hash(33),
        hash(91),
        hash(44),
    )
    .unwrap_or_else(|error| panic!("toolchain binding: {error}"));
    let changed_verifier = FootprintAuthorityBinding::try_new(
        ComponentId::new(1),
        hash(10),
        hash(21),
        empty_footprint(),
        PathSet::empty(),
        hash(30),
        hash(31),
        hash(32),
        hash(33),
        hash(43),
        hash(92),
    )
    .unwrap_or_else(|error| panic!("verifier binding: {error}"));
    let substitutions = vec![
        binding(
            2,
            10,
            21,
            empty_footprint(),
            PathSet::empty(),
            30,
            31,
            32,
            33,
        ),
        binding(
            1,
            11,
            21,
            empty_footprint(),
            PathSet::empty(),
            30,
            31,
            32,
            33,
        ),
        binding(
            1,
            10,
            22,
            empty_footprint(),
            PathSet::empty(),
            30,
            31,
            32,
            33,
        ),
        binding(
            1,
            10,
            21,
            changed_footprint,
            PathSet::empty(),
            30,
            31,
            32,
            33,
        ),
        binding(1, 10, 21, empty_footprint(), changed_outbox, 30, 31, 32, 33),
        binding(
            1,
            10,
            21,
            empty_footprint(),
            PathSet::empty(),
            34,
            31,
            32,
            33,
        ),
        binding(
            1,
            10,
            21,
            empty_footprint(),
            PathSet::empty(),
            30,
            35,
            32,
            33,
        ),
        binding(
            1,
            10,
            21,
            empty_footprint(),
            PathSet::empty(),
            30,
            31,
            36,
            33,
        ),
        binding(
            1,
            10,
            21,
            empty_footprint(),
            PathSet::empty(),
            30,
            31,
            32,
            37,
        ),
        changed_toolchain,
        changed_verifier,
    ];
    let verifier = ExactFootprintVerifier {
        identity: hash(44),
        accept: true,
    };

    for substitution in substitutions {
        let claim = claim(substitution);
        let artifact = claim
            .commitment::<TestHasher>()
            .unwrap_or_else(|error| panic!("claim commitment: {error}"));
        let evidence = FootprintCompletenessEvidence::try_new(claim, artifact, verifier.identity)
            .unwrap_or_else(|error| panic!("evidence: {error}"));
        assert_eq!(
            verify_complete_footprint(&expected, evidence, &verifier),
            Err(FootprintWitnessError::AuthorityBindingMismatch)
        );
    }
}

#[test]
fn witness_requires_exact_verifier_identity_and_acceptance() {
    let expected = base_binding(1, 10);
    let claim = claim(expected.clone());
    let artifact = claim
        .commitment::<TestHasher>()
        .unwrap_or_else(|error| panic!("claim commitment: {error}"));
    let exact = ExactFootprintVerifier {
        identity: hash(44),
        accept: true,
    };
    let wrong_identity = FootprintCompletenessEvidence::try_new(claim.clone(), artifact, hash(45))
        .unwrap_or_else(|error| panic!("evidence: {error}"));
    assert_eq!(
        verify_complete_footprint(&expected, wrong_identity, &exact),
        Err(FootprintWitnessError::VerifierIdentityMismatch)
    );

    let wrong_artifact =
        FootprintCompletenessEvidence::try_new(claim.clone(), hash(99), exact.identity)
            .unwrap_or_else(|error| panic!("evidence: {error}"));
    assert_eq!(
        verify_complete_footprint(&expected, wrong_artifact, &exact),
        Err(FootprintWitnessError::UnverifiedEvidence)
    );

    let rejecting = ExactFootprintVerifier {
        identity: hash(44),
        accept: false,
    };
    let rejected = FootprintCompletenessEvidence::try_new(claim, artifact, rejecting.identity)
        .unwrap_or_else(|error| panic!("evidence: {error}"));
    assert_eq!(
        verify_complete_footprint(&expected, rejected, &rejecting),
        Err(FootprintWitnessError::UnverifiedEvidence)
    );
}

#[test]
fn decision_class_coverage_is_complete_and_identity_bearing() {
    let covered = DecisionClassCoverage::new(
        DecisionCoverageStatus::Covered,
        DecisionCoverageStatus::Covered,
        DecisionCoverageStatus::Covered,
    );
    let unreachable_committed_failure = DecisionClassCoverage::new(
        DecisionCoverageStatus::Covered,
        DecisionCoverageStatus::Covered,
        DecisionCoverageStatus::ProvedUnreachable,
    );
    let method = FootprintProofMethod::theorem(hash(40), hash(41))
        .unwrap_or_else(|error| panic!("method: {error}"));
    let first =
        FootprintCompletenessClaim::try_new(base_binding(1, 10), method.clone(), covered, hash(42))
            .unwrap_or_else(|error| panic!("first claim: {error}"));
    let second = FootprintCompletenessClaim::try_new(
        base_binding(1, 10),
        method,
        unreachable_committed_failure,
        hash(42),
    )
    .unwrap_or_else(|error| panic!("second claim: {error}"));

    assert_ne!(
        first
            .commitment::<TestHasher>()
            .unwrap_or_else(|error| panic!("first commitment: {error}")),
        second
            .commitment::<TestHasher>()
            .unwrap_or_else(|error| panic!("second commitment: {error}"))
    );
}

fn component(id: u32, profile: u8) -> ComponentContract {
    ComponentContract::try_new_with_outbox(
        ComponentId::new(id),
        hash(profile),
        empty_footprint(),
        PathSet::empty(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .unwrap_or_else(|error| panic!("component: {error}"))
}

fn parallel_fixture() -> (
    CompositionSpec,
    CompositionEvidence,
    ParallelVerificationContext,
    Vec<FootprintAuthorityBinding>,
    Vec<FootprintCompletenessEvidence>,
) {
    let first_id = ComponentId::new(1);
    let second_id = ComponentId::new(2);
    let spec = CompositionSpec::try_new(
        2,
        vec![component(2, 12), component(1, 11)],
        Vec::new(),
        Vec::new(),
        vec![first_id, second_id],
    )
    .unwrap_or_else(|error| panic!("spec: {error}"));
    let spec_hash = spec
        .commitment::<TestHasher>()
        .unwrap_or_else(|error| panic!("spec commitment: {error}"));
    let context = ParallelVerificationContext::try_new(
        spec_hash,
        hash(51),
        hash(52),
        hash(53),
        hash(54),
        hash(55),
        hash(56),
        vec![first_id, second_id],
    )
    .unwrap_or_else(|error| panic!("context: {error}"));
    let result = hash(57);
    let parity_claim = CompositionClaim::ParallelParity {
        context: Box::new(context.clone()),
        sequential_result: result,
        composed_result: result,
    };
    let parity_artifact = parity_claim
        .commitment::<TestHasher>()
        .unwrap_or_else(|error| panic!("parity artifact: {error}"));
    let parity = ParallelParityEvidence::try_new(context.clone(), result, result, parity_artifact)
        .unwrap_or_else(|error| panic!("parity: {error}"));
    let evidence = CompositionEvidence::try_new(Vec::new(), Vec::new(), Some(parity))
        .unwrap_or_else(|error| panic!("composition evidence: {error}"));
    let first = base_binding(1, 11);
    let second = base_binding(2, 12);
    let footprint_items = vec![
        footprint_evidence(second.clone()),
        footprint_evidence(first.clone()),
    ];
    (
        spec,
        evidence,
        context,
        vec![second, first],
        footprint_items,
    )
}

#[test]
fn deterministic_parallel_authorization_requires_exact_complete_evidence_set() {
    let (spec, evidence, context, bindings, footprint_items) = parallel_fixture();
    let authorization = authorize_deterministic_parallel::<TestHasher, _, _>(
        &spec,
        &evidence,
        &context,
        &bindings,
        footprint_items,
        &ExactCompositionVerifier,
        &exact_footprint_verifier(),
    )
    .unwrap_or_else(|error| panic!("authorization: {error}"));

    assert_eq!(authorization.footprint_witnesses().len(), 2);
    assert_eq!(
        authorization.footprint_witnesses()[0].binding().component(),
        ComponentId::new(1)
    );
    assert_eq!(authorization.spec_hash(), context.composition_spec_hash());
}

#[test]
fn deterministic_parallel_authorization_obeys_selected_footprint_verifier() {
    let (spec, evidence, context, bindings, footprint_items) = parallel_fixture();
    let rejecting = ExactFootprintVerifier {
        identity: hash(44),
        accept: false,
    };

    assert_eq!(
        authorize_deterministic_parallel::<TestHasher, _, _>(
            &spec,
            &evidence,
            &context,
            &bindings,
            footprint_items,
            &ExactCompositionVerifier,
            &rejecting,
        ),
        Err(ParallelAuthorizationError::FootprintEvidence {
            component: ComponentId::new(1),
            index: 0,
            error: FootprintWitnessError::UnverifiedEvidence,
        })
    );
}

#[test]
fn deterministic_parallel_authorization_rejects_missing_duplicate_and_stale_evidence() {
    let (spec, evidence, context, bindings, footprint_items) = parallel_fixture();
    assert_eq!(
        authorize_deterministic_parallel::<TestHasher, _, _>(
            &spec,
            &evidence,
            &context,
            &bindings,
            vec![footprint_items[0].clone()],
            &ExactCompositionVerifier,
            &exact_footprint_verifier(),
        ),
        Err(ParallelAuthorizationError::FootprintEvidenceSetCardinality)
    );
    assert_eq!(
        authorize_deterministic_parallel::<TestHasher, _, _>(
            &spec,
            &evidence,
            &context,
            &bindings,
            vec![footprint_items[0].clone(), footprint_items[0].clone(),],
            &ExactCompositionVerifier,
            &exact_footprint_verifier(),
        ),
        Err(ParallelAuthorizationError::DuplicateFootprintEvidence)
    );

    let stale_first = binding(
        1,
        11,
        21,
        empty_footprint(),
        PathSet::empty(),
        30,
        31,
        32,
        99,
    );
    assert_eq!(
        authorize_deterministic_parallel::<TestHasher, _, _>(
            &spec,
            &evidence,
            &context,
            &bindings,
            vec![footprint_evidence(stale_first), footprint_items[0].clone(),],
            &ExactCompositionVerifier,
            &exact_footprint_verifier(),
        ),
        Err(
            ParallelAuthorizationError::FootprintEvidenceBindingMismatch {
                component: ComponentId::new(1)
            }
        )
    );
}

#[test]
fn authority_binding_set_cannot_self_validate_a_changed_contract() {
    let (spec, evidence, context, mut bindings, _) = parallel_fixture();
    bindings[1] = base_binding(1, 88);
    let footprint_items = bindings.iter().cloned().map(footprint_evidence).collect();

    assert_eq!(
        authorize_deterministic_parallel::<TestHasher, _, _>(
            &spec,
            &evidence,
            &context,
            &bindings,
            footprint_items,
            &ExactCompositionVerifier,
            &exact_footprint_verifier(),
        ),
        Err(ParallelAuthorizationError::AuthorityBindingMismatch {
            component: ComponentId::new(1)
        })
    );
}
