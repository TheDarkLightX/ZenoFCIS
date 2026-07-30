//! Bounded `.zeno` authoring, typed project specifications, and executable logic.
//!
//! This crate is a pure `no_std + alloc` compiler layer. It parses inert source
//! text into a typed, canonically ordered [`ProjectSpec`]. It cannot execute
//! processes, read files, mint evidence, or construct production authority.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

mod ast;
mod builder;
mod diagnostic;
mod elaborate;
mod lexer;
mod logic;
mod mini_determinator;
mod parser;
mod views;

pub use ast::*;
pub use builder::{CompositionAstBuilder, ProjectSpecBuilder};
pub use diagnostic::{
    AstPath, Diagnostic, DiagnosticCode, DiagnosticSet, DiagnosticStage, SourceSpan,
};
pub use elaborate::elaborate_project;
pub use logic::{
    EvalLimits, EvalOutcome, EvaluationContext, IndeterminateReason, NamedPredicate, Observation,
    PredicateProvider, TemporalEvaluation, TraceStep, evaluate_relational, evaluate_temporal,
};
pub use mini_determinator::{
    MergeConflict, MiniBlocker, MiniBudget, MiniCommand, MiniDecision, MiniDeterminator, MiniRun,
    MiniState, PrivateWork, WorkerInstruction, WorkerProgram, WorkerTrace, WorkspaceCell,
};
pub use parser::parse_project;
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

/// Maximum accepted UTF-8 source length.
pub const MAX_SOURCE_BYTES: usize = 1024 * 1024;
/// Maximum accepted lexer token count.
pub const MAX_SOURCE_TOKENS: usize = 262_144;
/// Maximum diagnostics retained in a returned set.
pub const MAX_RETAINED_DIAGNOSTICS: usize = 256;
/// Maximum recursive expression nesting admitted by the language parser.
pub const MAX_FORMULA_DEPTH: usize = 256;
/// Maximum total formula nodes admitted by elaboration and direct evaluation.
pub const MAX_FORMULA_NODES: usize = 1_000_000;
/// Maximum finite logical-trace horizon admitted by language version 1.
pub const MAX_FINITE_HORIZON: u32 = 256;

/// Caller-selected limits bounded by the hard language limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceLimits {
    max_bytes: usize,
    max_tokens: usize,
    max_diagnostics: usize,
}

impl SourceLimits {
    /// Creates a nonzero limit set no larger than the language hard limits.
    pub const fn try_new(
        max_bytes: usize,
        max_tokens: usize,
        max_diagnostics: usize,
    ) -> Option<Self> {
        if max_bytes == 0
            || max_bytes > MAX_SOURCE_BYTES
            || max_tokens == 0
            || max_tokens > MAX_SOURCE_TOKENS
            || max_diagnostics == 0
            || max_diagnostics > MAX_RETAINED_DIAGNOSTICS
        {
            return None;
        }
        Some(Self {
            max_bytes,
            max_tokens,
            max_diagnostics,
        })
    }

    /// Returns the source-byte limit.
    #[must_use]
    pub const fn max_bytes(self) -> usize {
        self.max_bytes
    }

    /// Returns the token limit.
    #[must_use]
    pub const fn max_tokens(self) -> usize {
        self.max_tokens
    }

    /// Returns the retained-diagnostic limit.
    #[must_use]
    pub const fn max_diagnostics(self) -> usize {
        self.max_diagnostics
    }
}

impl Default for SourceLimits {
    fn default() -> Self {
        Self {
            max_bytes: MAX_SOURCE_BYTES,
            max_tokens: MAX_SOURCE_TOKENS,
            max_diagnostics: MAX_RETAINED_DIAGNOSTICS,
        }
    }
}

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
