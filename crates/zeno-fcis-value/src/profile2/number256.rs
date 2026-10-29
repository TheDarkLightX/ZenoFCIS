//! Actual closed numeric atoms and order methods shared with the proof harness.
use super::ordering256::compare;
use core::cmp::Ordering;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
#[path = "ordering_types_model.rs"]
pub(crate) mod ordering_model;

/// Least-significant word first; this is an arithmetic representation, not wire.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct U256(pub(super) [u128; 2]);
/// An owned signed 256-bit integer represented by its two's-complement bits.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct I256(pub(super) U256);

#[cfg_attr(verus_keep_ghost, cfg_eval)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
impl U256 {
    /// Constructs an unsigned value from two words, least significant word first.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result.ordering_words() == words,
    ))]
    pub const fn from_words_le(words: [u128; 2]) -> Self {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(U256::ordering_words); }
        Self(words)
    }
    /// Returns the two unsigned words in least significant word order.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result == self.ordering_words(),
    ))]
    pub const fn words_le(self) -> [u128; 2] {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(U256::ordering_words); }
        self.0
    }
    /// Compares the complete unsigned 256-bit values.
    #[allow(
        clippy::should_implement_trait,
        reason = "Retain the proof-bound by-value comparison API"
    )]
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result == ordering_model::integer_order(
            ordering_model::unsigned(self), ordering_model::unsigned(other)),
    ))]
    pub fn cmp(self, other: Self) -> Ordering {
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(U256::ordering_words);
            assert forall|r: Ordering| ordering_model::lex_result(self.0@, other.0@, r)
                implies r == ordering_model::integer_order(
                    ordering_model::unsigned(self), ordering_model::unsigned(other)) by {
                ordering_model::lex_two(self.0, other.0, r);
            }
        }
        compare(&self.0, &other.0)
    }
}

#[cfg_attr(verus_keep_ghost, cfg_eval)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
impl I256 {
    /// Constructs a signed value from its complete two's-complement bit pattern.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result.ordering_bits() == bits,
    ))]
    pub const fn from_bits(bits: U256) -> Self {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(I256::ordering_bits); }
        Self(bits)
    }
    /// Returns the complete two's-complement bit pattern.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result == self.ordering_bits(),
    ))]
    pub const fn bits(self) -> U256 {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(I256::ordering_bits); }
        self.0
    }
    /// Reports whether the signed value is negative.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures
            result == (ordering_model::unsigned(self.ordering_bits()) >= ordering_model::half()),
            result == (ordering_model::signed(self) < 0),
    ))]
    pub fn is_negative(self) -> bool {
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(U256::ordering_words);
            reveal(I256::ordering_bits);
            let words = self.0.0;
            ordering_model::sign_bit(words@[1]);
            ordering_model::high_half(words);
            ordering_model::signed_bounds(self.0);
        }
        self.0.0[1] >> 127 != 0
    }
    /// Compares the complete signed 256-bit values.
    #[allow(
        clippy::should_implement_trait,
        reason = "Retain the proof-bound by-value comparison API"
    )]
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result == ordering_model::integer_order(
            ordering_model::signed(self), ordering_model::signed(other)),
    ))]
    pub fn cmp(self, other: Self) -> Ordering {
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(I256::ordering_bits);
            ordering_model::signed_order(self.0, other.0);
        }
        match (self.is_negative(), other.is_negative()) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => self.0.cmp(other.0),
        }
    }
}

#[cfg(verus_keep_ghost)]
verus! {
impl U256 {
    pub closed spec fn ordering_words(&self) -> [u128; 2] { self.0 }
}
impl I256 {
    pub closed spec fn ordering_bits(&self) -> U256 { self.0 }
}

/// Reveals the representation only in proof; no mutable reference escapes.
pub(super) proof fn numeric_representation()
    ensures
        forall|value: U256| #[trigger] value.ordering_words() == value.0,
        forall|value: I256| #[trigger] value.ordering_bits() == value.0,
        forall|value: U256| #[trigger] ordering_model::unsigned(value)
            == super::wide::number2(value.0),
        forall|value: I256| #[trigger] ordering_model::signed(value)
            == if super::wide::number2(value.0.0) >= ordering_model::half() {
                super::wide::number2(value.0.0) - ordering_model::modulus()
            } else { super::wide::number2(value.0.0) },
{
    reveal(U256::ordering_words);
    reveal(I256::ordering_bits);
}
}
