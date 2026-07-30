//! Validating builders that share the parser elaboration path.

use alloc::vec::Vec;

use crate::ast::*;
use crate::{DiagnosticSet, ProjectLimits, SourceSpan, ZENO_DSL_VERSION, elaborate_project};

/// Incremental builder for the same typed AST produced by `.zeno` parsing.
#[derive(Clone, Debug)]
pub struct ProjectSpecBuilder {
    project_id: StableId,
    name: Identifier,
    declarations: Vec<Declaration>,
    merge_order: Vec<StableId>,
}

impl ProjectSpecBuilder {
    /// Starts a project with explicit stable identity.
    #[must_use]
    pub const fn new(project_id: StableId, name: Identifier) -> Self {
        Self {
            project_id,
            name,
            declarations: Vec::new(),
            merge_order: Vec::new(),
        }
    }
    /// Adds a namespace.
    #[must_use]
    pub fn namespace(mut self, value: NamespaceDecl) -> Self {
        self.declarations.push(Declaration::Namespace(value));
        self
    }
    /// Adds a type.
    #[must_use]
    pub fn type_decl(mut self, value: TypeDecl) -> Self {
        self.declarations.push(Declaration::Type(value));
        self
    }
    /// Adds a field.
    #[must_use]
    pub fn field(mut self, value: FieldDecl) -> Self {
        self.declarations.push(Declaration::Field(value));
        self
    }
    /// Adds a variant.
    #[must_use]
    pub fn variant(mut self, value: VariantDecl) -> Self {
        self.declarations.push(Declaration::Variant(value));
        self
    }
    /// Adds a reason.
    #[must_use]
    pub fn reason(mut self, value: ReasonDecl) -> Self {
        self.declarations.push(Declaration::Reason(value));
        self
    }
    /// Adds an effect.
    #[must_use]
    pub fn effect(mut self, value: EffectDecl) -> Self {
        self.declarations.push(Declaration::Effect(value));
        self
    }
    /// Adds a channel.
    #[must_use]
    pub fn channel(mut self, value: ChannelDecl) -> Self {
        self.declarations.push(Declaration::Channel(value));
        self
    }
    /// Adds a domain component.
    #[must_use]
    pub fn component(mut self, value: ComponentDecl) -> Self {
        self.declarations.push(Declaration::Component(value));
        self
    }
    /// Adds an explicit port wiring.
    #[must_use]
    pub fn wiring(mut self, value: WiringDecl) -> Self {
        self.declarations.push(Declaration::Wiring(value));
        self
    }
    /// Adds a relational law.
    #[must_use]
    pub fn law(mut self, value: LawDecl) -> Self {
        self.declarations.push(Declaration::Law(value));
        self
    }
    /// Adds a formal claim.
    #[must_use]
    pub fn claim(mut self, value: ClaimDecl) -> Self {
        self.declarations.push(Declaration::Claim(value));
        self
    }
    /// Sets the protocol-visible composition merge order.
    #[must_use]
    pub fn merge_order(mut self, value: Vec<StableId>) -> Self {
        self.merge_order = value;
        self
    }
    /// Elaborates through the exact parser lowering path.
    pub fn finish(self, limits: ProjectLimits) -> Result<ProjectSpec, DiagnosticSet> {
        let span = SourceSpan::new(0, 0, 1, 1);
        elaborate_project(
            ParsedProject {
                version: ZENO_DSL_VERSION,
                project_id: self.project_id,
                name: self.name,
                declarations: self
                    .declarations
                    .into_iter()
                    .map(|declaration| SpannedDeclaration { declaration, span })
                    .collect(),
                merge_order: self.merge_order,
                diagnostic_limit: crate::MAX_RETAINED_DIAGNOSTICS,
            },
            limits,
        )
    }
}

/// Focused builder for canonical wiring and merge order.
#[derive(Clone, Debug, Default)]
pub struct CompositionAstBuilder {
    wirings: Vec<WiringDecl>,
    merge_order: Vec<StableId>,
}

impl CompositionAstBuilder {
    /// Creates an empty composition builder.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            wirings: Vec::new(),
            merge_order: Vec::new(),
        }
    }
    /// Adds one wiring.
    #[must_use]
    pub fn wiring(mut self, value: WiringDecl) -> Self {
        self.wirings.push(value);
        self
    }
    /// Sets semantic merge order.
    #[must_use]
    pub fn merge_order(mut self, value: Vec<StableId>) -> Self {
        self.merge_order = value;
        self
    }
    /// Canonicalizes wiring while retaining explicit merge order.
    #[must_use]
    pub fn finish(mut self) -> CompositionAst {
        self.wirings.sort_unstable();
        self.wirings.dedup();
        CompositionAst::new(self.wirings, self.merge_order)
    }
}
