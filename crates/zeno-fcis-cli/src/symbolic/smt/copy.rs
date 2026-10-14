//! The production deep clone of an SMT term; no assumed recursive derive.

use super::Term;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

// Keep the closed structural model inside its proof boundary.
#[cfg(verus_keep_ghost)]
verus! {
broadcast use {
    vstd::seq::lemma_seq_index_decreases,
    vstd::std_specs::vec::axiom_vec_decreases_to_view,
};

#[verifier::opaque]
pub closed spec fn same(a: Term, b: Term) -> bool
    decreases a,
{
    match (a, b) {
        (Term::Int(x), Term::Int(y)) => x == y,
        (Term::Bool(x), Term::Bool(y)) => x == y,
        (Term::Name(x), Term::Name(y)) => x@ == y@,
        (Term::Add(x, y), Term::Add(u, v))
        | (Term::Sub(x, y), Term::Sub(u, v))
        | (Term::Mul(x, y), Term::Mul(u, v))
        | (Term::Div(x, y), Term::Div(u, v))
        | (Term::Eq(x, y), Term::Eq(u, v))
        | (Term::Lt(x, y), Term::Lt(u, v))
        | (Term::Le(x, y), Term::Le(u, v)) => same(*x, *u) && same(*y, *v),
        (Term::Not(x), Term::Not(y)) => same(*x, *y),
        // Keep quantified branches separate: a combined Rust or-pattern inserts
        // conditionals into SMT quantifier triggers in the pinned verifier.
        (Term::And(x), Term::And(y)) =>
            x@.len() == y@.len() && forall|i: int| 0 <= i < x@.len() ==> same(#[trigger] x@[i], y@[i]),
        (Term::Or(x), Term::Or(y)) =>
            x@.len() == y@.len() && forall|i: int| 0 <= i < x@.len() ==> same(#[trigger] x@[i], y@[i]),
        (Term::Ite(c, x, y), Term::Ite(d, u, v)) => same(*c, *d) && same(*x, *u) && same(*y, *v),
        _ => false,
    }
}

#[verifier::opaque]
pub closed spec fn same_list(a: Seq<Term>, b: Seq<Term>) -> bool
    decreases a,
{
    a.len() == b.len() && forall|i: int| 0 <= i < a.len() ==> same(a[i], b[i])
}

pub(crate) proof fn list_intro(a: Seq<Term>, b: Seq<Term>)
    requires a.len() == b.len(),
        forall|i: int| 0 <= i < a.len() ==> #[trigger] same(a[i], b[i]),
    ensures same_list(a, b),
{
    reveal_with_fuel(same_list, 1);
}


pub(crate) proof fn junction_structure(a: Vec<Term>, b: Vec<Term>, conjunction: bool)
    requires same(if conjunction { Term::And(a) } else { Term::Or(a) },
        if conjunction { Term::And(b) } else { Term::Or(b) }),
    ensures same_list(a@, b@),
{
    reveal_with_fuel(same, 2);
    assert(a@.len() == b@.len());
    assert forall|i: int| 0 <= i < a@.len() implies #[trigger] same(a@[i], b@[i]) by {
        reveal_with_fuel(same, 2);
        assert(same(a@[i], b@[i]));
    }
    list_intro(a@, b@);
}

/// Structural equality established by the real Term::clone preserves meaning.
pub(crate) proof fn copy_preserves_meaning(a: Term, b: Term, environment: Map<Seq<char>, super::denotation::Value>)
    requires same(a, b),
    ensures super::denotation::term(a, environment) == super::denotation::term(b, environment),
    decreases a,
{
    reveal_with_fuel(same, 2);
    reveal_with_fuel(same_list, 1);
    reveal_with_fuel(super::denotation::term, 2);
    reveal_with_fuel(super::denotation::junction, 2);
    match (a, b) {
        (Term::Add(x, y), Term::Add(u, v))
        | (Term::Sub(x, y), Term::Sub(u, v))
        | (Term::Eq(x, y), Term::Eq(u, v))
        | (Term::Lt(x, y), Term::Lt(u, v))
        | (Term::Le(x, y), Term::Le(u, v)) => {
            copy_preserves_meaning(*x, *u, environment);
            copy_preserves_meaning(*y, *v, environment);
        },
        (Term::Not(x), Term::Not(y)) => copy_preserves_meaning(*x, *y, environment),
        (Term::And(x), Term::And(y)) => {
            junction_structure(x, y, true);
            junction_copy(x@, y@, x@.len(), environment, true);
        },
        (Term::Or(x), Term::Or(y)) => {
            junction_structure(x, y, false);
            junction_copy(x@, y@, x@.len(), environment, false);
        },
        (Term::Ite(c, x, y), Term::Ite(d, u, v)) => {
            copy_preserves_meaning(*c, *d, environment);
            copy_preserves_meaning(*x, *u, environment);
            copy_preserves_meaning(*y, *v, environment);
        },
        _ => {},
    }
}

pub(crate) proof fn junction_copy(a: Seq<Term>, b: Seq<Term>, count: nat,
    environment: Map<Seq<char>, super::denotation::Value>, conjunction: bool)
    requires same_list(a, b), count <= a.len(),
    ensures super::denotation::junction(a, count, environment, conjunction) == super::denotation::junction(b, count, environment, conjunction),
    decreases a, count,
{
    reveal_with_fuel(super::denotation::junction, 2);
    reveal_with_fuel(super::denotation::term, 2);
    reveal_with_fuel(same_list, 1);
    reveal_with_fuel(same, 2);
    if count > 0 {
        vstd::seq::lemma_seq_index_decreases(a, count as int - 1);
        copy_preserves_meaning(a[count as int - 1], b[count as int - 1], environment);
        junction_copy(a, b, (count - 1) as nat, environment, conjunction);
    }
}

}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures same_list(items@, result@), result@.len() == items@.len(),
        forall|i: int| 0 <= i < items@.len() ==> same(items@[i], result@[i]),
    decreases items@,
))]
fn copy_list(items: &[Term]) -> Vec<Term> {
    let mut copied = Vec::with_capacity(items.len());
    let mut index = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= items@.len(), copied@.len() == index,
            forall|i: int| 0 <= i < index ==> same(items@[i], copied@[i]),
        decreases items@.len() - index,
    ))]
    while index < items.len() {
        #[cfg(verus_keep_ghost)]
        proof! { vstd::seq::lemma_seq_index_decreases(items@, index as int); }
        copied.push(items[index].clone());
        index += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! { list_intro(items@, copied@); }
    copied
}

impl Clone for Term {
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures same(*self, result),
            forall|environment: Map<Seq<char>, super::denotation::Value>|
                super::denotation::term(result, environment) == super::denotation::term(*self, environment),
        decreases self,
    ))]
    fn clone(&self) -> Term {
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal_with_fuel(same, 2);
        }
        let result = match self {
            Self::Int(value) => Self::Int(*value),
            Self::Bool(value) => Self::Bool(*value),
            Self::Name(name) => {
                let mut copied = String::new();
                copied.push_str(name);
                Self::Name(copied)
            }
            Self::Add(a, b) => Self::Add(Box::new((**a).clone()), Box::new((**b).clone())),
            Self::Sub(a, b) => Self::Sub(Box::new((**a).clone()), Box::new((**b).clone())),
            Self::Mul(a, b) => Self::Mul(Box::new((**a).clone()), Box::new((**b).clone())),
            Self::Div(a, b) => Self::Div(Box::new((**a).clone()), Box::new((**b).clone())),
            Self::Eq(a, b) => Self::Eq(Box::new((**a).clone()), Box::new((**b).clone())),
            Self::Lt(a, b) => Self::Lt(Box::new((**a).clone()), Box::new((**b).clone())),
            Self::Le(a, b) => Self::Le(Box::new((**a).clone()), Box::new((**b).clone())),
            Self::Not(a) => Self::Not(Box::new((**a).clone())),
            Self::And(items) => Self::And(copy_list(items)),
            Self::Or(items) => Self::Or(copy_list(items)),
            Self::Ite(c, a, b) => Self::Ite(
                Box::new((**c).clone()),
                Box::new((**a).clone()),
                Box::new((**b).clone()),
            ),
        };
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(same(*self, result));
            assert forall|environment: Map<Seq<char>, super::denotation::Value>|
                super::denotation::term(result, environment) == super::denotation::term(*self, environment) by {
                copy_preserves_meaning(*self, result, environment);
            }
        }
        result
    }
}
