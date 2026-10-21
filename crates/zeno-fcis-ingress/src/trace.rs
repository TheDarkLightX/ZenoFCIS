//! Redacted, fixed-schema observations. A sink is an I/O adapter, never authority.
use crate::{Command, Refusal};
use std::fmt;
use zeno_fcis_codec::Hash32;

/// Journey stages, including incomplete attempts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Event {
    /// A request passed transport authentication.
    Request,
    /// Core produced a committing publication.
    Decision,
    /// Commit succeeded or recovered its original certificate.
    Commit,
    /// A committed outbox record is being sent.
    Send,
    /// Delivery acknowledgement was recorded.
    Acknowledge,
    /// An effect did not complete or returned a non-success response.
    Fault,
    /// HTTP reply was prepared (its receipt by the client remains unknown).
    Reply,
    /// Fixed refusal, without raw input or diagnostics.
    Refused(Refusal),
}

/// Correlation IDs only: never credentials, command, context, payload or headers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Trace {
    /// Journey stage.
    pub event: Event,
    /// Hash of authenticated bindings and command, absent before body admission.
    pub request: Option<Hash32>,
    /// Scoped durable replay key, absent before body admission.
    pub replay: Option<Hash32>,
    /// Hash of the Authority publication subject.
    pub decision: Option<Hash32>,
    /// Commit certificate's chain link.
    pub commit: Option<Hash32>,
    /// Durable delivery ID, when observing delivery.
    pub delivery: Option<Hash32>,
}
impl Trace {
    pub(crate) fn empty(event: Event) -> Self {
        Self {
            event,
            request: None,
            replay: None,
            decision: None,
            commit: None,
            delivery: None,
        }
    }
    pub(crate) fn request(command: &Command) -> Self {
        Self {
            request: Some(command.id()),
            replay: Some(command.replay_key()),
            ..Self::empty(Event::Request)
        }
    }
}
impl fmt::Display for Trace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "event={:?}", self.event)?;
        for (name, id) in [
            ("request", self.request),
            ("replay", self.replay),
            ("decision", self.decision),
            ("commit", self.commit),
            ("delivery", self.delivery),
        ] {
            if let Some(id) = id {
                write!(f, " {name}={id}")?;
            }
        }
        Ok(())
    }
}

/// Caller-owned output adapter. It receives no credentials or raw bytes.
pub trait TraceSink {
    /// Record one observation; never use it to authorize a decision or commit.
    fn record(&mut self, trace: Trace);
}
