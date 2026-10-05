//! The decision table: one compact row per input, with each distinct
//! successor state, outbox and refusal kept once.

use std::collections::BTreeMap;

use super::super::rules::Class;
use super::evaluate::{Decision, Delivery, Fields, Outcome};

/// One input's outcome, by index into the table's dictionaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Row {
    Decided {
        class: Class,
        reason: Option<u32>,
        post: u32,
        outbox: u32,
    },
    Refused(u32),
}

#[derive(Debug, Default)]
pub(super) struct Table {
    pub(super) rows: Vec<Row>,
    /// Distinct successor states with their digests, in first-seen order.
    pub(super) post_states: Vec<([u8; 32], Fields)>,
    /// Distinct outboxes with their digests, in first-seen order.
    pub(super) outboxes: Vec<([u8; 32], Vec<Delivery>)>,
    /// Distinct refusals, in first-seen order.
    pub(super) refusals: Vec<String>,
    post_index: BTreeMap<[u8; 32], u32>,
    outbox_index: BTreeMap<[u8; 32], u32>,
    refusal_index: BTreeMap<String, u32>,
}

impl Table {
    pub(super) fn record(&mut self, outcome: &Outcome) {
        let row = match outcome {
            Outcome::Decision(decision) => {
                let post = match self.post_index.get(&decision.post_digest) {
                    Some(index) => *index,
                    None => {
                        let index = count(self.post_states.len());
                        self.post_states
                            .push((decision.post_digest, decision.post.clone()));
                        self.post_index.insert(decision.post_digest, index);
                        index
                    }
                };
                let outbox = match self.outbox_index.get(&decision.outbox_digest) {
                    Some(index) => *index,
                    None => {
                        let index = count(self.outboxes.len());
                        self.outboxes
                            .push((decision.outbox_digest, decision.outbox.clone()));
                        self.outbox_index.insert(decision.outbox_digest, index);
                        index
                    }
                };
                Row::Decided {
                    class: decision.class,
                    reason: decision.reason,
                    post,
                    outbox,
                }
            }
            Outcome::Refused(refusal) => match self.refusal_index.get(refusal) {
                Some(index) => Row::Refused(*index),
                None => {
                    let index = count(self.refusals.len());
                    self.refusals.push(refusal.clone());
                    self.refusal_index.insert(refusal.clone(), index);
                    Row::Refused(index)
                }
            },
        };
        self.rows.push(row);
    }

    /// Whether an outcome is the recorded one: the same class, reason and
    /// digests, or the same refusal.
    pub(super) fn matches(&self, row: Row, outcome: &Outcome) -> bool {
        match (row, outcome) {
            (
                Row::Decided {
                    class,
                    reason,
                    post,
                    outbox,
                },
                Outcome::Decision(decision),
            ) => {
                class == decision.class
                    && reason == decision.reason
                    && self.post_states[post as usize].0 == decision.post_digest
                    && self.outboxes[outbox as usize].0 == decision.outbox_digest
            }
            (Row::Refused(index), Outcome::Refused(refusal)) => {
                self.refusals[index as usize] == *refusal
            }
            _ => false,
        }
    }

    /// The complete outcome a row records.
    pub(super) fn outcome(&self, row: Row) -> Outcome {
        match row {
            Row::Decided {
                class,
                reason,
                post,
                outbox,
            } => {
                let (post_digest, post) = &self.post_states[post as usize];
                let (outbox_digest, outbox) = &self.outboxes[outbox as usize];
                Outcome::Decision(Decision {
                    class,
                    reason,
                    post: post.clone(),
                    outbox: outbox.clone(),
                    post_digest: *post_digest,
                    outbox_digest: *outbox_digest,
                })
            }
            Row::Refused(index) => Outcome::Refused(self.refusals[index as usize].clone()),
        }
    }
}

fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
