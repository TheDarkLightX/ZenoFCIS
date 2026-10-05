//! Immutable law inputs over the one shared decision value model.
//! Producer/catalog correspondence is a separate relation.
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

pub use super::super::composition::{ReadAttempt as TraceRead, Selector};
pub use super::super::decision::{Atom, Attempt, Class, Delivery, Field, Patch, RootView};

/// Complete candidate observations borrowed from one immutable decision.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub struct Candidate<'a> {
    /// Exact decision class.
    pub class: Class,
    /// Absent for Accept; nonzero stable ID otherwise.
    pub reason: Option<u32>,
    /// Complete successor; empty for Reject.
    pub post: RootView<'a, 'a>,
    /// Complete canonical changed-field plan.
    pub patch: &'a [Patch<'a>],
    /// Complete effects in declared order.
    pub effects: &'a [Delivery<'a>],
    /// Complete outbox in declared order.
    pub outbox: &'a [Delivery<'a>],
    /// Actual ingress read attempts, including denied ones, in interpretation order.
    pub reads: &'a [TraceRead],
    /// Actual candidate, write, effect and outbox attempts in order.
    pub attempts: &'a [Attempt],
    /// Opaque actual usage measured before laws; final usage remains on the shared meter.
    pub usage: super::super::Usage,
}
/// A transition frame or the actual initial-state record, without fixtures.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
#[non_exhaustive]
pub enum Frame<'a> {
    /// An exact invocation and complete decision.
    Transition {
        /// Complete original pre-state.
        pre: RootView<'a, 'a>,
        /// Complete original command.
        command: RootView<'a, 'a>,
        /// Complete original context.
        context: RootView<'a, 'a>,
        /// Exact complete decision observation.
        candidate: &'a Candidate<'a>,
    },
    /// Only the actual initial state.
    Genesis {
        /// Actual initial-state record.
        initial: RootView<'a, 'a>,
    },
}
/// A closed observation selector. Missing or unavailable observations refuse.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Observation {
    /// Exact scalar pre-state root; unavailable for records.
    PreRoot,
    /// Exact scalar command root; unavailable for records.
    CommandRoot,
    /// Exact scalar context root; unavailable for records.
    ContextRoot,
    /// Exact scalar successor root; unavailable for records.
    PostRoot,
    /// Exact scalar initial root; unavailable outside genesis or for records.
    InitialRoot,
    /// Observe pre.
    Pre(u16),
    /// Observe command.
    Command(u16),
    /// Observe context.
    Context(u16),
    /// Observe post.
    Post(u16),
    /// Observe initial.
    Initial(u16),
    /// Observe class.
    Class,
    /// Observe hasreason.
    HasReason,
    /// Observe reason.
    Reason,
    /// Observe post length.
    PostLength,
    /// Observe patch length.
    PatchLength,
    /// Observe effect length.
    EffectLength,
    /// Observe outbox length.
    OutboxLength,
    /// Observe patchfield.
    PatchField(usize),
    /// Observe patchbefore.
    PatchBefore(usize),
    /// Observe patchafter.
    PatchAfter(usize),
    /// Observe effectordinal.
    EffectOrdinal(usize),
    /// Observe effectchannel.
    EffectChannel(usize),
    /// Observe effectdestination.
    EffectDestination(usize),
    /// Observe effect payload.
    EffectPayload(usize, u16),
    /// Observe effect identifierempotency.
    EffectIdempotency(usize),
    /// Observe outboxordinal.
    OutboxOrdinal(usize),
    /// Observe outboxchannel.
    OutboxChannel(usize),
    /// Observe outboxdestination.
    OutboxDestination(usize),
    /// Observe outbox payload.
    OutboxPayload(usize, u16),
    /// Observe outbox identifierempotency.
    OutboxIdempotency(usize),
    /// Observe read length.
    ReadLength,
    /// Observe write length.
    WriteLength,
    /// Observe effectattempt length.
    EffectAttemptLength,
    /// Observe read source.
    ReadSource(usize),
    /// Observe read identifier.
    ReadId(usize),
    /// Observe read permission.
    ReadPermitted(usize),
    /// Observe write source.
    WriteSource(usize),
    /// Observe write identifier.
    WriteId(usize),
    /// Observe write permission.
    WritePermitted(usize),
    /// Observe effectattempt source.
    EffectAttemptSource(usize),
    /// Observe effectattempt identifier.
    EffectAttemptId(usize),
    /// Observe effectattempt permission.
    EffectAttemptPermitted(usize),
    /// Observe usage.
    Usage(super::super::Resource),
}
/// Closed eager predicate language; every earlier node is evaluated.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum Op<'a> {
    /// A typed literal.
    Literal(Atom<'a>),
    /// Read one exact frame observation.
    Observe(Observation),
    /// Read only when a strictly prior node is Bool(true); Bool(false) returns
    /// the exact reviewed default without a Read charge or frame access.
    /// Invalid guards and active observation failures never use the default.
    ObserveWhen(usize, Observation, Atom<'a>),
    /// Checked same-type integer addition.
    Add(usize, usize),
    /// Checked same-type integer subtraction.
    Sub(usize, usize),
    /// Checked same-type integer multiplication.
    Mul(usize, usize),
    /// Checked same-type integer division with declared rounding.
    Div(Division, usize, usize),
    /// Explicit numeric projection of Bool, enum/sum variant, or representable integer.
    ToI128(usize),
    /// Exact typed equality.
    Eq(usize, usize),
    /// Same-type integer ordering.
    Lt(usize, usize),
    /// Boolean conjunction; both operands are already evaluated.
    And(usize, usize),
    /// Boolean negation.
    Not(usize),
    /// Typed selection; both branches are already evaluated.
    Select(usize, usize, usize),
}
/// Complete predicate, with no callbacks or verdict fields.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub struct Program<'a> {
    /// All eager nodes in declared order.
    pub nodes: &'a [Op<'a>],
    /// Predicate result node; it must evaluate to Bool.
    pub root: usize,
}
/// Every supported law family, including V2's mandatory additions.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Kind {
    /// Mandatory committing-state and genesis invariant.
    StateInvariant,
    /// Declared asset conservation predicate.
    AssetConservation,
    /// Declared mint/burn authorization predicate.
    MintBurnAuthorization,
    /// Declared debit/credit/effect correspondence.
    DebitCreditEffectEquality,
    /// Declared fees and rounding predicate.
    FeeAndRounding,
    /// Declared authority, subject and recipient relation.
    AuthoritySubjectRecipient,
    /// Mandatory rejection predicate.
    RejectNoAuthority,
    /// Mandatory committing-failure predicate.
    CommittedFailureEffects,
    /// Mandatory predicate for every decision class.
    DecisionConformance,
    /// Mandatory genesis-only predicate.
    InitialCondition,
}
/// Exact declared ordinary decision scope.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Scope {
    /// Every ordinary decision.
    Always,
    /// Successful committing decision.
    Accept,
    /// Noncommitting rejection.
    Reject,
    /// Declared committing failure.
    CommittedFailure,
    /// Accept and CommittedFailure.
    Committing,
}
/// One declared law. InitialCondition is genesis-only regardless of scope.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug)]
pub struct Law<'a> {
    /// Stable identifier.
    pub id: u32,
    /// Declared supported family.
    pub kind: Kind,
    /// Exact ordinary decision scope.
    pub scope: Scope,
    /// Whether this law must evaluate on the actual initial state.
    pub genesis: bool,
    /// Library-interpreted closed predicate.
    pub program: Program<'a>,
}
/// Refusal reason without any partial candidate or authorizing token.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Failure {
    /// Invalid definitions, required IDs, scopes or graph shape.
    Metadata,
    /// Malformed structural decision or initial-state frame.
    Frame,
    /// Missing observation, wrong operand type, arithmetic trap or invalid result.
    Undefined,
    /// Predicate evaluated to false.
    Violated,
    /// Actual shared-meter charge refusal.
    Budget(super::super::MeterFailure),
}
/// Ordered law diagnostics, including inapplicability and first refusal.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Verdict {
    /// Declared scope excludes this frame.
    Skipped,
    /// Applicable predicate evaluated to true.
    Satisfied,
    /// Applicable evaluation refused.
    Refused(Failure),
}
/// One library-computed law diagnostic.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    /// Stable identifier.
    pub id: u32,
    /// Library-computed evaluation status.
    pub verdict: Verdict,
}
/// One attempted law observation, including a denied Read charge.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadAttempt {
    /// Stable law identifier.
    pub law: u32,
    /// Attempted eager-node index.
    pub node: usize,
    /// Exact requested observation.
    pub observation: Observation,
    /// Whether the actual charge permitted this attempt.
    pub permitted: bool,
}

/// Exact, floor or ceiling checked integer division.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Division {
    /// Require a zero remainder.
    Exact,
    /// Round toward negative infinity.
    Floor,
    /// Round toward positive infinity.
    Ceil,
}
