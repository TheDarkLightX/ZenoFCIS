//! The same scalar candidate universe for the gap judge and question loop.
//! Callers first validate the contract: each reason then has exactly one class.

use std::collections::BTreeSet;

use super::super::rules::{Class, Rules};

/// Invocation-owned reason IDs; no decision or application authority.
pub(crate) struct DecisionSpace {
    rejects: Vec<u32>,
    failures: Vec<u32>,
}

impl DecisionSpace {
    pub(crate) fn new(rules: &Rules) -> Self {
        let reasons = |class| {
            rules
                .cases
                .iter()
                .filter(|case| case.class == class)
                .filter_map(|case| case.reason)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect()
        };
        Self {
            rejects: reasons(Class::Reject),
            failures: reasons(Class::CommittedFailure),
        }
    }

    pub(crate) fn count(&self, states: usize) -> u128 {
        self.rejects.len() as u128 + states as u128 * (1 + self.failures.len() as u128)
    }

    /// Rejects retain the pre-state (`None`); committing outcomes name every
    /// declared successor. Order is reasons ascending, then successors ascending.
    pub(crate) fn candidates(
        &self,
        states: usize,
    ) -> impl Iterator<Item = (Class, Option<u32>, Option<usize>)> + '_ {
        self.rejects
            .iter()
            .map(|reason| (Class::Reject, Some(*reason), None))
            .chain((0..states).map(|next| (Class::Accept, None, Some(next))))
            .chain(self.failures.iter().flat_map(move |reason| {
                (0..states).map(move |next| (Class::CommittedFailure, Some(*reason), Some(next)))
            }))
    }
}
