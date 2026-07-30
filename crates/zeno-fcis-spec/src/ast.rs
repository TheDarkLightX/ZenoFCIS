//! Typed authoring AST and canonical identity.

use alloc::boxed::Box;
use alloc::vec::Vec;

use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher, Domain, EncodeError, Hash32, commitment};

use crate::{PROJECT_SPEC_FORMAT_VERSION, SourceSpan};

/// Nonzero stable identifier allocated explicitly by source or a builder.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StableId(u32);

impl StableId {
    /// Creates a nonzero stable identifier.
    pub const fn new(value: u32) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }
    /// Returns the numeric identifier.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl CanonicalEncode for StableId {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.0.to_be_bytes());
        Ok(())
    }
}

/// Owned ASCII identifier.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Identifier(Box<str>);

impl Identifier {
    /// Creates an identifier after checking the lexical identifier grammar.
    pub fn try_new(value: impl Into<Box<str>>) -> Option<Self> {
        let value = value.into();
        let mut bytes = value.bytes();
        let first = bytes.next()?;
        if !(first.is_ascii_alphabetic() || first == b'_')
            || !bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
            || value.len() > 128
        {
            return None;
        }
        Some(Self(value))
    }
    /// Returns the identifier text.
    #[must_use]
    pub const fn as_str(&self) -> &str {
        &self.0
    }
}

impl CanonicalEncode for Identifier {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        put_blob(output, self.0.as_bytes())
    }
}

/// Semantic role of a declared type.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum TypeKind {
    State,
    Command,
    Context,
    Effect,
    Destination,
    Payload,
    Data,
    Bool,
    Int,
}

impl TypeKind {
    pub(crate) const fn tag(self) -> u8 {
        match self {
            Self::State => 0,
            Self::Command => 1,
            Self::Context => 2,
            Self::Effect => 3,
            Self::Destination => 4,
            Self::Payload => 5,
            Self::Data => 6,
            Self::Bool => 7,
            Self::Int => 8,
        }
    }
}

/// One explicit type declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeDecl {
    pub(crate) id: StableId,
    pub(crate) kind: TypeKind,
    pub(crate) name: Identifier,
}
impl TypeDecl {
    /// Creates a type declaration.
    #[must_use]
    pub const fn new(id: StableId, kind: TypeKind, name: Identifier) -> Self {
        Self { id, kind, name }
    }
    /// Returns the stable identifier.
    #[must_use]
    pub const fn id(&self) -> StableId {
        self.id
    }
    /// Returns the semantic kind.
    #[must_use]
    pub const fn kind(&self) -> TypeKind {
        self.kind
    }
    /// Returns the stable name.
    #[must_use]
    pub const fn name(&self) -> &Identifier {
        &self.name
    }
}

/// One explicit record-field declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldDecl {
    pub(crate) id: StableId,
    pub(crate) owner: StableId,
    pub(crate) name: Identifier,
    pub(crate) field_type: StableId,
}
impl FieldDecl {
    /// Creates a field declaration.
    #[must_use]
    pub const fn new(
        id: StableId,
        owner: StableId,
        name: Identifier,
        field_type: StableId,
    ) -> Self {
        Self {
            id,
            owner,
            name,
            field_type,
        }
    }
    /// Returns the field identifier.
    #[must_use]
    pub const fn id(&self) -> StableId {
        self.id
    }
    /// Returns the owning type.
    #[must_use]
    pub const fn owner(&self) -> StableId {
        self.owner
    }
    /// Returns the stable name.
    #[must_use]
    pub const fn name(&self) -> &Identifier {
        &self.name
    }
    /// Returns the field type.
    #[must_use]
    pub const fn field_type(&self) -> StableId {
        self.field_type
    }
}

/// One explicit sum-variant declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VariantDecl {
    pub(crate) id: StableId,
    pub(crate) owner: StableId,
    pub(crate) name: Identifier,
    pub(crate) payload_type: Option<StableId>,
}
impl VariantDecl {
    /// Creates a variant declaration.
    #[must_use]
    pub const fn new(
        id: StableId,
        owner: StableId,
        name: Identifier,
        payload_type: Option<StableId>,
    ) -> Self {
        Self {
            id,
            owner,
            name,
            payload_type,
        }
    }
    /// Returns the variant identifier.
    #[must_use]
    pub const fn id(&self) -> StableId {
        self.id
    }
    /// Returns the owning type.
    #[must_use]
    pub const fn owner(&self) -> StableId {
        self.owner
    }
    /// Returns the stable name.
    #[must_use]
    pub const fn name(&self) -> &Identifier {
        &self.name
    }
    /// Returns the optional payload type.
    #[must_use]
    pub const fn payload_type(&self) -> Option<StableId> {
        self.payload_type
    }
}

/// Stable rejection reason and its explicit total-precedence rank.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReasonDecl {
    pub(crate) id: StableId,
    pub(crate) name: Identifier,
    pub(crate) precedence: u32,
}
impl ReasonDecl {
    /// Creates a reason declaration.
    #[must_use]
    pub const fn new(id: StableId, name: Identifier, precedence: u32) -> Self {
        Self {
            id,
            name,
            precedence,
        }
    }
    /// Returns the reason identifier.
    #[must_use]
    pub const fn id(&self) -> StableId {
        self.id
    }
    /// Returns the stable name.
    #[must_use]
    pub const fn name(&self) -> &Identifier {
        &self.name
    }
    /// Returns the total-precedence rank.
    #[must_use]
    pub const fn precedence(&self) -> u32 {
        self.precedence
    }
}

/// Closed effect-data declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectDecl {
    pub(crate) id: StableId,
    pub(crate) name: Identifier,
    pub(crate) destination_type: StableId,
    pub(crate) payload_type: StableId,
}
impl EffectDecl {
    /// Creates an effect declaration.
    #[must_use]
    pub const fn new(
        id: StableId,
        name: Identifier,
        destination_type: StableId,
        payload_type: StableId,
    ) -> Self {
        Self {
            id,
            name,
            destination_type,
            payload_type,
        }
    }
    /// Returns the identifier.
    #[must_use]
    pub const fn id(&self) -> StableId {
        self.id
    }
    /// Returns the name.
    #[must_use]
    pub const fn name(&self) -> &Identifier {
        &self.name
    }
    /// Returns the destination type.
    #[must_use]
    pub const fn destination_type(&self) -> StableId {
        self.destination_type
    }
    /// Returns the payload type.
    #[must_use]
    pub const fn payload_type(&self) -> StableId {
        self.payload_type
    }
}

/// Typed external channel declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelDecl {
    pub(crate) id: StableId,
    pub(crate) name: Identifier,
    pub(crate) destination_type: StableId,
    pub(crate) payload_type: StableId,
}
impl ChannelDecl {
    /// Creates a channel declaration.
    #[must_use]
    pub const fn new(
        id: StableId,
        name: Identifier,
        destination_type: StableId,
        payload_type: StableId,
    ) -> Self {
        Self {
            id,
            name,
            destination_type,
            payload_type,
        }
    }
    /// Returns the identifier.
    #[must_use]
    pub const fn id(&self) -> StableId {
        self.id
    }
    /// Returns the name.
    #[must_use]
    pub const fn name(&self) -> &Identifier {
        &self.name
    }
    /// Returns the destination type.
    #[must_use]
    pub const fn destination_type(&self) -> StableId {
        self.destination_type
    }
    /// Returns the payload type.
    #[must_use]
    pub const fn payload_type(&self) -> StableId {
        self.payload_type
    }
}

/// Stable namespace declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamespaceDecl {
    pub(crate) id: StableId,
    pub(crate) name: Identifier,
}
impl NamespaceDecl {
    /// Creates a namespace declaration.
    #[must_use]
    pub const fn new(id: StableId, name: Identifier) -> Self {
        Self { id, name }
    }
    /// Returns the identifier.
    #[must_use]
    pub const fn id(&self) -> StableId {
        self.id
    }
    /// Returns the name.
    #[must_use]
    pub const fn name(&self) -> &Identifier {
        &self.name
    }
}

/// Typed port direction.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PortDirection {
    Input,
    Output,
}

/// One component port.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortDecl {
    pub(crate) id: StableId,
    pub(crate) direction: PortDirection,
    pub(crate) name: Identifier,
    pub(crate) payload_type: StableId,
}
impl PortDecl {
    /// Creates a port.
    #[must_use]
    pub const fn new(
        id: StableId,
        direction: PortDirection,
        name: Identifier,
        payload_type: StableId,
    ) -> Self {
        Self {
            id,
            direction,
            name,
            payload_type,
        }
    }
    /// Returns the identifier.
    #[must_use]
    pub const fn id(&self) -> StableId {
        self.id
    }
    /// Returns the direction.
    #[must_use]
    pub const fn direction(&self) -> PortDirection {
        self.direction
    }
    /// Returns the name.
    #[must_use]
    pub const fn name(&self) -> &Identifier {
        &self.name
    }
    /// Returns the payload type.
    #[must_use]
    pub const fn payload_type(&self) -> StableId {
        self.payload_type
    }
}

/// Root from which a logic projection reads.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ProjectionRoot {
    Pre,
    Post,
    Command,
    Context,
    Effects,
    Outbox,
    Events,
}

impl ProjectionRoot {
    pub(crate) const fn tag(self) -> u8 {
        match self {
            Self::Pre => 0,
            Self::Post => 1,
            Self::Command => 2,
            Self::Context => 3,
            Self::Effects => 4,
            Self::Outbox => 5,
            Self::Events => 6,
        }
    }
}

/// Stable-ID path projected from a typed root.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ProjectionPath {
    pub(crate) root: ProjectionRoot,
    pub(crate) segments: Box<[StableId]>,
}
impl ProjectionPath {
    /// Creates a nonempty bounded projection path.
    pub fn try_new(root: ProjectionRoot, segments: Vec<StableId>) -> Option<Self> {
        if segments.is_empty() || segments.len() > 64 {
            None
        } else {
            Some(Self {
                root,
                segments: segments.into_boxed_slice(),
            })
        }
    }
    /// Returns the root.
    #[must_use]
    pub const fn root(&self) -> ProjectionRoot {
        self.root
    }
    /// Returns stable path segments.
    #[must_use]
    pub const fn segments(&self) -> &[StableId] {
        &self.segments
    }
}

/// Component footprint kind.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum FootprintKind {
    Read,
    Write,
    Context,
    Effect,
    Outbox,
}

/// Conservative declared access footprint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FootprintDecl {
    pub(crate) kind: FootprintKind,
    pub(crate) path: ProjectionPath,
}
impl FootprintDecl {
    /// Creates a footprint declaration.
    #[must_use]
    pub const fn new(kind: FootprintKind, path: ProjectionPath) -> Self {
        Self { kind, path }
    }
    /// Returns the kind.
    #[must_use]
    pub const fn kind(&self) -> FootprintKind {
        self.kind
    }
    /// Returns the path.
    #[must_use]
    pub const fn path(&self) -> &ProjectionPath {
        &self.path
    }
}

/// Logical resource governed by a component budget.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum BudgetResource {
    Steps,
    Nodes,
    Bytes,
    PredicateCalls,
}

/// Nonzero component resource budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BudgetDecl {
    pub(crate) resource: BudgetResource,
    pub(crate) limit: u64,
}
impl BudgetDecl {
    /// Creates a nonzero budget.
    pub const fn try_new(resource: BudgetResource, limit: u64) -> Option<Self> {
        if limit == 0 {
            None
        } else {
            Some(Self { resource, limit })
        }
    }
    /// Returns the resource.
    #[must_use]
    pub const fn resource(self) -> BudgetResource {
        self.resource
    }
    /// Returns the limit.
    #[must_use]
    pub const fn limit(self) -> u64 {
        self.limit
    }
}

/// Pure domain-machine authoring declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentDecl {
    pub(crate) id: StableId,
    pub(crate) name: Identifier,
    pub(crate) owned_state: Box<[StableId]>,
    pub(crate) ports: Box<[PortDecl]>,
    pub(crate) footprints: Box<[FootprintDecl]>,
    pub(crate) budgets: Box<[BudgetDecl]>,
    pub(crate) assumptions: Box<[StableId]>,
    pub(crate) guarantees: Box<[StableId]>,
}
impl ComponentDecl {
    /// Creates one owned component declaration. Elaboration checks its references.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        id: StableId,
        name: Identifier,
        owned_state: Vec<StableId>,
        ports: Vec<PortDecl>,
        footprints: Vec<FootprintDecl>,
        budgets: Vec<BudgetDecl>,
        assumptions: Vec<StableId>,
        guarantees: Vec<StableId>,
    ) -> Self {
        Self {
            id,
            name,
            owned_state: owned_state.into_boxed_slice(),
            ports: ports.into_boxed_slice(),
            footprints: footprints.into_boxed_slice(),
            budgets: budgets.into_boxed_slice(),
            assumptions: assumptions.into_boxed_slice(),
            guarantees: guarantees.into_boxed_slice(),
        }
    }
    /// Returns the identifier.
    #[must_use]
    pub const fn id(&self) -> StableId {
        self.id
    }
    /// Returns the name.
    #[must_use]
    pub const fn name(&self) -> &Identifier {
        &self.name
    }
    /// Returns owned state type IDs.
    #[must_use]
    pub const fn owned_state(&self) -> &[StableId] {
        &self.owned_state
    }
    /// Returns typed ports in stable-ID order.
    #[must_use]
    pub const fn ports(&self) -> &[PortDecl] {
        &self.ports
    }
    /// Returns declared conservative footprints.
    #[must_use]
    pub const fn footprints(&self) -> &[FootprintDecl] {
        &self.footprints
    }
    /// Returns budgets in resource order.
    #[must_use]
    pub const fn budgets(&self) -> &[BudgetDecl] {
        &self.budgets
    }
    /// Returns required claims.
    #[must_use]
    pub const fn assumptions(&self) -> &[StableId] {
        &self.assumptions
    }
    /// Returns established claims.
    #[must_use]
    pub const fn guarantees(&self) -> &[StableId] {
        &self.guarantees
    }
}

/// Explicit typed port wiring.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct WiringDecl {
    pub(crate) source_component: StableId,
    pub(crate) source_port: StableId,
    pub(crate) destination_component: StableId,
    pub(crate) destination_port: StableId,
}
impl WiringDecl {
    /// Creates a wiring.
    #[must_use]
    pub const fn new(
        source_component: StableId,
        source_port: StableId,
        destination_component: StableId,
        destination_port: StableId,
    ) -> Self {
        Self {
            source_component,
            source_port,
            destination_component,
            destination_port,
        }
    }
    /// Returns the source component.
    #[must_use]
    pub const fn source_component(self) -> StableId {
        self.source_component
    }
    /// Returns the source port.
    #[must_use]
    pub const fn source_port(self) -> StableId {
        self.source_port
    }
    /// Returns the destination component.
    #[must_use]
    pub const fn destination_component(self) -> StableId {
        self.destination_component
    }
    /// Returns the destination port.
    #[must_use]
    pub const fn destination_port(self) -> StableId {
        self.destination_port
    }
}

/// Canonical composition authoring AST.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompositionAst {
    pub(crate) wirings: Box<[WiringDecl]>,
    pub(crate) merge_order: Box<[StableId]>,
}
impl CompositionAst {
    /// Creates a composition AST. Elaboration validates port types and merge membership.
    #[must_use]
    pub fn new(wirings: Vec<WiringDecl>, merge_order: Vec<StableId>) -> Self {
        Self {
            wirings: wirings.into_boxed_slice(),
            merge_order: merge_order.into_boxed_slice(),
        }
    }
    /// Returns canonical wiring order.
    #[must_use]
    pub const fn wirings(&self) -> &[WiringDecl] {
        &self.wirings
    }
    /// Returns protocol-visible merge order.
    #[must_use]
    pub const fn merge_order(&self) -> &[StableId] {
        &self.merge_order
    }
}

/// Checked integer division policy.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DivisionMode {
    Exact,
    Floor,
    Ceil,
}

/// Scalar expression used by relational predicates.
#[allow(missing_docs)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValueExpr {
    Int(i128),
    Var(Identifier),
    Projection(ProjectionPath),
    Add(Box<ValueExpr>, Box<ValueExpr>),
    Sub(Box<ValueExpr>, Box<ValueExpr>),
    Mul(Box<ValueExpr>, Box<ValueExpr>),
    Div(DivisionMode, Box<ValueExpr>, Box<ValueExpr>),
    Sum {
        variable: Identifier,
        start: i128,
        end: i128,
        body: Box<ValueExpr>,
    },
}

/// Relational comparison operation.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompareOp {
    Eq,
    NotEq,
    Less,
    LessEq,
    Greater,
    GreaterEq,
}

/// Bounded relational formula.
#[allow(missing_docs)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RelExpr {
    Bool(bool),
    Not(Box<RelExpr>),
    And(Box<RelExpr>, Box<RelExpr>),
    Or(Box<RelExpr>, Box<RelExpr>),
    Implies(Box<RelExpr>, Box<RelExpr>),
    Compare(CompareOp, ValueExpr, ValueExpr),
    Predicate {
        name: Identifier,
        arguments: Box<[ValueExpr]>,
    },
    ForAll {
        variable: Identifier,
        start: i128,
        end: i128,
        body: Box<RelExpr>,
    },
    Exists {
        variable: Identifier,
        start: i128,
        end: i128,
        body: Box<RelExpr>,
    },
}

/// Strong finite-trace temporal formula.
#[allow(missing_docs)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TemporalFormula {
    Atom(RelExpr),
    Not(Box<TemporalFormula>),
    And(Box<TemporalFormula>, Box<TemporalFormula>),
    Or(Box<TemporalFormula>, Box<TemporalFormula>),
    Next(Box<TemporalFormula>),
    Always(Box<TemporalFormula>),
    Eventually(Box<TemporalFormula>),
    Until(Box<TemporalFormula>, Box<TemporalFormula>),
}

/// Closed public backend family.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum BackendId {
    Cvc5,
    Z3,
    Lean,
}

impl BackendId {
    /// Returns the exact qualified version.
    #[must_use]
    pub const fn qualified_version(self) -> &'static str {
        match self {
            Self::Cvc5 => "cvc5-1.3.3",
            Self::Z3 => "z3-4.16.0",
            Self::Lean => "lean-4.30.0",
        }
    }
    pub(crate) const fn tag(self) -> u8 {
        match self {
            Self::Cvc5 => 0,
            Self::Z3 => 1,
            Self::Lean => 2,
        }
    }
}

/// Explicit claim evaluation mode.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimMode {
    Relational,
    Finite { horizon: u32 },
    UnboundedProof,
}

/// Formula payload for a named claim.
#[allow(missing_docs)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClaimFormula {
    Relational(RelExpr),
    Temporal(TemporalFormula),
}

/// Named relational law.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LawDecl {
    pub(crate) id: StableId,
    pub(crate) name: Identifier,
    pub(crate) formula: RelExpr,
}
impl LawDecl {
    /// Creates a law.
    #[must_use]
    pub const fn new(id: StableId, name: Identifier, formula: RelExpr) -> Self {
        Self { id, name, formula }
    }
    /// Returns the identifier.
    #[must_use]
    pub const fn id(&self) -> StableId {
        self.id
    }
    /// Returns the name.
    #[must_use]
    pub const fn name(&self) -> &Identifier {
        &self.name
    }
    /// Returns the formula.
    #[must_use]
    pub const fn formula(&self) -> &RelExpr {
        &self.formula
    }
}

/// Named formal evidence obligation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimDecl {
    pub(crate) id: StableId,
    pub(crate) name: Identifier,
    pub(crate) backends: Box<[BackendId]>,
    pub(crate) mode: ClaimMode,
    pub(crate) formula: ClaimFormula,
}
impl ClaimDecl {
    /// Creates a claim. Elaboration validates mode, formula, and backend compatibility.
    #[must_use]
    pub fn new(
        id: StableId,
        name: Identifier,
        backends: Vec<BackendId>,
        mode: ClaimMode,
        formula: ClaimFormula,
    ) -> Self {
        Self {
            id,
            name,
            backends: backends.into_boxed_slice(),
            mode,
            formula,
        }
    }
    /// Returns the identifier.
    #[must_use]
    pub const fn id(&self) -> StableId {
        self.id
    }
    /// Returns the name.
    #[must_use]
    pub const fn name(&self) -> &Identifier {
        &self.name
    }
    /// Returns selected backends.
    #[must_use]
    pub const fn backends(&self) -> &[BackendId] {
        &self.backends
    }
    /// Returns the mode.
    #[must_use]
    pub const fn mode(&self) -> ClaimMode {
        self.mode
    }
    /// Returns the formula.
    #[must_use]
    pub const fn formula(&self) -> &ClaimFormula {
        &self.formula
    }
}

/// Parser output before reference resolution and canonical sorting.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedProject {
    pub(crate) version: u16,
    pub(crate) project_id: StableId,
    pub(crate) name: Identifier,
    pub(crate) declarations: Vec<SpannedDeclaration>,
    pub(crate) merge_order: Vec<StableId>,
    pub(crate) diagnostic_limit: usize,
}
impl ParsedProject {
    /// Returns the source language version.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }
    /// Returns the project identifier.
    #[must_use]
    pub const fn project_id(&self) -> StableId {
        self.project_id
    }
    /// Returns the project name.
    #[must_use]
    pub const fn name(&self) -> &Identifier {
        &self.name
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SpannedDeclaration {
    pub(crate) declaration: Declaration,
    pub(crate) span: SourceSpan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Declaration {
    Namespace(NamespaceDecl),
    Type(TypeDecl),
    Field(FieldDecl),
    Variant(VariantDecl),
    Reason(ReasonDecl),
    Effect(EffectDecl),
    Channel(ChannelDecl),
    Component(ComponentDecl),
    Wiring(WiringDecl),
    Law(LawDecl),
    Claim(ClaimDecl),
}

/// Canonical typed project specification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectSpec {
    pub(crate) project_id: StableId,
    pub(crate) name: Identifier,
    pub(crate) namespaces: Box<[NamespaceDecl]>,
    pub(crate) types: Box<[TypeDecl]>,
    pub(crate) fields: Box<[FieldDecl]>,
    pub(crate) variants: Box<[VariantDecl]>,
    pub(crate) reasons: Box<[ReasonDecl]>,
    pub(crate) effects: Box<[EffectDecl]>,
    pub(crate) channels: Box<[ChannelDecl]>,
    pub(crate) components: Box<[ComponentDecl]>,
    pub(crate) composition: CompositionAst,
    pub(crate) laws: Box<[LawDecl]>,
    pub(crate) claims: Box<[ClaimDecl]>,
}

impl ProjectSpec {
    /// Returns the project identifier.
    #[must_use]
    pub const fn project_id(&self) -> StableId {
        self.project_id
    }
    /// Returns the project name.
    #[must_use]
    pub const fn name(&self) -> &Identifier {
        &self.name
    }
    /// Returns namespaces in stable-ID order.
    #[must_use]
    pub const fn namespaces(&self) -> &[NamespaceDecl] {
        &self.namespaces
    }
    /// Returns types in stable-ID order.
    #[must_use]
    pub const fn types(&self) -> &[TypeDecl] {
        &self.types
    }
    /// Returns fields in owner and stable-ID order.
    #[must_use]
    pub const fn fields(&self) -> &[FieldDecl] {
        &self.fields
    }
    /// Returns variants in owner and stable-ID order.
    #[must_use]
    pub const fn variants(&self) -> &[VariantDecl] {
        &self.variants
    }
    /// Returns reasons in stable-ID order. Precedence remains explicit.
    #[must_use]
    pub const fn reasons(&self) -> &[ReasonDecl] {
        &self.reasons
    }
    /// Returns effects in stable-ID order.
    #[must_use]
    pub const fn effects(&self) -> &[EffectDecl] {
        &self.effects
    }
    /// Returns channels in stable-ID order.
    #[must_use]
    pub const fn channels(&self) -> &[ChannelDecl] {
        &self.channels
    }
    /// Returns components in stable-ID order.
    #[must_use]
    pub const fn components(&self) -> &[ComponentDecl] {
        &self.components
    }
    /// Returns explicit wiring and semantic merge order.
    #[must_use]
    pub const fn composition(&self) -> &CompositionAst {
        &self.composition
    }
    /// Returns laws in stable-ID order.
    #[must_use]
    pub const fn laws(&self) -> &[LawDecl] {
        &self.laws
    }
    /// Returns claims in stable-ID order.
    #[must_use]
    pub const fn claims(&self) -> &[ClaimDecl] {
        &self.claims
    }
    /// Finds a claim by stable identifier.
    #[must_use]
    pub fn claim(&self, id: StableId) -> Option<&ClaimDecl> {
        self.claims
            .binary_search_by_key(&id, ClaimDecl::id)
            .ok()
            .map(|index| &self.claims[index])
    }
    /// Computes the authoring identity. Source formatting and comments are excluded.
    pub fn commitment<H: CommitmentHasher>(&self) -> Result<Hash32, EncodeError> {
        let domain = Domain::new("zeno-fcis/project-spec", PROJECT_SPEC_FORMAT_VERSION)?;
        commitment::<H>(domain, &self.canonical_bytes()?)
    }
}

impl CanonicalEncode for ProjectSpec {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-PROJECT-SPEC\0");
        output.extend_from_slice(&PROJECT_SPEC_FORMAT_VERSION.to_be_bytes());
        self.project_id.encode_to(output)?;
        self.name.encode_to(output)?;
        put_slice(output, &self.namespaces, encode_namespace)?;
        put_slice(output, &self.types, encode_type)?;
        put_slice(output, &self.fields, encode_field)?;
        put_slice(output, &self.variants, encode_variant)?;
        put_slice(output, &self.reasons, encode_reason)?;
        put_slice(output, &self.effects, encode_effect)?;
        put_slice(output, &self.channels, encode_channel)?;
        put_slice(output, &self.components, encode_component)?;
        put_slice(output, &self.composition.wirings, encode_wiring)?;
        put_ids(output, &self.composition.merge_order)?;
        put_slice(output, &self.laws, encode_law)?;
        put_slice(output, &self.claims, encode_claim)
    }
}

fn put_length(output: &mut Vec<u8>, length: usize) -> Result<(), EncodeError> {
    let length = u32::try_from(length).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    Ok(())
}
fn put_blob(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), EncodeError> {
    put_length(output, bytes.len())?;
    output.extend_from_slice(bytes);
    Ok(())
}
fn put_slice<T>(
    output: &mut Vec<u8>,
    values: &[T],
    encode: fn(&T, &mut Vec<u8>) -> Result<(), EncodeError>,
) -> Result<(), EncodeError> {
    put_length(output, values.len())?;
    for value in values {
        encode(value, output)?;
    }
    Ok(())
}
fn put_ids(output: &mut Vec<u8>, ids: &[StableId]) -> Result<(), EncodeError> {
    put_length(output, ids.len())?;
    for id in ids {
        id.encode_to(output)?;
    }
    Ok(())
}
fn encode_namespace(value: &NamespaceDecl, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    value.id.encode_to(out)?;
    value.name.encode_to(out)
}
fn encode_type(value: &TypeDecl, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    value.id.encode_to(out)?;
    out.push(value.kind.tag());
    value.name.encode_to(out)
}
fn encode_field(value: &FieldDecl, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    value.owner.encode_to(out)?;
    value.id.encode_to(out)?;
    value.name.encode_to(out)?;
    value.field_type.encode_to(out)
}
fn encode_variant(value: &VariantDecl, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    value.owner.encode_to(out)?;
    value.id.encode_to(out)?;
    value.name.encode_to(out)?;
    match value.payload_type {
        Some(id) => {
            out.push(1);
            id.encode_to(out)
        }
        None => {
            out.push(0);
            Ok(())
        }
    }
}
fn encode_reason(value: &ReasonDecl, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    value.id.encode_to(out)?;
    value.name.encode_to(out)?;
    out.extend_from_slice(&value.precedence.to_be_bytes());
    Ok(())
}
fn encode_effect(value: &EffectDecl, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    value.id.encode_to(out)?;
    value.name.encode_to(out)?;
    value.destination_type.encode_to(out)?;
    value.payload_type.encode_to(out)
}
fn encode_channel(value: &ChannelDecl, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    value.id.encode_to(out)?;
    value.name.encode_to(out)?;
    value.destination_type.encode_to(out)?;
    value.payload_type.encode_to(out)
}
fn encode_port(value: &PortDecl, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    value.id.encode_to(out)?;
    out.push(match value.direction {
        PortDirection::Input => 0,
        PortDirection::Output => 1,
    });
    value.name.encode_to(out)?;
    value.payload_type.encode_to(out)
}
fn encode_path(value: &ProjectionPath, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    out.push(value.root.tag());
    put_ids(out, &value.segments)
}
fn encode_component(value: &ComponentDecl, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    value.id.encode_to(out)?;
    value.name.encode_to(out)?;
    put_ids(out, &value.owned_state)?;
    put_slice(out, &value.ports, encode_port)?;
    put_length(out, value.footprints.len())?;
    for footprint in &value.footprints {
        out.push(match footprint.kind {
            FootprintKind::Read => 0,
            FootprintKind::Write => 1,
            FootprintKind::Context => 2,
            FootprintKind::Effect => 3,
            FootprintKind::Outbox => 4,
        });
        encode_path(&footprint.path, out)?;
    }
    put_length(out, value.budgets.len())?;
    for budget in &value.budgets {
        out.push(match budget.resource {
            BudgetResource::Steps => 0,
            BudgetResource::Nodes => 1,
            BudgetResource::Bytes => 2,
            BudgetResource::PredicateCalls => 3,
        });
        out.extend_from_slice(&budget.limit.to_be_bytes());
    }
    put_ids(out, &value.assumptions)?;
    put_ids(out, &value.guarantees)
}
fn encode_wiring(value: &WiringDecl, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    value.source_component.encode_to(out)?;
    value.source_port.encode_to(out)?;
    value.destination_component.encode_to(out)?;
    value.destination_port.encode_to(out)
}
fn encode_value(value: &ValueExpr, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    match value {
        ValueExpr::Int(v) => {
            out.push(0);
            out.extend_from_slice(&v.to_be_bytes());
            Ok(())
        }
        ValueExpr::Var(v) => {
            out.push(1);
            v.encode_to(out)
        }
        ValueExpr::Projection(v) => {
            out.push(2);
            encode_path(v, out)
        }
        ValueExpr::Add(a, b) => {
            out.push(3);
            encode_value(a, out)?;
            encode_value(b, out)
        }
        ValueExpr::Sub(a, b) => {
            out.push(4);
            encode_value(a, out)?;
            encode_value(b, out)
        }
        ValueExpr::Mul(a, b) => {
            out.push(5);
            encode_value(a, out)?;
            encode_value(b, out)
        }
        ValueExpr::Div(mode, a, b) => {
            out.push(6);
            out.push(match mode {
                DivisionMode::Exact => 0,
                DivisionMode::Floor => 1,
                DivisionMode::Ceil => 2,
            });
            encode_value(a, out)?;
            encode_value(b, out)
        }
        ValueExpr::Sum {
            variable,
            start,
            end,
            body,
        } => {
            out.push(7);
            variable.encode_to(out)?;
            out.extend_from_slice(&start.to_be_bytes());
            out.extend_from_slice(&end.to_be_bytes());
            encode_value(body, out)
        }
    }
}
fn encode_rel(value: &RelExpr, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    match value {
        RelExpr::Bool(v) => {
            out.push(if *v { 1 } else { 0 });
            Ok(())
        }
        RelExpr::Not(v) => {
            out.push(2);
            encode_rel(v, out)
        }
        RelExpr::And(a, b) => {
            out.push(3);
            encode_rel(a, out)?;
            encode_rel(b, out)
        }
        RelExpr::Or(a, b) => {
            out.push(4);
            encode_rel(a, out)?;
            encode_rel(b, out)
        }
        RelExpr::Implies(a, b) => {
            out.push(5);
            encode_rel(a, out)?;
            encode_rel(b, out)
        }
        RelExpr::Compare(op, a, b) => {
            out.push(6);
            out.push(match op {
                CompareOp::Eq => 0,
                CompareOp::NotEq => 1,
                CompareOp::Less => 2,
                CompareOp::LessEq => 3,
                CompareOp::Greater => 4,
                CompareOp::GreaterEq => 5,
            });
            encode_value(a, out)?;
            encode_value(b, out)
        }
        RelExpr::Predicate { name, arguments } => {
            out.push(7);
            name.encode_to(out)?;
            put_length(out, arguments.len())?;
            for argument in arguments.iter() {
                encode_value(argument, out)?;
            }
            Ok(())
        }
        RelExpr::ForAll {
            variable,
            start,
            end,
            body,
        } => {
            out.push(8);
            variable.encode_to(out)?;
            out.extend_from_slice(&start.to_be_bytes());
            out.extend_from_slice(&end.to_be_bytes());
            encode_rel(body, out)
        }
        RelExpr::Exists {
            variable,
            start,
            end,
            body,
        } => {
            out.push(9);
            variable.encode_to(out)?;
            out.extend_from_slice(&start.to_be_bytes());
            out.extend_from_slice(&end.to_be_bytes());
            encode_rel(body, out)
        }
    }
}
fn encode_temporal(value: &TemporalFormula, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    match value {
        TemporalFormula::Atom(v) => {
            out.push(0);
            encode_rel(v, out)
        }
        TemporalFormula::Not(v) => {
            out.push(1);
            encode_temporal(v, out)
        }
        TemporalFormula::And(a, b) => {
            out.push(2);
            encode_temporal(a, out)?;
            encode_temporal(b, out)
        }
        TemporalFormula::Or(a, b) => {
            out.push(3);
            encode_temporal(a, out)?;
            encode_temporal(b, out)
        }
        TemporalFormula::Next(v) => {
            out.push(4);
            encode_temporal(v, out)
        }
        TemporalFormula::Always(v) => {
            out.push(5);
            encode_temporal(v, out)
        }
        TemporalFormula::Eventually(v) => {
            out.push(6);
            encode_temporal(v, out)
        }
        TemporalFormula::Until(a, b) => {
            out.push(7);
            encode_temporal(a, out)?;
            encode_temporal(b, out)
        }
    }
}
fn encode_law(value: &LawDecl, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    value.id.encode_to(out)?;
    value.name.encode_to(out)?;
    encode_rel(&value.formula, out)
}
fn encode_claim(value: &ClaimDecl, out: &mut Vec<u8>) -> Result<(), EncodeError> {
    value.id.encode_to(out)?;
    value.name.encode_to(out)?;
    put_length(out, value.backends.len())?;
    for backend in value.backends.iter() {
        out.push(backend.tag());
    }
    match value.mode {
        ClaimMode::Relational => out.push(0),
        ClaimMode::Finite { horizon } => {
            out.push(1);
            out.extend_from_slice(&horizon.to_be_bytes())
        }
        ClaimMode::UnboundedProof => out.push(2),
    }
    match &value.formula {
        ClaimFormula::Relational(v) => {
            out.push(0);
            encode_rel(v, out)
        }
        ClaimFormula::Temporal(v) => {
            out.push(1);
            encode_temporal(v, out)
        }
    }
}

fn formula_shape_value(value: &ValueExpr) -> (usize, usize) {
    let mut stack = Vec::new();
    stack.push((value, 1usize));
    let mut nodes = 0usize;
    let mut depth = 0usize;
    while let Some((current, current_depth)) = stack.pop() {
        nodes = nodes.saturating_add(1);
        depth = depth.max(current_depth);
        match current {
            ValueExpr::Add(left, right)
            | ValueExpr::Sub(left, right)
            | ValueExpr::Mul(left, right)
            | ValueExpr::Div(_, left, right) => {
                stack.push((left, current_depth.saturating_add(1)));
                stack.push((right, current_depth.saturating_add(1)));
            }
            ValueExpr::Sum { body, .. } => {
                stack.push((body, current_depth.saturating_add(1)));
            }
            ValueExpr::Int(_) | ValueExpr::Var(_) | ValueExpr::Projection(_) => {}
        }
    }
    (nodes, depth)
}

pub(crate) fn formula_shape_rel(value: &RelExpr) -> (usize, usize) {
    let mut stack = Vec::new();
    stack.push((value, 1usize));
    let mut nodes = 0usize;
    let mut depth = 0usize;
    while let Some((current, current_depth)) = stack.pop() {
        nodes = nodes.saturating_add(1);
        depth = depth.max(current_depth);
        match current {
            RelExpr::Not(inner) => {
                stack.push((inner, current_depth.saturating_add(1)));
            }
            RelExpr::And(left, right)
            | RelExpr::Or(left, right)
            | RelExpr::Implies(left, right) => {
                stack.push((left, current_depth.saturating_add(1)));
                stack.push((right, current_depth.saturating_add(1)));
            }
            RelExpr::Compare(_, left, right) => {
                for value in [left, right] {
                    let (value_nodes, value_depth) = formula_shape_value(value);
                    nodes = nodes.saturating_add(value_nodes);
                    depth = depth.max(current_depth.saturating_add(value_depth));
                }
            }
            RelExpr::Predicate { arguments, .. } => {
                for value in arguments {
                    let (value_nodes, value_depth) = formula_shape_value(value);
                    nodes = nodes.saturating_add(value_nodes);
                    depth = depth.max(current_depth.saturating_add(value_depth));
                }
            }
            RelExpr::ForAll { body, .. } | RelExpr::Exists { body, .. } => {
                stack.push((body, current_depth.saturating_add(1)));
            }
            RelExpr::Bool(_) => {}
        }
    }
    (nodes, depth)
}

pub(crate) fn formula_shape_temporal(value: &TemporalFormula) -> (usize, usize) {
    let mut stack = Vec::new();
    stack.push((value, 1usize));
    let mut nodes = 0usize;
    let mut depth = 0usize;
    while let Some((current, current_depth)) = stack.pop() {
        nodes = nodes.saturating_add(1);
        depth = depth.max(current_depth);
        match current {
            TemporalFormula::Atom(relational) => {
                let (relational_nodes, relational_depth) = formula_shape_rel(relational);
                nodes = nodes.saturating_add(relational_nodes);
                depth = depth.max(current_depth.saturating_add(relational_depth));
            }
            TemporalFormula::Not(inner)
            | TemporalFormula::Next(inner)
            | TemporalFormula::Always(inner)
            | TemporalFormula::Eventually(inner) => {
                stack.push((inner, current_depth.saturating_add(1)));
            }
            TemporalFormula::And(left, right)
            | TemporalFormula::Or(left, right)
            | TemporalFormula::Until(left, right) => {
                stack.push((left, current_depth.saturating_add(1)));
                stack.push((right, current_depth.saturating_add(1)));
            }
        }
    }
    (nodes, depth)
}

#[cfg(test)]
mod shape_tests {
    use super::{CompareOp, RelExpr, TemporalFormula, ValueExpr};

    #[test]
    fn formula_shapes_count_scalar_relational_and_temporal_depth_exactly() {
        let scalar = ValueExpr::Add(
            Box::new(ValueExpr::Int(1)),
            Box::new(ValueExpr::Mul(
                Box::new(ValueExpr::Int(2)),
                Box::new(ValueExpr::Int(3)),
            )),
        );
        assert_eq!(super::formula_shape_value(&scalar), (5, 3));

        let relational = RelExpr::And(
            Box::new(RelExpr::Compare(CompareOp::Eq, scalar, ValueExpr::Int(7))),
            Box::new(RelExpr::Not(Box::new(RelExpr::Bool(false)))),
        );
        assert_eq!(super::formula_shape_rel(&relational), (10, 5));

        let temporal = TemporalFormula::Always(Box::new(TemporalFormula::Atom(relational)));
        assert_eq!(super::formula_shape_temporal(&temporal), (12, 7));
    }
}
