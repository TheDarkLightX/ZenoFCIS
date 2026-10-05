//! The decision table's refusals: counted by class, split by whether the
//! input's pre-state satisfies every state law, and grouped by refusal and
//! by the first state law a pre-state does not satisfy.
//!
//! A law refusal of an input whose pre-state satisfies every state law is a
//! finding: from a state the laws allow, the rules make a decision that a
//! law refuses. A refusal of an input whose pre-state breaks a state law is
//! a refusal on a state the laws exclude: no committed state is such a
//! state. Whether a law-consistent pre-state is reachable from genesis is
//! not decided here.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::evaluate::RefusalClass;
use super::table::{Row, Table};

/// The refused inputs that share one refusal and one state-law verdict on
/// their pre-states.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Group {
    /// Index into the table's refusals.
    pub(super) refusal: u32,
    /// The first state law the pre-states do not satisfy; `None` when they
    /// satisfy every one.
    pub(super) unsatisfied: Option<u32>,
    pub(super) count: usize,
    /// Ordinal of the first such input in the input set.
    pub(super) first: usize,
}

/// Every group of refused inputs, in the order of each group's first input.
pub(super) fn groups(table: &Table) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    let mut positions: BTreeMap<(u32, Option<u32>), usize> = BTreeMap::new();
    for (ordinal, row) in table.rows.iter().enumerate() {
        if let Row::Refused {
            refusal,
            unsatisfied,
        } = *row
        {
            match positions.get(&(refusal, unsatisfied)) {
                Some(position) => groups[*position].count += 1,
                None => {
                    positions.insert((refusal, unsatisfied), groups.len());
                    groups.push(Group {
                        refusal,
                        unsatisfied,
                        count: 1,
                        first: ordinal,
                    });
                }
            }
        }
    }
    groups
}

/// Refused inputs counted by refusal class.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct Counts {
    pub(super) total: usize,
    pub(super) by_class: BTreeMap<&'static str, usize>,
}

impl Counts {
    fn add(&mut self, class: RefusalClass, count: usize) {
        self.total += count;
        *self.by_class.entry(class.name()).or_default() += count;
    }

    pub(super) fn json(&self) -> Value {
        json!({"count": self.total, "by_class": self.by_class})
    }

    /// `law 16, domain 2`, in class order; `none` when nothing was refused.
    pub(super) fn text(&self) -> String {
        let parts: Vec<String> = RefusalClass::ALL
            .iter()
            .filter_map(|class| {
                self.by_class
                    .get(class.name())
                    .map(|count| format!("{} {count}", class.name()))
            })
            .collect();
        if parts.is_empty() {
            "none".to_owned()
        } else {
            parts.join(", ")
        }
    }
}

/// The refusal counts a summary reports.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct Tally {
    pub(super) all: Counts,
    /// Refused inputs whose pre-state satisfies every state law.
    pub(super) law_consistent: Counts,
    /// Refused inputs whose pre-state breaks a state law.
    pub(super) excluded: Counts,
    /// Refused inputs by the first state law their pre-state does not satisfy.
    pub(super) by_state_law: BTreeMap<u32, usize>,
}

impl Tally {
    pub(super) fn of(table: &Table, groups: &[Group]) -> Self {
        let mut tally = Self::default();
        for group in groups {
            let class = table.refusals[group.refusal as usize].class;
            tally.all.add(class, group.count);
            match group.unsatisfied {
                None => tally.law_consistent.add(class, group.count),
                Some(law) => {
                    tally.excluded.add(class, group.count);
                    *tally.by_state_law.entry(law).or_default() += group.count;
                }
            }
        }
        tally
    }

    pub(super) fn json(&self) -> Value {
        json!({
            "count": self.all.total,
            "by_class": self.all.by_class,
            "on_law_consistent_states": self.law_consistent.json(),
            "on_states_the_laws_exclude": self.excluded.json(),
        })
    }
}
