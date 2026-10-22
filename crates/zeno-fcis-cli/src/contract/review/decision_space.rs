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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{declarations::Declarations, model::Contract, schema, schema_commitment};

    #[test]
    fn every_counted_reason_matches_the_language_catalog_and_has_a_law_scope() {
        let project = include_str!("../../../../../examples/dual-approval/project.zeno");
        let rules = Rules::read(include_str!(
            "../../../../../examples/dual-approval/v2/policy.json"
        ))
        .unwrap_or_else(|error| panic!("witness rules: {error:?}"));
        let declarations = Declarations::read(project, &rules.leaf_bindings)
            .unwrap_or_else(|error| panic!("witness declarations: {error:?}"));
        let schema = schema::encode(&declarations)
            .unwrap_or_else(|error| panic!("witness schema: {error:?}"));
        let commitment = schema_commitment(&schema)
            .unwrap_or_else(|error| panic!("witness commitment: {error:?}"));
        let catalog = Contract::build(&declarations, &rules, commitment)
            .unwrap_or_else(|error| panic!("witness contract: {error:?}"));
        let space = DecisionSpace::new(&rules);
        let candidates: Vec<_> = space.candidates(2).collect();
        assert_eq!(candidates.len() as u128, space.count(2));
        assert!(catalog.reasons.contains(&(202, Class::CommittedFailure)));
        let mut scoped_rules = rules.clone();
        scoped_rules
            .law_kinds
            .insert(9000, super::super::super::rules::LawKind::FeeAndRounding);
        for (class, reason, _) in candidates {
            let scope = match (class, reason) {
                (Class::Accept, None) => "accept".to_owned(),
                (Class::Reject, Some(reason)) => {
                    assert!(
                        catalog.reasons.contains(&(reason, class)),
                        "Reject {reason}"
                    );
                    format!("reject:{reason}")
                }
                (Class::CommittedFailure, Some(reason)) => {
                    assert!(
                        catalog.reasons.contains(&(reason, class)),
                        "Failure {reason}"
                    );
                    "failure".to_owned()
                }
                other => panic!("invalid candidate class and reason: {other:?}"),
            };
            let extended = format!("{project}\nlaw 9000 exclude_candidate on {scope} = false;\n");
            let declarations = Declarations::read(&extended, &rules.leaf_bindings)
                .unwrap_or_else(|error| panic!("candidate law: {error:?}"));
            assert!(
                Contract::build(&declarations, &scoped_rules, commitment).is_ok(),
                "a counted alternative must have an accepted law scope: {scope}"
            );
        }
    }
}
