//! Invariants, assumptions and guarantees as closed law programs.
//!
//! A manifest formula is compiled by the contract's own law compiler, with
//! the leaf's rule variables expanded, into the library's law program
//! language, and the library's law evaluator decides it. Nothing here
//! interprets a formula.
//!
//! Each kind reads a fixed set of observations, checked when it compiles:
//!
//! - a state predicate (an invariant or a StateFact) reads `post.100.f` and
//!   is decided on a state as a genesis frame, where `post` is that state,
//!   as an inductive claim is;
//! - an assumption reads `command.101.f` and `context.102.f` and is decided
//!   on the exact local input, the command and context fields as one record
//!   in field order, as a genesis frame. Each selector names its field ID,
//!   which is unique across the leaf's roots, so the record has one field
//!   for each selector;
//! - a transition relation reads `pre`, `post`, `command` and `context`
//!   fields and is decided on the actual transition frame of a decision,
//!   within its declared scope.
//!
//! The law evaluator requires its mandatory law family, so the predicate is
//! evaluated first, followed by one `true` law of each mandatory kind; the
//! predicate's own verdict is the first diagnostic.

use std::collections::{BTreeMap, BTreeSet};

use zeno_fcis_synthesis::finite::{V2Resource, v2_composition as c, v2_laws as l, v2_zero_limits};

use super::manifest::Scope;
use super::refusal::{LawRefusal, law_refusal};
use crate::internals::declarations::Declarations;
use crate::internals::expr::{self, Ast};
use crate::internals::graph::{Graph, LawGraph, LawOp, Observation};
use crate::internals::policy;

/// Generous evaluation limits: each node is one Step and each observation
/// one Read, far below these.
const LIMIT: u64 = 1 << 20;
const PREDICATE: u32 = 1;

/// Where a predicate is decided.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Form {
    State,
    Input,
    Transition(Scope),
}

/// The fields a leaf's roots declare, which bound what a predicate reads.
#[derive(Debug)]
pub(crate) struct RootFields<'a> {
    pub(crate) state: &'a [u16],
    pub(crate) command: &'a [u16],
    pub(crate) context: &'a [u16],
}

/// One compiled predicate.
#[derive(Clone, Debug)]
pub(crate) struct Predicate {
    nodes: Vec<LawOp>,
    root: usize,
    form: Form,
    /// Every field the predicate observes.
    fields: BTreeSet<u16>,
}

/// The law evaluator's verdict on a predicate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Truth {
    Holds,
    Fails,
    /// A transition relation whose scope excludes the decision.
    OutOfScope,
    /// A missing observation, a type error, an arithmetic trap or a refused
    /// frame: the predicate has no value here.
    Undefined(Undefined),
}

/// Why a predicate has no value, as the law evaluator reports it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Undefined {
    /// The predicate's law refused.
    Law(LawRefusal),
    /// The evaluator reported no verdict for the predicate.
    NoVerdict,
}

impl Truth {
    /// What the verdict means, for a reader.
    pub(crate) fn describe(self) -> String {
        match self {
            Self::Holds => "holds".to_owned(),
            Self::Fails => "fails".to_owned(),
            Self::OutOfScope => "is out of scope".to_owned(),
            Self::Undefined(Undefined::NoVerdict) => {
                "has no verdict: the law evaluator refused the frame or the program".to_owned()
            }
            Self::Undefined(Undefined::Law(refusal)) => {
                format!("is refused: {}", refusal.describe())
            }
        }
    }
}

impl Predicate {
    /// Compiles a formula of one form.
    ///
    /// # Errors
    /// The compiler's refusal, or an observation the form does not read.
    pub(crate) fn compile(
        declarations: &Declarations,
        variables: &BTreeMap<String, Ast>,
        formula: &str,
        form: Form,
        roots: &RootFields<'_>,
    ) -> Result<Self, String> {
        let ast = expr::parse(formula)?;
        let ast = expr::expand(&ast, variables)?;
        let mut graph = LawGraph::new(declarations);
        let value = graph.compile(&ast)?;
        let root = graph.boolean(value)?.index;
        let mut nodes = graph.table.nodes;
        let mut fields = BTreeSet::new();
        for node in &mut nodes {
            let observation = match node {
                LawOp::Observe(observation) | LawOp::ObserveWhen(_, observation, _) => observation,
                _ => continue,
            };
            let (field, allowed, relabel) = match (*observation, form) {
                (Observation::Post(field), Form::State) => (field, roots.state, None),
                (Observation::Command(field), Form::Input) => {
                    (field, roots.command, Some(Observation::Initial(field)))
                }
                (Observation::Context(field), Form::Input) => {
                    (field, roots.context, Some(Observation::Initial(field)))
                }
                (Observation::Pre(field) | Observation::Post(field), Form::Transition(_)) => {
                    (field, roots.state, None)
                }
                (Observation::Command(field), Form::Transition(_)) => (field, roots.command, None),
                (Observation::Context(field), Form::Transition(_)) => (field, roots.context, None),
                (other, _) => {
                    return Err(format!(
                        "reads {}, which a {} does not read",
                        observed(other),
                        form.name()
                    ));
                }
            };
            if !allowed.contains(&field) {
                return Err(format!(
                    "reads field {field}, which its root does not declare"
                ));
            }
            if let Some(relabeled) = relabel {
                *observation = relabeled;
            }
            fields.insert(field);
        }
        Ok(Self {
            nodes,
            root,
            form,
            fields,
        })
    }

    /// A state predicate over a law graph the caller compiled, whose every
    /// observation is a `post.100.f` read of a field of `fields`: a
    /// composed-domain formula, whose names the caller bound to such reads.
    pub(crate) fn from_state_graph(nodes: Vec<LawOp>, root: usize, fields: BTreeSet<u16>) -> Self {
        Self {
            nodes,
            root,
            form: Form::State,
            fields,
        }
    }

    /// Every field the predicate observes.
    pub(crate) fn fields(&self) -> &BTreeSet<u16> {
        &self.fields
    }

    /// The verdict on one state record, in field order.
    pub(crate) fn on_state(&self, state: &[c::Field<'_>]) -> Truth {
        debug_assert_eq!(self.form, Form::State);
        self.evaluate(&l::Frame::Genesis {
            initial: l::RootView::Record(state),
        })
    }

    /// The verdict on one local input: the command and context fields as one
    /// record in field order.
    pub(crate) fn on_input(&self, input: &[c::Field<'_>]) -> Truth {
        debug_assert_eq!(self.form, Form::Input);
        self.evaluate(&l::Frame::Genesis {
            initial: l::RootView::Record(input),
        })
    }

    /// The verdict on the actual transition frame of one decision.
    pub(crate) fn on_transition(&self, frame: &l::Frame<'_>) -> Truth {
        debug_assert!(matches!(self.form, Form::Transition(_)));
        self.evaluate(frame)
    }

    fn evaluate(&self, frame: &l::Frame<'_>) -> Truth {
        let nodes: Vec<l::Op<'_>> = self.nodes.iter().map(policy::law_op).collect();
        let (scope, genesis) = match self.form {
            Form::State | Form::Input => (l::Scope::Always, true),
            Form::Transition(scope) => (
                match scope {
                    Scope::Accept => l::Scope::Accept,
                    Scope::Reject => l::Scope::Reject,
                    Scope::Commit => l::Scope::Committing,
                    Scope::Any => l::Scope::Always,
                },
                false,
            ),
        };
        let truth = [l::Op::Literal(l::Atom::Bool(true))];
        let law = |id, kind, scope, genesis, nodes| l::Law {
            id,
            kind,
            scope,
            genesis,
            program: l::Program { nodes, root: 0 },
        };
        let laws = [
            l::Law {
                id: PREDICATE,
                kind: l::Kind::AssetConservation,
                scope,
                genesis,
                program: l::Program {
                    nodes: &nodes,
                    root: self.root,
                },
            },
            law(
                2,
                l::Kind::StateInvariant,
                l::Scope::Committing,
                true,
                &truth,
            ),
            law(
                3,
                l::Kind::RejectNoAuthority,
                l::Scope::Reject,
                false,
                &truth,
            ),
            law(
                4,
                l::Kind::CommittedFailureEffects,
                l::Scope::CommittedFailure,
                false,
                &truth,
            ),
            law(
                5,
                l::Kind::DecisionConformance,
                l::Scope::Always,
                false,
                &truth,
            ),
            law(6, l::Kind::InitialCondition, l::Scope::Always, true, &truth),
        ];
        let limits = v2_zero_limits()
            .with_limit(V2Resource::Step, LIMIT)
            .with_limit(V2Resource::Read, LIMIT);
        let (result, _, diagnostics, _) =
            l::evaluate(&laws, &[PREDICATE], frame, limits).into_parts();
        match diagnostics.first() {
            Some(diagnostic) if diagnostic.id == PREDICATE => match diagnostic.verdict {
                l::Verdict::Satisfied => Truth::Holds,
                l::Verdict::Skipped => Truth::OutOfScope,
                l::Verdict::Refused(l::Failure::Violated) => Truth::Fails,
                l::Verdict::Refused(failure) => {
                    Truth::Undefined(Undefined::Law(law_refusal(failure)))
                }
                _ => Truth::Undefined(Undefined::NoVerdict),
            },
            _ => match result {
                Err(failure) => Truth::Undefined(Undefined::Law(law_refusal(failure))),
                Ok(()) => Truth::Undefined(Undefined::NoVerdict),
            },
        }
    }
}

impl Form {
    fn name(self) -> &'static str {
        match self {
            Self::State => "state predicate",
            Self::Input => "assumption",
            Self::Transition(_) => "transition relation",
        }
    }
}

/// An observation as a formula names it.
fn observed(observation: Observation) -> String {
    match observation {
        Observation::Pre(field) => format!("pre.100.{field}"),
        Observation::Command(field) => format!("command.101.{field}"),
        Observation::Context(field) => format!("context.102.{field}"),
        Observation::Post(field) => format!("post.100.{field}"),
        Observation::Initial(field) => format!("the initial field {field}"),
        Observation::PreRoot => "the pre-state root".to_owned(),
        Observation::CommandRoot => "the command root".to_owned(),
        Observation::ContextRoot => "the context root".to_owned(),
        Observation::PostRoot => "the successor root".to_owned(),
        Observation::Class => "the decision class".to_owned(),
        Observation::HasReason | Observation::Reason => "the decision reason".to_owned(),
        Observation::PostLength | Observation::PatchLength => "the successor's size".to_owned(),
        Observation::EffectLength => "the effects".to_owned(),
        Observation::OutboxLength
        | Observation::OutboxOrdinal(_)
        | Observation::OutboxChannel(_)
        | Observation::OutboxDestination(_)
        | Observation::OutboxPayload(..)
        | Observation::OutboxIdempotency(_) => "the outbox".to_owned(),
    }
}
