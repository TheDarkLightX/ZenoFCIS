//! Closed data-only plans for authoritative commit and external delivery.
//!
//! Plans contain no closures, function pointers, trait objects, endpoints, or
//! ambient runtime handles. The shell interprets a closed operation registry.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt;

use zeno_fcis_codec::{
    CanonicalEncode, CommitmentHasher, DecodeError, DecodeLimits, Domain, EncodeError, Hash32,
    commitment, decode_value,
};
use zeno_fcis_value::Value;

/// Explicit resource bounds for strict canonical plan decoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlanDecodeLimits {
    /// Maximum bytes in one complete encoded plan.
    pub max_input_bytes: u64,
    /// Maximum authoritative effects in one commit plan.
    pub max_effects: u32,
    /// Maximum delivery obligations in one outbox plan.
    pub max_outbox_entries: u32,
    /// Maximum aggregate value nodes decoded across the complete plan.
    pub max_value_nodes: u64,
    /// Maximum aggregate byte and text payload bytes decoded across the complete plan.
    pub max_value_payload_bytes: u64,
    /// Per-value ZCVE decoding limits.
    pub value: DecodeLimits,
}

impl Default for PlanDecodeLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: DecodeLimits::DEFAULT_MAX_INPUT_BYTES,
            max_effects: 4_096,
            max_outbox_entries: 4_096,
            max_value_nodes: 1_000_000,
            max_value_payload_bytes: 64 * 1024 * 1024,
            value: DecodeLimits::default(),
        }
    }
}

/// One authoritative operation planned by the semantic core.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Effect {
    ordinal: u32,
    operation: u32,
    authority: Hash32,
    subject: Hash32,
    payload: Value,
}

impl Effect {
    /// Creates a closed authoritative operation.
    #[must_use]
    pub const fn new(
        ordinal: u32,
        operation: u32,
        authority: Hash32,
        subject: Hash32,
        payload: Value,
    ) -> Self {
        Self {
            ordinal,
            operation,
            authority,
            subject,
            payload,
        }
    }

    /// Returns the canonical operation ordinal.
    #[must_use]
    pub const fn ordinal(&self) -> u32 {
        self.ordinal
    }

    /// Returns the closed operation identifier.
    #[must_use]
    pub const fn operation(&self) -> u32 {
        self.operation
    }

    /// Returns the authority-domain commitment.
    #[must_use]
    pub const fn authority(&self) -> Hash32 {
        self.authority
    }

    /// Returns the bound subject commitment.
    #[must_use]
    pub const fn subject(&self) -> Hash32 {
        self.subject
    }

    /// Returns the immutable operation payload.
    #[must_use]
    pub const fn payload(&self) -> &Value {
        &self.payload
    }
}

impl CanonicalEncode for Effect {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.ordinal.to_be_bytes());
        output.extend_from_slice(&self.operation.to_be_bytes());
        output.extend_from_slice(self.authority.as_bytes());
        output.extend_from_slice(self.subject.as_bytes());
        put_blob(output, &self.payload.canonical_bytes()?)
    }
}

/// Canonically ordered authoritative operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitPlan {
    effects: Box<[Effect]>,
}

impl CommitPlan {
    /// Sorts by ordinal and rejects duplicate ordinals.
    pub fn try_new(mut effects: Vec<Effect>) -> Result<Self, PlanError> {
        effects.sort_by_key(Effect::ordinal);
        ensure_unique_effects(&effects)?;
        Ok(Self {
            effects: effects.into_boxed_slice(),
        })
    }

    /// Returns an empty plan.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            effects: Vec::new().into_boxed_slice(),
        }
    }

    /// Returns canonical effects.
    #[must_use]
    pub fn effects(&self) -> &[Effect] {
        &self.effects
    }
}

impl CanonicalEncode for CommitPlan {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        put_length(output, self.effects.len())?;
        for effect in &self.effects {
            put_blob(output, &effect.canonical_bytes()?)?;
        }
        Ok(())
    }
}

/// One external-delivery obligation committed as data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutboxEntry {
    ordinal: u32,
    channel: u32,
    destination: Value,
    payload: Value,
}

impl OutboxEntry {
    /// Creates an outbox entry.
    #[must_use]
    pub const fn new(ordinal: u32, channel: u32, destination: Value, payload: Value) -> Self {
        Self {
            ordinal,
            channel,
            destination,
            payload,
        }
    }

    /// Returns the canonical ordinal.
    #[must_use]
    pub const fn ordinal(&self) -> u32 {
        self.ordinal
    }

    /// Returns the closed channel identifier.
    #[must_use]
    pub const fn channel(&self) -> u32 {
        self.channel
    }

    /// Returns the destination value.
    #[must_use]
    pub const fn destination(&self) -> &Value {
        &self.destination
    }

    /// Returns the payload value.
    #[must_use]
    pub const fn payload(&self) -> &Value {
        &self.payload
    }

    /// Derives an idempotent delivery identity from candidate and entry content.
    pub fn delivery_id<H: CommitmentHasher>(
        &self,
        candidate_id: Hash32,
    ) -> Result<Hash32, EncodeError> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(candidate_id.as_bytes());
        bytes.extend_from_slice(&self.canonical_bytes()?);
        let domain = Domain::new("zeno-fcis/delivery", 1)?;
        commitment::<H>(domain, &bytes)
    }
}

impl CanonicalEncode for OutboxEntry {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.ordinal.to_be_bytes());
        output.extend_from_slice(&self.channel.to_be_bytes());
        put_blob(output, &self.destination.canonical_bytes()?)?;
        put_blob(output, &self.payload.canonical_bytes()?)
    }
}

/// Canonically ordered external-delivery obligations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutboxPlan {
    entries: Box<[OutboxEntry]>,
}

impl OutboxPlan {
    /// Sorts by ordinal and rejects duplicate ordinals.
    pub fn try_new(mut entries: Vec<OutboxEntry>) -> Result<Self, PlanError> {
        entries.sort_by_key(OutboxEntry::ordinal);
        for pair in entries.windows(2) {
            if pair[0].ordinal == pair[1].ordinal {
                return Err(PlanError::DuplicateOutboxOrdinal(pair[0].ordinal));
            }
        }
        Ok(Self {
            entries: entries.into_boxed_slice(),
        })
    }

    /// Returns an empty outbox plan.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            entries: Vec::new().into_boxed_slice(),
        }
    }

    /// Returns canonical entries.
    #[must_use]
    pub fn entries(&self) -> &[OutboxEntry] {
        &self.entries
    }
}

impl CanonicalEncode for OutboxPlan {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        put_length(output, self.entries.len())?;
        for entry in &self.entries {
            put_blob(output, &entry.canonical_bytes()?)?;
        }
        Ok(())
    }
}

/// Strictly decodes one canonical commit plan under explicit resource bounds.
///
/// Every effect payload is admitted through the ZCVE/1 decoder. The decoded
/// effects are reconstructed through [`CommitPlan::try_new`], and the resulting
/// canonical bytes must equal the complete input. Duplicate ordinals, alternate
/// effect order, malformed nested values, truncation, and trailing bytes fail
/// closed.
pub fn decode_commit_plan(
    bytes: &[u8],
    limits: PlanDecodeLimits,
) -> Result<CommitPlan, PlanDecodeError> {
    enforce_plan_input_limit(bytes, limits)?;
    let mut cursor = PlanCursor::new(bytes);
    let effect_count = cursor.take_u32()?;
    if effect_count > limits.max_effects {
        return Err(PlanDecodeError::EffectLimit {
            limit: limits.max_effects,
            actual: effect_count,
        });
    }
    let capacity = usize::try_from(effect_count).map_err(|_| PlanDecodeError::LengthOverflow)?;
    let mut state = PlanDecodeState::default();
    let mut effects = Vec::with_capacity(capacity);
    for _ in 0..effect_count {
        effects.push(decode_effect(cursor.take_blob()?, limits, &mut state)?);
    }
    ensure_plan_consumed(&cursor)?;

    let plan = CommitPlan::try_new(effects).map_err(PlanDecodeError::Plan)?;
    ensure_canonical_plan(bytes, &plan)?;
    Ok(plan)
}

/// Strictly decodes one canonical outbox plan under explicit resource bounds.
///
/// Every destination and payload is admitted through the ZCVE/1 decoder. The
/// decoded entries are reconstructed through [`OutboxPlan::try_new`], and the
/// resulting canonical bytes must equal the complete input. Duplicate ordinals,
/// alternate entry order, malformed nested values, truncation, and trailing
/// bytes fail closed.
pub fn decode_outbox_plan(
    bytes: &[u8],
    limits: PlanDecodeLimits,
) -> Result<OutboxPlan, PlanDecodeError> {
    enforce_plan_input_limit(bytes, limits)?;
    let mut cursor = PlanCursor::new(bytes);
    let entry_count = cursor.take_u32()?;
    if entry_count > limits.max_outbox_entries {
        return Err(PlanDecodeError::OutboxEntryLimit {
            limit: limits.max_outbox_entries,
            actual: entry_count,
        });
    }
    let capacity = usize::try_from(entry_count).map_err(|_| PlanDecodeError::LengthOverflow)?;
    let mut state = PlanDecodeState::default();
    let mut entries = Vec::with_capacity(capacity);
    for _ in 0..entry_count {
        entries.push(decode_outbox_entry(
            cursor.take_blob()?,
            limits,
            &mut state,
        )?);
    }
    ensure_plan_consumed(&cursor)?;

    let plan = OutboxPlan::try_new(entries).map_err(PlanDecodeError::Plan)?;
    ensure_canonical_plan(bytes, &plan)?;
    Ok(plan)
}

fn enforce_plan_input_limit(bytes: &[u8], limits: PlanDecodeLimits) -> Result<(), PlanDecodeError> {
    let actual = u64::try_from(bytes.len()).map_err(|_| PlanDecodeError::LengthOverflow)?;
    if actual > limits.max_input_bytes {
        return Err(PlanDecodeError::InputLimit {
            limit: limits.max_input_bytes,
            actual,
        });
    }
    Ok(())
}

fn ensure_plan_consumed(cursor: &PlanCursor<'_>) -> Result<(), PlanDecodeError> {
    if cursor.remaining() != 0 {
        return Err(PlanDecodeError::TrailingBytes {
            offset: cursor.offset,
        });
    }
    Ok(())
}

fn ensure_canonical_plan<T: CanonicalEncode>(
    input: &[u8],
    plan: &T,
) -> Result<(), PlanDecodeError> {
    let encoded = plan.canonical_bytes().map_err(PlanDecodeError::Encode)?;
    if encoded.as_slice() != input {
        return Err(PlanDecodeError::NonCanonical);
    }
    Ok(())
}

fn decode_effect(
    bytes: &[u8],
    limits: PlanDecodeLimits,
    state: &mut PlanDecodeState,
) -> Result<Effect, PlanDecodeError> {
    let mut cursor = PlanCursor::new(bytes);
    let ordinal = cursor.take_u32()?;
    let operation = cursor.take_u32()?;
    let authority = cursor.take_hash32()?;
    let subject = cursor.take_hash32()?;
    let payload = decode_plan_value(cursor.take_blob()?, limits, state)?;
    ensure_plan_consumed(&cursor)?;
    Ok(Effect::new(ordinal, operation, authority, subject, payload))
}

fn decode_outbox_entry(
    bytes: &[u8],
    limits: PlanDecodeLimits,
    state: &mut PlanDecodeState,
) -> Result<OutboxEntry, PlanDecodeError> {
    let mut cursor = PlanCursor::new(bytes);
    let ordinal = cursor.take_u32()?;
    let channel = cursor.take_u32()?;
    let destination = decode_plan_value(cursor.take_blob()?, limits, state)?;
    let payload = decode_plan_value(cursor.take_blob()?, limits, state)?;
    ensure_plan_consumed(&cursor)?;
    Ok(OutboxEntry::new(ordinal, channel, destination, payload))
}

fn decode_plan_value(
    bytes: &[u8],
    limits: PlanDecodeLimits,
    state: &mut PlanDecodeState,
) -> Result<Value, PlanDecodeError> {
    let value = decode_value(bytes, limits.value).map_err(PlanDecodeError::Value)?;
    let metrics = value
        .validate_limits(limits.value.value)
        .map_err(|error| PlanDecodeError::Value(DecodeError::InvalidValue(error)))?;
    state.value_nodes = state
        .value_nodes
        .checked_add(metrics.nodes)
        .ok_or(PlanDecodeError::LengthOverflow)?;
    if state.value_nodes > limits.max_value_nodes {
        return Err(PlanDecodeError::ValueNodeLimit {
            limit: limits.max_value_nodes,
            actual: state.value_nodes,
        });
    }
    state.value_payload_bytes = state
        .value_payload_bytes
        .checked_add(metrics.payload_bytes)
        .ok_or(PlanDecodeError::LengthOverflow)?;
    if state.value_payload_bytes > limits.max_value_payload_bytes {
        return Err(PlanDecodeError::ValuePayloadLimit {
            limit: limits.max_value_payload_bytes,
            actual: state.value_payload_bytes,
        });
    }
    Ok(value)
}

#[derive(Default)]
struct PlanDecodeState {
    value_nodes: u64,
    value_payload_bytes: u64,
}

struct PlanCursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> PlanCursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], PlanDecodeError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or(PlanDecodeError::LengthOverflow)?;
        let Some(bytes) = self.bytes.get(self.offset..end) else {
            return Err(PlanDecodeError::UnexpectedEnd {
                offset: self.offset,
                requested: count,
            });
        };
        self.offset = end;
        Ok(bytes)
    }

    fn take_u32(&mut self) -> Result<u32, PlanDecodeError> {
        let mut bytes = [0_u8; 4];
        bytes.copy_from_slice(self.take(4)?);
        Ok(u32::from_be_bytes(bytes))
    }

    fn take_hash32(&mut self) -> Result<Hash32, PlanDecodeError> {
        let mut bytes = [0_u8; 32];
        bytes.copy_from_slice(self.take(32)?);
        Ok(Hash32::new(bytes))
    }

    fn take_blob(&mut self) -> Result<&'a [u8], PlanDecodeError> {
        let length = self.take_u32()?;
        let length = usize::try_from(length).map_err(|_| PlanDecodeError::LengthOverflow)?;
        self.take(length)
    }
}

fn ensure_unique_effects(effects: &[Effect]) -> Result<(), PlanError> {
    for pair in effects.windows(2) {
        if pair[0].ordinal == pair[1].ordinal {
            return Err(PlanError::DuplicateEffectOrdinal(pair[0].ordinal));
        }
    }
    Ok(())
}

fn put_length(output: &mut Vec<u8>, length: usize) -> Result<(), EncodeError> {
    let length = u32::try_from(length).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    Ok(())
}

fn put_blob(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), EncodeError> {
    put_length(output, bytes.len())?;
    output.extend_from_slice(bytes);
    Ok(())
}

/// Closed-plan construction failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanError {
    /// Two authoritative effects share one ordinal.
    DuplicateEffectOrdinal(u32),
    /// Two outbox entries share one ordinal.
    DuplicateOutboxOrdinal(u32),
}

/// Strict canonical plan decoding failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlanDecodeError {
    /// Complete input exceeds the declared byte limit.
    InputLimit {
        /// Configured limit.
        limit: u64,
        /// Actual input bytes.
        actual: u64,
    },
    /// A commit plan declares too many effects.
    EffectLimit {
        /// Configured limit.
        limit: u32,
        /// Declared effect count.
        actual: u32,
    },
    /// An outbox plan declares too many entries.
    OutboxEntryLimit {
        /// Configured limit.
        limit: u32,
        /// Declared entry count.
        actual: u32,
    },
    /// Aggregate decoded value nodes exceed their limit.
    ValueNodeLimit {
        /// Configured limit.
        limit: u64,
        /// Attempted aggregate nodes.
        actual: u64,
    },
    /// Aggregate decoded value payload bytes exceed their limit.
    ValuePayloadLimit {
        /// Configured limit.
        limit: u64,
        /// Attempted aggregate payload bytes.
        actual: u64,
    },
    /// A length conversion or counter overflowed.
    LengthOverflow,
    /// Input ended before the declared item was complete.
    UnexpectedEnd {
        /// Byte offset where decoding stopped.
        offset: usize,
        /// Requested byte count.
        requested: usize,
    },
    /// Bytes remained after a complete item.
    TrailingBytes {
        /// First trailing byte offset within the current item.
        offset: usize,
    },
    /// A nested ZCVE value failed strict decoding.
    Value(DecodeError),
    /// Reconstructed plan invariants failed.
    Plan(PlanError),
    /// Canonical re-encoding failed.
    Encode(EncodeError),
    /// Reconstructed canonical bytes differ from the complete input.
    NonCanonical,
}

impl fmt::Display for PlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateEffectOrdinal(ordinal) => {
                write!(formatter, "duplicate effect ordinal {ordinal}")
            }
            Self::DuplicateOutboxOrdinal(ordinal) => {
                write!(formatter, "duplicate outbox ordinal {ordinal}")
            }
        }
    }
}

impl fmt::Display for PlanDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputLimit { limit, actual } => {
                write!(formatter, "plan input bytes {actual} exceeds limit {limit}")
            }
            Self::EffectLimit { limit, actual } => {
                write!(formatter, "plan effects {actual} exceeds limit {limit}")
            }
            Self::OutboxEntryLimit { limit, actual } => {
                write!(formatter, "outbox entries {actual} exceeds limit {limit}")
            }
            Self::ValueNodeLimit { limit, actual } => {
                write!(
                    formatter,
                    "decoded plan value nodes {actual} exceeds limit {limit}"
                )
            }
            Self::ValuePayloadLimit { limit, actual } => write!(
                formatter,
                "decoded plan value payload bytes {actual} exceeds limit {limit}"
            ),
            Self::LengthOverflow => formatter.write_str("plan decode length overflow"),
            Self::UnexpectedEnd { offset, requested } => write!(
                formatter,
                "plan input ended at offset {offset} before {requested} bytes"
            ),
            Self::TrailingBytes { offset } => {
                write!(formatter, "trailing plan bytes at offset {offset}")
            }
            Self::Value(error) => error.fmt(formatter),
            Self::Plan(error) => error.fmt(formatter),
            Self::Encode(error) => error.fmt(formatter),
            Self::NonCanonical => formatter.write_str("noncanonical plan encoding"),
        }
    }
}

impl core::error::Error for PlanDecodeError {}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn effect(ordinal: u32, payload: Value) -> Effect {
        Effect::new(ordinal, 10, Hash32::ZERO, Hash32::ZERO, payload)
    }

    fn outbox_entry(ordinal: u32, destination: Value, payload: Value) -> OutboxEntry {
        OutboxEntry::new(ordinal, 20, destination, payload)
    }

    fn encode_items<T: CanonicalEncode>(items: &[T]) -> Vec<u8> {
        let encoded = items
            .iter()
            .map(|item| match item.canonical_bytes() {
                Ok(encoded) => encoded,
                Err(error) => panic!("test item encoding: {error}"),
            })
            .collect::<Vec<_>>();
        encode_raw_items(&encoded)
    }

    fn encode_raw_items(items: &[Vec<u8>]) -> Vec<u8> {
        let mut bytes = Vec::new();
        let count = match u32::try_from(items.len()) {
            Ok(count) => count,
            Err(error) => panic!("test item count: {error}"),
        };
        bytes.extend_from_slice(&count.to_be_bytes());
        for encoded in items {
            let length = match u32::try_from(encoded.len()) {
                Ok(length) => length,
                Err(error) => panic!("test item length: {error}"),
            };
            bytes.extend_from_slice(&length.to_be_bytes());
            bytes.extend_from_slice(encoded);
        }
        bytes
    }

    #[test]
    fn effect_order_is_canonical() {
        let effects = vec![
            Effect::new(2, 10, Hash32::ZERO, Hash32::ZERO, Value::U128(2)),
            Effect::new(1, 10, Hash32::ZERO, Hash32::ZERO, Value::U128(1)),
        ];
        let plan = CommitPlan::try_new(effects);
        assert!(plan.is_ok());
        let plan = match plan {
            Ok(plan) => plan,
            Err(error) => panic!("unexpected plan error: {error}"),
        };
        assert_eq!(plan.effects()[0].ordinal(), 1);
    }

    #[test]
    fn duplicate_outbox_ordinals_fail_closed() {
        let entries = vec![
            OutboxEntry::new(1, 1, Value::Unit, Value::Bool(true)),
            OutboxEntry::new(1, 2, Value::Unit, Value::Bool(false)),
        ];
        assert_eq!(
            OutboxPlan::try_new(entries),
            Err(PlanError::DuplicateOutboxOrdinal(1))
        );
    }

    #[test]
    fn strict_decoders_round_trip_complete_canonical_plans() {
        let commit = match CommitPlan::try_new(vec![
            effect(2, Value::Bool(true)),
            effect(1, Value::U128(7)),
        ]) {
            Ok(plan) => plan,
            Err(error) => panic!("commit plan: {error}"),
        };
        let outbox = match OutboxPlan::try_new(vec![
            outbox_entry(2, Value::U128(9), Value::Bool(false)),
            outbox_entry(1, Value::Unit, Value::Bool(true)),
        ]) {
            Ok(plan) => plan,
            Err(error) => panic!("outbox plan: {error}"),
        };
        let commit_bytes = match commit.canonical_bytes() {
            Ok(bytes) => bytes,
            Err(error) => panic!("commit bytes: {error}"),
        };
        let outbox_bytes = match outbox.canonical_bytes() {
            Ok(bytes) => bytes,
            Err(error) => panic!("outbox bytes: {error}"),
        };

        assert_eq!(
            decode_commit_plan(&commit_bytes, PlanDecodeLimits::default()),
            Ok(commit)
        );
        assert_eq!(
            decode_outbox_plan(&outbox_bytes, PlanDecodeLimits::default()),
            Ok(outbox)
        );
        assert_eq!(
            decode_commit_plan(&4_097_u32.to_be_bytes(), PlanDecodeLimits::default()),
            Err(PlanDecodeError::EffectLimit {
                limit: 4_096,
                actual: 4_097,
            })
        );
        assert_eq!(
            decode_commit_plan(&0_u32.to_be_bytes(), PlanDecodeLimits::default()),
            Ok(CommitPlan::empty())
        );
        assert_eq!(
            decode_outbox_plan(&0_u32.to_be_bytes(), PlanDecodeLimits::default()),
            Ok(OutboxPlan::empty())
        );
    }

    #[test]
    fn input_and_cardinality_limits_are_exact() {
        let commit_bytes = encode_items(&[effect(1, Value::Unit)]);
        let outbox_bytes = encode_items(&[outbox_entry(1, Value::Unit, Value::Bool(true))]);
        let commit_length = match u64::try_from(commit_bytes.len()) {
            Ok(length) => length,
            Err(error) => panic!("commit length: {error}"),
        };

        let exact = PlanDecodeLimits {
            max_input_bytes: commit_length,
            max_effects: 1,
            ..PlanDecodeLimits::default()
        };
        assert!(decode_commit_plan(&commit_bytes, exact).is_ok());
        assert_eq!(
            decode_commit_plan(
                &commit_bytes,
                PlanDecodeLimits {
                    max_input_bytes: commit_length - 1,
                    ..exact
                }
            ),
            Err(PlanDecodeError::InputLimit {
                limit: commit_length - 1,
                actual: commit_length,
            })
        );
        assert_eq!(
            decode_commit_plan(
                &commit_bytes,
                PlanDecodeLimits {
                    max_effects: 0,
                    ..exact
                }
            ),
            Err(PlanDecodeError::EffectLimit {
                limit: 0,
                actual: 1,
            })
        );

        let outbox_exact = PlanDecodeLimits {
            max_outbox_entries: 1,
            ..PlanDecodeLimits::default()
        };
        assert!(decode_outbox_plan(&outbox_bytes, outbox_exact).is_ok());
        assert_eq!(
            decode_outbox_plan(
                &outbox_bytes,
                PlanDecodeLimits {
                    max_outbox_entries: 0,
                    ..outbox_exact
                }
            ),
            Err(PlanDecodeError::OutboxEntryLimit {
                limit: 0,
                actual: 1,
            })
        );
    }

    #[test]
    fn aggregate_value_limits_cover_complete_outbox_plans() {
        let bytes = encode_items(&[outbox_entry(
            1,
            Value::Bytes(vec![1_u8, 2].into_boxed_slice()),
            Value::Bytes(vec![3_u8, 4, 5].into_boxed_slice()),
        )]);
        let exact = PlanDecodeLimits {
            max_value_nodes: 2,
            max_value_payload_bytes: 5,
            ..PlanDecodeLimits::default()
        };
        assert!(decode_outbox_plan(&bytes, exact).is_ok());
        assert_eq!(
            decode_outbox_plan(
                &bytes,
                PlanDecodeLimits {
                    max_value_nodes: 1,
                    ..exact
                }
            ),
            Err(PlanDecodeError::ValueNodeLimit {
                limit: 1,
                actual: 2,
            })
        );
        assert_eq!(
            decode_outbox_plan(
                &bytes,
                PlanDecodeLimits {
                    max_value_payload_bytes: 4,
                    ..exact
                }
            ),
            Err(PlanDecodeError::ValuePayloadLimit {
                limit: 4,
                actual: 5,
            })
        );
    }

    #[test]
    fn nested_value_limits_propagate_without_partial_plans() {
        let bytes = encode_items(&[effect(
            1,
            Value::Vector(vec![Value::Unit, Value::Unit].into_boxed_slice()),
        )]);
        let mut limits = PlanDecodeLimits::default();
        limits.value.value.max_collection_len = 1;
        assert_eq!(
            decode_commit_plan(&bytes, limits),
            Err(PlanDecodeError::Value(DecodeError::CollectionLimit {
                limit: 1,
                attempted: 2,
            }))
        );

        let unit_bytes = encode_items(&[effect(1, Value::Unit)]);
        limits = PlanDecodeLimits::default();
        limits.value.max_input_bytes = 1;
        assert!(decode_commit_plan(&unit_bytes, limits).is_ok());
        limits.value.max_input_bytes = 0;
        assert_eq!(
            decode_commit_plan(&unit_bytes, limits),
            Err(PlanDecodeError::Value(DecodeError::InputLimit {
                limit: 0,
                actual: 1,
            }))
        );
    }

    #[test]
    fn alternate_item_order_is_rejected_for_both_plan_kinds() {
        let commit = encode_items(&[effect(2, Value::Bool(false)), effect(1, Value::Bool(true))]);
        let outbox = encode_items(&[
            outbox_entry(2, Value::Unit, Value::Bool(false)),
            outbox_entry(1, Value::Unit, Value::Bool(true)),
        ]);
        assert_eq!(
            decode_commit_plan(&commit, PlanDecodeLimits::default()),
            Err(PlanDecodeError::NonCanonical)
        );
        assert_eq!(
            decode_outbox_plan(&outbox, PlanDecodeLimits::default()),
            Err(PlanDecodeError::NonCanonical)
        );
    }

    #[test]
    fn duplicate_ordinals_are_rejected_during_reconstruction() {
        let commit = encode_items(&[effect(1, Value::Unit), effect(1, Value::Bool(true))]);
        let outbox = encode_items(&[
            outbox_entry(1, Value::Unit, Value::Bool(false)),
            outbox_entry(1, Value::Unit, Value::Bool(true)),
        ]);
        assert_eq!(
            decode_commit_plan(&commit, PlanDecodeLimits::default()),
            Err(PlanDecodeError::Plan(PlanError::DuplicateEffectOrdinal(1)))
        );
        assert_eq!(
            decode_outbox_plan(&outbox, PlanDecodeLimits::default()),
            Err(PlanDecodeError::Plan(PlanError::DuplicateOutboxOrdinal(1)))
        );
    }

    #[test]
    fn malformed_nested_and_item_bytes_fail_closed() {
        let mut encoded_effect = match effect(1, Value::Unit).canonical_bytes() {
            Ok(bytes) => bytes,
            Err(error) => panic!("effect bytes: {error}"),
        };
        encoded_effect.push(0);
        let nested_trailing = encode_raw_items(&[encoded_effect]);
        assert!(matches!(
            decode_commit_plan(&nested_trailing, PlanDecodeLimits::default()),
            Err(PlanDecodeError::TrailingBytes { .. })
        ));

        let mut invalid_value_effect = match effect(1, Value::Unit).canonical_bytes() {
            Ok(bytes) => bytes,
            Err(error) => panic!("effect bytes: {error}"),
        };
        let payload_offset = 4 + 4 + 32 + 32 + 4;
        invalid_value_effect[payload_offset] = u8::MAX;
        let invalid_value = encode_raw_items(&[invalid_value_effect]);
        assert!(matches!(
            decode_commit_plan(&invalid_value, PlanDecodeLimits::default()),
            Err(PlanDecodeError::Value(DecodeError::UnknownTag(u8::MAX)))
        ));
    }

    #[test]
    fn top_level_trailing_and_truncated_inputs_fail_closed() {
        let bytes = encode_items(&[effect(1, Value::Unit)]);
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(matches!(
            decode_commit_plan(&trailing, PlanDecodeLimits::default()),
            Err(PlanDecodeError::TrailingBytes { .. })
        ));

        let mut truncated = bytes;
        let _ = truncated.pop();
        assert!(matches!(
            decode_commit_plan(&truncated, PlanDecodeLimits::default()),
            Err(PlanDecodeError::UnexpectedEnd { .. })
        ));
    }
}
