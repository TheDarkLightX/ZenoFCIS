//! Proposer-side doors for the ZenoFCIS bounded loop.
//!
//! A *door* is what a proposer may submit to the loop besides a whole
//! canonical program. This crate turns each kind of submission into
//! candidate programs and explanations; it never accepts anything. Every
//! program an original-bound door returns must still be judged by `zeno-fcis transform check`
//! (with its receipt and `transform replay`), which is what the
//! `zeno-fcis loop` commands do with them.
//!
//! - [`expr`]: the loop expression language: the contract policy-expression
//!   grammar with `c ? a : b`, constant holes `?c` and `?c[lo..hi]`,
//!   expression holes `?e`, choice holes `?{a | b}`, family parameters `$P`
//!   and `keep` (a whole output kept from the original program);
//! - [`lower`]: complete expressions to a hash-consed finite program with the
//!   original's exact input and output domains;
//! - [`inverse`]: inverse semantics, the requirement one position of an
//!   expression must meet for its output to be right;
//! - [`repair`]: for a wrong proposal, the failing region as a formula,
//!   ranked repair sites in the proposer's own text, and a repair;
//! - [`sketch`]: hole solving, exact for single holes and complete over
//!   stated candidate spaces for constant and choice holes;
//! - [`family`]: parameter binding, linear fits `a * $P + b` and transfer of
//!   a program across family instances.
//! - [`learn`]: fixed-vocabulary enumeration from examples, with no original;
//!   counts and uniqueness remain proposals. The CLI confirms lowered examples
//!   with the existing library evaluator, without an equivalence receipt.
//!
//! # What a result means
//!
//! Expression values here are computed by this crate's own evaluator over
//! every declared input tuple, and candidate programs are compared with the
//! original by the library evaluator (`Program::evaluate`) on every tuple.
//! Those comparisons are the doors' own consistency checks and carry no
//! authority. A "not found" from synthesis claims nothing; an
//! "unrealizable" sketch carries either a witness input or the complete
//! candidate space that was exhausted, so it can be checked independently.
//!
//! # Determinism and budgets
//!
//! The crate reads no clock, file, environment or randomness. Sampling uses
//! a fixed seed. Domain size, enumerated assignments and synthesis work are
//! bounded; over a bound a door reports a typed stop or "inconclusive" and
//! no program.
//!
//! # Provenance
//!
//! This crate is LLM-generated. It ports to Rust the methods of an earlier
//! Python prototype by the same owner, who relicensed that work for this crate
//! under the workspace licence (MIT OR Apache-2.0).

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod expr;
pub mod family;
pub mod inverse;
pub mod lower;
pub mod repair;
pub mod sketch;
pub mod subject;
// BEGIN typed loop strategy algebra.
pub mod strategy_algebra;
pub mod strategy_questions;
pub mod strategy_split;
// END typed loop strategy algebra.
// BEGIN bounded structural knowledge engine.
pub mod strategy_knowledge;
// END bounded structural knowledge engine.

// Requirements-only questions (proposal computation, no acceptance authority).
pub mod version_space;

// Black-box capture: table encoding and example policies propose, never judge.
pub mod capture;

// Learn door: vocabulary enumeration proposes; the existing evaluator checks examples.
pub mod learn;

#[cfg(test)]
mod tests;

use alloc::string::String;
use core::fmt;

/// Why a door refused its input or could not proceed. The text is a stable
/// code, optionally followed by `:` and a detail.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct Refusal(pub String);

impl Refusal {
    /// A refusal with the given code.
    #[must_use]
    pub fn new(code: impl Into<String>) -> Self {
        Self(code.into())
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Shorthand for door results.
pub type Result<T> = core::result::Result<T, Refusal>;

/// Source text for internal tool fingerprints; not a stable application API.
/// Include `LIB` as well as leaf entries so fingerprints bind this mapping.
#[doc(hidden)]
pub mod source_fingerprint {
    /// Exact source bytes owned by this crate.
    pub const EXAMPLES_STRATEGY_EXAMPLE: &str = include_str!("../examples/strategy-example.kb");
    /// Exact source bytes owned by this crate.
    pub const CAPTURE: &str = include_str!("capture.rs");
    /// Exact source bytes owned by this crate.
    pub const EXPR: &str = include_str!("expr.rs");
    /// Exact source bytes owned by this crate.
    pub const INVERSE: &str = include_str!("inverse.rs");
    /// Exact source bytes owned by this crate.
    pub const LEARN: &str = include_str!("learn.rs");
    /// Exact source bytes owned by this crate.
    pub const LIB: &str = include_str!("lib.rs");
    /// Exact source bytes owned by this crate.
    pub const LOWER: &str = include_str!("lower.rs");
    /// Exact source bytes owned by this crate.
    pub const REPAIR: &str = include_str!("repair.rs");
    /// Exact source bytes owned by this crate.
    pub const SKETCH: &str = include_str!("sketch.rs");
    /// Exact source bytes owned by this crate.
    pub const STRATEGY_ALGEBRA: &str = include_str!("strategy_algebra.rs");
    /// Exact source bytes owned by this crate.
    pub const STRATEGY_KNOWLEDGE: &str = include_str!("strategy_knowledge.rs");
    /// Exact source bytes owned by this crate.
    pub const STRATEGY_KNOWLEDGE_REASONING: &str = include_str!("strategy_knowledge/reasoning.rs");
    /// Exact source bytes owned by this crate.
    pub const STRATEGY_QUESTIONS: &str = include_str!("strategy_questions.rs");
    /// Exact source bytes owned by this crate.
    pub const STRATEGY_SPLIT: &str = include_str!("strategy_split.rs");
    /// Exact source bytes owned by this crate.
    pub const SUBJECT: &str = include_str!("subject.rs");
}
