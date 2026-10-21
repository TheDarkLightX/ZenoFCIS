//! Test-only scalar fixtures, independently checked by the retained py_ecc rows.
use blst::min_pk::SecretKey;
use zeno_fcis_bls::{POP_DST, POP_PROOF_DST};
fn secret(s: u64) -> SecretKey {
    let mut b = [0; 32];
    b[24..].copy_from_slice(&s.to_be_bytes());
    SecretKey::from_bytes(&b).unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
}
pub(crate) fn key(s: u64) -> ([u8; 48], [u8; 96]) {
    let sk = secret(s);
    let pk = sk.sk_to_pk().to_bytes();
    (pk, sk.sign(&pk, POP_PROOF_DST, &[]).to_bytes())
}
pub(crate) fn sign(s: u64, m: &[u8]) -> [u8; 96] {
    secret(s).sign(m, POP_DST, &[]).to_bytes()
}
