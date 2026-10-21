//! Test-only archived oracle control; this is not a third native integration.
use super::engine::*;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Deserialize)]
pub(crate) struct Input {
    pub(crate) worlds: Vec<Vec<i128>>,
    pub(crate) edges: Vec<[usize; 3]>,
    pub(crate) provenance: Value,
}

pub(crate) struct Control {
    input: Input,
    edges: BTreeMap<(usize, usize), usize>,
    pub(crate) disabled_objective: Option<String>,
    pub(crate) only_objective: Option<String>,
}

impl Control {
    pub(crate) fn new(input: Input) -> Result<Self, String> {
        if input.worlds.is_empty()
            || input.worlds.len() > 4096
            || input.edges.len() > 16384
            || input.worlds.iter().any(|w| {
                w.len() != 15 || w[2] <= 0 || w.iter().any(|v| !(0..=1_000_000_000).contains(v))
            })
        {
            return Err("spot-control-domain".into());
        }
        let mut edges = BTreeMap::new();
        for [state, command, post] in &input.edges {
            if *state >= input.worlds.len()
                || *post >= input.worlds.len()
                || *command >= 4
                || edges.insert((*state, *command), *post).is_some()
            {
                return Err("spot-control-edge".into());
            }
        }
        Ok(Self {
            input,
            edges,
            disabled_objective: None,
            only_objective: None,
        })
    }
    pub(crate) fn packet(&self) -> Value {
        json!({"worlds":self.input.worlds,"edges":self.input.edges,"provenance":self.input.provenance,"oracle_only":true,"only_objective":self.only_objective,"disabled_objective":self.disabled_objective})
    }
    fn wealth(&self, state: usize, coalition: u8, liquid: bool) -> [i128; 2] {
        let world = &self.input.worlds[state];
        let mut wealth = [0, 0];
        for actor in 0..2 {
            if coalition & (1 << actor) != 0 {
                for asset in 0..2 {
                    wealth[asset] += world[8 + 2 * actor + asset];
                    if !liquid {
                        wealth[asset] += world[12 + actor] * world[asset] / world[2];
                    }
                }
            }
        }
        wealth
    }
}

impl Model for Control {
    fn loss(&self, snapshot: &Snapshot, coalition: u8) -> i128 {
        let Ok(state) = usize::try_from(snapshot.state[0]) else {
            return 0;
        };
        let before = self.wealth(0, coalition, false);
        let after = self.wealth(state, coalition, false);
        (after[0] + after[1] - before[0] - before[1]).max(0)
    }
    fn starts(&self) -> Vec<Vec<i128>> {
        vec![vec![0]]
    }
    fn initial_violations(&self, _: &[i128]) -> Vec<String> {
        vec![]
    }
    fn moves(&self, _: &Snapshot, coalition: u8) -> Vec<Move> {
        (0..4)
            .filter(|command| coalition & (1 << (command / 2)) != 0)
            .map(|command| Move {
                actor: (command / 2) as u8,
                request: vec![command],
            })
            .collect()
    }
    fn step(
        &self,
        _: &[i128],
        coalition: u8,
        snapshot: &Snapshot,
        action: &Move,
    ) -> Result<CheckedStep, String> {
        let state = usize::try_from(snapshot.state[0]).map_err(|_| "spot-state")?;
        let command = usize::try_from(action.request[0]).map_err(|_| "spot-command")?;
        let post = *self
            .edges
            .get(&(state, command))
            .ok_or("unsupported-spot-successor")?;
        let mut violations = vec![];
        {
            let before = self.wealth(0, coalition, false);
            let after = self.wealth(post, coalition, false);
            if after.iter().zip(before).all(|(a, b)| *a >= b) && after != before {
                violations.push("coalition-gain".into());
            }
            let before = self.wealth(0, coalition, true);
            let after = self.wealth(post, coalition, true);
            let closed = (0..2).all(|actor| {
                coalition & (1 << actor) == 0 || self.input.worlds[post][12 + actor] == 0
            });
            if closed && after.iter().zip(before).all(|(a, b)| *a >= b) && after != before {
                violations.push("realized-coalition-gain".into());
            }
        }
        let mut next = snapshot.clone();
        violations.retain(|v| {
            self.only_objective.as_ref().is_none_or(|p| p == v)
                && self.disabled_objective.as_ref().is_none_or(|p| p != v)
        });
        next.state = vec![post as i128];
        next.time += 1;
        Ok(CheckedStep {
            next,
            violations,
            outcome: Outcome {
                kind: "oracle-only".into(),
                reason: None,
                component: None,
                post: vec![post as i128],
                events: vec![],
            },
            completed: 0,
        })
    }
}
