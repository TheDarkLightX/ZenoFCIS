//! Replay-checked decision evidence, with no execution or publication authority.
//!
//! This optional pure checker compares an untrusted claim with the result of an
//! independently selected trusted relation. Its output is **not** a proof of
//! correct policy formalization, a zero-knowledge proof, an authorization token,
//! an authentication result, or evidence that an external effect happened.
//! Actual commits must still use the existing V2 Authority and atomic shell.
//!
//! The stable wire format is intentionally shared with ZAL's finite-step profile.
//! It uses exact bytes: magic, two 32-byte identities, then five length-prefixed
//! fields. A digest fingerprints a *claim*; hashing alone cannot prove it true.

use alloc::boxed::Box;
use alloc::vec::Vec;

use zeno_fcis_codec::{CommitmentHasher, Hash32, RustCryptoSha256};

/// Profile identifier for the portable decision claim.
pub const FORMAT: &str = "zeno-fcis/decision-evidence/1";
const MAGIC: &[u8] = b"ZFCIS-PCD1\0";
const HEADER_BYTES: usize = 11 + 64 + 5 * 4;
/// Largest admitted field, in bytes.
pub const MAX_FIELD_BYTES: usize = 16_384;
/// Largest admitted envelope, in bytes.
pub const MAX_ENVELOPE_BYTES: usize = 65_536;

/// Reasons to refuse evidence rather than admit an unverified decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refusal {
    /// Invalid encoding, length, version or claimed identity shape.
    InvalidEvidence,
    /// Evidence does not bind the independently selected specification/policy.
    IdentityMismatch,
    /// Evidence does not bind the provided current state, command, and context.
    InputMismatch,
    /// Prestate is outside the relation's supported invariant.
    BeforeInvariant,
    /// Recomputed successor does not preserve the relation's invariant.
    AfterInvariant,
    /// The relation denies the action.
    Denied,
    /// The relation is unable to decide in the current scope.
    Unknown,
    /// Claimed successor or effect differs from the recomputed decision.
    ResultMismatch,
}

/// An untrusted, owned, canonical claim, constructible by anyone.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecisionEvidence {
    specification: Hash32,
    policy: Hash32,
    prestate: Box<[u8]>,
    command: Box<[u8]>,
    context: Box<[u8]>,
    poststate: Box<[u8]>,
    effect: Box<[u8]>,
}

impl DecisionEvidence {
    /// Construct untrusted transport bytes, not a verified decision.
    pub fn new(
        specification: Hash32,
        policy: Hash32,
        prestate: Vec<u8>,
        command: Vec<u8>,
        context: Vec<u8>,
        poststate: Vec<u8>,
        effect: Vec<u8>,
    ) -> Result<Self, Refusal> {
        let fields = [
            prestate.as_slice(), command.as_slice(), context.as_slice(),
            poststate.as_slice(), effect.as_slice(),
        ];
        let total = HEADER_BYTES + fields.iter().map(|field| field.len()).sum::<usize>();
        if specification == Hash32::ZERO
            || policy == Hash32::ZERO
            || prestate.is_empty()
            || command.is_empty()
            || poststate.is_empty()
            || total > MAX_ENVELOPE_BYTES
            || fields.iter().any(|field| field.len() > MAX_FIELD_BYTES)
        {
            return Err(Refusal::InvalidEvidence);
        }
        Ok(Self {
            specification,
            policy,
            prestate: prestate.into_boxed_slice(),
            command: command.into_boxed_slice(),
            context: context.into_boxed_slice(),
            poststate: poststate.into_boxed_slice(),
            effect: effect.into_boxed_slice(),
        })
    }

    /// Decode exact versioned bytes; reject truncation, trailing data and huge lengths.
    pub fn decode(bytes: &[u8]) -> Result<Self, Refusal> {
        if !(HEADER_BYTES..=MAX_ENVELOPE_BYTES).contains(&bytes.len()) || !bytes.starts_with(MAGIC) {
            return Err(Refusal::InvalidEvidence);
        }
        let mut at = MAGIC.len();
        let mut spec = [0_u8; 32];
        let mut policy = [0_u8; 32];
        spec.copy_from_slice(&bytes[at..at + 32]);
        at += 32;
        policy.copy_from_slice(&bytes[at..at + 32]);
        at += 32;
        let prestate = read_field(bytes, &mut at)?;
        let command = read_field(bytes, &mut at)?;
        let context = read_field(bytes, &mut at)?;
        let poststate = read_field(bytes, &mut at)?;
        let effect = read_field(bytes, &mut at)?;
        if at != bytes.len() {
            return Err(Refusal::InvalidEvidence);
        }
        Self::new(Hash32::new(spec), Hash32::new(policy), prestate, command,
                  context, poststate, effect)
    }

    /// Canonical wire bytes. Only this module constructs the owned fields.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let fields = [self.prestate.as_ref(), self.command.as_ref(), self.context.as_ref(),
                      self.poststate.as_ref(), self.effect.as_ref()];
        let mut output = Vec::with_capacity(
            HEADER_BYTES + fields.iter().map(|field| field.len()).sum::<usize>()
        );
        output.extend_from_slice(MAGIC);
        output.extend_from_slice(self.specification.as_bytes());
        output.extend_from_slice(self.policy.as_bytes());
        for field in fields {
            output.extend_from_slice(&(field.len() as u32).to_be_bytes());
            output.extend_from_slice(field);
        }
        output
    }

    /// Transport fingerprint; not a proof or an authority credential.
    #[must_use]
    pub fn digest(&self) -> Hash32 {
        RustCryptoSha256::hash(&self.encode())
    }

    /// Claimed independently bound specification identity.
    #[must_use]
    pub const fn specification(&self) -> Hash32 { self.specification }
    /// Claimed independently bound policy and verification-semantics identity.
    #[must_use]
    pub const fn policy(&self) -> Hash32 { self.policy }
    /// Claimed state before the decision.
    #[must_use]
    pub fn prestate(&self) -> &[u8] { &self.prestate }
    /// Claimed exact command bytes.
    #[must_use]
    pub fn command(&self) -> &[u8] { &self.command }
    /// Claimed exact observed context bytes.
    #[must_use]
    pub fn context(&self) -> &[u8] { &self.context }
    /// Claimed state after the decision.
    #[must_use]
    pub fn poststate(&self) -> &[u8] { &self.poststate }
    /// Claimed external effect descriptor.
    #[must_use]
    pub fn effect(&self) -> &[u8] { &self.effect }
}

fn read_field(bytes: &[u8], at: &mut usize) -> Result<Vec<u8>, Refusal> {
    if bytes.len().saturating_sub(*at) < 4 {
        return Err(Refusal::InvalidEvidence);
    }
    let mut len_bytes = [0; 4];
    len_bytes.copy_from_slice(&bytes[*at..*at + 4]);
    let length = u32::from_be_bytes(len_bytes) as usize;
    *at += 4;
    if length > MAX_FIELD_BYTES || length > bytes.len().saturating_sub(*at) {
        return Err(Refusal::InvalidEvidence);
    }
    let field = bytes[*at..*at + length].to_vec();
    *at += length;
    Ok(field)
}

/// Pure result produced by the independently selected decision relation.
pub struct DecisionOutput {
    /// Exact successor state, not yet committed.
    pub poststate: Vec<u8>,
    /// Exact effect request, not yet executed.
    pub effect: Vec<u8>,
}

/// A denial or insufficiently supported decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvaluationError {
    /// The pinned policy does not authorize this action.
    Denied,
    /// The pinned implementation cannot decide, or is outside its scope.
    Unknown,
}

/// Independently selected trusted relation; never supplied by the claimant.
///
/// Implementations must bind their semantics and all relevant policy inputs in
/// the returned identities. The verifier cannot establish that the chosen
/// relation correctly represents natural-language intent.
pub trait DecisionRelation {
    /// Bound specification digest.
    fn specification(&self) -> Hash32;
    /// Bound policy, interpreter and verification semantics digest.
    fn policy(&self) -> Hash32;
    /// Check one concrete state; not a claim about all reachable states.
    fn invariant(&self, state: &[u8]) -> bool;
    /// Independently replay the decision, or fail closed.
    fn decide(&self, state: &[u8], command: &[u8], context: &[u8])
        -> Result<DecisionOutput, EvaluationError>;
}

/// Non-authoritative observation of a replay-checked claim.
pub struct CheckedDecision {
    digest: Hash32,
    poststate: Box<[u8]>,
    effect: Box<[u8]>,
}

impl CheckedDecision {
    /// Fingerprint of the exact checked claim (not a cryptographic proof).
    #[must_use]
    pub const fn digest(&self) -> Hash32 { self.digest }
    /// Recomputed successor bytes; no commit has occurred.
    #[must_use]
    pub fn poststate(&self) -> &[u8] { &self.poststate }
    /// Recomputed requested effect bytes; no effect has occurred.
    #[must_use]
    pub fn effect(&self) -> &[u8] { &self.effect }
}

/// Replay a claim against a separately trusted relation and caller-supplied
/// **current** inputs. No I/O, mutable alias, state advance, or publication.
///
/// Checking once is not enough to prevent TOCTOU: the caller still needs the
/// existing atomic expected-state-root check at its separate commit boundary.
pub fn verify<R: DecisionRelation>(
    trusted: &R,
    evidence: &DecisionEvidence,
    current_state: &[u8],
    command: &[u8],
    context: &[u8],
) -> Result<CheckedDecision, Refusal> {
    if evidence.specification != trusted.specification() || evidence.policy != trusted.policy() {
        return Err(Refusal::IdentityMismatch);
    }
    if evidence.prestate() != current_state ||
        evidence.command() != command || evidence.context() != context {
        return Err(Refusal::InputMismatch);
    }
    if !trusted.invariant(current_state) {
        return Err(Refusal::BeforeInvariant);
    }
    let result = trusted.decide(current_state, command, context).map_err(|failure| match failure {
        EvaluationError::Denied => Refusal::Denied,
        EvaluationError::Unknown => Refusal::Unknown,
    })?;
    if !trusted.invariant(&result.poststate) {
        return Err(Refusal::AfterInvariant);
    }
    if evidence.poststate() != result.poststate || evidence.effect() != result.effect {
        return Err(Refusal::ResultMismatch);
    }
    Ok(CheckedDecision {
        digest: evidence.digest(),
        poststate: result.poststate.into_boxed_slice(),
        effect: result.effect.into_boxed_slice(),
    })
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    const S: Hash32 = Hash32::new([1_u8; 32]);
    const P: Hash32 = Hash32::new([2_u8; 32]);

    struct Meter {
        specification: Hash32,
        policy: Hash32,
    }

    impl DecisionRelation for Meter {
        fn specification(&self) -> Hash32 { self.specification }
        fn policy(&self) -> Hash32 { self.policy }
        fn invariant(&self, state: &[u8]) -> bool {
            state.len() == 1 && state[0] <= 10
        }
        fn decide(&self, state: &[u8], command: &[u8], context: &[u8])
            -> Result<DecisionOutput, EvaluationError> {
            if context != b"authorized" { return Err(EvaluationError::Denied); }
            if command.len() != 1 || state.len() != 1 {
                return Err(EvaluationError::Unknown);
            }
            let Some(after) = state[0].checked_add(command[0]) else {
                return Err(EvaluationError::Unknown);
            };
            if after > 10 { return Err(EvaluationError::Denied); }
            Ok(DecisionOutput { poststate: vec![after], effect: vec![command[0]] })
        }
    }

    fn trusted() -> Meter { Meter { specification: S, policy: P } }

    fn claim() -> DecisionEvidence {
        match DecisionEvidence::new(S, P, vec![4], vec![2], b"authorized".to_vec(), vec![6], vec![2]) {
            Ok(v) => v,
            Err(e) => panic!("{e:?}"),
        }
    }

    fn checked(claim: &DecisionEvidence, state: &[u8], command: &[u8], context: &[u8])
        -> Result<CheckedDecision, Refusal> {
        verify(&trusted(), claim, state, command, context)
    }

    #[test]
    fn round_trip_and_independent_replay() {
        let evidence = claim();
        let wire = evidence.encode();
        assert_eq!(DecisionEvidence::decode(&wire), Ok(evidence.clone()));
        let result = checked(&evidence, &[4], &[2], b"authorized");
        let result = match result { Ok(v) => v, Err(e) => panic!("{e:?}") };
        assert_eq!(result.poststate(), &[6]);
        assert_eq!(result.effect(), &[2]);
        assert_eq!(result.digest(), evidence.digest());
    }

    #[test]
    fn refuse_stale_or_substituted_claims() {
        let evidence = claim();
        assert!(matches!(checked(&evidence, &[3], &[2], b"authorized"), Err(Refusal::InputMismatch)));
        assert!(matches!(checked(&evidence, &[4], &[3], b"authorized"), Err(Refusal::InputMismatch)));
        assert!(matches!(checked(&evidence, &[4], &[2], b"other"), Err(Refusal::InputMismatch)));
        let changed = Meter { specification: S, policy: Hash32::new([3; 32]) };
        assert!(matches!(verify(&changed, &evidence, &[4], &[2], b"authorized"), Err(Refusal::IdentityMismatch)));
        let changed = Meter { specification: Hash32::new([3; 32]), policy: P };
        assert!(matches!(verify(&changed, &evidence, &[4], &[2], b"authorized"), Err(Refusal::IdentityMismatch)));
    }

    #[test]
    fn refuse_tamper_and_denials() {
        let evidence = claim();
        let mut forged = evidence.clone();
        forged.poststate = vec![9].into_boxed_slice();
        assert!(matches!(checked(&forged, &[4], &[2], b"authorized"), Err(Refusal::ResultMismatch)));
        forged = evidence.clone();
        forged.effect = vec![3].into_boxed_slice();
        assert!(matches!(checked(&forged, &[4], &[2], b"authorized"), Err(Refusal::ResultMismatch)));
        let denied = DecisionEvidence::new(S, P, vec![4], vec![2], b"no".to_vec(), vec![6], vec![2]);
        let denied = match denied { Ok(v) => v, Err(e) => panic!("{e:?}") };
        assert!(matches!(checked(&denied, &[4], &[2], b"no"), Err(Refusal::Denied)));
        let impossible = DecisionEvidence::new(S, P, vec![4], vec![7], b"authorized".to_vec(), vec![11], vec![7]);
        let impossible = match impossible { Ok(v) => v, Err(e) => panic!("{e:?}") };
        assert!(matches!(checked(&impossible, &[4], &[7], b"authorized"), Err(Refusal::Denied)));
        assert!(matches!(checked(&evidence, &[11], &[2], b"authorized"), Err(Refusal::InputMismatch)));
    }

    #[test]
    fn refuse_invalid_encoding() {
        let evidence = claim();
        let wire = evidence.encode();
        for invalid in [
            wire[..wire.len() - 1].to_vec(),
            [wire.clone(), vec![0]].concat(),
            [b"bad".to_vec(), wire[3..].to_vec()].concat(),
            vec![0; MAX_ENVELOPE_BYTES + 1],
        ] {
            assert!(matches!(DecisionEvidence::decode(&invalid), Err(Refusal::InvalidEvidence)));
        }
        let mut impossible_len = wire.clone();
        let offset = MAGIC.len() + 64;
        impossible_len[offset..offset + 4].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(matches!(DecisionEvidence::decode(&impossible_len), Err(Refusal::InvalidEvidence)));
        assert!(matches!(
            DecisionEvidence::new(S, P, vec![4], vec![2], vec![], vec![6], vec![0; MAX_FIELD_BYTES + 1]),
            Err(Refusal::InvalidEvidence)
        ));
        assert!(matches!(
            DecisionEvidence::new(Hash32::ZERO, P, vec![4], vec![2], vec![], vec![6], vec![]),
            Err(Refusal::InvalidEvidence)
        ));
    }
}
