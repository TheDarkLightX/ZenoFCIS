//! Immutable, explicit registry adapter for ReceiptVerifierV1.
use crate::{MAX_MEMBERS, PopKey, Refusal, basic_verify, pop_fast_aggregate_verify, validate_key};
use zenofcis_zusd_v1::{
    Id,
    application::{Domain, ReceiptVerifierV1, VerificationFailure},
};

const PREFIX: &[u8] = b"zusd.ReceiptVerifierV1/1\0";
#[derive(Clone, Debug)]
enum Keys {
    Basic([u8; 48]),
    Quorum {
        members: Vec<PopKey>,
        threshold: usize,
    },
}
#[derive(Clone, Debug)]
pub struct Authority {
    domain: Domain,
    id: Id,
    keys: Keys,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReceiptRefusal {
    Crypto(Refusal),
    InvalidRegistry,
    UnknownAuthority,
    MessageTooLarge,
    CanonicalMessage,
    ProofEncoding,
    WrongSuite,
    InsufficientQuorum,
    SignerOrder,
}
impl From<Refusal> for ReceiptRefusal {
    fn from(e: Refusal) -> Self {
        Self::Crypto(e)
    }
}
impl Authority {
    /// Caller must qualify the issuer for this domain's semantic premises.
    pub fn basic(domain: Domain, id: Id, public_key: [u8; 48]) -> Result<Self, ReceiptRefusal> {
        validate_key(&public_key)?;
        Ok(Self {
            domain,
            id,
            keys: Keys::Basic(public_key),
        })
    }
    /// All members must have passed PopKey::admit. Duplicate keys cannot vote twice.
    pub fn quorum(
        domain: Domain,
        id: Id,
        members: Vec<PopKey>,
        threshold: usize,
    ) -> Result<Self, ReceiptRefusal> {
        if members.is_empty()
            || members.len() > MAX_MEMBERS
            || threshold == 0
            || threshold > members.len()
            || members
                .iter()
                .enumerate()
                .any(|(i, p)| members[..i].iter().any(|q| p.as_bytes() == q.as_bytes()))
        {
            return Err(ReceiptRefusal::InvalidRegistry);
        }
        Ok(Self {
            domain,
            id,
            keys: Keys::Quorum { members, threshold },
        })
    }
}
/// No implicit registry, acquisition, clock reads or mutable key rotation.
#[derive(Clone, Debug)]
pub struct BlsReceiptVerifier {
    authorities: Vec<Authority>,
    max_message_bytes: usize,
}
impl BlsReceiptVerifier {
    pub fn new(
        authorities: Vec<Authority>,
        max_message_bytes: usize,
    ) -> Result<Self, ReceiptRefusal> {
        if authorities.is_empty()
            || authorities.len() > 256
            || max_message_bytes < PREFIX.len() + 137
            || authorities.iter().enumerate().any(|(i, a)| {
                authorities[..i]
                    .iter()
                    .any(|b| a.domain == b.domain && a.id == b.id)
            })
        {
            return Err(ReceiptRefusal::InvalidRegistry);
        }
        Ok(Self {
            authorities,
            max_message_bytes,
        })
    }
    pub fn verify_detailed(
        &self,
        domain: Domain,
        authority: Id,
        message: &[u8],
        proof: &[u8],
    ) -> Result<(), ReceiptRefusal> {
        if message.len() > self.max_message_bytes {
            return Err(ReceiptRefusal::MessageTooLarge);
        }
        // Check the port envelope too: exact domain, authority and body length.
        let body = PREFIX.len() + 1 + 4 * 32 + 8;
        if message.len() < body
            || !message.starts_with(PREFIX)
            || message[PREFIX.len()] != domain as u8
            || message[PREFIX.len() + 1..PREFIX.len() + 33] != authority
            || u64::from_be_bytes(
                message[body - 8..body]
                    .try_into()
                    .map_err(|_| ReceiptRefusal::CanonicalMessage)?,
            ) != (message.len() - body) as u64
        {
            return Err(ReceiptRefusal::CanonicalMessage);
        }
        let a = self
            .authorities
            .iter()
            .find(|a| a.domain == domain && a.id == authority)
            .ok_or(ReceiptRefusal::UnknownAuthority)?;
        if proof.len() < 4 || proof[0] != 1 {
            return Err(ReceiptRefusal::ProofEncoding);
        }
        let n = u16::from_be_bytes([proof[2], proof[3]]) as usize;
        if n == 0 || n > MAX_MEMBERS || proof.len() != 4 + n * 2 + 96 {
            return Err(ReceiptRefusal::ProofEncoding);
        }
        let indices = proof[4..4 + n * 2]
            .chunks_exact(2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]) as usize)
            .collect::<Vec<_>>();
        if indices.windows(2).any(|w| w[0] >= w[1]) {
            return Err(ReceiptRefusal::SignerOrder);
        }
        let sig = &proof[4 + n * 2..];
        match &a.keys {
            Keys::Basic(key) => {
                if proof[1] != 0 {
                    return Err(ReceiptRefusal::WrongSuite);
                }
                if indices != [0] {
                    return Err(ReceiptRefusal::ProofEncoding);
                }
                basic_verify(key, message, sig)?;
            }
            Keys::Quorum { members, threshold } => {
                if proof[1] != 1 {
                    return Err(ReceiptRefusal::WrongSuite);
                }
                if n < *threshold {
                    return Err(ReceiptRefusal::InsufficientQuorum);
                }
                let selected = indices
                    .iter()
                    .map(|i| members.get(*i).ok_or(ReceiptRefusal::ProofEncoding))
                    .collect::<Result<Vec<_>, _>>()?;
                pop_fast_aggregate_verify(&selected, message, sig)?;
            }
        }
        Ok(())
    }
}
impl ReceiptVerifierV1 for BlsReceiptVerifier {
    fn verify(
        &self,
        domain: Domain,
        authority: Id,
        message: &[u8],
        proof: &[u8],
    ) -> Result<(), VerificationFailure> {
        self.verify_detailed(domain, authority, message, proof)
            .map_err(|e| match e {
                ReceiptRefusal::UnknownAuthority => VerificationFailure::Unsupported,
                _ => VerificationFailure::Invalid,
            })
    }
}
