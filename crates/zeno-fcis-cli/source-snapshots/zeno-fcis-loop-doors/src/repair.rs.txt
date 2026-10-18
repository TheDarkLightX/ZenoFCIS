//! The repair door: a wrong proposal gets an explanation and, where
//! possible, a repair.
//!
//! A counterexample tells the proposer one input where it is wrong. Given
//! the proposal's expressions, [`explain`] adds:
//!
//! - the **region**: how many tuples fail, on which outputs, and a formula
//!   over the inputs that holds exactly on the failing tuples (top-down
//!   synthesis on the failure mask, then checked on every tuple), or else the
//!   fields every failing tuple shares;
//! - the **sites**: subexpressions of the proposal where some value makes
//!   the output right on every tuple, in the proposer's own text. Every other
//!   position covered by the inverse is proved unable to repair that output.
//!   Arguments of nonlinear operations without an inverse remain unexplained;
//!   they are never counted as proved unable.
//!   Each site gets an exact constant, a one-step edit (a negated or flipped
//!   condition, swapped branches, an off-by-one, the other operator) or a
//!   synthesized term. Constants and edits rank before rewrites, then by
//!   replacement size. The root of an output is a site for a constant or a one-step
//!   edit, never for a synthesized replacement;
//! - a **repair**: each failing output repaired at its best site, or, when
//!   that is not enough, a greedy sequence of one-site fixes.
//!
//! The repair's status here comes from the library evaluator on every tuple
//! and is untrusted: the `loop` command judges a repair with
//! `transform check` and its replay before calling it a checked repair.

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use zeno_fcis_synthesis::finite::Program;

use crate::expr::{Env, Expr, HoleValue, Kind, Op, evaluate, size, skip_overflow};
use crate::inverse::{
    Need, Path, Synthesis, Synthesizer, common_cube, derive, describe_region, positions,
    replace_at, solve_constant, solve_expression, unsatisfiable,
};
use crate::lower::{lower_outputs, widened};
use crate::subject::Subject;
use crate::{Refusal, Result};

/// Most sites reported per output.
pub const MAX_SITES: usize = 6;
/// Viable positions per output that get a replacement attempt.
pub const MAX_ATTEMPTS: usize = 24;
/// A failing tuple's value in observations (no real output takes it).
pub const IMPOSSIBLE: i128 = -(1 << 62);

/// Door settings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Settings {
    /// Depth for synthesized replacement terms (0: no synthesis).
    pub synth_depth: u32,
    /// Depth for region formulas.
    pub region_depth: u32,
    /// Greedy repair steps.
    pub max_steps: usize,
    /// Positions per output the greedy repair considers.
    pub max_positions: usize,
    /// Top-down limits.
    pub synthesis: Synthesis,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            synth_depth: 4,
            region_depth: 3,
            max_steps: 8,
            max_positions: 64,
            synthesis: Synthesis::default(),
        }
    }
}

/// How a site's replacement was found.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SiteKind {
    /// An exact constant.
    Constant,
    /// A one-step edit of the subexpression.
    Edit,
    /// A synthesized term.
    Synthesised,
}

impl SiteKind {
    /// The kind's name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Constant => "constant",
            Self::Edit => "edit",
            Self::Synthesised => "synthesised",
        }
    }
}

/// A position that can repair an output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Site {
    /// The output.
    pub output: usize,
    /// The position.
    pub path: Path,
    /// The subexpression there.
    pub subexpression: Expr,
    /// Its size.
    pub size: usize,
    /// The replacement, when one was found and the repaired program lowers.
    pub replacement: Option<Expr>,
    /// The position's kind.
    pub kind_at: Kind,
    /// How the replacement was found.
    pub found_by: Option<SiteKind>,
    /// Instructions of the whole repaired program.
    pub repaired_nodes: Option<usize>,
}

/// The output's sites and what the derivation proved.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Localised {
    /// Ranked sites (at most [`MAX_SITES`]).
    pub sites: Vec<Site>,
    /// Positions proved unable to repair the output.
    pub proved_unable: usize,
    /// Positions where some value could repair it.
    pub viable: usize,
}

/// Where the proposal is wrong.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Region {
    /// Failing tuples.
    pub tuples: usize,
    /// Tuples in the domain.
    pub of: usize,
    /// Failing tuples per wrong output.
    pub per_output: Vec<(usize, usize)>,
    /// A formula exactly true on the failing tuples, when synthesis found
    /// one (checked on every tuple).
    pub formula: Option<Expr>,
    /// Otherwise, the fields every failing tuple shares.
    pub shared: Vec<(usize, i128)>,
    /// Every failing tuple, by ordinal.
    pub mask: Vec<bool>,
}

/// One greedy repair step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Step {
    /// The output.
    pub output: usize,
    /// The position.
    pub path: Path,
    /// The subexpression replaced.
    pub subexpression: Expr,
    /// Its replacement.
    pub replacement: Expr,
}

/// The repair's untrusted status.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Status {
    /// The repaired program agrees with the original on every tuple
    /// (library evaluator; the checker still decides).
    Repaired,
    /// It still differs, first at this ordinal.
    Partial(usize),
    /// No site had a replacement.
    NoRepairFound,
    /// The repaired expressions did not lower.
    Refused(Refusal),
}

impl Status {
    /// The status's name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Repaired => "repaired",
            Self::Partial(_) => "partial",
            Self::NoRepairFound => "no-repair-found",
            Self::Refused(_) => "refused",
        }
    }
}

/// A proposed repair.
#[derive(Clone, Debug)]
pub struct Repair {
    /// Its status.
    pub status: Status,
    /// The repaired expressions, one per output.
    pub expressions: Vec<Expr>,
    /// The lowered program.
    pub program: Option<Program>,
    /// Greedy steps, when the greedy repair produced it.
    pub steps: Vec<Step>,
}

/// Everything the door returns for a wrong proposal.
#[derive(Clone, Debug)]
pub struct Explanation {
    /// Per output: the tuples where it is wrong.
    pub failing: Vec<Vec<bool>>,
    /// The failing region.
    pub region: Region,
    /// Exact descriptions of each wrong output, before taking their union.
    pub output_regions: Vec<(usize, Region)>,
    /// Sites per wrong output.
    pub localised: Vec<(usize, Localised)>,
    /// The repair, when one was attempted.
    pub repair: Option<Repair>,
}

/// Every output of `program` on every tuple, from the library evaluator; a
/// trap, or an output outside its declared domain, reads as [`IMPOSSIBLE`].
pub fn observe(subject: &Subject, program: &Program) -> Result<Vec<Vec<i128>>> {
    let wide = widened(program)?;
    let outputs = program.outputs();
    let mut values: Vec<Vec<i128>> = outputs
        .iter()
        .map(|_| Vec::with_capacity(subject.size))
        .collect();
    for ordinal in 0..subject.size {
        let point = subject.point(ordinal);
        match wide.evaluate(&point) {
            Ok(row) => {
                for (j, value) in row.into_iter().enumerate() {
                    let inside = outputs.get(j).is_some_and(|domain| domain.contains(value));
                    values[j].push(if inside { value } else { IMPOSSIBLE });
                }
            }
            Err(_) => {
                for column in &mut values {
                    column.push(IMPOSSIBLE);
                }
            }
        }
    }
    Ok(values)
}

/// The first tuple where `program` differs from the original (library
/// evaluator, declared output domains), or `None` when it agrees everywhere.
#[must_use]
pub fn first_difference(subject: &Subject, program: &Program) -> Option<usize> {
    if program.inputs() != subject.original.inputs()
        || program.outputs() != subject.original.outputs()
    {
        return Some(0);
    }
    (0..subject.size).find(|ordinal| {
        let point = subject.point(*ordinal);
        match program.evaluate(&point) {
            Ok(row) => row
                .iter()
                .enumerate()
                .any(|(j, value)| subject.targets.get(j).is_none_or(|t| t[*ordinal] != *value)),
            Err(_) => true,
        }
    })
}

/// Explains a wrong proposal (one expression per output, variants
/// resolved) and, when `repair` is set, proposes a repair.
pub fn explain(
    subject: &Subject,
    nodes: &[Expr],
    program: &Program,
    repair: bool,
    settings: &Settings,
) -> Result<Explanation> {
    let synthesizer = Synthesizer::new(subject, settings.synthesis);
    explain_inner(subject, nodes, program, repair, settings, &synthesizer)
}

// Composable loops: owned accounting without changing existing explain results.
/// A bounded repair proposal with advisory synthesis accounting on every return.
#[derive(Clone, Debug)]
pub struct RepairSearchReport {
    /// Existing explanation or refusal; neither qualifies a candidate.
    pub outcome: Result<Explanation>,
    /// Charged top-down synthesis units, excluding other repair work.
    pub synthesis_work_used: u64,
    /// A solve stopped or returned no plan within its finite search limits.
    pub synthesis_stopped: bool,
}

/// Runs the existing repair with owned accounting for strict budget callers.
/// Invocation scratch never escapes, including on refusal. Greedy/local work
/// remains bounded by settings and the caller's stage deadline, not this meter.
pub fn explain_recorded(
    subject: &Subject,
    nodes: &[Expr],
    program: &Program,
    repair: bool,
    settings: &Settings,
) -> RepairSearchReport {
    let synthesizer = Synthesizer::new(subject, settings.synthesis);
    let outcome = explain_inner(subject, nodes, program, repair, settings, &synthesizer);
    RepairSearchReport {
        outcome,
        synthesis_work_used: settings
            .synthesis
            .total_work
            .saturating_sub(synthesizer.remaining()),
        synthesis_stopped: synthesizer.stopped(),
    }
}

fn explain_inner(
    subject: &Subject,
    nodes: &[Expr],
    program: &Program,
    repair: bool,
    settings: &Settings,
    synthesizer: &Synthesizer<'_>,
) -> Result<Explanation> {
    let got = observe(subject, program)?;
    let failing: Vec<Vec<bool>> = got
        .iter()
        .zip(&subject.targets)
        .map(|(g, t)| g.iter().zip(t).map(|(a, b)| a != b).collect())
        .collect();
    let mut fail = vec![false; subject.size];
    for f in &failing {
        for (t, wrong) in f.iter().enumerate() {
            fail[t] = fail[t] || *wrong;
        }
    }
    let per_output: Vec<(usize, usize)> = failing
        .iter()
        .enumerate()
        .filter_map(|(j, f)| {
            let count = f.iter().filter(|x| **x).count();
            (count > 0).then_some((j, count))
        })
        .collect();
    let region = failure_region(
        subject,
        fail,
        per_output.clone(),
        synthesizer,
        settings.region_depth,
    )?;
    let output_regions = per_output
        .iter()
        .map(|(j, count)| {
            failure_region(
                subject,
                failing[*j].clone(),
                vec![(*j, *count)],
                synthesizer,
                settings.region_depth,
            )
            .map(|region| (*j, region))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut localised = Vec::new();
    let mut repaired = nodes.to_vec();
    for (j, f) in failing.iter().enumerate() {
        if !f.iter().any(|x| *x) {
            continue;
        }
        let found = localise(subject, &nodes[j], j, synthesizer, 0, nodes)?;
        if repair
            && let Some(best) = found.sites.iter().find(|site| site.replacement.is_some())
            && let Some(replacement) = &best.replacement
        {
            repaired[j] = replace_at(&nodes[j], &best.path, replacement)?;
        }
        localised.push((j, found));
    }
    let repair = if repair {
        let mut attempt = repaired_program(subject, nodes, repaired.clone());
        if attempt.status != Status::Repaired {
            let (stepped, steps) = repair_greedy(subject, &repaired, settings)?;
            if !steps.is_empty() {
                let mut greedy = repaired_program(subject, nodes, stepped);
                greedy.steps = steps;
                if greedy.status == Status::Repaired
                    || !matches!(attempt.status, Status::Partial(_))
                {
                    attempt = greedy;
                }
            }
        }
        // Only a failed edit repair permits synthesized replacements. Keep
        // the same exact finite comparison and independent caller check.
        if attempt.status != Status::Repaired && settings.synth_depth > 0 {
            localised.clear();
            for (j, f) in failing.iter().enumerate() {
                if !f.iter().any(|x| *x) {
                    continue;
                }
                let found = localise(
                    subject,
                    &nodes[j],
                    j,
                    synthesizer,
                    settings.synth_depth,
                    nodes,
                )?;
                if let Some(best) = found.sites.iter().find(|site| site.replacement.is_some())
                    && let Some(replacement) = &best.replacement
                {
                    repaired[j] = replace_at(&nodes[j], &best.path, replacement)?;
                }
                localised.push((j, found));
            }
            let rewrite = repaired_program(subject, nodes, repaired);
            if rewrite.status == Status::Repaired || !matches!(attempt.status, Status::Partial(_)) {
                attempt = rewrite;
            }
        }
        Some(attempt)
    } else {
        None
    };
    Ok(Explanation {
        failing,
        region,
        output_regions,
        localised,
        repair,
    })
}

fn failure_region(
    subject: &Subject,
    mask: Vec<bool>,
    per_output: Vec<(usize, usize)>,
    synthesizer: &Synthesizer<'_>,
    depth: u32,
) -> Result<Region> {
    let formula = match describe_region(&mask, synthesizer, depth)? {
        Some(expr) => {
            let values = evaluate(&expr, &subject.env(None), &BTreeMap::new())?;
            values
                .iter()
                .zip(&mask)
                .all(|(v, wrong)| (*v == 1) == *wrong)
                .then_some(expr)
        }
        None => None,
    };
    let shared = if formula.is_none() {
        common_cube(&mask, subject)
    } else {
        Vec::new()
    };
    Ok(Region {
        tuples: mask.iter().filter(|wrong| **wrong).count(),
        of: subject.size,
        per_output,
        formula,
        shared,
        mask,
    })
}

fn repaired_program(subject: &Subject, original: &[Expr], repaired: Vec<Expr>) -> Repair {
    if repaired == original {
        return Repair {
            status: Status::NoRepairFound,
            expressions: repaired,
            program: None,
            steps: Vec::new(),
        };
    }
    match lower_outputs(&repaired, &subject.vocab, &subject.original) {
        Ok(program) => Repair {
            status: match first_difference(subject, &program) {
                None => Status::Repaired,
                Some(ordinal) => Status::Partial(ordinal),
            },
            expressions: repaired,
            program: Some(program),
            steps: Vec::new(),
        },
        Err(refusal) => Repair {
            status: Status::Refused(refusal),
            expressions: repaired,
            program: None,
            steps: Vec::new(),
        },
    }
}

/// Positions of `node` (output `j`) where a value repairs the output, best
/// repair first.
///
/// Every position is tried by inverse semantics; those with no
/// contradiction are viable. The smallest viable subexpressions (up to
/// [`MAX_ATTEMPTS`]) and the root get a replacement: the exact constant
/// when one exists, else a one-step edit, else (not at the root) a
/// synthesized term. Constants and edits precede synthesized rewrites;
/// replacement and subexpression size precede whole-program size.
pub fn localise(
    subject: &Subject,
    node: &Expr,
    j: usize,
    synthesizer: &Synthesizer<'_>,
    synth_depth: u32,
    others: &[Expr],
) -> Result<Localised> {
    let env = subject.env(Some(j));
    let holes = BTreeMap::new();
    let care = vec![true; subject.size];
    let kind = subject.output_kind(j);
    let root_need = Need::exact(kind, &subject.targets[j], &care);
    let mut viable = Vec::new();
    let mut blocked = 0;
    for (path, sub) in positions(node) {
        if path.is_empty() {
            continue;
        }
        if !crate::inverse::can_derive(node, &path) {
            continue;
        }
        // A sibling that overflows somewhere leaves this position unexplained.
        let Some(need) = skip_overflow(derive(node, &path, &root_need, &env, &holes))? else {
            continue;
        };
        if unsatisfiable(&need).iter().any(|bad| *bad) {
            blocked += 1;
            continue;
        }
        viable.push((size(sub), path, sub, need));
    }
    let viable_count = viable.len();
    viable.sort_by_key(|v| (v.0, core::cmp::Reverse(v.1.len())));
    viable.truncate(MAX_ATTEMPTS);
    viable.push((size(node), Vec::new(), node, root_need));
    let prefer = subject.original_constants();
    let mut found: Vec<(Site, usize)> = Vec::new();
    for (sub_size, path, sub, need) in viable {
        let mut replacement = solve_constant(&need, &prefer).map(Expr::Int);
        let mut found_by = SiteKind::Constant;
        if replacement.is_none() {
            replacement = edit(sub, &need, &env)?;
            found_by = SiteKind::Edit;
        }
        if replacement.is_none() && synth_depth > 0 && !path.is_empty() {
            replacement = solve_expression(&need, synthesizer, synth_depth)?;
            found_by = SiteKind::Synthesised;
        }
        if replacement.is_none() && path.is_empty() {
            continue;
        }
        let mut site = Site {
            output: j,
            path: path.clone(),
            subexpression: sub.clone(),
            size: sub_size,
            replacement: None,
            kind_at: need.kind,
            found_by: None,
            repaired_nodes: None,
        };
        let mut replacement_size = 0;
        if let Some(replacement) = replacement {
            let repaired = replace_at(node, &path, &replacement)?;
            let mut outputs = others.to_vec();
            outputs[j] = repaired;
            if let Ok(program) = lower_outputs(&outputs, &subject.vocab, &subject.original) {
                replacement_size = size(&replacement);
                site.repaired_nodes = Some(program.nodes().len());
                site.replacement = Some(replacement);
                site.found_by = Some(found_by);
            }
        }
        found.push((site, replacement_size));
    }
    found.sort_by(|(a, ra), (b, rb)| {
        (
            a.repaired_nodes.is_none(),
            a.found_by == Some(SiteKind::Synthesised),
            *ra,
            a.size,
            a.repaired_nodes.unwrap_or(0),
        )
            .cmp(&(
                b.repaired_nodes.is_none(),
                b.found_by == Some(SiteKind::Synthesised),
                *rb,
                b.size,
                b.repaired_nodes.unwrap_or(0),
            ))
    });
    Ok(Localised {
        sites: found
            .into_iter()
            .take(MAX_SITES)
            .map(|(site, _)| site)
            .collect(),
        proved_unable: blocked,
        viable: viable_count,
    })
}

/// One-step edits of a subexpression, the mistakes proposers make most: a
/// negated or flipped condition, swapped branches, an off-by-one, the other
/// operator.
#[must_use]
pub fn local_edits(node: &Expr) -> Vec<Expr> {
    let mut out = Vec::new();
    match node {
        Expr::Call(function, args) => {
            use crate::expr::Function;
            let other = match function {
                Function::MulDivFloor => Some(Function::MulDivCeil),
                Function::MulDivCeil => Some(Function::MulDivFloor),
                Function::DivFloor => Some(Function::DivCeil),
                Function::DivCeil => Some(Function::DivFloor),
                Function::ProductsGe => None,
            };
            if let Some(other) = other {
                out.push(Expr::Call(other, args.clone()));
            }
        }
        Expr::If(c, a, b) => out.push(Expr::select((**c).clone(), (**b).clone(), (**a).clone())),
        Expr::Not(a) => out.push((**a).clone()),
        _ => {}
    }
    if let Expr::Bin(op, a, b) = node {
        let (a, b) = ((**a).clone(), (**b).clone());
        if op.is_compare() {
            for other in Op::COMPARE {
                if other != *op {
                    out.push(Expr::bin(other, a.clone(), b.clone()));
                }
            }
            for other in Op::COMPARE {
                out.push(Expr::bin(other, b.clone(), a.clone()));
            }
        }
        if matches!(op, Op::Add | Op::Sub) {
            let swapped = if *op == Op::Add { Op::Sub } else { Op::Add };
            out.push(Expr::bin(swapped, a.clone(), b.clone()));
            if *op == Op::Sub {
                out.push(Expr::bin(Op::Sub, b.clone(), a.clone()));
            }
        }
        if matches!(op, Op::And | Op::Or) {
            let swapped = if *op == Op::And { Op::Or } else { Op::And };
            out.push(Expr::bin(swapped, a.clone(), b.clone()));
            out.push(Expr::bin(*op, Expr::negation(a.clone()), b.clone()));
            out.push(Expr::bin(*op, a, Expr::negation(b)));
        }
    }
    let arithmetic = matches!(node, Expr::Bin(Op::Add | Op::Sub, ..));
    if matches!(
        node,
        Expr::Bin(..) | Expr::If(..) | Expr::Not(_) | Expr::Var(_)
    ) && !arithmetic
    {
        out.push(Expr::negation(node.clone()));
    }
    let logical = matches!(node, Expr::Bin(op, ..) if op.is_compare() || op.is_boolean());
    if matches!(node, Expr::Var(_) | Expr::Bin(..) | Expr::If(..)) && !logical {
        out.push(Expr::bin(Op::Add, node.clone(), Expr::Int(1)));
        out.push(Expr::bin(Op::Sub, node.clone(), Expr::Int(1)));
    }
    out
}

/// The first one-step edit of `sub` that meets the need on every tuple.
fn edit(sub: &Expr, need: &Need, env: &Env<'_>) -> Result<Option<Expr>> {
    let holes: BTreeMap<usize, HoleValue> = BTreeMap::new();
    for candidate in local_edits(sub) {
        let Ok(values) = evaluate(&candidate, env, &holes) else {
            continue;
        };
        if need.kind == Kind::Bool && values.iter().any(|v| *v < 0 || *v > 1) {
            continue;
        }
        if need.accepts_all(&values) {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

fn failing(subject: &Subject, nodes: &[Expr]) -> Option<Vec<Vec<bool>>> {
    let holes = BTreeMap::new();
    let mut out = Vec::with_capacity(nodes.len());
    for (j, node) in nodes.iter().enumerate() {
        let env = subject.env(Some(j));
        let values = evaluate(node, &env, &holes).ok()?;
        let domain = subject.original.outputs()[j];
        out.push(
            values
                .iter()
                .zip(&subject.targets[j])
                .map(|(v, t)| v != t || !domain.contains(*v))
                .collect(),
        );
    }
    Some(out)
}

/// The best greedy step so far: its key (tuples righted, then smaller fix),
/// output, position, fix and the subexpression it replaces.
type Best = ((usize, core::cmp::Reverse<usize>), usize, Path, Expr, Expr);

/// Several faults: repeatedly applies the one-site fix that rights the most
/// failing tuples. A site only has to meet its need where it can; its
/// impossible tuples are left to later steps. Each step is a constant or a
/// one-step edit.
pub fn repair_greedy(
    subject: &Subject,
    nodes: &[Expr],
    settings: &Settings,
) -> Result<(Vec<Expr>, Vec<Step>)> {
    let mut current = nodes.to_vec();
    let mut steps = Vec::new();
    let prefer = subject.original_constants();
    let holes = BTreeMap::new();
    let care = vec![true; subject.size];
    for _ in 0..settings.max_steps {
        let Some(wrong) = failing(subject, &current) else {
            break;
        };
        if !wrong.iter().any(|f| f.iter().any(|x| *x)) {
            break;
        }
        let mut best: Option<Best> = None;
        for (j, f) in wrong.iter().enumerate() {
            if !f.iter().any(|x| *x) {
                continue;
            }
            let env = subject.env(Some(j));
            let root = Need::exact(subject.output_kind(j), &subject.targets[j], &care);
            let before = f.iter().filter(|x| **x).count();
            for (path, sub) in positions(&current[j])
                .into_iter()
                .take(settings.max_positions)
            {
                if path.is_empty() {
                    continue;
                }
                if !crate::inverse::can_derive(&current[j], &path) {
                    continue;
                }
                let Some(need) = skip_overflow(derive(&current[j], &path, &root, &env, &holes))?
                else {
                    continue;
                };
                let bad = unsatisfiable(&need);
                if f.iter().zip(&bad).filter(|(x, _)| **x).all(|(_, b)| *b) {
                    continue;
                }
                let keep: Vec<bool> = bad.iter().map(|b| !b).collect();
                let mut partial = need.restrict(&keep);
                partial.impossible = vec![false; subject.size];
                let fix = match solve_constant(&partial, &prefer) {
                    Some(c) => Some(Expr::Int(c)),
                    None => edit(sub, &partial, &env)?,
                };
                let Some(fix) = fix else {
                    continue;
                };
                let trial = replace_at(&current[j], &path, &fix)?;
                let Ok(values) = evaluate(&trial, &env, &holes) else {
                    continue;
                };
                let after = values
                    .iter()
                    .zip(&subject.targets[j])
                    .filter(|(v, t)| v != t)
                    .count();
                let gain = before.saturating_sub(after);
                let key = (gain, core::cmp::Reverse(size(&fix)));
                if gain > 0 && best.as_ref().is_none_or(|b| key > b.0) {
                    best = Some((key, j, path, fix, sub.clone()));
                }
            }
        }
        let Some((_, j, path, fix, sub)) = best else {
            break;
        };
        current[j] = replace_at(&current[j], &path, &fix)?;
        steps.push(Step {
            output: j,
            path,
            subexpression: sub,
            replacement: fix,
        });
    }
    Ok((current, steps))
}

// Composable loops: preservation and stop-accounting checks for the adapter.
#[cfg(test)]
mod accounting_tests {
    use super::*;
    use crate::expr::{Allow, Vocabulary, parse};
    use zeno_fcis_synthesis::finite::{Domain, Op};

    fn fixture() -> (Subject, Vec<Expr>, Program) {
        let original = Program::try_new(
            vec![Domain::Int { min: 0, max: 3 }],
            vec![Domain::Int {
                min: i128::MIN,
                max: i128::MAX,
            }],
            vec![Op::Input(0)],
            vec![0],
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        let vocabulary = Vocabulary::new(
            vec!["x".into()],
            original.inputs().to_vec(),
            original.outputs().to_vec(),
            &BTreeMap::new(),
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        let subject = Subject::new(original, vocabulary, BTreeMap::new(), None, 4)
            .unwrap_or_else(|e| panic!("{e:?}"));
        let nodes = vec![parse("x + 1", 0, Allow::ALL).unwrap_or_else(|e| panic!("{e:?}"))];
        let candidate = lower_outputs(&nodes, &subject.vocab, &subject.original)
            .unwrap_or_else(|e| panic!("{e:?}"));
        (subject, nodes, candidate)
    }

    #[test]
    fn repair_recorded_preserves_existing_explanation() {
        let (subject, nodes, candidate) = fixture();
        let settings = Settings::default();
        let old = explain(&subject, &nodes, &candidate, true, &settings)
            .unwrap_or_else(|e| panic!("{e:?}"));
        let recorded = explain_recorded(&subject, &nodes, &candidate, true, &settings);
        let new = recorded.outcome.unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(old.failing, new.failing);
        assert_eq!(old.region, new.region);
        assert_eq!(old.localised, new.localised);
        assert_eq!(
            old.repair
                .as_ref()
                .map(|r| (&r.status, &r.program, &r.expressions)),
            new.repair
                .as_ref()
                .map(|r| (&r.status, &r.program, &r.expressions))
        );
        assert!(recorded.synthesis_work_used <= settings.synthesis.total_work);
    }

    #[test]
    fn repair_recorded_exposes_a_hidden_solver_stop() {
        let (subject, _, _) = fixture();
        let nodes =
            vec![parse("x == 0 ? x : x + 1", 0, Allow::ALL).unwrap_or_else(|e| panic!("{e:?}"))];
        let candidate = lower_outputs(&nodes, &subject.vocab, &subject.original)
            .unwrap_or_else(|e| panic!("{e:?}"));
        let mut settings = Settings::default();
        settings.synthesis.total_work = 0;
        let recorded = explain_recorded(&subject, &nodes, &candidate, true, &settings);
        assert!(recorded.synthesis_stopped);
        assert_eq!(recorded.synthesis_work_used, 0);
        // Original behavior can still produce local edits; strict callers must
        // honor the recorded stop rather than treating them as a completed run.
        assert!(recorded.outcome.is_ok());
    }
}
