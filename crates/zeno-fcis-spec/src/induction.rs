//! The solver-free parts of law-relative induction.
//!
//! An inductive claim states an invariant over `pre.` state paths and names
//! the laws its induction step may assume. The formal tools export the step:
//! does every transition that satisfies the assumed laws, and starts in a state
//! that satisfies the invariant, end in one? This module restates the invariant
//! over another root for that obligation, and evaluates it on one state for the
//! base case, which needs no solver.

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::ast::{
    ClaimDecl, ClaimFormula, ClaimMode, Identifier, ProjectionPath, ProjectionRoot, RelExpr,
    ValueExpr,
};
use crate::logic::{
    EvalLimits, EvalOutcome, EvaluationContext, PredicateProvider, TraceStep, evaluate_relational,
};

/// Restates a state invariant over `root`, replacing every `pre.` projection
/// and keeping everything else.
///
/// Returns `None` when the invariant reads anything other than `pre.`.
/// Elaboration refuses such inductive claims, so this only guards callers that
/// build claims directly.
#[must_use]
pub fn invariant_at(invariant: &RelExpr, root: ProjectionRoot) -> Option<RelExpr> {
    Some(match invariant {
        RelExpr::Bool(value) => RelExpr::Bool(*value),
        RelExpr::Not(value) => RelExpr::Not(Box::new(invariant_at(value, root)?)),
        RelExpr::And(left, right) => RelExpr::And(
            Box::new(invariant_at(left, root)?),
            Box::new(invariant_at(right, root)?),
        ),
        RelExpr::Or(left, right) => RelExpr::Or(
            Box::new(invariant_at(left, root)?),
            Box::new(invariant_at(right, root)?),
        ),
        RelExpr::Implies(left, right) => RelExpr::Implies(
            Box::new(invariant_at(left, root)?),
            Box::new(invariant_at(right, root)?),
        ),
        RelExpr::Compare(operation, left, right) => {
            RelExpr::Compare(*operation, value_at(left, root)?, value_at(right, root)?)
        }
        RelExpr::Predicate { name, arguments } => RelExpr::Predicate {
            name: name.clone(),
            arguments: arguments
                .iter()
                .map(|argument| value_at(argument, root))
                .collect::<Option<Vec<_>>>()?
                .into_boxed_slice(),
        },
        RelExpr::ForAll {
            variable,
            start,
            end,
            body,
        } => RelExpr::ForAll {
            variable: variable.clone(),
            start: *start,
            end: *end,
            body: Box::new(invariant_at(body, root)?),
        },
        RelExpr::Exists {
            variable,
            start,
            end,
            body,
        } => RelExpr::Exists {
            variable: variable.clone(),
            start: *start,
            end: *end,
            body: Box::new(invariant_at(body, root)?),
        },
    })
}

fn value_at(value: &ValueExpr, root: ProjectionRoot) -> Option<ValueExpr> {
    Some(match value {
        ValueExpr::Int(value) => ValueExpr::Int(*value),
        ValueExpr::Var(name) => ValueExpr::Var(name.clone()),
        ValueExpr::Projection(path) if path.root() == ProjectionRoot::Pre => {
            ValueExpr::Projection(ProjectionPath::try_new(root, path.segments().to_vec())?)
        }
        ValueExpr::Projection(_) => return None,
        ValueExpr::Add(left, right) => ValueExpr::Add(
            Box::new(value_at(left, root)?),
            Box::new(value_at(right, root)?),
        ),
        ValueExpr::Sub(left, right) => ValueExpr::Sub(
            Box::new(value_at(left, root)?),
            Box::new(value_at(right, root)?),
        ),
        ValueExpr::Mul(left, right) => ValueExpr::Mul(
            Box::new(value_at(left, root)?),
            Box::new(value_at(right, root)?),
        ),
        ValueExpr::Div(mode, left, right) => ValueExpr::Div(
            *mode,
            Box::new(value_at(left, root)?),
            Box::new(value_at(right, root)?),
        ),
        ValueExpr::Sum {
            variable,
            start,
            end,
            body,
        } => ValueExpr::Sum {
            variable: variable.clone(),
            start: *start,
            end: *end,
            body: Box::new(value_at(body, root)?),
        },
    })
}

struct NoPredicates;
impl PredicateProvider for NoPredicates {
    fn evaluate(&self, _: &Identifier, _: &[i128]) -> Option<bool> {
        None
    }
}

/// Evaluates an inductive claim's invariant on one state, observed under
/// `pre.` paths, for the base case of the induction.
///
/// The base case holds only when this returns `Some(EvalOutcome::True)`.
/// Evaluation is strict, as for laws: an undefined subterm, a missing
/// observation, or a named predicate gives `Indeterminate`, and so does not
/// establish the base case. Returns `None` for a claim that is not inductive.
#[must_use]
pub fn evaluate_invariant(
    claim: &ClaimDecl,
    state: &TraceStep,
    limits: EvalLimits,
) -> Option<EvalOutcome> {
    match (claim.mode(), claim.formula()) {
        (ClaimMode::Inductive, ClaimFormula::Relational(invariant)) => Some(evaluate_relational(
            invariant,
            EvaluationContext::new(state, &NoPredicates, limits),
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use alloc::format;
    use alloc::string::String;
    use alloc::vec;

    use super::*;
    use crate::ast::{ProjectSpec, StableId};
    use crate::diagnostic::DiagnosticCode;
    use crate::logic::{IndeterminateReason, Observation};
    use crate::substance::{Substance, claim_substance};
    use crate::{ProjectLimits, SourceLimits, elaborate_project, parse_project};

    const DECLARATIONS: &str = "zeno 1;\nproject 1 induction;\n\
        type 100 state State;\ntype 101 command Command;\ntype 102 context Context;\n\
        type 105 int Count;\n\
        field 110 100 count 105;\nfield 120 101 amount 105;\n\
        reason 200 bad precedence 0;\n\
        component 300 machine { owns 100; reads pre.100; writes post.100; budget steps 10; }\n\
        merge [300];\n\
        law 400 bounded = post.100.110 >= 0 && post.100.110 <= 9;\n\
        law 401 steps = post.100.110 == pre.100.110 + command.101.120;\n";

    fn project(claims: &str) -> Result<ProjectSpec, String> {
        let source = format!("{DECLARATIONS}{claims}");
        let parsed =
            parse_project(&source, SourceLimits::default()).map_err(|set| format!("{set}"))?;
        elaborate_project(parsed, ProjectLimits::default()).map_err(|set| format!("{set}"))
    }

    fn codes(claims: &str) -> Vec<DiagnosticCode> {
        let source = format!("{DECLARATIONS}{claims}");
        let parsed = match parse_project(&source, SourceLimits::default()) {
            Ok(parsed) => parsed,
            Err(set) => return set.diagnostics().iter().map(|d| d.code()).collect(),
        };
        match elaborate_project(parsed, ProjectLimits::default()) {
            Ok(_) => Vec::new(),
            Err(set) => set.diagnostics().iter().map(|d| d.code()).collect(),
        }
    }

    fn id(value: u32) -> StableId {
        StableId::new(value).unwrap_or_else(|| unreachable!())
    }

    fn count(root: ProjectionRoot) -> ProjectionPath {
        ProjectionPath::try_new(root, vec![id(100), id(110)]).unwrap_or_else(|| unreachable!())
    }

    fn state(value: i128) -> TraceStep {
        TraceStep::try_new(vec![Observation::new(count(ProjectionRoot::Pre), value)])
            .unwrap_or_else(|| unreachable!())
    }

    #[test]
    fn inductive_claims_keep_their_assumed_laws_in_the_canonical_project() {
        let spec =
            project("claim 500 nonnegative cvc5 inductive assume [401, 400] = pre.100.110 >= 0;\n")
                .unwrap_or_else(|error| panic!("{error}"));
        let claim = &spec.claims()[0];
        assert_eq!(claim.mode(), ClaimMode::Inductive);
        assert_eq!(claim.assumptions().every_commit(), &[id(401), id(400)]);
        assert!(claim.assumptions().accepts().is_empty());
        let grouped = project(
            "claim 500 nonnegative cvc5 inductive assume [400] accept [401] = pre.100.110 >= 0;\n",
        )
        .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(grouped.claims()[0].assumptions().every_commit(), &[id(400)]);
        assert_eq!(grouped.claims()[0].assumptions().accepts(), &[id(401)]);
        assert!(
            grouped.claims()[0]
                .assumptions()
                .committed_failures()
                .is_empty()
        );
        let commitment = |claims: &str| {
            project(claims)
                .unwrap_or_else(|error| panic!("{error}"))
                .commitment::<zeno_fcis_crypto::RustCryptoSha256>()
                .unwrap_or_else(|_| unreachable!())
        };
        let assumed_both = commitment(
            "claim 500 nonnegative cvc5 inductive assume [401, 400] = pre.100.110 >= 0;\n",
        );
        let assumed_one =
            commitment("claim 500 nonnegative cvc5 inductive assume [400] = pre.100.110 >= 0;\n");
        let relational = commitment("claim 500 nonnegative cvc5 relational = pre.100.110 >= 0;\n");
        let as_accept =
            commitment("claim 500 nonnegative cvc5 inductive accept [400] = pre.100.110 >= 0;\n");
        let as_failure =
            commitment("claim 500 nonnegative cvc5 inductive failure [400] = pre.100.110 >= 0;\n");
        assert_ne!(assumed_both, assumed_one);
        assert_ne!(assumed_one, relational);
        // The group a law is assumed in is part of the claim's meaning.
        assert_ne!(assumed_one, as_accept);
        assert_ne!(as_accept, as_failure);
    }

    #[test]
    fn elaboration_refuses_inductive_claims_it_cannot_ground() {
        assert_eq!(
            codes("claim 500 empty cvc5 inductive assume [] = pre.100.110 >= 0;\n"),
            vec![DiagnosticCode::InvalidDeclaration]
        );
        assert_eq!(
            codes("claim 500 twice cvc5 inductive assume [400, 400] = pre.100.110 >= 0;\n"),
            vec![DiagnosticCode::DuplicateId]
        );
        assert_eq!(
            codes("claim 500 unknown cvc5 inductive assume [499] = pre.100.110 >= 0;\n"),
            vec![DiagnosticCode::UnknownReference]
        );
        assert_eq!(
            codes("claim 500 outcome cvc5 inductive assume [400] = post.100.110 >= 0;\n"),
            vec![DiagnosticCode::InvalidDeclaration]
        );
        assert_eq!(
            codes("claim 500 unlisted cvc5 inductive = pre.100.110 >= 0;\n"),
            vec![DiagnosticCode::ExpectedToken]
        );
        assert_eq!(
            codes(
                "claim 500 across cvc5 inductive assume [400] failure [400] = pre.100.110 >= 0;\n"
            ),
            vec![DiagnosticCode::DuplicateId]
        );
        assert_eq!(
            codes(
                "claim 500 cases cvc5 inductive accept [401] failure [400] = pre.100.110 >= 0;\n"
            ),
            Vec::new()
        );
        assert!(
            codes("claim 500 fine cvc5 inductive assume [400, 401] = pre.100.110 >= 0;\n")
                .is_empty()
        );
    }

    #[test]
    fn the_invariant_is_restated_over_the_post_state_and_nothing_else() {
        let invariant = |claims: &str| match project(claims)
            .unwrap_or_else(|error| panic!("{error}"))
            .claims()[0]
            .formula()
        {
            ClaimFormula::Relational(formula) => formula.clone(),
            ClaimFormula::Temporal(_) => unreachable!(),
        };
        let pre = invariant(
            "claim 500 grows cvc5 inductive assume [400] = pre.100.110 + 1 > pre.100.110 && pre.100.110 >= 0;\n",
        );
        let post = invariant(
            "claim 500 grows cvc5 relational = post.100.110 + 1 > post.100.110 && post.100.110 >= 0;\n",
        );
        assert_eq!(invariant_at(&pre, ProjectionRoot::Post), Some(post));
        assert_eq!(invariant_at(&pre, ProjectionRoot::Pre), Some(pre.clone()));
        let reads_command = RelExpr::Compare(
            crate::ast::CompareOp::Eq,
            ValueExpr::Projection(count(ProjectionRoot::Pre)),
            ValueExpr::Projection(
                ProjectionPath::try_new(ProjectionRoot::Command, vec![id(101), id(120)])
                    .unwrap_or_else(|| unreachable!()),
            ),
        );
        assert_eq!(invariant_at(&reads_command, ProjectionRoot::Post), None);
    }

    #[test]
    fn the_base_case_evaluates_the_invariant_strictly_on_one_state() {
        let spec = project(
            "claim 500 nonnegative cvc5 inductive assume [400] = pre.100.110 >= 0;\n\
             claim 501 grows cvc5 inductive assume [400] = pre.100.110 + 1 > pre.100.110;\n\
             claim 502 relational cvc5 relational = pre.100.110 >= 0;\n",
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let claim = |value| spec.claim(id(value)).unwrap_or_else(|| unreachable!());
        let limits = EvalLimits::default();
        assert_eq!(
            evaluate_invariant(claim(500), &state(3), limits),
            Some(EvalOutcome::True)
        );
        assert_eq!(
            evaluate_invariant(claim(500), &state(-1), limits),
            Some(EvalOutcome::False)
        );
        assert_eq!(
            evaluate_invariant(claim(501), &state(i128::MAX), limits),
            Some(EvalOutcome::Indeterminate(IndeterminateReason::Overflow))
        );
        let unobserved = TraceStep::try_new(Vec::new()).unwrap_or_else(|| unreachable!());
        assert_eq!(
            evaluate_invariant(claim(500), &unobserved, limits),
            Some(EvalOutcome::Indeterminate(
                IndeterminateReason::MissingProjection
            ))
        );
        assert_eq!(evaluate_invariant(claim(502), &state(3), limits), None);
    }

    #[test]
    fn an_invariant_that_reads_state_is_substantive() {
        let spec = project(
            "claim 500 reads cvc5 inductive assume [400] = pre.100.110 >= 0;\n\
             claim 501 constant cvc5 inductive assume [400] = true;\n\
             claim 502 single_step cvc5 relational = pre.100.110 >= 0;\n",
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let substance =
            |value| claim_substance(spec.claim(id(value)).unwrap_or_else(|| unreachable!()));
        assert_eq!(substance(500), Substance::MayConstrainTransition);
        assert_eq!(substance(501), Substance::Constant { value: true });
        // A single-step claim over the pre-state still says nothing about a transition.
        assert_eq!(substance(502), Substance::IgnoresTransition);
    }
}
