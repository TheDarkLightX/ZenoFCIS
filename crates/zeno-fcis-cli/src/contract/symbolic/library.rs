//! The library's own evaluators, as replay and the exhaustive route use
//! them: the scalar program evaluator, the law evaluator on a state, and the
//! bound Authority on a whole transition.
//!
//! The Authority exposes no successor for a decision a law refuses, so the
//! successor of such a decision is resolved here from the library program's
//! outputs and the descriptor's decision table, exactly as decision
//! construction resolves it. Wherever the Authority does commit, its
//! successor must equal this one; a difference makes the check inconclusive.

use zeno_fcis_synthesis::finite::{
    V2InputLeaf as Leaf, V2Resource, execute_v2, v2_authority::Authority, v2_composition as c,
    v2_laws as l, v2_zero_limits,
};

use super::super::declarations::Source;
use super::super::review::domain::Position;
use super::super::review::evaluate::{self, Framer, Outcome, RefusalClass};

/// Generous law limits: every law node is one Step and every observation
/// one Read, far below these.
const LAW_LIMIT: u64 = 1 << 20;

/// One law program, borrowed from the descriptor or compiled from a
/// strengthening clause.
#[derive(Clone, Copy, Debug)]
pub(super) struct LawProgram<'a> {
    pub(super) id: u32,
    pub(super) nodes: &'a [l::Op<'a>],
    pub(super) root: usize,
}

/// A tuple's state record in field-ID order, as the library admits it: the
/// review's framing of the state root.
pub(super) fn state(framer: &Framer, tuple: &[i64]) -> Vec<c::Field<'static>> {
    framer.fields(&framer.roots[0], tuple)
}

/// The library law evaluator's verdict on one law alone, on a state as a
/// genesis frame, where a law that reads the successor reads that state.
/// The descriptor's other laws follow it, as the evaluator's metadata check
/// requires, and its verdict is the first reported.
pub(super) fn verdict(
    descriptor: &c::Descriptor<'_>,
    law: LawProgram<'_>,
    state: &[c::Field<'_>],
) -> Option<l::Verdict> {
    let mut laws = vec![l::Law {
        id: law.id,
        kind: l::Kind::StateInvariant,
        scope: l::Scope::Committing,
        genesis: true,
        program: l::Program {
            nodes: law.nodes,
            root: law.root,
        },
    }];
    laws.extend(
        descriptor
            .laws
            .iter()
            .filter(|other| other.id != law.id)
            .map(|other| l::Law {
                id: other.id,
                kind: other.kind,
                scope: other.scope,
                genesis: other.genesis,
                program: l::Program {
                    nodes: other.program.nodes,
                    root: other.program.root,
                },
            }),
    );
    let frame = l::Frame::Genesis {
        initial: l::RootView::Record(state),
    };
    let limits = v2_zero_limits()
        .with_limit(V2Resource::Step, LAW_LIMIT)
        .with_limit(V2Resource::Read, LAW_LIMIT);
    let (_, _, diagnostics, _) =
        l::evaluate(&laws, descriptor.required, &frame, limits).into_parts();
    diagnostics
        .iter()
        .find(|diagnostic| diagnostic.id == law.id)
        .map(|diagnostic| diagnostic.verdict)
}

/// A verdict as reports name it.
pub(super) fn verdict_name(verdict: Option<l::Verdict>) -> &'static str {
    match verdict {
        Some(l::Verdict::Satisfied) => "satisfied",
        Some(l::Verdict::Refused(l::Failure::Violated)) => "violated",
        Some(l::Verdict::Refused(l::Failure::Undefined)) => "undefined",
        Some(l::Verdict::Refused(_)) => "refused",
        Some(_) => "skipped",
        None => "not-evaluated",
    }
}

/// The descriptor's program on a tuple, at a budget no admitted program
/// exhausts.
pub(super) fn run(descriptor: &c::Descriptor<'_>, tuple: &[i64]) -> Option<Vec<i64>> {
    let program = &descriptor.program;
    let meter = v2_zero_limits().with_limit(V2Resource::Step, crate::transform::FULL_BUDGET);
    execute_v2(
        program.inputs,
        program.outputs,
        program.nodes,
        program.roots,
        tuple,
        meter,
    )
    .into_parts()
    .0
    .ok()
}

/// The number an atom's value is: 0 or 1, the integer, or the variant ID.
fn number(atom: c::Atom<'_>) -> Option<i128> {
    match atom {
        c::Atom::Bool(value) => Some(i128::from(value)),
        c::Atom::I128(value) => Some(value),
        c::Atom::U128(value) => i128::try_from(value).ok(),
        c::Atom::Sum { variant, .. } | c::Atom::Enum { variant, .. } => Some(i128::from(variant)),
        _ => None,
    }
}

/// An output's number by its output type, as reification decodes it.
fn reify(leaf: &Leaf, code: i64) -> Option<i128> {
    match leaf {
        Leaf::Bool => (code == 0 || code == 1).then_some(i128::from(code)),
        Leaf::I128 { min, max } => (*min..=*max).contains(&code).then_some(i128::from(code)),
        Leaf::U128 { min, max } => u128::try_from(code)
            .is_ok_and(|unsigned| (*min..=*max).contains(&unsigned))
            .then_some(i128::from(code)),
        Leaf::Sum { variants, .. } | Leaf::Enum { variants, .. } => variants
            .iter()
            .find(|variant| variant.code == code)
            .map(|variant| i128::from(variant.id)),
        _ => None,
    }
}

/// Whether a slot's domain admits a number, for the atom type it has.
fn admitted(domain: c::Domain<'_>, value: i128) -> bool {
    match domain {
        c::Domain::Bool => value == 0 || value == 1,
        c::Domain::I128 { min, max } => (min..=max).contains(&value),
        c::Domain::U128 { min, max } => {
            u128::try_from(value).is_ok_and(|value| (min..=max).contains(&value))
        }
        c::Domain::Sum { variants, .. } | c::Domain::Enum { variants, .. } => {
            u16::try_from(value).is_ok_and(|value| variants.contains(&value))
        }
        _ => false,
    }
}

/// The branch the decision output selects on a tuple, from the program's
/// outputs; `None` when the outputs do not reify or no branch has the code.
pub(super) fn branch(descriptor: &c::Descriptor<'_>, outputs: &[i64]) -> Option<usize> {
    let code = reify(
        descriptor.output_types.get(descriptor.decision_output)?,
        *outputs.get(descriptor.decision_output)?,
    )?;
    descriptor
        .branches
        .iter()
        .position(|branch| branch.code == code)
}

/// The successor branch `index` assigns on a tuple, field by field, as
/// decision construction resolves and admits it.
///
/// # Errors
/// Why the library would construct no successor.
pub(super) fn successor(
    descriptor: &c::Descriptor<'_>,
    positions: &[Position],
    tuple: &[i64],
    outputs: &[i64],
    index: usize,
) -> Result<Vec<(u16, i128)>, String> {
    let branch = descriptor
        .branches
        .get(index)
        .ok_or_else(|| format!("no branch {index}"))?;
    let input = |source: Source, field: Option<u16>| {
        positions
            .iter()
            .position(|position| (position.source, position.field) == (source, field))
            .and_then(|at| tuple.get(at))
            .map(|value| i128::from(*value))
    };
    let mut post = Vec::new();
    for plan in branch.assignments {
        let value = match plan.value {
            c::Expr::Input(c::Source::State, id) => input(Source::State, Some(id)),
            c::Expr::Input(c::Source::Command, id) => input(Source::Command, Some(id)),
            c::Expr::Input(c::Source::Context, id) => input(Source::Context, Some(id)),
            c::Expr::Root(c::Source::Command) => input(Source::Command, None),
            c::Expr::Root(c::Source::Context) => input(Source::Context, None),
            c::Expr::Output(at) => descriptor
                .output_types
                .get(at)
                .zip(outputs.get(at))
                .and_then(|(leaf, code)| reify(leaf, *code)),
            c::Expr::Constant(atom) => number(atom),
            _ => None,
        }
        .ok_or_else(|| format!("field {} resolves to no number", plan.field))?;
        if !admitted(plan.domain, value) {
            return Err(format!(
                "field {} = {value} is outside its declared domain",
                plan.field
            ));
        }
        post.push((plan.field, value));
    }
    Ok(post)
}

/// The tuple with its state positions set to `state`.
pub(super) fn with_state(positions: &[Position], tuple: &[i64], state: &[(u16, i128)]) -> Vec<i64> {
    positions
        .iter()
        .zip(tuple)
        .map(|(position, value)| {
            match (position.source, position.field) {
                (Source::State, Some(field)) => state
                    .iter()
                    .find(|(id, _)| *id == field)
                    .and_then(|(_, value)| i64::try_from(*value).ok()),
                _ => None,
            }
            .unwrap_or(*value)
        })
        .collect()
}

/// What the bound Authority made of a tuple, as replay needs it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Decided {
    /// A committing decision with its successor.
    Commits(Vec<(u16, i128)>),
    /// A reject.
    Rejects,
    /// A law refused the decision; the law, as the diagnostics name it.
    LawRefuses(Option<u32>, String),
    /// Refused before law evaluation, or otherwise.
    Refuses(String),
}

/// Evaluates a tuple with the bound Authority.
pub(super) fn decide(authority: &Authority<'_>, framer: &Framer, tuple: &[i64]) -> Decided {
    match evaluate::evaluate(authority, framer, tuple) {
        Outcome::Decision(decision) => {
            if decision.class == super::super::rules::Class::Reject {
                return Decided::Rejects;
            }
            let post: Option<Vec<(u16, i128)>> = decision
                .post
                .iter()
                .map(|(field, value)| Some((*field, value.number()?)))
                .collect();
            match post {
                Some(post) => Decided::Commits(post),
                None => Decided::Refuses("the successor has a value with no number".to_owned()),
            }
        }
        Outcome::Refused(refusal) if refusal.class == RefusalClass::Law => {
            Decided::LawRefuses(refusal.law, refusal.to_string())
        }
        Outcome::Refused(refusal) => Decided::Refuses(refusal.to_string()),
    }
}

impl Decided {
    pub(super) fn text(&self) -> String {
        match self {
            Self::Commits(_) => "commits".to_owned(),
            Self::Rejects => "rejects".to_owned(),
            Self::LawRefuses(_, text) | Self::Refuses(text) => format!("refuses: {text}"),
        }
    }
}
