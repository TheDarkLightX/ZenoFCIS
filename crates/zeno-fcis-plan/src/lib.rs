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

use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher, Domain, EncodeError, Hash32, commitment};
use zeno_fcis_value::Value;

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

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

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
}
