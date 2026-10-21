//! Owned consumer projection; it supplies data, never monetary authority.

use super::aggregation::Aggregate;
use super::lifecycle::LifecycleState;
use super::prices::{ClockView, Pending, PriceView};
use super::types::{Hash, Reason};
use super::wire;
use super::{Evaluation, Failure, Limits, Resource, measured};
/// Owned exact source-state projection for a read-only consumer.
#[derive(Clone, Debug)]
pub struct ConsumerRead {
    root: Hash,
    committed: Option<Aggregate>,
    pending: Option<Pending>,
    watermark: u64,
    status: Result<PriceView, Reason>,
    pending_blocked: bool,
    bytes: alloc::vec::Vec<u8>,
}
impl ConsumerRead {
    /// Return the canonical state root.
    pub fn root(&self) -> Hash {
        self.root
    }
    /// Borrow the committed aggregate, if any.
    pub fn committed(&self) -> Option<&Aggregate> {
        self.committed.as_ref()
    }
    /// Borrow the buffered aggregate and its timing, if any.
    pub fn pending(&self) -> Option<&Pending> {
        self.pending.as_ref()
    }
    /// Return the authenticated observation-time watermark in seconds.
    pub fn watermark(&self) -> u64 {
        self.watermark
    }
    /// Borrow the committed read status or its business refusal.
    pub fn status(&self) -> Result<&PriceView, Reason> {
        self.status.as_ref().map_err(|r| *r)
    }
    /// Report whether stale or quarantined pending evidence remains restrictive.
    pub fn pending_blocked(&self) -> bool {
        self.pending_blocked
    }
    /// Borrow the exact canonical source-state bytes for receipt binding.
    pub fn evidence_bytes(&self) -> &[u8] {
        &self.bytes
    }
}
/// Return the evaluator identity installed by the integration freeze.
pub fn evaluator_identity() -> Hash {
    super::super::authority::EVALUATOR
}
/// Extract an owned exact committed and pending consumer view with measured work.
pub fn native_consumer_read(
    s: &LifecycleState,
    feed: Hash,
    clock: ClockView,
    limits: Limits,
) -> Evaluation<ConsumerRead> {
    measured(limits, |m| {
        wire::reserve_scans(super::lifecycle::charged_row_count(s, m)?, m)?;
        let f = s.feed(&feed).ok_or(Failure::InvalidEvidence)?;
        if clock.epoch < s.epoch() {
            return Err(Failure::InvalidEvidence);
        }
        let mut state = s.price(&feed).ok_or(Failure::InvalidState)?.clone();
        state.quarantines = super::native_ports::quarantines(s, feed);
        let security =
            super::native_ports::security(s, feed, state.committed.as_ref(), clock.epoch)?;
        let status = super::prices::checked_view(
            &f.rules,
            &s.profile().parameters,
            &state,
            clock,
            &security,
            m,
        )?;
        let pending_blocked = if let Some(p) = &state.pending {
            p.aggregate.numbers().oldest_epoch > clock.epoch
                || super::prices::stale(&f.rules.clock, &p.aggregate, clock.epoch, state.watermark)
                || super::prices::quarantined(&state, &p.aggregate, m)?
        } else {
            false
        };
        let bytes = s.canonical_bytes()?;
        m.charge(
            Resource::Byte,
            u64::try_from(bytes.len()).map_err(|_| Failure::Arithmetic)?,
        )?;
        Ok(ConsumerRead {
            root: s.root()?,
            committed: state.committed,
            pending: state.pending,
            watermark: state.watermark,
            status,
            pending_blocked,
            bytes,
        })
    })
}
