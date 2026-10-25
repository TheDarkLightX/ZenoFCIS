//! Actual nongeneric scalar AST and shared arithmetic constructors.
//! Opaque identifier, projection, and relation operands retain their real Rust types.

use crate::ast::{Identifier, ProjectionPath, RelExpr};
use alloc::boxed::Box;

use crate::lexical_core::{LexemeKind, check_spelling};

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// Checked integer division policy.
#[allow(missing_docs)]
#[cfg_attr(verus_keep_ghost, verifier::verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DivisionMode {
    Exact,
    Floor,
    Ceil,
}

/// Scalar expression used by relational predicates.
#[allow(missing_docs)]
#[cfg_attr(verus_keep_ghost, verifier::verify)]
#[cfg_attr(not(verus_keep_ghost), derive(Clone, Debug, Eq, PartialEq))]
pub enum ValueExpr {
    Int(i128),
    U128(u128),
    ToI128(Box<ValueExpr>),
    ToU128(Box<ValueExpr>),
    Var(Identifier),
    Projection(ProjectionPath),
    /// Eager condition and both arms, lowered to the library Select operation.
    Select(Box<RelExpr>, Box<ValueExpr>, Box<ValueExpr>),
    Add(Box<ValueExpr>, Box<ValueExpr>),
    Sub(Box<ValueExpr>, Box<ValueExpr>),
    Mul(Box<ValueExpr>, Box<ValueExpr>),
    Div(DivisionMode, Box<ValueExpr>, Box<ValueExpr>),
    /// Nonnegative full-product division, distinct from a possibly trapping
    /// intermediate multiplication. Authored calls use Floor or Ceil.
    MulDiv(DivisionMode, Box<ValueExpr>, Box<ValueExpr>, Box<ValueExpr>),
    SignedMulDivFloor(Box<ValueExpr>, Box<ValueExpr>, Box<ValueExpr>),
    /// Explicit target scale, rounding, and signed-factor domain.
    ScaledMulDiv(
        DivisionMode,
        bool,
        Box<ValueExpr>,
        Box<ValueExpr>,
        Box<ValueExpr>,
        u8,
    ),
    /// Eager operands followed by a literal number of ordered floor steps.
    RepeatMulDiv(bool, Box<ValueExpr>, Box<ValueExpr>, Box<ValueExpr>, u8),
    Sum {
        variable: Identifier,
        start: i128,
        end: i128,
        body: Box<ValueExpr>,
    },
}

#[cfg(verus_keep_ghost)]
verus! {
    pub(crate) open spec fn arithmetic_kind_valid(kind: LexemeKind, additive: bool) -> bool {
        if additive {
            matches!(kind, LexemeKind::Plus | LexemeKind::Minus)
        } else {
            matches!(kind, LexemeKind::Star)
        }
    }

    pub(crate) open spec fn arithmetic_node(
        kind: LexemeKind,
        additive: bool,
        left: ValueExpr,
        right: ValueExpr,
    ) -> Option<ValueExpr> {
        match kind {
            LexemeKind::Plus if additive =>
                Some(ValueExpr::Add(Box::new(left), Box::new(right))),
            LexemeKind::Minus if additive =>
                Some(ValueExpr::Sub(Box::new(left), Box::new(right))),
            LexemeKind::Star if !additive =>
                Some(ValueExpr::Mul(Box::new(left), Box::new(right))),
            _ => None,
        }
    }

    pub(crate) open spec fn division_word(word: Seq<u8>) -> Option<DivisionMode> {
        if word == seq![b'd', b'i', b'v', b'_', b'e', b'x', b'a', b'c', b't'] {
            Some(DivisionMode::Exact)
        } else if word == seq![b'd', b'i', b'v', b'_', b'f', b'l', b'o', b'o', b'r'] {
            Some(DivisionMode::Floor)
        } else if word == seq![b'd', b'i', b'v', b'_', b'c', b'e', b'i', b'l'] {
            Some(DivisionMode::Ceil)
        } else {
            None
        }
    }

    pub(crate) open spec fn maximum(left: nat, right: nat) -> nat {
        if left >= right { left } else { right }
    }

    /// Complete scalar-node height parameterized by the real relational
    /// operand's height. The parser-to-metric correspondence is separate.
    pub(crate) open spec fn value_height(
        value: ValueExpr,
        relation_height: spec_fn(RelExpr) -> nat,
    ) -> nat
        decreases value,
    {
        match value {
            ValueExpr::Int(_) | ValueExpr::U128(_)
            | ValueExpr::Var(_) | ValueExpr::Projection(_) => 1,
            ValueExpr::ToI128(body) | ValueExpr::ToU128(body)
            | ValueExpr::Sum { body, .. } => 1 + value_height(*body, relation_height),
            ValueExpr::Add(left, right) | ValueExpr::Sub(left, right)
            | ValueExpr::Mul(left, right) | ValueExpr::Div(_, left, right) =>
                1 + maximum(value_height(*left, relation_height), value_height(*right, relation_height)),
            ValueExpr::Select(condition, yes, no) =>
                1 + maximum(relation_height(*condition),
                    maximum(value_height(*yes, relation_height), value_height(*no, relation_height))),
            ValueExpr::MulDiv(_, a, b, c)
            | ValueExpr::SignedMulDivFloor(a, b, c)
            | ValueExpr::ScaledMulDiv(_, _, a, b, c, _)
            | ValueExpr::RepeatMulDiv(_, a, b, c, _) =>
                1 + maximum(value_height(*a, relation_height),
                    maximum(value_height(*b, relation_height), value_height(*c, relation_height))),
        }
    }

    /// The numerical transfer used before construction agrees with the actual
    /// Add/Sub/Mul constructor, for every supplied relational-height function.
    pub(crate) proof fn arithmetic_height(
        kind: LexemeKind,
        additive: bool,
        left: ValueExpr,
        right: ValueExpr,
        relation_height: spec_fn(RelExpr) -> nat,
    )
        requires arithmetic_kind_valid(kind, additive),
        ensures
            arithmetic_node(kind, additive, left, right).is_some(),
            value_height(arithmetic_node(kind, additive, left, right).unwrap(), relation_height)
                == 1 + maximum(value_height(left, relation_height), value_height(right, relation_height)),
    {}

    pub(crate) proof fn division_height(
        mode: DivisionMode,
        left: ValueExpr,
        right: ValueExpr,
        relation_height: spec_fn(RelExpr) -> nat,
    )
        ensures value_height(ValueExpr::Div(mode, Box::new(left), Box::new(right)), relation_height)
            == 1 + maximum(value_height(left, relation_height), value_height(right, relation_height)),
    {}
}

/// Performs one arithmetic fold step with the real AST constructors.
/// The left accumulator and right operand retain their exact order. Only
/// `+`/`-` are accepted at the additive level and `*` at the product level.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures
        result == arithmetic_node(kind, additive, left, right),
        result.is_some() == arithmetic_kind_valid(kind, additive),
))]
pub(crate) fn fold_arithmetic(
    kind: LexemeKind,
    additive: bool,
    left: ValueExpr,
    right: ValueExpr,
) -> Option<ValueExpr> {
    match kind {
        LexemeKind::Plus if additive => Some(ValueExpr::Add(Box::new(left), Box::new(right))),
        LexemeKind::Minus if additive => Some(ValueExpr::Sub(Box::new(left), Box::new(right))),
        LexemeKind::Star if !additive => Some(ValueExpr::Mul(Box::new(left), Box::new(right))),
        _ => None,
    }
}

/// Interprets a complete division function name without accepting a prefix.
// Explicit byte arrays expose each literal byte to the pinned verifier.
#[allow(clippy::byte_char_slices)]
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == division_word(word@),
))]
pub(crate) fn read_division_mode(word: &[u8]) -> Option<DivisionMode> {
    #[cfg(verus_keep_ghost)]
    proof! { assert(word@.subrange(0, word@.len() as int) == word@); }
    if check_spelling(
        word,
        0,
        word.len(),
        &[b'd', b'i', b'v', b'_', b'e', b'x', b'a', b'c', b't'],
    ) {
        Some(DivisionMode::Exact)
    } else if check_spelling(
        word,
        0,
        word.len(),
        &[b'd', b'i', b'v', b'_', b'f', b'l', b'o', b'o', b'r'],
    ) {
        Some(DivisionMode::Floor)
    } else if check_spelling(
        word,
        0,
        word.len(),
        &[b'd', b'i', b'v', b'_', b'c', b'e', b'i', b'l'],
    ) {
        Some(DivisionMode::Ceil)
    } else {
        None
    }
}

/// Builds the actual division node with its policy and ordered operands intact.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result == ValueExpr::Div(mode, Box::new(left), Box::new(right)),
))]
pub(crate) fn division(mode: DivisionMode, left: ValueExpr, right: ValueExpr) -> ValueExpr {
    ValueExpr::Div(mode, Box::new(left), Box::new(right))
}

/// Computes a binary node's height, refusing before an overflow or limit breach.
/// The caller binds the input numbers to the heights of the actual operands.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures
        result.is_some() == (1 + maximum(left as nat, right as nat) <= limit),
        match result {
            Some(height) => height == 1 + maximum(left as nat, right as nat)
                && height <= limit,
            None => 1 + maximum(left as nat, right as nat) > limit,
        },
))]
pub(crate) fn binary_height(left: usize, right: usize, limit: usize) -> Option<usize> {
    if left >= limit || right >= limit {
        return None;
    }
    Some(1 + if left >= right { left } else { right })
}
