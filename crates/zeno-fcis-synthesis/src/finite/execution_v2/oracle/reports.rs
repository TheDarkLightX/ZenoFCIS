//! Exact report bytes/linked-sequence relation and closed raw signature gate.
//! Sequence success is arithmetic evidence, never reporter authentication.

use super::types::{BLS_POP_SCHEME, Hash, Id, Observation, PriceE8, REPORT_DOMAIN, Reason};
use super::verification::{BlsVerifier, verify_bls_pop};
use super::wire;
use super::{Evaluation, Failure, Limits, Meter, Resource, measured, step};
use alloc::{format, string::String, vec::Vec};
use zeno_fcis_codec::CommitmentHasher;
use zeno_fcis_crypto::RustCryptoSha256;

// Typed IDs are at most 64 bytes, all other fields have fixed widths, and
// integers have at most 39 decimal digits. Reserve before rendering/hashing;
// actual canonical-byte charges below are additional logical work.
const SIGNING_SCRATCH_BYTES: u64 = 4096;

/// Canonical report fields bound into the signed message.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReportPayload {
    /// Chain identity bound into signed reports.
    pub chain: Id,
    /// Reporter identity.
    pub reporter: Id,
    /// Reporter key version bound into the payload.
    pub key_version: u64,
    /// Canonical feed identity.
    pub feed: Hash,
    /// Bound profile identity.
    pub profile: Hash,
    /// Canonical query identity.
    pub query: Hash,
    /// Source identity.
    pub source: Id,
    /// Logical round identifier.
    pub round: u64,
    /// Positive price or stress observation in E8 units.
    pub value: PriceE8,
    /// Logical epoch of the observation.
    pub observed_epoch: u64,
    /// Reported observation timestamp in seconds.
    pub observation_seconds: u64,
    /// Monotone report sequence number.
    pub sequence: u64,
    /// Identity of the preceding report, if any.
    pub previous: Option<Hash>,
}
impl ReportPayload {
    /// Native extended schema. Fixed ASCII field names are sorted; opaque IDs
    /// use lowercase hex, and every integer is written directly in base 10.
    pub fn canonical_json(&self) -> Vec<u8> {
        let previous = self
            .previous
            .map(|h| format!("\"sha256:{}\"", hex(&h)))
            .unwrap_or_else(|| String::from("null"));
        format!("{{\"chain_id\":\"{}\",\"feed_id\":\"sha256:{}\",\"key_version\":{},\"observation_seconds\":{},\"observed_epoch\":{},\"payload_schema\":\"zeno.oracle.report_payload/1\",\"previous_report_id\":{},\"profile_id\":\"sha256:{}\",\"query_id\":\"sha256:{}\",\"reporter_id\":\"{}\",\"round_id\":{},\"scheme_id\":\"{}\",\"sequence\":{},\"source_id\":\"{}\",\"value_e8\":{}}}",
            hex(self.chain.bytes()),hex(&self.feed),self.key_version,self.observation_seconds,self.observed_epoch,previous,
            hex(&self.profile),hex(&self.query),hex(self.reporter.bytes()),self.round,BLS_POP_SCHEME,self.sequence,hex(self.source.bytes()),self.value.raw()).into_bytes()
    }
    /// Return the fixed-domain digest signed by the reporter.
    pub fn message(&self) -> Hash {
        *RustCryptoSha256::hash_parts(&[REPORT_DOMAIN, &self.canonical_json()]).as_bytes()
    }
    /// Derive the canonical report identity from this payload and signature.
    pub fn report_id(&self, signature: &[u8]) -> Hash {
        let payload_hash = RustCryptoSha256::hash(&self.canonical_json());
        let json = format!(
            "{{\"payload\":{},\"payload_hash\":\"sha256:{}\",\"schema\":\"zeno.oracle.report/1\",\"signature\":\"{}\"}}",
            String::from_utf8_lossy(&self.canonical_json()),
            hex(payload_hash.as_bytes()),
            hex(signature)
        );
        *RustCryptoSha256::hash(json.as_bytes()).as_bytes()
    }
}
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::new();
    for b in bytes {
        out.push(char::from(DIGITS[usize::from(b >> 4)]));
        out.push(char::from(DIGITS[usize::from(b & 15)]));
    }
    out
}
/// Reporter sequence, previous-report link and observation-time cursor.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct ReportChain {
    /// Next report sequence number expected from this reporter.
    pub next_sequence: u64,
    /// Identity of the preceding report, if any.
    pub previous: Option<Hash>,
    /// Previous observation time in seconds, if any.
    pub last_seconds: Option<u64>,
}
/// Advance a report cursor after ordered sequence and timestamp checks.
pub fn report_chain_relation(
    chain: &ReportChain,
    payload: &ReportPayload,
    report_id: Hash,
    now: u64,
    max_age: u64,
    limits: Limits,
) -> Evaluation<Result<ReportChain, Reason>> {
    measured(limits, |m| {
        let result = chain_delta(chain, payload, report_id, now, max_age, m)?;
        super::laws::chain(
            chain,
            payload,
            report_id,
            now,
            max_age,
            result.as_ref().map_err(|r| *r),
            m,
        )?;
        Ok(result)
    })
}
fn chain_delta(
    chain: &ReportChain,
    p: &ReportPayload,
    id: Hash,
    now: u64,
    max_age: u64,
    m: &mut Meter,
) -> Result<Result<ReportChain, Reason>, Failure> {
    step(m)?;
    let Some(next) = chain.next_sequence.checked_add(1) else {
        return Ok(Err(Reason::CounterExhausted));
    };
    step(m)?;
    if p.sequence != chain.next_sequence || p.previous != chain.previous {
        return Ok(Err(Reason::BadSequence));
    }
    step(m)?;
    if p.observed_epoch > now {
        return Ok(Err(Reason::FutureObservation));
    }
    if chain
        .last_seconds
        .is_some_and(|old| p.observation_seconds < old)
    {
        return Ok(Err(Reason::BackwardObservation));
    }
    if now - p.observed_epoch > max_age {
        return Ok(Err(Reason::StaleObservation));
    }
    Ok(Ok(ReportChain {
        next_sequence: next,
        previous: Some(id),
        last_seconds: Some(p.observation_seconds),
    }))
}

/// Exact raw key payload; no unchecked Boolean or callback can establish PoP.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyRegistration {
    /// Reporter identity.
    pub reporter: Id,
    /// Account that owns the reporter bond.
    pub owner: Id,
    /// Monotone registration version.
    pub version: u64,
    /// Compressed BLS G1 public key.
    pub public_key: [u8; 48],
    /// Proof of possession for the registered public key.
    pub possession: [u8; 96],
}
/// Untrusted signed report before admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawReport {
    /// Signed report payload.
    pub payload: ReportPayload,
    /// Digest of the canonical report payload.
    pub payload_hash: Hash,
    /// Canonical identity of this record.
    pub id: Hash,
    /// Compressed BLS G2 signature over the report message.
    pub signature: [u8; 96],
}
/// Compatibility audit entry: checks native signature bytes and the sequence,
/// then refuses the missing lifecycle binding. Native admission must also join
/// retained feed/round/key state and reserve its bond in the same candidate.
pub fn admit_signed_report(
    verifier: &impl BlsVerifier,
    key: &KeyRegistration,
    chain: &ReportChain,
    report: &RawReport,
    now: u64,
    max_age: u64,
    limits: Limits,
) -> Evaluation<Result<Observation, Reason>> {
    measured(limits, |m| {
        step(m)?;
        m.charge(Resource::Byte, SIGNING_SCRATCH_BYTES)?;
        let bytes = report.payload.canonical_json();
        m.charge(
            Resource::Byte,
            u64::try_from(bytes.len()).map_err(|_| Failure::Arithmetic)?,
        )?;
        if *RustCryptoSha256::hash(&bytes).as_bytes() != report.payload_hash
            || report.payload.report_id(&report.signature) != report.id
        {
            return Err(Failure::InvalidEvidence);
        }
        m.charge(Resource::WitnessByte, 272)?;
        step(m)?;
        verify_bls_pop(
            verifier,
            BLS_POP_SCHEME,
            &key.public_key,
            &key.possession,
            &report.payload.message(),
            &report.signature,
        )?;
        if key.reporter != report.payload.reporter || key.version != report.payload.key_version {
            return Ok(Err(Reason::BindingMismatch));
        }
        match chain_delta(chain, &report.payload, report.id, now, max_age, m)? {
            Ok(_) => {}
            Err(r) => return Ok(Err(r)),
        }
        // Lifecycle and free bond remain separate mandatory admission ports.
        // No Observation can escape this incomplete raw boundary.
        Err(Failure::UnboundParameter)
    })
}

/// Key-proof preparation only; neither helper registers a retained reporter.
pub fn register_key(
    verifier: &impl BlsVerifier,
    key: &KeyRegistration,
    limits: Limits,
) -> Evaluation<()> {
    measured(limits, |m| {
        step(m)?;
        m.charge(Resource::WitnessByte, 144)?;
        if verifier.possession(&key.public_key, &key.possession) {
            Ok(())
        } else {
            Err(Failure::InvalidEvidence)
        }
    })
}
/// Verify both rotation authorizations and the new key’s proof of possession.
pub fn rotate_key(
    verifier: &impl BlsVerifier,
    old: &KeyRegistration,
    new: &KeyRegistration,
    old_signature: &[u8; 96],
    new_signature: &[u8; 96],
    limits: Limits,
) -> Evaluation<Result<(), Reason>> {
    measured(limits, |m| {
        step(m)?;
        let Some(next) = old.version.checked_add(1) else {
            return Ok(Err(Reason::CounterExhausted));
        };
        m.charge(Resource::Byte, SIGNING_SCRATCH_BYTES)?;
        let subject = wire::record(alloc::vec![
            wire::blob(old.reporter.bytes())?,
            wire::blob(old.owner.bytes())?,
            wire::integers(&[u128::from(old.version), u128::from(new.version)])?,
            wire::blob(&old.public_key)?,
            wire::blob(&new.public_key)?
        ])?;
        wire::charge(&subject, m)?;
        let message = wire::hash(b"zeno.oracle.rotate/1\0", &subject)?;
        m.charge(Resource::WitnessByte, 544)?;
        step(m)?;
        verify_bls_pop(
            verifier,
            BLS_POP_SCHEME,
            &old.public_key,
            &old.possession,
            &message,
            old_signature,
        )?;
        verify_bls_pop(
            verifier,
            BLS_POP_SCHEME,
            &new.public_key,
            &new.possession,
            &message,
            new_signature,
        )?;
        if old.reporter != new.reporter || old.owner != new.owner || new.version != next {
            return Ok(Err(Reason::BadPoPOrRotation));
        }
        Ok(Ok(()))
    })
}
