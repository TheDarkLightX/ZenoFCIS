//! The same scalar candidate universe for the gap judge and question loop.
//! Callers first validate the contract: each reason then has exactly one class.

use std::collections::BTreeSet;

use super::super::rules::{Class, Rules};
use super::evaluate::Delivery;
use serde_json::{Value, json};

/// Both counts use the same outcome definition; the requirements supplied to
/// each judge can differ. Complete delivery answers are not exported as laws.
pub(crate) fn determination_scope(has_channels: bool, basis: &str) -> Value {
    json!({
        "basis": basis,
        "components": ["outcome_class", "next_state", "reason", "deliveries"],
        "meaning": "exactly one full lawful outcome equal to the Authority's successful decision",
        "uncovered_components": if has_channels { vec!["deliveries"] } else { vec![] },
        "uncovered_scope": "exported-laws",
        "uncovered_reason": has_channels.then_some("complete ordered delivery requirements have no exported law form"),
    })
}

/// Scalar outcome with the actual successor index, including the unchanged
/// pre-state for a reject. Candidates must be distinct and law-checked.
pub(crate) type ScalarDecision = (Class, Option<u32>, usize);

/// Delivery alternatives left by the checked requirements. Unrestricted is
/// valid only after the caller checks complete coverage and law independence.
#[derive(Clone, Copy)]
pub(crate) enum DeliveryChoices<'a> {
    Fixed(&'a [Delivery]),
    Unrestricted,
}

pub(crate) struct Determination {
    /// Exact zero/one/multiple classification; unrestricted counts are lower bounds.
    pub(crate) outcomes: usize,
    pub(crate) determined: bool,
    pub(crate) disagreement: bool,
}

/// One definition for both judges: exactly one complete lawful outcome, and
/// it agrees with the successfully executed Authority outcome in every field.
/// No decision authority is created here. Inputs are borrowed immutably, and
/// the returned counts contain no references to invocation scratch.
pub(crate) fn determination(
    candidates: impl IntoIterator<Item = ScalarDecision>,
    reference: Option<ScalarDecision>,
    native_deliveries: &[Delivery],
    deliveries: DeliveryChoices<'_>,
) -> Determination {
    let mut outcomes = 0;
    let mut agrees = false;
    for candidate in candidates {
        let count = match (candidate.0, deliveries) {
            (Class::Reject, DeliveryChoices::Fixed(wanted)) if !wanted.is_empty() => 0,
            (Class::Reject, _) | (_, DeliveryChoices::Fixed(_)) => 1,
            (_, DeliveryChoices::Unrestricted) => 2,
        };
        outcomes += count;
        let delivery_agrees = match deliveries {
            DeliveryChoices::Fixed(wanted) => wanted == native_deliveries,
            DeliveryChoices::Unrestricted => {
                candidate.0 != Class::Reject || native_deliveries.is_empty()
            }
        };
        agrees |= count > 0 && Some(candidate) == reference && delivery_agrees;
    }
    Determination {
        outcomes,
        determined: outcomes == 1 && agrees,
        disagreement: reference.is_some() && outcomes > 0 && !agrees,
    }
}

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
    fn determination_covers_every_outcome_component_and_native_refusal() {
        use super::super::evaluate::Value as Field;
        let own = (Class::CommittedFailure, Some(202), 1);
        let delivery = Delivery {
            ordinal: 0,
            channel: 300,
            destination: Field::Text(b"desk".to_vec()),
            payload: vec![(130, Field::I128(1))],
            idempotency: Field::U128(0),
        };
        let outbox = [delivery];
        let fixed = DeliveryChoices::Fixed(&outbox);
        assert!(determination([own], Some(own), &outbox, fixed).determined);
        for alternative in [
            (Class::Accept, None, 1),
            (Class::CommittedFailure, Some(203), 1),
            (Class::CommittedFailure, Some(202), 0),
        ] {
            let result = determination([alternative], Some(own), &outbox, fixed);
            assert!(!result.determined);
            assert!(result.disagreement);
        }
        for field in 0..6 {
            let mut different = outbox.clone();
            match field {
                0 => different[0].ordinal = 1,
                1 => different[0].channel = 301,
                2 => different[0].destination = Field::Text(b"other".to_vec()),
                3 => different[0].payload[0].1 = Field::I128(2),
                4 => different[0].payload[0].0 = 131,
                _ => different[0].idempotency = Field::U128(1),
            }
            let result = determination(
                [own],
                Some(own),
                &outbox,
                DeliveryChoices::Fixed(&different),
            );
            assert!(!result.determined, "delivery field {field}");
            assert!(result.disagreement);
        }
        assert!(!determination([own], None, &outbox, fixed).determined);
        assert!(!determination([], Some(own), &outbox, fixed).determined);
        assert!(
            !determination([own], Some(own), &outbox, DeliveryChoices::Unrestricted).determined
        );
        assert!(
            !determination([own, (Class::Accept, None, 1)], Some(own), &outbox, fixed).determined
        );
        let reject = (Class::Reject, Some(200), 0);
        assert!(
            determination([reject], Some(reject), &[], DeliveryChoices::Unrestricted).determined
        );
    }

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
