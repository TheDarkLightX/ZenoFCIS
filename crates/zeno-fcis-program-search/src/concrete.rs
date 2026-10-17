//! Concrete rewrite proposals over exact, bounded term tables.
//!
//! This LLM-generated module reuses the search's graph, hash-consed pool,
//! enumeration and library evaluator. Groups, costs and rules are proposals,
//! never equivalence certificates. Callers must check/replay each rule on its
//! target domain and every rewritten whole program with the existing checker.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use zeno_fcis_synthesis::finite::{Domain, Program};

use crate::Stop;
use crate::ir::{Graph, Kind, domain_kind, encode};
use crate::pool::{Pool, canonical};
use crate::space::{Meter, Space};

/// Hard limits, checked before allocating semantic tables.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Budget {
    /// Largest complete declared input domain.
    pub max_tuples: u64,
    /// Largest concrete corpus, including extracted program nodes.
    pub max_terms: usize,
    /// Most retained scalar table cells.
    pub max_cells: u64,
    /// Most instructions evaluated over tuples, plus extraction work.
    pub max_work: u64,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_tuples: 1 << 17,
            max_terms: 128,
            max_cells: 1 << 22,
            max_work: 50_000_000,
        }
    }
}

/// Two concrete live terms with identical input domains and full typed output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rule {
    /// More expensive term; this is not a pattern with metavariables.
    pub lhs: Program,
    /// Proposed cheaper replacement.
    pub rhs: Program,
}

/// Advisory corpus partition and proposed decreasing rules.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Discovery {
    /// Canonical, deduplicated terms in byte order.
    pub terms: Vec<Program>,
    /// Exact nontrapping table classes, as indices into `terms`.
    pub classes: Vec<Vec<usize>>,
    /// Excluded trapping terms, with their first failing tuple ordinal.
    pub rejected: Vec<(usize, usize)>,
    /// Concrete decreasing replacements; no rule is checker-qualified here.
    pub rules: Vec<Rule>,
    /// Charged proposer work for this completed discovery.
    pub work_used: u64,
}

fn admitted(graph: &Graph) -> Result<Program, Stop> {
    graph.to_program().ok_or(Stop::Inconsistent {
        code: "concrete-program-admission",
    })
}

fn graph(program: &Program) -> Result<Graph, Stop> {
    Graph::from_program(program).map_err(|code| Stop::NotAdmitted { code })
}

/// Hash-cons and prune using the existing pool. Pruning may remove eager traps:
/// this is a proposal that always needs a whole-program check.
pub fn canonical_candidate(program: &Program) -> Result<Program, Stop> {
    admitted(&canonical(&graph(program)?))
}

/// Extract one node's live dependency cone with a full-range typed output.
pub fn node_term(program: &Program, root: usize) -> Result<Program, Stop> {
    let graph = graph(program)?;
    let kind = graph.kinds().get(root).copied().ok_or(Stop::NotAdmitted {
        code: "concrete-root",
    })?;
    let output = match kind {
        Kind::Bool => Domain::Bool,
        Kind::Int => Domain::Int {
            min: i64::MIN,
            max: i64::MAX,
        },
    };
    let mut pool = Pool::new(&graph.inputs);
    let ids = pool.load(&graph);
    admitted(&pool.emit(&[output], &[ids[root]]))
}

/// A corpus term must contain only its live cone and a single full output.
pub fn validate_term(term: &Program, inputs: &[Domain]) -> Result<(), Stop> {
    let full = match term.outputs() {
        [Domain::Bool] => true,
        [Domain::Int { min, max }] => *min == i64::MIN && *max == i64::MAX,
        _ => false,
    };
    if term.inputs() != inputs || !full {
        return Err(Stop::NotAdmitted {
            code: "concrete-term-abi",
        });
    }
    // A count comparison avoids requiring the caller's numbering to be the
    // pool's numbering while excluding dead nodes and duplicate instructions.
    if canonical_candidate(term)?.nodes().len() != term.nodes().len() {
        return Err(Stop::NotAdmitted {
            code: "concrete-term-live-cone",
        });
    }
    Ok(())
}

/// Rebind a concrete term to new input ranges. This establishes only shape;
/// the target rule must receive a fresh checker verdict and replay.
pub fn rebind(term: &Program, inputs: &[Domain]) -> Result<Program, Stop> {
    if term.inputs().len() != inputs.len()
        || term
            .inputs()
            .iter()
            .zip(inputs)
            .any(|(a, b)| domain_kind(*a) != domain_kind(*b))
    {
        return Err(Stop::NotAdmitted {
            code: "concrete-transfer-input-kinds",
        });
    }
    let mut graph = graph(term)?;
    graph.inputs = inputs.to_vec();
    admitted(&graph)
}

/// Apply a concrete structural replacement using the existing pool. Matching
/// is independent of node numbering and orders commutative operands exactly as
/// the search does. The result carries no semantic authority.
pub fn replace(program: &Program, rule: &Rule) -> Result<Program, Stop> {
    validate_term(&rule.lhs, program.inputs())?;
    validate_term(&rule.rhs, program.inputs())?;
    if rule.lhs.outputs() != rule.rhs.outputs() {
        return Err(Stop::NotAdmitted {
            code: "concrete-rule-output-kind",
        });
    }
    let original = graph(program)?;
    let lhs = graph(&rule.lhs)?;
    let rhs = graph(&rule.rhs)?;
    let mut matching = Pool::new(&original.inputs);
    let pattern_ids = matching.load(&lhs);
    let pattern = pattern_ids[lhs.roots[0]];
    let original_ids = matching.load(&original);
    if !original_ids.contains(&pattern) {
        return Ok(program.clone());
    }
    let mut pool = Pool::new(&original.inputs);
    let replacement_ids = pool.load(&rhs);
    let replacement = replacement_ids[rhs.roots[0]];
    let mut ids = Vec::with_capacity(original.nodes.len());
    for (n, node) in original.nodes.iter().enumerate() {
        let id = if original_ids[n] == pattern {
            replacement
        } else {
            pool.add(node.map(|operand| ids[operand]))
        };
        ids.push(id);
    }
    let roots = original
        .roots
        .iter()
        .map(|root| ids[*root])
        .collect::<Vec<_>>();
    admitted(&pool.emit(&original.outputs, &roots))
}

/// Nodes, encoded size, then canonical bytes: deterministic advisory cost.
pub fn cost(program: &Program) -> Result<(usize, usize, Vec<u8>), Stop> {
    let bytes = encode(program).ok_or(Stop::NotAdmitted {
        code: "concrete-encoding",
    })?;
    Ok((program.nodes().len(), bytes.len(), bytes))
}

/// A stopped discovery, retaining charged work and no partial bank.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryStop {
    /// Existing typed refusal or resource stop.
    pub reason: Stop,
    /// Charged units, including the reservation that encountered the limit.
    pub work_used: u64,
}

/// Group an explicit finite corpus by exact complete values. Any trapping term
/// is excluded. No sampled or hashed table is used as an equality decision.
pub fn discover(
    program: &Program,
    proposed: &[Program],
    budget: Budget,
) -> Result<Discovery, DiscoveryStop> {
    let mut meter = Meter::new(budget.max_work);
    discover_metered(program, proposed, budget, &mut meter).map_err(|reason| DiscoveryStop {
        reason,
        work_used: meter.used(),
    })
}

fn discover_metered(
    program: &Program,
    proposed: &[Program],
    budget: Budget,
    meter: &mut Meter,
) -> Result<Discovery, Stop> {
    let space = Space::new(program.inputs(), budget.max_tuples)?;
    if program.nodes().len() > budget.max_terms || proposed.len() > budget.max_terms {
        return Err(Stop::CellsOverBudget {
            cells: (program.nodes().len() + proposed.len()) as u128,
            limit: u64::try_from(budget.max_terms).unwrap_or(u64::MAX),
        });
    }
    let mut corpus = BTreeMap::new();
    for n in 0..program.nodes().len() {
        meter.charge(program.nodes().len())?;
        let term = node_term(program, n)?;
        corpus.insert(cost(&term)?.2, term);
    }
    for term in proposed {
        meter.charge(term.nodes().len())?;
        validate_term(term, program.inputs())?;
        let term = canonical_candidate(term)?;
        corpus.insert(cost(&term)?.2, term);
    }
    if corpus.len() > budget.max_terms {
        return Err(Stop::CellsOverBudget {
            cells: corpus.len() as u128,
            limit: u64::try_from(budget.max_terms).unwrap_or(u64::MAX),
        });
    }
    let cells = (space.count() as u128).saturating_mul(corpus.len() as u128);
    if cells > u128::from(budget.max_cells) {
        return Err(Stop::CellsOverBudget {
            cells,
            limit: budget.max_cells,
        });
    }
    let terms = corpus.into_values().collect::<Vec<_>>();
    let needed = terms.iter().fold(0u128, |sum, term| {
        sum.saturating_add((space.count() as u128).saturating_mul(term.nodes().len() as u128))
    });
    if needed > u128::from(budget.max_work.saturating_sub(meter.used())) {
        return Err(Stop::WorkBudget {
            limit: budget.max_work,
        });
    }
    let mut groups: BTreeMap<(Kind, Vec<i64>), Vec<usize>> = BTreeMap::new();
    let mut rejected = Vec::new();
    let mut point = Vec::new();
    for (index, term) in terms.iter().enumerate() {
        let mut values = Vec::with_capacity(space.count());
        for ordinal in 0..space.count() {
            meter.charge(term.nodes().len())?;
            space.tuple(ordinal, &mut point);
            match term.evaluate(&point) {
                Ok(row) => values.push(row[0]),
                Err(_) => {
                    rejected.push((index, ordinal));
                    break;
                }
            }
        }
        if values.len() == space.count() {
            let kind = domain_kind(term.outputs()[0]).ok_or(Stop::NotAdmitted {
                code: "concrete-output-kind",
            })?;
            groups.entry((kind, values)).or_default().push(index);
        }
    }
    let mut classes = Vec::new();
    let mut proposed_rules = Vec::new();
    for mut members in groups.into_values() {
        members.sort_by_cached_key(|index| cost(&terms[*index]).ok());
        let best = members[0];
        let best_cost = cost(&terms[best])?;
        for index in &members[1..] {
            let other_cost = cost(&terms[*index])?;
            if (best_cost.0, best_cost.1) < (other_cost.0, other_cost.1) {
                proposed_rules.push(Rule {
                    lhs: terms[*index].clone(),
                    rhs: terms[best].clone(),
                });
            }
        }
        classes.push(members);
    }
    proposed_rules.sort_by_cached_key(|rule| (cost(&rule.lhs).ok(), cost(&rule.rhs).ok()));
    // Remove pairs already obtained by earlier concrete proposals. This is
    // corpus normalization, not a proof of a minimal or universal rule basis.
    let mut rules: Vec<Rule> = Vec::new();
    for rule in proposed_rules {
        let mut normalized = rule.lhs.clone();
        for prior in &rules {
            meter.charge(normalized.nodes().len() + prior.lhs.nodes().len())?;
            normalized = replace(&normalized, prior)?;
        }
        if cost(&normalized)?.2 != cost(&rule.rhs)?.2 {
            rules.push(rule);
        }
    }
    Ok(Discovery {
        terms,
        classes,
        rejected,
        rules,
        work_used: meter.used(),
    })
}

#[cfg(test)]
#[path = "concrete_tests.rs"]
mod tests;
