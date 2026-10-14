//! The production term representation and checked text serialization.

#[path = "constructors.rs"]
mod constructors;
#[path = "copy.rs"]
mod copy;
#[cfg(verus_keep_ghost)]
#[path = "denotation.rs"]
pub(super) mod denotation;
#[path = "folding.rs"]
mod folding;

#[path = "names.rs"]
pub(super) mod names;
#[path = "numeral.rs"]
pub(super) mod numeral;

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
#[path = "printing.rs"]
pub(super) mod printing;

/// The sort of a term or definition.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Sort {
    Int,
    Bool,
}

/// A term. Integer terms denote unbounded integers, as SMT-LIB's `Int` does.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[cfg_attr(not(verus_keep_ghost), derive(Debug, Eq, PartialEq))]
// Public inside this private module so the Clone trait proof contract can
// expose its mathematical meaning. The CLI re-export remains crate-private.
#[allow(
    unreachable_pub,
    reason = "The public Clone proof contract names this type; its containing SMT module is private."
)]
pub enum Term {
    Int(i128),
    Bool(bool),
    /// A declared constant or an earlier definition.
    Name(String),
    Add(Box<Term>, Box<Term>),
    Sub(Box<Term>, Box<Term>),
    Mul(Box<Term>, Box<Term>),
    /// SMT-LIB's `div`: the quotient whose remainder lies in `0..|divisor|`.
    /// Its value for a zero divisor is unspecified, so every user guards the
    /// divisor.
    Div(Box<Term>, Box<Term>),
    Eq(Box<Term>, Box<Term>),
    Lt(Box<Term>, Box<Term>),
    Le(Box<Term>, Box<Term>),
    Not(Box<Term>),
    And(Vec<Term>),
    Or(Vec<Term>),
    Ite(Box<Term>, Box<Term>, Box<Term>),
}

impl Term {
    #[cfg_attr(verus_keep_ghost, verus_spec(
        ensures final(out)@ == old(out)@ + printing::text(*self),
        decreases self,
    ))]
    pub(super) fn write(&self, out: &mut String) {
        #[cfg(verus_keep_ghost)]
        proof! {
            broadcast use vstd::seq::group_seq_lemmas;
        numeral::spec::punctuation();
            reveal_with_fuel(printing::text, 1);
            reveal_with_fuel(printing::list, 1);
        }
        match self {
            Self::Int(value) => numeral::append_signed(out, *value),
            Self::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
            Self::Name(name) => out.push_str(name),
            Self::Add(a, b)
            | Self::Sub(a, b)
            | Self::Mul(a, b)
            | Self::Div(a, b)
            | Self::Eq(a, b)
            | Self::Lt(a, b)
            | Self::Le(a, b) => {
                let operator = match self {
                    Self::Add(..) => "+",
                    Self::Sub(..) => "-",
                    Self::Mul(..) => "*",
                    Self::Div(..) => "div",
                    Self::Eq(..) => "=",
                    Self::Lt(..) => "<",
                    _ => "<=",
                };
                #[cfg(verus_keep_ghost)]
                proof! {
                    reveal_strlit("+");
                    reveal_strlit("-");
                    reveal_strlit("*");
                    reveal_strlit("div");
                    reveal_strlit("=");
                    reveal_strlit("<");
                    reveal_strlit("<=");
                    assert(operator@ == match self {
                        Self::Add(..) => "+"@,
                        Self::Sub(..) => "-"@,
                        Self::Mul(..) => "*"@,
                        Self::Div(..) => "div"@,
                        Self::Eq(..) => "="@,
                        Self::Lt(..) => "<"@,
                        _ => "<="@,
                    });
                }
                out.push('(');
                out.push_str(operator);
                out.push(' ');
                a.write(out);
                out.push(' ');
                b.write(out);
                out.push(')');
            }
            Self::Not(a) => {
                out.push_str("(not ");
                a.write(out);
                out.push(')');
            }
            Self::And(items) | Self::Or(items) => {
                out.push_str(if matches!(self, Self::And(..)) {
                    "(and"
                } else {
                    "(or"
                });
                let mut index = 0;
                #[cfg_attr(verus_keep_ghost, verus_spec(
                    invariant index <= items@.len(),
                        decreases_to!(*self => *items),
                        out@ == old(out)@ + (if self is And { "(and"@ } else { "(or"@ })
                            + printing::list(items@, index as nat),
                    decreases items@.len() - index,
                ))]
                while index < items.len() {
                    #[cfg(verus_keep_ghost)]
                    proof! {
                        numeral::spec::punctuation();
                        vstd::std_specs::vec::axiom_vec_index_decreases(*items, index as int);
                    }
                    out.push(' ');
                    items[index].write(out);
                    #[cfg(verus_keep_ghost)]
                    proof! {
                        printing::list_step(items@, index as nat);
                        vstd::seq_lib::lemma_concat_associative(
                            old(out)@ + (if self is And { "(and"@ } else { "(or"@ }),
                            printing::list(items@, index as nat),
                            " "@ + printing::text(items@[index as int]),
                        );
                    }
                    index += 1;
                }
                out.push(')');
            }
            Self::Ite(c, a, b) => {
                out.push_str("(ite ");
                c.write(out);
                out.push(' ');
                a.write(out);
                out.push(' ');
                b.write(out);
                out.push(')');
            }
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            broadcast use vstd::seq::group_seq_lemmas;
        numeral::spec::punctuation();
            assert(out@ == old(out)@ + printing::text(*self));
        }
    }
}
