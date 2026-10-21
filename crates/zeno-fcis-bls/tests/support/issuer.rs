//! TEST ONLY signing. These fixed public test keys must never be deployed.
use super::support::id;
use blst::min_pk::{AggregateSignature, SecretKey};
use std::sync::OnceLock;
use zeno_fcis_bls::{receipt::*, *};
use zenofcis_zusd_v1::application::*;
fn sk(index: usize) -> SecretKey {
    SecretKey::key_gen(&[0x71 + index as u8; 32], &[])
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
}
fn authority(domain: Domain) -> [u8; 32] {
    id(match domain {
        Domain::Actor => 61,
        Domain::Clock => 62,
        Domain::Oracle => 63,
        Domain::Custody => 64,
        Domain::Allocation => 66,
        Domain::Drip => 68,
        Domain::Funding => 65,
        Domain::ParameterPolicy => 72,
        Domain::ExposureRelease => 67,
    })
}
pub(crate) fn backend() -> &'static BlsReceiptVerifier {
    static BACKEND: OnceLock<BlsReceiptVerifier> = OnceLock::new();
    BACKEND.get_or_init(|| {
        let members = (0..3)
            .map(|i| {
                let sk = sk(i);
                let pk = sk.sk_to_pk().to_bytes();
                PopKey::admit(&pk, &sk.sign(&pk, POP_PROOF_DST, &[]).to_bytes())
                    .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
            })
            .collect::<Vec<_>>();
        let authorities = [
            Domain::Actor,
            Domain::Clock,
            Domain::Oracle,
            Domain::Custody,
            Domain::Allocation,
            Domain::Drip,
            Domain::Funding,
            Domain::ParameterPolicy,
            Domain::ExposureRelease,
        ]
        .into_iter()
        .map(|d| {
            if d == Domain::Oracle {
                Authority::quorum(d, authority(d), members.clone(), 2)
                    .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
            } else {
                Authority::basic(d, authority(d), sk(3).sk_to_pk().to_bytes())
                    .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
            }
        })
        .collect();
        BlsReceiptVerifier::new(authorities, 131_072)
            .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
    })
}
pub(crate) struct TestVerifier;
impl ReceiptVerifierV1 for TestVerifier {
    fn verify(
        &self,
        d: Domain,
        a: [u8; 32],
        m: &[u8],
        p: &[u8],
    ) -> Result<(), VerificationFailure> {
        backend().verify(d, a, m, p)
    }
}
// The shared journey substitutes only test issuance.
pub(crate) fn issue(message: &[u8]) -> Vec<u8> {
    let domain = message[b"zusd.ReceiptVerifierV1/1\0".len()];
    let mut proof;
    let sig;
    if domain == Domain::Oracle as u8 {
        proof = vec![1, 1, 0, 2, 0, 0, 0, 1];
        let sigs = (0..2)
            .map(|i| sk(i).sign(message, POP_DST, &[]))
            .collect::<Vec<_>>();
        sig = AggregateSignature::aggregate(&sigs.iter().collect::<Vec<_>>(), true)
            .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
            .to_signature()
            .to_bytes();
    } else {
        proof = vec![1, 0, 0, 1, 0, 0];
        sig = sk(3).sign(message, BASIC_DST, &[]).to_bytes();
    }
    proof.extend_from_slice(&sig);
    proof
}
