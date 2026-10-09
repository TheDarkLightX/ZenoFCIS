//! The behaviour-change admission: whether a later contract's state laws and
//! the inductive claims its lineage declares hold on the store's current
//! state. It needs no comparison of decision programs, so it never waits on
//! one.
//!
//! The check runs the library's own law evaluator through the verified
//! core's genesis framing, `v2_composition::bind` and `BoundCore::frame`,
//! exactly as a genesis publication evaluates laws, but over a descriptor in
//! which:
//!
//! - every *state law* of the new contract keeps its program: a law that
//!   applies at genesis and to every committing decision, other than an
//!   `InitialCondition` law (the definition `zeno-fcis contract review`
//!   uses);
//! - every other law's program is the single literal `true`, so its own
//!   predicate never runs on the store's state. This is how generated law
//!   990, genesis exactness, is not evaluated: it applies only to new
//!   stores, and the record lists it as not evaluated;
//! - each declared claim joins as a state invariant over the state;
//! - the Read and Step limits grow by one for each claim node, so that the
//!   claims' own evaluation cannot exhaust the meter the contract declares.
//!
//! The core then decodes the state against the new contract's own schema and
//! framing, which also checks that every value is admissible under it, and
//! evaluates every law on that one frame. A law or claim that is false,
//! undefined or out of budget refuses the admission.
//!
//! What this establishes: the state laws and claims hold on this state, as
//! the library evaluates them. It does not show that the new contract could
//! have reached this state from its own genesis: the history was made under
//! the old rules.

use std::fmt;

use zeno_fcis_codec::{CommitmentHasher, Hash32};
use zeno_fcis_crypto::RustCryptoSha256;

use zeno_fcis_synthesis::finite::{
    V2Resource, V2ScalarProgram, v2_authority,
    v2_catalog::BoundCatalog,
    v2_composition::{self as c, Descriptor},
    v2_laws as laws,
};

/// One inductive claim of a contract, compiled to a law program over the
/// state: it reads the successor's fields, which a genesis frame binds to
/// the state itself. `zeno-fcis generate contract` compiles each inductive
/// claim of a contract that a behaviour change or migration leads to.
#[derive(Clone, Copy, Debug)]
pub struct Claim<'s> {
    /// The claim's ID in `project.zeno`; it must differ from every law ID.
    pub id: u32,
    /// The program's nodes, in the library's law language.
    pub nodes: &'s [laws::Op<'s>],
    /// The node holding the claim's Boolean value.
    pub root: usize,
}

/// Why the new contract does not admit the store's current state as a
/// behaviour change. A refusal writes nothing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Unmet {
    /// This state law of the new contract does not hold on the state.
    Law {
        /// The law's ID.
        id: u32,
        /// The library's verdict: false, undefined or out of budget.
        failure: laws::Failure,
    },
    /// This claim does not hold on the state.
    Claim {
        /// The claim's ID.
        id: u32,
        /// The library's verdict.
        failure: laws::Failure,
    },
    /// The laws could not be evaluated on the state: the core refused the
    /// descriptor, for instance for a claim whose ID is a law's, or the
    /// state under the new contract's schema and framing.
    Unevaluable,
}

impl fmt::Display for Unmet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let verdict = |failure: &laws::Failure| match failure {
            laws::Failure::Violated => "is false",
            laws::Failure::Undefined => "has no value",
            laws::Failure::Budget(_) => "exceeds the evaluation budget",
            _ => "cannot be evaluated",
        };
        match self {
            Self::Law { id, failure } => write!(
                f,
                "state law {id} of the new contract {} on the store's current state",
                verdict(failure)
            ),
            Self::Claim { id, failure } => write!(
                f,
                "inductive claim {id} of the new contract {} on the store's current state",
                verdict(failure)
            ),
            Self::Unevaluable => f.write_str(
                "the new contract's laws and claims cannot be evaluated on the store's current state",
            ),
        }
    }
}

/// What a state admission evaluated, in the new contract's law
/// order and then the claims' declared order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Checked {
    laws: Vec<u32>,
    claims: Vec<u32>,
    unevaluated: Vec<u32>,
    claims_binding: Option<Hash32>,
}

impl Checked {
    /// The canonical policy of this exact state check, including each claim
    /// program, its ID and root. Absent when there are no auxiliary claims.
    /// This digest describes the checked claims; it grants no publication or
    /// upgrade authority and exposes no mutable state.
    pub fn claims_binding(&self) -> Option<Hash32> {
        self.claims_binding
    }

    /// The state laws that held on the state.
    pub fn laws(&self) -> &[u32] {
        &self.laws
    }

    /// The claims that held on the state.
    pub fn claims(&self) -> &[u32] {
        &self.claims
    }

    /// The laws that apply at genesis but were not evaluated, because they
    /// are not state laws: genesis exactness, law 990, among them.
    pub fn unevaluated(&self) -> &[u32] {
        &self.unevaluated
    }
}

/// The program every law outside the check is replaced with.
const TRUE: &[laws::Op<'static>] = &[laws::Op::Literal(laws::Atom::Bool(true))];

/// A state law: it applies at genesis and to every committing decision, and
/// it is not an `InitialCondition` law, which applies at genesis only.
fn is_state_law(law: &laws::Law<'_>) -> bool {
    law.genesis
        && matches!(law.scope, laws::Scope::Committing | laws::Scope::Always)
        && !matches!(law.kind, laws::Kind::InitialCondition)
}

/// Checks the new contract `to`'s state laws and `claims` on `state`, the
/// store's exact current state bytes; see the module documentation.
///
/// # Errors
/// The first law or claim that does not hold, or `Unevaluable`.
pub(super) fn check(
    to: &BoundCatalog<'_>,
    claims: &[Claim<'_>],
    state: &[u8],
) -> Result<Checked, Unmet> {
    let d = to.descriptor();
    let mut evaluated = Vec::with_capacity(d.laws.len() + claims.len());
    let (mut state_laws, mut unevaluated) = (Vec::new(), Vec::new());
    for law in d.laws {
        let kept = is_state_law(law);
        if kept {
            state_laws.push(law.id);
        } else if law.genesis {
            unevaluated.push(law.id);
        }
        evaluated.push(laws::Law {
            id: law.id,
            kind: law.kind,
            scope: law.scope,
            genesis: law.genesis,
            program: if kept {
                laws::Program {
                    nodes: law.program.nodes,
                    root: law.program.root,
                }
            } else {
                laws::Program {
                    nodes: TRUE,
                    root: 0,
                }
            },
        });
    }
    let mut extra: u64 = 0;
    for claim in claims {
        extra = extra.saturating_add(u64::try_from(claim.nodes.len()).unwrap_or(u64::MAX));
        evaluated.push(laws::Law {
            id: claim.id,
            kind: laws::Kind::StateInvariant,
            scope: laws::Scope::Committing,
            genesis: true,
            program: laws::Program {
                nodes: claim.nodes,
                root: claim.root,
            },
        });
    }
    let raised = |resource| d.limits.limit(resource).saturating_add(extra);
    let descriptor = Descriptor {
        state: d.state,
        command: d.command,
        context: d.context,
        program: V2ScalarProgram {
            inputs: d.program.inputs,
            outputs: d.program.outputs,
            nodes: d.program.nodes,
            roots: d.program.roots,
        },
        bindings: d.bindings,
        output_types: d.output_types,
        decision_output: d.decision_output,
        branches: d.branches,
        reasons: d.reasons,
        channels: d.channels,
        laws: &evaluated,
        required: d.required,
        limits: d
            .limits
            .with_limit(V2Resource::Read, raised(V2Resource::Read))
            .with_limit(V2Resource::Step, raised(V2Resource::Step)),
    };
    let core = c::bind(&descriptor).map_err(|_| Unmet::Unevaluable)?;
    let outcome = core.frame(
        c::Kind::Genesis,
        c::Raw {
            state,
            command: &[],
            context: &[],
        },
        to.framing(),
    );
    let is_claim = |id: u32| claims.iter().any(|claim| claim.id == id);
    for diagnostic in outcome.diagnostics() {
        if let laws::Verdict::Refused(failure) = diagnostic.verdict {
            return Err(if is_claim(diagnostic.id) {
                Unmet::Claim {
                    id: diagnostic.id,
                    failure,
                }
            } else {
                Unmet::Law {
                    id: diagnostic.id,
                    failure,
                }
            });
        }
    }
    if outcome.result().is_err() {
        return Err(Unmet::Unevaluable);
    }
    // Every state law and claim must have been evaluated and satisfied.
    let satisfied = |id: u32| {
        outcome
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.id == id && diagnostic.verdict == laws::Verdict::Satisfied)
    };
    if !state_laws.iter().all(|id| satisfied(*id))
        || !claims.iter().all(|claim| satisfied(claim.id))
    {
        return Err(Unmet::Unevaluable);
    }
    let claims_binding = if claims.is_empty() {
        None
    } else {
        let policy = v2_authority::policy_bytes(
            &descriptor,
            to.original_schema(),
            to.framing(),
            to.channel_roots(),
        )
        .ok_or(Unmet::Unevaluable)?;
        Some(RustCryptoSha256::hash(&policy))
    };
    Ok(Checked {
        laws: state_laws,
        claims: claims.iter().map(|claim| claim.id).collect(),
        unevaluated,
        claims_binding,
    })
}
