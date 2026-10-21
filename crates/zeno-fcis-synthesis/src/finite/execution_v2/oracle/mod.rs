//! Native pure oracle relations. These plans cannot publish, authenticate a host,
//! or authorize monetary use. Raw cryptographic admission is a distinct gate.
//! All numerical profiles are explicit; there are no live deployment defaults.

#[cfg(test)]
mod account_law_tests;
mod aggregation;
mod arithmetic;
mod verification;
pub use verification::*;
mod bond_laws;
mod bonds;
#[cfg(test)]
mod finite_account_tests;
#[cfg(test)]
mod finite_price_tests;
mod laws;
mod lifecycle;
mod lifecycle_ingress;
mod lifecycle_laws;
mod lifecycle_schema;
#[cfg(test)]
mod lifecycle_tests;
mod native_ports;
mod native_rewards;
mod prices;
mod quorum_laws;
mod read;
mod reports;
#[cfg(test)]
mod test_verifier;
pub use read::*;
mod reward_laws;
mod rewards;
mod types;
mod wire;

use super::meter::{self, Meter};
/// Canonical table limits bound into the oracle profile.
pub use super::tables::CollectionLimits;
pub use super::{Limits, MeterFailure, Resource, Usage, zero_limits};
pub use aggregation::{Aggregate, FeedRules, TimeQuorum, aggregate, time_quorum};
pub use arithmetic::{AggregateNumbers, median3, security_capacity};
pub use bond_laws::check_bond_laws;
pub use bonds::*;
pub use laws::{check_price_laws, check_report_chain_laws};
pub use lifecycle::*;
pub use lifecycle_ingress::{lifecycle_command_bytes, lifecycle_relation_bytes};
pub use lifecycle_laws::check_lifecycle_laws;
pub use lifecycle_schema::{check_lifecycle_snapshot_bytes, lifecycle_schema_bytes};
pub use native_ports::native_committed_view;
pub use prices::*;
pub use reports::*;
pub use reward_laws::check_reward_laws;
pub use rewards::*;
pub use types::*;

/// Preparation, validation or independent-law failure; never publication authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Failure {
    /// A required policy parameter is absent.
    UnboundParameter,
    /// The requested signature scheme is outside the bound profile.
    UnsupportedScheme,
    /// The installed verifier backend is unavailable.
    BackendUnavailable,
    /// Supplied evidence violates its binding or validation rules.
    InvalidEvidence,
    /// Retained state violates its structural or accounting invariants.
    InvalidState,
    /// Explicit profile bindings are inconsistent or out of bounds.
    InvalidProfile,
    /// The value or encoding is outside the closed canonical format.
    Canonical,
    /// Exact arithmetic overflowed or violated its numeric domain.
    Arithmetic,
    /// An independent law identified this stable violation code.
    Law(u32),
    /// The invocation exceeded this measured resource limit.
    Budget(MeterFailure),
}
impl From<MeterFailure> for Failure {
    fn from(value: MeterFailure) -> Self {
        Self::Budget(value)
    }
}

/// An owned audit result. No publication constructor consumes this type.
#[must_use]
#[derive(Debug)]
pub struct Evaluation<T> {
    result: Result<T, Failure>,
    usage: Usage,
}
impl<T> Evaluation<T> {
    /// Borrow the checked result or preparation failure.
    pub fn result(&self) -> Result<&T, &Failure> {
        self.result.as_ref()
    }
    /// Return work charged on success or refusal.
    pub fn usage(&self) -> Usage {
        self.usage
    }
    /// Consume this result into its owned outcome and measured work.
    pub fn into_parts(self) -> (Result<T, Failure>, Usage) {
        (self.result, self.usage)
    }
}
fn measured<T>(
    limits: Limits,
    run: impl FnOnce(&mut Meter) -> Result<T, Failure>,
) -> Evaluation<T> {
    let mut meter = meter::new(limits);
    let result = run(&mut meter);
    Evaluation {
        result,
        usage: meter.used,
    }
}
fn step(m: &mut Meter) -> Result<(), Failure> {
    m.charge(Resource::Step, 1).map_err(Into::into)
}
