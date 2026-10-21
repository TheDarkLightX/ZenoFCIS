//! Stateless shell implementation of the oracle authentication port.
use crate::{POP_DST, PopKey, Stage, key, signature, verify_parsed};
use zeno_fcis_synthesis::finite::v2_oracle::BlsVerifier;
/// Stateless safe-API verifier for native oracle keys, possession and signatures.
#[derive(Clone, Copy, Debug, Default)]
pub struct OracleVerifier;
impl BlsVerifier for OracleVerifier {
    fn canonical_key(&self, bytes: &[u8]) -> bool {
        key(bytes).is_ok()
    }
    fn canonical_signature(&self, bytes: &[u8]) -> bool {
        signature(bytes).is_ok_and(|s| s.validate(true).is_ok())
    }
    fn possession(&self, bytes: &[u8], proof: &[u8]) -> bool {
        PopKey::admit(bytes, proof).is_ok()
    }
    fn signature(&self, bytes: &[u8], message: &[u8], sig: &[u8]) -> bool {
        if message.len() > 4096 {
            return false;
        }
        let (Ok(pk), Ok(sig)) = (key(bytes), signature(sig)) else {
            return false;
        };
        // Authentication only. The lifecycle independently requires the PoP gate
        // at registration/rotation before retaining any usable reporter key.
        verify_parsed(&pk, message, &sig, POP_DST, Stage::Verification).is_ok()
    }
}
