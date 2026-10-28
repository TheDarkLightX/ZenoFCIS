//! Bounded wire requests over supervisor-held intent handles.
//!
//! Trusted administration binds a channel to an authenticated tenant/principal
//! and an immutable table of actual opaque handles. A worker supplies only a
//! sequence number and table slot. Identity labels and wire bytes cannot grant
//! authority. The supervisor must own the dedicated IPC endpoints, frame reads
//! within 16 bytes, enforce a wall deadline and contain the worker process.
//! This module does not authenticate arbitrary network connections or isolate
//! native code. Never recreate a channel as a malformed-message recovery path.
//!
//! A request is `ZFW1`, a little-endian u64 sequence starting at zero, and a
//! little-endian u32 slot. A reply is `ZFR1`, the same sequence, one status byte,
//! one detail byte, and two zero bytes. Replies are observations, never receipts,
//! acknowledgement authority or authorization to retry an unknown operation.

use zeno_fcis_codec::Hash32;
use zeno_fcis_shell::effects::port::Sent;

use super::{
    dispatcher::{self, IntentHandle, Refusal, TrustedDispatcher},
    production::Transport,
};

/// Exact size of one version-1 request or reply.
pub const FRAME_BYTES: usize = 16;
/// Maximum number of handles in one channel's immutable slot table.
pub const MAX_HANDLES: usize = 64;
/// Maximum admitted requests over one channel's lifetime.
pub const MAX_REQUESTS: u32 = 1024;

/// A protocol refusal; no dispatch starts for the refused request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// The trusted configuration exceeds the bounds or permits no requests.
    Configuration,
    /// The exact frame length or version marker differs.
    Format,
    /// A request repeats, skips or overflows the next expected sequence.
    Sequence,
    /// The selected slot is absent from the trusted table.
    Slot,
    /// The channel consumed its request budget.
    Limit,
    /// An earlier protocol violation, dispatcher failure or unwind stopped the channel.
    Stopped,
}

/// Coarse observation; transport/storage diagnostics stay with the dispatcher.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    /// Byte 0: the transport reported the original delivery sent.
    Sent,
    /// Byte 1: the delivery may or may not have been sent; retain its reservation.
    Unknown,
    /// Byte 2: the transport reported that it did not send.
    NotSent,
    /// Byte 3: the durable policy refused the attempt.
    PolicyRefused,
    /// Byte 4: current dispatcher admission refused; detail is 1 through 7 in order.
    DispatcherRefused(Refusal),
    /// Byte 5: dispatcher/storage failure; effects may have occurred. Channel stops.
    DispatcherUnavailable,
}

/// An advisory reply for one consumed request sequence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Reply {
    /// The request's channel-local sequence number.
    pub sequence: u64,
    /// Reported outcome; it cannot acknowledge or publish anything.
    pub status: Status,
}

impl Reply {
    /// Serialize the fixed version-1 report without changing dispatcher state.
    #[must_use]
    pub fn encode(self) -> [u8; FRAME_BYTES] {
        let (status, detail) = match self.status {
            Status::Sent => (0, 0),
            Status::Unknown => (1, 0),
            Status::NotSent => (2, 0),
            Status::PolicyRefused => (3, 0),
            Status::DispatcherRefused(reason) => (
                4,
                match reason {
                    Refusal::Owner => 1,
                    Refusal::Tenant => 2,
                    Refusal::Principal => 3,
                    Refusal::Grant => 4,
                    Refusal::Intent => 5,
                    Refusal::Pending => 6,
                    Refusal::Content => 7,
                },
            ),
            Status::DispatcherUnavailable => (5, 0),
        };
        let mut out = [0; FRAME_BYTES];
        out[..4].copy_from_slice(b"ZFR1");
        out[4..12].copy_from_slice(&self.sequence.to_le_bytes());
        out[12] = status;
        out[13] = detail;
        out
    }
}

/// Encode a request. Producing these bytes confers no authority.
#[must_use]
pub fn request(sequence: u64, slot: u32) -> [u8; FRAME_BYTES] {
    let mut out = [0; FRAME_BYTES];
    out[..4].copy_from_slice(b"ZFW1");
    out[4..12].copy_from_slice(&sequence.to_le_bytes());
    out[12..].copy_from_slice(&slot.to_le_bytes());
    out
}

/// Exclusive supervisor state for one dedicated authenticated IPC channel.
/// No handles, identity selection or mutable dispatcher access arrive over the wire.
pub struct Channel<'h> {
    tenant: Hash32,
    principal: Hash32,
    handles: &'h [IntentHandle],
    next: u32,
    limit: u32,
    stopped: bool,
}

impl<'h> Channel<'h> {
    /// Bind authenticated identity and handles in trusted administration.
    /// An empty table permits computation but no dispatch request can succeed.
    pub fn new(
        tenant: Hash32,
        principal: Hash32,
        handles: &'h [IntentHandle],
        limit: u32,
    ) -> Result<Self, Error> {
        if handles.len() > MAX_HANDLES || limit == 0 || limit > MAX_REQUESTS {
            return Err(Error::Configuration);
        }
        Ok(Self {
            tenant,
            principal,
            handles,
            next: 0,
            limit,
            stopped: false,
        })
    }

    /// Consume one complete frame and recheck the actual dispatcher's current grant.
    /// Protocol refusal stops this channel before invoking the dispatcher. Poisoning
    /// precedes possible dispatch so an unwind or lost reply cannot replay this
    /// sequence. A new channel still encounters the durable intent reservation.
    /// Refusal precedence: stopped, format, sequence, slot, request limit.
    pub fn dispatch<T: Transport>(
        &mut self,
        dispatcher: &mut TrustedDispatcher<'_, '_, T>,
        bytes: &[u8],
    ) -> Result<Reply, Error> {
        if self.stopped {
            return Err(Error::Stopped);
        }
        self.stopped = true;
        let frame: &[u8; FRAME_BYTES] = bytes.try_into().map_err(|_| Error::Format)?;
        if &frame[..4] != b"ZFW1" {
            return Err(Error::Format);
        }
        let mut seq = [0; 8];
        seq.copy_from_slice(&frame[4..12]);
        let sequence = u64::from_le_bytes(seq);
        if sequence != u64::from(self.next) {
            return Err(Error::Sequence);
        }
        let mut slot = [0; 4];
        slot.copy_from_slice(&frame[12..]);
        let index = usize::try_from(u32::from_le_bytes(slot)).map_err(|_| Error::Slot)?;
        let handle = self.handles.get(index).ok_or(Error::Slot)?;
        if self.next >= self.limit {
            return Err(Error::Limit);
        }
        // next <= limit <= 1024: neither this addition nor the wire conversion wraps.
        self.next += 1;
        let status = match dispatcher
            .session(self.tenant, self.principal)
            .dispatch(handle)
        {
            Ok(Sent::Sent(_)) => Status::Sent,
            Ok(Sent::Unknown(_)) => Status::Unknown,
            Ok(Sent::NotSent(_)) => Status::NotSent,
            Ok(Sent::Refused(_)) => Status::PolicyRefused,
            Err(dispatcher::Error::Refused(reason)) => Status::DispatcherRefused(reason),
            Err(_) => Status::DispatcherUnavailable,
        };
        self.stopped = status == Status::DispatcherUnavailable;
        Ok(Reply { sequence, status })
    }
}
