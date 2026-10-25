//! Bounded `.zeno` authoring, typed project specifications, and inert logic declarations.
//!
//! This crate is a pure `no_std + alloc` compiler layer. It parses inert source
//! text into a typed, canonically ordered [`ProjectSpec`]. It cannot execute
//! processes, read files, mint evidence, or construct production authority.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

mod ast;
mod bool_circuit;
mod builder;
mod checked_parser;
mod collections;
mod comparison_ops;
mod diagnostic;
mod distribution;
mod elaborate;
mod induction;
mod lexer;
mod lexical_core;
mod mini_determinator;
mod parser;
mod paths;
mod semantic_readers;
mod source;
mod source_limits;
mod source_span;
mod source_token;
mod substance;
mod surface_lexer;
mod syntax;
#[allow(dead_code, unreachable_pub)]
// Shared generic API is independently exercised by proof harnesses.
mod syntax_certificate;
#[allow(dead_code)] // Generator exposes identities and roots to all grammar clients.
mod syntax_grammar;
#[allow(dead_code)]
mod syntax_grammar_extended;
mod syntax_lowering;
mod syntax_resources;
mod tables;
mod value_expr;
mod views;

pub use ast::*;
pub use bool_circuit::{BoolCircuit, BoolNode};
pub use builder::{CompositionAstBuilder, ProjectSpecBuilder};
pub use collections::{
    CollectionsSpec, EntryDecl, KeyTypeDecl, MapDecl, OrderReadDecl, OrderReadKind, SummaryDecl,
    SummaryKind, TallyDecl,
};
pub use diagnostic::{
    AstPath, Diagnostic, DiagnosticCode, DiagnosticSet, DiagnosticStage, SourceSpan,
};
pub use distribution::{
    DistributionDecl, DistributionOutcome, Probability, RandomInputDecl, Rational,
    RewardComparison, RewardDecl, RewardKind, RewardMeasure,
};
pub use elaborate::elaborate_project;
pub use induction::invariant_at;
pub use mini_determinator::{
    MergeConflict, MiniBlocker, MiniBudget, MiniCommand, MiniDecision, MiniDeterminator, MiniRun,
    MiniState, PrivateWork, WorkerInstruction, WorkerProgram, WorkerTrace, WorkspaceCell,
};
pub use parser::{
    TableReadExpression, expand_tables, parse_formula, parse_law_formula, parse_project,
    parse_table_expression, parse_table_read, table_index_sets,
};
pub use paths::{
    DeclaredDomain, PathResolution, TableIndexRange, claim_paths, declared_domain,
    declared_table_index_range, law_paths, resolve_path,
};
pub use source::{SOURCE_FILES, source_digest};
pub use source_limits::{
    MAX_RETAINED_DIAGNOSTICS, MAX_SOURCE_BYTES, MAX_SOURCE_TOKENS, SourceLimits,
};
pub use substance::{
    Substance, claim_substance, invariant_substance, law_substance, relation_substance,
    temporal_substance,
};
pub use tables::{CountInfo, MAX_TABLE_ENTRIES, TableExpansion, TableInfo};
pub use views::{
    DerivedComposition, GeneratedProject, GraphFormat, ObligationKind, UnresolvedObligation,
    derive_composition, generate_project, render_graph,
};

/// Version of the `.zeno` grammar and lexer contract.
pub const ZENO_DSL_VERSION: u16 = 1;
/// Version of [`ProjectSpec`] canonical bytes.
pub const PROJECT_SPEC_FORMAT_VERSION: u16 = 1;
/// Version of relational and temporal formula canonical bytes.
pub const TEMPORAL_SPEC_FORMAT_VERSION: u16 = 1;

/// Maximum recursive expression nesting admitted by the language parser.
pub const MAX_FORMULA_DEPTH: usize = 256;
/// Maximum composite scalar/formula height built by source parsing. Numeric
/// preflight applies this bound before allocating a recursive expression.
pub const MAX_PARSED_EXPRESSION_HEIGHT: usize = MAX_FORMULA_DEPTH + 2;
/// Maximum total formula nodes admitted by elaboration.
pub const MAX_FORMULA_NODES: usize = 1_000_000;
/// Maximum finite logical-trace horizon admitted by language version 1.
pub const MAX_FINITE_HORIZON: u32 = 256;

/// Elaboration and formula-complexity limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectLimits {
    max_declarations: usize,
    max_components: usize,
    max_ports_per_component: usize,
    max_formula_nodes: usize,
    max_formula_depth: usize,
}

impl ProjectLimits {
    /// Creates a nonzero bounded elaboration envelope.
    pub const fn try_new(
        max_declarations: usize,
        max_components: usize,
        max_ports_per_component: usize,
        max_formula_nodes: usize,
        max_formula_depth: usize,
    ) -> Option<Self> {
        if max_declarations == 0
            || max_declarations > 65_536
            || max_components == 0
            || max_components > 256
            || max_ports_per_component == 0
            || max_ports_per_component > 256
            || max_formula_nodes == 0
            || max_formula_nodes > MAX_FORMULA_NODES
            || max_formula_depth == 0
            || max_formula_depth > MAX_FORMULA_DEPTH
        {
            return None;
        }
        Some(Self {
            max_declarations,
            max_components,
            max_ports_per_component,
            max_formula_nodes,
            max_formula_depth,
        })
    }

    /// Returns the total declaration bound.
    #[must_use]
    pub const fn max_declarations(self) -> usize {
        self.max_declarations
    }

    /// Returns the component bound.
    #[must_use]
    pub const fn max_components(self) -> usize {
        self.max_components
    }

    /// Returns the per-component port bound.
    #[must_use]
    pub const fn max_ports_per_component(self) -> usize {
        self.max_ports_per_component
    }

    /// Returns the total formula-node bound.
    #[must_use]
    pub const fn max_formula_nodes(self) -> usize {
        self.max_formula_nodes
    }

    /// Returns the formula-depth bound.
    #[must_use]
    pub const fn max_formula_depth(self) -> usize {
        self.max_formula_depth
    }
}

impl Default for ProjectLimits {
    fn default() -> Self {
        Self {
            max_declarations: 65_536,
            max_components: 256,
            max_ports_per_component: 256,
            max_formula_nodes: MAX_FORMULA_NODES,
            max_formula_depth: MAX_FORMULA_DEPTH,
        }
    }
}
