//! Bounded counterexample search, never an equivalence or acceptance checker.
//!
//! Replays supplied traces first, then mutates typed actions around constants
//! and previously observed results. All observations use `Program::evaluate`.
//! A finding is reduced and replayed from the supplied initial state before
//! return. No finding within the budget is **inconclusive**, even for identical
//! programs. This module reads no ambient randomness, clock or external state.

use alloc::collections::BTreeSet;
use alloc::{format, vec, vec::Vec};
use zeno_fcis_synthesis::finite::{Domain, Error, Op, Program};

use crate::{Refusal, Result};

/// A sequence of actions. Each row supplies the non-state inputs in ABI order.
pub type Trace = Vec<Vec<i64>>;

/// An explicitly supplied initial value and its successor wiring.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct State {
    /// Input position occupied by this state value.
    pub input: usize,
    /// Output position that becomes its value on the next step.
    pub output: usize,
    /// Initial value; the caller must justify its relationship to app genesis.
    pub initial: i64,
}

/// Fixed, reproducible search bounds. Work includes corpus, reduction and replay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    /// Seed for the local deterministic mutation sequence.
    pub seed: u64,
    /// Maximum trials, including supplied traces and reduction attempts.
    pub trials: u32,
    /// Maximum evaluator pairs (one reference and one candidate call each).
    pub pairs: u32,
    /// Maximum actions per trace, including supplied traces.
    pub steps: usize,
    /// Maximum retained traces, including supplied traces.
    pub corpus: usize,
    /// Disable boundary and corpus guidance for an equal-budget baseline.
    pub uniform: bool,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            seed: 0,
            trials: 256,
            pairs: 4096,
            steps: 8,
            corpus: 64,
            uniform: false,
        }
    }
}

/// The actual evaluator observation, including eager failures.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Observation {
    /// Complete outputs in ABI order.
    Values(Vec<i64>),
    /// Checked arithmetic failed.
    Arithmetic,
    /// The evaluator rejected an output outside its declared domain.
    OutputDomain,
}

/// A concrete difference, replayed from the invocation's supplied initial state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Witness {
    /// Actions through the first difference; also usable as a future seed.
    pub trace: Trace,
    /// Zero-based index of the differing step.
    pub step: usize,
    /// Full input, including reconstructed state, at that step.
    pub input: Vec<i64>,
    /// Actual reference observation.
    pub reference: Observation,
    /// Actual candidate observation.
    pub candidate: Observation,
}

/// Screening evidence, not a transform receipt or an authorization capability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Report {
    /// `None` means inconclusive, never equivalent or correct.
    pub witness: Option<Witness>,
    /// Trials spent, including reduction attempts and the confirming replay.
    pub trials: u32,
    /// Evaluation pairs spent, including reduction and the confirming replay.
    pub pairs: u32,
    /// Bounded seeds that can be replayed against a subsequent candidate.
    pub corpus: Vec<Trace>,
}

fn observe(program: &Program, input: &[i64]) -> Result<Observation> {
    match program.evaluate(input) {
        Ok(v) => Ok(Observation::Values(v)),
        Err(Error::Arithmetic) => Ok(Observation::Arithmetic),
        Err(Error::Invalid("output-domain")) => Ok(Observation::OutputDomain),
        Err(e) => Err(Refusal::new(format!("fuzz-evaluator:{e:?}"))),
    }
}

struct Runner<'a> {
    reference: &'a Program,
    candidate: &'a Program,
    state: &'a [State],
    action_positions: Vec<usize>,
    action_domains: Vec<Domain>,
    limits: Limits,
    trials: u32,
    pairs: u32,
}

impl Runner<'_> {
    // Reserve a whole second replay before starting a potentially failing trace.
    // Reduction similarly reserves the current witness's complete replay cost.
    fn room(&self, trace: &Trace, reserve: usize) -> bool {
        self.trials + 1 < self.limits.trials
            && (trace.len() + reserve) as u64 <= u64::from(self.limits.pairs - self.pairs)
    }

    fn run(&mut self, trace: &Trace) -> Result<(Option<Witness>, Observation)> {
        self.trials += 1;
        let mut input: Vec<i64> = self
            .reference
            .inputs()
            .iter()
            .map(|d| d.bounds().0)
            .collect();
        for s in self.state {
            input[s.input] = s.initial;
        }
        let mut last = Observation::Values(Vec::new());
        for (step, action) in trace.iter().enumerate() {
            for (&pos, &value) in self.action_positions.iter().zip(action) {
                input[pos] = value;
            }
            self.pairs += 1;
            let reference = observe(self.reference, &input)?;
            let candidate = observe(self.candidate, &input)?;
            if reference != candidate {
                return Ok((
                    Some(Witness {
                        trace: trace[..=step].to_vec(),
                        step,
                        input,
                        reference,
                        candidate,
                    }),
                    last,
                ));
            }
            if let Observation::Values(values) = &reference {
                for s in self.state {
                    input[s.input] = values[s.output];
                }
            } else {
                // An equal trap terminates both executions, with no successor.
                return Ok((None, reference));
            }
            last = reference;
        }
        Ok((None, last))
    }

    fn finish(mut self, mut witness: Witness, mut corpus: Vec<Trace>) -> Result<Report> {
        // Greedy deletion; no global or local minimum is promised on budget stop.
        let mut k = 0;
        while k < witness.trace.len() && witness.trace.len() > 1 {
            let mut shorter = witness.trace.clone();
            shorter.remove(k);
            if !self.room(&shorter, witness.trace.len()) {
                break;
            }
            if let (Some(w), _) = self.run(&shorter)? {
                witness = w;
                k = 0;
            } else {
                k += 1;
            }
        }
        // Simplify action values toward zero while preserving a real difference.
        for step in 0..witness.trace.len() {
            for col in 0..self.action_domains.len() {
                let (lo, hi) = self.action_domains[col].bounds();
                let simple = 0i64.clamp(lo, hi);
                if witness.trace[step][col] == simple {
                    continue;
                }
                let mut trace = witness.trace.clone();
                trace[step][col] = simple;
                if !self.room(&trace, witness.trace.len()) {
                    break;
                }
                if let (Some(w), _) = self.run(&trace)? {
                    witness = w;
                }
                // A new first mismatch can shorten the trace.
                if step >= witness.trace.len() {
                    break;
                }
            }
            if step + 1 >= witness.trace.len() {
                break;
            }
        }
        // The reservation above guarantees this replay fits the total budget.
        let (confirmed, _) = self.run(&witness.trace)?;
        let confirmed = confirmed.ok_or_else(|| Refusal::new("fuzz-witness-did-not-replay"))?;
        if confirmed != witness {
            return Err(Refusal::new("fuzz-witness-changed-on-replay"));
        }
        if !corpus.contains(&witness.trace) {
            if corpus.len() == self.limits.corpus {
                corpus.pop();
            }
            corpus.push(witness.trace.clone());
        }
        Ok(Report {
            witness: Some(witness),
            trials: self.trials,
            pairs: self.pairs,
            corpus,
        })
    }
}

// SplitMix64 is a reproducible mutation source, not cryptography or ambient RNG.
fn random(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut n = *seed;
    n = (n ^ (n >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    n = (n ^ (n >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    n ^ (n >> 31)
}

fn index(seed: &mut u64, len: usize) -> usize {
    (u128::from(random(seed)) % len as u128) as usize
}

fn value(seed: &mut u64, domain: Domain) -> i64 {
    let (lo, hi) = domain.bounds();
    let width = (i128::from(hi) - i128::from(lo) + 1) as u128;
    (i128::from(lo) + (u128::from(random(seed)) % width) as i128) as i64
}

fn boundaries(domain: Domain, constants: &[i64]) -> Vec<i64> {
    let (lo, hi) = domain.bounds();
    let mut out = vec![lo, hi, 0i64.clamp(lo, hi)];
    for c in constants.iter().take(64).chain([&-1, &1]) {
        for v in [c.checked_sub(1), Some(*c), c.checked_add(1)]
            .into_iter()
            .flatten()
        {
            if domain.contains(v) {
                out.push(v);
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Screen a fixed reference/candidate pair without enumerating its input domain.
///
/// The profile is exact checked-i64 `Program::evaluate` observations. State
/// wiring only determines which action traces are explored; it does not qualify
/// app genesis, temporal properties, Authority publications or external effects.
/// Both programs and every supplied seed are validated before any evaluation.
pub fn screen(
    reference: &Program,
    candidate: &Program,
    state: &[State],
    seeds: &[Trace],
    limits: Limits,
) -> Result<Report> {
    if reference.inputs() != candidate.inputs() || reference.outputs() != candidate.outputs() {
        return Err(Refusal::new("fuzz-program-abi"));
    }
    if limits.trials < 2
        || limits.trials > 10_000
        || limits.pairs < 2
        || limits.pairs > 100_000
        || limits.steps == 0
        || limits.steps > 64
        || limits.corpus == 0
        || limits.corpus > 256
    {
        return Err(Refusal::new("fuzz-limits"));
    }
    if seeds.len() > limits.corpus || state.len() > reference.inputs().len() {
        return Err(Refusal::new("fuzz-corpus-or-state-size"));
    }
    let mut occupied = BTreeSet::new();
    for s in state {
        let input = reference
            .inputs()
            .get(s.input)
            .ok_or_else(|| Refusal::new("fuzz-state-input"))?;
        if !occupied.insert(s.input)
            || reference.outputs().get(s.output) != Some(input)
            || !input.contains(s.initial)
        {
            return Err(Refusal::new("fuzz-state-binding"));
        }
    }
    let positions: Vec<_> = (0..reference.inputs().len())
        .filter(|p| !occupied.contains(p))
        .collect();
    let domains: Vec<_> = positions.iter().map(|p| reference.inputs()[*p]).collect();
    for trace in seeds {
        if trace.is_empty()
            || trace.len() > limits.steps
            || trace.iter().any(|row| {
                row.len() != domains.len() || row.iter().zip(&domains).any(|(v, d)| !d.contains(*v))
            })
        {
            return Err(Refusal::new("fuzz-seed-domain-or-shape"));
        }
    }
    let mut runner = Runner {
        reference,
        candidate,
        state,
        action_positions: positions,
        action_domains: domains,
        limits,
        trials: 0,
        pairs: 0,
    };
    let mut corpus = seeds.to_vec();
    for trace in seeds {
        if !runner.room(trace, trace.len()) {
            return Ok(Report {
                witness: None,
                trials: runner.trials,
                pairs: runner.pairs,
                corpus,
            });
        }
        if let (Some(w), _) = runner.run(trace)? {
            return runner.finish(w, corpus);
        }
    }
    let constants: Vec<_> = reference
        .nodes()
        .iter()
        .chain(candidate.nodes())
        .filter_map(|op| if let Op::Int(v) = op { Some(*v) } else { None })
        .take(64)
        .collect();
    let bounds: Vec<_> = runner
        .action_domains
        .iter()
        .map(|d| boundaries(*d, &constants))
        .collect();
    let mut seed = limits.seed;
    let mut seen = BTreeSet::new();
    while runner.trials + 1 < limits.trials {
        let trial = runner.trials as usize;
        let uniform = limits.uniform || trial % 4 == 3;
        let mut trace = if uniform || corpus.is_empty() {
            let len = if state.is_empty() {
                1
            } else {
                1 + index(&mut seed, limits.steps)
            };
            (0..len)
                .map(|_| {
                    runner
                        .action_domains
                        .iter()
                        .enumerate()
                        .map(|(col, d)| {
                            if uniform {
                                value(&mut seed, *d)
                            } else {
                                bounds[col][index(&mut seed, bounds[col].len())]
                            }
                        })
                        .collect()
                })
                .collect::<Trace>()
        } else {
            corpus[index(&mut seed, corpus.len())].clone()
        };
        if !uniform && !trace.is_empty() {
            let step = index(&mut seed, trace.len());
            match trial % 3 {
                0 if trace.len() < limits.steps => {
                    let row = trace[step].clone();
                    trace.push(row);
                }
                1 if trace.len() > 1 => {
                    trace.remove(step);
                }
                _ => {}
            }
            if !runner.action_domains.is_empty() {
                let step = index(&mut seed, trace.len());
                let col = index(&mut seed, bounds.len());
                trace[step][col] = bounds[col][index(&mut seed, bounds[col].len())];
            }
        }
        if !runner.room(&trace, trace.len()) {
            break;
        }
        let (witness, last) = runner.run(&trace)?;
        if let Some(w) = witness {
            return runner.finish(w, corpus);
        }
        if corpus.len() < limits.corpus && seen.insert(last) && !corpus.contains(&trace) {
            corpus.push(trace);
        }
    }
    Ok(Report {
        witness: None,
        trials: runner.trials,
        pairs: runner.pairs,
        corpus,
    })
}
