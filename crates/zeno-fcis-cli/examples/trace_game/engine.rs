//! Internal unreduced search. Adapters and requirements are fixed by the harness.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub(crate) struct Move {
    pub(crate) actor: u8,
    pub(crate) request: Vec<i128>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Outcome {
    pub(crate) kind: String,
    pub(crate) reason: Option<u32>,
    pub(crate) component: Option<String>,
    pub(crate) post: Vec<i128>,
    // These are declared model observations, not native outbox deliveries.
    pub(crate) events: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Pending {
    pub(crate) id: u8,
    pub(crate) submitted: u8,
    pub(crate) entitlement: i128,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Snapshot {
    pub(crate) state: Vec<i128>,
    pub(crate) reference: Vec<i128>,
    pub(crate) pending: Option<Pending>,
    pub(crate) time: u8,
    pub(crate) stage: u8,
    pub(crate) monitor: u8,
}

pub(crate) struct CheckedStep {
    pub(crate) next: Snapshot,
    pub(crate) violations: Vec<String>,
    pub(crate) outcome: Outcome,
    pub(crate) completed: u64,
}

// There is deliberately no acceptance callback on the repair proposal path.
pub(crate) trait Model {
    fn starts(&self) -> Vec<Vec<i128>>;
    fn moves(&self, snapshot: &Snapshot, coalition: u8) -> Vec<Move>;
    fn initial_violations(&self, start: &[i128]) -> Vec<String>;
    fn loss(&self, _snapshot: &Snapshot, _coalition: u8) -> i128 {
        0
    }
    fn step(
        &self,
        start: &[i128],
        coalition: u8,
        snapshot: &Snapshot,
        action: &Move,
    ) -> Result<CheckedStep, String>;
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub(crate) struct Limits {
    pub(crate) horizon: u8,
    pub(crate) transitions: u64,
    pub(crate) frontier: usize,
    pub(crate) nodes: usize,
    // Deterministic externally captured work deadline, not a core clock read.
    pub(crate) deadline_after_work: u64,
    // Advisory enumeration clock only; never a game or Authority input.
    pub(crate) wall_millis: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Node {
    pub(crate) parent: Option<usize>,
    pub(crate) action: Option<Move>,
    pub(crate) start: usize,
    pub(crate) coalition: u8,
    pub(crate) snapshot: Snapshot,
    pub(crate) outcome: Option<Outcome>,
    pub(crate) violations: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Run {
    pub(crate) status: String,
    pub(crate) stop: String,
    pub(crate) completed_depth: u8,
    pub(crate) transitions: u64,
    pub(crate) completed_requests: u64,
    pub(crate) open_requests: u64,
    pub(crate) late_requests: u64,
    pub(crate) frontier: Vec<usize>,
    pub(crate) nodes: Vec<Node>,
    pub(crate) witnesses: BTreeMap<String, usize>,
    pub(crate) maximum_loss: i128,
    pub(crate) maximum_loss_node: Option<usize>,
}

pub(crate) fn search(model: &impl Model, limits: Limits) -> Run {
    let started = limits.wall_millis.map(|_| Instant::now());
    let starts = model.starts();
    let mut run = Run {
        status: "UNKNOWN".into(),
        stop: "not-started".into(),
        completed_depth: 0,
        transitions: 0,
        completed_requests: 0,
        open_requests: 0,
        late_requests: 0,
        frontier: Vec::new(),
        nodes: Vec::new(),
        witnesses: BTreeMap::new(),
        maximum_loss: 0,
        maximum_loss_node: None,
    };
    for (start, values) in starts.iter().enumerate() {
        for coalition in [1, 2, 3] {
            let violations = model.initial_violations(values);
            if run.nodes.len() >= limits.nodes {
                run.stop = "node-budget:initial-states".into();
                return stopped(run);
            }
            let id = run.nodes.len();
            for v in &violations {
                run.witnesses.entry(v.clone()).or_insert(id);
            }
            run.nodes.push(Node {
                parent: None,
                action: None,
                start,
                coalition,
                snapshot: Snapshot {
                    state: values.clone(),
                    reference: values.clone(),
                    pending: None,
                    time: 0,
                    stage: 0,
                    monitor: 0,
                },
                outcome: None,
                violations,
            });
            run.frontier.push(id);
        }
    }
    if run.nodes.is_empty() {
        run.stop = "unsupported-no-initial-states".into();
        return run;
    }
    for depth in 1..=limits.horizon {
        let mut next = Vec::new();
        let current = run.frontier.clone();
        for (position, id) in current.iter().copied().enumerate() {
            let node = run.nodes[id].clone();
            let actions = model.moves(&node.snapshot, node.coalition);
            // Empty alphabets cannot silently discharge pending obligations.
            if actions.is_empty() {
                run.stop = "unsupported-empty-alphabet".into();
                return stopped(run);
            }
            for (action_index, action) in actions.iter().enumerate() {
                let wall_expired = started
                    .zip(limits.wall_millis)
                    .is_some_and(|(t, ms)| t.elapsed() >= Duration::from_millis(ms));
                let stop = if wall_expired {
                    Some("wall-time-budget")
                } else if run.transitions >= limits.transitions {
                    Some("work-budget")
                } else if run.transitions >= limits.deadline_after_work {
                    Some("captured-deadline")
                } else if run.nodes.len() >= limits.nodes {
                    Some("node-budget")
                } else if next.len() >= limits.frontier {
                    Some("frontier-budget")
                } else {
                    None
                };
                if let Some(reason) = stop {
                    // Retain the whole unexpanded level as well as generated children.
                    // The current parent may have remaining moves; no depth is credited.
                    run.frontier = current[position..].iter().copied().chain(next).collect();
                    run.stop = format!("{reason}:parent={id}:move={action_index}");
                    return stopped(run);
                }
                run.transitions += 1; // charged on unsupported/refused evaluation too
                let checked =
                    match model.step(&starts[node.start], node.coalition, &node.snapshot, action) {
                        Ok(checked) => checked,
                        Err(reason) => {
                            run.frontier =
                                current[position..].iter().copied().chain(next).collect();
                            run.stop = format!("unsupported:{reason}");
                            return stopped(run);
                        }
                    };
                let child = run.nodes.len();
                let loss = model.loss(&checked.next, node.coalition);
                if loss > run.maximum_loss {
                    run.maximum_loss = loss;
                    run.maximum_loss_node = Some(child);
                }
                for v in &checked.violations {
                    run.witnesses.entry(v.clone()).or_insert(child);
                }
                run.completed_requests += checked.completed;
                run.nodes.push(Node {
                    parent: Some(id),
                    action: Some(action.clone()),
                    start: node.start,
                    coalition: node.coalition,
                    snapshot: checked.next,
                    outcome: Some(checked.outcome),
                    violations: checked.violations,
                });
                next.push(child);
            }
        }
        run.frontier = next;
        run.completed_depth = depth;
    }
    for id in &run.frontier {
        if let Some(request) = &run.nodes[*id].snapshot.pending {
            run.open_requests += 1;
            if request.submitted.saturating_add(2) > limits.horizon {
                run.late_requests += 1;
            }
        }
    }
    run.status = if run.witnesses.is_empty() {
        "checked-through-K"
    } else {
        "counterexample"
    }
    .into();
    run.stop = "exhausted-declared-prefixes".into();
    run
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Tiny {
        initial_bad: bool,
        unsupported_at: Option<i128>,
    }
    impl Model for Tiny {
        fn starts(&self) -> Vec<Vec<i128>> {
            vec![vec![0]]
        }
        fn initial_violations(&self, _: &[i128]) -> Vec<String> {
            if self.initial_bad {
                vec!["initial".into()]
            } else {
                vec![]
            }
        }
        fn moves(&self, _: &Snapshot, _: u8) -> Vec<Move> {
            vec![
                Move {
                    actor: 0,
                    request: vec![0],
                },
                Move {
                    actor: 0,
                    request: vec![1],
                },
            ]
        }
        fn step(
            &self,
            _: &[i128],
            _: u8,
            before: &Snapshot,
            action: &Move,
        ) -> Result<CheckedStep, String> {
            if self.unsupported_at == Some(before.state[0]) {
                return Err("successor-not-supported".into());
            }
            let mut next = before.clone();
            next.time += 1;
            next.stage = before.stage * 2 + u8::from(action.request[0] == 0);
            if action.request[0] == 0 {
                next.state[0] += 1;
            }
            Ok(CheckedStep {
                outcome: Outcome {
                    kind: if action.request[0] == 0 {
                        "Accept"
                    } else {
                        "Reject"
                    }
                    .into(),
                    reason: None,
                    component: None,
                    post: next.state.clone(),
                    events: vec![],
                },
                next,
                violations: vec![],
                completed: 0,
            })
        }
    }
    fn budget() -> Limits {
        Limits {
            horizon: 3,
            transitions: 1000,
            frontier: 1000,
            nodes: 1000,
            deadline_after_work: u64::MAX,
            wall_millis: None,
        }
    }
    #[test]
    fn every_prefix_reject_and_initial_are_counted() {
        let model = Tiny {
            initial_bad: false,
            unsupported_at: None,
        };
        let run = search(&model, budget());
        assert_eq!(run.transitions, 3 * (2 + 4 + 8));
        assert_eq!(run.nodes.len(), 45);
        assert_eq!(run.completed_depth, 3);
        assert_eq!(run.status, "checked-through-K");
        assert_eq!(run.frontier.len(), 24);
        assert!(run.nodes.iter().any(|n| n.snapshot.state == [3])); // no initial-domain clipping
        let bad = search(
            &Tiny {
                initial_bad: true,
                unsupported_at: None,
            },
            budget(),
        );
        assert!(replay(
            &Tiny {
                initial_bad: true,
                unsupported_at: None
            },
            &[0],
            1,
            &[],
            "initial"
        ));
        assert_eq!(bad.status, "counterexample");
    }
    #[test]
    fn incomplete_runs_never_pass_and_keep_work() {
        let model = Tiny {
            initial_bad: false,
            unsupported_at: None,
        };
        for b in [
            Limits {
                transitions: 7,
                ..budget()
            },
            Limits {
                nodes: 7,
                ..budget()
            },
            Limits {
                frontier: 7,
                ..budget()
            },
            Limits {
                deadline_after_work: 7,
                ..budget()
            },
        ] {
            let run = search(&model, b);
            assert_eq!(run.status, "UNKNOWN");
            assert!(run.completed_depth < 3);
            assert!(!run.frontier.is_empty());
            assert!(run.transitions > 0);
        }
        let run = search(
            &Tiny {
                initial_bad: false,
                unsupported_at: Some(1),
            },
            budget(),
        );
        assert_eq!(run.status, "UNKNOWN");
        assert!(run.stop.starts_with("unsupported:"));
        assert!(run.transitions > 6);
    }
    #[test]
    fn equal_visible_endpoints_keep_distinct_histories() {
        let run = search(
            &Tiny {
                initial_bad: false,
                unsupported_at: None,
            },
            budget(),
        );
        let same = run
            .nodes
            .iter()
            .filter(|n| n.snapshot.time == 3 && n.snapshot.state == [1])
            .count();
        assert_eq!(same, 9); // three histories for each coalition; none merged
        let monitors = run
            .nodes
            .iter()
            .filter(|n| n.snapshot.time == 3 && n.snapshot.state == [1])
            .map(|n| n.snapshot.stage)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(monitors.into_iter().collect::<Vec<_>>(), [1, 2, 4]);
    }
}

fn stopped(mut run: Run) -> Run {
    if !run.witnesses.is_empty() {
        run.status = "counterexample".into();
    }
    run
}

pub(crate) fn trace(run: &Run, mut node: usize) -> Vec<Move> {
    let mut result = Vec::new();
    loop {
        let current = &run.nodes[node];
        if let Some(action) = &current.action {
            result.push(action.clone());
        }
        match current.parent {
            Some(parent) => node = parent,
            None => break,
        }
    }
    result.reverse();
    result
}

// Witness proposals are always recomputed, including enabledness and deadlines.
pub(crate) fn replay(
    model: &impl Model,
    start: &[i128],
    coalition: u8,
    actions: &[Move],
    property: &str,
) -> bool {
    let mut state = Snapshot {
        state: start.to_vec(),
        reference: start.to_vec(),
        pending: None,
        time: 0,
        stage: 0,
        monitor: 0,
    };
    if actions.is_empty() {
        return model
            .initial_violations(start)
            .iter()
            .any(|p| p == property);
    }
    for (i, action) in actions.iter().enumerate() {
        if !model.moves(&state, coalition).contains(action) {
            return false;
        }
        let Ok(step) = model.step(start, coalition, &state, action) else {
            return false;
        };
        if i + 1 == actions.len() {
            return step.violations.iter().any(|p| p == property);
        }
        state = step.next;
    }
    false
}

pub(crate) fn minimize(
    model: &impl Model,
    start: &[i128],
    coalition: u8,
    original: &[Move],
    property: &str,
) -> Vec<Move> {
    let mut result = original.to_vec();
    // Delta debugging contiguous chunks, followed by single-field shrinking.
    loop {
        let before = result.clone();
        let mut width = result.len();
        while width > 0 {
            let mut at = 0;
            while at + width <= result.len() {
                let mut proposal = result.clone();
                proposal.drain(at..at + width);
                if replay(model, start, coalition, &proposal, property) {
                    result = proposal;
                } else {
                    at += 1;
                }
            }
            width -= 1;
        }
        for i in 0..result.len() {
            for field in 0..result[i].request.len() {
                let mut proposal = result.clone();
                proposal[i].request[field] = 0;
                if replay(model, start, coalition, &proposal, property) {
                    result = proposal;
                }
            }
        }
        if result == before {
            break;
        }
    }
    result
}
