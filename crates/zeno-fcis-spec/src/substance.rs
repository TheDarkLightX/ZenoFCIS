//! Syntactic substance analysis for authored laws and claims.
//!
//! A law or claim whose value cannot change with the system's behavior gives a
//! reviewer no evidence about that behavior, even when a checker or prover
//! accepts it. This analysis reports such formulas before anyone relies on
//! them. It is deliberately conservative: it never states that a formula
//! constrains behavior, only that it might.
//!
//! Constant results assume that every observation the formula reads is present
//! and that evaluation stays within its limits. A formula is folded to a
//! constant only when no subterm can become indeterminate: arithmetic, host
//! predicates, and non-empty quantifiers are never folded, because the checked
//! evaluator can report them as indeterminate and fail closed.

use crate::ast::{
    ClaimDecl, ClaimFormula, CompareOp, LawDecl, ProjectionRoot, RelExpr, TemporalFormula,
    ValueExpr,
};

/// What an authored law or claim can say about system behavior, judged from
/// its syntax alone.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Substance {
    /// The formula has this truth value for every assignment of the
    /// observations it reads. A true formula can never fail and a false one
    /// can never hold, so neither checks any behavior.
    Constant {
        /// The fixed truth value.
        value: bool,
    },
    /// The formula reads no observation that a transition outcome can change:
    /// no post-state, effect, outbox, or event for a single-step formula, and
    /// no observation at all for a temporal formula.
    IgnoresTransition,
    /// The formula reads an observation that a transition can change. This is
    /// necessary for a meaningful constraint; it does not establish one.
    MayConstrainTransition,
}

impl Substance {
    /// Returns the stable machine-readable name.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Constant { value: true } => "constant-true",
            Self::Constant { value: false } => "constant-false",
            Self::IgnoresTransition => "ignores-transition",
            Self::MayConstrainTransition => "may-constrain-transition",
        }
    }

    /// Returns true when the formula cannot distinguish two system behaviors.
    #[must_use]
    pub const fn is_vacuous(self) -> bool {
        !matches!(self, Self::MayConstrainTransition)
    }
}

/// Classifies one law. A law is a single-step relation over one transition.
#[must_use]
pub fn law_substance(law: &LawDecl) -> Substance {
    relation_substance(law.formula())
}

/// Classifies one claim.
#[must_use]
pub fn claim_substance(claim: &ClaimDecl) -> Substance {
    match claim.formula() {
        ClaimFormula::Relational(expr) => relation_substance(expr),
        ClaimFormula::Temporal(formula) => temporal_substance(formula),
    }
}

/// Classifies a single-step relational formula.
#[must_use]
pub fn relation_substance(expr: &RelExpr) -> Substance {
    if let Some(value) = constant_relation(expr) {
        Substance::Constant { value }
    } else if relation_reads(expr, ReadScope::Outcome) {
        Substance::MayConstrainTransition
    } else {
        Substance::IgnoresTransition
    }
}

/// Classifies a finite-trace temporal formula.
///
/// Across steps, a later pre-state is an earlier post-state, so a temporal
/// formula over pre-state observations can still constrain behavior. Only
/// constant formulas and formulas that read no observation are reported.
#[must_use]
pub fn temporal_substance(formula: &TemporalFormula) -> Substance {
    if let Some(value) = constant_temporal(formula) {
        Substance::Constant { value }
    } else if temporal_reads_any(formula) {
        Substance::MayConstrainTransition
    } else {
        Substance::IgnoresTransition
    }
}

#[derive(Clone, Copy)]
enum ReadScope {
    Any,
    Outcome,
}

impl ReadScope {
    const fn accepts(self, root: ProjectionRoot) -> bool {
        match self {
            Self::Any => true,
            Self::Outcome => matches!(
                root,
                ProjectionRoot::Post
                    | ProjectionRoot::Effects
                    | ProjectionRoot::Outbox
                    | ProjectionRoot::Events
            ),
        }
    }
}

/// Returns the value a relation takes whenever its observations are present,
/// if syntax alone determines it and no subterm can be indeterminate.
fn constant_relation(expr: &RelExpr) -> Option<bool> {
    match expr {
        RelExpr::Bool(value) => Some(*value),
        RelExpr::Not(inner) => constant_relation(inner).map(|value| !value),
        RelExpr::And(left, right) => fold_connective(left, right, false, |a, b| a && b),
        RelExpr::Or(left, right) => fold_connective(left, right, true, |a, b| a || b),
        RelExpr::Implies(left, right) => {
            match (constant_relation(left), constant_relation(right)) {
                (Some(antecedent), Some(consequent)) => Some(!antecedent || consequent),
                (Some(false), None) if relation_always_defined(right) => Some(true),
                (None, Some(true)) if relation_always_defined(left) => Some(true),
                _ => None,
            }
        }
        RelExpr::Compare(operation, left, right) => constant_comparison(*operation, left, right),
        RelExpr::Predicate { .. } => None,
        RelExpr::ForAll { start, end, .. } => (start >= end).then_some(true),
        RelExpr::Exists { start, end, .. } => (start >= end).then_some(false),
    }
}

/// Folds a strict binary connective. `absorbing` is the operand value that
/// fixes the result regardless of the other operand, provided that other
/// operand is always defined.
fn fold_connective(
    left: &RelExpr,
    right: &RelExpr,
    absorbing: bool,
    combine: fn(bool, bool) -> bool,
) -> Option<bool> {
    match (constant_relation(left), constant_relation(right)) {
        (Some(a), Some(b)) => Some(combine(a, b)),
        (Some(value), None) if value == absorbing && relation_always_defined(right) => {
            Some(absorbing)
        }
        (None, Some(value)) if value == absorbing && relation_always_defined(left) => {
            Some(absorbing)
        }
        _ => None,
    }
}

fn constant_comparison(operation: CompareOp, left: &ValueExpr, right: &ValueExpr) -> Option<bool> {
    if let (ValueExpr::Int(a), ValueExpr::Int(b)) = (left, right) {
        return Some(compare(operation, *a, *b));
    }
    if left == right && value_always_defined(left) {
        return Some(matches!(
            operation,
            CompareOp::Eq | CompareOp::LessEq | CompareOp::GreaterEq
        ));
    }
    None
}

const fn compare(operation: CompareOp, left: i128, right: i128) -> bool {
    match operation {
        CompareOp::Eq => left == right,
        CompareOp::NotEq => left != right,
        CompareOp::Less => left < right,
        CompareOp::LessEq => left <= right,
        CompareOp::Greater => left > right,
        CompareOp::GreaterEq => left >= right,
    }
}

/// True when the relation can never evaluate to indeterminate while its
/// observations are present. Predicates and quantifiers are excluded because
/// host predicates may be missing and quantifier iterations are budgeted.
fn relation_always_defined(expr: &RelExpr) -> bool {
    match expr {
        RelExpr::Bool(_) => true,
        RelExpr::Not(inner) => relation_always_defined(inner),
        RelExpr::And(left, right) | RelExpr::Or(left, right) | RelExpr::Implies(left, right) => {
            relation_always_defined(left) && relation_always_defined(right)
        }
        RelExpr::Compare(_, left, right) => {
            value_always_defined(left) && value_always_defined(right)
        }
        RelExpr::Predicate { .. } | RelExpr::ForAll { .. } | RelExpr::Exists { .. } => false,
    }
}

/// True for constants, variables, and projections. Arithmetic can overflow or
/// divide inexactly and is therefore never treated as always defined.
const fn value_always_defined(expr: &ValueExpr) -> bool {
    matches!(
        expr,
        ValueExpr::Int(_) | ValueExpr::Var(_) | ValueExpr::Projection(_)
    )
}

fn relation_reads(expr: &RelExpr, scope: ReadScope) -> bool {
    match expr {
        RelExpr::Bool(_) => false,
        RelExpr::Not(inner) => relation_reads(inner, scope),
        RelExpr::And(left, right) | RelExpr::Or(left, right) | RelExpr::Implies(left, right) => {
            relation_reads(left, scope) || relation_reads(right, scope)
        }
        RelExpr::Compare(_, left, right) => value_reads(left, scope) || value_reads(right, scope),
        RelExpr::Predicate { arguments, .. } => arguments
            .iter()
            .any(|argument| value_reads(argument, scope)),
        RelExpr::ForAll { body, .. } | RelExpr::Exists { body, .. } => relation_reads(body, scope),
    }
}

fn value_reads(expr: &ValueExpr, scope: ReadScope) -> bool {
    match expr {
        ValueExpr::Int(_) | ValueExpr::Var(_) => false,
        ValueExpr::Projection(path) => scope.accepts(path.root()),
        ValueExpr::Add(left, right)
        | ValueExpr::Sub(left, right)
        | ValueExpr::Mul(left, right)
        | ValueExpr::Div(_, left, right) => value_reads(left, scope) || value_reads(right, scope),
        ValueExpr::Sum { body, .. } => value_reads(body, scope),
    }
}

/// Returns the value a temporal formula takes on every non-empty finite trace
/// whose observations are present, if syntax alone determines it.
fn constant_temporal(formula: &TemporalFormula) -> Option<bool> {
    match formula {
        TemporalFormula::Atom(expr) => constant_relation(expr),
        TemporalFormula::Not(inner) => constant_temporal(inner).map(|value| !value),
        TemporalFormula::And(left, right) => {
            fold_temporal_connective(left, right, false, |a, b| a && b)
        }
        TemporalFormula::Or(left, right) => {
            fold_temporal_connective(left, right, true, |a, b| a || b)
        }
        // Every remaining suffix is non-empty, so a constant body fixes both.
        TemporalFormula::Always(inner) | TemporalFormula::Eventually(inner) => {
            constant_temporal(inner)
        }
        // `next` is false at the final step, so only a false body is constant.
        TemporalFormula::Next(inner) => match constant_temporal(inner) {
            Some(false) => Some(false),
            _ => None,
        },
        // Strong until is decided at the current step by a constant goal, if
        // the left operand cannot make strict evaluation indeterminate.
        TemporalFormula::Until(left, goal) => {
            constant_temporal(goal).filter(|_| temporal_always_defined(left))
        }
    }
}

fn fold_temporal_connective(
    left: &TemporalFormula,
    right: &TemporalFormula,
    absorbing: bool,
    combine: fn(bool, bool) -> bool,
) -> Option<bool> {
    match (constant_temporal(left), constant_temporal(right)) {
        (Some(a), Some(b)) => Some(combine(a, b)),
        (Some(value), None) if value == absorbing && temporal_always_defined(right) => {
            Some(absorbing)
        }
        (None, Some(value)) if value == absorbing && temporal_always_defined(left) => {
            Some(absorbing)
        }
        _ => None,
    }
}

fn temporal_always_defined(formula: &TemporalFormula) -> bool {
    match formula {
        TemporalFormula::Atom(expr) => relation_always_defined(expr),
        TemporalFormula::Not(inner)
        | TemporalFormula::Next(inner)
        | TemporalFormula::Always(inner)
        | TemporalFormula::Eventually(inner) => temporal_always_defined(inner),
        TemporalFormula::And(left, right)
        | TemporalFormula::Or(left, right)
        | TemporalFormula::Until(left, right) => {
            temporal_always_defined(left) && temporal_always_defined(right)
        }
    }
}

fn temporal_reads_any(formula: &TemporalFormula) -> bool {
    match formula {
        TemporalFormula::Atom(expr) => relation_reads(expr, ReadScope::Any),
        TemporalFormula::Not(inner)
        | TemporalFormula::Next(inner)
        | TemporalFormula::Always(inner)
        | TemporalFormula::Eventually(inner) => temporal_reads_any(inner),
        TemporalFormula::And(left, right)
        | TemporalFormula::Or(left, right)
        | TemporalFormula::Until(left, right) => {
            temporal_reads_any(left) || temporal_reads_any(right)
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::format;
    use alloc::string::String;

    use super::*;
    use crate::{ProjectLimits, ProjectSpec, SourceLimits, elaborate_project, parse_project};

    const HEADER: &str = "zeno 1;
project 1 substance;
namespace 10 core;
type 100 state State;
type 101 command Command;
type 102 context Context;
type 103 destination Destination;
type 104 payload Payload;
reason 200 invalid precedence 0;
component 300 machine {
  owns 100;
  reads pre.100;
  writes post.100;
  contexts context.102;
  budget steps 1024;
}
merge [300];
";

    fn project(declarations: &str) -> ProjectSpec {
        let mut source = String::from(HEADER);
        source.push_str(declarations);
        source.push('\n');
        let parsed =
            parse_project(&source, SourceLimits::default()).unwrap_or_else(|set| panic!("{set}"));
        elaborate_project(parsed, ProjectLimits::default()).unwrap_or_else(|set| panic!("{set}"))
    }

    fn law(formula: &str) -> Substance {
        let spec = project(&format!("law 400 subject = {formula};"));
        law_substance(&spec.laws()[0])
    }

    fn claim(backends_and_mode: &str, formula: &str) -> Substance {
        let spec = project(&format!(
            "claim 500 subject {backends_and_mode} = {formula};"
        ));
        claim_substance(&spec.claims()[0])
    }

    const TRUE: Substance = Substance::Constant { value: true };
    const FALSE: Substance = Substance::Constant { value: false };

    #[test]
    fn substance_flags_the_shipped_reflexive_examples() {
        // Vacuity control: the exact formulas shipped in examples/minimal and
        // examples/mini-determinator can never fail.
        assert_eq!(law("pre.100 == pre.100"), TRUE);
        assert_eq!(claim("cvc5 relational", "pre.100 == pre.100"), TRUE);
        assert_eq!(
            claim("all finite 4", "always atom(pre.100 == pre.100)"),
            TRUE
        );
        assert_eq!(
            claim("lean unbounded", "always atom(pre.100 == pre.100)"),
            TRUE
        );
    }

    #[test]
    fn substance_flags_constant_template_placeholders() {
        assert_eq!(law("true"), TRUE);
        assert_eq!(law("false"), FALSE);
        assert_eq!(law("pre.100.110 != pre.100.110"), FALSE);
        assert_eq!(law("3 < 2"), FALSE);
    }

    #[test]
    fn substance_keeps_template_transition_laws() {
        // Control against false alarms: the durable-counter template's
        // transition laws read post-state and are never reported.
        let bounded =
            "post.100.110 >= 0 && post.100.110 <= 3 && post.100.111 >= 0 && post.100.111 <= 3";
        let increment = "context.102 == 1 && command.101 == 120 && post.100.110 == pre.100.110 + 1 && post.100.111 == pre.100.111";
        for formula in [bounded, increment] {
            let substance = law(formula);
            assert_eq!(substance, Substance::MayConstrainTransition, "{formula}");
            assert!(!substance.is_vacuous());
        }
    }

    #[test]
    fn substance_reports_single_step_laws_that_ignore_the_outcome() {
        assert_eq!(
            law("context.102 == 1 && command.101 == 120"),
            Substance::IgnoresTransition
        );
        assert_eq!(law("pre.100.110 <= 3"), Substance::IgnoresTransition);
    }

    #[test]
    fn substance_folds_only_operands_that_cannot_be_indeterminate() {
        // Strict connectives: an absorbing constant fixes the result only when
        // the other operand is always defined.
        assert_eq!(law("false && post.100.110 == 1"), FALSE);
        assert_eq!(law("true || post.100.110 == 1"), TRUE);
        assert_eq!(law("false -> post.100.110 == 1"), TRUE);
        // Division can be inexact or divide by zero and fail closed, so the
        // same shapes are not constant.
        assert_eq!(
            law("false && div_exact(post.100.110, 2) == 1"),
            Substance::MayConstrainTransition
        );
        // Reflexive arithmetic can overflow and fail closed.
        assert_eq!(
            law("post.100.110 + 1 == post.100.110 + 1"),
            Substance::MayConstrainTransition
        );
        // Empty quantifier ranges are decided without evaluating the body.
        assert_eq!(law("forall i in 3..3 { post.100.110 == i }"), TRUE);
        assert_eq!(law("exists i in 3..3 { post.100.110 == i }"), FALSE);
    }

    #[test]
    fn substance_keeps_temporal_pre_state_claims() {
        // Across steps a later pre-state is an earlier post-state, so this is a
        // reachable-state claim and is not reported.
        assert_eq!(
            claim("all finite 4", "always atom(pre.100.110 <= 3)"),
            Substance::MayConstrainTransition
        );
    }

    #[test]
    fn substance_respects_finite_trace_temporal_semantics() {
        // `next` is false at the final step, so a true body is not constant,
        // but it still reads no observation.
        assert_eq!(
            claim("all finite 4", "next atom(true)"),
            Substance::IgnoresTransition
        );
        assert_eq!(claim("all finite 4", "next atom(false)"), FALSE);
        assert_eq!(claim("all finite 4", "eventually atom(true)"), TRUE);
        assert_eq!(
            claim("all finite 4", "atom(pre.100.110 <= 3) until atom(true)"),
            TRUE
        );
    }

    #[test]
    fn substance_codes_are_stable() {
        assert_eq!(TRUE.code(), "constant-true");
        assert_eq!(FALSE.code(), "constant-false");
        assert_eq!(Substance::IgnoresTransition.code(), "ignores-transition");
        assert_eq!(
            Substance::MayConstrainTransition.code(),
            "may-constrain-transition"
        );
        assert!(TRUE.is_vacuous() && FALSE.is_vacuous());
        assert!(Substance::IgnoresTransition.is_vacuous());
    }
}
