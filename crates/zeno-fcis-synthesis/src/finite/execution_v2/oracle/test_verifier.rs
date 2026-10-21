//! Deterministic lifecycle test double, never compiled into the library.
use super::*;
use zeno_fcis_codec::CommitmentHasher;
use zeno_fcis_crypto::RustCryptoSha256;
pub(super) struct TestVerifier;
pub(super) fn key(sk: u64) -> ([u8; 48], [u8; 96]) {
    let mut key = [0; 48];
    key[..8].copy_from_slice(&sk.to_be_bytes());
    key[0] |= 0x80;
    (key, sign_key(&key, POSSESSION_DOMAIN, &key))
}
fn sign_key(key: &[u8], domain: &[u8], message: &[u8]) -> [u8; 96] {
    let hash = RustCryptoSha256::hash_parts(&[domain, key, message]);
    let mut sig = [0; 96];
    for chunk in sig.chunks_mut(32) {
        chunk.copy_from_slice(hash.as_bytes());
    }
    sig[0] = (sig[0] & 0x3f) | 0x80;
    sig
}
pub(super) fn sign(sk: u64, message: &[u8]) -> [u8; 96] {
    sign_key(&key(sk).0, SIGNATURE_DOMAIN, message)
}
impl BlsVerifier for TestVerifier {
    fn canonical_key(&self, k: &[u8]) -> bool {
        k.len() == 48 && k[0] & 0xc0 == 0x80
    }
    fn canonical_signature(&self, s: &[u8]) -> bool {
        s.len() == 96 && s[0] & 0xc0 == 0x80
    }
    fn possession(&self, k: &[u8], p: &[u8]) -> bool {
        self.canonical_key(k) && p == sign_key(k, POSSESSION_DOMAIN, k)
    }
    fn signature(&self, k: &[u8], m: &[u8], s: &[u8]) -> bool {
        self.canonical_key(k) && s == sign_key(k, SIGNATURE_DOMAIN, m)
    }
}
