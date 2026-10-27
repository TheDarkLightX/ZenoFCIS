//! ReceiptVerifierV1: deterministic authentication port, no economic authority.
use crate::Id;
use alloc::vec::Vec;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Domain {
    Actor = 0,
    Clock = 1,
    Oracle = 2,
    Custody = 3,
    Allocation = 4,
    Drip = 5,
    Funding = 6,
    ParameterPolicy = 7,
    ExposureRelease = 8,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Proof {
    pub domain: Domain,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerificationFailure {
    Invalid,
    Unavailable,
    Unsupported,
}
/// Contract ReceiptVerifierV1 (see STEP4.md for exact domain obligations).
///
/// `message` is EXACTLY the domain-separated canonical statement supplied by
/// genesis_messages/receipt_messages: core identity, installed policy, actual
/// pre-root, actor/nonce, command and all explicit facts; proofs are excluded.
/// `authority` comes only from installed policy, never the request. Verify the
/// whole byte string with the pinned scheme/key registry, source graph, epoch,
/// finality and custody/allocation policy required by `domain`. No prefixes,
/// normalization, fallback or unverified derived PriceView may succeed.
///
/// The implementation and its reachable policy are transitively immutable;
/// equal inputs give equal results, without I/O, time, retained scratch or
/// caller-controlled Usage. Err always refuses. Ok supplies authenticated facts
/// conditional on this contract; zUSD alone decides admissibility, arithmetic,
/// resource usage, movements, replay, exposure and publication.
///
/// The shell BLS implementation authenticates a qualified issuer's attestation
/// over these exact bytes. Registry adoption must establish the following
/// semantic premises; cryptographic acceptance alone does not establish them.
/// Oracle additionally verifies exact committed-record extraction and pending
/// restrictions against the accepted oracle graph. Clock verifies certified
/// synchronization; Actor checks actual actor authority for this exact action.
/// Custody/Funding establish backing, existing-asset provenance and unique ingress;
/// Allocation establishes exclusive shared locks for the entire configured risk
/// bound/window. Drip proves the bound schedule on the exact prior state/supply.
/// ParameterPolicy proves a SEPARATELY APPROVED policy, not just a signer.
/// ExposureRelease proves all linked liabilities/finality obligations have ended.
/// The production adapter lives in the inward-dependent shell crate
/// zeno-fcis-bls; this semantic crate has no BLS dependency.
pub trait ReceiptVerifierV1 {
    fn verify(
        &self,
        domain: Domain,
        authority: Id,
        message: &[u8],
        proof: &[u8],
    ) -> Result<(), VerificationFailure>;
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Message {
    pub domain: Domain,
    pub authority: Id,
    pub bytes: Vec<u8>,
}
