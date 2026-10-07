//! Pure optimizer for `zeno-fcis optimize`: an in-house e-graph proposes
//! smaller programs, and F3's exhaustive checker judges every one of them.
//!
//! The optimizer is an untrusted proposer. Nothing it believes about a
//! candidate counts: a candidate is reported as accepted only when
//! [`transform::check`] has compared it with the original on every tuple of
//! the declared input domain. No I/O, clock, randomness or environment is
//! read here; the command layer owns files and printing.

pub(crate) mod cut_table;
pub(crate) mod cuts;
pub(crate) mod egraph;
pub(crate) mod extract;
pub(crate) mod rules;
pub(crate) mod semantics;
pub(crate) mod signature;
pub(crate) mod strategy;

use serde_json::{Value, json};
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_synthesis::finite::{Error, Program};
use zeno_fcis_synthesis::finite_runtime::import_program;

use crate::neural_loop::profiles::{Profile, ProfileRefusal};
use crate::transform::{self, Equivalence, Inconclusive, Limits, Observation, Refusal, Rejection};
use egraph::{AddError, ClassId, EGraph, Merge, MergeReason};
use extract::{ExtractError, Extraction};
use rules::PhaseReport;
use semantics::DomainInfo;
use strategy::{Phase, Plan, Strategy};

/// Version of the result format.
pub(crate) const RESULT_SCHEMA: &str = "zeno-fcis/optimize-result/1";

/// Why the original could not be optimized at all.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Refused {
    /// The library importer refused the canonical bytes; `code` is its diagnostic.
    NotAdmitted { code: &'static str },
    /// An input domain has no values.
    EmptyInputDomain,
    /// The strategy names a profile whose inputs, outputs or domain size the
    /// original's ABI cannot meet, so no candidate could pass its gate.
    OutsideProfile(ProfileRefusal),
}

/// The checker could never judge a candidate: the domain exceeds the cap.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DomainTooLarge {
    pub(crate) size: Option<u64>,
    pub(crate) limit: u64,
}

/// What became of one extracted candidate.
#[derive(Debug)]
pub(crate) enum Verdict {
    /// F3 accepted it; this candidate was the incumbent when judged.
    Accepted,
    /// F3 accepted it, but an earlier candidate is at least as good.
    AcceptedNotBetter,
    /// F3 rejected it: a counterexample, an inconclusive check or a refusal.
    Rejected(Rejection),
    /// Not componentwise smaller than the original; never judged.
    NotSmaller,
    /// Byte-identical to the original or an earlier candidate; never judged.
    Duplicate,
    /// Worse than the incumbent on (nodes, bytes); never judged.
    NotBetterThanIncumbent,
    /// Extraction produced no admitted program.
    Unextractable(ExtractError),
}

impl Verdict {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::AcceptedNotBetter => "accepted-not-better",
            Self::Rejected(Rejection::Counterexample(_)) => "counterexample",
            Self::Rejected(Rejection::Inconclusive(_)) => "inconclusive",
            Self::Rejected(Rejection::Refused(_)) => "refused",
            Self::NotSmaller => "not-smaller",
            Self::Duplicate => "duplicate",
            Self::NotBetterThanIncumbent => "not-better-than-incumbent",
            Self::Unextractable(_) => "unextractable",
        }
    }

    pub(crate) fn accepted(&self) -> bool {
        matches!(self, Self::Accepted | Self::AcceptedNotBetter)
    }
}

/// One extracted candidate and its fate.
#[derive(Debug)]
pub(crate) struct Candidate {
    /// Index of the run (the strategy within the plan) that extracted it.
    pub(crate) run: usize,
    pub(crate) phase_index: usize,
    pub(crate) phase: Phase,
    pub(crate) nodes: Option<usize>,
    pub(crate) bytes: Option<usize>,
    pub(crate) sha256: Option<String>,
    pub(crate) extraction_rounds: u32,
    pub(crate) extraction_limit_hit: bool,
    pub(crate) verdict: Verdict,
}

/// Where an accepted candidate came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Source {
    /// Extracted after phase `phase_index` of run `run`.
    Phase { run: usize, phase_index: usize },
    /// The `index`-th program supplied with the request.
    Supplied { index: usize },
}

/// The best F3-accepted candidate.
#[derive(Debug)]
pub(crate) struct Accepted {
    pub(crate) bytes: Vec<u8>,
    pub(crate) nodes: usize,
    pub(crate) max_steps: u64,
    pub(crate) source: Source,
    pub(crate) equivalence: Equivalence,
}

/// A program supplied with the request and what became of it. Only a
/// program the checker accepts is fused into the runs' e-graphs.
#[derive(Debug)]
pub(crate) struct Supplied {
    pub(crate) sha256: String,
    pub(crate) bytes: usize,
    pub(crate) nodes: Option<usize>,
    pub(crate) verdict: Verdict,
}

impl Supplied {
    fn json(&self, index: usize) -> Value {
        let detail = match &self.verdict {
            Verdict::Rejected(rejection) => rejection_json(rejection),
            Verdict::Unextractable(ExtractError::NotAdmitted(code)) => {
                json!({"cause": "candidate-not-admitted", "code": code})
            }
            _ => Value::Null,
        };
        json!({
            "index": index,
            "sha256": self.sha256,
            "bytes": self.bytes,
            "nodes": self.nodes,
            "verdict": self.verdict.name(),
            "accepted": self.verdict.accepted(),
            "fused": self.verdict.accepted(),
            "detail": detail,
        })
    }
}

impl Accepted {
    fn key(&self) -> (usize, usize, u64, &[u8]) {
        (self.nodes, self.bytes.len(), self.max_steps, &self.bytes)
    }
}

/// One strategy's run within a search, from its own e-graph.
#[derive(Debug)]
pub(crate) struct Run {
    pub(crate) phases_requested: usize,
    /// Why the run ended before its last phase, when it did.
    pub(crate) stopped: Option<&'static str>,
    pub(crate) enodes: u32,
    pub(crate) classes: u32,
    /// Merges the e-graph accepted and refused.
    pub(crate) merges: u64,
    pub(crate) refused_merges: u64,
    /// Live classes with and without an exact table at the end, the table
    /// work spent and whether the table budget ran out.
    pub(crate) tables: (u32, u32, u64, bool),
    /// Deterministic work spent (see `Limits::max_work`).
    pub(crate) work: u64,
    /// Roots of supplied programs merged with the original's, and refused.
    pub(crate) fused_roots: (u64, u64),
}

impl Run {
    fn json(&self, index: usize) -> Value {
        json!({
            "run": index,
            "phases_requested": self.phases_requested,
            "stopped": self.stopped,
            "fused_roots": {"merged": self.fused_roots.0, "refused": self.fused_roots.1},
            "enodes": self.enodes,
            "classes": self.classes,
            "merges": self.merges,
            "refused_merges": self.refused_merges,
            "tables": {
                "exact_classes": self.tables.0,
                "inexact_classes": self.tables.1,
                "work": self.tables.2,
                "budget_exhausted": self.tables.3,
            },
            "work": self.work,
        })
    }
}

/// A complete search, whatever it found.
#[derive(Debug)]
pub(crate) struct Search {
    pub(crate) original_nodes: usize,
    pub(crate) original_bytes: usize,
    pub(crate) original_sha256: String,
    pub(crate) domain_size: u64,
    /// Whether every class of the original has an exact table.
    pub(crate) exact_signatures: bool,
    pub(crate) pinned_nodes: usize,
    /// Every run's phases, in order.
    pub(crate) phases: Vec<PhaseReport>,
    /// Every run's candidates, in order.
    pub(crate) candidates: Vec<Candidate>,
    /// The programs supplied with the request, judged before any run.
    pub(crate) supplied: Vec<Supplied>,
    pub(crate) best: Option<Accepted>,
    /// Why the first run that ended early did so.
    pub(crate) stopped: Option<&'static str>,
    pub(crate) runs: Vec<Run>,
    pub(crate) plan: Plan,
    pub(crate) max_input_tuples: u64,
}

impl Search {
    pub(crate) fn any_limit_hit(&self) -> bool {
        self.phases.iter().any(|phase| phase.limit_hit.is_some())
            || self
                .candidates
                .iter()
                .any(|candidate| candidate.extraction_limit_hit)
    }

    /// The report fields. Every value is computed by this search. With one
    /// run, `enodes`, `classes` and the merge counts are that run's; with a
    /// portfolio, the largest e-graph and the merge totals over all runs.
    pub(crate) fn json(&self) -> Value {
        json!({
            "original": {
                "sha256": self.original_sha256,
                "bytes": self.original_bytes,
                "nodes": self.original_nodes,
            },
            "domain": {
                "size": self.domain_size,
                "exact_signatures": self.exact_signatures,
                "max_input_tuples": self.max_input_tuples,
            },
            "strategy": self.plan.json(),
            "search": {
                "phases_requested": self.runs.iter().map(|run| run.phases_requested).sum::<usize>(),
                "phases_run": self.phases.len(),
                "pinned_nodes": self.pinned_nodes,
                "enodes": self.runs.iter().map(|run| run.enodes).max().unwrap_or(0),
                "classes": self.runs.iter().map(|run| run.classes).max().unwrap_or(0),
                "merges": self.runs.iter().map(|run| run.merges).sum::<u64>(),
                "refused_merges": self.runs.iter().map(|run| run.refused_merges).sum::<u64>(),
                "any_limit_hit": self.any_limit_hit(),
                "stopped": self.stopped,
                "runs": self.runs.iter().enumerate().map(|(index, run)| run.json(index)).collect::<Vec<_>>(),
                "phases": self.phases.iter().map(phase_json).collect::<Vec<_>>(),
            },
            "candidates": self.candidates.iter().map(candidate_json).collect::<Vec<_>>(),
            "supplied": self.supplied.iter().enumerate().map(|(index, supplied)| supplied.json(index)).collect::<Vec<_>>(),
            "best": self.best.as_ref().map(|best| json!({
                "sha256": transform::sha256_hex(&best.bytes),
                "bytes": best.bytes.len(),
                "nodes": best.nodes,
                "max_steps": best.max_steps,
                "run": match best.source { Source::Phase { run, .. } => Some(run), Source::Supplied { .. } => None },
                "phase_index": match best.source { Source::Phase { phase_index, .. } => Some(phase_index), Source::Supplied { .. } => None },
                "supplied": match best.source { Source::Supplied { index } => Some(index), Source::Phase { .. } => None },
                "receipt": best.equivalence.receipt_value(),
                "receipt_sha256": transform::sha256_hex(&best.equivalence.receipt()),
            })),
        })
    }
}

fn phase_json(phase: &PhaseReport) -> Value {
    json!({
        "run": phase.run,
        "phase": phase.phase.name(),
        "rounds_requested": phase.rounds_requested,
        "rounds_run": phase.rounds_run,
        "saturated": phase.saturated,
        "limit_hit": phase.limit_hit.map(egraph::Limit::name),
        "skipped": phase.skipped,
        "rewrites": phase.rewrites,
        "nodes_added": phase.nodes_added,
        "merges": phase.merges,
        "refused_merges": phase.refused,
        "cut_evaluations": phase.cut_evaluations,
        "work": phase.work,
        "enodes": phase.enodes,
        "classes": phase.classes,
    })
}

fn candidate_json(candidate: &Candidate) -> Value {
    let detail = match &candidate.verdict {
        Verdict::Rejected(rejection) => rejection_json(rejection),
        Verdict::Unextractable(ExtractError::Unextractable(class)) => {
            json!({"cause": "unextractable-class", "class": class})
        }
        Verdict::Unextractable(ExtractError::NotAdmitted(code)) => {
            json!({"cause": "candidate-not-admitted", "code": code})
        }
        _ => Value::Null,
    };
    json!({
        "run": candidate.run,
        "phase_index": candidate.phase_index,
        "phase": candidate.phase.name(),
        "nodes": candidate.nodes,
        "bytes": candidate.bytes,
        "sha256": candidate.sha256,
        "extraction": {
            "rounds_run": candidate.extraction_rounds,
            "limit_hit": candidate.extraction_limit_hit,
        },
        "verdict": candidate.verdict.name(),
        "accepted": candidate.verdict.accepted(),
        "detail": detail,
    })
}

fn observation_json(observation: &Observation) -> Value {
    match &observation.result {
        Ok(outputs) => json!({"ok": transform::values_json(outputs), "steps": observation.steps}),
        Err(failure) => {
            json!({"error": transform::failure_tag(*failure), "steps": observation.steps})
        }
    }
}

/// The checker's rejection, in the same shape `transform check` prints.
pub(crate) fn rejection_json(rejection: &Rejection) -> Value {
    match rejection {
        Rejection::Counterexample(witness) => json!({
            "ordinal": witness.ordinal,
            "input": transform::values_json(&witness.input),
            "original": observation_json(&witness.original),
            "candidate": observation_json(&witness.candidate),
        }),
        Rejection::Inconclusive(Inconclusive::DomainTooLarge { size, limit }) => json!({
            "cause": "domain-too-large",
            "domain_size": size.map(|size| size.to_string()),
            "max_input_tuples": limit,
        }),
        Rejection::Inconclusive(Inconclusive::BudgetBoundary {
            over_limit,
            minimum_limit,
            usage,
        }) => json!({
            "cause": "budget-boundary",
            "over_limit": {"original": over_limit[0], "candidate": over_limit[1]},
            "minimum_step_limit": minimum_limit,
            "usage": transform::usage_json(usage),
        }),
        Rejection::Inconclusive(Inconclusive::CoverageMismatch { expected, visited }) => json!({
            "cause": "coverage-mismatch", "expected": expected, "visited": visited,
        }),
        Rejection::Refused(Refusal::NotAdmitted { side, code }) => json!({
            "reason": match side {
                transform::Side::Original => "original-not-admitted",
                transform::Side::Candidate => "candidate-not-admitted",
            },
            "code": code,
        }),
        Rejection::Refused(Refusal::InputAbi {
            original,
            candidate,
        }) => json!({
            "reason": "input-abi-mismatch",
            "original": transform::domains_json(original),
            "candidate": transform::domains_json(candidate),
        }),
        Rejection::Refused(Refusal::OutputAbi {
            original,
            candidate,
        }) => json!({
            "reason": "output-abi-mismatch",
            "original": transform::domains_json(original),
            "candidate": transform::domains_json(candidate),
        }),
        Rejection::Refused(Refusal::EmptyInputDomain { position }) => json!({
            "reason": "empty-input-domain", "position": position,
        }),
    }
}

/// The result of one optimization request.
#[derive(Debug)]
pub(crate) enum Outcome {
    Refused(Refused),
    Inconclusive(DomainTooLarge),
    Searched(Box<Search>),
}

/// Runs one strategy over the original program and judges every candidate;
/// the commands call [`optimize_plan`].
#[cfg(test)]
pub(crate) fn optimize(original: &[u8], strategy: &Strategy, max_input_tuples: u64) -> Outcome {
    optimize_plan(
        original,
        &Plan::single(strategy.clone()),
        &[],
        max_input_tuples,
    )
}

/// What every judgment in one search shares.
struct Judging<'a> {
    original: &'a [u8],
    program: &'a Program,
    limits: Limits,
}

/// Runs a plan over the original program and judges every candidate.
///
/// Refusal order: admission, empty domain, an ABI or domain a strategy's
/// profile cannot hold; then a domain above `max_input_tuples` is
/// inconclusive before any search, since no candidate could be judged.
/// Otherwise each strategy runs in turn from the e-graph of the original:
/// each phase runs in order, and after each phase a candidate is extracted,
/// re-encoded canonically, screened by profile and cost and judged by the
/// checker over the full domain. One incumbent spans every run; the best
/// accepted candidate wins by (nodes, bytes, largest Step usage, canonical
/// bytes).
///
/// Each `supplied` program is judged first, like a candidate: the checker
/// must accept it against the original (and the profile's gate, when there
/// is one), and it becomes the incumbent when it is better. Every accepted
/// one is then fused into each run's e-graph: its instructions are added
/// unpinned, and each root is merged with the original's when the original
/// can never fail, so the checker's verdict makes the roots equal on every
/// tuple, or when both roots have identical exact tables.
pub(crate) fn optimize_plan(
    original: &[u8],
    plan: &Plan,
    supplied: &[Vec<u8>],
    max_input_tuples: u64,
) -> Outcome {
    let program = match import_program(original) {
        Ok(program) => program,
        Err(error) => {
            return Outcome::Refused(Refused::NotAdmitted {
                code: match error {
                    Error::Invalid(code) | Error::Limit(code) => code,
                    _ => "unclassified",
                },
            });
        }
    };
    let Some(domain) = DomainInfo::new(program.inputs()) else {
        return Outcome::Refused(Refused::EmptyInputDomain);
    };
    for profile in plan
        .strategies
        .iter()
        .filter_map(|strategy| strategy.profile)
    {
        // Instructions outside the profile can be replaced by the search;
        // the ABI and the domain size cannot.
        match profile.admit(&program) {
            Ok(_) | Err(ProfileRefusal::Opcode { .. }) => {}
            Err(refusal) => return Outcome::Refused(Refused::OutsideProfile(refusal)),
        }
    }
    let domain_size = match domain.size() {
        Some(size) if size <= max_input_tuples => size,
        size => {
            return Outcome::Inconclusive(DomainTooLarge {
                size,
                limit: max_input_tuples,
            });
        }
    };
    let mut search = Search {
        original_nodes: program.nodes().len(),
        original_bytes: original.len(),
        original_sha256: transform::sha256_hex(original),
        domain_size,
        exact_signatures: false,
        pinned_nodes: 0,
        phases: Vec::new(),
        candidates: Vec::new(),
        best: None,
        stopped: None,
        runs: Vec::new(),
        plan: plan.clone(),
        supplied: Vec::new(),
        max_input_tuples,
    };
    let judging = Judging {
        original,
        program: &program,
        limits: Limits {
            steps: transform::DEFAULT_STEP_LIMIT,
            input_tuples: max_input_tuples,
        },
    };
    let profile = plan
        .strategies
        .first()
        .and_then(|strategy| strategy.profile);
    let mut fused: Vec<Program> = Vec::new();
    for (index, bytes) in supplied.iter().enumerate() {
        let (record, accepted, program) = judge_supplied(&search, &judging, index, bytes, profile);
        if let Some(accepted) = accepted {
            search.best = Some(accepted);
        }
        fused.extend(program);
        search.supplied.push(record);
    }
    for (index, strategy) in plan.strategies.iter().enumerate() {
        let run = run_strategy(&mut search, &judging, &domain, index, strategy, &fused);
        if search.stopped.is_none() {
            search.stopped = run.stopped;
        }
        search.runs.push(run);
    }
    Outcome::Searched(Box::new(search))
}

/// One strategy from the e-graph of the original; its phases and candidates
/// join the search's, under the search's incumbent.
fn run_strategy(
    search: &mut Search,
    judging: &Judging<'_>,
    domain: &DomainInfo,
    index: usize,
    strategy: &Strategy,
    fused: &[Program],
) -> Run {
    let mut run = Run {
        phases_requested: strategy.phases.len(),
        stopped: None,
        enodes: 0,
        classes: 0,
        merges: 0,
        refused_merges: 0,
        tables: (0, 0, 0, false),
        work: 0,
        fused_roots: (0, 0),
    };
    let caps = strategy.limits.caps();
    let (mut egraph, roots) = match EGraph::from_program(judging.program, domain.clone(), caps) {
        Ok(built) => built,
        Err(AddError::Limit(limit)) => {
            run.stopped = Some(match limit {
                egraph::Limit::ENodes => "original-exceeds-max-enodes",
                egraph::Limit::Classes => "original-exceeds-max-classes",
                egraph::Limit::Rewrites | egraph::Limit::Work => "original-exceeds-limits",
            });
            return run;
        }
        Err(AddError::Type(_)) => {
            run.stopped = Some("original-not-typable");
            return run;
        }
    };
    if let Some(profile) = strategy.profile {
        egraph.restrict(profile);
    }
    if index == 0 {
        search.pinned_nodes = egraph.pinned().len();
        search.exact_signatures = egraph.exactness().1 == 0;
    }
    match fuse(&mut egraph, &roots, fused, caps) {
        Ok(counts) => run.fused_roots = counts,
        Err(stop) => {
            run.stopped = Some(stop);
            return run;
        }
    }
    for (phase_index, spec) in strategy.phases.iter().enumerate() {
        match rules::run_phase(&mut egraph, spec.phase, spec.rounds, &strategy.limits) {
            Ok(mut report) => {
                report.run = index;
                search.phases.push(report);
            }
            Err(_) => {
                run.stopped = Some("inconsistent-egraph");
                break;
            }
        }
        let extraction = extract::extract(
            &egraph,
            &roots,
            judging.program.inputs(),
            judging.program.outputs(),
            strategy.extractor,
            strategy.limits.max_extraction_rounds,
        );
        let (candidate, accepted) = judge(
            search,
            judging,
            (index, phase_index, spec.phase),
            extraction,
            strategy.profile,
        );
        if let Some(accepted) = accepted {
            search.best = Some(accepted);
        }
        search.candidates.push(candidate);
    }
    let (exact, inexact) = egraph.exactness();
    run.enodes = egraph.enode_count();
    run.classes = egraph.class_count();
    run.merges = egraph.merges();
    run.refused_merges = egraph.refused_merges();
    run.tables = (
        exact,
        inexact,
        egraph.budget().work_spent,
        egraph.budget().exhausted,
    );
    run.work = egraph.work();
    run
}

/// Screens one extraction by profile and cost, then asks the checker.
/// Returns the candidate record and the new incumbent when this candidate is
/// better. `at` is the run, phase index and phase that extracted it.
fn judge(
    search: &Search,
    judging: &Judging<'_>,
    at: (usize, usize, Phase),
    extraction: Result<Extraction, ExtractError>,
    profile: Option<Profile>,
) -> (Candidate, Option<Accepted>) {
    let (run, phase_index, phase) = at;
    let (original, program, limits) = (judging.original, judging.program, judging.limits);
    let mut candidate = Candidate {
        run,
        phase_index,
        phase,
        nodes: None,
        bytes: None,
        sha256: None,
        extraction_rounds: 0,
        extraction_limit_hit: false,
        verdict: Verdict::NotSmaller,
    };
    let extraction = match extraction {
        Ok(extraction) => extraction,
        Err(error) => {
            candidate.verdict = Verdict::Unextractable(error);
            return (candidate, None);
        }
    };
    candidate.extraction_rounds = extraction.rounds_run;
    candidate.extraction_limit_hit = extraction.limit_hit;
    if let Some(profile) = profile
        && profile.admit(&extraction.program).is_err()
    {
        candidate.verdict = Verdict::Unextractable(ExtractError::NotAdmitted("outside-profile"));
        return (candidate, None);
    }
    let bytes = match extraction
        .program
        .value()
        .and_then(|value| value.canonical_bytes())
    {
        Ok(bytes) => bytes,
        Err(_) => {
            candidate.verdict =
                Verdict::Unextractable(ExtractError::NotAdmitted("program-encoding"));
            return (candidate, None);
        }
    };
    let nodes = extraction.program.nodes().len();
    candidate.nodes = Some(nodes);
    candidate.bytes = Some(bytes.len());
    candidate.sha256 = Some(transform::sha256_hex(&bytes));
    let (original_nodes, original_bytes) = (program.nodes().len(), original.len());
    let smaller = nodes <= original_nodes
        && bytes.len() <= original_bytes
        && (nodes < original_nodes || bytes.len() < original_bytes);
    if !smaller {
        candidate.verdict = Verdict::NotSmaller;
        return (candidate, None);
    }
    if bytes == original
        || search
            .candidates
            .iter()
            .any(|earlier| earlier.sha256 == candidate.sha256)
        || search
            .supplied
            .iter()
            .any(|earlier| Some(&earlier.sha256) == candidate.sha256.as_ref())
    {
        candidate.verdict = Verdict::Duplicate;
        return (candidate, None);
    }
    if let Some(best) = &search.best
        && (nodes, bytes.len()) > (best.nodes, best.bytes.len())
    {
        candidate.verdict = Verdict::NotBetterThanIncumbent;
        return (candidate, None);
    }
    match transform::check(original, &bytes, limits) {
        Ok(equivalence) => {
            let max_steps = equivalence.receipt_value()["usage"]["max_steps"]["candidate"]
                .as_u64()
                .unwrap_or(u64::MAX);
            let accepted = Accepted {
                bytes,
                nodes,
                max_steps,
                source: Source::Phase { run, phase_index },
                equivalence,
            };
            let better = search
                .best
                .as_ref()
                .is_none_or(|best| accepted.key() < best.key());
            if better {
                candidate.verdict = Verdict::Accepted;
                (candidate, Some(accepted))
            } else {
                candidate.verdict = Verdict::AcceptedNotBetter;
                (candidate, None)
            }
        }
        Err(rejection) => {
            candidate.verdict = Verdict::Rejected(rejection);
            (candidate, None)
        }
    }
}

/// Judges one supplied program: the checker always runs, since only an
/// accepted program may be fused; the program becomes the incumbent when it
/// is smaller than the original and better than the incumbent. Returns the
/// record, the new incumbent if any and the program to fuse if accepted.
fn judge_supplied(
    search: &Search,
    judging: &Judging<'_>,
    index: usize,
    bytes: &[u8],
    profile: Option<Profile>,
) -> (Supplied, Option<Accepted>, Option<Program>) {
    let mut record = Supplied {
        sha256: transform::sha256_hex(bytes),
        bytes: bytes.len(),
        nodes: None,
        verdict: Verdict::NotSmaller,
    };
    let program = match import_program(bytes) {
        Ok(program) => program,
        Err(error) => {
            record.verdict = Verdict::Unextractable(ExtractError::NotAdmitted(match error {
                Error::Invalid(code) | Error::Limit(code) => code,
                _ => "unclassified",
            }));
            return (record, None, None);
        }
    };
    record.nodes = Some(program.nodes().len());
    if let Some(profile) = profile
        && profile.admit(&program).is_err()
    {
        record.verdict = Verdict::Unextractable(ExtractError::NotAdmitted("outside-profile"));
        return (record, None, None);
    }
    let equivalence = match transform::check(judging.original, bytes, judging.limits) {
        Ok(equivalence) => equivalence,
        Err(rejection) => {
            record.verdict = Verdict::Rejected(rejection);
            return (record, None, None);
        }
    };
    let nodes = program.nodes().len();
    let (original_nodes, original_bytes) = (judging.program.nodes().len(), judging.original.len());
    let smaller = nodes <= original_nodes
        && bytes.len() <= original_bytes
        && (nodes < original_nodes || bytes.len() < original_bytes);
    let max_steps = equivalence.receipt_value()["usage"]["max_steps"]["candidate"]
        .as_u64()
        .unwrap_or(u64::MAX);
    let accepted = Accepted {
        bytes: bytes.to_vec(),
        nodes,
        max_steps,
        source: Source::Supplied { index },
        equivalence,
    };
    let better = smaller
        && search
            .best
            .as_ref()
            .is_none_or(|best| accepted.key() < best.key());
    if better {
        record.verdict = Verdict::Accepted;
        (record, Some(accepted), Some(program))
    } else {
        record.verdict = Verdict::AcceptedNotBetter;
        (record, None, Some(program))
    }
}

/// Adds every accepted supplied program to the e-graph and merges its roots
/// with the original's: by the checker's verdict when the original has no
/// instruction that may trap, so neither program fails on any tuple, and
/// otherwise only when both roots have identical exact tables. Returns the
/// roots merged and refused.
fn fuse(
    egraph: &mut EGraph,
    roots: &[ClassId],
    fused: &[Program],
    caps: egraph::Caps,
) -> Result<(u64, u64), &'static str> {
    let failure_free = egraph.pinned().is_empty();
    let (mut merged, mut refused) = (0, 0);
    for program in fused {
        let theirs = match egraph.add_equivalent(program, caps) {
            Ok(theirs) => theirs,
            Err(AddError::Limit(_)) => return Err("supplied-exceeds-limits"),
            Err(AddError::Type(_)) => return Err("supplied-not-typable"),
        };
        for (&ours, &theirs) in roots.iter().zip(&theirs) {
            let exact = egraph.data(ours).table.is_some() && egraph.data(theirs).table.is_some();
            let reason = if failure_free {
                MergeReason::Checked
            } else if exact {
                MergeReason::Rule
            } else {
                refused += 1;
                continue;
            };
            match egraph.union(ours, theirs, reason) {
                Ok(Merge::Merged | Merge::AlreadyEqual) => merged += 1,
                Ok(Merge::Refused) => refused += 1,
                Err(_) => return Err("inconsistent-egraph"),
            }
        }
        if egraph.rebuild().is_err() {
            return Err("inconsistent-egraph");
        }
    }
    Ok((merged, refused))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
