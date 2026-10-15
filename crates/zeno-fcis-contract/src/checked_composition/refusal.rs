//! Typed refusals. A checker verdict is a [`Stage`], an [`Obligation`] and,
//! where the Authority refused, an [`AuthorityRefusal`] read from the
//! library's refusal values by their variants, never from any rendering;
//! where `zeno-fcis-compose` decided, its own [`ComposeFinding`]. The place
//! and reason are explanations for a reader; nothing decides on them.

use serde_json::{Value, json};
use zeno_fcis_compose::{ComponentId, CompositionBlocker, ContractError as ComposeError};
use zeno_fcis_synthesis::finite::{
    V2ExecutionFailure, V2Resource, v2_authority, v2_composition as c, v2_laws as l,
};

/// The stage of the checker that refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Stage {
    /// The manifest's closed form.
    Manifest,
    /// A leaf's files, pins and contract route.
    Binding,
    /// Ownership and footprints.
    Ownership,
    /// A leaf's checks on its local domain.
    Local,
    /// Interface edges and their implications.
    Interface,
    /// The aggregate contract against the composition.
    Assembly,
    /// The group tree and the checks of equation 2.8.
    Hierarchy,
}

impl Stage {
    /// The stage's name in a result.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Manifest => "manifest",
            Self::Binding => "binding",
            Self::Ownership => "ownership",
            Self::Local => "local",
            Self::Interface => "interface",
            Self::Assembly => "assembly",
            Self::Hierarchy => "hierarchy",
        }
    }
}

/// The obligation that failed, or the boundary that refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Obligation {
    /// The manifest is not in its closed form.
    ClosedManifestForm,
    /// A leaf file differs from its pin.
    SourceBinding,
    /// The ordinary contract route refuses a leaf.
    OrdinaryContractRoute,
    /// A leaf is outside the profile.
    CheckedProfile,
    /// A predicate does not compile or has no value.
    DefinedPredicate,
    /// The bound program differs from its pin.
    ProgramBinding,
    /// A domain is too large to enumerate completely.
    CompleteEnumeration,
    /// A read or write is outside the declared footprint.
    CompleteFootprint,
    /// Two leaves own one state slot.
    DisjointOwnership,
    /// The state slots are not `0..n` without a gap.
    CompleteLayout,
    /// The genesis state does not satisfy the invariant.
    InitImpliesInvariant,
    /// The Authority refuses the declared genesis.
    GenuineGenesis,
    /// A publication does not replay to the same subject.
    ExactReplay,
    /// The Authority refuses an input that satisfies the invariant and the
    /// assumption.
    NoTechnicalRefusal,
    /// The Authority returned an outcome the checker does not know.
    KnownOutcome,
    /// A decision delivers an effect or outbox entry.
    NoEffectsOrOutbox,
    /// An Accept changes a field outside the declared writes.
    Frame,
    /// An admitted Accept leaves the invariant.
    Preservation,
    /// An admitted Reject is not an unchanged business rejection.
    UnchangedBusinessReject,
    /// An admitted input commits a failure.
    CommittedFailureUnreachable,
    /// A transition relation fails on an admitted decision.
    TransitionRelation,
    /// A listed decision class occurs on no admitted input.
    RequiredWitness,
    /// A StateFact does not follow from the invariant.
    StateFact,
    /// An interface field has no provider, or more than one, or a
    /// provider that is not another listed leaf.
    ProviderBinding,
    /// A view is not one exact typed state field of its provider.
    TypedView,
    /// A fact is not one of the provider's StateFacts.
    ProviderClaim,
    /// The providers' facts hold and the consumer's assumption fails.
    InterfaceImplication,
    /// The aggregate is not an ordinary contract in the assembly profile.
    AssemblyProfile,
    /// The aggregate's state fields are not the slots with their domains.
    StateProjection,
    /// The aggregate's command does not route to exactly one leaf command.
    CommandRouting,
    /// An aggregate context field is not an environment input.
    EnvironmentInputs,
    /// The aggregate's genesis is not the leaves' genesis.
    AggregateGenesis,
    /// On some route the aggregate does not decide as the selected leaf.
    DecisionCorrespondence,
    /// On some route the aggregate writes a slot of an unselected leaf.
    FrameUnselectedSlots,
    /// A leaf's peer view is not read from the provider's slot in the same
    /// pre-state.
    SamePreStateView,
    /// An aggregate program node can trap on an input within its domains.
    EagerNodesTotal,
    /// The aggregate's Step budget does not cover its eager work.
    MeterCoversEagerWork,
    /// An aggregate law is not implied by the leaves' invariants.
    AggregateLaws,
    /// The groups are not one tree over every leaf.
    GroupTree,
    /// A group's inputs are not its environment inputs, or an export is not
    /// a view under it.
    GroupInterface,
    /// A group formula does not compile against the group's fields.
    GroupFormula,
    /// A child's assumption is neither discharged by the inner guarantees
    /// and the group's outer assumption nor able to rise, or the root's
    /// assumption holds on no input.
    OuterAssumption,
    /// The children's guarantees do not imply a group's outer guarantee.
    OuterGuarantee,
    /// Two leaves declare one command field ID with different types.
    SharedCommandField,
}

impl Obligation {
    /// The obligation's name in a result.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::ClosedManifestForm => "closed manifest form",
            Self::SourceBinding => "source binding",
            Self::OrdinaryContractRoute => "ordinary contract route",
            Self::CheckedProfile => "checked profile",
            Self::DefinedPredicate => "defined predicate",
            Self::ProgramBinding => "program binding",
            Self::CompleteEnumeration => "complete enumeration",
            Self::CompleteFootprint => "complete footprint",
            Self::DisjointOwnership => "disjoint ownership",
            Self::CompleteLayout => "complete layout",
            Self::InitImpliesInvariant => "Init => I",
            Self::GenuineGenesis => "genuine genesis",
            Self::ExactReplay => "exact replay",
            Self::NoTechnicalRefusal => "no technical refusal on admitted inputs",
            Self::KnownOutcome => "known outcome",
            Self::NoEffectsOrOutbox => "no effects or outbox",
            Self::Frame => "frame",
            Self::Preservation => "I and A => I'",
            Self::UnchangedBusinessReject => "unchanged business Reject",
            Self::CommittedFailureUnreachable => "CommittedFailure unreachable",
            Self::TransitionRelation => "I and A => G",
            Self::RequiredWitness => "required witness",
            Self::StateFact => "I => G",
            Self::ProviderBinding => "provider binding",
            Self::TypedView => "typed view",
            Self::ProviderClaim => "provider claim",
            Self::InterfaceImplication => "provider facts => consumer assumption",
            Self::AssemblyProfile => "assembly profile",
            Self::StateProjection => "state projection",
            Self::CommandRouting => "command routing",
            Self::EnvironmentInputs => "environment inputs",
            Self::AggregateGenesis => "aggregate genesis",
            Self::DecisionCorrespondence => "decision correspondence",
            Self::FrameUnselectedSlots => "unselected slots unchanged",
            Self::SamePreStateView => "views from the same pre-state",
            Self::EagerNodesTotal => "eager nodes total",
            Self::MeterCoversEagerWork => "meter covers eager work",
            Self::AggregateLaws => "aggregate laws implied by invariants",
            Self::GroupTree => "group tree",
            Self::GroupInterface => "group interface",
            Self::GroupFormula => "group formula",
            Self::OuterAssumption => "outer assumption discharges inner assumptions",
            Self::OuterGuarantee => "outer guarantee implied by inner guarantees",
            Self::SharedCommandField => "command fields shared by ID with one type",
        }
    }
}

/// A shared-meter refusal: the resource whose charge was refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MeterRefusal {
    /// The refused resource.
    pub resource: V2Resource,
}

/// Why a law program had no value or refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum LawRefusal {
    /// Invalid definitions, required IDs, scopes or graph shape.
    Metadata,
    /// A malformed frame.
    Frame,
    /// A missing observation, a type error or an arithmetic trap.
    Undefined,
    /// The predicate evaluated to false.
    Violated,
    /// The shared meter refused.
    Meter(MeterRefusal),
    /// A refusal this checker does not know.
    Other,
}

/// Why the scalar decision program refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ExecutionRefusal {
    /// An input is outside its declared domain.
    InputDomain,
    /// An unavailable scalar was referenced.
    Reference,
    /// Checked arithmetic overflowed.
    Arithmetic,
    /// An output is outside its declared domain.
    OutputDomain,
    /// The shared meter refused.
    Meter(MeterRefusal),
    /// A refusal this checker does not know.
    Other,
}

/// The Authority's technical refusal of an input, by kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum AuthorityRefusal {
    /// Descriptor admission failed.
    Metadata,
    /// Typed ingress refused a source (0 state, 1 command, 2 context).
    Ingress(u8),
    /// The input ABI could not be constructed.
    Binding,
    /// The decision program refused.
    Execution(ExecutionRefusal),
    /// Outputs could not be reified into their declared types.
    Output,
    /// Decision construction refused.
    Decision,
    /// A candidate left a declared value domain.
    Schema,
    /// A law refused; `law` is the first refusing law the library reports.
    Law {
        /// The refusing law's ID.
        law: Option<u32>,
        /// How it refused.
        refusal: LawRefusal,
    },
    /// Envelope framing refused a source.
    Frame(u8),
    /// Canonical serialization exceeded its length.
    Encoding,
    /// A recomputed subject differed from the expected bytes.
    ReplayMismatch,
    /// Original-wire output encoding refused.
    WireOutput,
    /// An emitted channel has no bound roots.
    Channel,
    /// A refusal this checker does not know.
    Other,
}

impl AuthorityRefusal {
    /// The kind of `refusal`, with the first refusing law of `diagnostics`.
    #[must_use]
    pub(crate) fn of(refusal: &v2_authority::Refusal, diagnostics: &[l::Diagnostic]) -> Self {
        let law = diagnostics
            .iter()
            .find(|diagnostic| matches!(diagnostic.verdict, l::Verdict::Refused(_)))
            .map(|diagnostic| diagnostic.id);
        match refusal {
            v2_authority::Refusal::Core(failure) => match failure {
                c::Failure::Metadata => Self::Metadata,
                c::Failure::Ingress(source, _) => Self::Ingress(*source),
                c::Failure::Binding => Self::Binding,
                c::Failure::Execution(failure) => Self::Execution(execution(*failure)),
                c::Failure::Output => Self::Output,
                c::Failure::Decision(_) => Self::Decision,
                c::Failure::Schema => Self::Schema,
                c::Failure::Law(failure) => Self::Law {
                    law,
                    refusal: law_refusal(*failure),
                },
                c::Failure::Frame(source, _) => Self::Frame(*source),
                _ => Self::Other,
            },
            v2_authority::Refusal::Encoding => Self::Encoding,
            v2_authority::Refusal::ReplayMismatch => Self::ReplayMismatch,
            v2_authority::Refusal::Output(_) => Self::WireOutput,
            v2_authority::Refusal::Channel => Self::Channel,
            _ => Self::Other,
        }
    }

    /// Stable bytes for a checked table: a tag and its fields.
    pub(crate) fn encode(self, out: &mut Vec<u8>) {
        let meter = |out: &mut Vec<u8>, meter: MeterRefusal| {
            out.push(u8::try_from(meter.resource.index()).unwrap_or(u8::MAX));
        };
        let law = |out: &mut Vec<u8>, refusal: LawRefusal| match refusal {
            LawRefusal::Metadata => out.push(0),
            LawRefusal::Frame => out.push(1),
            LawRefusal::Undefined => out.push(2),
            LawRefusal::Violated => out.push(3),
            LawRefusal::Meter(value) => {
                out.push(4);
                meter(out, value);
            }
            LawRefusal::Other => out.push(255),
        };
        match self {
            Self::Metadata => out.push(0),
            Self::Ingress(source) => out.extend_from_slice(&[1, source]),
            Self::Binding => out.push(2),
            Self::Execution(refusal) => {
                out.push(3);
                match refusal {
                    ExecutionRefusal::InputDomain => out.push(0),
                    ExecutionRefusal::Reference => out.push(1),
                    ExecutionRefusal::Arithmetic => out.push(2),
                    ExecutionRefusal::OutputDomain => out.push(3),
                    ExecutionRefusal::Meter(value) => {
                        out.push(4);
                        meter(out, value);
                    }
                    ExecutionRefusal::Other => out.push(255),
                }
            }
            Self::Output => out.push(4),
            Self::Decision => out.push(5),
            Self::Schema => out.push(6),
            Self::Law { law: id, refusal } => {
                out.push(7);
                out.extend_from_slice(&id.unwrap_or(0).to_be_bytes());
                law(out, refusal);
            }
            Self::Frame(source) => out.extend_from_slice(&[8, source]),
            Self::Encoding => out.push(9),
            Self::ReplayMismatch => out.push(10),
            Self::WireOutput => out.push(11),
            Self::Channel => out.push(12),
            Self::Other => out.push(255),
        }
    }

    /// The refusal in a result, named by its variants.
    #[must_use]
    pub fn json(self) -> Value {
        match self {
            Self::Metadata => json!({"kind": "metadata"}),
            Self::Ingress(source) => json!({"kind": "ingress", "source": source}),
            Self::Binding => json!({"kind": "binding"}),
            Self::Execution(refusal) => json!({"kind": "execution", "refusal": refusal.json()}),
            Self::Output => json!({"kind": "output"}),
            Self::Decision => json!({"kind": "decision"}),
            Self::Schema => json!({"kind": "schema"}),
            Self::Law { law, refusal } => {
                json!({"kind": "law", "law": law, "refusal": refusal.json()})
            }
            Self::Frame(source) => json!({"kind": "frame", "source": source}),
            Self::Encoding => json!({"kind": "encoding"}),
            Self::ReplayMismatch => json!({"kind": "replay-mismatch"}),
            Self::WireOutput => json!({"kind": "wire-output"}),
            Self::Channel => json!({"kind": "channel"}),
            Self::Other => json!({"kind": "other"}),
        }
    }
}

impl MeterRefusal {
    /// The refused resource's name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self.resource {
            V2Resource::Read => "read",
            V2Resource::Write => "write",
            V2Resource::Candidate => "candidate",
            V2Resource::Effect => "effect",
            V2Resource::Byte => "byte",
            V2Resource::WitnessByte => "witness-byte",
            V2Resource::Depth => "depth",
            V2Resource::Step => "step",
            _ => "other",
        }
    }
}

impl LawRefusal {
    /// What the refusal means, for a reader.
    #[must_use]
    pub fn describe(self) -> String {
        match self {
            Self::Metadata => "invalid law definitions".to_owned(),
            Self::Frame => "a malformed frame".to_owned(),
            Self::Undefined => {
                "a missing observation, a type error or an arithmetic trap".to_owned()
            }
            Self::Violated => "the predicate is false".to_owned(),
            Self::Meter(meter) => format!("the shared meter refused a {} charge", meter.name()),
            Self::Other => "a refusal this checker does not know".to_owned(),
        }
    }

    fn json(self) -> Value {
        match self {
            Self::Metadata => json!("metadata"),
            Self::Frame => json!("frame"),
            Self::Undefined => json!("undefined"),
            Self::Violated => json!("violated"),
            Self::Meter(meter) => json!({"meter": meter.name()}),
            Self::Other => json!("other"),
        }
    }
}

impl ExecutionRefusal {
    fn json(self) -> Value {
        match self {
            Self::InputDomain => json!("input-domain"),
            Self::Reference => json!("reference"),
            Self::Arithmetic => json!("arithmetic"),
            Self::OutputDomain => json!("output-domain"),
            Self::Meter(meter) => json!({"meter": meter.name()}),
            Self::Other => json!("other"),
        }
    }
}

pub(crate) fn law_refusal(failure: l::Failure) -> LawRefusal {
    match failure {
        l::Failure::Metadata => LawRefusal::Metadata,
        l::Failure::Frame => LawRefusal::Frame,
        l::Failure::Undefined => LawRefusal::Undefined,
        l::Failure::Violated => LawRefusal::Violated,
        l::Failure::Budget(failure) => LawRefusal::Meter(MeterRefusal {
            resource: failure.resource,
        }),
        _ => LawRefusal::Other,
    }
}

fn execution(failure: V2ExecutionFailure) -> ExecutionRefusal {
    match failure {
        V2ExecutionFailure::InputDomain => ExecutionRefusal::InputDomain,
        V2ExecutionFailure::Reference => ExecutionRefusal::Reference,
        V2ExecutionFailure::Arithmetic => ExecutionRefusal::Arithmetic,
        V2ExecutionFailure::OutputDomain => ExecutionRefusal::OutputDomain,
        V2ExecutionFailure::Budget(failure) => ExecutionRefusal::Meter(MeterRefusal {
            resource: failure.resource,
        }),
        _ => ExecutionRefusal::Other,
    }
}

/// What `zeno-fcis-compose` reported on the specification a manifest
/// denotes, where its verdict decided the refusal.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ComposeFinding {
    /// A compose constructor refused part of the specification.
    Constructor(ComposeError),
    /// `verify_assume_guarantee` reported this blocker first.
    Blocker(CompositionBlocker),
    /// `conflicts` found a write-write conflict between two components.
    WriteWrite {
        /// The component with the lower ID.
        left: ComponentId,
        /// The component with the higher ID.
        right: ComponentId,
    },
}

impl ComposeFinding {
    /// The finding as JSON, named by its variants.
    #[must_use]
    pub fn json(&self) -> Value {
        match self {
            Self::Constructor(error) => json!({"constructor": constructor_name(error)}),
            Self::Blocker(blocker) => blocker_json(blocker),
            Self::WriteWrite { left, right } => {
                json!({"conflict": "write-write", "left": left.get(), "right": right.get()})
            }
        }
    }
}

fn constructor_name(error: &ComposeError) -> &'static str {
    match error {
        ComposeError::ZeroIdentifier => "zero-identifier",
        ComposeError::ZeroHash => "zero-hash",
        ComposeError::PathTooDeep => "path-too-deep",
        ComposeError::NonTerminalWildcard => "non-terminal-wildcard",
        ComposeError::PathSetTooLarge => "path-set-too-large",
        ComposeError::DuplicatePath => "duplicate-path",
        ComposeError::ComponentCardinality => "component-cardinality",
        ComposeError::DuplicateComponent => "duplicate-component",
        ComposeError::DuplicateClaim => "duplicate-claim",
        ComposeError::TooManyClaims => "too-many-claims",
        ComposeError::InvalidMergeOrder => "invalid-merge-order",
        ComposeError::DuplicateWiring => "duplicate-wiring",
        ComposeError::UnknownComponent => "unknown-component",
        ComposeError::InvalidAssumptionDischarge => "invalid-assumption-discharge",
        ComposeError::DuplicateEvidence => "duplicate-evidence",
        ComposeError::Encode(_) => "encode",
        _ => "other",
    }
}

fn blocker_json(blocker: &CompositionBlocker) -> Value {
    let hex = super::hex;
    match blocker {
        CompositionBlocker::MissingAssumptionDischarge { component, claim } => json!({
            "blocker": "missing-assumption-discharge",
            "component": component.get(),
            "claim": hex(claim.as_bytes()),
        }),
        CompositionBlocker::UnknownProviderGuarantee {
            assumption,
            component,
            guarantee,
        } => json!({
            "blocker": "unknown-provider-guarantee",
            "assumption": hex(assumption.as_bytes()),
            "component": component.get(),
            "guarantee": hex(guarantee.as_bytes()),
        }),
        CompositionBlocker::MissingGuaranteeEvidence { component, claim } => json!({
            "blocker": "missing-guarantee-evidence",
            "component": component.get(),
            "claim": hex(claim.as_bytes()),
        }),
        CompositionBlocker::CompositionIdentityFailure => {
            json!({"blocker": "composition-identity-failure"})
        }
        _ => json!({"blocker": "other"}),
    }
}

/// Why a composition was not checked: the stage, the obligation that failed
/// or the boundary that refused, the Authority's or compose's finding where
/// one decided it, where, why, and the input that shows it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Refusal {
    stage: Stage,
    obligation: Obligation,
    authority: Option<AuthorityRefusal>,
    compose: Option<Box<ComposeFinding>>,
    place: String,
    reason: String,
    witness: Option<Value>,
}

impl Refusal {
    pub(crate) fn new(
        stage: Stage,
        obligation: Obligation,
        place: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            stage,
            obligation,
            authority: None,
            compose: None,
            place: place.into(),
            reason: reason.into(),
            witness: None,
        }
    }

    pub(crate) fn with_witness(mut self, witness: Value) -> Self {
        self.witness = Some(witness);
        self
    }

    pub(crate) fn with_authority(mut self, authority: AuthorityRefusal) -> Self {
        self.authority = Some(authority);
        self
    }

    pub(crate) fn with_compose(mut self, finding: ComposeFinding) -> Self {
        self.compose = Some(Box::new(finding));
        self
    }

    /// The stage that refused.
    #[must_use]
    pub const fn stage(&self) -> Stage {
        self.stage
    }

    /// The obligation that failed, or the boundary that refused.
    #[must_use]
    pub const fn obligation(&self) -> Obligation {
        self.obligation
    }

    /// The Authority's refusal, when an Authority refusal decided this one.
    #[must_use]
    pub const fn authority(&self) -> Option<AuthorityRefusal> {
        self.authority
    }

    /// Compose's finding, when compose's verdict decided this refusal.
    #[must_use]
    pub fn compose(&self) -> Option<&ComposeFinding> {
        self.compose.as_deref()
    }

    /// The leaf, edge or manifest entry the refusal is about, for a reader.
    #[must_use]
    pub fn place(&self) -> &str {
        &self.place
    }

    /// Why, for a reader.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// The input that shows it, when there is one.
    #[must_use]
    pub const fn witness(&self) -> Option<&Value> {
        self.witness.as_ref()
    }

    /// The refusal as JSON.
    #[must_use]
    pub fn json(&self) -> Value {
        json!({
            "stage": self.stage.name(),
            "obligation": self.obligation.name(),
            "authority": self.authority.map(AuthorityRefusal::json),
            "compose": self.compose.as_deref().map(ComposeFinding::json),
            "place": self.place,
            "reason": self.reason,
            "witness": self.witness,
        })
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} ({}): {}: {}",
            self.obligation.name(),
            self.stage.name(),
            self.place,
            self.reason
        )
    }
}

impl std::error::Error for Refusal {}
