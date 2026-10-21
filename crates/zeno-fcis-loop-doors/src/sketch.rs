//! The sketch door: the proposer supplies structure, the solver supplies
//! constants and subterms.
//!
//! A sketch is one expression per output with holes. Outputs are independent
//! targets, so each output's holes are solved on their own:
//!
//! - a single hole is solved exactly by inverse semantics: a constant hole
//!   has a complete answer over signed 128-bit constants (a value, or a tuple
//!   where no value works), a choice hole is decided by trying each alternative, and
//!   an expression hole is synthesized top-down (incomplete: "not found" is
//!   never "impossible");
//! - with several holes, the meanings search freezes the joint pivot,
//!   initial candidate spaces and reading order, then quotients that traversal
//!   by equal parent vectors and independently solvable holes. Only terminal
//!   assignments or disjoint rejected subtrees spend the assignment budget;
//!   visits and class-value work are reported separately. A stopped budget
//!   cannot establish an empty space. This preserves the joint assignment
//!   prefix for constant/choice holes, not a runtime or total-work bound;
//! - [`Search::Joint`], kept for comparison, enumerates all holes but one
//!   (the *pivot*: the expression hole, else the widest constant hole) over
//!   their candidate sets in a fixed order, nearest zero first, each
//!   filtered on a seeded stratified sample before the pivot is derived on
//!   the whole domain.
//!
//! A candidate whose `+` or `-` would leave the signed 128-bit range on some
//! tuple is skipped: the lowered program would trap there.
//!
//! A constant hole compared with a hole-free expression gets a complete
//! candidate range: that expression's values, widened by one, since every
//! other constant compares like one of the ends. Explicit ranges and choice
//! holes are complete too. Other constant holes get default candidates,
//! which are not complete.
//!
//! "Unrealizable" is claimed only with a witness input (a tuple where no
//! value of a hole alone on its route makes the output right, whatever the
//! other holes hold), or after a complete stated space was exhausted: every
//! assignment with a constant or choice pivot, or every effect class with
//! every alone hole solved exactly.
//! The completed program's status here comes from the library evaluator and
//! is untrusted; the `loop` command judges it with `transform check`.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use zeno_fcis_synthesis::finite::Program;

use crate::expr::{
    Env, Expr, Fill, Hole, Kind, Op, constants, evaluate, fill, has_holes, hole_kinds, holes_of,
    kind_of, params_of, renumber, size, skip_overflow,
};
use crate::inverse::{
    Need, Synthesis, Synthesizer, derive, derive_loose, first_unsatisfiable, node_at, positions,
    solve_constant, solve_expression,
};
use crate::lower::{cost, lower_outputs};
use crate::repair::first_difference;
use crate::subject::{Subject, domain_size};
use crate::{Refusal, Result};

mod targets;
pub use targets::{TargetProblem, TargetResult, propose_targets};

/// How an output with several holes is searched.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Search {
    /// Holes alone on their route solved exactly, the rest branched over
    /// effect classes.
    Meanings,
    /// Every hole but a pivot enumerated jointly over its candidate values.
    Joint,
}

/// Door settings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Settings {
    /// The search for outputs with several holes.
    pub search: Search,
    /// Most enumerated assignments per output.
    pub max_assignments: usize,
    /// Most evaluation work for one sketch, counted as tuples times
    /// expression nodes per evaluation or derivation.
    pub max_work: u64,
    /// Depth for expression holes.
    pub synth_depth: u32,
    /// Sample size for the filter.
    pub sample: usize,
    /// Sampling seed.
    pub seed: u64,
    /// Top-down limits.
    pub synthesis: Synthesis,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            search: Search::Meanings,
            max_assignments: 20_000,
            max_work: 50_000_000,
            synth_depth: 4,
            sample: 512,
            seed: 0,
            synthesis: Synthesis::default(),
        }
    }
}

/// Completions kept per output.
pub const MAX_COMPLETIONS: usize = 8;
/// Completion combinations lowered.
pub const MAX_COMBINATIONS: usize = 256;
/// Default candidates nearest zero.
pub const MAX_CANDIDATES: usize = 48;

/// The candidate set searched for one enumerated hole.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Searched {
    /// The hole.
    pub hole: usize,
    /// Its candidates (choice: alternative indices), in search order.
    pub values: Vec<i128>,
    /// Whether the set is complete: no value outside it behaves differently.
    pub complete: bool,
    /// Whether the hole is a choice hole.
    pub choice: bool,
}

/// A tuple where no value of a single hole makes the output right.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Witness {
    /// The output.
    pub output: usize,
    /// The tuple.
    pub ordinal: usize,
}

/// What was searched for one output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputSearch {
    /// The output.
    pub output: usize,
    /// Its holes.
    pub holes: Vec<usize>,
    /// The pivot, derived exactly.
    pub pivot: Option<usize>,
    /// Enumerated holes.
    pub searched: Vec<Searched>,
    /// Assignments in the full product (saturating).
    pub assignments: u128,
    /// Terminal assignments or disjoint rejected subtrees charged.
    pub tried: usize,
    /// Internal visits (not charged as assignments).
    pub visits: usize,
    /// Candidate values considered while constructing effect classes.
    pub class_values: usize,
    /// When no completion was found: why.
    pub verdict: Option<Status>,
    /// The witness, for an unrealizable output.
    pub witness: Option<Witness>,
    /// Inputs imposing incompatible values on one shared constant, when a
    /// two-input conflict explains exhaustion. These are not single-input witnesses.
    pub conflicting_inputs: Vec<usize>,
    /// The search used: `"single"`, `"meanings"` or `"joint"`.
    pub search: &'static str,
    /// Holes solved exactly by themselves (search over meanings).
    pub solved_alone: usize,
}

/// A door outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Status {
    /// Every output completed and the program agreed with the original on
    /// every tuple (library evaluator; the checker still decides).
    Completed,
    /// No completion exists: a witness, or a complete space exhausted.
    Unrealizable,
    /// Nothing found; claims nothing.
    NotFound,
    /// The enumeration budget ran out first.
    Inconclusive,
    /// The sketch was refused.
    Refused,
}

impl Status {
    /// The status's name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Unrealizable => "unrealizable",
            Self::NotFound => "not-found",
            Self::Inconclusive => "inconclusive",
            Self::Refused => "refused",
        }
    }
}

/// The result of one sketch.
#[derive(Clone, Debug)]
pub struct SketchResult {
    /// The outcome.
    pub status: Status,
    /// The completed program, when completed.
    pub program: Option<Program>,
    /// The completed expressions.
    pub expressions: Vec<Expr>,
    /// Hole fills, by renumbered hole.
    pub holes: BTreeMap<usize, Fill>,
    /// The renumbered sketch.
    pub sketch: Vec<Expr>,
    /// Per output, what was searched.
    pub outputs: Vec<OutputSearch>,
    /// Assignments evaluated in all.
    pub evaluated: usize,
    /// Why, when not completed.
    pub reason: Option<String>,
}

impl SketchResult {
    fn stopped(status: Status, reason: String) -> Self {
        Self {
            status,
            program: None,
            expressions: Vec::new(),
            holes: BTreeMap::new(),
            sketch: Vec::new(),
            outputs: Vec::new(),
            evaluated: 0,
            reason: Some(reason),
        }
    }
}

#[derive(Clone, Debug)]
struct Completion {
    assignment: BTreeMap<usize, Fill>,
    expression: Expr,
    nodes: usize,
}

/// A small deterministic generator (SplitMix64).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// `count` distinct items of `items` (all of them when fewer).
    fn choose(&mut self, items: &[usize], count: usize) -> Vec<usize> {
        let mut pool = items.to_vec();
        let count = count.min(pool.len());
        for i in 0..count {
            let span = (pool.len() - i) as u64;
            let j = i + usize::try_from(self.next() % span).unwrap_or(0);
            pool.swap(i, j);
        }
        pool.truncate(count);
        pool
    }
}

/// Tuple ordinals: a seeded random sample plus a stratum per distinct target
/// value, so rare values are represented.
fn sample(target: &[i128], size: usize, seed: u64) -> Vec<usize> {
    let pool: Vec<usize> = (0..target.len()).collect();
    if pool.len() <= size {
        return pool;
    }
    let mut rng = Rng(seed);
    let mut chosen: BTreeSet<usize> = rng.choose(&pool, size).into_iter().collect();
    let mut groups: BTreeMap<i128, Vec<usize>> = BTreeMap::new();
    for (t, value) in target.iter().enumerate() {
        if groups.len() > 64 {
            break;
        }
        groups.entry(*value).or_default().push(t);
    }
    if groups.len() <= 64 {
        let per = (size / 16).max(4);
        for members in groups.values() {
            chosen.extend(rng.choose(members, per));
        }
    }
    chosen.into_iter().collect()
}

fn nearest_zero(values: impl IntoIterator<Item = i128>) -> Vec<i128> {
    let mut out: Vec<i128> = values
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    out.sort_by_key(|v| (v.unsigned_abs(), *v));
    out
}

/// Constants worth trying for a hole without a complete range, nearest zero
/// first: those of the sketch and of the original program, case indices,
/// small domain bounds and their neighbours, family parameters and small
/// integers, then the most frequent offsets between an input field and an
/// output.
#[must_use]
pub fn default_candidates(
    subject: &Subject,
    kind: Kind,
    sketch_constants: &BTreeSet<i128>,
) -> Vec<i128> {
    if kind == Kind::Bool {
        return vec![0, 1];
    }
    let mut found: BTreeSet<i128> = [-1, 0, 1, 2].into_iter().collect();
    found.extend(sketch_constants.iter().copied());
    found.extend(subject.original_constants());
    if let Some(cases) = subject.cases {
        found.extend(0..cases.clamp(0, 1 << 16));
    }
    for domain in subject.vocab.inputs().iter().chain(subject.vocab.outputs()) {
        if Kind::of(*domain) == Some(Kind::Int) && domain_size(*domain) <= 1 << 20 {
            let (lo, hi) = domain.bounds();
            found.extend([lo, hi, lo.saturating_sub(1), hi.saturating_add(1)]);
        }
    }
    found.extend(subject.parameters.values().copied());
    let mut base = nearest_zero(found.iter().copied());
    base.truncate(MAX_CANDIDATES);
    // The most frequent offsets between an input and an output, in first-seen
    // order, then by total count.
    let mut mined: Vec<(i128, usize)> = Vec::new();
    let kinds = subject.vocab.input_kinds();
    for (k, column) in subject.columns.iter().enumerate() {
        if kinds[k] != Kind::Int {
            continue;
        }
        for (j, target) in subject.targets.iter().enumerate() {
            if subject.output_kind(j) != Kind::Int {
                continue;
            }
            for flip in [false, true] {
                let mut counts: BTreeMap<i128, usize> = BTreeMap::new();
                for (x, t) in column.iter().zip(target) {
                    let diff = if flip {
                        t.wrapping_sub(*x)
                    } else {
                        x.wrapping_sub(*t)
                    };
                    *counts.entry(diff).or_default() += 1;
                }
                let mut ranked: Vec<(i128, usize)> = counts.into_iter().collect();
                ranked.sort_by_key(|entry| core::cmp::Reverse(entry.1));
                for (value, count) in ranked.into_iter().take(4) {
                    match mined.iter_mut().find(|(v, _)| *v == value) {
                        Some(entry) => entry.1 += count,
                        None => mined.push((value, count)),
                    }
                }
            }
        }
    }
    mined.sort_by_key(|entry| core::cmp::Reverse(entry.1));
    base.extend(
        mined
            .into_iter()
            .map(|(v, _)| v)
            .filter(|v| !found.contains(v))
            .take(16),
    );
    base
}

/// Borrowed invocation inputs shared by original-based and explicit-target search.
/// The optional original is available only to the legacy mode.
#[derive(Clone, Copy)]
struct TargetView<'a> {
    vocab: &'a crate::expr::Vocabulary,
    columns: &'a [Vec<i128>],
    targets: &'a [Vec<i128>],
    size: usize,
    original: Option<&'a Subject>,
}

impl<'a> TargetView<'a> {
    fn from_subject(subject: &'a Subject) -> Self {
        Self {
            vocab: &subject.vocab,
            columns: &subject.columns,
            targets: &subject.targets,
            size: subject.size,
            original: Some(subject),
        }
    }

    fn env(self, keep: Option<usize>) -> Env<'a> {
        Env {
            vocab: self.vocab,
            columns: self.columns,
            keep: self.original.and(keep).map(|j| self.targets[j].as_slice()),
            len: self.size,
        }
    }

    fn output_kind(self, j: usize) -> Kind {
        self.vocab.output_kinds()[j]
    }

    fn original_constants(self) -> Vec<i128> {
        self.original
            .map_or_else(Vec::new, Subject::original_constants)
    }
}

struct Solver<'a> {
    subject: TargetView<'a>,
    settings: Settings,
    synthesizer: Option<Synthesizer<'a>>,
    evaluated: usize,
    work: u64,
}

fn hole_id(hole: &Expr) -> usize {
    match hole {
        Expr::Hole(k, _) => *k,
        _ => usize::MAX,
    }
}

fn path_to_hole(node: &Expr, k: usize) -> Result<Vec<u8>> {
    positions(node)
        .into_iter()
        .find(|(_, sub)| matches!(sub, Expr::Hole(id, _) if *id == k))
        .map(|(path, _)| path)
        .ok_or_else(|| Refusal::new(format!("hole-not-found:{k}")))
}

/// A single unbounded constant hole with coefficient +1. Offsets and the
/// intersection of every intermediate's safe integer range are invocation
/// scratch; overflow never creates a supposedly complete search space.
fn affine_hole(node: &Expr, env: &Env<'_>) -> Option<(usize, Vec<i128>, i128, i128)> {
    if let Expr::Hole(k, Hole::Const(None)) = node {
        return Some((*k, vec![0; env.len], i128::MIN, i128::MAX));
    }
    let Expr::Bin(op @ (Op::Add | Op::Sub), a, b) = node else {
        return None;
    };
    let (hole, sibling) = if has_holes(a) && !has_holes(b) {
        (a, b)
    } else if *op == Op::Add && !has_holes(a) && has_holes(b) {
        (b, a)
    } else {
        return None;
    };
    let (k, mut offsets, mut lo, mut hi) = affine_hole(hole, env)?;
    let values = evaluate(sibling, env, &BTreeMap::new()).ok()?;
    for (offset, value) in offsets.iter_mut().zip(values) {
        *offset = if *op == Op::Add {
            offset.checked_add(value)?
        } else {
            offset.checked_sub(value)?
        };
        lo = lo.max(i128::MIN.saturating_sub(*offset));
        hi = hi.min(i128::MAX.saturating_sub(*offset));
    }
    (lo <= hi).then_some((k, offsets, lo, hi))
}

fn constant_conflict(need: &Need) -> Vec<usize> {
    if let Some(a) = (0..need.len()).find(|t| need.has_eq[*t]) {
        let value = need.eq[a];
        if let Some(b) = (0..need.len()).find(|t| {
            (need.has_eq[*t] && need.eq[*t] != value)
                || need.lo[*t] > value
                || need.hi[*t] < value
                || need
                    .neq
                    .iter()
                    .any(|(values, mask)| mask[*t] && values[*t] == value)
        }) {
            return vec![a, b];
        }
    }
    let lo = (0..need.len()).max_by_key(|t| need.lo[*t]);
    let hi = (0..need.len()).min_by_key(|t| need.hi[*t]);
    match (lo, hi) {
        (Some(a), Some(b)) if need.lo[a] > need.hi[b] => vec![a, b],
        _ => Vec::new(),
    }
}

impl Solver<'_> {
    fn completion_limit(&self) -> usize {
        if self.subject.original.is_some() {
            MAX_COMPLETIONS
        } else {
            1
        }
    }

    fn retain_completions(&self, found: &mut Vec<Completion>) {
        if self.subject.original.is_some() {
            found.sort_by_key(|c| c.nodes);
            found.truncate(3);
        }
    }

    /// Legacy accounting is unchanged. Explicit-target calls reserve work
    /// before evaluation and retain all charged work when refusing a budget.
    fn charge(&mut self, work: u64) -> Result<()> {
        if self.subject.original.is_none()
            && work > self.settings.max_work.saturating_sub(self.work)
        {
            return Err(Refusal::new("sketch-work-budget"));
        }
        self.work = self.work.saturating_add(work);
        Ok(())
    }

    fn full_env(&self, j: usize) -> Env<'_> {
        self.subject.env(Some(j))
    }

    /// Comparisons against a hole-free integer expression, including
    /// additive offsets around a hole. Threshold neighbours represent all
    /// comparison outcomes; intersect with the eager-overflow-safe range.
    fn comparison_ranges(&mut self, node: &Expr) -> Result<BTreeMap<usize, (i128, i128)>> {
        let mut out = BTreeMap::new();
        let env = self.subject.env(None);
        let none = BTreeMap::new();
        for (_, sub) in positions(node) {
            let Expr::Bin(op, a, b) = sub else {
                continue;
            };
            if !op.is_compare() {
                continue;
            }
            let (hole, sibling) = if has_holes(a) && !has_holes(b) {
                (a, b)
            } else if !has_holes(a) && has_holes(b) {
                (b, a)
            } else {
                continue;
            };
            if kind_of(sibling, self.subject.vocab)? == Some(Kind::Bool) {
                continue;
            }
            if self.subject.original.is_none() {
                self.charge((size(node) as u64).saturating_mul(env.len as u64))?;
            }
            let Some((k, offsets, safe_lo, safe_hi)) = affine_hole(hole, &env) else {
                continue;
            };
            let Ok(values) = evaluate(sibling, &env, &none) else {
                continue;
            };
            let Some(thresholds) = values
                .iter()
                .zip(offsets)
                .map(|(value, offset)| value.checked_sub(offset))
                .collect::<Option<Vec<i128>>>()
            else {
                continue;
            };
            let (Some(min), Some(max)) = (thresholds.iter().min(), thresholds.iter().max()) else {
                continue;
            };
            let Some(lo) = min.checked_sub(1).map(|n| n.clamp(safe_lo, safe_hi)) else {
                continue;
            };
            let Some(hi) = max.checked_add(1).map(|n| n.clamp(safe_lo, safe_hi)) else {
                continue;
            };
            if hi.checked_sub(lo).is_some_and(|n| n <= 4096) {
                out.insert(k, (lo, hi));
            }
        }
        Ok(out)
    }

    fn width(hole: &Expr, compared: &BTreeMap<usize, (i128, i128)>) -> i128 {
        match hole {
            Expr::Hole(_, Hole::Const(Some((lo, hi)))) => hi
                .checked_sub(*lo)
                .and_then(|n| n.checked_add(1))
                .unwrap_or(i128::MAX),
            Expr::Hole(k, Hole::Const(None)) => compared.get(k).map_or(1 << 30, |(lo, hi)| {
                hi.checked_sub(*lo)
                    .and_then(|n| n.checked_add(1))
                    .unwrap_or(i128::MAX)
            }),
            Expr::Hole(_, Hole::Choice(alternatives)) => {
                i128::try_from(alternatives.len()).unwrap_or(0)
            }
            _ => 0,
        }
    }

    fn space(
        &self,
        hole: &Expr,
        kinds: &BTreeMap<usize, Kind>,
        sketch_constants: &BTreeSet<i128>,
        compared: &BTreeMap<usize, (i128, i128)>,
    ) -> Result<Searched> {
        let Expr::Hole(k, kind) = hole else {
            return Err(Refusal::new("hole-expected"));
        };
        let (values, complete, choice) = match kind {
            Hole::Choice(alternatives) => (
                (0..i128::try_from(alternatives.len()).unwrap_or(0)).collect(),
                true,
                true,
            ),
            Hole::Expr => return Err(Refusal::new("expression-hole-must-be-the-pivot")),
            Hole::Const(Some((lo, hi))) => (nearest_zero(*lo..=*hi), true, false),
            Hole::Const(None) => match compared.get(k) {
                Some((lo, hi)) => (nearest_zero(*lo..=*hi), true, false),
                None => (
                    default_candidates(
                        self.subject
                            .original
                            .ok_or_else(|| Refusal::new("target-unbounded-hole"))?,
                        kinds.get(k).copied().unwrap_or(Kind::Int),
                        sketch_constants,
                    ),
                    false,
                    false,
                ),
            },
        };
        Ok(Searched {
            hole: *k,
            values,
            complete,
            choice,
        })
    }

    fn meet(
        &mut self,
        pivot: &Expr,
        need: &Need,
        sketch_constants: &BTreeSet<i128>,
    ) -> Result<Option<Fill>> {
        match pivot {
            Expr::Hole(_, Hole::Const(range)) => {
                let mut need = need.clone();
                if let Some((lo, hi)) = range {
                    for v in &mut need.lo {
                        *v = (*v).max(*lo);
                    }
                    for v in &mut need.hi {
                        *v = (*v).min(*hi);
                    }
                }
                let mut prefer: BTreeSet<i128> = sketch_constants.clone();
                prefer.extend(self.subject.original_constants());
                let prefer = nearest_zero(prefer);
                Ok(solve_constant(&need, &prefer).map(Fill::Const))
            }
            Expr::Hole(_, Hole::Choice(alternatives)) => {
                let env = self.subject.env(None);
                let none = BTreeMap::new();
                for (index, alternative) in alternatives.iter().enumerate() {
                    let resolved = self.subject.vocab.resolve_variants(alternative)?;
                    if self.subject.original.is_none() {
                        self.charge((size(&resolved) as u64).saturating_mul(env.len as u64))?;
                    }
                    // An alternative that overflows cannot be a completion;
                    // it does not hide the others.
                    let Some(values) = skip_overflow(evaluate(&resolved, &env, &none))? else {
                        continue;
                    };
                    if need.accepts_all(&values) {
                        return Ok(Some(Fill::Choice(index)));
                    }
                }
                Ok(None)
            }
            Expr::Hole(_, Hole::Expr) => Ok(solve_expression(
                need,
                self.synthesizer
                    .as_ref()
                    .ok_or_else(|| Refusal::new("target-expression-hole"))?,
                self.settings.synth_depth,
            )?
            .map(Fill::Ast)),
            _ => Err(Refusal::new("hole-expected")),
        }
    }

    fn solve_output(
        &mut self,
        j: usize,
        node: &Expr,
        kinds: &BTreeMap<usize, Kind>,
        sketch_constants: &BTreeSet<i128>,
    ) -> Result<(Vec<Completion>, OutputSearch)> {
        let view = self.subject;
        let target = &view.targets[j];
        let holes: Vec<Expr> = holes_of(node).into_iter().cloned().collect();
        let mut info = OutputSearch {
            output: j,
            holes: holes.iter().map(hole_id).collect(),
            pivot: None,
            searched: Vec::new(),
            assignments: 1,
            tried: 0,
            visits: 0,
            class_values: 0,
            verdict: None,
            witness: None,
            conflicting_inputs: Vec::new(),
            search: if holes.len() > 1 { "joint" } else { "single" },
            solved_alone: 0,
        };
        if holes.is_empty() {
            if self.subject.original.is_none() {
                self.charge((size(node) as u64).saturating_mul(self.subject.size as u64))?;
                if evaluate(node, &self.full_env(j), &BTreeMap::new())? != *target {
                    info.verdict = Some(Status::Unrealizable);
                    return Ok((Vec::new(), info));
                }
            }
            return Ok((
                vec![Completion {
                    assignment: BTreeMap::new(),
                    expression: node.clone(),
                    nodes: 0,
                }],
                info,
            ));
        }
        let expression_holes: Vec<&Expr> = holes
            .iter()
            .filter(|h| matches!(h, Expr::Hole(_, Hole::Expr)))
            .collect();
        if expression_holes.len() > 1 {
            return Err(Refusal::new(format!(
                "one-expression-hole-per-output:out{j}"
            )));
        }
        if self.settings.search == Search::Meanings && holes.len() > 1 {
            return self.meanings_output(j, node, kinds, sketch_constants, info);
        }
        let compared = self.comparison_ranges(node)?;
        let pivot: Option<Expr> = match expression_holes.first() {
            Some(hole) => Some((*hole).clone()),
            None => {
                let mut best: Option<&Expr> = None;
                for hole in holes
                    .iter()
                    .filter(|h| matches!(h, Expr::Hole(_, Hole::Const(_))))
                {
                    if best.is_none_or(|b| Self::width(hole, &compared) > Self::width(b, &compared))
                    {
                        best = Some(hole);
                    }
                }
                best.cloned()
            }
        };
        let pivot = match pivot {
            Some(hole)
                if !crate::inverse::can_derive(node, &path_to_hole(node, hole_id(&hole))?) =>
            {
                if matches!(hole, Expr::Hole(_, Hole::Expr)) {
                    info.verdict = Some(Status::NotFound);
                    return Ok((Vec::new(), info));
                }
                None
            }
            other => other,
        };
        info.pivot = pivot.as_ref().map(hole_id);
        let others: Vec<&Expr> = holes
            .iter()
            .filter(|h| Some(hole_id(h)) != info.pivot)
            .collect();
        for hole in &others {
            info.searched
                .push(self.space(hole, kinds, sketch_constants, &compared)?);
        }
        info.assignments = info.searched.iter().fold(1u128, |acc, s| {
            acc.saturating_mul(s.values.len().max(1) as u128)
        });
        let rows = sample(
            target,
            self.settings.sample,
            self.settings.seed.wrapping_add(j as u64),
        );
        let sample_columns: Vec<Vec<i128>> = self
            .subject
            .columns
            .iter()
            .map(|column| rows.iter().map(|t| column[*t]).collect())
            .collect();
        let sample_target: Vec<i128> = rows.iter().map(|t| target[*t]).collect();
        let sample_env = Env {
            vocab: self.subject.vocab,
            columns: &sample_columns,
            keep: self.subject.original.map(|_| sample_target.as_slice()),
            len: rows.len(),
        };
        let want = self.subject.output_kind(j);
        let care = vec![true; self.subject.size];
        let sample_care = vec![true; rows.len()];
        let none = BTreeMap::new();
        let mut found: Vec<Completion> = Vec::new();
        let mut witness = None;
        let mut index = vec![0usize; others.len()];
        let mut exhausted = false;
        let mut tried = 0;
        while tried < self.settings.max_assignments && !exhausted {
            if self.work > self.settings.max_work {
                break;
            }
            tried += 1;
            self.evaluated += 1;
            let mut assignment = BTreeMap::new();
            for (s, i) in info.searched.iter().zip(&index) {
                let value = s.values.get(*i).copied().unwrap_or(0);
                assignment.insert(
                    s.hole,
                    if s.choice {
                        Fill::Choice(usize::try_from(value).unwrap_or(0))
                    } else {
                        Fill::Const(value)
                    },
                );
            }
            // advance the odometer, last hole fastest
            exhausted = true;
            for k in (0..index.len()).rev() {
                index[k] += 1;
                if index[k] < info.searched[k].values.len().max(1) {
                    exhausted = false;
                    break;
                }
                index[k] = 0;
            }
            let partial = fill(node, &assignment)?;
            let cost = size(&partial) as u64;
            self.charge(cost.saturating_mul(rows.len() as u64))?;
            let Some(pivot) = &pivot else {
                let Ok(got) = evaluate(&partial, &sample_env, &none) else {
                    continue;
                };
                if got != sample_target {
                    continue;
                }
                self.charge(cost.saturating_mul(self.subject.size as u64))?;
                let Some(full) = skip_overflow(evaluate(&partial, &self.full_env(j), &none))?
                else {
                    continue;
                };
                if full == *target {
                    let nodes = size(&partial);
                    found.push(Completion {
                        assignment,
                        expression: partial,
                        nodes,
                    });
                    if found.len() >= self.completion_limit() {
                        break;
                    }
                }
                continue;
            };
            let pivot_id = hole_id(pivot);
            let path = path_to_hole(&partial, pivot_id)?;
            let derived = derive(
                &partial,
                &path,
                &Need::exact(want, &sample_target, &sample_care),
                &sample_env,
                &none,
            );
            // With other holes, an overflowing assignment is skipped; with
            // none, the overflow does not depend on the hole: refused.
            let Some(need_sample) = (if others.is_empty() {
                Some(derived?)
            } else {
                skip_overflow(derived)?
            }) else {
                continue;
            };
            let need_sample_bad = first_unsatisfiable(&need_sample).is_some();
            if need_sample_bad && !others.is_empty() {
                continue;
            }
            self.charge(cost.saturating_mul(self.subject.size as u64))?;
            let derived = derive(
                &partial,
                &path,
                &Need::exact(want, target, &care),
                &self.full_env(j),
                &none,
            );
            let Some(need) = (if others.is_empty() {
                Some(derived?)
            } else {
                skip_overflow(derived)?
            }) else {
                continue;
            };
            if let Some(ordinal) = first_unsatisfiable(&need) {
                if others.is_empty() {
                    witness = Some(Witness { output: j, ordinal });
                }
                continue;
            }
            let Some(filled) = self.meet(pivot, &need, sketch_constants)? else {
                if others.is_empty() && matches!(pivot, Expr::Hole(_, Hole::Const(None))) {
                    info.conflicting_inputs = constant_conflict(&need);
                }
                continue;
            };
            let mut one = BTreeMap::new();
            one.insert(pivot_id, filled.clone());
            let complete = fill(&partial, &one)?;
            assignment.insert(pivot_id, filled);
            let nodes = size(&complete);
            found.push(Completion {
                assignment,
                expression: complete,
                nodes,
            });
            if found.len() >= self.completion_limit() {
                break;
            }
        }
        info.tried = tried;
        if found.is_empty() {
            let all_complete = info.searched.iter().all(|s| s.complete);
            let pivot_exact = !matches!(pivot, Some(Expr::Hole(_, Hole::Expr)));
            info.verdict = Some(if witness.is_some() && others.is_empty() {
                info.witness = witness;
                Status::Unrealizable
            } else if (tried as u128) < info.assignments {
                Status::Inconclusive
            } else if all_complete && pivot_exact {
                Status::Unrealizable
            } else {
                Status::NotFound
            });
        }
        self.retain_completions(&mut found);
        Ok((found, info))
    }
}

/// Whether the hole at `path` can be solved by itself: every test above it
/// and every operand beside it is hole-free. Holes in the other branch of a
/// `c ? a : b` do not count: its tuples take the other route.
fn alone(node: &Expr, path: &[u8]) -> bool {
    if !crate::inverse::can_derive(node, path) {
        return false;
    }
    let mut node = node;
    for i in path {
        let blocked = match node {
            Expr::If(c, a, b) => {
                (*i == 0 && (has_holes(a) || has_holes(b))) || (*i > 0 && has_holes(c))
            }
            Expr::Bin(_, a, b) => has_holes(if *i == 0 { b } else { a }),
            _ => false,
        };
        if blocked {
            return false;
        }
        match crate::expr::children(node).get(usize::from(*i)) {
            Some(child) => node = child,
            None => return false,
        }
    }
    true
}

/// Search state for one output's search over meanings.
#[derive(Default)]
struct Meanings {
    found: Vec<Completion>,
    tried: usize,
    visits: usize,
    class_values: usize,
    exhaustive: bool,
    budget: bool,
    witness: Option<Witness>,
    solved_alone: usize,
    searched: Vec<Searched>,
}

/// Joint order fixed before independently solving any hole.
struct MeaningOrder {
    holes: Vec<Expr>,
    spaces: Vec<Searched>,
}

/// Why a branch of the search over meanings has no completion.
enum Dead {
    /// A tuple where a hole alone on its route has no right value.
    Witness(usize),
    /// A hole alone on its route has no value at all (its space is complete).
    Unmet,
}

impl Solver<'_> {
    fn over_budget(&self) -> bool {
        self.work > self.settings.max_work
    }

    /// Several holes: quotient the frozen joint order with terminal charging.
    fn meanings_output(
        &mut self,
        j: usize,
        node: &Expr,
        kinds: &BTreeMap<usize, Kind>,
        sketch_constants: &BTreeSet<i128>,
        mut info: OutputSearch,
    ) -> Result<(Vec<Completion>, OutputSearch)> {
        info.search = "meanings";
        let mut state = Meanings {
            exhaustive: true,
            ..Meanings::default()
        };
        let compared = self.comparison_ranges(node)?;
        let holes: Vec<Expr> = holes_of(node).into_iter().cloned().collect();
        let pivot = holes
            .iter()
            .find(|h| matches!(h, Expr::Hole(_, Hole::Expr)))
            .or_else(|| {
                let mut best = None;
                for h in holes
                    .iter()
                    .filter(|h| matches!(h, Expr::Hole(_, Hole::Const(_))))
                {
                    if best.is_none_or(|b| Self::width(h, &compared) > Self::width(b, &compared)) {
                        best = Some(h);
                    }
                }
                best
            });
        info.pivot = pivot.map(hole_id);
        let others: Vec<Expr> = holes
            .iter()
            .filter(|h| Some(hole_id(h)) != info.pivot)
            .cloned()
            .collect();
        let spaces = others
            .iter()
            .map(|h| self.space(h, kinds, sketch_constants, &compared))
            .collect::<Result<Vec<_>>>()?;
        info.assignments = spaces
            .iter()
            .fold(1u128, |n, s| n.saturating_mul(s.values.len() as u128));
        let order = MeaningOrder {
            holes: others,
            spaces,
        };
        match self.visit(
            j,
            node.clone(),
            BTreeMap::new(),
            0,
            &order,
            sketch_constants,
            &mut state,
        ) {
            Err(refusal) if refusal.0 == "sketch-work-budget" => state.budget = true,
            Err(refusal) => return Err(refusal),
            Ok(()) => {}
        }
        info.tried = state.tried;
        info.visits = state.visits;
        info.class_values = state.class_values;
        info.solved_alone = state.solved_alone;
        info.searched = state.searched;
        self.evaluated += state.tried;
        let mut found = state.found;
        if found.is_empty() {
            info.verdict = Some(if state.budget {
                Status::Inconclusive
            } else if state.witness.is_some() {
                info.witness = state.witness;
                Status::Unrealizable
            } else if state.exhaustive {
                Status::Unrealizable
            } else {
                Status::NotFound
            });
        }
        self.retain_completions(&mut found);
        Ok((found, info))
    }

    #[allow(clippy::too_many_arguments)]
    fn visit(
        &mut self,
        j: usize,
        node: Expr,
        assignment: BTreeMap<usize, Fill>,
        depth: usize,
        order: &MeaningOrder,
        sketch_constants: &BTreeSet<i128>,
        state: &mut Meanings,
    ) -> Result<()> {
        if state.found.len() >= self.completion_limit() {
            return Ok(());
        }
        if state.tried >= self.settings.max_assignments || self.over_budget() {
            state.budget = true;
            return Ok(());
        }
        state.visits += 1;
        let (node, solved, dead) = self.solve_alone(j, node, sketch_constants)?;
        state.solved_alone += solved.len();
        let mut assignment = assignment;
        assignment.extend(solved);
        if self.over_budget() {
            state.budget = true;
            return Ok(());
        }
        match dead {
            // Exact: no completion down this branch. At the top, a tuple
            // where no value works is a witness for the whole output.
            Some(Dead::Witness(ordinal)) => {
                state.tried += 1;
                if depth == 0 {
                    state.witness = Some(Witness { output: j, ordinal });
                }
                return Ok(());
            }
            Some(Dead::Unmet) => {
                state.tried += 1;
                return Ok(());
            }
            None => {}
        }
        let holes: Vec<Expr> = holes_of(&node).into_iter().cloned().collect();
        let ids: BTreeSet<usize> = holes.iter().map(hole_id).collect();
        if let Some((hole, space)) = order
            .holes
            .iter()
            .zip(&order.spaces)
            .find(|(h, _)| ids.contains(&hole_id(h)))
        {
            state.class_values = state.class_values.saturating_add(space.values.len());
            let classes = self.classes(j, &node, hole, space)?;
            state.exhaustive &= space.complete;
            if !state.searched.iter().any(|s| s.hole == hole_id(hole)) {
                state.searched.push(Searched {
                    hole: hole_id(hole),
                    values: classes.clone(),
                    complete: space.complete,
                    choice: space.choice,
                });
            }
            if classes.is_empty() {
                state.tried += 1;
            }
            for value in classes {
                let value = if space.choice {
                    Fill::Choice(usize::try_from(value).map_err(|_| Refusal::new("choice-index"))?)
                } else {
                    Fill::Const(value)
                };
                let k = hole_id(hole);
                let mut one = BTreeMap::new();
                one.insert(k, value.clone());
                let next = fill(&node, &one)?;
                let mut extended = assignment.clone();
                extended.insert(k, value);
                self.visit(j, next, extended, depth + 1, order, sketch_constants, state)?;
                if state.found.len() >= self.completion_limit() || state.budget {
                    break;
                }
            }
            return Ok(());
        }
        state.tried += 1;
        let mut done = node;
        if let Some(hole) = holes.first() {
            // Only the frozen expression pivot remains; synthesis is incomplete.
            state.exhaustive = false;
            let k = hole_id(hole);
            let path = path_to_hole(&done, k)?;
            if !crate::inverse::can_derive(&done, &path) {
                return Ok(());
            }
            let care = vec![true; self.subject.size];
            let root = Need::exact(self.subject.output_kind(j), &self.subject.targets[j], &care);
            let none = BTreeMap::new();
            let Some(need) =
                skip_overflow(derive_loose(&done, &path, &root, &self.full_env(j), &none))?
            else {
                return Ok(());
            };
            if first_unsatisfiable(&need).is_some() {
                return Ok(());
            }
            let Some(value) = self.meet(hole, &need, sketch_constants)? else {
                return Ok(());
            };
            let mut one = BTreeMap::new();
            one.insert(k, value.clone());
            done = fill(&done, &one)?;
            assignment.insert(k, value);
        }
        self.charge((size(&done) as u64).saturating_mul(self.subject.size as u64))?;
        if self.over_budget() {
            state.budget = true;
            return Ok(());
        }
        let right = skip_overflow(evaluate(&done, &self.full_env(j), &BTreeMap::new()))?
            .is_some_and(|values| values == self.subject.targets[j]);
        if right {
            state.found.push(Completion {
                assignment,
                nodes: size(&done),
                expression: done,
            });
        }
        Ok(())
    }

    /// Solves every constant or choice hole that sits alone on its route,
    /// exactly and by itself; returns the partly filled output, the values
    /// found, and why the branch is dead when a hole has no value.
    fn solve_alone(
        &mut self,
        j: usize,
        node: Expr,
        sketch_constants: &BTreeSet<i128>,
    ) -> Result<(Expr, BTreeMap<usize, Fill>, Option<Dead>)> {
        let mut node = node;
        let mut solved = BTreeMap::new();
        let care = vec![true; self.subject.size];
        let root = Need::exact(self.subject.output_kind(j), &self.subject.targets[j], &care);
        let none = BTreeMap::new();
        let ids: Vec<(usize, bool)> = holes_of(&node)
            .into_iter()
            .map(|h| (hole_id(h), matches!(h, Expr::Hole(_, Hole::Expr))))
            .collect();
        for (k, expression) in ids {
            if expression {
                continue;
            }
            let path = path_to_hole(&node, k)?;
            if !alone(&node, &path) {
                continue;
            }
            let cost = size(&node) as u64;
            self.charge(cost.saturating_mul(self.subject.size as u64))?;
            // An overflow beside the hole leaves it to the branching search.
            let Some(need) =
                skip_overflow(derive_loose(&node, &path, &root, &self.full_env(j), &none))?
            else {
                continue;
            };
            if let Some(ordinal) = first_unsatisfiable(&need) {
                return Ok((node, solved, Some(Dead::Witness(ordinal))));
            }
            let hole = node_at(&node, &path)?.clone();
            let Some(value) = self.meet(&hole, &need, sketch_constants)? else {
                return Ok((node, solved, Some(Dead::Unmet)));
            };
            let mut one = BTreeMap::new();
            one.insert(k, value.clone());
            node = fill(&node, &one)?;
            solved.insert(k, value);
        }
        Ok((node, solved, None))
    }

    /// One candidate per effect class of `hole` (values giving its parent
    /// expression the same value on every tuple make the same program), in
    /// the order the classes are first met, retaining the first value. The whole
    /// space when the parent holds another hole (its classes then depend on
    /// that hole). Values that overflow somewhere are dropped: they cannot
    /// be completions.
    fn classes(
        &mut self,
        j: usize,
        node: &Expr,
        hole: &Expr,
        space: &Searched,
    ) -> Result<Vec<i128>> {
        let k = hole_id(hole);
        let path = path_to_hole(node, k)?;
        let Some((_, parent_path)) = path.split_last() else {
            return Ok(space.values.clone());
        };
        let parent = node_at(node, parent_path)?;
        if holes_of(parent).len() > 1 {
            return Ok(space.values.clone());
        }
        let subject = self.subject;
        let env = subject.env(Some(j));
        let none = BTreeMap::new();
        let mut classes: Vec<(Vec<i128>, i128)> = Vec::new();
        let cost = size(parent) as u64;
        for value in &space.values {
            if self.over_budget() {
                return Err(Refusal::new("sketch-work-budget"));
            }
            let fill_value = if space.choice {
                Fill::Choice(usize::try_from(*value).unwrap_or(0))
            } else {
                Fill::Const(*value)
            };
            let mut one = BTreeMap::new();
            one.insert(k, fill_value);
            let filled = fill(parent, &one)?;
            self.charge(cost.saturating_mul(self.subject.size as u64))?;
            if self.over_budget() {
                return Err(Refusal::new("sketch-work-budget"));
            }
            let Some(vector) = skip_overflow(evaluate(&filled, &env, &none))? else {
                continue;
            };
            // Keep the first representative in the frozen joint order.
            if !classes.iter().any(|(v, _)| *v == vector) {
                classes.push((vector, *value));
            }
        }
        Ok(classes.into_iter().map(|(_, value)| value).collect())
    }
}

/// Completes a sketch (one expression per output, parameters bound) and
/// compares the completion with the original on every tuple.
#[must_use]
pub fn solve(subject: &Subject, outputs: &[Expr], settings: &Settings) -> SketchResult {
    let mut result = match solve_inner(subject, outputs, settings) {
        Ok(result) => result,
        Err(refusal) => SketchResult::stopped(Status::Refused, refusal.0),
    };
    if matches!(result.status, Status::NotFound | Status::Inconclusive) {
        result.reason = Some(format!(
            "{}; bounded-search:synth_depth={},width={},solve_limit={},max_assignments={},max_work={},synthesis_work={}; request a supported lowerable primitive or sketch for the same requirements",
            result
                .reason
                .as_deref()
                .unwrap_or("no-completion-within-declared-search"),
            settings.synth_depth,
            settings.synthesis.budget.width,
            settings.synthesis.budget.solve_limit,
            settings.max_assignments,
            settings.max_work,
            settings.synthesis.total_work,
        ));
    }
    result
}

fn solve_inner(subject: &Subject, outputs: &[Expr], settings: &Settings) -> Result<SketchResult> {
    if outputs.len() != subject.vocab.outputs().len() {
        return Err(Refusal::new("output-count"));
    }
    let mut resolved = Vec::with_capacity(outputs.len());
    for node in outputs {
        subject.vocab.check_names(node)?;
        if let Some(name) = params_of(node).first() {
            return Err(Refusal::new(format!("unbound-parameter:{name}")));
        }
        resolved.push(subject.vocab.resolve_variants(node)?);
    }
    let nodes = renumber(&resolved)?;
    let mut kinds = BTreeMap::new();
    for (j, node) in nodes.iter().enumerate() {
        hole_kinds(node, subject.output_kind(j), &subject.vocab, &mut kinds)?;
    }
    let mut sketch_constants = BTreeSet::new();
    for node in &nodes {
        sketch_constants.extend(constants(node));
    }
    let mut solver = Solver {
        subject: TargetView::from_subject(subject),
        settings: *settings,
        synthesizer: Some(Synthesizer::new(subject, settings.synthesis)),
        evaluated: 0,
        work: 0,
    };
    let mut chosen: Vec<Vec<Completion>> = Vec::new();
    let mut infos = Vec::new();
    for (j, node) in nodes.iter().enumerate() {
        let (found, info) = solver.solve_output(j, node, &kinds, &sketch_constants)?;
        let verdict = info.verdict.clone();
        infos.push(info);
        if found.is_empty() {
            return Ok(SketchResult {
                status: verdict.unwrap_or(Status::NotFound),
                program: None,
                expressions: Vec::new(),
                holes: BTreeMap::new(),
                sketch: nodes,
                outputs: infos,
                evaluated: solver.evaluated,
                reason: Some(format!("no completion of out{j}")),
            });
        }
        chosen.push(found);
    }
    // The cheapest lowered combination, among the first combinations.
    let mut best: Option<((usize, usize), Program, Vec<usize>)> = None;
    let mut index = vec![0usize; chosen.len()];
    for _ in 0..MAX_COMBINATIONS {
        let combo: Vec<Expr> = chosen
            .iter()
            .zip(&index)
            .map(|(c, i)| c[*i].expression.clone())
            .collect();
        if let Ok(program) = lower_outputs(&combo, &subject.vocab, &subject.original) {
            let key = cost(&program);
            if best.as_ref().is_none_or(|(b, _, _)| key < *b) {
                best = Some((key, program, index.clone()));
            }
        }
        let mut done = true;
        for k in (0..index.len()).rev() {
            index[k] += 1;
            if index[k] < chosen[k].len() {
                done = false;
                break;
            }
            index[k] = 0;
        }
        if done {
            break;
        }
    }
    let Some((_, program, picked)) = best else {
        return Ok(SketchResult {
            status: Status::NotFound,
            program: None,
            expressions: Vec::new(),
            holes: BTreeMap::new(),
            sketch: nodes,
            outputs: infos,
            evaluated: solver.evaluated,
            reason: Some(String::from("no completion lowers")),
        });
    };
    let mut holes = BTreeMap::new();
    let mut expressions = Vec::new();
    for (c, i) in chosen.iter().zip(&picked) {
        holes.extend(c[*i].assignment.clone());
        expressions.push(c[*i].expression.clone());
    }
    let agrees = first_difference(subject, &program).is_none();
    Ok(SketchResult {
        status: if agrees {
            Status::Completed
        } else {
            Status::NotFound
        },
        program: agrees.then_some(program),
        expressions,
        holes,
        sketch: nodes,
        outputs: infos,
        evaluated: solver.evaluated,
        reason: (!agrees).then(|| String::from("completion failed the comparison on every tuple")),
    })
}

/// For additive/Boolean sketches without multiplication or function calls,
/// a bound `B` such that, when the only remaining hole is one
/// constant, every value outside `[-B, B]` gives the same comparison results
/// as `B` or `-B` and an output different from every target on the domain.
/// It is the sum of the absolute literals and of the largest absolute input
/// value times the number of input reads, plus the largest absolute target,
/// plus 2. Used to state a checkable candidate space for a constant pivot.
#[must_use]
pub fn constant_bound(subject: &Subject, node: &Expr) -> i128 {
    let mut literals: i128 = 0;
    let mut reads: i128 = 0;
    fn visit(node: &Expr, literals: &mut i128, reads: &mut i128) {
        match node {
            Expr::Int(v) => *literals = literals.saturating_add(v.saturating_abs()),
            Expr::Var(_) => *reads = reads.saturating_add(1),
            Expr::Not(a) => visit(a, literals, reads),
            Expr::Bin(_, a, b) => {
                visit(a, literals, reads);
                visit(b, literals, reads);
            }
            Expr::If(c, a, b) => {
                visit(c, literals, reads);
                visit(a, literals, reads);
                visit(b, literals, reads);
            }
            Expr::Hole(_, Hole::Choice(alternatives)) => {
                for a in alternatives {
                    visit(a, literals, reads);
                }
            }
            _ => {}
        }
    }
    visit(node, &mut literals, &mut reads);
    let largest_input = subject
        .vocab
        .inputs()
        .iter()
        .map(|d| {
            let (lo, hi) = d.bounds();
            lo.saturating_abs().max(hi.saturating_abs())
        })
        .max()
        .unwrap_or(0);
    let largest_target = subject
        .targets
        .iter()
        .flat_map(|t| t.iter().map(|v| v.saturating_abs()))
        .max()
        .unwrap_or(0);
    literals
        .saturating_add(largest_input.saturating_mul(reads))
        .saturating_add(largest_target)
        .saturating_add(2)
        .min(BOUND_CAP)
}

/// Largest [`constant_bound`]: far inside the 128-bit range.
const BOUND_CAP: i128 = 1 << 62;
