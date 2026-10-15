//! The checked class of a specification's claims comes only from the
//! checker's own run on the sources; a generic evidence verifier that
//! accepts everything gives compose's attested report and nothing more.

use serde_json::json;
use zeno_fcis_codec::Hash32;
use zeno_fcis_compose::{
    AssumptionDischarge, ClaimEvidence, CompositionClaim, CompositionEvidence, CompositionSpec,
    EvidenceArtifact, EvidenceVerifier, ProviderGuarantee, verify_assume_guarantee,
};
use zeno_fcis_crypto::RustCryptoSha256;

use super::leaf;
use super::manifest::Manifest;
use super::structure;
use super::tests::{Ring, assert_refused, fail};
use super::{ComposeRefusal, CompositionSources, require_checked};

fn require(
    ring: &Ring,
    spec: &CompositionSpec,
) -> Result<super::CheckedAssumeGuarantee, ComposeRefusal> {
    let manifest = ring.manifest_text();
    require_checked(
        spec,
        CompositionSources {
            manifest: &manifest,
            leaves: &ring.leaves,
            aggregate: None,
        },
    )
}

/// The specification a ring's manifest denotes, built without checking it.
fn denoted(ring: &Ring) -> CompositionSpec {
    let manifest = Manifest::read(&ring.manifest_text()).unwrap_or_else(fail);
    let compiled = manifest
        .leaves
        .iter()
        .zip(&ring.leaves)
        .map(|(entry, files)| leaf::compile(entry, &files.project, &files.rules))
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(fail);
    structure::specification(&manifest, &compiled).unwrap_or_else(fail)
}

/// Accepts every artifact.
struct Permissive;

impl EvidenceVerifier for Permissive {
    fn verify(&self, _: &CompositionClaim, _: &[u8]) -> bool {
        true
    }
}

/// Evidence for every guarantee and assumption of `spec`, with artifacts
/// that are no evidence at all.
fn forged_evidence(spec: &CompositionSpec) -> CompositionEvidence {
    let artifact = || EvidenceArtifact::new::<RustCryptoSha256>(b"trust me".to_vec());
    let mut claims = Vec::new();
    let mut discharges = Vec::new();
    for component in spec.components() {
        for guarantee in component.guarantees() {
            claims.push(ClaimEvidence::new(guarantee.claim(), artifact()));
        }
        let peer = spec
            .components()
            .iter()
            .find(|other| other.id() != component.id())
            .unwrap_or_else(|| fail("a peer"));
        for assumption in component.assumptions() {
            let provider = ProviderGuarantee::try_new(peer.id(), peer.guarantees()[0].claim())
                .unwrap_or_else(fail);
            discharges.push(
                AssumptionDischarge::try_new(
                    component.id(),
                    assumption.claim(),
                    vec![provider],
                    artifact(),
                )
                .unwrap_or_else(fail),
            );
        }
    }
    CompositionEvidence::try_new(claims, discharges, None).unwrap_or_else(fail)
}

#[test]
fn the_checked_class_covers_exactly_the_denoted_specification() {
    let ring = Ring::new(2);
    let checked = ring.check().unwrap_or_else(fail);
    let spec = checked.composition_spec().clone();
    assert_eq!(spec, denoted(&ring));
    let class = require(&ring, &spec).unwrap_or_else(fail);
    let hash = *spec
        .commitment::<RustCryptoSha256>()
        .unwrap_or_else(fail)
        .as_bytes();
    assert_eq!(class.spec_hash(), hash);
    assert_eq!(class.manifest_commitment(), checked.manifest_commitment());
    assert_eq!(
        class.checker_source_digest(),
        checked.checker_source_digest()
    );
    // Three guarantees and one discharge for each leaf.
    assert_eq!(class.claims().len(), 8);
    let spec_hash = Hash32::new(hash);
    for component in spec.components() {
        for guarantee in component.guarantees() {
            assert!(class.covers(&CompositionClaim::Guarantee {
                spec_hash,
                component: component.id(),
                claim: guarantee.claim(),
            }));
            // The same claim under another specification is not covered.
            assert!(!class.covers(&CompositionClaim::Guarantee {
                spec_hash: Hash32::new([7; 32]),
                component: component.id(),
                claim: guarantee.claim(),
            }));
        }
    }
    assert_eq!(
        checked.json()["composition_spec"],
        json!({"version": 1, "commitment": super::hex(&hash), "claims": 8})
    );
}

#[test]
fn another_specification_is_not_accepted_as_checked() {
    let ring = Ring::new(2);
    let other = Ring::new(4)
        .check()
        .unwrap_or_else(fail)
        .composition_spec()
        .clone();
    match require(&ring, &other) {
        Err(ComposeRefusal::Different { expected, actual }) => assert_ne!(expected, actual),
        other => panic!("expected a different specification, got {other:?}"),
    }
}

#[test]
fn a_permissive_verifier_attests_what_the_checker_refuses() {
    // The weaker provider fact fails the interface implication: compose,
    // shown the same specification and an accept-everything verifier,
    // still reports it verified.
    let mut ring = Ring::new(2);
    ring.manifest["leaves"][1]["guarantees"][0]["formula"] = json!("post.100.110 <= 4");
    let spec = denoted(&ring);
    let report =
        verify_assume_guarantee::<RustCryptoSha256, _>(&spec, &forged_evidence(&spec), &Permissive);
    assert!(report.is_verified(), "{:?}", report.blockers());
    match require(&ring, &spec) {
        Err(ComposeRefusal::Check(refusal)) => {
            assert_refused(
                &refusal,
                "interface",
                "provider facts => consumer assumption",
            );
        }
        other => panic!("expected the check's refusal, got {other:?}"),
    }
    // A sound ring with forged evidence is attested by compose as well; the
    // checked class still comes only from the checker's run.
    let ring = Ring::new(2);
    let spec = denoted(&ring);
    let report =
        verify_assume_guarantee::<RustCryptoSha256, _>(&spec, &forged_evidence(&spec), &Permissive);
    assert!(report.is_verified());
    assert!(require(&ring, &spec).is_ok());
}
