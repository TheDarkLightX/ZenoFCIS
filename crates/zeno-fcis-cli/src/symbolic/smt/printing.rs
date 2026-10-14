//! Erased syntax specification for the production recursive term printer.
//! Mathematical denotation and query refinement are separate obligations.

use super::{Term, numeral};
use vstd::prelude::*;

verus! {

broadcast use {
    vstd::seq::lemma_seq_index_decreases,
    vstd::std_specs::vec::axiom_vec_decreases_to_view,
};

#[verifier::opaque]
pub(crate) open spec fn text(term: Term) -> Seq<char>
    decreases term,
{
    match term {
        Term::Int(n) => numeral::spec::signed(n as int),
        Term::Bool(b) => if b { "true"@ } else { "false"@ },
        Term::Name(name) => name@,
        Term::Add(a, b) => "("@ + "+"@ + " "@ + text(*a) + " "@ + text(*b) + ")"@,
        Term::Sub(a, b) => "("@ + "-"@ + " "@ + text(*a) + " "@ + text(*b) + ")"@,
        Term::Mul(a, b) => "("@ + "*"@ + " "@ + text(*a) + " "@ + text(*b) + ")"@,
        Term::Div(a, b) => "("@ + "div"@ + " "@ + text(*a) + " "@ + text(*b) + ")"@,
        Term::Eq(a, b) => "("@ + "="@ + " "@ + text(*a) + " "@ + text(*b) + ")"@,
        Term::Lt(a, b) => "("@ + "<"@ + " "@ + text(*a) + " "@ + text(*b) + ")"@,
        Term::Le(a, b) => "("@ + "<="@ + " "@ + text(*a) + " "@ + text(*b) + ")"@,
        Term::Not(a) => "(not "@ + text(*a) + ")"@,
        Term::And(items) => "(and"@ + list(items@, items@.len()) + ")"@,
        Term::Or(items) => "(or"@ + list(items@, items@.len()) + ")"@,
        Term::Ite(c, a, b) => "(ite "@ + text(*c) + " "@ + text(*a) + " "@ + text(*b) + ")"@,
    }
}

#[verifier::opaque]
pub(crate) open spec fn list(items: Seq<Term>, count: nat) -> Seq<char>
    recommends count <= items.len(),
    decreases items, count,
{
    if count == 0 || count > items.len() { Seq::empty() }
    else { list(items, (count - 1) as nat) + " "@ + text(items[count as int - 1]) }
}

pub(crate) proof fn list_step(items: Seq<Term>, count: nat)
    requires count < items.len(),
    ensures list(items, count + 1) == list(items, count) + " "@ + text(items[count as int]),
{
    reveal_with_fuel(list, 2);
    reveal_with_fuel(text, 1);
    assert(count + 1 <= items.len());
    assert((count + 1 - 1) as nat == count);
}

}
