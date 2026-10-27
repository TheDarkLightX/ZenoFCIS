//! TEST ONLY: fixed-key MAC issuer, never linked into a library.
use super::support::hash;
use zenofcis_zusd_v1::application::*;
pub struct TestVerifier;
pub fn mac(message: &[u8]) -> Vec<u8> {
    let key = [0x42u8; 32];
    let mut inner = vec![0x36; 64];
    let mut outer = vec![0x5c; 64];
    for i in 0..32 {
        inner[i] ^= key[i];
        outer[i] ^= key[i]
    }
    inner.extend_from_slice(message);
    outer.extend_from_slice(&hash(&inner));
    hash(&outer).to_vec()
}
impl ReceiptVerifierV1 for TestVerifier {
    fn verify(
        &self,
        _d: Domain,
        _a: [u8; 32],
        message: &[u8],
        proof: &[u8],
    ) -> Result<(), VerificationFailure> {
        let expected = mac(message);
        let different = if proof.len() != expected.len() {
            true
        } else {
            proof.iter().zip(expected).fold(0, |v, (a, b)| v | (a ^ b)) != 0
        };
        if different {
            Err(VerificationFailure::Invalid)
        } else {
            Ok(())
        }
    }
}

pub fn issue(message: &[u8]) -> Vec<u8> {
    mac(message)
}
