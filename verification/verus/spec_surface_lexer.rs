#![no_std]
#![allow(dead_code, unused_imports)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]
extern crate alloc;
#[path = "../../crates/zeno-fcis-spec/src/diagnostic.rs"]
mod diagnostic;
#[path = "../../crates/zeno-fcis-spec/src/lexical_core.rs"]
mod lexical_core;
#[path = "../../crates/zeno-fcis-spec/src/source_limits.rs"]
mod source_limits;
#[path = "../../crates/zeno-fcis-spec/src/source_span.rs"]
mod source_span;
#[path = "../../crates/zeno-fcis-spec/src/source_token.rs"]
mod source_token;
#[path = "../../crates/zeno-fcis-spec/src/surface_lexer.rs"]
mod surface_lexer;
#[cfg(not(verus_keep_ghost))]
use diagnostic::DiagnosticSet;
use diagnostic::{AstPath, Diagnostic, DiagnosticCode, DiagnosticStage};
use source_limits::SourceLimits;
use source_span::SourceSpan;

#[cfg(test)]
#[path = "spec_surface_lexer_tests.rs"]
mod tests;
