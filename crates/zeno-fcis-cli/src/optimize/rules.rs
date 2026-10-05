//! Rewrite phases: pattern rules for the Boolean and Select phases, constant
//! folding, commutative sharing and the signature-directed semantic phase.
//!
//! A rule proposes an equality between an existing class and a right-hand
//! side built from the class's operands and constants; the e-graph's merge
//! guard then accepts or refuses it. Rules never build `Add`, `Sub`, `Lt` or
//! `Input` nodes: arithmetic is touched only by constant folding, and folding
//! produces a constant only for a class that is never poisoned, so a possible
//! trap is never folded away.

use std::collections::BTreeMap;

use super::egraph::{
    AddError, Caps, ClassId, EGraph, ENode, Inconsistency, Limit, Merge, MergeReason,
};
use super::extract::tree_costs;
use super::semantics::{Kind, Signature};
use super::strategy::{Limits, Phase};

/// A right-hand side over existing classes and new constants.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Term {
    Class(ClassId),
    Bool(bool),
    Int(i64),
    Not(Box<Term>),
    And(Box<Term>, Box<Term>),
    Eq(Box<Term>, Box<Term>),
    Select(Box<Term>, Box<Term>, Box<Term>),
}

impl Term {
    fn class(class: ClassId) -> Box<Self> {
        Box::new(Self::Class(class))
    }

    /// The Or form `Select(a, a, b)`.
    fn or(a: ClassId, b: ClassId) -> Self {
        Self::Select(Self::class(a), Self::class(a), Self::class(b))
    }
}

/// One proposed equality.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Rewrite {
    pub(crate) lhs: ClassId,
    pub(crate) rhs: Term,
    pub(crate) rule: &'static str,
    pub(crate) reason: MergeReason,
}

/// A pattern rule: proposes rewrites for one e-node of one class.
pub(crate) type Rule = fn(&EGraph, ClassId, ENode, &mut Vec<Rewrite>);

/// What one phase did.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PhaseReport {
    pub(crate) phase: Phase,
    pub(crate) rounds_requested: u32,
    pub(crate) rounds_run: u32,
    /// The phase stopped early because a round changed nothing.
    pub(crate) saturated: bool,
    pub(crate) limit_hit: Option<Limit>,
    pub(crate) rewrites: u64,
    pub(crate) nodes_added: u64,
    pub(crate) merges: u64,
    pub(crate) refused: u64,
    /// Why the phase did nothing at all, when so.
    pub(crate) skipped: Option<&'static str>,
    /// E-nodes ever created and live classes after the phase.
    pub(crate) enodes: u32,
    pub(crate) classes: u32,
}

/// Counters for one round.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct RoundTally {
    rewrites: u64,
    nodes_added: u64,
    merges: u64,
    refused: u64,
    limit_hit: Option<Limit>,
}

impl RoundTally {
    fn changed(&self) -> bool {
        self.nodes_added > 0 || self.merges > 0
    }
}

/// Runs one phase for up to `rounds` rounds, rebuilding after each.
pub(crate) fn run_phase(
    egraph: &mut EGraph,
    phase: Phase,
    rounds: u32,
    limits: &Limits,
) -> Result<PhaseReport, Inconsistency> {
    let mut report = PhaseReport {
        phase,
        rounds_requested: rounds,
        rounds_run: 0,
        saturated: false,
        limit_hit: None,
        rewrites: 0,
        nodes_added: 0,
        merges: 0,
        refused: 0,
        skipped: None,
        enodes: egraph.enode_count(),
        classes: egraph.class_count(),
    };
    if phase == Phase::SemanticMerge && !egraph.domain().exact() {
        report.skipped = Some("domain has more than 64 tuples; no exact signatures");
        return Ok(report);
    }
    for _ in 0..rounds {
        let tally = match phase {
            Phase::Boolean => round(egraph, &boolean_rules(), limits)?,
            Phase::Select => round(egraph, &select_rules(), limits)?,
            Phase::Fold => round(egraph, &[fold_rule], limits)?,
            Phase::Share => round(egraph, &[commute_rule], limits)?,
            Phase::SemanticMerge => semantic_round(egraph, limits)?,
        };
        report.rounds_run += 1;
        report.rewrites += tally.rewrites;
        report.nodes_added += tally.nodes_added;
        report.merges += tally.merges;
        report.refused += tally.refused;
        if let Some(limit) = tally.limit_hit {
            report.limit_hit = Some(limit);
            break;
        }
        if !tally.changed() {
            report.saturated = true;
            break;
        }
    }
    report.enodes = egraph.enode_count();
    report.classes = egraph.class_count();
    Ok(report)
}

/// Runs the given pattern rules for `rounds` rounds. Tests use this entry to
/// plant an unsound rule and watch the judge refuse its candidates.
#[cfg(test)]
pub(crate) fn run_rules(
    egraph: &mut EGraph,
    rules: &[Rule],
    rounds: u32,
    limits: &Limits,
) -> Result<PhaseReport, Inconsistency> {
    let mut report = PhaseReport {
        phase: Phase::Boolean,
        rounds_requested: rounds,
        rounds_run: 0,
        saturated: false,
        limit_hit: None,
        rewrites: 0,
        nodes_added: 0,
        merges: 0,
        refused: 0,
        skipped: None,
        enodes: egraph.enode_count(),
        classes: egraph.class_count(),
    };
    for _ in 0..rounds {
        let tally = round(egraph, rules, limits)?;
        report.rounds_run += 1;
        report.rewrites += tally.rewrites;
        report.nodes_added += tally.nodes_added;
        report.merges += tally.merges;
        report.refused += tally.refused;
        if let Some(limit) = tally.limit_hit {
            report.limit_hit = Some(limit);
            break;
        }
        if !tally.changed() {
            report.saturated = true;
            break;
        }
    }
    report.enodes = egraph.enode_count();
    report.classes = egraph.class_count();
    Ok(report)
}

/// One round: collect every match against the current graph, then apply the
/// rewrites in order and rebuild.
fn round(
    egraph: &mut EGraph,
    rules: &[Rule],
    limits: &Limits,
) -> Result<RoundTally, Inconsistency> {
    let mut rewrites = Vec::new();
    let mut tally = RoundTally::default();
    'collect: for class in egraph.classes() {
        for id in egraph.class_nodes(class) {
            let node = egraph.node(id);
            for rule in rules {
                rule(egraph, class, node, &mut rewrites);
                if rewrites.len() > limits.max_rewrites_per_round as usize {
                    rewrites.truncate(limits.max_rewrites_per_round as usize);
                    tally.limit_hit = Some(Limit::Rewrites);
                    break 'collect;
                }
            }
        }
    }
    tally.rewrites = rewrites.len() as u64;
    apply(egraph, &rewrites, limits.caps(), &mut tally)?;
    egraph.rebuild()?;
    Ok(tally)
}

fn apply(
    egraph: &mut EGraph,
    rewrites: &[Rewrite],
    caps: Caps,
    tally: &mut RoundTally,
) -> Result<(), Inconsistency> {
    for rewrite in rewrites {
        let before = egraph.enode_count();
        let rhs = match instantiate(egraph, &rewrite.rhs, caps) {
            Ok(class) => class,
            Err(AddError::Limit(limit)) => {
                tally.limit_hit = Some(limit);
                return Ok(());
            }
            Err(AddError::Type(_)) => {
                // An ill-typed right-hand side is a rule defect; it is
                // refused like a guard failure rather than applied.
                tally.refused += 1;
                continue;
            }
        };
        tally.nodes_added += u64::from(egraph.enode_count() - before);
        match egraph.union(rewrite.lhs, rhs, rewrite.reason)? {
            Merge::Merged => tally.merges += 1,
            Merge::Refused => tally.refused += 1,
            Merge::AlreadyEqual => {}
        }
    }
    Ok(())
}

fn instantiate(egraph: &mut EGraph, term: &Term, caps: Caps) -> Result<ClassId, AddError> {
    Ok(match term {
        Term::Class(class) => egraph.find(*class),
        Term::Bool(value) => egraph.add(ENode::Bool(*value), caps)?,
        Term::Int(value) => egraph.add(ENode::Int(*value), caps)?,
        Term::Not(a) => {
            let a = instantiate(egraph, a, caps)?;
            egraph.add(ENode::Not(a), caps)?
        }
        Term::And(a, b) => {
            let a = instantiate(egraph, a, caps)?;
            let b = instantiate(egraph, b, caps)?;
            egraph.add(ENode::And(a, b), caps)?
        }
        Term::Eq(a, b) => {
            let a = instantiate(egraph, a, caps)?;
            let b = instantiate(egraph, b, caps)?;
            egraph.add(ENode::Eq(a, b), caps)?
        }
        Term::Select(c, a, b) => {
            let c = instantiate(egraph, c, caps)?;
            let a = instantiate(egraph, a, caps)?;
            let b = instantiate(egraph, b, caps)?;
            egraph.add(ENode::Select(c, a, b), caps)?
        }
    })
}

// ---- matching helpers -------------------------------------------------------

/// Operands of the `Not` e-nodes in a class.
fn nots(egraph: &EGraph, class: ClassId) -> Vec<ClassId> {
    egraph
        .class_nodes(class)
        .into_iter()
        .filter_map(|id| match egraph.node(id) {
            ENode::Not(a) => Some(a),
            _ => None,
        })
        .collect()
}

/// Operand pairs of the `And` e-nodes in a class.
fn ands(egraph: &EGraph, class: ClassId) -> Vec<(ClassId, ClassId)> {
    egraph
        .class_nodes(class)
        .into_iter()
        .filter_map(|id| match egraph.node(id) {
            ENode::And(a, b) => Some((a, b)),
            _ => None,
        })
        .collect()
}

/// Operand triples of the `Select` e-nodes in a class.
fn selects(egraph: &EGraph, class: ClassId) -> Vec<(ClassId, ClassId, ClassId)> {
    egraph
        .class_nodes(class)
        .into_iter()
        .filter_map(|id| match egraph.node(id) {
            ENode::Select(c, a, b) => Some((c, a, b)),
            _ => None,
        })
        .collect()
}

/// Operand pairs `(a, b)` of the Or forms `Select(a, a, b)` in a class.
fn ors(egraph: &EGraph, class: ClassId) -> Vec<(ClassId, ClassId)> {
    selects(egraph, class)
        .into_iter()
        .filter_map(|(c, a, b)| (c == a).then_some((a, b)))
        .collect()
}

fn is_true(egraph: &EGraph, class: ClassId) -> bool {
    egraph.data(class).kind == Kind::Bool && egraph.data(class).constant() == Some(1)
}

fn is_false(egraph: &EGraph, class: ClassId) -> bool {
    egraph.data(class).kind == Kind::Bool && egraph.data(class).constant() == Some(0)
}

fn is_bool(egraph: &EGraph, class: ClassId) -> bool {
    egraph.data(class).kind == Kind::Bool
}

fn propose(out: &mut Vec<Rewrite>, lhs: ClassId, rhs: Term, rule: &'static str) {
    out.push(Rewrite {
        lhs,
        rhs,
        rule,
        reason: MergeReason::Rule,
    });
}

/// The operand the two pairs share, with the two others, in every position.
fn shared_operand(
    (p, q): (ClassId, ClassId),
    (r, s): (ClassId, ClassId),
) -> Vec<(ClassId, ClassId, ClassId)> {
    let mut found = Vec::new();
    if p == r {
        found.push((p, q, s));
    }
    if p == s {
        found.push((p, q, r));
    }
    if q == r {
        found.push((q, p, s));
    }
    if q == s {
        found.push((q, p, r));
    }
    found
}

// ---- Boolean phase -----------------------------------------------------------

pub(crate) fn boolean_rules() -> Vec<Rule> {
    vec![not_rules, and_rules, or_rules, eq_rules, lt_rules]
}

/// `Not(Not(a)) = a`; `Not(And(Not(a), Not(b))) = Or(a, b)`.
fn not_rules(egraph: &EGraph, class: ClassId, node: ENode, out: &mut Vec<Rewrite>) {
    let ENode::Not(x) = node else {
        return;
    };
    for y in nots(egraph, x) {
        propose(out, class, Term::Class(y), "not-not");
    }
    for (a, b) in ands(egraph, x) {
        for na in nots(egraph, a) {
            for nb in nots(egraph, b) {
                propose(out, class, Term::or(na, nb), "or-select");
            }
        }
    }
}

/// Idempotence, identity, annihilation, complement, absorption,
/// sharing-directed associativity and factoring of `And` over the Or form.
fn and_rules(egraph: &EGraph, class: ClassId, node: ENode, out: &mut Vec<Rewrite>) {
    let ENode::And(a, b) = node else {
        return;
    };
    if a == b {
        propose(out, class, Term::Class(a), "and-idem");
    }
    for (x, y) in [(a, b), (b, a)] {
        if is_true(egraph, y) {
            propose(out, class, Term::Class(x), "and-true");
        }
        if is_false(egraph, y) {
            propose(out, class, Term::Bool(false), "and-false");
        }
        if nots(egraph, y).contains(&x) {
            propose(out, class, Term::Bool(false), "and-complement");
        }
        for (p, q) in ors(egraph, y) {
            if p == x || q == x {
                propose(out, class, Term::Class(x), "and-absorb");
            }
        }
        // Reassociate only towards an existing conjunction, so sharing is
        // gained without enumerating every association of a long chain.
        for (p, q) in ands(egraph, x) {
            for (r, s) in [(q, y), (y, q), (p, y), (y, p)] {
                let Some(inner) = egraph.lookup(ENode::And(r, s)) else {
                    continue;
                };
                let outer = if r == q || s == q { p } else { q };
                propose(
                    out,
                    class,
                    Term::And(Term::class(outer), Term::class(inner)),
                    "and-assoc",
                );
            }
        }
    }
    for left in ors(egraph, a) {
        for right in ors(egraph, b) {
            for (shared, other, another) in shared_operand(left, right) {
                propose(
                    out,
                    class,
                    Term::Select(
                        Term::class(shared),
                        Term::class(shared),
                        Box::new(Term::And(Term::class(other), Term::class(another))),
                    ),
                    "and-factor",
                );
            }
        }
    }
}

/// Rules on the Or form `Select(a, a, b)`: idempotence, constants,
/// complement, absorption, commutation, factoring and mux recognition.
fn or_rules(egraph: &EGraph, class: ClassId, node: ENode, out: &mut Vec<Rewrite>) {
    let ENode::Select(c, x, y) = node else {
        return;
    };
    if c != x || !is_bool(egraph, y) {
        return;
    }
    if x == y {
        propose(out, class, Term::Class(x), "or-idem");
    }
    if is_true(egraph, y) {
        propose(out, class, Term::Bool(true), "or-true");
    }
    if is_false(egraph, y) {
        propose(out, class, Term::Class(x), "or-false");
    }
    if nots(egraph, y).contains(&x) || nots(egraph, x).contains(&y) {
        propose(out, class, Term::Bool(true), "or-complement");
    }
    for (p, q) in ands(egraph, y) {
        if p == x || q == x {
            propose(out, class, Term::Class(x), "or-absorb");
        }
    }
    for (p, q) in ands(egraph, x) {
        if p == y || q == y {
            propose(out, class, Term::Class(y), "or-absorb");
        }
    }
    propose(out, class, Term::or(y, x), "or-commute");
    for left in ands(egraph, x) {
        for right in ands(egraph, y) {
            for (shared, other, another) in shared_operand(left, right) {
                propose(
                    out,
                    class,
                    Term::And(Term::class(shared), Box::new(Term::or(other, another))),
                    "or-factor",
                );
            }
            // Or(And(k, p), And(Not(k), q)) = Select(k, p, q).
            let (p, q) = left;
            let (r, s) = right;
            for (k, value) in [(p, q), (q, p)] {
                for (nk, other) in [(r, s), (s, r)] {
                    if nots(egraph, nk).contains(&k) {
                        propose(
                            out,
                            class,
                            Term::Select(Term::class(k), Term::class(value), Term::class(other)),
                            "mux-recognize",
                        );
                    }
                    if nots(egraph, k).contains(&nk) {
                        propose(
                            out,
                            class,
                            Term::Select(Term::class(nk), Term::class(other), Term::class(value)),
                            "mux-recognize",
                        );
                    }
                }
            }
        }
    }
}

/// `Eq(a, a) = true`; Boolean `Eq` against constants and under negation.
fn eq_rules(egraph: &EGraph, class: ClassId, node: ENode, out: &mut Vec<Rewrite>) {
    let ENode::Eq(a, b) = node else {
        return;
    };
    if a == b {
        propose(out, class, Term::Bool(true), "eq-same");
    }
    if !is_bool(egraph, a) {
        return;
    }
    for (x, y) in [(a, b), (b, a)] {
        if is_true(egraph, y) {
            propose(out, class, Term::Class(x), "eq-true");
        }
        if is_false(egraph, y) {
            propose(out, class, Term::Not(Term::class(x)), "eq-false");
        }
    }
    for na in nots(egraph, a) {
        for nb in nots(egraph, b) {
            propose(
                out,
                class,
                Term::Eq(Term::class(na), Term::class(nb)),
                "eq-not-not",
            );
        }
    }
}

/// `Lt(a, a) = false`.
fn lt_rules(_: &EGraph, class: ClassId, node: ENode, out: &mut Vec<Rewrite>) {
    if let ENode::Lt(a, b) = node
        && a == b
    {
        propose(out, class, Term::Bool(false), "lt-same");
    }
}

// ---- Select phase ------------------------------------------------------------

pub(crate) fn select_rules() -> Vec<Rule> {
    vec![select_rule]
}

/// Select simplification for both kinds: equal arms, constant or negated
/// conditions, Boolean identities, and the condition's value inside its arms.
fn select_rule(egraph: &EGraph, class: ClassId, node: ENode, out: &mut Vec<Rewrite>) {
    let ENode::Select(c, a, b) = node else {
        return;
    };
    if a == b {
        propose(out, class, Term::Class(a), "select-same");
    }
    if is_true(egraph, c) {
        propose(out, class, Term::Class(a), "select-true");
    }
    if is_false(egraph, c) {
        propose(out, class, Term::Class(b), "select-false");
    }
    for nc in nots(egraph, c) {
        propose(
            out,
            class,
            Term::Select(Term::class(nc), Term::class(b), Term::class(a)),
            "select-not-cond",
        );
    }
    for (c2, p, _) in selects(egraph, a) {
        if c2 == c {
            propose(
                out,
                class,
                Term::Select(Term::class(c), Term::class(p), Term::class(b)),
                "select-nested-then",
            );
        }
    }
    for (c2, _, q) in selects(egraph, b) {
        if c2 == c {
            propose(
                out,
                class,
                Term::Select(Term::class(c), Term::class(a), Term::class(q)),
                "select-nested-else",
            );
        }
    }
    if !is_bool(egraph, a) {
        return;
    }
    if is_true(egraph, a) && is_false(egraph, b) {
        propose(out, class, Term::Class(c), "select-id");
    }
    if is_false(egraph, a) && is_true(egraph, b) {
        propose(out, class, Term::Not(Term::class(c)), "select-not");
    }
    if is_false(egraph, b) {
        propose(
            out,
            class,
            Term::And(Term::class(c), Term::class(a)),
            "select-and",
        );
    }
    if is_false(egraph, a) {
        propose(
            out,
            class,
            Term::And(Box::new(Term::Not(Term::class(c))), Term::class(b)),
            "select-and-not",
        );
    }
    if is_true(egraph, a) && c != a {
        propose(out, class, Term::or(c, b), "select-or");
    }
    if b == c {
        propose(
            out,
            class,
            Term::And(Term::class(c), Term::class(a)),
            "select-else-cond",
        );
    }
    for (p, q) in ands(egraph, a) {
        for (k, other) in [(p, q), (q, p)] {
            if k == c {
                propose(
                    out,
                    class,
                    Term::Select(Term::class(c), Term::class(other), Term::class(b)),
                    "select-and-then",
                );
            }
        }
    }
    for (p, q) in ands(egraph, b) {
        if p == c || q == c {
            propose(
                out,
                class,
                Term::And(Term::class(c), Term::class(a)),
                "select-and-else",
            );
        }
    }
    for (p, q) in ors(egraph, a) {
        if p == c || q == c {
            propose(out, class, Term::or(c, b), "select-or-then");
        }
    }
    if nots(egraph, a).contains(&c) {
        propose(
            out,
            class,
            Term::And(Box::new(Term::Not(Term::class(c))), Term::class(b)),
            "select-not-then",
        );
    }
    if nots(egraph, b).contains(&c) {
        propose(
            out,
            class,
            Term::Select(Term::class(c), Term::class(a), Box::new(Term::Bool(true))),
            "select-not-else",
        );
    }
}

// ---- Fold phase --------------------------------------------------------------

/// Adds the constant a class is known to equal on every tuple. The class
/// data proves the constant only for never-poisoned classes, so an `Add` or
/// `Sub` that may overflow is never folded.
fn fold_rule(egraph: &EGraph, class: ClassId, node: ENode, out: &mut Vec<Rewrite>) {
    if matches!(node, ENode::Int(_) | ENode::Bool(_)) {
        return;
    }
    let data = egraph.data(class);
    let Some(value) = data.constant() else {
        return;
    };
    let constant = match data.kind {
        Kind::Bool => ENode::Bool(value == 1),
        Kind::Int => ENode::Int(value),
    };
    if egraph.lookup(constant) == Some(class) {
        return;
    }
    let term = match data.kind {
        Kind::Bool => Term::Bool(value == 1),
        Kind::Int => Term::Int(value),
    };
    propose(out, class, term, "fold");
}

// ---- Share phase -------------------------------------------------------------

/// Commutativity of `And` and `Eq`, as a structural merge: both sides are
/// the same computation on the same operands.
fn commute_rule(egraph: &EGraph, class: ClassId, node: ENode, out: &mut Vec<Rewrite>) {
    let (rhs, rule) = match node {
        ENode::And(a, b) if a != b => (Term::And(Term::class(b), Term::class(a)), "and-commute"),
        ENode::Eq(a, b) if a != b => (Term::Eq(Term::class(b), Term::class(a)), "eq-commute"),
        _ => return,
    };
    if egraph.lookup(match node {
        ENode::And(a, b) => ENode::And(b, a),
        ENode::Eq(a, b) => ENode::Eq(b, a),
        _ => return,
    }) == Some(class)
    {
        return;
    }
    out.push(Rewrite {
        lhs: class,
        rhs,
        rule,
        reason: MergeReason::Structural,
    });
}

// ---- Semantic phase ----------------------------------------------------------

/// Merges classes with identical exact signatures, then adds single
/// instructions over existing classes whose exact signature some existing
/// class already has, so extraction can use them as cheaper alternatives.
fn semantic_round(egraph: &mut EGraph, limits: &Limits) -> Result<RoundTally, Inconsistency> {
    let mut tally = RoundTally::default();
    let mut by_signature: BTreeMap<(Kind, Signature), ClassId> = BTreeMap::new();
    let mut unions = Vec::new();
    for class in egraph.classes() {
        let data = egraph.data(class);
        let Some(signature) = data.signature.clone() else {
            continue;
        };
        match by_signature.get(&(data.kind, signature.clone())) {
            Some(existing) => unions.push((*existing, class)),
            None => {
                by_signature.insert((data.kind, signature), class);
            }
        }
    }
    for (a, b) in unions {
        match egraph.union(a, b, MergeReason::Rule)? {
            Merge::Merged => tally.merges += 1,
            Merge::Refused => tally.refused += 1,
            Merge::AlreadyEqual => {}
        }
    }
    egraph.rebuild()?;
    complete(egraph, limits, &mut tally)?;
    egraph.rebuild()?;
    Ok(tally)
}

/// Signature-directed completion. Targets are classes whose cheapest tree
/// needs at least two instructions; a one-instruction target cannot improve.
fn complete(
    egraph: &mut EGraph,
    limits: &Limits,
    tally: &mut RoundTally,
) -> Result<(), Inconsistency> {
    let caps = limits.caps();
    let full = egraph.domain().full_mask();
    let costs = tree_costs(egraph);
    let expensive = |class: ClassId| costs.get(&class).is_none_or(|(cost, _)| cost.nodes >= 2);
    // Unpoisoned Boolean classes by their bit signature; unpoisoned classes of
    // either kind by their full signature.
    let mut bools: Vec<(ClassId, u64)> = Vec::new();
    let mut ints: Vec<(ClassId, Signature)> = Vec::new();
    let mut by_bits: BTreeMap<u64, ClassId> = BTreeMap::new();
    let mut by_signature: BTreeMap<(Kind, Signature), ClassId> = BTreeMap::new();
    for class in egraph.classes() {
        let data = egraph.data(class);
        let Some(signature) = &data.signature else {
            continue;
        };
        if signature.poisoned() {
            continue;
        }
        by_signature.insert((data.kind, signature.clone()), class);
        match data.kind {
            Kind::Bool => {
                if let Some(bits) = signature.bits() {
                    bools.push((class, bits));
                    by_bits.insert(bits, class);
                }
            }
            Kind::Int => ints.push((class, signature.clone())),
        }
    }
    let mut proposals: Vec<(ClassId, ENode)> = Vec::new();
    let budget = limits.max_rewrites_per_round as usize;
    // Not(x).
    for &(x, bits) in &bools {
        if let Some(&target) = by_bits.get(&(!bits & full))
            && target != x
            && expensive(target)
        {
            proposals.push((target, ENode::Not(x)));
        }
    }
    // And(x, y) and Boolean Eq(x, y).
    for (index, &(x, xb)) in bools.iter().enumerate() {
        for &(y, yb) in &bools[index..] {
            if let Some(&target) = by_bits.get(&(xb & yb))
                && target != x
                && target != y
                && expensive(target)
            {
                proposals.push((target, ENode::And(x, y)));
            }
            if x != y
                && let Some(&target) = by_bits.get(&(!(xb ^ yb) & full))
                && target != x
                && target != y
                && expensive(target)
            {
                proposals.push((target, ENode::Eq(x, y)));
            }
        }
    }
    // Integer Eq(x, y) and Lt(x, y).
    for (index, (x, xs)) in ints.iter().enumerate() {
        for (y, ys) in &ints[index..] {
            let pairs = || xs.values().iter().zip(ys.values());
            let eq = Signature::from_values(
                pairs()
                    .map(|(a, b)| Some(i64::from((*a)? == (*b)?)))
                    .collect(),
            );
            if let Some(&target) = by_signature.get(&(Kind::Bool, eq))
                && expensive(target)
            {
                proposals.push((target, ENode::Eq(*x, *y)));
            }
            if x == y {
                continue;
            }
            for (p, q) in [(*x, *y), (*y, *x)] {
                let (ps, qs) = if p == *x { (xs, ys) } else { (ys, xs) };
                let lt = Signature::from_values(
                    ps.values()
                        .iter()
                        .zip(qs.values())
                        .map(|(a, b)| Some(i64::from((*a)? < (*b)?)))
                        .collect(),
                );
                if let Some(&target) = by_signature.get(&(Kind::Bool, lt))
                    && expensive(target)
                {
                    proposals.push((target, ENode::Lt(p, q)));
                }
            }
        }
    }
    // Select(c, a, b): a agrees with the target where c is 1 and b where it is 0.
    let targets: Vec<ClassId> = egraph
        .classes()
        .into_iter()
        .filter(|class| {
            expensive(*class)
                && egraph
                    .data(*class)
                    .signature
                    .as_ref()
                    .is_some_and(|signature| !signature.poisoned())
        })
        .collect();
    for &(c, ones) in &bools {
        if ones == 0 || ones == full {
            continue;
        }
        let zeros = !ones & full;
        let mut on_ones: BTreeMap<u64, Vec<ClassId>> = BTreeMap::new();
        let mut on_zeros: BTreeMap<u64, Vec<ClassId>> = BTreeMap::new();
        for &(x, xb) in &bools {
            on_ones.entry(xb & ones).or_default().push(x);
            on_zeros.entry(xb & zeros).or_default().push(x);
        }
        for &target in &targets {
            // A target as its own condition would only add a cyclic member.
            if target == c {
                continue;
            }
            let data = egraph.data(target);
            let Some(signature) = &data.signature else {
                continue;
            };
            match data.kind {
                Kind::Bool => {
                    let Some(tb) = signature.bits() else {
                        continue;
                    };
                    let empty = Vec::new();
                    let arms_a = on_ones.get(&(tb & ones)).unwrap_or(&empty);
                    let arms_b = on_zeros.get(&(tb & zeros)).unwrap_or(&empty);
                    for &a in arms_a.iter().filter(|a| **a != target).take(4) {
                        for &b in arms_b.iter().filter(|b| **b != target).take(4) {
                            if a != b {
                                proposals.push((target, ENode::Select(c, a, b)));
                            }
                        }
                    }
                }
                Kind::Int => {
                    let arms_a: Vec<ClassId> = ints
                        .iter()
                        .filter(|(x, xs)| *x != target && xs.agrees_on(signature, ones))
                        .map(|(x, _)| *x)
                        .take(4)
                        .collect();
                    let arms_b: Vec<ClassId> = ints
                        .iter()
                        .filter(|(x, xs)| *x != target && xs.agrees_on(signature, zeros))
                        .map(|(x, _)| *x)
                        .take(4)
                        .collect();
                    for &a in &arms_a {
                        for &b in &arms_b {
                            if a != b {
                                proposals.push((target, ENode::Select(c, a, b)));
                            }
                        }
                    }
                }
            }
        }
    }
    if proposals.len() > budget {
        proposals.truncate(budget);
        tally.limit_hit = Some(Limit::Rewrites);
    }
    tally.rewrites += proposals.len() as u64;
    for (target, node) in proposals {
        let before = egraph.enode_count();
        let class = match egraph.add(node, caps) {
            Ok(class) => class,
            Err(AddError::Limit(limit)) => {
                tally.limit_hit = Some(limit);
                return Ok(());
            }
            Err(AddError::Type(_)) => {
                tally.refused += 1;
                continue;
            }
        };
        tally.nodes_added += u64::from(egraph.enode_count() - before);
        match egraph.union(target, class, MergeReason::Rule)? {
            Merge::Merged => tally.merges += 1,
            Merge::Refused => tally.refused += 1,
            Merge::AlreadyEqual => {}
        }
    }
    Ok(())
}
