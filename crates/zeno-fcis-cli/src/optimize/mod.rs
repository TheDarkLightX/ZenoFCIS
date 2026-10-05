//! Pure optimizer for `zeno-fcis optimize`: an in-house e-graph proposes
//! smaller programs, and F3's exhaustive checker judges every one of them.
//!
//! The optimizer is an untrusted proposer. Nothing it believes about a
//! candidate counts: a candidate is reported as accepted only when
//! [`transform::check`] has compared it with the original on every tuple of
//! the declared input domain. No I/O, clock, randomness or environment is
//! read here; the command layer owns files and printing.

pub(crate) mod egraph;
pub(crate) mod extract;
pub(crate) mod rules;
pub(crate) mod semantics;
pub(crate) mod strategy;

use serde_json::{Value, json};
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_synthesis::finite::{Error, Program};
use zeno_fcis_synthesis::finite_runtime::import_program;

use crate::transform::{self, Equivalence, Inconclusive, Limits, Observation, Refusal, Rejection};
use egraph::{AddError, EGraph};
use extract::{ExtractError, Extraction};
use rules::PhaseReport;
use semantics::DomainInfo;
use strategy::{Phase, Strategy};

/// Version of the result format.
pub(crate) const RESULT_SCHEMA: &str = "zeno-fcis/optimize-result/1";

/// Why the original could not be optimized at all.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Refused {
    /// The library importer refused the canonical bytes; `code` is its diagnostic.
    NotAdmitted { code: &'static str },
    /// An input domain has no values.
    EmptyInputDomain,
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
    pub(crate) phase_index: usize,
    pub(crate) phase: Phase,
    pub(crate) nodes: Option<usize>,
    pub(crate) bytes: Option<usize>,
    pub(crate) sha256: Option<String>,
    pub(crate) extraction_rounds: u32,
    pub(crate) extraction_limit_hit: bool,
    pub(crate) verdict: Verdict,
}

/// The best F3-accepted candidate.
#[derive(Debug)]
pub(crate) struct Accepted {
    pub(crate) bytes: Vec<u8>,
    pub(crate) nodes: usize,
    pub(crate) max_steps: u64,
    pub(crate) phase_index: usize,
    pub(crate) equivalence: Equivalence,
}

impl Accepted {
    fn key(&self) -> (usize, usize, u64, &[u8]) {
        (self.nodes, self.bytes.len(), self.max_steps, &self.bytes)
    }
}

/// A complete search, whatever it found.
#[derive(Debug)]
pub(crate) struct Search {
    pub(crate) original_nodes: usize,
    pub(crate) original_bytes: usize,
    pub(crate) original_sha256: String,
    pub(crate) domain_size: u64,
    pub(crate) exact_signatures: bool,
    pub(crate) pinned_nodes: usize,
    pub(crate) phases: Vec<PhaseReport>,
    pub(crate) candidates: Vec<Candidate>,
    pub(crate) best: Option<Accepted>,
    /// Why the search ended before its last phase, when it did.
    pub(crate) stopped: Option<&'static str>,
    pub(crate) enodes: u32,
    pub(crate) classes: u32,
    /// Merges the e-graph accepted and refused over the whole search.
    pub(crate) merges: u64,
    pub(crate) refused_merges: u64,
    pub(crate) strategy: Strategy,
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

    /// The report fields. Every value is computed by this search.
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
            "strategy": self.strategy.json(),
            "search": {
                "phases_requested": self.strategy.phases.len(),
                "phases_run": self.phases.len(),
                "pinned_nodes": self.pinned_nodes,
                "enodes": self.enodes,
                "classes": self.classes,
                "merges": self.merges,
                "refused_merges": self.refused_merges,
                "any_limit_hit": self.any_limit_hit(),
                "stopped": self.stopped,
                "phases": self.phases.iter().map(phase_json).collect::<Vec<_>>(),
            },
            "candidates": self.candidates.iter().map(candidate_json).collect::<Vec<_>>(),
            "best": self.best.as_ref().map(|best| json!({
                "sha256": transform::sha256_hex(&best.bytes),
                "bytes": best.bytes.len(),
                "nodes": best.nodes,
                "max_steps": best.max_steps,
                "phase_index": best.phase_index,
                "receipt": best.equivalence.receipt_value(),
                "receipt_sha256": transform::sha256_hex(&best.equivalence.receipt()),
            })),
        })
    }
}

fn phase_json(phase: &PhaseReport) -> Value {
    json!({
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

/// Runs the strategy over the original program and judges every candidate.
///
/// Refusal order: admission, empty domain; then a domain above
/// `max_input_tuples` is inconclusive before any search, since no candidate
/// could be judged. Otherwise the e-graph of the original is built, each
/// phase runs in order, and after each phase a candidate is extracted,
/// re-encoded canonically, screened by cost and judged by the checker over
/// the full domain. The best accepted candidate wins by (nodes, bytes,
/// largest Step usage, canonical bytes).
pub(crate) fn optimize(original: &[u8], strategy: &Strategy, max_input_tuples: u64) -> Outcome {
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
        exact_signatures: domain.exact(),
        pinned_nodes: 0,
        phases: Vec::new(),
        candidates: Vec::new(),
        best: None,
        stopped: None,
        enodes: 0,
        classes: 0,
        merges: 0,
        refused_merges: 0,
        strategy: strategy.clone(),
        max_input_tuples,
    };
    let caps = strategy.limits.caps();
    let (mut egraph, roots) = match EGraph::from_program(&program, domain, caps) {
        Ok(built) => built,
        Err(AddError::Limit(limit)) => {
            search.stopped = Some(match limit {
                egraph::Limit::ENodes => "original-exceeds-max-enodes",
                egraph::Limit::Classes => "original-exceeds-max-classes",
                egraph::Limit::Rewrites => "original-exceeds-limits",
            });
            return Outcome::Searched(Box::new(search));
        }
        Err(AddError::Type(_)) => {
            search.stopped = Some("original-not-typable");
            return Outcome::Searched(Box::new(search));
        }
    };
    search.pinned_nodes = egraph.pinned().len();
    let limits = Limits {
        steps: transform::DEFAULT_STEP_LIMIT,
        input_tuples: max_input_tuples,
    };
    for (phase_index, spec) in strategy.phases.iter().enumerate() {
        match rules::run_phase(&mut egraph, spec.phase, spec.rounds, &strategy.limits) {
            Ok(report) => search.phases.push(report),
            Err(_) => {
                search.stopped = Some("inconsistent-egraph");
                break;
            }
        }
        let extraction = extract::extract(
            &egraph,
            &roots,
            program.inputs(),
            program.outputs(),
            strategy.extractor,
            strategy.limits.max_extraction_rounds,
        );
        let candidate = judge(
            &search,
            original,
            &program,
            phase_index,
            spec.phase,
            extraction,
            limits,
        );
        if let Some(accepted) = candidate.1 {
            search.best = Some(accepted);
        }
        search.candidates.push(candidate.0);
    }
    search.enodes = egraph.enode_count();
    search.classes = egraph.class_count();
    search.merges = egraph.merges();
    search.refused_merges = egraph.refused_merges();
    Outcome::Searched(Box::new(search))
}

/// Screens one extraction by cost, then asks the checker. Returns the
/// candidate record and the new incumbent when this candidate is better.
fn judge(
    search: &Search,
    original: &[u8],
    program: &Program,
    phase_index: usize,
    phase: Phase,
    extraction: Result<Extraction, ExtractError>,
    limits: Limits,
) -> (Candidate, Option<Accepted>) {
    let mut candidate = Candidate {
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
                phase_index,
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

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
