//! Shell-side BLS12-381 min_pk authentication. No signing or key generation.
//! See CONTRACT.md for wire formats, issuer premises and ownership boundaries.
#![forbid(unsafe_code)]
#![allow(missing_docs)]
use blst::{
    BLST_ERROR,
    min_pk::{AggregatePublicKey, PublicKey, Signature},
};

pub mod receipt;

pub const BASIC_DST: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_NUL_";
pub const POP_DST: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";
pub const POP_PROOF_DST: &[u8] = b"BLS_POP_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";
pub const MAX_MEMBERS: usize = 64;
// This is a mandatory verification contract, not a configurable profile.
const SIGNATURE_GROUP_CHECK: bool = true;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stage {
    PublicKey,
    Signature,
    Verification,
    Possession,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CryptoError {
    BadEncoding,
    PointNotOnCurve,
    PointNotInGroup,
    AggregateTypeMismatch,
    VerificationFailed,
    PublicKeyInfinity,
    BadScalar,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refusal {
    PublicKeyLength,
    SignatureLength,
    EmptyAggregate,
    TooManyMembers,
    LengthMismatch,
    DuplicateMessage,
    Crypto(Stage, CryptoError),
}
fn checked(code: BLST_ERROR, stage: Stage) -> Result<(), Refusal> {
    use BLST_ERROR::*;
    let reason = match code {
        BLST_SUCCESS => return Ok(()),
        BLST_BAD_ENCODING => CryptoError::BadEncoding,
        BLST_POINT_NOT_ON_CURVE => CryptoError::PointNotOnCurve,
        BLST_POINT_NOT_IN_GROUP => CryptoError::PointNotInGroup,
        BLST_AGGR_TYPE_MISMATCH => CryptoError::AggregateTypeMismatch,
        BLST_VERIFY_FAIL => CryptoError::VerificationFailed,
        BLST_PK_IS_INFINITY => CryptoError::PublicKeyInfinity,
        BLST_BAD_SCALAR => CryptoError::BadScalar,
    };
    Err(Refusal::Crypto(stage, reason))
}
fn decode<T>(result: Result<T, BLST_ERROR>, stage: Stage) -> Result<T, Refusal> {
    result.map_err(|e| {
        checked(e, stage)
            .err()
            .unwrap_or(Refusal::Crypto(stage, CryptoError::VerificationFailed))
    })
}
fn key(bytes: &[u8]) -> Result<PublicKey, Refusal> {
    if bytes.len() != 48 {
        return Err(Refusal::PublicKeyLength);
    }
    decode(PublicKey::key_validate(bytes), Stage::PublicKey)
}
fn signature(bytes: &[u8]) -> Result<Signature, Refusal> {
    if bytes.len() != 96 {
        return Err(Refusal::SignatureLength);
    }
    decode(Signature::from_bytes(bytes), Stage::Signature)
}
fn count(n: usize) -> Result<(), Refusal> {
    if n == 0 {
        Err(Refusal::EmptyAggregate)
    } else if n > MAX_MEMBERS {
        Err(Refusal::TooManyMembers)
    } else {
        Ok(())
    }
}
// Private, parsed-input primitive. Public entry points also key_validate every
// key. Unit tests exercise key validation and enforce the signature-check contract.
fn verify_parsed(
    pk: &PublicKey,
    msg: &[u8],
    sig: &Signature,
    dst: &[u8],
    stage: Stage,
) -> Result<(), Refusal> {
    checked(
        sig.verify(SIGNATURE_GROUP_CHECK, msg, dst, &[], pk, true),
        stage,
    )
}
pub fn validate_key(bytes: &[u8]) -> Result<(), Refusal> {
    key(bytes).map(|_| ())
}
pub fn basic_verify(pk: &[u8], msg: &[u8], sig: &[u8]) -> Result<(), Refusal> {
    verify_parsed(
        &key(pk)?,
        msg,
        &signature(sig)?,
        BASIC_DST,
        Stage::Verification,
    )
}
pub fn basic_aggregate_verify(
    pks: &[&[u8]],
    messages: &[&[u8]],
    sig: &[u8],
) -> Result<(), Refusal> {
    count(pks.len())?;
    if pks.len() != messages.len() {
        return Err(Refusal::LengthMismatch);
    }
    if messages
        .iter()
        .enumerate()
        .any(|(i, m)| messages[..i].contains(m))
    {
        return Err(Refusal::DuplicateMessage);
    }
    let keys = pks.iter().map(|p| key(p)).collect::<Result<Vec<_>, _>>()?;
    let refs = keys.iter().collect::<Vec<_>>();
    checked(
        signature(sig)?.aggregate_verify(SIGNATURE_GROUP_CHECK, messages, BASIC_DST, &refs, true),
        Stage::Verification,
    )
}
/// An owned, validated key whose exact compressed bytes passed a PoP proof.
/// Construction is possible only through admit; no unchecked import exists.
#[derive(Clone, Debug)]
pub struct PopKey {
    bytes: [u8; 48],
}
impl PopKey {
    pub fn admit(pk: &[u8], proof: &[u8]) -> Result<Self, Refusal> {
        let parsed = key(pk)?;
        verify_parsed(
            &parsed,
            pk,
            &signature(proof)?,
            POP_PROOF_DST,
            Stage::Possession,
        )?;
        let mut bytes = [0; 48];
        bytes.copy_from_slice(pk);
        Ok(Self { bytes })
    }
    pub fn as_bytes(&self) -> &[u8; 48] {
        &self.bytes
    }
}
pub fn pop_verify(pk: &PopKey, msg: &[u8], sig: &[u8]) -> Result<(), Refusal> {
    verify_parsed(
        &key(&pk.bytes)?,
        msg,
        &signature(sig)?,
        POP_DST,
        Stage::Verification,
    )
}
pub fn pop_aggregate_verify(
    pks: &[&PopKey],
    messages: &[&[u8]],
    sig: &[u8],
) -> Result<(), Refusal> {
    count(pks.len())?;
    if pks.len() != messages.len() {
        return Err(Refusal::LengthMismatch);
    }
    let keys = pks
        .iter()
        .map(|p| key(&p.bytes))
        .collect::<Result<Vec<_>, _>>()?;
    checked(
        signature(sig)?.aggregate_verify(
            SIGNATURE_GROUP_CHECK,
            messages,
            POP_DST,
            &keys.iter().collect::<Vec<_>>(),
            true,
        ),
        Stage::Verification,
    )
}
pub fn pop_fast_aggregate_verify(pks: &[&PopKey], msg: &[u8], sig: &[u8]) -> Result<(), Refusal> {
    count(pks.len())?;
    let keys = pks
        .iter()
        .map(|p| key(&p.bytes))
        .collect::<Result<Vec<_>, _>>()?;
    // blst's fast_aggregate_verify internally sets pk_validate=false. Instead,
    // aggregate validated members and verify the aggregate with BOTH flags true.
    let aggregate = decode(
        AggregatePublicKey::aggregate(&keys.iter().collect::<Vec<_>>(), true),
        Stage::PublicKey,
    )?
    .to_public_key();
    verify_parsed(
        &aggregate,
        msg,
        &signature(sig)?,
        POP_DST,
        Stage::Verification,
    )
}

#[cfg(test)]
mod tests;
