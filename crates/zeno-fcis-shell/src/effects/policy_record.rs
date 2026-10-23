//! Pure admission evidence, shared by the shell stack and handler-free replay.

use alloc::vec::Vec;
use zeno_fcis_codec::Hash32;

use super::codec;
use super::policy::{self, Monitor, Verdict};
use super::port::{Code, Operation};

/// One monitor's verdict and proposed state, before all-layer acceptance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Decision {
    /// Source- and configuration-bound policy identity.
    pub identity: Hash32,
    /// Canonical state before evaluating the request.
    pub before: Vec<u8>,
    /// Proposed state; a later policy's veto discards every proposal.
    pub proposed: Vec<u8>,
    /// Allow or explicit refusal, never an operation.
    pub verdict: Verdict,
}

/// Ordered admission decisions, ending at the first refusal.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Check {
    /// Decisions in the bound monitor order.
    pub decisions: Vec<Decision>,
}

impl Check {
    /// The first refusal, if any.
    #[must_use]
    pub fn rejection(&self) -> Option<Code> {
        self.decisions
            .iter()
            .find_map(|decision| match decision.verdict {
                Verdict::Allow => None,
                Verdict::Reject(code) => Some(code),
            })
    }

    /// Versioned canonical evidence; independent of Rust memory layout.
    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        codec::bytes(&mut bytes, b"zeno-fcis/effect-policy-check/1");
        bytes.extend_from_slice(&(self.decisions.len() as u64).to_be_bytes());
        for decision in &self.decisions {
            codec::hash(&mut bytes, decision.identity);
            codec::bytes(&mut bytes, &decision.before);
            codec::bytes(&mut bytes, &decision.proposed);
            match decision.verdict {
                Verdict::Allow => bytes.push(0),
                Verdict::Reject(code) => bytes.extend_from_slice(&[1, code as u8]),
            }
        }
        bytes
    }
}

/// Evaluate immutable monitors on the exact request. A veto returns the
/// original states; success returns owned reservations to install *before*
/// invoking the terminal. The caller may undo them only on proven non-invocation.
#[must_use]
pub fn admit(monitors: &[Monitor], operation: &Operation) -> (Vec<Monitor>, Check) {
    let mut next = monitors.to_vec();
    let mut check = Check::default();
    for monitor in &mut next {
        let (state, verdict) = policy::step(&monitor.policy, &monitor.state, operation, None);
        check.decisions.push(Decision {
            identity: monitor.identity(),
            before: monitor.state.bytes(),
            proposed: state.bytes(),
            verdict,
        });
        if let Verdict::Reject(_) = verdict {
            return (monitors.to_vec(), check);
        }
        monitor.state = state;
    }
    (next, check)
}
