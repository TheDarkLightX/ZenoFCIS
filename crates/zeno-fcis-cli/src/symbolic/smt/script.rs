//! The production query representation and its checked text serialization.

use super::term::{Sort, Term, numeral};

#[path = "prepare.rs"]
mod prepare;

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
#[path = "script_spec.rs"]
pub(crate) mod spec;

/// The values one declared constant may take.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[cfg_attr(not(verus_keep_ghost), derive(Clone, Debug, Eq, PartialEq))]
pub(crate) enum Values {
    /// Every integer from `min` to `max` inclusive.
    Range { min: i128, max: i128 },
    /// Exactly these integers, preserving their declared order.
    Set(Vec<i128>),
}

#[cfg_attr(verus_keep_ghost, verus_spec(
    ensures final(out)@ == old(out)@ + spec::member(name@, value),
))]
fn write_member(out: &mut String, name: &str, value: i128) {
    #[cfg(verus_keep_ghost)]
    proof! { broadcast use vstd::seq::group_seq_lemmas;
    numeral::spec::punctuation();
    reveal_strlit("\n");
    assert("\n"@ =~= seq!['\n']); }
    out.push_str("(= ");
    out.push_str(name);
    out.push(' ');
    numeral::append_signed(out, value);
    out.push(')');
    #[cfg(verus_keep_ghost)]
    proof! {
        broadcast use vstd::seq::group_seq_lemmas;
        numeral::spec::punctuation();
        assert(out@ == old(out)@ + spec::member(name@, value));
    }
}

impl Values {
    #[cfg_attr(verus_keep_ghost, verus_spec(
        ensures final(out)@ == old(out)@ + spec::values(*self, name@),
    ))]
    fn write(&self, out: &mut String, name: &str) {
        #[cfg(verus_keep_ghost)]
        proof! { broadcast use vstd::seq::group_seq_lemmas;
        numeral::spec::punctuation();
        reveal_strlit("\n");
        assert("\n"@ =~= seq!['\n']); }
        match self {
            Self::Range { min, max } => {
                out.push_str("(and (<= ");
                numeral::append_signed(out, *min);
                out.push(' ');
                out.push_str(name);
                out.push_str(") (<= ");
                out.push_str(name);
                out.push(' ');
                numeral::append_signed(out, *max);
                out.push_str("))");
            }
            Self::Set(values) => {
                if values.is_empty() {
                    out.push_str("false");
                } else if values.len() == 1 {
                    write_member(out, name, values[0]);
                } else {
                    out.push_str("(or");
                    #[cfg(verus_keep_ghost)]
                    let base = verus_exec_expr!(Ghost(out@));
                    let mut index = 0;
                    #[cfg_attr(verus_keep_ghost, verus_spec(
                        invariant index <= values@.len(),
                            out@ == base@ + spec::members(values@, name@, index as nat),
                        decreases values@.len() - index,
                    ))]
                    while index < values.len() {
                        out.push(' ');
                        write_member(out, name, values[index]);
                        #[cfg(verus_keep_ghost)]
                        proof! {
                                            spec::members_step(values@, name@, index as nat);
                                            broadcast use vstd::seq::group_seq_lemmas;
                        numeral::spec::punctuation();
                                            assert(out@ == base@ + spec::members(values@, name@, (index + 1) as nat));
                                        }
                        index += 1;
                    }
                    out.push(')');
                }
            }
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            broadcast use vstd::seq::group_seq_lemmas;
        numeral::spec::punctuation();
            assert(out@ == old(out)@ + spec::values(*self, name@));
        }
    }
}

/// One query: declared constants, ordered definitions, observations and assertions.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[cfg_attr(not(verus_keep_ghost), derive(Clone, Debug))]
pub(crate) struct Script {
    pub(crate) comments: Vec<String>,
    pub(crate) constants: Vec<(String, Values)>,
    pub(crate) definitions: Vec<(String, Sort, Term)>,
    pub(crate) observations: Vec<(String, Term)>,
    pub(crate) assertions: Vec<Term>,
}

impl Script {
    /// Emits the complete SMT-LIB query, including its final check-sat command.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures result@ == spec::source(*self),
    ))]
    pub(crate) fn source(&self) -> String {
        #[cfg(verus_keep_ghost)]
        proof! {
            broadcast use vstd::seq::group_seq_lemmas;
        numeral::spec::punctuation();
            spec::empty_prefixes(*self);
        }
        let mut out = String::new();
        let mut index = 0;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant index <= self.comments@.len(),
                out@ == spec::comments(self.comments@, index as nat),
            decreases self.comments@.len() - index,
        ))]
        while index < self.comments.len() {
            out.push_str("; ");
            out.push_str(&self.comments[index]);
            out.push('\n');
            #[cfg(verus_keep_ghost)]
            proof! {
                numeral::spec::punctuation();
                spec::comments_step(self.comments@, index as nat);
                broadcast use vstd::seq::group_seq_lemmas;
            }
            index += 1;
        }
        out.push_str("(set-logic ALL)\n(set-option :produce-models true)\n");
        #[cfg(verus_keep_ghost)]
        let constants_base = verus_exec_expr!(Ghost(out@));
        index = 0;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant index <= self.constants@.len(),
                out@ == constants_base@ + spec::constants(self.constants@, index as nat),
            decreases self.constants@.len() - index,
        ))]
        while index < self.constants.len() {
            let (name, values) = &self.constants[index];
            out.push_str("(declare-const ");
            out.push_str(name);
            out.push_str(" Int)\n(assert ");
            values.write(&mut out, name);
            out.push_str(")\n");
            #[cfg(verus_keep_ghost)]
            proof! {
                    spec::constants_step(self.constants@, index as nat);
                    broadcast use vstd::seq::group_seq_lemmas;
            numeral::spec::punctuation();
                    assert(out@ == constants_base@ + spec::constants(self.constants@, (index + 1) as nat));
                }
            index += 1;
        }
        #[cfg(verus_keep_ghost)]
        let declarations_base = verus_exec_expr!(Ghost(out@));
        index = 0;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant index <= self.observations@.len(),
                out@ == declarations_base@ + spec::declarations(self.observations@, index as nat),
            decreases self.observations@.len() - index,
        ))]
        while index < self.observations.len() {
            out.push_str("(declare-const ");
            out.push_str(&self.observations[index].0);
            out.push_str(" Int)\n");
            #[cfg(verus_keep_ghost)]
            proof! {
                    spec::declarations_step(self.observations@, index as nat);
                    broadcast use vstd::seq::group_seq_lemmas;
            numeral::spec::punctuation();
                    assert(out@ == declarations_base@ + spec::declarations(self.observations@, (index + 1) as nat));
                }
            index += 1;
        }
        #[cfg(verus_keep_ghost)]
        let definitions_base = verus_exec_expr!(Ghost(out@));
        index = 0;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant index <= self.definitions@.len(),
                out@ == definitions_base@ + spec::definitions(self.definitions@, index as nat),
            decreases self.definitions@.len() - index,
        ))]
        while index < self.definitions.len() {
            let (name, sort, term) = &self.definitions[index];
            out.push_str("(define-fun ");
            out.push_str(name);
            out.push_str(" () ");
            out.push_str(match sort {
                Sort::Int => "Int",
                Sort::Bool => "Bool",
            });
            out.push(' ');
            term.write(&mut out);
            out.push_str(")\n");
            #[cfg(verus_keep_ghost)]
            proof! {
                    spec::definitions_step(self.definitions@, index as nat);
                    broadcast use vstd::seq::group_seq_lemmas;
            numeral::spec::punctuation();
                    assert(out@ == definitions_base@ + spec::definitions(self.definitions@, (index + 1) as nat));
                }
            index += 1;
        }
        #[cfg(verus_keep_ghost)]
        let observations_base = verus_exec_expr!(Ghost(out@));
        index = 0;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant index <= self.observations@.len(),
                out@ == observations_base@ + spec::observations(self.observations@, index as nat),
            decreases self.observations@.len() - index,
        ))]
        while index < self.observations.len() {
            let (name, term) = &self.observations[index];
            out.push_str("(assert (= ");
            out.push_str(name);
            out.push(' ');
            term.write(&mut out);
            out.push_str("))\n");
            #[cfg(verus_keep_ghost)]
            proof! {
                    spec::observations_step(self.observations@, index as nat);
                    broadcast use vstd::seq::group_seq_lemmas;
            numeral::spec::punctuation();
                    assert(out@ == observations_base@ + spec::observations(self.observations@, (index + 1) as nat));
                }
            index += 1;
        }
        #[cfg(verus_keep_ghost)]
        let assertions_base = verus_exec_expr!(Ghost(out@));
        index = 0;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant index <= self.assertions@.len(),
                out@ == assertions_base@ + spec::assertions(self.assertions@, index as nat),
            decreases self.assertions@.len() - index,
        ))]
        while index < self.assertions.len() {
            out.push_str("(assert ");
            self.assertions[index].write(&mut out);
            out.push_str(")\n");
            #[cfg(verus_keep_ghost)]
            proof! {
                    spec::assertions_step(self.assertions@, index as nat);
                    broadcast use vstd::seq::group_seq_lemmas;
            numeral::spec::punctuation();
                    assert(out@ == assertions_base@ + spec::assertions(self.assertions@, (index + 1) as nat));
                }
            index += 1;
        }
        out.push_str("(check-sat)\n");
        #[cfg(verus_keep_ghost)]
        proof! {
            broadcast use vstd::seq::group_seq_lemmas;
        numeral::spec::punctuation();
            assert(out@ == spec::source(*self));
        }
        out
    }
}
