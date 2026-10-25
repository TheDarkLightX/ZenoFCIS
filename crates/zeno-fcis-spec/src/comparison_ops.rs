//! Comparison operations shared by the public AST, parser, and proof harness.
//!
//! The enum below is the actual AST type. There is no proof-only mirror whose
//! interpretation could diverge from the typed result returned by parsing.

use crate::lexical_core::LexemeKind;

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// Relational comparison operation.
#[allow(missing_docs)]
#[cfg_attr(verus_keep_ghost, verifier::verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompareOp {
    Eq,
    NotEq,
    Less,
    LessEq,
    Greater,
    GreaterEq,
}

#[cfg(verus_keep_ghost)]
verus! {
    /// The six accepted lexical tokens and their actual AST interpretation.
    pub(crate) open spec fn token_comparison(kind: LexemeKind) -> Option<CompareOp> {
        match kind {
            LexemeKind::EqEq => Some(CompareOp::Eq),
            LexemeKind::NotEq => Some(CompareOp::NotEq),
            LexemeKind::Less => Some(CompareOp::Less),
            LexemeKind::LessEq => Some(CompareOp::LessEq),
            LexemeKind::Greater => Some(CompareOp::Greater),
            LexemeKind::GreaterEq => Some(CompareOp::GreaterEq),
            _ => None,
        }
    }

    /// Mathematical semantics, with the first operand kept on the left.
    pub open spec fn comparison_holds(op: CompareOp, left: int, right: int) -> bool {
        match op {
            CompareOp::Eq => left == right,
            CompareOp::NotEq => left != right,
            CompareOp::Less => left < right,
            CompareOp::LessEq => left <= right,
            CompareOp::Greater => left > right,
            CompareOp::GreaterEq => left >= right,
        }
    }
}

/// Converts exactly the six comparison tokens into the real public AST enum.
/// Assignment, arithmetic, trivia, errors, and EOF all refuse.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == token_comparison(kind),
))]
pub(crate) fn read_compare_op(kind: LexemeKind) -> Option<CompareOp> {
    match kind {
        LexemeKind::EqEq => Some(CompareOp::Eq),
        LexemeKind::NotEq => Some(CompareOp::NotEq),
        LexemeKind::Less => Some(CompareOp::Less),
        LexemeKind::LessEq => Some(CompareOp::LessEq),
        LexemeKind::Greater => Some(CompareOp::Greater),
        LexemeKind::GreaterEq => Some(CompareOp::GreaterEq),
        _ => None,
    }
}

impl CompareOp {
    /// Applies this comparison to ordered signed operands without arithmetic.
    /// The complete `i128` range, including both endpoints, is supported.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result == comparison_holds(self, left as int, right as int),
    ))]
    pub fn evaluate_i128(self, left: i128, right: i128) -> bool {
        match self {
            Self::Eq => left == right,
            Self::NotEq => left != right,
            Self::Less => left < right,
            Self::LessEq => left <= right,
            Self::Greater => left > right,
            Self::GreaterEq => left >= right,
        }
    }

    /// Applies this comparison to ordered unsigned operands without narrowing.
    /// Values above `i128::MAX` retain their unsigned mathematical meaning.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result == comparison_holds(self, left as int, right as int),
    ))]
    pub fn evaluate_u128(self, left: u128, right: u128) -> bool {
        match self {
            Self::Eq => left == right,
            Self::NotEq => left != right,
            Self::Less => left < right,
            Self::LessEq => left <= right,
            Self::Greater => left > right,
            Self::GreaterEq => left >= right,
        }
    }
}
