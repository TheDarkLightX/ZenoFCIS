//! Regression laws for directional composition-frame authorization.

use zeno_fcis_codec::{CommitmentHasher, Hash32};
use zeno_fcis_compose::{
    AccessPath, ClaimEvidence, ComponentContract, ComponentId, CompositionBlocker,
    CompositionClaim, CompositionEvidence, CompositionSpec, EvidenceVerifier, Footprint, FrameRule,
    PathAtom, PathSet, Wiring, verify_assume_guarantee,
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

struct ExactVerifier;

impl EvidenceVerifier for ExactVerifier {
    fn verify(&self, claim: &CompositionClaim, artifact: Hash32) -> bool {
        claim.commitment::<TestHasher>().ok() == Some(artifact)
    }
}

fn hash(byte: u8) -> Hash32 {
    Hash32::new([byte; 32])
}

fn path(namespace: u32, atoms: Vec<PathAtom>) -> AccessPath {
    AccessPath::try_new(namespace, atoms).unwrap_or_else(|error| panic!("path: {error}"))
}

fn component(id: ComponentId, footprint: Footprint, frames: Vec<FrameRule>) -> ComponentContract {
    ComponentContract::try_new(id, hash(60), footprint, vec![], vec![], frames)
        .unwrap_or_else(|error| panic!("component: {error}"))
}

fn source_component(id: ComponentId, effect: AccessPath) -> ComponentContract {
    component(
        id,
        Footprint::new(
            PathSet::empty(),
            PathSet::empty(),
            PathSet::empty(),
            PathSet::try_new(vec![effect])
                .unwrap_or_else(|error| panic!("effect footprint: {error}")),
        ),
        vec![],
    )
}

fn report_for(
    protected: AccessPath,
    destination_path: AccessPath,
    frame_claim: Hash32,
) -> (
    ComponentId,
    ComponentId,
    zeno_fcis_compose::CompositionReport,
) {
    let source_id = ComponentId::new(1);
    let destination_id = ComponentId::new(2);
    let source_effect = path(70, vec![PathAtom::Field(1)]);
    let protected_for_claim = protected.clone();
    let frame = FrameRule::try_new(protected, vec![source_id], frame_claim)
        .unwrap_or_else(|error| panic!("frame: {error}"));
    let spec = CompositionSpec::try_new(
        2,
        vec![
            source_component(source_id, source_effect.clone()),
            component(destination_id, Footprint::default(), vec![frame]),
        ],
        vec![Wiring::new(
            source_id,
            source_effect,
            destination_id,
            destination_path,
            hash(50),
        )],
        vec![],
        vec![source_id, destination_id],
    )
    .unwrap_or_else(|error| panic!("spec: {error}"));
    let spec_hash = spec
        .commitment::<TestHasher>()
        .unwrap_or_else(|error| panic!("spec hash: {error}"));
    let statement = CompositionClaim::Frame {
        spec_hash,
        component: destination_id,
        protected: protected_for_claim,
        claim: frame_claim,
    };
    let artifact = statement
        .commitment::<TestHasher>()
        .unwrap_or_else(|error| panic!("frame evidence: {error}"));
    let evidence = CompositionEvidence::try_new(
        vec![ClaimEvidence::new(frame_claim, artifact)],
        vec![],
        None,
    )
    .unwrap_or_else(|error| panic!("evidence: {error}"));
    let report = verify_assume_guarantee::<TestHasher, _>(&spec, &evidence, &ExactVerifier);
    (source_id, destination_id, report)
}

#[test]
fn narrow_frame_does_not_authorize_ancestor_destination() {
    let protected = path(80, vec![PathAtom::Field(7), PathAtom::Field(2)]);
    let ancestor = path(80, vec![PathAtom::Field(7)]);
    let (source_id, destination_id, report) = report_for(protected, ancestor, hash(40));

    assert!(matches!(
        report.blockers(),
        [CompositionBlocker::UnauthorizedWiring {
            source,
            destination
        }] if *source == source_id && *destination == destination_id
    ));
}

#[test]
fn broad_frame_authorizes_descendant_destination() {
    let protected = path(80, vec![PathAtom::Field(7), PathAtom::AnyDescendant]);
    let descendant = path(80, vec![PathAtom::Field(7), PathAtom::Field(2)]);
    let (_, _, report) = report_for(protected, descendant, hash(41));

    assert!(report.is_verified());
}

#[test]
fn exact_frame_authorizes_exact_destination() {
    let exact = path(80, vec![PathAtom::Field(7), PathAtom::Field(2)]);
    let (_, _, report) = report_for(exact.clone(), exact, hash(42));

    assert!(report.is_verified());
}

#[test]
fn frame_does_not_authorize_sibling_destination() {
    let protected = path(80, vec![PathAtom::Field(7), PathAtom::Field(2)]);
    let sibling = path(80, vec![PathAtom::Field(7), PathAtom::Field(3)]);
    let (source_id, destination_id, report) = report_for(protected, sibling, hash(43));

    assert!(matches!(
        report.blockers(),
        [CompositionBlocker::UnauthorizedWiring {
            source,
            destination
        }] if *source == source_id && *destination == destination_id
    ));
}
