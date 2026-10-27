//! Native zUSD relations and conditional step-4 composition. Authentication
//! requires a ReceiptVerifierV1 implementation meeting its complete contract;
//! no live verifier, clock mapping, drip or owner policy is supplied.
#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;
mod accounting;
pub mod accumulator;
pub mod application;
pub use accumulator::Accumulator256;
pub mod common;
pub mod composition;
pub mod fees;
pub mod genesis;
pub mod laws;
pub mod ledger;
pub mod liquidation;
pub mod observations;
pub mod parameters;
pub mod redemption;
pub mod stability;
pub mod staking;
pub mod vaults;
mod wide;
pub mod wire;

/// Fixed-point rate precision; quantities retain separate asset tags.
pub const Q: u128 = 1_000_000_000_000_000_000;
pub const NICR_Q: u128 = 100_000_000_000_000_000_000;
pub const SCALE: u128 = 1_000_000_000;
pub type Id = [u8; 32];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Failure {
    Arithmetic,
    Domain,
    Canonical,
    Capacity,
    Conservation,
    Law(u16),
    UnboundProfile,
    Budget {
        lane: u8,
        limit: u64,
        attempted: u64,
    },
    Reject(u16),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub steps: u64,
    pub reads: u64,
    pub bytes: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Usage {
    steps: u64,
    reads: u64,
    bytes: u64,
}
impl Usage {
    pub fn steps(self) -> u64 {
        self.steps
    }
    pub fn reads(self) -> u64 {
        self.reads
    }
    pub fn bytes(self) -> u64 {
        self.bytes
    }
}
#[derive(Debug)]
#[must_use]
pub struct Evaluation<T> {
    result: Result<T, Failure>,
    usage: Usage,
}
impl<T> Evaluation<T> {
    pub fn result(&self) -> Result<&T, Failure> {
        self.result.as_ref().map_err(|e| *e)
    }
    pub fn usage(&self) -> Usage {
        self.usage
    }
    pub fn into_parts(self) -> (Result<T, Failure>, Usage) {
        (self.result, self.usage)
    }
}
// Exclusive invocation scratch. No caller can construct or retain a Work.
pub(crate) struct Work {
    usage: Usage,
    limits: Limits,
}
impl Work {
    fn new(limits: Limits) -> Self {
        Self {
            usage: Usage {
                steps: 0,
                reads: 0,
                bytes: 0,
            },
            limits,
        }
    }
    /// Only crate-owned pure relations may supply a sub-evaluation. The outer
    /// invocation retains every completed charge, including a refused subcall.
    pub(crate) fn invoke<T>(
        &mut self,
        call: impl FnOnce(Limits) -> Evaluation<T>,
    ) -> Result<T, Failure> {
        let before = self.usage;
        let remaining = Limits {
            steps: self.limits.steps - before.steps,
            reads: self.limits.reads - before.reads,
            bytes: self.limits.bytes - before.bytes,
        };
        let (result, used) = call(remaining).into_parts();
        self.charge(used.steps, used.reads, used.bytes)?;
        result.map_err(|e| match e {
            Failure::Budget {
                lane, attempted, ..
            } => {
                let (prior, limit) = match lane {
                    0 => (before.steps, self.limits.steps),
                    1 => (before.reads, self.limits.reads),
                    _ => (before.bytes, self.limits.bytes),
                };
                Failure::Budget {
                    lane,
                    limit,
                    attempted: prior.saturating_add(attempted),
                }
            }
            x => x,
        })
    }
    pub(crate) fn charge(&mut self, steps: u64, reads: u64, bytes: u64) -> Result<(), Failure> {
        for (lane, used, limit, add) in [
            (0, &mut self.usage.steps, self.limits.steps, steps),
            (1, &mut self.usage.reads, self.limits.reads, reads),
            (2, &mut self.usage.bytes, self.limits.bytes, bytes),
        ] {
            let next = used.checked_add(add).ok_or(Failure::Budget {
                lane,
                limit,
                attempted: u64::MAX,
            })?;
            if next > limit {
                return Err(Failure::Budget {
                    lane,
                    limit,
                    attempted: next,
                });
            }
            *used = next;
        }
        Ok(())
    }
}
pub(crate) fn run<T>(
    limits: Limits,
    f: impl FnOnce(&mut Work) -> Result<T, Failure>,
) -> Evaluation<T> {
    let mut work = Work::new(limits);
    let result = f(&mut work);
    Evaluation {
        result,
        usage: work.usage,
    }
}
pub(crate) fn add(a: u128, b: u128, w: &mut Work) -> Result<u128, Failure> {
    w.charge(1, 0, 0)?;
    a.checked_add(b).ok_or(Failure::Arithmetic)
}
pub(crate) fn sub(a: u128, b: u128, w: &mut Work) -> Result<u128, Failure> {
    w.charge(1, 0, 0)?;
    a.checked_sub(b).ok_or(Failure::Arithmetic)
}
pub(crate) fn muldiv(a: u128, b: u128, d: u128, w: &mut Work) -> Result<u128, Failure> {
    wide::Wide::from(a)
        .mul(b, w)?
        .quotient(wide::Wide::from(d), w)
        .map(|(q, _)| q)
}
/// Exact wide arithmetic observation, without economic authority.
pub fn mul_div(a: u128, b: u128, d: u128, limits: Limits) -> Evaluation<u128> {
    run(limits, |w| muldiv(a, b, d, w))
}
/// Preserve two ordered floors, allowing their intermediate to exceed U128.
pub fn divided_product(a: u128, b: u128, d: u128, e: u128, limits: Limits) -> Evaluation<u128> {
    run(limits, |w| {
        let denominator = wide::Wide::from(d).mul(e, w)?;
        wide::Wide::from(a)
            .mul(b, w)?
            .quotient(denominator, w)
            .map(|(q, _)| q)
    })
}
