//! Profile-bound FIFO state, not a transport or schema validator.
//!
//! A profile identifier is an equality binding only. Payloads are admitted by
//! the existing default Value structural limits and the byte/count policy.
//! Successful transitions confer no publication, delivery or commit authority.
use crate::bounded::{BoundedError, CollectionLimits, add, blob, check, header, value_size};
use alloc::vec::Vec;
use zeno_fcis_value::Value;

/// Supplied profile identity. Neither field authenticates or validates a schema.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PipeProfile {
    /// Application-supplied domain identity.
    pub domain: [u8; 32],
    /// Application-supplied schema identity.
    pub schema: [u8; 32],
}
/// Immutable message proposal. IDs are supplied, never derived from a clock.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PipeMessage {
    profile: PipeProfile,
    id: [u8; 32],
    payload: Value,
}
impl PipeMessage {
    /// Constructs a proposal, not an admitted or authorized delivery.
    pub fn new(profile: PipeProfile, id: [u8; 32], payload: Value) -> Self {
        Self {
            profile,
            id,
            payload,
        }
    }
    /// Supplied identity of this proposal.
    pub fn profile(&self) -> PipeProfile {
        self.profile
    }
    /// Stable caller-supplied ID, scoped to the pipe profile.
    pub fn id(&self) -> &[u8; 32] {
        &self.id
    }
    /// Immutable original payload.
    pub fn payload(&self) -> &Value {
        &self.payload
    }
}
/// Pipe refusal precedence is documented on each transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PipeError {
    /// Domain or schema identity differs from this pipe.
    ProfileMismatch,
    /// A pending ID already names a different payload.
    IdConflict,
    /// Acknowledgment does not exactly match the current head.
    HeadMismatch,
    /// Collection admission or empty-head refusal.
    Bounds(BoundedError),
}
impl From<BoundedError> for PipeError {
    fn from(e: BoundedError) -> Self {
        Self::Bounds(e)
    }
}
/// Bounded immutable FIFO with pending-ID idempotency and exact head acknowledgment.
///
/// Pending messages retain insertion order. Retries read the same head without
/// modification; there is no attempt counter, timer, reordering or auto-removal.
/// Acknowledgment frees the ID: replay after acknowledgment may enqueue again.
///
/// A proposal cannot bypass the private state constructor:
/// ```compile_fail
/// use zeno_fcis_collections::bounded::CollectionLimits;
/// use zeno_fcis_collections::pipe::{PipeProfile, ProfileBoundPipe};
/// let profile = PipeProfile { domain: [0; 32], schema: [0; 32] };
/// let limits = CollectionLimits {
///     max_entries: 0, max_item_bytes: 0, max_snapshot_bytes: 0,
/// };
/// let _ = ProfileBoundPipe { profile, limits, pending: vec![], bytes: 0 };
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileBoundPipe {
    profile: PipeProfile,
    limits: CollectionLimits,
    pending: Vec<PipeMessage>,
    bytes: u64,
}
impl ProfileBoundPipe {
    /// Creates an empty pipe. Its complete header is 93 bytes.
    pub fn empty(profile: PipeProfile, limits: CollectionLimits) -> Result<Self, PipeError> {
        if limits.max_snapshot_bytes < 93 {
            return Err(BoundedError::InvalidLimits.into());
        }
        Ok(Self {
            profile,
            limits,
            pending: Vec::new(),
            bytes: 93,
        })
    }
    /// Pending messages in exact FIFO order.
    pub fn pending(&self) -> &[PipeMessage] {
        &self.pending
    }
    /// Complete encoded snapshot length including profile and policy.
    pub fn snapshot_len(&self) -> u64 {
        self.bytes
    }
    /// Returns the head for an initial attempt or an unchanged retry.
    pub fn head(&self) -> Result<&PipeMessage, PipeError> {
        self.pending.first().ok_or(BoundedError::Empty.into())
    }
    /// Identity check, then pending-ID equality/conflict, then Value validation,
    /// item bytes, capacity and complete snapshot bytes, in that order.
    /// Exact pending duplicates succeed without appending or moving the item.
    pub fn enqueue(&self, message: PipeMessage) -> Result<Self, PipeError> {
        if message.profile != self.profile {
            return Err(PipeError::ProfileMismatch);
        }
        if let Some(old) = self.pending.iter().find(|old| old.id == message.id) {
            return if old == &message {
                Ok(self.clone())
            } else {
                Err(PipeError::IdConflict)
            };
        }
        let size = add(40, value_size(&message.payload)?)?;
        let bytes = add(self.bytes, size)?;
        let count = self
            .pending
            .len()
            .checked_add(1)
            .ok_or(BoundedError::ArithmeticOverflow)?;
        check(self.limits, count, size, bytes)?;
        let mut pending = self.pending.clone();
        pending.push(message);
        Ok(Self {
            profile: self.profile,
            limits: self.limits,
            pending,
            bytes,
        })
    }
    /// Checks profile, nonempty head, then exact full-message equality.
    /// Removes only the matching head; no claim about external delivery is made.
    pub fn acknowledge(&self, message: &PipeMessage) -> Result<Self, PipeError> {
        if message.profile != self.profile {
            return Err(PipeError::ProfileMismatch);
        }
        let head = self.head()?;
        if head != message {
            return Err(PipeError::HeadMismatch);
        }
        let size = add(40, value_size(&head.payload)?)?;
        let bytes = self
            .bytes
            .checked_sub(size)
            .ok_or(BoundedError::ArithmeticOverflow)?;
        Ok(Self {
            profile: self.profile,
            limits: self.limits,
            pending: self.pending[1..].to_vec(),
            bytes,
        })
    }
    /// Encodes ZBC1 kind 4, policy, count, domain, schema, then each raw 32-byte
    /// message ID followed by an 8-byte length and canonical payload bytes.
    pub fn snapshot_bytes(&self) -> Result<Vec<u8>, PipeError> {
        let mut out = header(4, self.limits, self.pending.len())?;
        out.extend_from_slice(&self.profile.domain);
        out.extend_from_slice(&self.profile.schema);
        for message in &self.pending {
            out.extend_from_slice(&message.id);
            blob(&mut out, &message.payload)?;
        }
        Ok(out)
    }
}
