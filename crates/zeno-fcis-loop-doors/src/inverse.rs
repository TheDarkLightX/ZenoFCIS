//! Inverse semantics: what must one subexpression compute so that its output
//! is right?
//!
//! Given an expression for one output, the output's required value on every
//! tuple and a *position* in the expression, [`derive()`] walks from the root
//! to the position and transforms the requirement through each operator:
//!
//! - through `c ? a : b` into a branch, the requirement is kept where the
//!   test selects that branch and dropped elsewhere (the other branch must
//!   already be right there); into the test it becomes "select whichever
//!   branch is right";
//! - through `+` and `-` it shifts by the sibling's value (bounds flip under
//!   `k - x`);
//! - through `&&`, `||`, `->` and `!` it is kept where the sibling does not
//!   already decide the result, and is unsatisfiable where the sibling
//!   decides it wrongly;
//! - through `==` and `!=` it becomes equality or inequality with the
//!   sibling, and through `<`, `<=`, `>`, `>=` a per-tuple bound.
//!
//! The result is a [`Need`]: per tuple an exact value, an interval and values
//! to avoid, or nothing (a don't-care), plus the tuples where no value at
//! that position can make the output right. If those exist the position
//! cannot repair the output. [`solve_constant`] is exact and complete over
//! signed 64-bit constants; [`solve_expression`] uses top-down synthesis and is
//! incomplete: `None` means *not found*, never *impossible*.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec;
use alloc::vec::Vec;
use zeno_fcis_program_search::target::{
    Plan, PlanNode, TargetBudget, TargetKind, synthesize_target_counted,
};

use crate::expr::{Env, Expr, HoleValue, Kind, Op, Vocabulary, compare_side, evaluate, has_holes};
use crate::subject::{Subject, domain_size};
use crate::{Refusal, Result};

/// The least value a position can hold: values are signed 64-bit integers.
pub const MIN: i128 = i64::MIN as i128;
/// The greatest value a position can hold.
pub const MAX: i128 = i64::MAX as i128;

/// The requirement on one position, per tuple.
///
/// Requirements are kept as exact wider integers: inverse arithmetic may
/// leave the 64-bit range even though no evaluated value can. Bounds are
/// clamped to the 64-bit range, so `[MIN, MAX]` means "no bound" and also
/// states that the position's own value must not overflow; a required value
/// outside that range is unsatisfiable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Need {
    /// The position's kind.
    pub kind: Kind,
    /// Required value where `has_eq`.
    pub eq: Vec<i128>,
    /// Where a value is required.
    pub has_eq: Vec<bool>,
    /// Inclusive lower bounds (`MIN`: none).
    pub lo: Vec<i128>,
    /// Inclusive upper bounds (`MAX`: none).
    pub hi: Vec<i128>,
    /// Values to avoid: `(value, mask)` means differ from `value` where
    /// `mask`.
    pub neq: Vec<(Vec<i128>, Vec<bool>)>,
    /// Tuples where no value here can make the output right.
    pub impossible: Vec<bool>,
}

fn wide(values: &[i64]) -> Vec<i128> {
    values.iter().map(|v| i128::from(*v)).collect()
}

fn lower_bound(value: i128) -> i128 {
    value.max(MIN)
}

fn upper_bound(value: i128) -> i128 {
    value.min(MAX)
}

/// `bound + by`, for a bound kept within the 64-bit range.
fn shift(bound: &[i128], by: &[i128], clamp: fn(i128) -> i128) -> Vec<i128> {
    bound
        .iter()
        .zip(by)
        .map(|(b, s)| clamp(b.saturating_add(*s)))
        .collect()
}

/// `s - bound`, for a bound kept within the 64-bit range.
fn flip(s: &[i128], bound: &[i128], clamp: fn(i128) -> i128) -> Vec<i128> {
    s.iter()
        .zip(bound)
        .map(|(s, b)| clamp(s.saturating_sub(*b)))
        .collect()
}

impl Need {
    /// The output must equal `target` where `care` holds.
    #[must_use]
    pub fn exact(kind: Kind, target: &[i64], care: &[bool]) -> Self {
        let n = target.len();
        Self {
            kind,
            eq: wide(target),
            has_eq: care.to_vec(),
            lo: vec![MIN; n],
            hi: vec![MAX; n],
            neq: Vec::new(),
            impossible: vec![false; n],
        }
    }

    /// Tuples considered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.eq.len()
    }

    /// Whether no tuple is considered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.eq.is_empty()
    }

    /// Per tuple: would `values` at the position meet the need?
    #[must_use]
    pub fn accepts(&self, values: &[i64]) -> Vec<bool> {
        (0..self.len())
            .map(|t| {
                let v = i128::from(values[t]);
                (!self.has_eq[t] || v == self.eq[t])
                    && v >= self.lo[t]
                    && v <= self.hi[t]
                    && self
                        .neq
                        .iter()
                        .all(|(value, mask)| !mask[t] || v != value[t])
            })
            .collect()
    }

    /// Whether `values` meet the need on every tuple.
    #[must_use]
    pub fn accepts_all(&self, values: &[i64]) -> bool {
        self.accepts(values).into_iter().all(|ok| ok)
    }

    /// Drops every requirement outside `keep` (those tuples become
    /// don't-cares, apart from holding a 64-bit value). `impossible` is kept.
    #[must_use]
    pub fn restrict(&self, keep: &[bool]) -> Self {
        let n = self.len();
        Self {
            kind: self.kind,
            eq: self.eq.clone(),
            has_eq: (0..n).map(|t| self.has_eq[t] && keep[t]).collect(),
            lo: (0..n)
                .map(|t| if keep[t] { self.lo[t] } else { MIN })
                .collect(),
            hi: (0..n)
                .map(|t| if keep[t] { self.hi[t] } else { MAX })
                .collect(),
            neq: self
                .neq
                .iter()
                .map(|(value, mask)| {
                    (
                        value.clone(),
                        mask.iter().zip(keep).map(|(m, k)| *m && *k).collect(),
                    )
                })
                .collect(),
            impossible: self.impossible.clone(),
        }
    }

    /// A Boolean need is an exact value or free: bounds and inequalities are
    /// folded into it.
    #[must_use]
    pub fn as_bool(&self) -> Self {
        let n = self.len();
        let mut eq = self.eq.clone();
        let mut has = self.has_eq.clone();
        let mut impossible = self.impossible.clone();
        let lo_one: Vec<bool> = self.lo.iter().map(|lo| *lo >= 1).collect();
        let hi_zero: Vec<bool> = self.hi.iter().map(|hi| *hi <= 0).collect();
        for (flag, value) in [(&lo_one, 1), (&hi_zero, 0)] {
            for t in 0..n {
                if flag[t] && has[t] && eq[t] != value {
                    impossible[t] = true;
                }
                if flag[t] && !has[t] {
                    eq[t] = value;
                }
                has[t] = has[t] || flag[t];
            }
        }
        for t in 0..n {
            if self.lo[t] > 1 || self.hi[t] < 0 || (lo_one[t] && hi_zero[t]) {
                impossible[t] = true;
            }
        }
        for (value, mask) in &self.neq {
            for t in 0..n {
                let other = 1 - value[t];
                if mask[t] && has[t] && eq[t] != other {
                    impossible[t] = true;
                }
                if mask[t] && !has[t] {
                    eq[t] = other;
                }
                has[t] = has[t] || mask[t];
            }
        }
        Self {
            kind: Kind::Bool,
            eq,
            has_eq: has,
            lo: vec![MIN; n],
            hi: vec![MAX; n],
            neq: Vec::new(),
            impossible,
        }
    }
}

// --- positions ---------------------------------------------------------------

/// A position: child indices from the root.
pub type Path = Vec<u8>;

/// Every `(path, subtree)` in pre-order.
#[must_use]
pub fn positions(node: &Expr) -> Vec<(Path, &Expr)> {
    fn visit<'a>(node: &'a Expr, path: &mut Path, out: &mut Vec<(Path, &'a Expr)>) {
        out.push((path.clone(), node));
        for (i, child) in crate::expr::children(node).into_iter().enumerate() {
            path.push(u8::try_from(i).unwrap_or(u8::MAX));
            visit(child, path, out);
            path.pop();
        }
    }
    let mut out = Vec::new();
    visit(node, &mut Vec::new(), &mut out);
    out
}

/// The subtree at `path`.
pub fn node_at<'a>(node: &'a Expr, path: &[u8]) -> Result<&'a Expr> {
    let mut node = node;
    for i in path {
        node = crate::expr::children(node)
            .get(usize::from(*i))
            .copied()
            .ok_or_else(|| Refusal::new("position-outside-tree"))?;
    }
    Ok(node)
}

/// The expression with the subtree at `path` replaced by `new`.
pub fn replace_at(node: &Expr, path: &[u8], new: &Expr) -> Result<Expr> {
    let Some((first, rest)) = path.split_first() else {
        return Ok(new.clone());
    };
    Ok(match (node, *first) {
        (Expr::Not(a), 0) => Expr::negation(replace_at(a, rest, new)?),
        (Expr::Bin(op, a, b), 0) => Expr::bin(*op, replace_at(a, rest, new)?, (**b).clone()),
        (Expr::Bin(op, a, b), 1) => Expr::bin(*op, (**a).clone(), replace_at(b, rest, new)?),
        (Expr::If(c, a, b), 0) => {
            Expr::select(replace_at(c, rest, new)?, (**a).clone(), (**b).clone())
        }
        (Expr::If(c, a, b), 1) => {
            Expr::select((**c).clone(), replace_at(a, rest, new)?, (**b).clone())
        }
        (Expr::If(c, a, b), 2) => {
            Expr::select((**c).clone(), (**a).clone(), replace_at(b, rest, new)?)
        }
        _ => return Err(Refusal::new("position-outside-tree")),
    })
}

// --- derivation --------------------------------------------------------------

/// The requirement on the subtree at `path` of `root`, given `need` on
/// `root`; siblings are evaluated in `env` with `holes`.
pub fn derive(
    root: &Expr,
    path: &[u8],
    need: &Need,
    env: &Env<'_>,
    holes: &BTreeMap<usize, HoleValue>,
) -> Result<Need> {
    derive_with(root, path, need, env, holes, false)
}

/// [`derive()`] for a position whose `c ? a : b` ancestors may hold unfilled
/// holes in their other branch: those tuples are left to those holes, so
/// they are neither required here nor declared impossible. The sketch solver
/// solves holes on disjoint routes one at a time this way.
pub fn derive_loose(
    root: &Expr,
    path: &[u8],
    need: &Need,
    env: &Env<'_>,
    holes: &BTreeMap<usize, HoleValue>,
) -> Result<Need> {
    derive_with(root, path, need, env, holes, true)
}

#[allow(clippy::too_many_lines)]
fn derive_with(
    root: &Expr,
    path: &[u8],
    need: &Need,
    env: &Env<'_>,
    holes: &BTreeMap<usize, HoleValue>,
    loose: bool,
) -> Result<Need> {
    let mut node = root;
    let mut need = need.clone();
    let mut want = need.kind;
    let n = need.len();
    let free = || (vec![MIN; n], vec![MAX; n]);
    for i in path {
        if want == Kind::Bool {
            need = need.as_bool();
        }
        let mut imp = need.impossible.clone();
        match node {
            Expr::Not(a) => {
                need = Need {
                    kind: Kind::Bool,
                    eq: need.eq.iter().map(|e| 1 - e).collect(),
                    has_eq: need.has_eq.clone(),
                    lo: need.lo.clone(),
                    hi: need.hi.clone(),
                    neq: Vec::new(),
                    impossible: imp,
                };
                node = a;
                want = Kind::Bool;
            }
            Expr::If(c, a, b) => {
                if *i == 0 {
                    let va = evaluate(a, env, holes)?;
                    let vb = evaluate(b, env, holes)?;
                    let ok_a = need.accepts(&va);
                    let ok_b = need.accepts(&vb);
                    for t in 0..n {
                        imp[t] = imp[t] || (!ok_a[t] && !ok_b[t]);
                    }
                    let (lo, hi) = free();
                    need = Need {
                        kind: Kind::Bool,
                        eq: ok_a.iter().map(|ok| i128::from(*ok)).collect(),
                        has_eq: ok_a.iter().zip(&ok_b).map(|(x, y)| x ^ y).collect(),
                        lo,
                        hi,
                        neq: Vec::new(),
                        impossible: imp,
                    };
                    node = c;
                    want = Kind::Bool;
                } else {
                    let vc = evaluate(c, env, holes)?;
                    let taken: Vec<bool> = vc
                        .iter()
                        .map(|v| if *i == 1 { *v == 1 } else { *v != 1 })
                        .collect();
                    let elsewhere = if *i == 1 { b } else { a };
                    if !(loose && has_holes(elsewhere)) {
                        let other = evaluate(elsewhere, env, holes)?;
                        let ok = need.accepts(&other);
                        for t in 0..n {
                            imp[t] = imp[t] || (!taken[t] && !ok[t]);
                        }
                    }
                    need = need.restrict(&taken);
                    need.impossible = imp;
                    node = if *i == 1 { a } else { b };
                }
            }
            Expr::Bin(op, a, b) => {
                let (mine, sibling) = if *i == 0 { (a, b) } else { (b, a) };
                let s64 = evaluate(sibling, env, holes)?;
                let s = wide(&s64);
                match op {
                    Op::Mul => return Err(Refusal::new("proposal-operator-unsupported:*")),
                    Op::Add | Op::Sub => {
                        if *op == Op::Add || *i == 0 {
                            // x + s = t: x = t - s;  x - s = t: x = t + s
                            let by: Vec<i128> = if *op == Op::Add {
                                s.iter().map(|v| -v).collect()
                            } else {
                                s.clone()
                            };
                            need = Need {
                                kind: Kind::Int,
                                eq: need
                                    .eq
                                    .iter()
                                    .zip(&by)
                                    .map(|(e, b)| e.saturating_add(*b))
                                    .collect(),
                                has_eq: need.has_eq.clone(),
                                lo: shift(&need.lo, &by, lower_bound),
                                hi: shift(&need.hi, &by, upper_bound),
                                neq: need
                                    .neq
                                    .iter()
                                    .map(|(v, m)| {
                                        (
                                            v.iter()
                                                .zip(&by)
                                                .map(|(v, b)| v.saturating_add(*b))
                                                .collect(),
                                            m.clone(),
                                        )
                                    })
                                    .collect(),
                                impossible: imp,
                            };
                        } else {
                            // s - x = t: x = s - t, bounds flip
                            need = Need {
                                kind: Kind::Int,
                                eq: s
                                    .iter()
                                    .zip(&need.eq)
                                    .map(|(s, e)| s.saturating_sub(*e))
                                    .collect(),
                                has_eq: need.has_eq.clone(),
                                lo: flip(&s, &need.hi, lower_bound),
                                hi: flip(&s, &need.lo, upper_bound),
                                neq: need
                                    .neq
                                    .iter()
                                    .map(|(v, m)| {
                                        (
                                            s.iter()
                                                .zip(v)
                                                .map(|(s, v)| s.saturating_sub(*v))
                                                .collect(),
                                            m.clone(),
                                        )
                                    })
                                    .collect(),
                                impossible: imp,
                            };
                        }
                        node = mine;
                        want = Kind::Int;
                    }
                    Op::And | Op::Or | Op::Implies => {
                        // where the sibling alone decides the result, this
                        // position is free; it is impossible where that
                        // decision is wrong
                        let (free_at, forced, flip_mine): (Vec<bool>, i128, bool) = match (op, *i) {
                            (Op::And, _) => (s64.iter().map(|v| *v != 1).collect(), 0, false),
                            (Op::Or, _) => (s64.iter().map(|v| *v == 1).collect(), 1, false),
                            (_, 0) => (s64.iter().map(|v| *v == 1).collect(), 1, true),
                            _ => (s64.iter().map(|v| *v != 1).collect(), 1, false),
                        };
                        for t in 0..n {
                            if free_at[t] && need.has_eq[t] && need.eq[t] != forced {
                                imp[t] = true;
                            }
                        }
                        let (lo, hi) = free();
                        need = Need {
                            kind: Kind::Bool,
                            eq: need
                                .eq
                                .iter()
                                .map(|e| if flip_mine { 1 - e } else { *e })
                                .collect(),
                            has_eq: need
                                .has_eq
                                .iter()
                                .zip(&free_at)
                                .map(|(h, f)| *h && !*f)
                                .collect(),
                            lo,
                            hi,
                            neq: Vec::new(),
                            impossible: imp,
                        };
                        node = mine;
                        want = Kind::Bool;
                    }
                    Op::Eq | Op::Ne => {
                        let side = compare_side(mine, sibling, env.vocab)?;
                        let truth: Vec<i128> = need
                            .eq
                            .iter()
                            .map(|e| if *op == Op::Eq { *e } else { 1 - e })
                            .collect();
                        let (lo, hi) = free();
                        need = if side == Kind::Bool {
                            Need {
                                kind: Kind::Bool,
                                eq: truth
                                    .iter()
                                    .zip(&s)
                                    .map(|(t, s)| if *t == 1 { *s } else { 1 - s })
                                    .collect(),
                                has_eq: need.has_eq.clone(),
                                lo,
                                hi,
                                neq: Vec::new(),
                                impossible: imp,
                            }
                        } else {
                            let same = (0..n).map(|t| need.has_eq[t] && truth[t] == 1).collect();
                            let differ = (0..n).map(|t| need.has_eq[t] && truth[t] == 0).collect();
                            Need {
                                kind: Kind::Int,
                                eq: s.clone(),
                                has_eq: same,
                                lo,
                                hi,
                                neq: vec![(s, differ)],
                                impossible: imp,
                            }
                        };
                        node = mine;
                        want = side;
                    }
                    Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                        // Exact at the ends of the range: `x < MIN` and
                        // `x > MAX` leave no value, `x <= MAX` and `x >= MIN`
                        // leave every value.
                        let (mut lo, mut hi) = free();
                        let rel = if *i == 0 {
                            *op
                        } else {
                            match op {
                                Op::Lt => Op::Gt,
                                Op::Le => Op::Ge,
                                Op::Gt => Op::Lt,
                                _ => Op::Le,
                            }
                        };
                        for t in 0..n {
                            let yes = need.has_eq[t] && need.eq[t] == 1;
                            let no = need.has_eq[t] && need.eq[t] == 0;
                            let v = s[t];
                            match rel {
                                Op::Lt => {
                                    if yes {
                                        hi[t] = v - 1;
                                    }
                                    if no {
                                        lo[t] = v;
                                    }
                                }
                                Op::Le => {
                                    if yes {
                                        hi[t] = v;
                                    }
                                    if no {
                                        lo[t] = v + 1;
                                    }
                                }
                                Op::Gt => {
                                    if yes {
                                        lo[t] = v + 1;
                                    }
                                    if no {
                                        hi[t] = v;
                                    }
                                }
                                _ => {
                                    if yes {
                                        lo[t] = v;
                                    }
                                    if no {
                                        hi[t] = v - 1;
                                    }
                                }
                            }
                        }
                        need = Need {
                            kind: Kind::Int,
                            eq: vec![0; n],
                            has_eq: vec![false; n],
                            lo,
                            hi,
                            neq: Vec::new(),
                            impossible: imp,
                        };
                        node = mine;
                        want = Kind::Int;
                    }
                }
            }
            _ => return Err(Refusal::new("position-outside-tree")),
        }
    }
    if want == Kind::Bool {
        need = need.as_bool();
    }
    need.kind = want;
    Ok(need)
}

// --- meeting a need ----------------------------------------------------------

/// Tuples where no value can meet the need at all.
#[must_use]
pub fn unsatisfiable(need: &Need) -> Vec<bool> {
    (0..need.len())
        .map(|t| {
            need.impossible[t]
                || need.lo[t] > need.hi[t]
                || (need.has_eq[t] && (need.eq[t] < need.lo[t] || need.eq[t] > need.hi[t]))
                || (need.kind == Kind::Bool && need.has_eq[t] && (need.eq[t] < 0 || need.eq[t] > 1))
        })
        .collect()
}

/// The first tuple where the need cannot be met, if any.
#[must_use]
pub fn first_unsatisfiable(need: &Need) -> Option<usize> {
    unsatisfiable(need).iter().position(|bad| *bad)
}

/// The 64-bit constant meeting the need on every tuple, or `None`. Exact and
/// complete over signed 64-bit constants: the feasible set is one value (if
/// any tuple fixes it) or an interval minus finitely many values. Among
/// feasible constants, `prefer` comes first (constants already in the
/// program cost nothing extra), then the one nearest zero.
#[must_use]
pub fn solve_constant(need: &Need, prefer: &[i64]) -> Option<i64> {
    if first_unsatisfiable(need).is_some() {
        return None;
    }
    let mut lo = need.lo.iter().copied().max().unwrap_or(MIN).max(MIN);
    let mut hi = need.hi.iter().copied().min().unwrap_or(MAX).min(MAX);
    if need.kind == Kind::Bool {
        lo = lo.max(0);
        hi = hi.min(1);
    }
    if lo > hi {
        return None;
    }
    let fixed: BTreeSet<i128> = (0..need.len())
        .filter(|t| need.has_eq[*t])
        .map(|t| need.eq[t])
        .collect();
    if fixed.len() > 1 {
        return None;
    }
    let mut forbidden = BTreeSet::new();
    for (value, mask) in &need.neq {
        for t in 0..need.len() {
            if mask[t] {
                forbidden.insert(value[t]);
            }
        }
    }
    let feasible = |c: i128| lo <= c && c <= hi && !forbidden.contains(&c);
    if let Some(c) = fixed.first() {
        return feasible(*c).then(|| i64::try_from(*c).ok()).flatten();
    }
    if let Some(c) = prefer.iter().find(|c| feasible(i128::from(**c))) {
        return Some(*c);
    }
    let start = lo.max(0).min(hi);
    for step in 0..=i128::try_from(forbidden.len()).unwrap_or(MAX) {
        for c in [start.saturating_add(step), start.saturating_sub(step)] {
            if feasible(c) {
                return i64::try_from(c).ok();
            }
        }
    }
    None
}

/// Synthesis limits for subexpressions and region formulas.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Synthesis {
    /// Top-down settings and the work one solve may use.
    pub budget: TargetBudget,
    /// Work all the solves of one door run may use together. A solve gets
    /// at most what is left; with nothing left it finds nothing.
    pub total_work: u64,
}

impl Default for Synthesis {
    fn default() -> Self {
        Self {
            budget: TargetBudget {
                depth: 4,
                width: 6,
                solve_limit: 20_000,
                max_cells: 2_000_000,
                work_units: 1 << 21,
            },
            total_work: 1 << 22,
        }
    }
}

/// Least work worth starting a solve with.
const MIN_SOLVE_WORK: u64 = 1 << 12;

/// A synthesized plan as an expression over the input names. `readable`
/// writes a Boolean select with a constant Boolean branch as a connective
/// (`c ? false : x` as `!c && x`); used for region formulas shown to people.
pub fn from_plan(plan: &Plan, vocab: &Vocabulary, readable: bool) -> Result<Expr> {
    let mut built: Vec<Expr> = Vec::with_capacity(plan.nodes.len());
    let mut total = 0usize;
    for node in &plan.nodes {
        let at = |k: usize| {
            built
                .get(k)
                .cloned()
                .ok_or_else(|| Refusal::new("plan-reference"))
        };
        let boolean = |k: usize| matches!(plan.nodes.get(k), Some(PlanNode::Bool(_)));
        let expr = match *node {
            PlanNode::Input(k) => Expr::Var(
                vocab
                    .names()
                    .get(usize::from(k))
                    .cloned()
                    .ok_or_else(|| Refusal::new("plan-reference"))?,
            ),
            PlanNode::Int(value) => Expr::Int(value),
            PlanNode::Bool(value) => Expr::Int(i64::from(value)),
            PlanNode::Not(a) => match at(a)? {
                Expr::Not(inner) if readable => *inner,
                other => Expr::negation(other),
            },
            PlanNode::Select(c, a, b) => {
                let (cond, yes, no) = (at(c)?, at(a)?, at(b)?);
                if readable && (boolean(a) || boolean(b)) {
                    let negated = match &cond {
                        Expr::Not(inner) => (**inner).clone(),
                        other => Expr::negation(other.clone()),
                    };
                    if boolean(a) && boolean(b) {
                        if yes == Expr::Int(1) { cond } else { negated }
                    } else if boolean(a) {
                        if yes == Expr::Int(1) {
                            Expr::bin(Op::Or, cond, no)
                        } else {
                            Expr::bin(Op::And, negated, no)
                        }
                    } else if no == Expr::Int(1) {
                        Expr::bin(Op::Or, negated, yes)
                    } else {
                        Expr::bin(Op::And, cond, yes)
                    }
                } else {
                    Expr::select(cond, yes, no)
                }
            }
            PlanNode::Add(a, b) => Expr::bin(Op::Add, at(a)?, at(b)?),
            PlanNode::Sub(a, b) => Expr::bin(Op::Sub, at(a)?, at(b)?),
            PlanNode::Eq(a, b) => Expr::bin(Op::Eq, at(a)?, at(b)?),
            PlanNode::Lt(a, b) => Expr::bin(Op::Lt, at(a)?, at(b)?),
            PlanNode::And(a, b) => Expr::bin(Op::And, at(a)?, at(b)?),
        };
        total = total.saturating_add(crate::expr::size(&expr));
        if total > 1 << 20 {
            return Err(Refusal::new("plan-too-shared-to-inline"));
        }
        built.push(expr);
    }
    built
        .get(plan.root)
        .cloned()
        .ok_or_else(|| Refusal::new("plan-reference"))
}

fn target_kind(kind: Kind) -> TargetKind {
    match kind {
        Kind::Int => TargetKind::Int,
        Kind::Bool => TargetKind::Bool,
    }
}

/// Stateful preparation for plans over one subject's complete table.
/// A caller owns the handle for one invocation. Its internal cells account
/// advisory search work; neither this handle nor its plans decide acceptance.
/// Subject inputs stay borrowed read-only and returned expressions own data.
pub struct Synthesizer<'a> {
    subject: &'a Subject,
    settings: Synthesis,
    remaining: core::cell::Cell<u64>,
    // Composable loops: invocation-local advisory stop accounting.
    stopped: core::cell::Cell<bool>,
}

impl<'a> Synthesizer<'a> {
    /// A synthesizer over every tuple of `subject`.
    #[must_use]
    pub fn new(subject: &'a Subject, settings: Synthesis) -> Self {
        Self {
            subject,
            settings,
            remaining: core::cell::Cell::new(settings.total_work),
            stopped: core::cell::Cell::new(false),
        }
    }

    /// Work left for further solves.
    #[must_use]
    pub fn remaining(&self) -> u64 {
        self.remaining.get()
    }

    /// Whether a solve stopped or returned no plan under bounded search.
    /// This advisory flag cannot establish that no solution exists.
    #[must_use]
    pub fn stopped(&self) -> bool {
        self.stopped.get()
    }

    /// The plan meeting `target` wherever `care` holds, at `depth`, as an
    /// expression; `None` when not found (or a budget stopped the search).
    pub fn solve(
        &self,
        target: &[i64],
        care: &[bool],
        kind: Kind,
        depth: u32,
        readable: bool,
    ) -> Result<Option<Expr>> {
        let allotted = self.settings.budget.work_units.min(self.remaining.get());
        if allotted < MIN_SOLVE_WORK {
            self.stopped.set(true);
            return Ok(None);
        }
        let budget = TargetBudget {
            depth,
            work_units: allotted,
            ..self.settings.budget
        };
        let (plan, used) = synthesize_target_counted(
            self.subject.vocab.inputs(),
            &self.subject.columns,
            target,
            care,
            target_kind(kind),
            &budget,
        );
        self.remaining
            .set(self.remaining.get().saturating_sub(used.max(1)));
        match plan {
            Ok(Some(plan)) => Ok(Some(from_plan(&plan, &self.subject.vocab, readable)?)),
            Ok(None) | Err(_) => {
                self.stopped.set(true);
                Ok(None)
            }
        }
    }

    /// The prototype's atoms of a kind: the inputs of that kind, then (for
    /// `Bool`) point and threshold predicates on integer inputs with at most
    /// 64 values and comparisons between integer inputs.
    #[must_use]
    pub fn atoms(&self, kind: Kind) -> Vec<Expr> {
        let vocab = &self.subject.vocab;
        let names = vocab.names();
        let kinds = vocab.input_kinds();
        let mut out: Vec<Expr> = names
            .iter()
            .zip(kinds)
            .filter(|(_, k)| **k == kind)
            .map(|(name, _)| Expr::Var(name.clone()))
            .collect();
        if kind == Kind::Bool {
            let ints: Vec<usize> = (0..names.len())
                .filter(|k| kinds[*k] == Kind::Int)
                .collect();
            for k in &ints {
                let domain = vocab.inputs()[*k];
                if domain_size(domain) > 64 {
                    continue;
                }
                let (lo, hi) = domain.bounds();
                for v in lo..=hi {
                    out.push(Expr::bin(
                        Op::Eq,
                        Expr::Var(names[*k].clone()),
                        Expr::Int(v),
                    ));
                }
                for v in lo.saturating_add(1)..=hi {
                    out.push(Expr::bin(
                        Op::Lt,
                        Expr::Var(names[*k].clone()),
                        Expr::Int(v),
                    ));
                }
            }
            for (i, a) in ints.iter().enumerate() {
                for b in &ints[i + 1..] {
                    let (x, y) = (Expr::Var(names[*a].clone()), Expr::Var(names[*b].clone()));
                    out.push(Expr::bin(Op::Eq, x.clone(), y.clone()));
                    out.push(Expr::bin(Op::Lt, x.clone(), y.clone()));
                    out.push(Expr::bin(Op::Lt, y, x));
                }
            }
        }
        out
    }
}

/// An expression meeting the need: a constant, a synthesized plan, an atom
/// or an atom shifted by at most 2. Incomplete beyond constants: `None`
/// means not found, never impossible.
pub fn solve_expression(
    need: &Need,
    synthesizer: &Synthesizer<'_>,
    depth: u32,
) -> Result<Option<Expr>> {
    if first_unsatisfiable(need).is_some() {
        return Ok(None);
    }
    if let Some(c) = solve_constant(need, &[]) {
        return Ok(Some(Expr::Int(c)));
    }
    let env = synthesizer.subject.env(None);
    let none = BTreeMap::new();
    if need.has_eq.iter().any(|h| *h) {
        // Every required value is within the 64-bit range: the need is
        // satisfiable.
        let target: Vec<i64> = (0..need.len())
            .map(|t| {
                if need.has_eq[t] {
                    i64::try_from(need.eq[t]).unwrap_or(0)
                } else {
                    0
                }
            })
            .collect();
        if let Some(expr) = synthesizer.solve(&target, &need.has_eq, need.kind, depth, false)? {
            let values = evaluate(&expr, &env, &none)?;
            if need.accepts_all(&values) {
                return Ok(Some(expr));
            }
        }
    }
    for atom in synthesizer.atoms(need.kind) {
        let values = evaluate(&atom, &env, &none)?;
        if need.accepts_all(&values) {
            return Ok(Some(atom));
        }
        if need.kind == Kind::Int {
            for k in [1i64, -1, 2, -2] {
                // A shift that overflows on some tuple would trap there.
                let Some(shifted) = values
                    .iter()
                    .map(|v| v.checked_add(k))
                    .collect::<Option<Vec<i64>>>()
                else {
                    continue;
                };
                if need.accepts_all(&shifted) {
                    let op = if k > 0 { Op::Add } else { Op::Sub };
                    return Ok(Some(Expr::bin(op, atom, Expr::Int(k.abs()))));
                }
            }
        }
    }
    Ok(None)
}

/// An exact Boolean formula over the inputs that holds exactly on `mask`, or
/// `None` when synthesis finds none at `depth`.
pub fn describe_region(
    mask: &[bool],
    synthesizer: &Synthesizer<'_>,
    depth: u32,
) -> Result<Option<Expr>> {
    if !mask.iter().any(|m| *m) {
        return Ok(Some(Expr::Int(0)));
    }
    if mask.iter().all(|m| *m) {
        return Ok(Some(Expr::Int(1)));
    }
    let target: Vec<i64> = mask.iter().map(|m| i64::from(*m)).collect();
    let all = vec![true; mask.len()];
    synthesizer.solve(&target, &all, Kind::Bool, depth, true)
}

/// Input fields that take one value on every tuple of `mask` (a coarse
/// description when no formula is found).
#[must_use]
pub fn common_cube(mask: &[bool], subject: &Subject) -> Vec<(usize, i64)> {
    let mut out = Vec::new();
    for (k, column) in subject.columns.iter().enumerate() {
        let values: BTreeSet<i64> = column
            .iter()
            .zip(mask)
            .filter(|(_, m)| **m)
            .map(|(v, _)| *v)
            .collect();
        if values.len() == 1
            && let Some(value) = values.first()
        {
            out.push((k, *value));
        }
    }
    out
}
