//! Untrusted search for smaller equivalent ZenoFCIS finite decision programs.
//!
//! The search proposes; it never judges. Four proposers work over
//! *signatures*: the value of every node on every declared input tuple on
//! which the original does not trap, in `ordered-product-last-input-fastest-v1`
//! order. (On a tuple where the original traps, nodes have no values; a
//! candidate must trap there too, which the final comparison checks.)
//!
//! - **don't-care resubstitution** replaces a node by an existing node, a
//!   constant, or one new operation over existing nodes and new constants,
//!   using the tuples on which the node's value cannot reach an output;
//! - **top-down synthesis** rebuilds each output with case splits, arithmetic
//!   and equality inversion and two-valued selection, counterexample-guided on
//!   a seeded, stratified sample above `cegis_tuples` tuples;
//! - **decision-list folding** rewrites `(a && b) ? x : (a ? y : z)`;
//! - **decision-diagram resynthesis** rebuilds outputs with few values as
//!   reduced decision diagrams over input variable orders, on small domains
//!   where the original never traps.
//!
//! A portfolio, which may also start from supplied seed programs, runs them
//! in rounds until a round improves nothing, keeping every admitted program
//! and choosing the smallest by (instructions, canonical bytes). No result is
//! claimed to be the smallest program that exists.
//!
//! # What a result means
//!
//! Node values, observations and admission come from the library: programs
//! are imported with `finite_runtime::import_program`, observed with
//! `Program::execute_v2` at the full Step budget (as `transform check` does),
//! node values are read through `Program::evaluate` on probe programs, and
//! bytes are the library's canonical encoding. Eager semantics are therefore
//! the library's own: every node runs, `Add` and `Sub` trap on overflow
//! (including in unused nodes and unselected `Select` arms), and an output
//! outside its declared domain is a refusal. A tuple on which the original
//! traps has no node values; a candidate must trap there too.
//!
//! A returned candidate has been compared with the original on every tuple of
//! the declared domain by that library evaluator, and has the same input and
//! output domains. This is the search's own consistency check, not a
//! judgement: the result carries no authority, and only `zeno-fcis transform
//! check` (with its receipt and `transform replay`) can accept it. Adoption
//! remains `zeno-fcis contract adopt`, a human action.
//!
//! # Budgets
//!
//! [`Budget`] bounds the domain size, the memory for signatures and plans,
//! the work units and the rounds. Over any budget the search returns
//! [`Outcome::Stopped`] with a typed reason and no candidate, never a partial
//! claim. Results are deterministic: the same bytes and settings give the same
//! result, and the crate reads no clock, file, environment or randomness.
//!
//! # Provenance
//!
//! This crate is LLM-generated. It ports to Rust the methods of an earlier
//! Python prototype by the same owner, who relicensed that work for this crate
//! under the workspace licence (MIT OR Apache-2.0).

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod arithmetic;
mod diagram;
mod fold;
mod ir;
mod pool;
mod resub;
mod space;
mod topdown;
mod util;

/// Care-masked top-down synthesis of a supplied target, for proposers outside
/// this crate.
pub mod target;

// Composable loops: concrete-term proposals reuse this crate's graph and pool.
pub mod concrete;

#[cfg(test)]
mod tests;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;
use zeno_fcis_synthesis::finite::{Error, Program};
use zeno_fcis_synthesis::finite_runtime::import_program;

pub use crate::diagram::DiagramReport;
use crate::ir::{Cost, Graph, encode};
use crate::space::{Meter, Reference, Space, Verdict};

/// Most supplied seed programs one search accepts (as `optimize`).
pub const MAX_SEEDS: usize = 8;

/// Largest top-down depth. Synthesis recurses once per level, so the clamp
/// bounds the stack whatever depth is requested.
pub const MAX_DEPTH: u32 = 16;

/// Proposer name for single resubstitution rewrites refused as they were
/// accepted.
const RESUB_STEP: &str = "resub-step";

/// Proposer name for decision-diagram resynthesis.
const DIAGRAM: &str = "dd-resynth";

/// Explicit resource limits. Exceeding any of them stops the search with no
/// candidate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Budget {
    /// Largest declared input domain the search enumerates.
    pub max_tuples: u64,
    /// Most `i64` cells in each of three tables: the original's observations,
    /// a program's node signatures, and the top-down plan values. Each table
    /// is limited separately, so together they can hold up to three times
    /// this; smaller buffers are not counted.
    pub max_cells: u64,
    /// Work units: one per evaluated instruction per tuple, and one per value
    /// examined by a proposer.
    pub work_units: u64,
    /// Most portfolio rounds after the initial top-down proposal.
    pub rounds: u32,
    /// Most accepted rewrites in one resubstitution run.
    pub resub_rounds: u32,
    /// Largest domain on which each resubstitution rewrite is checked as it is
    /// accepted; above it only the portfolio's final check applies.
    pub step_verify_tuples: u64,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_tuples: 1 << 22,
            max_cells: 1 << 26,
            work_units: 1 << 36,
            rounds: 6,
            resub_rounds: 64,
            step_verify_tuples: 1 << 16,
        }
    }
}

/// Which proposers and rewrites run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Proposers {
    /// Top-down synthesis of each output.
    pub top_down: bool,
    /// Don't-care resubstitution.
    pub resub: bool,
    /// Decision-list folding.
    pub fold: bool,
    /// The `K - x` replacement inside resubstitution.
    pub complement: bool,
    /// Two-valued selection inside top-down synthesis.
    pub two_valued: bool,
    /// Decision-diagram resynthesis over variable orders.
    pub diagram: bool,
}

impl Default for Proposers {
    fn default() -> Self {
        Self {
            top_down: true,
            resub: true,
            fold: true,
            complement: true,
            two_valued: true,
            diagram: true,
        }
    }
}

/// Search-shape parameters. They change what is found, never what a result
/// means.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Shape {
    /// Top-down depth; by default 8, 6 or 5 as the tuple count grows. Any
    /// value is clamped to [`MAX_DEPTH`].
    pub depth: Option<u32>,
    /// Case-split predicates tried per subproblem.
    pub width: usize,
    /// Subproblems solved per output before only constants and atoms remain.
    pub solve_limit: u32,
    /// 0 limits resubstitution to existing nodes and constants; any other
    /// value also allows one new operation over existing nodes and new
    /// constants.
    pub max_new: u32,
    /// Best proposals tried per resubstitution round.
    pub resub_choices: usize,
    /// Above this many live tuples, top-down synthesis is counterexample-guided.
    pub cegis_tuples: usize,
    /// Initial sample size for counterexample-guided synthesis.
    pub sample: usize,
    /// Counterexample-guided rounds.
    pub cegis_rounds: u32,
    /// Seed of the sampling sequence.
    pub seed: u64,
    /// Best programs each portfolio round starts from, besides the best
    /// program of every proposer.
    pub portfolio_seeds: usize,
    /// Variable orders the decision-diagram proposer tries, in lexicographic
    /// order.
    pub diagram_orders: usize,
    /// Largest input domain (values per input) the diagram proposer accepts.
    pub diagram_cardinality: u64,
    /// Largest number of tuples the diagram proposer accepts.
    pub diagram_tuples: usize,
    /// Diagram nodes one order may build before it is abandoned.
    pub diagram_nodes: usize,
    /// Smallest distinct diagram programs offered to the portfolio.
    pub diagram_candidates: usize,
}

impl Default for Shape {
    fn default() -> Self {
        Self {
            depth: None,
            width: 6,
            solve_limit: 20_000,
            max_new: 2,
            resub_choices: 64,
            cegis_tuples: 1 << 15,
            sample: 4096,
            cegis_rounds: 8,
            seed: 0,
            portfolio_seeds: 3,
            diagram_orders: 120,
            diagram_cardinality: 16,
            diagram_tuples: 8192,
            diagram_nodes: 16_384,
            diagram_candidates: 8,
        }
    }
}

/// Every setting of one search.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Settings {
    /// Resource limits.
    pub budget: Budget,
    /// Proposers and rewrites.
    pub proposers: Proposers,
    /// Search shape.
    pub shape: Shape,
}

/// Why a search returned no result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Stop {
    /// The library importer refused the original program.
    NotAdmitted {
        /// The library's diagnostic code.
        code: &'static str,
    },
    /// Some input domain is empty.
    EmptyInputDomain,
    /// The declared domain has more tuples than `max_tuples`.
    DomainOverBudget {
        /// Declared tuples (saturating).
        tuples: u128,
        /// The limit.
        limit: u64,
    },
    /// Observations, signatures or plans would exceed `max_cells`.
    CellsOverBudget {
        /// Cells required so far (saturating).
        cells: u128,
        /// The limit.
        limit: u64,
    },
    /// The work units ran out.
    WorkBudget {
        /// The limit.
        limit: u64,
    },
    /// More than [`MAX_SEEDS`] seed programs were supplied.
    TooManySeeds {
        /// The limit.
        limit: usize,
    },
    /// An internal consistency check failed; nothing is claimed.
    Inconsistent {
        /// Which check.
        code: &'static str,
    },
}

/// One improvement of the incumbent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Step {
    /// Which proposer produced it.
    pub proposer: &'static str,
    /// Portfolio round (0 for the original, seeds and top-down).
    pub round: u32,
    /// Instructions.
    pub nodes: usize,
    /// Canonical bytes.
    pub bytes: usize,
}

/// What one proposer offered and what became of it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Offers {
    /// Proposer name.
    pub proposer: &'static str,
    /// Offers, including empty ones.
    pub offered: u32,
    /// Offers with no program.
    pub empty: u32,
    /// Programs already admitted or already refused.
    pub duplicate: u32,
    /// Programs that agreed with the original on every tuple.
    pub admitted: u32,
    /// Programs that did not.
    pub refused: u32,
}

/// Why a proposed program was refused by the search's own check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refusal {
    /// The first tuple (by ordinal) where the observations differ.
    Differs {
        /// Ordinal in the declared enumeration order.
        ordinal: u64,
    },
    /// Different input or output domains.
    Abi,
    /// The library refused the program at admission.
    NotAdmitted,
}

/// One refused proposal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Refused {
    /// Which proposer offered it.
    pub proposer: &'static str,
    /// Portfolio round.
    pub round: u32,
    /// Why.
    pub refusal: Refusal,
}

/// Untrusted account of a finished search.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Provenance {
    /// Tuples in the declared domain.
    pub domain_tuples: u64,
    /// Tuples on which the original's nodes have no values: it refuses with
    /// an arithmetic trap (or any refusal other than an output-domain one).
    pub trapping_tuples: u64,
    /// Portfolio rounds run.
    pub rounds: u32,
    /// Work units used.
    pub work_used: u64,
    /// Proposer of the returned program (`original` when unimproved).
    pub winner: &'static str,
    /// Round in which it was admitted.
    pub winner_round: u32,
    /// Each improvement of the incumbent, starting with the original.
    pub trail: Vec<Step>,
    /// Per proposer, in name order.
    pub offers: Vec<Offers>,
    /// Every refused proposal, in order.
    pub refused: Vec<Refused>,
    /// The decision-diagram proposer's account, when it was enabled.
    pub diagram: Option<DiagramReport>,
}

/// A smaller program that agreed with the original on every tuple.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Candidate {
    /// Canonical program bytes (`program.zcve` encoding).
    pub bytes: Vec<u8>,
    /// Instructions.
    pub nodes: usize,
    /// How it was found.
    pub provenance: Provenance,
}

/// The result of one search.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outcome {
    /// A program strictly smaller by (instructions, bytes).
    Improved(Candidate),
    /// No strictly smaller program was found within the budget.
    Unimproved(Provenance),
    /// A budget or an input refusal stopped the search; nothing is claimed.
    Stopped(Stop),
}

/// Shared read-only context of one search.
pub(crate) struct Env<'a> {
    pub(crate) reference: &'a Reference,
    pub(crate) space: &'a Space,
    pub(crate) settings: &'a Settings,
}

/// Library admission, canonical bytes and cost of a search graph.
pub(crate) fn admit(graph: &Graph) -> Option<((Program, Vec<u8>), Cost)> {
    let program = graph.to_program()?;
    let bytes = encode(&program)?;
    let cost = (program.nodes().len(), bytes.len());
    Some(((program, bytes), cost))
}

struct Entry {
    graph: Graph,
    bytes: Vec<u8>,
    cost: Cost,
    proposer: &'static str,
    round: u32,
}

/// Every admitted program, in admission order; the original is the reference
/// and is admitted without a check.
struct Archive {
    entries: Vec<Entry>,
    refused_bytes: BTreeSet<Vec<u8>>,
    trail: Vec<Step>,
    offers: BTreeMap<&'static str, Offers>,
    refused: Vec<Refused>,
}

impl Archive {
    fn new(graph: Graph, bytes: Vec<u8>, cost: Cost) -> Self {
        let trail = alloc::vec![Step {
            proposer: "original",
            round: 0,
            nodes: cost.0,
            bytes: cost.1,
        }];
        Self {
            entries: alloc::vec![Entry {
                graph,
                bytes,
                cost,
                proposer: "original",
                round: 0,
            }],
            refused_bytes: BTreeSet::new(),
            trail,
            offers: BTreeMap::new(),
            refused: Vec::new(),
        }
    }

    fn tally(&mut self, proposer: &'static str) -> &mut Offers {
        self.offers.entry(proposer).or_insert_with(|| Offers {
            proposer,
            ..Offers::default()
        })
    }

    fn refuse(&mut self, proposer: &'static str, round: u32, refusal: Refusal) {
        self.tally(proposer).refused += 1;
        self.refused.push(Refused {
            proposer,
            round,
            refusal,
        });
    }

    fn best(&self) -> usize {
        let mut best = 0;
        for (index, entry) in self.entries.iter().enumerate() {
            let current = &self.entries[best];
            if (entry.cost, entry.round) < (current.cost, current.round) {
                best = index;
            }
        }
        best
    }

    fn best_cost(&self) -> Cost {
        self.entries[self.best()].cost
    }

    /// The `count` best programs, then the best program of every other
    /// proposer (the original included), in rank order. A proposer whose
    /// programs are all outranked still contributes a starting point, so
    /// adding a proposer never removes another proposer's start.
    fn starts(&self, count: usize) -> Vec<Graph> {
        let mut order: Vec<usize> = (0..self.entries.len()).collect();
        order.sort_by_key(|index| (self.entries[*index].cost, self.entries[*index].round));
        let mut covered = BTreeSet::new();
        let mut chosen = Vec::new();
        for (rank, index) in order.into_iter().enumerate() {
            let entry = &self.entries[index];
            if rank < count || covered.insert(entry.proposer) {
                covered.insert(entry.proposer);
                chosen.push(entry.graph.clone());
            }
        }
        chosen
    }

    fn offer(
        &mut self,
        graph: Option<Graph>,
        proposer: &'static str,
        round: u32,
        env: &Env<'_>,
        meter: &mut Meter,
    ) -> Result<(), Stop> {
        self.tally(proposer).offered += 1;
        let Some(graph) = graph else {
            self.tally(proposer).empty += 1;
            return Ok(());
        };
        let Some(((program, bytes), cost)) = admit(&graph) else {
            self.refuse(proposer, round, Refusal::NotAdmitted);
            return Ok(());
        };
        if self.refused_bytes.contains(&bytes) || self.entries.iter().any(|e| e.bytes == bytes) {
            self.tally(proposer).duplicate += 1;
            return Ok(());
        }
        let verdict = env.reference.compare(&program, env.space, None, meter)?;
        if let Some(refusal) = refusal(verdict) {
            self.refused_bytes.insert(bytes);
            self.refuse(proposer, round, refusal);
            return Ok(());
        }
        let before = self.best_cost();
        self.tally(proposer).admitted += 1;
        if cost < before {
            self.trail.push(Step {
                proposer,
                round,
                nodes: cost.0,
                bytes: cost.1,
            });
        }
        self.entries.push(Entry {
            graph,
            bytes,
            cost,
            proposer,
            round,
        });
        Ok(())
    }
}

fn refusal(verdict: Verdict) -> Option<Refusal> {
    match verdict {
        Verdict::Equal => None,
        Verdict::Abi => Some(Refusal::Abi),
        Verdict::Differs(ordinal) => Some(Refusal::Differs { ordinal }),
    }
}

fn error_code(error: &Error) -> &'static str {
    match *error {
        Error::Invalid(code) | Error::Limit(code) => code,
        _ => "unclassified",
    }
}

/// Searches for a program strictly smaller than `original` (canonical program
/// bytes) that agrees with it on every declared input tuple.
///
/// `seeds` are further canonical programs claimed equivalent to the original,
/// for example the output of `zeno-fcis optimize`; each is checked like every
/// proposal, and a refused or malformed seed is only counted.
#[must_use]
pub fn search(original: &[u8], seeds: &[&[u8]], settings: &Settings) -> Outcome {
    match run(original, seeds, settings) {
        Ok(outcome) => outcome,
        Err(stop) => Outcome::Stopped(stop),
    }
}

fn run(original: &[u8], seeds: &[&[u8]], settings: &Settings) -> Result<Outcome, Stop> {
    if seeds.len() > MAX_SEEDS {
        return Err(Stop::TooManySeeds { limit: MAX_SEEDS });
    }
    let program = import_program(original).map_err(|error| Stop::NotAdmitted {
        code: error_code(&error),
    })?;
    let graph = Graph::from_program(&program).map_err(|code| Stop::NotAdmitted { code })?;
    let budget = &settings.budget;
    let space = Space::new(program.inputs(), budget.max_tuples)?;
    let mut meter = Meter::new(budget.work_units);
    let reference = Reference::new(&program, &space, budget.max_cells, &mut meter)?;
    let env = Env {
        reference: &reference,
        space: &space,
        settings,
    };
    let cost = (program.nodes().len(), original.len());
    let mut archive = Archive::new(graph.clone(), original.to_vec(), cost);
    for seed in seeds {
        let imported = import_program(seed)
            .ok()
            .and_then(|seed| Graph::from_program(&seed).ok());
        match imported {
            Some(seed) => archive.offer(Some(seed), "seed", 0, &env, &mut meter)?,
            None => {
                archive.tally("seed").offered += 1;
                archive.refuse("seed", 0, Refusal::NotAdmitted);
            }
        }
    }
    let proposers = &settings.proposers;
    let mut diagram_report = None;
    if (proposers.top_down || proposers.diagram)
        && let Some(sig) = reference.signatures(&graph, &space, budget.max_cells, &mut meter)?
    {
        if proposers.top_down {
            let proposal = topdown::top_down(&graph, &sig, &env, &mut meter)?;
            archive.offer(proposal, "top-down", 0, &env, &mut meter)?;
        }
        if proposers.diagram {
            let (proposals, report) = diagram::propose(&graph, &sig, &env, &mut meter)?;
            for proposal in proposals {
                archive.offer(Some(proposal), DIAGRAM, 0, &env, &mut meter)?;
            }
            diagram_report = Some(report);
        }
    }
    let mut rounds = 0;
    for round in 1..=budget.rounds {
        rounds = round;
        let before = archive.best_cost();
        let starts = archive.starts(settings.shape.portfolio_seeds);
        if proposers.fold {
            for start in &starts {
                let folded = fold::fold_decision_lists(start);
                archive.offer(Some(folded), "fold", round, &env, &mut meter)?;
            }
        }
        if proposers.resub {
            for start in &starts {
                let (improved, steps) = resub::resubstitute(start, &env, &mut meter)?;
                // Rewrites the step check refused are counted, not dropped.
                for verdict in steps {
                    if let Some(refused) = refusal(verdict) {
                        archive.tally(RESUB_STEP).offered += 1;
                        archive.refuse(RESUB_STEP, round, refused);
                    }
                }
                archive.offer(Some(improved), "resub", round, &env, &mut meter)?;
            }
        }
        if archive.best_cost() >= before {
            break;
        }
    }
    let best = &archive.entries[archive.best()];
    let provenance = Provenance {
        domain_tuples: space.count() as u64,
        trapping_tuples: reference.trapping() as u64,
        rounds,
        work_used: meter.used(),
        winner: best.proposer,
        winner_round: best.round,
        trail: archive.trail.clone(),
        offers: archive.offers.values().cloned().collect(),
        refused: archive.refused.clone(),
        diagram: diagram_report,
    };
    if best.cost >= cost {
        return Ok(Outcome::Unimproved(provenance));
    }
    // The bytes must read back through the library importer unchanged.
    let round_trip = import_program(&best.bytes)
        .ok()
        .and_then(|program| encode(&program));
    if round_trip.as_deref() != Some(best.bytes.as_slice()) {
        return Err(Stop::Inconsistent {
            code: "candidate-round-trip",
        });
    }
    Ok(Outcome::Improved(Candidate {
        bytes: best.bytes.clone(),
        nodes: best.cost.0,
        provenance,
    }))
}
