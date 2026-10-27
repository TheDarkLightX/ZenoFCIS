//! Installed authentication port. Implementations are trusted, immutable shell code.
//! They must validate every key, reject infinity/noncanonical/subgroup encodings,
//! use the fixed PoP domains, and enable both group checks. No state, policy,
//! resource-use or publication authority is delegated to this port.

use super::Failure;
use super::types::BLS_POP_SCHEME;
/// Installed immutable shell authentication port with full key and group validation.
pub trait BlsVerifier {
    /// Validate canonical key encoding, non-infinity and group membership.
    fn canonical_key(&self, key: &[u8]) -> bool;
    /// Validate canonical signature encoding, non-infinity and group membership.
    fn canonical_signature(&self, signature: &[u8]) -> bool;
    /// Verify the fixed-domain proof of possession with both group checks.
    fn possession(&self, key: &[u8], proof: &[u8]) -> bool;
    /// Verify a fixed-domain signature with key validation and both group checks.
    fn signature(&self, key: &[u8], message: &[u8], signature: &[u8]) -> bool;
}

/// Compatibility preparation; it cannot register a key or admit a report.
pub fn verify_bls_pop(
    verifier: &impl BlsVerifier,
    scheme: &str,
    key: &[u8],
    pop: &[u8],
    message: &[u8],
    signature: &[u8],
) -> Result<(), Failure> {
    if scheme != BLS_POP_SCHEME {
        return Err(Failure::UnsupportedScheme);
    }
    if !verifier.possession(key, pop) || !verifier.signature(key, message, signature) {
        return Err(Failure::InvalidEvidence);
    }
    Ok(())
}
