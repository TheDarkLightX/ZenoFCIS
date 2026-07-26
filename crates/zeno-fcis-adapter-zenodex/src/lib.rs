//! Strict callable and canonical JSON-line adapters for mounted ZenoDEX decisions.
//!
//! JSON is a shell interchange format only. Protocol meaning remains the
//! canonical [`NormalizedDecision`] artifact and its ZCVE-derived component
//! bytes. Inputs are accepted only in one byte-identical JSON representation,
//! so duplicate fields, reordered fields, whitespace aliases, uppercase hex,
//! unknown fields, and trailing output fail closed.

#![forbid(unsafe_code)]

use core::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};

use serde::{Deserialize, Serialize};
use zeno_fcis_codec::{CanonicalEncode, Domain, EncodeError, Hash32, commitment};
use zeno_fcis_core::DecisionKind;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_refine::{
    DecisionArtifacts, Mismatch, NormalizedDecision, RefineError, RefinementReport, compare_exact,
};

const MAX_REASON_BYTES: usize = 96;

/// Explicit resource limits for one JSON-line decision exchange.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JsonLimits {
    /// Maximum request or response line length, including the newline.
    pub max_line_bytes: usize,
    /// Maximum decoded size of any one canonical artifact.
    pub max_artifact_bytes: usize,
}

impl Default for JsonLimits {
    fn default() -> Self {
        Self {
            max_line_bytes: 16 * 1024 * 1024,
            max_artifact_bytes: 8 * 1024 * 1024,
        }
    }
}

/// Runtime callable that already returns a complete normalized decision.
pub trait CallableRuntime<I> {
    /// Runtime-specific failure type.
    type Error: fmt::Display;

    /// Executes one bounded input without changing adapter authority.
    fn decide(&mut self, input: &I) -> Result<NormalizedDecision, Self::Error>;
}

/// Runtime that exchanges exactly one canonical JSON line.
pub trait JsonLineRuntime {
    /// Runtime-specific failure type.
    type Error: fmt::Display;

    /// Sends one line and returns the complete captured output.
    fn exchange(&mut self, request: &[u8]) -> Result<Vec<u8>, Self::Error>;
}

/// Executes a callable runtime and converts panics into a fail-closed crash.
pub fn invoke_callable<I, R>(runtime: &mut R, input: &I) -> Result<NormalizedDecision, AdapterError>
where
    R: CallableRuntime<I>,
{
    match catch_unwind(AssertUnwindSafe(|| runtime.decide(input))) {
        Ok(Ok(decision)) => Ok(decision),
        Ok(Err(error)) => Err(AdapterError::Runtime(error.to_string())),
        Err(_) => Err(AdapterError::RuntimeCrash),
    }
}

/// Executes a JSON-line runtime and strictly decodes its complete output.
pub fn invoke_json_line<R: JsonLineRuntime>(
    runtime: &mut R,
    request: &[u8],
    limits: JsonLimits,
) -> Result<NormalizedDecision, AdapterError> {
    validate_single_line(request, limits.max_line_bytes)?;
    let output = runtime
        .exchange(request)
        .map_err(|error| AdapterError::Runtime(error.to_string()))?;
    decode_decision_line(&output, limits)
}

/// Encodes one normalized decision in the sole accepted JSON-line form.
pub fn encode_decision_line(
    decision: &NormalizedDecision,
    limits: JsonLimits,
) -> Result<Vec<u8>, AdapterError> {
    let wire = WireDecision::from_decision(decision);
    let mut bytes = serde_json::to_vec(&wire).map_err(AdapterError::Json)?;
    bytes.push(b'\n');
    validate_single_line(&bytes, limits.max_line_bytes)?;
    Ok(bytes)
}

/// Decodes one byte-canonical JSON line into the complete refinement surface.
pub fn decode_decision_line(
    bytes: &[u8],
    limits: JsonLimits,
) -> Result<NormalizedDecision, AdapterError> {
    validate_single_line(bytes, limits.max_line_bytes)?;
    let payload = &bytes[..bytes.len() - 1];
    let wire: WireDecision = serde_json::from_slice(payload).map_err(AdapterError::Json)?;
    let mut canonical = serde_json::to_vec(&wire).map_err(AdapterError::Json)?;
    canonical.push(b'\n');
    if canonical != bytes {
        return Err(AdapterError::NonCanonicalJson);
    }
    wire.into_decision(limits)
}

/// Compares a mounted runtime decision against the model and binds any mismatch.
pub fn compare_case(
    case_id: Hash32,
    canonical_input: &[u8],
    model: &NormalizedDecision,
    runtime: &NormalizedDecision,
) -> Result<MountedCase, AdapterError> {
    let input_hash = hash_bytes("zeno-fcis/mounted-input", canonical_input)?;
    let report = compare_exact(model, runtime);
    let replay = if report.is_exact() {
        None
    } else {
        Some(ReplayFixture::new(
            case_id, input_hash, model, runtime, &report,
        )?)
    };
    Ok(MountedCase { report, replay })
}

/// Result of one exact mounted comparison.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MountedCase {
    report: RefinementReport,
    replay: Option<ReplayFixture>,
}

impl MountedCase {
    /// Returns the exact field-by-field comparison.
    #[must_use]
    pub const fn report(&self) -> &RefinementReport {
        &self.report
    }

    /// Returns the canonical counterexample when the comparison differs.
    #[must_use]
    pub const fn replay(&self) -> Option<&ReplayFixture> {
        self.replay.as_ref()
    }
}

/// Canonical replay identity for one complete mounted disagreement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayFixture {
    case_id: Hash32,
    input_hash: Hash32,
    model_hash: Hash32,
    runtime_hash: Hash32,
    mismatches: Box<[Mismatch]>,
}

impl ReplayFixture {
    fn new(
        case_id: Hash32,
        input_hash: Hash32,
        model: &NormalizedDecision,
        runtime: &NormalizedDecision,
        report: &RefinementReport,
    ) -> Result<Self, AdapterError> {
        Ok(Self {
            case_id,
            input_hash,
            model_hash: decision_commitment(model)?,
            runtime_hash: decision_commitment(runtime)?,
            mismatches: report.mismatches().to_vec().into_boxed_slice(),
        })
    }

    /// Returns the stable case identity.
    #[must_use]
    pub const fn case_id(&self) -> Hash32 {
        self.case_id
    }

    /// Returns the exact canonical input commitment.
    #[must_use]
    pub const fn input_hash(&self) -> Hash32 {
        self.input_hash
    }

    /// Returns the differing fields in comparison order.
    #[must_use]
    pub const fn mismatches(&self) -> &[Mismatch] {
        &self.mismatches
    }

    /// Returns a content commitment for persistence and deduplication.
    pub fn commitment(&self) -> Result<Hash32, AdapterError> {
        hash_bytes(
            "zeno-fcis/mounted-counterexample",
            &self.canonical_bytes().map_err(AdapterError::Encode)?,
        )
    }
}

impl CanonicalEncode for ReplayFixture {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(self.case_id.as_bytes());
        output.extend_from_slice(self.input_hash.as_bytes());
        output.extend_from_slice(self.model_hash.as_bytes());
        output.extend_from_slice(self.runtime_hash.as_bytes());
        let count =
            u16::try_from(self.mismatches.len()).map_err(|_| EncodeError::LengthOverflow)?;
        output.extend_from_slice(&count.to_be_bytes());
        for mismatch in &self.mismatches {
            output.push(mismatch_tag(*mismatch));
        }
        Ok(())
    }
}

/// Commits the complete normalized decision, including receipt and bundle bytes.
pub fn decision_commitment(decision: &NormalizedDecision) -> Result<Hash32, AdapterError> {
    let bytes = decision.canonical_bytes().map_err(AdapterError::Encode)?;
    hash_bytes("zeno-fcis/normalized-decision", &bytes)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct WireDecision {
    kind: String,
    reason_code: Option<String>,
    profile_hash: String,
    command_hash: String,
    context_hash: String,
    precedence_hash: String,
    algorithm_hash: String,
    budget_hash: String,
    pre_root: String,
    post_root: String,
    candidate_id: Option<String>,
    patch: Option<String>,
    commit_plan: Option<String>,
    outbox_plan: Option<String>,
    receipt: String,
    bundle: Option<String>,
}

impl WireDecision {
    fn from_decision(decision: &NormalizedDecision) -> Self {
        let artifacts = decision.artifacts();
        Self {
            kind: kind_label(artifacts.kind).to_owned(),
            reason_code: artifacts.reason_code.as_deref().map(str::to_owned),
            profile_hash: artifacts.profile_hash.to_string(),
            command_hash: artifacts.command_hash.to_string(),
            context_hash: artifacts.context_hash.to_string(),
            precedence_hash: artifacts.precedence_hash.to_string(),
            algorithm_hash: artifacts.algorithm_hash.to_string(),
            budget_hash: artifacts.budget_hash.to_string(),
            pre_root: artifacts.pre_root.to_string(),
            post_root: artifacts.post_root.to_string(),
            candidate_id: artifacts.candidate_id.map(|hash| hash.to_string()),
            patch: artifacts.patch_bytes.as_deref().map(encode_hex),
            commit_plan: artifacts.commit_plan_bytes.as_deref().map(encode_hex),
            outbox_plan: artifacts.outbox_plan_bytes.as_deref().map(encode_hex),
            receipt: encode_hex(&artifacts.receipt_bytes),
            bundle: artifacts.bundle_bytes.as_deref().map(encode_hex),
        }
    }

    fn into_decision(self, limits: JsonLimits) -> Result<NormalizedDecision, AdapterError> {
        if self.reason_code.as_ref().is_some_and(|reason| {
            reason.is_empty() || !reason.is_ascii() || reason.len() > MAX_REASON_BYTES
        }) {
            return Err(AdapterError::InvalidReason);
        }
        let artifacts = DecisionArtifacts {
            kind: parse_kind(&self.kind)?,
            reason_code: self.reason_code.map(String::into_boxed_str),
            profile_hash: parse_hash(&self.profile_hash)?,
            command_hash: parse_hash(&self.command_hash)?,
            context_hash: parse_hash(&self.context_hash)?,
            precedence_hash: parse_hash(&self.precedence_hash)?,
            algorithm_hash: parse_hash(&self.algorithm_hash)?,
            budget_hash: parse_hash(&self.budget_hash)?,
            pre_root: parse_hash(&self.pre_root)?,
            post_root: parse_hash(&self.post_root)?,
            candidate_id: self.candidate_id.as_deref().map(parse_hash).transpose()?,
            patch_bytes: decode_optional(self.patch, limits.max_artifact_bytes)?,
            commit_plan_bytes: decode_optional(self.commit_plan, limits.max_artifact_bytes)?,
            outbox_plan_bytes: decode_optional(self.outbox_plan, limits.max_artifact_bytes)?,
            receipt_bytes: decode_hex(&self.receipt, limits.max_artifact_bytes)?.into_boxed_slice(),
            bundle_bytes: decode_optional(self.bundle, limits.max_artifact_bytes)?,
        };
        NormalizedDecision::try_new(artifacts).map_err(AdapterError::Refine)
    }
}

fn validate_single_line(bytes: &[u8], maximum: usize) -> Result<(), AdapterError> {
    if bytes.is_empty() || bytes.len() > maximum {
        return Err(AdapterError::LineLength);
    }
    if bytes.last() != Some(&b'\n')
        || bytes[..bytes.len() - 1]
            .iter()
            .any(|byte| matches!(byte, b'\n' | b'\r'))
    {
        return Err(AdapterError::NotSingleLine);
    }
    Ok(())
}

fn parse_kind(value: &str) -> Result<DecisionKind, AdapterError> {
    match value {
        "accept" => Ok(DecisionKind::Accept),
        "reject" => Ok(DecisionKind::Reject),
        "committed_failure" => Ok(DecisionKind::CommittedFailure),
        _ => Err(AdapterError::UnknownDecisionKind),
    }
}

const fn kind_label(kind: DecisionKind) -> &'static str {
    match kind {
        DecisionKind::Accept => "accept",
        DecisionKind::Reject => "reject",
        DecisionKind::CommittedFailure => "committed_failure",
    }
}

fn parse_hash(value: &str) -> Result<Hash32, AdapterError> {
    let bytes = decode_hex(value, 32)?;
    let exact: [u8; 32] = bytes.try_into().map_err(|_| AdapterError::InvalidHash)?;
    Ok(Hash32::new(exact))
}

fn decode_optional(
    value: Option<String>,
    maximum: usize,
) -> Result<Option<Box<[u8]>>, AdapterError> {
    value
        .map(|text| decode_hex(&text, maximum).map(Vec::into_boxed_slice))
        .transpose()
}

fn decode_hex(value: &str, maximum: usize) -> Result<Vec<u8>, AdapterError> {
    if !value.len().is_multiple_of(2) || value.len() / 2 > maximum {
        return Err(AdapterError::ArtifactLength);
    }
    let mut output = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().chunks_exact(2) {
        let high = hex_nibble(pair[0]).ok_or(AdapterError::InvalidHex)?;
        let low = hex_nibble(pair[1]).ok_or(AdapterError::InvalidHex)?;
        output.push((high << 4) | low);
    }
    Ok(output)
}

const fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}

fn hash_bytes(domain: &'static str, bytes: &[u8]) -> Result<Hash32, AdapterError> {
    let domain = Domain::new(domain, 1).map_err(AdapterError::Encode)?;
    commitment::<RustCryptoSha256>(domain, bytes).map_err(AdapterError::Encode)
}

const fn mismatch_tag(mismatch: Mismatch) -> u8 {
    match mismatch {
        Mismatch::DecisionKind => 0,
        Mismatch::ReasonCode => 1,
        Mismatch::ProfileHash => 2,
        Mismatch::CommandHash => 3,
        Mismatch::ContextHash => 4,
        Mismatch::PrecedenceHash => 5,
        Mismatch::AlgorithmHash => 6,
        Mismatch::BudgetHash => 7,
        Mismatch::PreRoot => 8,
        Mismatch::PostRoot => 9,
        Mismatch::CandidateId => 10,
        Mismatch::PatchBytes => 11,
        Mismatch::CommitPlanBytes => 12,
        Mismatch::OutboxPlanBytes => 13,
        Mismatch::ReceiptBytes => 14,
        Mismatch::BundleBytes => 15,
    }
}

/// Strict mounted-adapter failure.
#[derive(Debug)]
pub enum AdapterError {
    /// Input or output exceeds the declared line budget.
    LineLength,
    /// Exchange is not exactly one LF-terminated line.
    NotSingleLine,
    /// JSON syntax, duplicate fields, missing fields, or unknown fields failed.
    Json(serde_json::Error),
    /// Parsed JSON has an alternate byte representation.
    NonCanonicalJson,
    /// Decision kind is outside the three-case algebra.
    UnknownDecisionKind,
    /// Stable reason is empty, non-ASCII, or oversized.
    InvalidReason,
    /// A commitment is not exactly 32 bytes.
    InvalidHash,
    /// Hex is not lowercase canonical hexadecimal.
    InvalidHex,
    /// An artifact exceeds its declared bound or has odd-length hex.
    ArtifactLength,
    /// The normalized decision violates its semantic shape.
    Refine(RefineError),
    /// Canonical encoding or commitment failed.
    Encode(EncodeError),
    /// Mounted runtime reported a failure such as timeout or tool disagreement.
    Runtime(String),
    /// Mounted callable panicked.
    RuntimeCrash,
}

impl fmt::Display for AdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LineLength => formatter.write_str("JSON line exceeds its resource bound"),
            Self::NotSingleLine => formatter.write_str("expected exactly one LF-terminated line"),
            Self::Json(error) => write!(formatter, "strict JSON decoding failed: {error}"),
            Self::NonCanonicalJson => formatter.write_str("JSON line is not byte-canonical"),
            Self::UnknownDecisionKind => formatter.write_str("unknown decision kind"),
            Self::InvalidReason => formatter.write_str("invalid stable reason code"),
            Self::InvalidHash => formatter.write_str("invalid 32-byte commitment"),
            Self::InvalidHex => formatter.write_str("invalid lowercase hexadecimal"),
            Self::ArtifactLength => formatter.write_str("artifact length exceeds its bound"),
            Self::Refine(error) => write!(formatter, "normalized decision rejected: {error}"),
            Self::Encode(error) => write!(formatter, "canonical encoding failed: {error}"),
            Self::Runtime(error) => write!(formatter, "mounted runtime failed: {error}"),
            Self::RuntimeCrash => formatter.write_str("mounted runtime crashed"),
        }
    }
}

impl std::error::Error for AdapterError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(byte: u8) -> Hash32 {
        Hash32::new([byte; 32])
    }

    fn reject(reason: &str) -> NormalizedDecision {
        NormalizedDecision::try_new(DecisionArtifacts {
            kind: DecisionKind::Reject,
            reason_code: Some(reason.into()),
            profile_hash: hash(1),
            command_hash: hash(2),
            context_hash: hash(3),
            precedence_hash: hash(4),
            algorithm_hash: hash(5),
            budget_hash: hash(6),
            pre_root: hash(7),
            post_root: hash(7),
            candidate_id: None,
            patch_bytes: None,
            commit_plan_bytes: None,
            outbox_plan_bytes: None,
            receipt_bytes: vec![8].into_boxed_slice(),
            bundle_bytes: None,
        })
        .unwrap_or_else(|error| panic!("fixture: {error}"))
    }

    #[test]
    fn canonical_line_round_trips_exactly() {
        let decision = reject("mint_exceeds_cap");
        let bytes = encode_decision_line(&decision, JsonLimits::default())
            .unwrap_or_else(|error| panic!("encode: {error}"));
        let decoded = decode_decision_line(&bytes, JsonLimits::default())
            .unwrap_or_else(|error| panic!("decode: {error}"));
        assert_eq!(decoded, decision);
        let encoded = encode_decision_line(&decoded, JsonLimits::default())
            .unwrap_or_else(|error| panic!("re-encode: {error}"));
        assert_eq!(encoded, bytes);
    }

    #[test]
    fn whitespace_alias_fails_closed() {
        let mut bytes = encode_decision_line(&reject("reason"), JsonLimits::default())
            .unwrap_or_else(|error| panic!("encode: {error}"));
        bytes.insert(1, b' ');
        assert!(matches!(
            decode_decision_line(&bytes, JsonLimits::default()),
            Err(AdapterError::NonCanonicalJson)
        ));
    }

    #[test]
    fn unknown_field_fails_closed() {
        let bytes = encode_decision_line(&reject("reason"), JsonLimits::default())
            .unwrap_or_else(|error| panic!("encode: {error}"));
        let text = String::from_utf8(bytes).unwrap_or_else(|error| panic!("utf8: {error}"));
        let hostile = text.replacen('{', "{\"extra\":null,", 1);
        assert!(matches!(
            decode_decision_line(hostile.as_bytes(), JsonLimits::default()),
            Err(AdapterError::Json(_))
        ));
    }

    #[test]
    fn mismatch_produces_content_bound_replay() {
        let model = reject("first");
        let runtime = reject("second");
        let mounted = compare_case(hash(20), b"canonical-input", &model, &runtime)
            .unwrap_or_else(|error| panic!("compare: {error}"));
        assert!(!mounted.report().is_exact());
        let replay = mounted.replay().unwrap_or_else(|| panic!("missing replay"));
        assert_eq!(replay.mismatches(), &[Mismatch::ReasonCode]);
        assert_ne!(
            replay
                .commitment()
                .unwrap_or_else(|error| panic!("hash: {error}")),
            Hash32::ZERO
        );
    }

    #[test]
    fn callable_panic_is_a_crash() {
        struct Panics;
        impl CallableRuntime<()> for Panics {
            type Error = AdapterError;
            fn decide(&mut self, (): &()) -> Result<NormalizedDecision, Self::Error> {
                panic!("boom")
            }
        }
        assert!(matches!(
            invoke_callable(&mut Panics, &()),
            Err(AdapterError::RuntimeCrash)
        ));
    }
}
